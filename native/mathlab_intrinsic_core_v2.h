// SPDX-License-Identifier: GPL-3.0-or-later
// PRIVATE SOURCE PROPOSAL. Not compiled or runtime-qualified.
// Octave 11.3 selected API candidate; transport code only, no numerical ownership.
// Include this header in exactly ONE selected startup provider for a process.
#ifndef MATHLAB_INTRINSIC_CORE_V2_PROPOSAL_H
#define MATHLAB_INTRINSIC_CORE_V2_PROPOSAL_H

#include <octave/oct.h>
#include <octave/error.h>
#include <octave/interpreter.h>
#include <octave/load-path.h>
#include <octave/ov-builtin.h>
#include <octave/ov-dld-fcn.h>
#include <octave/ov-fcn-handle.h>
#include <octave/ov-fcn.h>
#include <octave/ov-usr-fcn.h>
#include <octave/parse.h>
#include <octave/pt-eval.h>
#include <octave/symtab.h>
#include <octave/unwind-prot.h>
#include <octave/version.h>
#include <cstdio>
#include <filesystem>
#include <string>
#include <string_view>
#include <vector>

namespace mathlab_intrinsic_v2_proposal {
namespace fs = std::filesystem;

static_assert (std::string_view (OCTAVE_VERSION) == "11.3.0"
               && std::string_view (OCTAVE_API_VERSION) == "api-v61",
               "Private provider V2 requires the exact selected Octave 11.3.0/api-v61 headers");

enum class bootstrap_phase { empty, binding, ready, failed };

struct role_spec {
  const char *role, *export_name, *protocol;
  int version;
  bool protocol_zero;
  const char *flags[7];
};
static const role_spec roles[] = {
  {"input", "mathlab_input_bridge", "mathlab-input", 1, false,
    {"original_builtin", "caller_scope", "separate_reply", "genuine_reads", nullptr}},
  {"scope", "mathlab_scope_bridge", "mathlab-scope", 2, true,
    {"pre_entry", "original_source_reflection", "source_entry", "stable_registry", "off_path_registry", nullptr}},
  {"value", "mathlab_value_bridge", "mathlab-value", 1, false,
    {"dense_complex_components", nullptr}},
  {"debug", "mathlab_debug_bridge", "mathlab-debugger", 4, true,
    {"source_entry", "stable_registry", "off_path_registry", "breakpoint_source_metadata", "streamed_evaluation", "input_wait_cancel", nullptr}},
  {"graphics", "mathlab_graphics_bridge", "mathlab-graphics", 1, false,
    {"native_object_lifetime", "bound_primitive_dispatcher", nullptr}}
};
struct adapter_slot {
  const role_spec *spec = nullptr;
  fs::path path;
  octave_value function;
};
struct runtime_state {
  bootstrap_phase phase = bootstrap_phase::empty;
  octave::interpreter *owner = nullptr;
  fs::path module, worker, registry;
  std::string entry_alias, shutdown_alias;
  std::vector<std::string> required_names;
  std::vector<fs::path> runtime_directories;
  std::vector<adapter_slot> adapters;
  octave_value dispatcher, caller_lock, provider, worker_function, who, whos;
  octave_value shutdown_proxy, carrier;
  bool initial_path_admitted = false, shutdown_registered = false;
  bool shutdown_started = false;
};
// Exactly one provider owns this code-only process pin; adapters never include it.
static runtime_state& state () {
  static runtime_state * const value = new runtime_state;
  return *value;
}

static std::string text (const octave_value& input, std::size_t limit,
                         const char *field) {
  if (! input.is_string () || input.isobject () || input.rows () != 1
      || input.numel () < 1 || static_cast<std::size_t> (input.numel ()) > limit)
    error_with_id ("MathLab:IntrinsicArguments", "%s must be a bounded character row", field);
  const auto value = input.string_value ();
  if (value.empty () || value.size () > limit || value.find ('\0') != std::string::npos)
    error_with_id ("MathLab:IntrinsicArguments", "%s has invalid bytes", field);
  return value;
}

static bool identifier (const std::string& value) {
  if (value.empty () || value.size () > 63) return false;
  const auto initial = [] (unsigned char c) {
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_';
  };
  if (! initial (value[0])) return false;
  for (unsigned char c : value)
    if (! initial (c) && ! (c >= '0' && c <= '9')) return false;
  return true;
}

static bool exact_utf8 (const std::string& value) {
  for (std::size_t i = 0; i < value.size ();) {
    const auto c = static_cast<unsigned char> (value[i++]);
    if (c < 0x80) continue;
    unsigned count = 0;
    unsigned code = 0, minimum = 0;
    if (c >= 0xc2 && c <= 0xdf) { count = 1; code = c & 0x1f; minimum = 0x80; }
    else if (c >= 0xe0 && c <= 0xef) { count = 2; code = c & 0x0f; minimum = 0x800; }
    else if (c >= 0xf0 && c <= 0xf4) { count = 3; code = c & 0x07; minimum = 0x10000; }
    else return false;
    if (value.size () - i < count) return false;
    while (count--) {
      const auto next = static_cast<unsigned char> (value[i++]);
      if ((next & 0xc0) != 0x80) return false;
      code = (code << 6) | (next & 0x3f);
    }
    if (code < minimum || code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff))
      return false;
  }
  return true;
}

