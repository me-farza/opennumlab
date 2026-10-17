// SPDX-License-Identifier: GPL-3.0-or-later
// A narrow adapter to GNU Octave's public interpreter event API.
// Numerical execution, scopes, breakpoints and stepping remain owned by Octave.
#include <octave/oct.h>
#include <octave/defun-dld.h>
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
#include "mathlab_adapter_binding_v2.h"
#endif
#include <octave/interpreter.h>
#include <octave/event-manager.h>
#include <octave/pt-eval.h>
#include <octave/bp-table.h>
#include <octave/load-path.h>
#include <octave/parse.h>
#include <octave/ov-usr-fcn.h>
#include <octave/ov-fcn-handle.h>
#include <octave/pt-stmt.h>
#include <octave/unwind-prot.h>
#include <octave/symtab.h>
#include <octave/stack-frame.h>
#include <octave/pager.h>
#include <octave/quit.h>
#include <octave/oct-string.h>
#include "mathlab_source_context.h"
#include <algorithm>
#include <atomic>
#include <chrono>
#include <cstdio>
#include <filesystem>
#include <fstream>
#include <memory>
#include <map>
#include <set>
#include <string>
#include <thread>
#include <tuple>
#include <iostream>
#include <streambuf>
#include <vector>

namespace fs = std::filesystem;

namespace {
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
mathlab_adapter_binding_v2::binding& intrinsic_binding() {
  // Process-lifetime transport configuration only. Retention/teardown is unproved.
  static auto *value = new mathlab_adapter_binding_v2::binding;
  return *value;
}

#endif
struct evaluation_interrupt_context {
  octave::tree_evaluator& evaluator;
  bool& quit_requested;
  void (*previous_hook)();
};

evaluation_interrupt_context *active_evaluation_interrupt = nullptr;

void evaluation_signal_hook() {
  auto *context = active_evaluation_interrupt;
  // This hook runs at Octave's interpreter safe point, before it throws the
  // genuine interrupt. Debug mode otherwise catches that interrupt inside a
  // watch and continues with its next statement, including entered input code.
  if (context && octave_interrupt_state.load() > 0) {
    context->quit_requested = true;
    context->evaluator.reset_debug_state(false);
    context->evaluator.dbquit(true);
  }
  if (context && context->previous_hook)
    context->previous_hook();
}

class evaluation_interrupt_guard {
public:
  evaluation_interrupt_guard(octave::tree_evaluator& evaluator,
                             bool& quit_requested, bool outermost)
    : context {evaluator, quit_requested, octave_signal_hook}, installed(outermost),
      saved_mode(evaluator.debug_mode()), previous(active_evaluation_interrupt) {
    if (installed) {
      active_evaluation_interrupt = &context;
      octave_signal_hook = evaluation_signal_hook;
    }
  }
  ~evaluation_interrupt_guard() {
    if (installed) {
      octave_signal_hook = context.previous_hook;
      active_evaluation_interrupt = previous;
      context.evaluator.reset_debug_state(saved_mode);
    }
  }
private:
  evaluation_interrupt_context context;
  bool installed;
  bool saved_mode;
  evaluation_interrupt_context *previous;
};

struct evaluation_capture {
  static constexpr std::size_t limit = 1024 * 1024;
  bool live;
  std::string output;
  std::size_t dropped = 0;

