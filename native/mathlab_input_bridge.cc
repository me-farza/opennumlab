// SPDX-License-Identifier: GPL-3.0-or-later
// Delegate numerical/input-expression semantics to GNU Octave's original input
// builtin. Only the genuine command-editor read uses the private reply mailbox.
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include <octave/oct.h>
#include <octave/defun-dld.h>
#include <octave/interpreter.h>
#include <octave/symtab.h>
#include <octave/ov-builtin.h>
#include <octave/ov-dld-fcn.h>
#include <octave/pt-eval.h>
#include <octave/cmd-edit.h>
#include <octave/event-manager.h>
#include <octave/pager.h>
#include <octave/unwind-prot.h>
#include <octave/quit.h>
#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <cerrno>
#include <exception>
#include <filesystem>
#include <fstream>
#include <limits>
#include <memory>
#include <string>
#include <thread>
#include <vector>
#if defined(__unix__) || defined(__APPLE__)
#include <unistd.h>
#endif

namespace fs = std::filesystem;
namespace {
constexpr std::size_t max_text = 1024 * 1024;
constexpr std::size_t max_prompt = 64 * 1024;
constexpr std::size_t max_metadata = 128 * 1024;
constexpr std::size_t max_reply_file = 8 * 1024 * 1024;
constexpr std::uint64_t max_integer = 9007199254740991ULL;

struct key {
  std::uint64_t generation{0}, invocation{0}, request{0}, input{0};
};
struct input_reader;
struct bridge_state {
  fs::path directory;
  std::string nonce;
  key current;
  std::uint64_t next_input{1};
  bool enabled{false};
  octave_value original_input, wrapped_input, json_encode, json_decode;
  octave_builtin::meth original_method{nullptr};
  input_reader *waiting{nullptr};
  octave::dynamic_library *library_pin{nullptr};
};
bridge_state& state() {
  // The core builtin table and saved @input handles can outlive the autoload
  // entry. Keep one process-lifetime owner, avoiding shared-library destructor
  // order and callbacks into an unloaded module after munlock/clear functions.
  static bridge_state *value = new bridge_state;
  return *value;
}

std::uint64_t integer(const octave_value& value, const char *message, bool allow_zero = false, const char *identifier = "MathLab:InputContext") {
  if (!value.isnumeric() || !value.is_scalar_type())
    error_with_id(identifier, "%s", message);
  const double number = value.double_value();
  if (!std::isfinite(number) || number != std::floor(number)
      || number < (allow_zero ? 0.0 : 1.0) || number > static_cast<double>(max_integer))
    error_with_id(identifier, "%s", message);
  return static_cast<std::uint64_t>(number);
}

bool utf8(const std::string& value) {
  for (std::size_t index = 0; index < value.size();) {
    const auto first = static_cast<unsigned char>(value[index++]);
    if (first < 0x80) continue;
    unsigned continuation = 0;
    std::uint32_t scalar = 0, minimum = 0;
    if (first >= 0xc2 && first <= 0xdf) { continuation = 1; scalar = first & 0x1f; minimum = 0x80; }
    else if (first >= 0xe0 && first <= 0xef) { continuation = 2; scalar = first & 0x0f; minimum = 0x800; }
    else if (first >= 0xf0 && first <= 0xf4) { continuation = 3; scalar = first & 0x07; minimum = 0x10000; }
    else return false;
    if (continuation > value.size() - index) return false;
    while (continuation--) {
      const auto next = static_cast<unsigned char>(value[index++]);
      if ((next & 0xc0) != 0x80) return false;
      scalar = (scalar << 6) | (next & 0x3f);
    }
    if (scalar < minimum || scalar > 0x10ffff || (scalar >= 0xd800 && scalar <= 0xdfff)) return false;
  }
  return true;
}

std::string suffix(const key& value) {
  return std::to_string(value.generation) + "-" + std::to_string(value.invocation)
    + "-" + std::to_string(value.request) + "-" + std::to_string(value.input);
}
fs::path file(const char *kind, const key& value) {
  return state().directory / (std::string("input-") + kind + "-" + suffix(value) + ".json");
}
octave_scalar_map identity(const key& value) {
  octave_scalar_map result;
  result.assign("generation", static_cast<double>(value.generation));
  result.assign("invocation", static_cast<double>(value.invocation));
  result.assign("request_id", static_cast<double>(value.request));
  result.assign("input_id", static_cast<double>(value.input));
  return result;
}

octave_value call(octave::interpreter& interp, const octave_value& function,
                  const octave_value_list& arguments) {
  const auto result = interp.feval(function.function_value(), arguments, 1);
  if (result.length() != 1)
    error_with_id("MathLab:InputTransport", "Internal JSON builtin returned an invalid result");
  return result(0);
}

void atomic_json(octave::interpreter& interp, const fs::path& target, const octave_value& value) {
  const auto encoded = call(interp, state().json_encode, ovl(value)).string_value();
  if (encoded.size() > max_metadata)
    error_with_id("MathLab:InputTransport", "Input metadata exceeds the 128 KiB limit");
  const auto temporary = target.string() + ".pending";
  std::ofstream stream(temporary, std::ios::binary | std::ios::trunc);
  if (!stream) error_with_id("MathLab:InputTransport", "Cannot create input metadata");
  stream.write(encoded.data(), static_cast<std::streamsize>(encoded.size()));
  stream.close();
  if (!stream) error_with_id("MathLab:InputTransport", "Incomplete input metadata write");
  fs::rename(temporary, target);
}

void fence(char kind, const key& value) {
  const auto text = std::string("\x1eMATHLAB:") + state().nonce + ":"
    + std::to_string(value.generation) + ":" + kind + ":"
    + std::to_string(value.invocation) + ":" + std::to_string(value.request)
    + ":" + std::to_string(value.input) + "\x1f";
  if (text.size() > 512)
    error_with_id("MathLab:InputTransport", "Input fence exceeds the atomic pipe limit");
  octave::flush_stdout();
  std::fflush(stdout);
  std::fflush(stderr);
#if defined(__unix__) || defined(__APPLE__)
  for (;;) {
    const auto count = ::write(STDOUT_FILENO, text.data(), text.size());
    if (count == static_cast<ssize_t>(text.size())) return;
    if (count < 0 && errno == EINTR) continue;
    error_with_id("MathLab:InputTransport", "Cannot publish the input fence atomically");
  }
#else
  error_with_id("MathLab:InputPlatform", "Interactive input currently requires a Unix console pipe");
#endif
}

std::string bounded_reply(const fs::path& path) {
  const auto size = fs::file_size(path);
  if (size > max_reply_file)
    error_with_id("MathLab:InputReply", "Input reply file exceeds the 8 MiB limit");
  std::ifstream stream(path, std::ios::binary);
  if (!stream) error_with_id("MathLab:InputReply", "Cannot open input reply");
  std::string bytes(static_cast<std::size_t>(size) + 1, '\0');
  stream.read(bytes.data(), static_cast<std::streamsize>(bytes.size()));
  if (stream.bad() || stream.gcount() != static_cast<std::streamsize>(size))
    error_with_id("MathLab:InputReply", "Input reply changed while being read");
  bytes.resize(static_cast<std::size_t>(size));
  return bytes;
}

void check_identity(const octave_scalar_map& reply, const key& expected) {
  for (const auto *name : {"generation", "invocation", "request_id", "input_id"})
    if (!reply.contains(name)) error_with_id("MathLab:InputReply", "Input reply is missing its identity");
  if (integer(reply.getfield("generation"), "Invalid reply generation", false, "MathLab:InputReply") != expected.generation
      || integer(reply.getfield("invocation"), "Invalid reply invocation", false, "MathLab:InputReply") != expected.invocation
      || integer(reply.getfield("request_id"), "Invalid reply request", false, "MathLab:InputReply") != expected.request
      || integer(reply.getfield("input_id"), "Invalid reply input", false, "MathLab:InputReply") != expected.input)
    error_with_id("MathLab:InputReply", "Input reply belongs to an obsolete or different request");
}

bool logical(const octave_scalar_map& map, const char *name) {
  if (!map.contains(name) || !map.getfield(name).islogical() || !map.getfield(name).is_scalar_type())
    error_with_id("MathLab:InputReply", "Input reply field %s must be a logical scalar", name);
  return map.getfield(name).bool_value();
}

struct input_reader {
  octave::interpreter& interp;
  key id;
  std::string prompt;
  bool literal;
  bool announced{false}, finished{false}, loaded{false}, eof{false}, canceled{false};
  std::string line;
  std::size_t offset{0};
  std::exception_ptr deferred;

