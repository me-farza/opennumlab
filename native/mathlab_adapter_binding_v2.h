// SPDX-License-Identifier: GPL-3.0-or-later
// PRIVATE SOURCE CANDIDATE: transport configuration only, not runtime-qualified.
// No provider core, numerical registry, source AST or user value is retained here.
#ifndef MATHLAB_ADAPTER_BINDING_V2_H
#define MATHLAB_ADAPTER_BINDING_V2_H

#include <octave/oct.h>
#include <octave/error.h>
#include <octave/interpreter.h>
#include <octave/ov-builtin.h>
#include <octave/ov-dld-fcn.h>
#include <octave/ov-fcn-handle.h>
#include <octave/ov-fcn.h>
#include <octave/ov-usr-fcn.h>
#include <octave/pt-eval.h>
#include <octave/symtab.h>
#include <octave/version.h>
#include <filesystem>
#include <initializer_list>
#include <map>
#include <string>
#include <string_view>
#include <utility>

namespace mathlab_adapter_binding_v2 {
namespace fs = std::filesystem;

static_assert (std::string_view (OCTAVE_VERSION) == "11.3.0"
               && std::string_view (OCTAVE_API_VERSION) == "api-v61",
               "Private adapter binding V2 requires Octave 11.3.0/api-v61 headers");

struct binding {
  octave::interpreter *owner = nullptr;
  octave_function *adapter = nullptr; // Identity only; provider owns the adapter.
  octave_value dispatcher, provider;
  fs::path adapter_path;
  std::map<std::string, octave_value> primitives;
  bool shutdown_proxy_registered = false;
};

inline bool exact_utf8 (const std::string& value) {
  for (std::size_t i = 0; i < value.size ();) {
    const auto first = static_cast<unsigned char> (value[i++]);
    if (first < 0x80) continue;
    unsigned remaining = 0, code = 0, minimum = 0;
    if (first >= 0xc2 && first <= 0xdf) { remaining = 1; code = first & 0x1f; minimum = 0x80; }
    else if (first >= 0xe0 && first <= 0xef) { remaining = 2; code = first & 0x0f; minimum = 0x800; }
    else if (first >= 0xf0 && first <= 0xf4) { remaining = 3; code = first & 0x07; minimum = 0x10000; }
    else return false;
    if (value.size () - i < remaining) return false;
    while (remaining--) {
      const auto next = static_cast<unsigned char> (value[i++]);
      if ((next & 0xc0) != 0x80) return false;
      code = (code << 6) | (next & 0x3f);
    }
    if (code < minimum || code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff)) return false;
  }
  return true;
}

inline std::string text (const octave_value& input, std::size_t limit) {
  if (! input.is_string () || input.isobject () || input.ndims () != 2
      || input.rows () != 1 || input.numel () < 1
      || static_cast<std::size_t> (input.numel ()) > limit)
    error_with_id ("MathLab:AdapterBindingArguments", "Expected a bounded plain character row");
  const auto value = input.string_value ();
  if (value.empty () || value.size () > limit || value.find ('\0') != std::string::npos
      || value.find ('\r') != std::string::npos || value.find ('\n') != std::string::npos
      || ! exact_utf8 (value))
    error_with_id ("MathLab:AdapterBindingArguments", "Invalid native binding text bytes");
  return value;
}

inline fs::path module_path (const std::string& bytes, bool exact) {
  const auto admitted = text (octave_value (bytes), 4096);
  const fs::path requested (admitted);
  std::error_code ec;
  const auto canonical = fs::canonical (requested, ec);
  if (! requested.is_absolute () || ec || canonical.extension () != ".oct"
      || ! fs::is_regular_file (canonical, ec) || ec
      || (exact && canonical.string () != admitted))
    error_with_id ("MathLab:AdapterBindingIdentity", "Native binding module path is invalid");
  return canonical;
}