  void append(const char *bytes, std::size_t size) {
    const auto room = output.size() < limit ? limit - output.size() : 0;
    const auto count = std::min(size, room);
    output.append(bytes, count);
    dropped += size - count;
  }
};

// Observe stable C++ sinks, never the pager's owned buffer. input() resets
// that buffer; evalc's saved pointer then becomes dangling on this runtime.
class evaluation_stream final : public std::streambuf {
public:
  evaluation_stream(std::streambuf *next,
                    std::vector<evaluation_capture *>& captures)
    : next(next), captures(captures) { }
protected:
  std::streamsize xsputn(const char *bytes, std::streamsize size) override {
    if (size <= 0) return 0;
    if (!captures.empty()) captures.back()->append(bytes, static_cast<std::size_t>(size));
    return captures.empty() || captures.back()->live ? next->sputn(bytes, size) : size;
  }
  int_type overflow(int_type value) override {
    if (traits_type::eq_int_type(value, traits_type::eof())) return traits_type::not_eof(value);
    const char byte = traits_type::to_char_type(value);
    return xsputn(&byte, 1) == 1 ? value : traits_type::eof();
  }
  int sync() override {
    return captures.empty() || captures.back()->live ? next->pubsync() : 0;
  }
private:
  std::streambuf *next;
  std::vector<evaluation_capture *>& captures;
};

class debug_events final : public octave::interpreter_events,
                           public std::enable_shared_from_this<debug_events> {
public:
  debug_events(octave::interpreter& interpreter, fs::path directory)
    : interp(interpreter), root(std::move(directory)) { fs::create_directories(root); }

  ~debug_events() override {
    active = false;
    if (monitor.joinable()) monitor.join();
  }

  void start() {
    std::weak_ptr<debug_events> weak = shared_from_this();
    monitor = std::thread([this, weak]() {
      while (active) {
        const auto request = root / "debug-command.json";
        std::error_code ec;
        if (fs::exists(request, ec)) {
          std::ifstream input(request);
          std::string raw((std::istreambuf_iterator<char>(input)), {});
          input.close();
          fs::remove(request, ec);
          if (raw.size() <= 1024 * 1024) {
            interp.get_event_manager().post_event([weak, raw](octave::interpreter&) {
              if (auto self = weak.lock()) self->command(raw);
            });
          }
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(10));
      }
    });
  }

  void enter_debugger_event(const std::string& name, const std::string& file, int line) override {
    // Octave's event name can be the source filename, including for a nested
    // function. The actual paused user-code frame retains its function name.
    auto *code = interp.get_evaluator().current_user_code();
    function_name = code ? code->name() : name;
    // Reflection metadata can name the original source. Transport retains the
    // immutable scope key so Rust can distinguish retained source generations.
    const auto scope_file = code ? code->scope().fcn_file_name() : "";
    publish(true, scope_file.empty() ? file : scope_file, line);
  }

  void execute_in_debugger_event(const std::string& file, int line) override {
    // Octave sends this as the paused scope unwinds back to program execution.
    publish(false, file, line);
  }

  void exit_debugger_event() override { publish(false, "", 0); }

  void interpreter_output(const std::string& message) override {
    if (!evaluation_captures.empty()) {
      auto *capture = evaluation_captures.back();
      capture->append(message.data(), message.size());
      if (!capture->live) return;
    }
    std::fwrite(message.data(), 1, message.size(), stdout);
    std::fflush(stdout);
  }

  bool consume_requested_quit() {
    const bool requested = quit_requested;
    quit_requested = false;
    return requested;
  }

  void configure_input(const octave_value& adapter) {
    if (!adapter.is_function_handle())
      error_with_id("MathLab:InputIdentity", "Expected the verified input adapter handle");
    input_adapter = adapter;
  }

  bool has_prepared_source(const std::string& file) const {
    std::error_code ec;
    const auto canonical = fs::canonical(file, ec).string();
    return !ec && prepared_sources.count(canonical) != 0;
  }

  bool has_prepared_breakpoints() const {
    return std::any_of(prepared_breakpoints.begin(), prepared_breakpoints.end(),
      [](const auto& entry) { return !entry.second.empty(); });
  }

  void configure_registry(const std::string& file) {
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
    const auto canonical = mathlab_adapter_binding_v2::active(intrinsic_binding(), interp)
      ? mathlab_adapter_binding_v2::registry_path(octave_value(file)).string()
      : canonical_file(file);
#else
    const auto canonical = canonical_file(file);
#endif
    if (fs::path(canonical).filename() != std::string(registry_name) + ".m")
      error_with_id("MathLab:DebuggerRegistry", "Expected the engine's script registry source file");
    if (!registry_file.empty() && registry_file != canonical)
      error_with_id("MathLab:DebuggerRegistry", "Restart MathLab to change the script debugger registry");
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
    if (mathlab_adapter_binding_v2::active(intrinsic_binding(), interp))
      mathlab_adapter_binding_v2::registry_function(intrinsic_binding(), interp, octave_value(canonical));
    else
      mathlab_source_context::retain_registry(interp, octave_value(canonical));
#else
    mathlab_source_context::retain_registry(interp, octave_value(canonical));
#endif
    registry_file = canonical;
  }

  void source_prepared(const std::string& file, const Cell *retained = nullptr,
                       const octave_value *original = nullptr,
                       const octave_value *text = nullptr) {
    const auto canonical = canonical_file(file);
    auto parsed = prepared_script(canonical);
    // Parse before installing filename-local maps: source parsing can invalidate
    // prior bindings. The worker supplies the exact retained objects, avoiding
    // another helper lookup or any user-body initialization.
    auto bindings = retained ? mathlab_source_context::bindings_from_handles(fs::path(canonical), *retained)
                             : mathlab_source_context::bindings_type {};
    if (original && text)
      mathlab_source_context::reflect_functions(bindings, *original, *text, parsed.user_code_value());
    if (retained) mathlab_source_context::install_bindings(interp, fs::path(canonical), bindings);
    auto& evaluator = interp.get_evaluator();
    const auto frame = evaluator.current_call_stack_frame_number();
    octave::unwind_action restore([&evaluator, frame]() {
      evaluator.restore_frame(frame);
      evaluator.reset_debug_state();
    });
    // Re-resolve function objects: clear functions/all may have discarded them.
    // The cache owns script ASTs, never local-function persistent state.
    refresh_source(canonical);
    evaluator.goto_base_frame();
    evaluator.reset_debug_state(true);
    parsed.user_code_value()->call(evaluator, 0, octave_value_list());
  }

  void refresh_source(const std::string& file) {
    const auto canonical = canonical_file(file);
    // Called again after the runtime registry has selected its retained handles.
    // Temporary references let us mark their real bodies without pinning local
    // persistent state in the debugger across clear functions/all.
    for (auto& [target, lines] : prepared_breakpoints) {
      if (target.scope_file == canonical && !lines.empty()) {
        auto code = prepared_target(target);
        for (auto entry = lines.begin(); entry != lines.end();) {
          auto *body = breakpoint_body(code, *entry);
          if (register_host_breakpoint(body, *entry, true).empty())
            entry = lines.erase(entry);
          else
            ++entry;
        }
      }
    }
    if (has_prepared_source(canonical))
      interp.get_evaluator().reset_debug_state(true);
  }

  void synchronize_prepared() {
    std::set<std::string> scopes;
    for (const auto& [target, lines] : prepared_breakpoints)
      if (!lines.empty()) scopes.insert(target.scope_file);
    for (const auto& scope : scopes) refresh_source(scope);
  }

  void evaluate_base(const std::string& code) {
    auto& evaluator = interp.get_evaluator();
    const auto frame = evaluator.current_call_stack_frame_number();
    octave::unwind_action restore([&evaluator, frame]() {
      evaluator.restore_frame(frame);
      evaluator.reset_debug_state();
    });
    evaluator.goto_base_frame();
    if (has_prepared_breakpoints()) evaluator.reset_debug_state(true);
    int parse_status = 0;
    interp.eval_string(code, false, parse_status, 0);
    if (parse_status != 0)
      error_with_id("Octave:parse-error", "Could not parse the command source");
  }

private:
  static constexpr const char *registry_name = "mathlab_scope_register";

  std::string alias_for_body(octave_user_code *body) {
    const auto key = std::make_tuple(body->scope().fcn_file_name(),
                                    body->name(), body->beg_pos().line());
    auto found = prepared_aliases.find(key);
    if (found != prepared_aliases.end()) return found->second;
    const auto alias = "mathlab_prepared_" + std::to_string(prepared_aliases.size() + 1);
    prepared_aliases.emplace(key, alias);
    return alias;
  }

  octave_value registry_host() {
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
    if (mathlab_adapter_binding_v2::active(intrinsic_binding(), interp)) {
      if (registry_file.empty())
        error_with_id("MathLab:DebuggerRegistry", "Qualified script debugger registry is not configured");
      return mathlab_adapter_binding_v2::registry_function(intrinsic_binding(), interp, octave_value(registry_file));
    }
#endif
    auto host = interp.get_symbol_table().find_cmdline_function(registry_name);
    if (host.is_undefined() && !registry_file.empty()) {
      mathlab_source_context::retain_registry(interp, octave_value(registry_file));
      host = interp.get_symbol_table().find_cmdline_function(registry_name);
    }
    if (registry_file.empty() || !host.is_user_function()
        || canonical_file(host.user_code_value()->scope().fcn_file_name()) != registry_file)
      error_with_id("MathLab:DebuggerRegistry", "Script debugger registry is unavailable or shadowed; restart MathLab with an unshadowed registry helper");
    return host;
  }

  octave::bp_table::bp_lines register_host_breakpoint(octave_user_code *body,
                                                     int line, bool automatic = false) {
    auto host = registry_host();
    auto scope = host.user_code_value()->scope();
    auto& table = interp.get_evaluator().get_bp_table();
    const auto alias = alias_for_body(body);
    const auto existing = scope.find_subfunction(alias);
    if (automatic && existing.is_user_code()) {
      bool present = false;
      if (!table.get_breakpoint_list(ovl(registry_name)).empty()) {
        const auto active = existing.user_code_value()->body()->list_breakpoints();
        for (octave_idx_type index = 0; index < active.length(); ++index)
          if (active(index).int_value() == line) present = true;
      }
      if (!present) {
        // Native dbclear may remove table membership while detached AST marks
        // remain. Clear both objects and honor that user removal on later runs.
        existing.user_code_value()->body()->delete_breakpoint(line);
        body->body()->delete_breakpoint(line);
        return {};
      }
    }
    // The genuine inactive registry already owns these retained local objects.
    // Hosting debugger aliases there lets the public breakpoint table keep
    // statement debugging enabled after Continue. Ordinary clear functions/all
    // releases the host; the native cache never owns local persistent state.
    scope.install_subfunction(alias, octave_value(body, true));
    auto names = scope.subfunction_names();
    if (std::find(names.begin(), names.end(), alias) == names.end()) {
      names.push_back(alias);
      scope.stash_subfunction_names(names);
    }
    return table.add_breakpoints_in_function(
      std::string(registry_name) + ">" + alias, {line}, "");
  }

  void remove_host_breakpoint(octave_user_code *body, int line) {
    auto host = registry_host();
    auto& table = interp.get_evaluator().get_bp_table();
    const auto alias = alias_for_body(body);
    if (host.user_code_value()->scope().find_subfunction(alias).is_user_code())
      table.remove_breakpoint_from_function(std::string(registry_name) + ">" + alias, line);
    body->body()->delete_breakpoint(line);
    // Octave registers the normalized host name but qualified removal can leave
    // that membership behind. Reconcile only an empty host, preserving all other
    // prepared and ordinary file breakpoints.
    if (table.get_breakpoint_list(ovl(registry_name)).empty())
      table.remove_all_breakpoints_from_function(registry_name, true);
  }

  struct prepared_key {
    std::string file;
    std::string scope_file;
    std::string local_function;
    bool operator<(const prepared_key& other) const {
      return std::tie(file, scope_file, local_function)
           < std::tie(other.file, other.scope_file, other.local_function);
    }
  };

  std::string canonical_file(const std::string& file) {
    std::error_code ec;
    const auto path = fs::canonical(file, ec);
    if (ec || !fs::is_regular_file(path, ec))
      error_with_id("MathLab:PreparedSource", "Prepared source file does not exist");
    return path.string();
  }

  octave_value prepared_script(const std::string& file) {
    auto found = prepared_sources.find(file);
    if (found != prepared_sources.end()) return found->second;
    fs::path path(file);
    auto parsed = octave::parse_fcn_file(interp, file, path.filename().string(),
      path.parent_path().string(), "", "", true, true, false, false);
    if (!parsed.is_user_script())
      error_with_id("MathLab:PreparedSource", "Prepared function/class buffers must be saved before setting breakpoints");
    prepared_sources.emplace(file, parsed);
    return parsed;
  }

  octave_value prepared_target(const prepared_key& target) {
    auto script = prepared_script(target.scope_file);
    if (target.local_function.empty()) {
      if (target.file != target.scope_file)
        error_with_id("MathLab:PreparedSource", "Script breakpoint must belong to its prepared source scope");
      return script;
    }
    // Preparing an edited buffer does not execute its worker registration yet.
    // Make its private capsule discoverable before resolving a local target.
    // Appending preserves the user's existing global search precedence.
    const auto capsule = fs::path(target.scope_file).parent_path().string();
    if (!interp.get_load_path().contains_canonical(capsule))
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
      mathlab_adapter_binding_v2::call_primitive(intrinsic_binding(), interp, "addpath", ovl(capsule, "-end"), 0);
#else
      interp.feval("addpath", ovl(capsule, "-end"), 0);
#endif
    octave_value function;
    if (!registry_file.empty()) {
      const auto host = registry_host(); // Never resolve a paused user's local helper name.
      const auto lookup = interp.feval(host.function_value(),
        ovl(target.scope_file, Cell(), "lookup"), 1);
      const Cell handles = lookup(0).cell_value();
      for (octave_idx_type index = 0; index < handles.numel(); ++index) {
        if (!handles(index).is_function_handle()) continue;
        auto *handle = handles(index).fcn_handle_value();
        if (handle->fcn_name() != target.local_function) continue;
        function = handle->fcn_val();
        if (function.is_undefined()) {
          auto *pointer = handle->function_value(true);
          if (pointer) function = octave_value(pointer, true);
        }
        break;
      }
    }
    if (function.is_undefined())
      function = interp.get_symbol_table().find_function(
        target.local_function, script.user_code_value()->scope());
    if (!function.is_user_function()
        || canonical_file(function.user_code_value()->scope().fcn_file_name()) != target.file)
      error_with_id("MathLab:PreparedSource", "Local breakpoint did not resolve to its prepared function");
    return function;
  }

  octave_user_code *breakpoint_body(const octave_value& code, int line) {
    auto *selected = code.user_code_value();
    for (const auto& [name, subfunction] : selected->subfunctions()) {
      auto *nested = subfunction.user_code_value(true);
      if (nested && nested->beg_pos().line() <= line && nested->end_pos().line() >= line)
        return breakpoint_body(subfunction, line);
    }
    return selected;
  }
  void write_json(const fs::path& destination, const octave_value& value) {
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
    auto encoded = mathlab_adapter_binding_v2::call_primitive(intrinsic_binding(), interp, "jsonencode", ovl(value), 1);
#else
    auto encoded = interp.feval("jsonencode", ovl(value), 1);
#endif
    const auto temporary = destination.string() + ".pending";
    {
      std::ofstream output(temporary, std::ios::binary | std::ios::trunc);
      output << encoded(0).string_value() << '\n';
    }
    fs::rename(temporary, destination);
  }

  void publish(bool paused, const std::string& file, int line) {
    is_paused = paused;
    paused_file = file;
    paused_line = line;
    octave_scalar_map state;
    state.assign("revision", static_cast<double>(++revision));
    state.assign("paused", paused);
    state.assign("file", file);
    state.assign("line", line);
    state.assign("function_name", function_name);
    write_json(root / "debug-state.json", state);
  }

  void command(const std::string& raw) {
    double id = 0;
    octave_scalar_map response;
    try {
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
      auto decoded = mathlab_adapter_binding_v2::call_primitive(intrinsic_binding(), interp, "jsondecode", ovl(raw), 1);
#else
      auto decoded = interp.feval("jsondecode", ovl(raw), 1);
#endif
      auto request = decoded(0).scalar_map_value();
      id = request.getfield("id").double_value();
      const std::string action = request.getfield("command").string_value();
      if (action != "breakpoint" && action != "clear_breakpoint"
          && action != "prepared_breakpoint" && action != "prepared_clear_breakpoint") {
        if (!is_paused)
          error_with_id("MathLab:DebugState", "The interpreter is not paused");
        if (request.contains("revision")
            && request.getfield("revision").double_value() != static_cast<double>(revision))
          error_with_id("MathLab:DebugState", "The paused scope changed before this command was processed");
      }
      octave_value result;
      if (action == "continue") {
        interp.get_evaluator().dbcont();
        publish(false, "", 0);
      } else if (action == "step") {
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
        mathlab_adapter_binding_v2::call_primitive(intrinsic_binding(), interp, "dbstep", octave_value_list(), 0);
#else
        interp.feval("dbstep", octave_value_list(), 0);
#endif
        publish(false, "", 0);
      } else if (action == "step_in") {
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
        mathlab_adapter_binding_v2::call_primitive(intrinsic_binding(), interp, "dbstep", ovl("in"), 0);
#else
        interp.feval("dbstep", ovl("in"), 0);
#endif
        publish(false, "", 0);
      } else if (action == "step_out") {
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
        mathlab_adapter_binding_v2::call_primitive(intrinsic_binding(), interp, "dbstep", ovl("out"), 0);
#else
        interp.feval("dbstep", ovl("out"), 0);
#endif
        publish(false, "", 0);
      } else if (action == "quit") {
        if (input_adapter.is_defined())
          interp.feval(input_adapter, ovl("cancel_wait"), 0);
        quit_requested = true;
        if (!evaluation_captures.empty())
          interp.get_evaluator().reset_debug_state(false);
        interp.get_evaluator().dbquit(true);
        publish(false, "", 0);
      } else if (action == "workspace") {
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
        auto values = mathlab_adapter_binding_v2::call_primitive(intrinsic_binding(), interp, "whos", octave_value_list(), 1);
#else
        auto values = interp.feval("whos", octave_value_list(), 1);
#endif
        result = values(0);
      } else if (action == "stack") {
        const auto frames = interp.get_evaluator().backtrace_frames();
        octave_idx_type count = 0;
        for (const auto& frame : frames)
          if (frame->is_user_fcn_frame() || frame->is_user_script_frame()) ++count;
        Cell files(dim_vector(1, count)), names(dim_vector(1, count)), lines(dim_vector(1, count));
        octave_idx_type index = 0;
        for (const auto& frame : frames) {
          if (!frame->is_user_fcn_frame() && !frame->is_user_script_frame()) continue;
          auto *code = dynamic_cast<octave_user_code *>(frame->function());
          const auto scope_file = code ? code->scope().fcn_file_name() : "";
          files(index) = scope_file.empty() ? frame->fcn_file_name() : scope_file;
          names(index) = frame->fcn_name();
          lines(index) = std::max(0, frame->line());
          ++index;
        }
        octave_map stack(dim_vector(1, count));
        stack.assign("file", files);
        stack.assign("name", names);
        stack.assign("line", lines);
        result = stack;
      } else if (action == "evaluate" || action == "evaluate_stream") {
        evaluation_interrupt_guard restore_debug(interp.get_evaluator(), quit_requested,
                                                   evaluation_captures.empty());
        const auto code = request.getfield("code").string_value();
        const auto saved_file = paused_file;
        const auto saved_line = paused_line;
        const auto saved_function = function_name;
        const auto saved_revision = revision;
        octave::unwind_action restore_pause([this, saved_file, saved_line,
                                            saved_function, saved_revision]() {
          if (!quit_requested && revision != saved_revision) {
            // A nested watch may return normally or throw. Both unwind back to
            // the enclosing paused scope unless the user requested Quit.
            function_name = saved_function;
            publish(true, saved_file, saved_line);
          }
        });
        // Preserve the caller's real paused frame. The legacy API returns a
        // bounded capture; user evaluations can also forward those bytes live.
        // Do not use evalc here: input() resets/deletes its saved pager buffer.
        octave::flush_stdout();
        evaluation_capture capture {action == "evaluate_stream", {}, 0};
        const bool outermost = evaluation_captures.empty();
        auto *saved_out = std::cout.rdbuf();
        auto *saved_error = std::cerr.rdbuf();
        evaluation_stream out(saved_out, evaluation_captures);
        evaluation_stream errors(saved_error, evaluation_captures);
        if (outermost) {
          std::cout.rdbuf(&out);
          std::cerr.rdbuf(&errors);
        }
        evaluation_captures.push_back(&capture);
        octave::unwind_action restore_streams([this, outermost, saved_out, saved_error]() {
          try { octave::flush_stdout(); } catch (...) { }
          evaluation_captures.pop_back();
          if (outermost) {
            std::cout.rdbuf(saved_out);
            std::cerr.rdbuf(saved_error);
          }
        });
        int parse_status = 0;
        interp.eval_string(code, false, parse_status, 0);
        octave::flush_stdout();
        if (parse_status != 0)
          error_with_id("MathLab:DebugEvaluation", "Unable to parse debugger expression");
        // Raw output can contain invalid UTF-8, including a valid character
        // clipped mid-sequence. Normalize the returned copy before JSON; the
        // live console keeps the original bytes and its own decoding contract.
        std::string returned_output = capture.output;
        octave::string::u8_validate("MathLab debugger output", returned_output);
        if (capture.dropped != 0)
          returned_output += "\n[MathLab: returned evaluation output clipped at 1 MiB]\n";
        result = returned_output;
        response.assign("evaluation_streamed", capture.live);
        response.assign("output_truncated", capture.dropped != 0);
      } else if (action == "prepared_breakpoint" || action == "prepared_clear_breakpoint") {
        prepared_key target {canonical_file(request.getfield("file").string_value()),
          canonical_file(request.getfield("scope_file").string_value()), ""};
        if (request.contains("local_function"))
          target.local_function = request.getfield("local_function").string_value();
        const int line = request.getfield("line").int_value();
        if (line < 1) error_with_id("MathLab:BreakpointLine", "Breakpoint line must be positive");
        auto code = prepared_target(target);
        if (action == "prepared_breakpoint" && request.contains("snapshot_file")
            && request.contains("original_file")) {
          const auto snapshot = fs::path(canonical_file(request.getfield("snapshot_file").string_value()));
          const auto size = fs::file_size(snapshot);
          if (snapshot.parent_path() != fs::path(target.scope_file).parent_path()
              || size > 8 * 1024 * 1024)
            error_with_id("MathLab:SourceSnapshot", "Source snapshot is outside its generation or exceeds 8 MiB");
          std::ifstream input(snapshot, std::ios::binary);
          if (!input) error_with_id("MathLab:SourceSnapshot", "Could not read prepared source snapshot");
          std::string source(size + 1, '\0');
          input.read(source.data(), static_cast<std::streamsize>(source.size()));
          if (input.bad() || static_cast<std::uintmax_t>(input.gcount()) != size)
            error_with_id("MathLab:SourceSnapshot", "Source snapshot changed or could not be read completely");
          source.resize(size);
          mathlab_source_context::bindings_type reflected;
          octave_user_code *script = nullptr;
          if (code.is_user_script()) script = code.user_code_value();
          else reflected.emplace_back(target.local_function, code);
          // Public breakpoint registration sees the final display metadata from
          // its first bind; execution will keep the same immutable scope key.
          mathlab_source_context::reflect_functions(reflected, request.getfield("original_file"), octave_value(source), script);
        }
        auto *body = breakpoint_body(code, line);
        if (action == "prepared_breakpoint") {
          const auto lines = register_host_breakpoint(body, line);
          if (lines.empty() || *lines.begin() <= 0)
            error_with_id("MathLab:BreakpointUnavailable", "Prepared source has no executable line at this location");
          prepared_breakpoints[target].insert(lines.begin(), lines.end());
          result = *lines.begin();
        } else {
          remove_host_breakpoint(body, line);
          auto found = prepared_breakpoints.find(target);
          if (found != prepared_breakpoints.end()) {
            found->second.erase(line);
            if (found->second.empty()) prepared_breakpoints.erase(found);
          }
        }
        interp.get_evaluator().reset_debug_state(true);
      } else if (action == "breakpoint") {
        const auto file = request.getfield("file").string_value();
        const int line = request.getfield("line").int_value();
        std::error_code ec;
        if (!fs::path(file).is_absolute() || !fs::is_regular_file(file, ec)
            || fs::path(file).extension() != ".m" || line < 1)
          error_with_id("MathLab:BreakpointFile", "A breakpoint requires an existing absolute .m file and a positive line");
        auto parent = fs::canonical(file, ec).parent_path();
        if (ec) error_with_id("MathLab:BreakpointFile", "Cannot resolve the source file");
        if (!parent.filename().string().empty() && parent.filename().string()[0] == '@')
          parent = parent.parent_path();
        if (!interp.get_load_path().contains_canonical(parent.string()))
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
          mathlab_adapter_binding_v2::call_primitive(intrinsic_binding(), interp, "addpath", ovl(parent.string(), "-end"), 0);
#else
          interp.feval("addpath", ovl(parent.string(), "-end"), 0);
#endif
        const int actual = interp.get_evaluator().get_bp_table().add_breakpoint_in_file(file, line);
        if (actual <= 0)
          error_with_id("MathLab:BreakpointUnavailable", "No breakpoint was installed: the file is shadowed, unsupported, or has no executable line at this location");
        result = actual;
      } else if (action == "clear_breakpoint") {
        const auto file = request.getfield("file").string_value();
        const int line = request.getfield("line").int_value();
        interp.get_evaluator().get_bp_table().remove_breakpoint_from_file(file, line);
      } else {
        error_with_id("MathLab:DebugCommand", "Unknown debugger command");
      }
      response.assign("ok", true);
      response.assign("result", result.is_defined() ? result : octave_value(Matrix()));
    } catch (const octave::execution_exception& caught) {
      response.assign("ok", false);
      response.assign("error", caught.message());
    } catch (const octave::interrupt_exception&) {
      // A canceled watch must release its client while the genuine interrupt
      // still unwinds through Octave's debugger/control boundary.
      response.assign("ok", false);
      response.assign("error", "Debugger evaluation interrupted");
      response.assign("id", id);
      try { write_json(root / ("debug-response-" + std::to_string(static_cast<long long>(id)) + ".json"), response); } catch (...) { }
      throw;
    } catch (...) {
      if (quit_requested) {
        response.assign("ok", false);
        response.assign("error", "Debugger evaluation stopped");
        response.assign("id", id);
        try { write_json(root / ("debug-response-" + std::to_string(static_cast<long long>(id)) + ".json"), response); } catch (...) { }
      }
      throw;
    }
    response.assign("id", id);
    write_json(root / ("debug-response-" + std::to_string(static_cast<long long>(id)) + ".json"), response);
  }

  octave::interpreter& interp;
  fs::path root;
  std::atomic<bool> active{true};
  std::thread monitor;
  std::uint64_t revision{0};
  bool is_paused{false};
  bool quit_requested{false};
  std::string paused_file;
  int paused_line{0};
  std::string function_name;
  std::string registry_file;
  std::map<std::string, octave_value> prepared_sources;
  std::map<prepared_key, std::set<int>> prepared_breakpoints;
  std::map<std::tuple<std::string, std::string, int>, std::string> prepared_aliases;
  std::vector<evaluation_capture *> evaluation_captures;
  octave_value input_adapter;
};

std::shared_ptr<debug_events> current_events;
}