static std::string path_text (const octave_value& input, const char *field) {
  const auto value = text (input, 4096, field);
  if (! exact_utf8 (value) || value.find_first_of ("\r\n") != std::string::npos)
    error_with_id ("MathLab:IntrinsicArguments", "%s must have exact UTF8 path bytes", field);
  return value;
}

static fs::path canonical_file (const octave_value& input, const char *field) {
  const fs::path path (path_text (input, field));
  std::error_code ec;
  const auto canonical = fs::canonical (path, ec);
  if (! path.is_absolute () || ec || canonical != path
      || ! fs::is_regular_file (canonical, ec) || ec)
    error_with_id ("MathLab:IntrinsicIdentity", "%s is not an existing canonical absolute file", field);
  return canonical;
}

static octave_dld_function * verified_dld (const octave_value& function,
                                          const fs::path& path, const char *name) {
  auto *actual = function.is_defined () && function.is_dld_function ()
    ? dynamic_cast<octave_dld_function *> (function.function_value ()) : nullptr;
  if (! actual || actual->name () != name)
    error_with_id ("MathLab:IntrinsicIdentity", "Selected native export identity is invalid");
  std::error_code ec;
  const auto function_file = fs::canonical (actual->fcn_file_name (), ec);
  if (ec || function_file != path)
    error_with_id ("MathLab:IntrinsicIdentity", "Selected native function file identity is invalid");
  const auto library = actual->get_shlib ();
  if (! library)
    error_with_id ("MathLab:IntrinsicIdentity", "Selected native library is not open");
  const auto library_file = fs::canonical (library.file_name (), ec);
  if (ec || library_file != path)
    error_with_id ("MathLab:IntrinsicIdentity", "Selected native library file identity is invalid");
  return actual;
}

static octave_value actual_builtin (octave::interpreter& interp, const std::string& name) {
  auto value = interp.get_symbol_table ().find_built_in_function (name);
  if (! value.is_defined () || ! value.is_builtin_function ())
    error_with_id ("MathLab:IntrinsicUnsupported", "Required genuine builtin %s is unavailable", name.c_str ());
  return value;
}

static runtime_state& initialized (octave::interpreter& interp) {
  auto& value = state ();
  if (value.owner != &interp || value.phase != bootstrap_phase::ready
      || value.dispatcher.is_undefined () || value.caller_lock.is_undefined ()
      || value.shutdown_started)
    error_with_id ("MathLab:IntrinsicUnavailable", "Qualified primitives were not installed before project entry");
  return value;
}

static octave_value bindings (octave::interpreter& interp) {
  return initialized (interp).carrier;
}

static octave_value make_carrier (const runtime_state& value) {
  octave_scalar_map result, adapters;
  result.assign ("dispatcher", octave_value (new octave_fcn_handle (value.dispatcher)));
  // Fmlock skips its own frame. Calling it through Fbuiltin would select that
  // dispatcher as its caller instead of the private helper that owns bindings.
  result.assign ("caller_lock", octave_value (new octave_fcn_handle (value.caller_lock)));
  result.assign ("bridge", octave_value (new octave_fcn_handle (value.provider)));
  for (const auto& slot : value.adapters) {
    octave_scalar_map entry;
    entry.assign ("callback", octave_value (new octave_fcn_handle (slot.function)));
    entry.assign ("path", slot.path.string ());
    entry.assign ("protocol", slot.spec->protocol);
    entry.assign ("version", std::to_string (slot.spec->version));
    adapters.assign (slot.spec->role, entry);
  }
  result.assign ("adapters", adapters);
  return octave_value (result);
}

static octave_value_list base_query (octave::interpreter& interp,
                                     const octave_value_list& args, bool metadata) {
  auto& held = initialized (interp);
  if (args.length () != (metadata ? 2 : 1))
    error_with_id ("MathLab:IntrinsicArguments", "Invalid base-query arguments");
  octave_value_list query;
  if (metadata) {
    const auto name = text (args(1), 63, "Variable name");
    if (! identifier (name))
      error_with_id ("MathLab:IntrinsicArguments", "Expected an ASCII variable identifier");
    query(0) = name;
  }
  auto& evaluator = interp.get_evaluator ();
  const auto frame = evaluator.current_call_stack_frame_number ();
  octave::unwind_action restore ([&evaluator, frame] () { evaluator.restore_frame (frame); });
  evaluator.goto_base_frame ();
  if (metadata) {
    const auto value = evaluator.varval (query(0).string_value ());
    // whos may ask an object for overloaded size. Restrict BEFORE calling it.
    // Containers/objects retain the existing unknown-bytes metadata policy.
    if (! value.is_defined () || value.isobject ()
        || ! (value.isnumeric () || value.islogical () || value.is_string ()))
      error_with_id ("MathLab:IntrinsicUnsupported", "Byte accounting requires a genuine primitive value");
  }
  const auto function = metadata ? held.whos : held.who;
  return function.function_value ()->call (evaluator, 1, query);
}