inline fs::path dld_path (octave_function *function, const char *export_name) {
  auto *actual = dynamic_cast<octave_dld_function *> (function);
  if (! actual || ! actual->is_dld_function () || actual->name () != export_name)
    error_with_id ("MathLab:AdapterBindingIdentity", "Expected the fixed actual native function object");
  const auto file = module_path (actual->fcn_file_name (), false);
  const auto library = actual->get_shlib ();
  if (! library || module_path (library.file_name (), false) != file)
    error_with_id ("MathLab:AdapterBindingIdentity", "Actual native library identity is invalid");
  return file;
}

inline octave_value builtin (octave::interpreter& interp, const char *name) {
  const auto value = interp.get_symbol_table ().find_built_in_function (name);
  auto *actual = value.is_function () && value.is_builtin_function ()
    && ! value.is_function_handle () && ! value.is_dld_function ()
    ? dynamic_cast<octave_builtin *> (value.function_value ()) : nullptr;
  if (! actual)
    error_with_id ("MathLab:AdapterBindingIdentity", "Required genuine builtin %s is unavailable", name);
  return value;
}

inline bool active (const binding& held, octave::interpreter& interp) {
  if (! held.owner) return false;
  if (held.owner != &interp || octave::interpreter::the_interpreter () != &interp)
    error_with_id ("MathLab:AdapterBindingIdentity", "Native binding belongs to another interpreter");
  return true;
}

inline bool is_bind_operation (const octave_value_list& args) {
  return args.length () > 0 && args(0).is_string () && ! args(0).isobject ()
    && args(0).ndims () == 2 && args(0).rows () == 1 && args(0).numel () == 15
    && args(0).string_value () == "bind_intrinsics";
}

inline void bind (binding& held, octave::interpreter& interp,
                  const octave_value_list& args, int nargout,
                  const char *role, const char *export_name,
                  std::initializer_list<const char *> primitive_names = {}) {
  if (args.length () != 3 || nargout != 0)
    error_with_id ("MathLab:AdapterBindingArguments", "bind_intrinsics takes two raw owning function values and no outputs");
  if (octave::interpreter::the_interpreter () != &interp)
    error_with_id ("MathLab:AdapterBindingIdentity", "Native binding requires the active interpreter");
  const auto dispatcher = builtin (interp, "builtin");
  if (! args(1).is_function () || ! args(1).is_builtin_function ()
      || args(1).is_function_handle () || args(1).is_dld_function ()
      || args(1).function_value () != dispatcher.function_value ())
    error_with_id ("MathLab:AdapterBindingIdentity", "Expected the actual genuine builtin dispatcher object");
  if (! args(2).is_function () || ! args(2).is_dld_function () || args(2).is_function_handle ())
    error_with_id ("MathLab:AdapterBindingIdentity", "Expected the actual raw native provider object");
  auto *provider = args(2).function_value ();
  dld_path (provider, "mathlab_intrinsic_bridge");
  auto *adapter = interp.get_evaluator ().current_function ();
  const auto adapter_file = dld_path (adapter, export_name);
  if (held.owner) {
    active (held, interp);
    if (held.adapter != adapter || held.adapter_path != adapter_file
        || held.dispatcher.function_value () != args(1).function_value ()
        || held.provider.function_value () != provider || ! held.shutdown_proxy_registered)
      error_with_id ("MathLab:AdapterBindingIdentity", "Restart the process to replace native binding objects");
    return; // Exact saved objects only; never re-enter the provider's staged context.
  }

  const auto context_result = interp.feval (provider,
    ovl ("intrinsic_binding_context_v2", role), 1);
  if (context_result.length () != 1 || ! context_result(0).isstruct ()
      || context_result(0).isobject () || context_result(0).numel () != 1)
    error_with_id ("MathLab:AdapterBindingIdentity", "Provider returned an invalid binding context");
  const auto context = context_result(0).scalar_map_value ();
  if (context.nfields () != 5 || ! context.contains ("schema") || ! context.contains ("role")
      || ! context.contains ("canonical_path") || ! context.contains ("shutdown_proxy_registered")
      || ! context.contains ("phase"))
    error_with_id ("MathLab:AdapterBindingIdentity", "Provider binding context fields differ");
  const auto shutdown = context.getfield ("shutdown_proxy_registered");
  if (text (context.getfield ("schema"), 1) != "2"
      || text (context.getfield ("role"), 63) != role
      || text (context.getfield ("phase"), 7) != "binding"
      || module_path (text (context.getfield ("canonical_path"), 4096), true) != adapter_file
      || ! shutdown.islogical () || shutdown.isobject () || shutdown.issparse ()
      || ! shutdown.is_scalar_type () || ! shutdown.bool_value ())
    error_with_id ("MathLab:AdapterBindingIdentity", "Provider binding context identity or shutdown proxy is invalid");
  if (interp.get_evaluator ().current_function () != adapter)
    error_with_id ("MathLab:AdapterBindingIdentity", "Provider call changed the actual adapter frame");

  binding candidate;
  candidate.owner = &interp; candidate.adapter = adapter;
  candidate.dispatcher = args(1); candidate.provider = args(2);
  candidate.adapter_path = adapter_file; candidate.shutdown_proxy_registered = true;
  for (const auto *name : primitive_names) {
    if (! candidate.primitives.emplace (name, builtin (interp, name)).second)
      error_with_id ("MathLab:AdapterBindingArguments", "Duplicate fixed native control primitive");
  }
  held = std::move (candidate); // Publish transport configuration only after every check.
}