#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
DEFMETHOD_DLD(mathlab_debug_bridge, interp, args, nargout,
             "Install the Open MathLab debugger event bridge in a private session directory.") {
#else
DEFMETHOD_DLD(mathlab_debug_bridge, interp, args, ,
             "Install the Open MathLab debugger event bridge in a private session directory.") {
#endif
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
  if (mathlab_adapter_binding_v2::is_bind_operation(args)) {
    mathlab_adapter_binding_v2::bind(intrinsic_binding(), interp, args, nargout,
      "debug", "mathlab_debug_bridge",
      {"addpath", "jsonencode", "jsondecode", "dbstep", "whos", "source", "evalin", "mlock"});
    return octave_value_list();
  }
  mathlab_adapter_binding_v2::active(intrinsic_binding(), interp);
#endif
  if (args.length() == 2 || args.length() == 4 || args.length() == 5) {
    const auto operation = args(0).string_value();
    if (operation != "source" && args.length() != 2) print_usage();
    if (operation == "protocol") {
      octave_scalar_map protocol;
      protocol.assign("protocol", "mathlab-debugger");
      protocol.assign("version", 4);
      protocol.assign("source_entry", true);
      protocol.assign("stable_registry", true);
      protocol.assign("off_path_registry", true);
      protocol.assign("breakpoint_source_metadata", true);
      protocol.assign("streamed_evaluation", true);
      protocol.assign("input_wait_cancel", true);
      return ovl(protocol);
    }
    if (operation != "source" && operation != "evaluate" && operation != "refresh_source"
        && operation != "synchronize" && operation != "registry" && operation != "input_adapter") print_usage();
    if (!current_events)
      error_with_id("MathLab:DebugState", "The debugger adapter is not installed");
    if (operation == "registry") {
      current_events->configure_registry(args(1).string_value());
      return ovl(true);
    }
    if (operation == "input_adapter") {
      current_events->configure_input(args(1));
      return ovl(true);
    }
    if (operation == "refresh_source") {
      current_events->refresh_source(args(1).string_value());
      return ovl(true);
    }
    if (operation == "synchronize") {
      current_events->synchronize_prepared();
      return ovl(true);
    }
    try {
      if (operation == "source") {
        const auto file = args(1).string_value();
        if (args.length() == 5) {
          if (!args(2).iscell()) print_usage();
          const Cell retained = args(2).cell_value();
          current_events->source_prepared(file, &retained, &args(3), &args(4));
        }
        else if (args.length() == 4)
          current_events->source_prepared(file, nullptr, &args(2), &args(3));
        else if (current_events->has_prepared_source(file))
          current_events->source_prepared(file);
        else
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
          mathlab_adapter_binding_v2::call_primitive(intrinsic_binding(), interp, "source", ovl(args(1), "base"), 0);
#else
          interp.feval("source", ovl(args(1), "base"), 0);
#endif
      }
      else {
        if (current_events->has_prepared_breakpoints())
          current_events->evaluate_base(args(1).string_value());
        else
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
          mathlab_adapter_binding_v2::call_primitive(intrinsic_binding(), interp, "evalin", ovl("base", args(1)), 0);
#else
          interp.feval("evalin", ovl("base", args(1)), 0);
#endif
      }
      return ovl(true);
    } catch (const octave::execution_exception&) {
      throw;
    } catch (const octave::interrupt_exception&) {
      throw;
    } catch (...) {
      // GNU Octave's quit_debug_exception is private to pt-eval.cc. Bound its
      // expected unwind here: --eval's top level does not catch it. All ordinary
      // execution errors and interrupts keep their existing propagation paths.
      if (current_events->consume_requested_quit()) {
        interp.recover_from_exception();
        return ovl(false);
      }
      throw;
    }
  }
  if (args.length() == 0) {
    interp.get_event_manager().connect_link(nullptr);
    current_events.reset();
    interp.get_evaluator().server_mode(false);
    return ovl(true);
  }
  if (args.length() == 1 && args(0).string_value() == "poll") {
    interp.get_event_manager().process_events();
    return ovl(true);
  }
  if (args.length() != 1) print_usage();
  const std::string directory = args(0).string_value();
  if (directory.empty()) error_with_id("MathLab:DebugDirectory", "A debugger session directory is required");
#if defined(MATHLAB_QUALIFIED_CORE_V2) && MATHLAB_QUALIFIED_CORE_V2 == 1
  mathlab_adapter_binding_v2::call_primitive(intrinsic_binding(), interp, "mlock", octave_value_list(), 0);
  if (mathlab_adapter_binding_v2::active(intrinsic_binding(), interp)) {
    if (!intrinsic_binding().shutdown_proxy_registered)
      error_with_id("MathLab:AdapterBindingUnavailable", "Qualified debugger requires its verified provider shutdown proxy");
  } else {
    interp.add_atexit_fcn("mathlab_debug_bridge");
  }
#else
  interp.feval("mlock", octave_value_list(), 0);
  interp.add_atexit_fcn("mathlab_debug_bridge");
#endif
  interp.get_evaluator().server_mode(true);
  current_events = std::make_shared<debug_events>(interp, directory);
  interp.get_event_manager().connect_link(current_events);
  interp.get_event_manager().enable();
  current_events->start();
  return ovl(true);
}