static octave_value base_value (octave::interpreter& interp, const octave_value_list& args) {
  initialized (interp);
  if (args.length () != 2)
    error_with_id ("MathLab:IntrinsicArguments", "Base value takes one variable name");
  const auto name = text (args(1), 63, "Variable name");
  if (! identifier (name))
    error_with_id ("MathLab:IntrinsicArguments", "Expected an ASCII variable identifier");
  auto& evaluator = interp.get_evaluator ();
  const auto frame = evaluator.current_call_stack_frame_number ();
  octave::unwind_action restore ([&evaluator, frame] () { evaluator.restore_frame (frame); });
  evaluator.goto_base_frame ();
  // varval reads the variable slot. It must NOT fall back to invoking a same-
  // named function when the variable is absent, unlike evaluating its name.
  auto value = evaluator.varval (name);
  if (! value.is_defined ())
    error_with_id ("MathLab:VariableNotFound", "Variable does not exist in the workspace: %s", name.c_str ());
  return value;
}

static std::vector<fs::path> startup_path (octave::interpreter& interp,
                                           const fs::path& session,
                                           const octave_value& encoded) {
  if (! encoded.iscell () || encoded.isobject () || encoded.issparse ()
      || encoded.numel () > 512)
    error_with_id ("MathLab:IntrinsicArguments", "Startup path admission needs a bounded directory allowlist");
  std::error_code ec;
  if (fs::current_path (ec) != session || ec)
    error_with_id ("MathLab:IntrinsicIdentity", "Bootstrap must run in the private session directory");
  std::vector<fs::path> configured;
  std::vector<fs::path> allowed {session};
  const auto cells = encoded.cell_value ();
  std::size_t configured_bytes = 0;
  for (octave_idx_type index = 0; index < cells.numel (); ++index) {
    const auto bytes = path_text (cells(index), "Runtime directory");
    configured_bytes += bytes.size () + 1;
    if (configured_bytes > 65536)
      error_with_id ("MathLab:IntrinsicArguments", "Startup path exceeds its byte budget");
    const fs::path input (bytes);
    const auto canonical = fs::canonical (input, ec);
    if (! input.is_absolute () || ec || canonical != input
        || ! fs::is_directory (canonical, ec) || ec)
      error_with_id ("MathLab:IntrinsicIdentity", "Runtime directories must be canonical and available");
    configured.push_back (canonical);
    allowed.push_back (canonical);
  }
  const auto actual = interp.get_load_path ().dir_list ();
  if (actual.size () > 512)
    error_with_id ("MathLab:IntrinsicIdentity", "Startup function path exceeds its admission budget");
  std::size_t actual_bytes = 0;
  for (const auto& entry : actual) {
    actual_bytes += entry.size () + 1;
    if (actual_bytes > 65536 || entry.size () > 4096 || ! exact_utf8 (entry)
        || entry.find_first_of ("\0\r\n", 0, 3) != std::string::npos)
      error_with_id ("MathLab:IntrinsicIdentity", "Startup function path has invalid or excessive bytes");
    const auto canonical = fs::canonical (entry.empty () ? fs::path (".") : fs::path (entry), ec);
    if (ec)
      error_with_id ("MathLab:IntrinsicIdentity", "A startup function directory is unavailable");
    bool admitted = false;
    for (const auto& candidate : allowed) admitted = admitted || canonical == candidate;
    if (! admitted)
      error_with_id ("MathLab:IntrinsicIdentity", "Startup function path contains an unqualified directory");
  }
  return configured;
}