  input_reader(octave::interpreter& interpreter, std::string text, bool string_mode)
    : interp(interpreter), id(state().current), prompt(std::move(text)), literal(string_mode) {
    if (state().next_input > max_integer)
      error_with_id("MathLab:InputContext", "Input identity space is exhausted; restart the session");
    id.input = state().next_input++;
  }

  void announce() {
    state().waiting = this;
    auto metadata = identity(id);
    metadata.assign("prompt", prompt);
    metadata.assign("mode", literal ? "string" : "expression");
    atomic_json(interp, file("request", id), metadata);
    // The original command editor has already printed and flushed its prompt.
    fence('I', id);
    announced = true;
  }

  void finish(const char *outcome) {
    auto metadata = identity(id);
    metadata.assign("outcome", outcome);
    atomic_json(interp, file("finish", id), metadata);
    fence('F', id);
    finished = true;
    if (state().waiting == this) state().waiting = nullptr;
  }

  void load() {
    const auto path = file("reply", id);
    while (!fs::exists(path)) {
      octave_quit();
      if (canceled) throw octave::interrupt_exception();
      interp.get_event_manager().process_events();
      if (canceled) throw octave::interrupt_exception();
      std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    octave_quit();
    if (canceled) throw octave::interrupt_exception();
    const auto reply_value = call(interp, state().json_decode, ovl(bounded_reply(path)));
    if (!reply_value.isstruct() || reply_value.numel() != 1)
      error_with_id("MathLab:InputReply", "Input reply must be a JSON object");
    const auto reply = reply_value.scalar_map_value();
    check_identity(reply, id);
    eof = logical(reply, "eof");
    const bool echo = logical(reply, "echo");
    if (!reply.contains("text") || !reply.getfield("text").is_string())
      error_with_id("MathLab:InputReply", "Input reply text must be a string");
    line = reply.getfield("text").string_value();
    if (line.size() > max_text || !utf8(line) || line.find_first_of(std::string("\0\r\n", 3)) != std::string::npos)
      error_with_id("MathLab:InputReply", "Input replies must be valid UTF-8 single lines of at most 1 MiB, without NUL/CR/LF");
    if (eof && !line.empty())
      error_with_id("MathLab:InputReply", "An EOF reply cannot also contain a line");
    fs::remove(path);
    loaded = true;
    if (!eof) {
      line += '\n';
      if (echo) {
        if (std::fwrite(line.data(), 1, line.size(), stdout) != line.size())
          error_with_id("MathLab:InputTransport", "Cannot echo the accepted input line");
        std::fflush(stdout);
      }
    }
  }

  std::size_t read(char *buffer, std::size_t length) {
    if (finished || length == 0) return 0;
    if (!announced) announce();
    octave_quit();
    if (!loaded) load();
    if (eof) { finish("eof"); return 0; }
    const auto count = std::min(length, line.size() - offset);
    std::memcpy(buffer, line.data() + offset, count);
    offset += count;
    if (offset == line.size()) finish("line");
    return count;
  }

  void finish_abort() {
    if (!announced || finished || !deferred) return;
    const char *outcome = "error";
    try { std::rethrow_exception(deferred); }
    catch (const octave::interrupt_exception&) { outcome = "interrupted"; }
    catch (...) { }
    // This runs only after libc's reader returned. Publishing cancellation
    // inside its FILE callback could itself unwind through a held stdio lock.
    finish(outcome);
  }
};

std::ptrdiff_t safe_read(input_reader *reader, char *buffer, std::size_t length) noexcept {
  try { return static_cast<std::ptrdiff_t>(reader->read(buffer, length)); }
  catch (...) {
    // Never unwind an interpreter/debugger exception through libc's FILE lock.
    // The original builtin may raise a generic EOF error; rethrow the saved
    // exception only after its stdio reader has returned to the C++ wrapper.
    reader->deferred = std::current_exception();
    if (state().waiting == reader) state().waiting = nullptr;
    errno = EINTR;
    return -1;
  }
}
#if defined(__APPLE__)
int cookie_read(void *cookie, char *buffer, int length) {
  return static_cast<int>(safe_read(static_cast<input_reader *>(cookie), buffer, static_cast<std::size_t>(length)));
}
int cookie_close(void *) { return 0; }
FILE *reader_file(input_reader& reader) { return funopen(&reader, cookie_read, nullptr, nullptr, cookie_close); }
#elif defined(__linux__)
ssize_t cookie_read(void *cookie, char *buffer, std::size_t length) {
  return static_cast<ssize_t>(safe_read(static_cast<input_reader *>(cookie), buffer, length));
}
int cookie_close(void *) { return 0; }
FILE *reader_file(input_reader& reader) {
  cookie_io_functions_t callbacks{};
  callbacks.read = cookie_read;
  callbacks.close = cookie_close;
  return fopencookie(&reader, "r", callbacks);
}
#else
FILE *reader_file(input_reader&) { return nullptr; }
#endif

octave_value_list input_wrapper_impl(octave::interpreter& interp, const octave_value_list& args, int nargout) {
  auto& bridge = state();
  if (bridge.waiting)
    error_with_id("MathLab:InputBusy", "Another input read is already waiting for a reply");
  if (!bridge.enabled || bridge.current.request == 0)
    error_with_id("MathLab:InputContext", "Interactive input has no active MathLab request");
  if (args.length() < 1 || args.length() > 2)
    return bridge.original_method(interp, args, nargout);
  const auto prompt = args(0).xstring_value("input: unrecognized argument");
  octave_value_list forwarded = args;
  forwarded(0) = prompt;
  bool literal = false;
  if (args.length() == 2) {
    const auto mode = args(1).xstring_value("input: second argument must be 's'.");
    if (mode != "s") return bridge.original_method(interp, args, nargout);
    literal = true;
    forwarded(1) = "s";
  }
  if (prompt.size() > max_prompt || !utf8(prompt))
    error_with_id("MathLab:InputPrompt", "Input prompts must be valid UTF-8 and at most 64 KiB");
  input_reader reader(interp, prompt, literal);
  FILE *stream = reader_file(reader);
  if (!stream) error_with_id("MathLab:InputPlatform", "The native input stream adapter is unavailable on this platform");
  FILE *previous = octave::command_editor::get_input_stream();
  octave::unwind_action restore([previous, stream, &reader]() {
    octave::command_editor::set_input_stream(previous);
    std::fclose(stream);
    if (state().waiting == &reader) state().waiting = nullptr;
  });
  octave::command_editor::set_input_stream(stream);
  try {
    auto result = bridge.original_method(interp, forwarded, nargout);
    if (reader.deferred) {
      reader.finish_abort();
      std::rethrow_exception(reader.deferred);
    }
    return result;
  } catch (...) {
    if (reader.deferred) {
      reader.finish_abort();
      std::rethrow_exception(reader.deferred);
    }
    throw;
  }
}

octave_value_list input_wrapper(octave::interpreter& interp, const octave_value_list& args, int nargout) {
  try { return input_wrapper_impl(interp, args, nargout); }
  catch (const octave::execution_exception&) { throw; }
  catch (const octave::interrupt_exception&) { throw; }
  catch (const std::exception& caught) {
    error_with_id("MathLab:InputTransport", "%s", caught.what());
  }
  return ovl();
}

key parsed_context(const octave_value_list& args, int first) {
  auto& bridge = state();
  key next;
  next.generation = integer(args(first), "Expected a positive worker generation");
  next.invocation = integer(args(first + 1), "Expected a positive worker invocation");
  next.request = integer(args(first + 2), "Expected a nonnegative request identity", true);
  if (bridge.current.generation != 0 && next.generation != bridge.current.generation)
    error_with_id("MathLab:InputContext", "Restart the worker to change its input generation");
  if (next.invocation < bridge.current.invocation)
    error_with_id("MathLab:InputContext", "Cannot restore an obsolete input invocation");
  return next;
}
void context(const octave_value_list& args, int first) {
  auto& bridge = state();
  if (!bridge.enabled) error_with_id("MathLab:InputContext", "The input adapter is not installed");
  if (bridge.waiting) error_with_id("MathLab:InputBusy", "Cannot replace an active input context");
  bridge.current = parsed_context(args, first);
}
} // namespace

DEFMETHOD_DLD(mathlab_input_bridge, interp, args, ,
             "Install authentic file-backed input reads while retaining GNU Octave builtin semantics.") {
  try {
    if (args.length() < 1) print_usage();
    const auto operation = args(0).string_value();
    if (operation == "protocol" && (args.length() == 1 || args.length() == 2)) {
      octave_scalar_map result;
      result.assign("protocol", "mathlab-input");
      result.assign("version", 1);
      result.assign("original_builtin", true);
      result.assign("caller_scope", true);
      result.assign("separate_reply", true);
      result.assign("genuine_reads", true);
      return ovl(result);
    }
    auto& bridge = state();
    if (operation == "cancel_wait" && args.length() == 1) {
      if (bridge.waiting) bridge.waiting->canceled = true;
      return ovl(bridge.waiting != nullptr);
    }
    if (operation == "install" && args.length() == 6) {
      if (bridge.waiting) error_with_id("MathLab:InputBusy", "Cannot install input during an active read");
      auto directory = fs::path(args(1).string_value());
      if (!directory.is_absolute() || !fs::is_directory(directory))
        error_with_id("MathLab:InputContext", "Input transport requires an existing absolute directory");
      directory = fs::canonical(directory);
      const auto nonce = args(2).string_value();
      if (nonce.empty() || nonce.size() > 128 || !std::all_of(nonce.begin(), nonce.end(), [](unsigned char ch) {
            return (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || (ch >= '0' && ch <= '9') || ch == '_' || ch == '-';
          })) error_with_id("MathLab:InputContext", "Invalid input framing nonce");
      const auto next_context = parsed_context(args, 3);
      auto& symbols = interp.get_symbol_table();
      if (!bridge.original_method) {
        bridge.original_input = symbols.find_built_in_function("input");
        auto *builtin = dynamic_cast<octave_builtin *>(bridge.original_input.function_value());
        if (!builtin || !builtin->is_builtin_function() || !builtin->method())
          error_with_id("MathLab:InputIdentity", "The original input builtin callback is unavailable");
        auto *owner = dynamic_cast<octave_dld_function *>(interp.get_evaluator().current_function());
        if (!owner) error_with_id("MathLab:InputIdentity", "Cannot retain the input adapter's native library");
        bridge.library_pin = new octave::dynamic_library(owner->get_shlib());
        bridge.original_method = builtin->method();
        bridge.json_encode = symbols.find_built_in_function("jsonencode");
        bridge.json_decode = symbols.find_built_in_function("jsondecode");
        bridge.wrapped_input = octave_value(new octave_builtin(input_wrapper, "input", builtin->src_file_name(), builtin->doc_string()));
      }
      const auto current = symbols.find_built_in_function("input");
      if (current.function_value() != bridge.original_input.function_value()
          && current.function_value() != bridge.wrapped_input.function_value())
        error_with_id("MathLab:InputIdentity", "Another adapter has replaced the input builtin");
      if (bridge.enabled && (directory != bridge.directory || nonce != bridge.nonce))
        error_with_id("MathLab:InputContext", "Restart the worker to replace its input transport");
      bridge.directory = directory;
      bridge.nonce = nonce;
      bridge.enabled = true;
      bridge.current = next_context;
      symbols.install_built_in_function("input", bridge.wrapped_input);
      interp.mlock();
      return ovl(true);
    }
    if (operation == "context" && args.length() == 4) {
      context(args, 1);
      return ovl(true);
    }
    if (operation == "detach" && args.length() == 1) {
      if (bridge.waiting) error_with_id("MathLab:InputBusy", "Cannot detach during an active read");
      if (bridge.original_method) interp.get_symbol_table().install_built_in_function("input", bridge.original_input);
      bridge.enabled = false;
      bridge.current.request = 0;
      return ovl(true);
    }
    error_with_id("MathLab:InputProtocol", "Unsupported input adapter operation or arguments");
  } catch (const octave::execution_exception&) { throw; }
    catch (const octave::interrupt_exception&) { throw; }
    catch (const std::exception& caught) {
      error_with_id("MathLab:InputTransport", "%s", caught.what());
    }
  return ovl();
}