inline octave_value_list call_primitive (const binding& held, octave::interpreter& interp,
                                         const char *name, const octave_value_list& args,
                                         int nargout) {
  if (! active (held, interp))
    return interp.feval (name, args, nargout); // Existing unqualified legacy path.
  const auto target = held.primitives.find (name);
  if (target == held.primitives.end ())
    error_with_id ("MathLab:AdapterBindingUnavailable", "Qualified control primitive was not captured");
  // Normal actual-function invocation preserves the genuine callee frame.
  // Caller-sensitive runtime behavior still requires new qualification.
  return interp.feval (target->second.function_value (), args, nargout);
}

inline fs::path registry_path (const octave_value& input) {
  const auto bytes = text (input, 4096);
  const fs::path requested (bytes);
  std::error_code ec;
  const auto canonical = fs::canonical (requested, ec);
  if (! requested.is_absolute () || ec || canonical.string () != bytes
      || canonical.filename () != "mathlab_scope_register.m"
      || ! fs::is_regular_file (canonical, ec) || ec)
    error_with_id ("MathLab:AdapterBindingIdentity", "Expected the exact canonical numerical registry path");
  return canonical;
}

inline octave_value registry_function (const binding& held, octave::interpreter& interp,
                                       const octave_value& requested) {
  if (! active (held, interp))
    error_with_id ("MathLab:AdapterBindingUnavailable", "Qualified registry requires its actual provider");
  const auto expected = registry_path (requested);
  const auto output = interp.feval (held.provider.function_value (),
    ovl ("intrinsic_bind_registry", expected.string ()), 1);
  if (output.length () != 1 || ! output(0).is_function_handle () || output(0).isobject ())
    error_with_id ("MathLab:AdapterBindingIdentity", "Provider returned an invalid registry handle");
  auto *handle = output(0).fcn_handle_value ();
  if (! handle->is_internal ())
    error_with_id ("MathLab:AdapterBindingIdentity", "Registry handle lacks internal actual-object provenance");
  // Only the owning value from the actual provider is accepted. Never resolve
  // a simple/name-only handle or adopt a borrowed raw numerical function pointer.
  const auto registry = handle->fcn_val ();
  if (! registry.is_user_function () || registry.isobject ()
      || registry.function_value ()->name () != "mathlab_scope_register"
      || registry_path (octave_value (registry.user_code_value ()->scope ().fcn_file_name ())) != expected)
    error_with_id ("MathLab:AdapterBindingIdentity", "Provider registry function identity differs");
  const auto installed = interp.get_symbol_table ().find_cmdline_function ("mathlab_scope_register");
  if (! installed.is_user_function () || installed.function_value () != registry.function_value ())
    error_with_id ("MathLab:AdapterBindingIdentity", "Provider registry publication changed actual object identity");
  return registry; // Transient ownership only; never store in adapter binding state.
}
} // namespace mathlab_adapter_binding_v2
#endif