static std::vector<adapter_slot> adapter_manifest (const octave_value& input) {
  if (! input.iscell () || input.isobject () || input.issparse () || input.numel () > 5)
    error_with_id ("MathLab:IntrinsicArguments", "Adapter manifest must be a bounded genuine cell array");
  const auto cells = input.cell_value ();
  std::vector<adapter_slot> result;
  for (octave_idx_type index = 0; index < cells.numel (); ++index) {
    const auto record = cells(index);
    if (! record.isstruct () || record.isobject () || record.numel () != 1 || record.issparse ())
      error_with_id ("MathLab:IntrinsicArguments", "Adapter manifest entries must be plain scalar structs");
    const auto fields = record.scalar_map_value ();
    if (fields.nfields () != 2 || ! fields.contains ("role") || ! fields.contains ("path"))
      error_with_id ("MathLab:IntrinsicArguments", "Adapter manifest entry fields are invalid");
    const auto role = text (fields.getfield ("role"), 63, "Adapter role");
    const role_spec *spec = nullptr;
    for (const auto& candidate : roles)
      if (role == candidate.role) spec = &candidate;
    if (! spec)
      error_with_id ("MathLab:IntrinsicArguments", "Adapter role is not configured by this provider");
    for (const auto& existing : result)
      if (existing.spec == spec)
        error_with_id ("MathLab:IntrinsicArguments", "Adapter roles must be unique");
    const auto path = canonical_file (fields.getfield ("path"), "Adapter file");
    if (path.extension () != ".oct")
      error_with_id ("MathLab:IntrinsicIdentity", "Adapter file must be a selected native module");
    adapter_slot slot;
    slot.spec = spec; slot.path = path;
    result.push_back (slot);
  }
  return result;
}

static void unoccupied_alias (octave::interpreter& interp, const std::string& alias) {
  auto& symbols = interp.get_symbol_table ();
  if (symbols.find_cmdline_function (alias).is_defined ()
      || symbols.find_user_function (alias).is_defined ()
      || symbols.find_built_in_function (alias).is_defined ())
    error_with_id ("MathLab:IntrinsicIdentity", "A reserved native alias is already defined");
}

static octave_value_list shutdown_proxy (octave::interpreter& interp,
                                         const octave_value_list& args, int nargout) {
  auto& held = state ();
  if (args.length () != 0 || nargout != 0 || held.owner != &interp
      || ! held.shutdown_registered)
    error_with_id ("MathLab:IntrinsicShutdown", "Invalid native shutdown invocation");
  if (held.shutdown_started) return octave_value_list ();
  held.shutdown_started = true;
  for (const auto& slot : held.adapters) {
    const std::string role (slot.spec->role);
    if (role != "debug" && role != "graphics") continue;
    try {
      // Both cleanup calls use the actual retained adapter while the owning
      // interpreter is alive. Graphics references may own user callbacks.
      interp.feval (slot.function.function_value (), octave_value_list (), 0);
    } catch (...) {
      std::fputs (role == "debug"
        ? "MathLab:IntrinsicShutdown: retained debugger cleanup failed; cleanup will not be retried.\n"
        : "MathLab:IntrinsicShutdown: retained graphics cleanup failed; cleanup will not be retried.\n", stderr);
    }
  }
  return octave_value_list ();
}

static void register_shutdown (octave::interpreter& interp, runtime_state& held) {
  unoccupied_alias (interp, held.shutdown_alias);
  octave_value proxy (new octave_builtin (
    static_cast<octave_builtin::meth> (&shutdown_proxy), held.shutdown_alias,
    "Run retained native adapter cleanup once"));
  proxy.function_value ()->lock ();
  held.shutdown_proxy = proxy;
  auto& symbols = interp.get_symbol_table ();
  symbols.install_cmdline_function (held.shutdown_alias, proxy);
  symbols.install_user_function (held.shutdown_alias, proxy);
  if (symbols.find_cmdline_function (held.shutdown_alias).function_value () != proxy.function_value ()
      || symbols.find_user_function (held.shutdown_alias).function_value () != proxy.function_value ())
    error_with_id ("MathLab:IntrinsicIdentity", "Shutdown proxy publication changed object identity");
  interp.add_atexit_fcn (held.shutdown_alias);
  held.shutdown_registered = true;
}

static bool registered_shutdown (octave::interpreter& interp, const runtime_state& held) {
  if (! held.shutdown_registered || ! held.shutdown_proxy.is_builtin_function ()) return false;
  auto& symbols = interp.get_symbol_table ();
  const auto alias = symbols.find_cmdline_function (held.shutdown_alias);
  const auto path_alias = symbols.find_user_function (held.shutdown_alias);
  return alias.is_builtin_function () && path_alias.is_builtin_function ()
    && alias.function_value () == held.shutdown_proxy.function_value ()
    && path_alias.function_value () == held.shutdown_proxy.function_value ()
    && alias.function_value ()->islocked () && held.provider.function_value ()->islocked ();
}

static octave_value binding_context (octave::interpreter& interp,
                                      const octave_value_list& args, int nargout) {
  const auto& held = state ();
  if (args.length () != 2 || nargout != 1 || held.owner != &interp
      || held.phase != bootstrap_phase::binding || ! registered_shutdown (interp, held)
      || held.shutdown_started)
    error_with_id ("MathLab:IntrinsicUnavailable", "Binding context is unavailable outside staged native binding");
  const auto role = text (args(1), 63, "Adapter role");
  for (const auto& slot : held.adapters) {
    if (role != slot.spec->role) continue;
    octave_scalar_map result;
    result.assign ("schema", "2");
    result.assign ("role", role);
    result.assign ("canonical_path", slot.path.string ());
    result.assign ("shutdown_proxy_registered", true);
    result.assign ("phase", "binding");
    return octave_value (result);
  }
  error_with_id ("MathLab:IntrinsicArguments", "Binding context requires a configured fixed role");
  return octave_value ();
}

static void required_number (const octave_scalar_map& metadata, const char *field,
                             double expected) {
  const auto value = metadata.getfield (field);
  if (! value.isnumeric () || value.isobject () || value.issparse ()
      || ! value.isreal () || value.numel () != 1 || value.double_value () != expected)
    error_with_id ("MathLab:IntrinsicProtocol", "Adapter protocol field %s has the wrong numeric value", field);
}

static void validate_protocol (octave::interpreter& interp, const adapter_slot& slot) {
  octave_value_list query;
  query(0) = "protocol";
  if (slot.spec->protocol_zero) query(1) = 0.0;
  const auto output = interp.feval (slot.function.function_value (), query, 1);
  if (output.length () != 1 || ! output(0).isstruct () || output(0).isobject ()
      || output(0).numel () != 1 || output(0).issparse ())
    error_with_id ("MathLab:IntrinsicProtocol", "Adapter protocol must be one plain scalar struct");
  const auto metadata = output(0).scalar_map_value ();
  if (text (metadata.getfield ("protocol"), 63, "Adapter protocol") != slot.spec->protocol)
    error_with_id ("MathLab:IntrinsicProtocol", "Adapter protocol identity is invalid");
  required_number (metadata, "version", slot.spec->version);
  for (const auto *flag : slot.spec->flags) {
    if (! flag) break;
    const auto value = metadata.getfield (flag);
    if (! value.islogical () || value.isobject () || value.issparse ()
        || value.numel () != 1 || ! value.bool_value ())
      error_with_id ("MathLab:IntrinsicProtocol", "Adapter protocol flag %s must be true", flag);
  }
  const auto role = std::string (slot.spec->role);
  if (role == "value") {
    required_number (metadata, "max_cells", 2000);
    required_number (metadata, "max_rank", 32);
    required_number (metadata, "max_replace_leaf_bytes", 134217728);
  } else if (role == "graphics") {
    required_number (metadata, "max_entries", 4096);
    if (text (metadata.getfield ("octave_version"), 256, "Graphics Octave version") != OCTAVE_VERSION
        || text (metadata.getfield ("octave_api_version"), 256, "Graphics Octave API") != OCTAVE_API_VERSION)
      error_with_id ("MathLab:IntrinsicProtocol", "Graphics installation identity does not match the provider");
  }
}

static octave_value parse_owned_function (octave::interpreter& interp,
                                          const fs::path& file, const char *name) {
  auto parsed = octave::parse_fcn_file (interp, file.string (), name,
    file.parent_path ().string (), "", "", true, false, false, false);
  std::error_code ec;
  const auto actual = parsed.is_user_function ()
    ? fs::canonical (parsed.user_code_value ()->scope ().fcn_file_name (), ec) : fs::path ();
  if (! parsed.is_user_function () || ec || actual != file || parsed.function_value ()->name () != name)
    error_with_id ("MathLab:IntrinsicIdentity", "An owned source function has the wrong identity");
  return parsed;
}

static octave_value_list bootstrap_v2 (octave::interpreter& interp,
                                       const octave_value_list& args, int nargout) {
  if (args.length () != 8)
    error_with_id ("MathLab:IntrinsicArguments", "Bootstrap V2 takes seven owned configuration arguments");
  if (octave::interpreter::the_interpreter () != &interp)
    error_with_id ("MathLab:IntrinsicIdentity", "Bootstrap requires the selected active interpreter");
  const auto module = canonical_file (args(1), "Provider file");
  const auto worker = canonical_file (args(2), "Worker file");
  const auto registry = canonical_file (args(3), "Registry file");
  const auto alias = text (args(4), 63, "Entry alias");
  if (module.extension () != ".oct" || worker.filename () != "mathlab_worker.m"
      || registry != worker.parent_path () / "registry/mathlab_scope_register.m"
      || ! identifier (alias) || alias.size () <= 10 || alias.compare (0, 10, "__mlentry_") != 0)
    error_with_id ("MathLab:IntrinsicIdentity", "Invalid owned-source or reserved-entry identity");
  const std::string shutdown_alias = "__mlshutdown_" + alias.substr (10);
  if (! identifier (shutdown_alias))
    error_with_id ("MathLab:IntrinsicIdentity", "Shutdown alias exceeds its reserved identity budget");
  if (! args(5).iscell () || args(5).isobject () || args(5).issparse () || args(5).numel () > 256)
    error_with_id ("MathLab:IntrinsicArguments", "Required builtin names must be a bounded genuine cell array");
  std::vector<std::string> required_names;
  const auto required = args(5).cell_value ();
  for (octave_idx_type index = 0; index < required.numel (); ++index) {
    const auto name = text (required(index), 63, "Builtin name");
    if (! identifier (name))
      error_with_id ("MathLab:IntrinsicArguments", "Required builtin name must be an ASCII identifier");
    actual_builtin (interp, name);
    required_names.push_back (name);
  }
  const auto directories = startup_path (interp, worker.parent_path (), args(6));
  auto adapters = adapter_manifest (args(7));
  auto *owner = dynamic_cast<octave_dld_function *> (interp.get_evaluator ().current_function ());
  if (! owner)
    error_with_id ("MathLab:IntrinsicIdentity", "Bootstrap requires the selected actual provider DLD");
  // The existing self-reference seam is separate from the owning adapter loader.
  octave_value self (owner, true);
  verified_dld (self, module, "mathlab_intrinsic_bridge");
  auto dispatcher = actual_builtin (interp, "builtin");
  auto caller_lock = actual_builtin (interp, "mlock");
  auto who = actual_builtin (interp, "who");
  auto whos = actual_builtin (interp, "whos");
  auto& held = state ();
  if (held.phase == bootstrap_phase::ready) {
    bool same = held.owner == &interp && held.module == module && held.worker == worker
      && held.registry == registry && held.entry_alias == alias && held.shutdown_alias == shutdown_alias
      && held.required_names == required_names && held.runtime_directories == directories
      && held.adapters.size () == adapters.size () && ! held.shutdown_started
      && held.provider.function_value () == self.function_value ()
      && held.dispatcher.function_value () == dispatcher.function_value ()
      && held.caller_lock.function_value () == caller_lock.function_value ()
      && held.who.function_value () == who.function_value () && held.whos.function_value () == whos.function_value ();
    for (std::size_t index = 0; same && index < adapters.size (); ++index) {
      same = held.adapters[index].spec == adapters[index].spec && held.adapters[index].path == adapters[index].path;
      if (same) verified_dld (held.adapters[index].function, adapters[index].path, adapters[index].spec->export_name);
    }
    auto& symbols = interp.get_symbol_table ();
    if (! same || ! registered_shutdown (interp, held)
        || symbols.find_cmdline_function (held.entry_alias).function_value () != held.provider.function_value ()
        || symbols.find_user_function (held.entry_alias).function_value () != held.provider.function_value ()
        || symbols.find_cmdline_function (held.shutdown_alias).function_value () != held.shutdown_proxy.function_value ())
      error_with_id ("MathLab:IntrinsicIdentity", "Restart to replace or recover the startup intrinsic profile");
    return nargout ? ovl (bindings (interp)) : octave_value_list ();
  }
  if (held.phase != bootstrap_phase::empty || held.owner)
    error_with_id ("MathLab:IntrinsicUnavailable", "Partial bootstrap requires a fresh process");
  unoccupied_alias (interp, alias);
  unoccupied_alias (interp, shutdown_alias);
  for (auto& slot : adapters) {
    if (octave::interpreter::the_interpreter () != &interp)
      error_with_id ("MathLab:IntrinsicIdentity", "Native callback load changed active interpreter");
    slot.function = octave::load_fcn_from_file (slot.path.string (),
      slot.path.parent_path ().string (), "", "", slot.spec->export_name, false);
    verified_dld (slot.function, slot.path, slot.spec->export_name);
  }
  held.owner = &interp; held.module = module; held.worker = worker;
  held.registry = registry; held.entry_alias = alias; held.shutdown_alias = shutdown_alias;
  held.required_names = required_names; held.runtime_directories = directories;
  held.adapters = adapters; held.dispatcher = dispatcher; held.caller_lock = caller_lock;
  held.provider = self;
  held.who = who; held.whos = whos; held.initial_path_admitted = true;
  held.phase = bootstrap_phase::binding;
  owner->lock ();
  for (const auto& slot : held.adapters) slot.function.function_value ()->lock ();
  register_shutdown (interp, held);
  for (const auto& slot : held.adapters) {
    octave_value_list bind;
    bind(0) = "bind_intrinsics";
    bind(1) = held.dispatcher; bind(2) = held.provider;
    const auto output = interp.feval (slot.function.function_value (), bind, 0);
    if (output.length () != 0)
      error_with_id ("MathLab:IntrinsicProtocol", "Private adapter bind returned unexpected output");
  }
  for (const auto& slot : held.adapters) validate_protocol (interp, slot);
  held.worker_function = parse_owned_function (interp, worker, "mathlab_worker");
  // Validation owns this registry reference only until this block returns.
  { const auto parsed_registry = parse_owned_function (interp, registry, "mathlab_scope_register"); }
  if (held.shutdown_started || ! registered_shutdown (interp, held))
    error_with_id ("MathLab:IntrinsicUnavailable", "Bootstrap lost its registered native shutdown proxy");
  held.carrier = make_carrier (held);
  unoccupied_alias (interp, alias);
  auto& symbols = interp.get_symbol_table ();
  symbols.install_cmdline_function (alias, self);
  symbols.install_user_function (alias, self); // Literal @alias uses this slot.
  if (symbols.find_cmdline_function (alias).function_value () != self.function_value ()
      || symbols.find_user_function (alias).function_value () != self.function_value ())
    error_with_id ("MathLab:IntrinsicIdentity", "Entry publication changed provider object identity");
  held.phase = bootstrap_phase::ready;
  return nargout ? ovl (bindings (interp)) : octave_value_list ();
}

static octave_value status (octave::interpreter& interp,
                            const octave_value_list& args, int nargout) {
  const auto& held = initialized (interp);
  if (args.length () != 1 || nargout != 1)
    error_with_id ("MathLab:IntrinsicArguments", "Status takes no arguments and requires one output");
  octave_scalar_map result, adapters;
  result.assign ("protocol", "mathlab-intrinsics");
  result.assign ("version", "1");
  result.assign ("phase", "ready");
  result.assign ("provider_file", held.module.string ());
  result.assign ("worker_file", held.worker.string ());
  result.assign ("registry_file", held.registry.string ());
  result.assign ("entry_alias", held.entry_alias);
  result.assign ("octave_version", OCTAVE_VERSION);
  result.assign ("octave_api_version", OCTAVE_API_VERSION);
  result.assign ("initial_path_admitted", held.initial_path_admitted);
  result.assign ("shutdown_alias", held.shutdown_alias);
  result.assign ("shutdown_registered", held.shutdown_registered);
  for (const auto& slot : held.adapters) {
    octave_scalar_map entry;
    entry.assign ("path", slot.path.string ());
    entry.assign ("protocol", slot.spec->protocol);
    entry.assign ("version", std::to_string (slot.spec->version));
    adapters.assign (slot.spec->role, entry);
  }
  result.assign ("adapters", adapters);
  return octave_value (result);
}

static octave_value bind_registry (octave::interpreter& interp,
                                   const octave_value *requested = nullptr) {
  auto& held = initialized (interp);
  if (requested && canonical_file (*requested, "Requested registry file") != held.registry)
    error_with_id ("MathLab:IntrinsicIdentity", "Requested registry differs from the admitted numerical registry");
  static constexpr const char *registry_primitives[] = {
    "nargin", "nargout", "ischar", "strcmp", "find", "isempty", "logical", "numel", "isfield", "error"
  };
  for (const auto *primitive : registry_primitives) actual_builtin (interp, primitive);
  auto& symbols = interp.get_symbol_table ();
  const std::string name = "mathlab_scope_register";
  auto registry = symbols.find_cmdline_function (name);
  // clear functions removes this unlocked registry. Reparse the admitted file
  // directly; ordinary path lookup may select a same-named project function.
  if (registry.is_undefined ())
    registry = octave::parse_fcn_file (interp, held.registry.string (), name,
      held.registry.parent_path ().string (), "", "", true, false, false, false);
  std::error_code ec;
  const auto actual = registry.is_user_function ()
    ? fs::canonical (registry.user_code_value ()->scope ().fcn_file_name (), ec) : fs::path ();
  if (! registry.is_user_function () || ec || actual != held.registry
      || registry.function_value ()->name () != name)
    error_with_id ("MathLab:IntrinsicIdentity", "The exact numerical registry was replaced");
  // Same public per-file binding seam used by mathlab_source_context.h.
  // This is a function object, not a name-resolving handle or base variable.
  symbols.install_local_function ("builtin", held.dispatcher, held.registry.string ());
  symbols.install_local_function ("__ml_registry_provider__", held.provider, held.registry.string ());
  for (const auto& slot : held.adapters)
    if (std::string (slot.spec->role) == "scope" || std::string (slot.spec->role) == "debug")
      symbols.install_local_function (slot.spec->export_name, slot.function, held.registry.string ());
  symbols.install_cmdline_function (name, registry);
  symbols.install_user_function (name, octave_value ());
  // NO lock and NO native owning cache of registry/local numerical functions.
  // clear functions/all must release its keys/scopes exactly as before.
  return octave_value (new octave_fcn_handle (registry));
}

// This core is included by the single standalone provider only.
static bool dispatch (octave::interpreter& interp, const octave_value_list& args,
                      int nargout, octave_value_list& result) {
  if (args.length () < 1 || ! args(0).is_string () || args(0).isobject ()
      || args(0).rows () != 1 || args(0).numel () < 1 || args(0).numel () > 63)
    return false; // Preserve the host adapter's own malformed-command errors.
  const auto command = args(0).string_value (); // <=63 primitive char bytes
  if (command.compare (0, 10, "intrinsic_") != 0) return false;
  text (args(0), 63, "Operation"); // intrinsic-only NUL/byte validation
  if (nargout < 0 || nargout > 1) {
    if (command == "intrinsic_bootstrap_v2") state ().phase = bootstrap_phase::failed;
    error_with_id ("MathLab:IntrinsicArguments", "Intrinsic operations return at most one result");
  }
  if (state ().phase == bootstrap_phase::failed)
    error_with_id ("MathLab:IntrinsicUnavailable", "Failed bootstrap requires a fresh process");
  if (state ().phase == bootstrap_phase::binding
      && command != "intrinsic_protocol" && command != "intrinsic_binding_context_v2")
    error_with_id ("MathLab:IntrinsicUnavailable", "Operation is unavailable during staged native binding");
  if (command == "intrinsic_protocol") {
    if (args.length () != 1) error_with_id ("MathLab:IntrinsicArguments", "Protocol takes no arguments");
    octave_scalar_map protocol;
    protocol.assign ("protocol", "mathlab-intrinsics");
    protocol.assign ("version", 1);
    protocol.assign ("octave_version", OCTAVE_VERSION);
    protocol.assign ("octave_api_version", OCTAVE_API_VERSION);
    protocol.assign ("actual_function_object", true);
    protocol.assign ("native_base_queries", true);
    protocol.assign ("unlocked_registry", true);
    protocol.assign ("callback_binding_v2", true);
    protocol.assign ("adapter_carrier_v2", true);
    protocol.assign ("provider_shutdown_proxy", true);
    result = ovl (protocol);
    return true;
  }
  if (command == "intrinsic_bootstrap_v2") {
    try { result = bootstrap_v2 (interp, args, nargout); }
    catch (...) { state ().phase = bootstrap_phase::failed; throw; }
    return true;
  }
  if (command == "intrinsic_binding_context_v2") {
    result = ovl (binding_context (interp, args, nargout)); return true;
  }
  if (command == "intrinsic_status") {
    result = ovl (status (interp, args, nargout)); return true;
  }
  if (command == "intrinsic_bindings") {
    if (args.length () != 1) error_with_id ("MathLab:IntrinsicArguments", "Bindings takes no arguments");
    result = ovl (bindings (interp)); return true;
  }
  if (command == "intrinsic_base_names" || command == "intrinsic_base_metadata") {
    result = base_query (interp, args, command == "intrinsic_base_metadata"); return true;
  }
  if (command == "intrinsic_base_value") {
    result = ovl (base_value (interp, args)); return true;
  }
  if (command == "intrinsic_bind_registry") {
    if (args.length () != 1 && args.length () != 2)
      error_with_id ("MathLab:IntrinsicArguments", "Registry binding takes an optional exact registry path");
    auto registry = bind_registry (interp, args.length () == 2 ? &args(1) : nullptr);
    result = nargout ? ovl (registry) : octave_value_list (); return true;
  }
  if (command == "intrinsic_path_contains") {
    initialized (interp);
    if (args.length () != 2) error_with_id ("MathLab:IntrinsicArguments", "Path membership takes one directory");
    result = ovl (interp.get_load_path ().contains_canonical (text (args(1), 4096, "Directory")));
    return true;
  }
  if (command == "intrinsic_run") {
    auto& held = initialized (interp);
    // Ten current worker arguments + initial directory; the actual native
    // bindings are inserted as arg11. NO name lookup of addpath/mathlab_worker.
    if (args.length () != 2 || ! args(1).iscell () || args(1).isobject ()
        || args(1).numel () != 11 || nargout != 0)
      error_with_id ("MathLab:IntrinsicArguments", "Run takes eleven owned worker arguments and has no output");
    const Cell encoded = args(1).cell_value ();
    if (canonical_file (octave_value ((held.worker.parent_path () / "mathlab_worker.m").string ()), "Worker file") != held.worker
        || text (encoded(0), 4096, "Session directory") != held.worker.parent_path ().string ())
      error_with_id ("MathLab:IntrinsicIdentity", "Run targets a different private session");
    octave_value_list arguments;
    for (octave_idx_type index = 0; index < 10; ++index) arguments(index) = encoded(index);
    arguments(10) = bindings (interp);
    arguments(11) = encoded(10); // initial cwd, empty on same-PID re-entry
    bind_registry (interp);
    auto& evaluator = interp.get_evaluator ();
    const auto frame = evaluator.current_call_stack_frame_number ();
    octave::unwind_action restore ([&evaluator, frame] () { evaluator.restore_frame (frame); });
    evaluator.goto_base_frame ();
    held.worker_function.function_value ()->call (evaluator, 0, arguments);
    result = octave_value_list (); return true;
  }
  error_with_id ("MathLab:IntrinsicArguments", "Unknown intrinsic operation");
  return true;
}
} // namespace mathlab_intrinsic_v2_proposal
#endif
