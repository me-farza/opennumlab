// SPDX-License-Identifier: GPL-3.0-or-later
// Shared public-API source context helpers. No static owning state or caches.
#ifndef MATHLAB_SOURCE_CONTEXT_H
#define MATHLAB_SOURCE_CONTEXT_H
#include <octave/oct.h>
#include <octave/interpreter.h>
#include <octave/load-path.h>
#include <octave/ov-fcn-handle.h>
#include <octave/ov-usr-fcn.h>
#include <octave/parse.h>
#include <octave/symtab.h>
#include <octave/error.h>
#include <filesystem>
#include <functional>
#include <set>
#include <string>
#include <utility>
#include <vector>

namespace mathlab_source_context {
using bindings_type = std::vector<std::pair<std::string, octave_value>>;

inline std::filesystem::path source_path (const octave_value& value)
{
  if (! value.is_string ())
    error_with_id ("MathLab:ScopeBridgeArguments", "Expected a generated source path");
  std::error_code ec;
  const auto file = std::filesystem::canonical (value.string_value (), ec);
  if (ec || ! std::filesystem::is_regular_file (file, ec))
    error_with_id ("MathLab:ScopeBridgeFile", "Generated source file does not exist");
  return file;
}

inline bool valid_name (const std::string& name)
{
  const auto letter = [] (char ch) {
    return (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || ch == '_';
  };
  if (name.empty () || ! letter (name.front ())) return false;
  for (const char ch : name)
    if (! letter (ch) && ! (ch >= '0' && ch <= '9')) return false;
  return true;
}

inline void validate_function (const octave_value& function, const std::string& name,
                        const std::filesystem::path& private_directory)
{
  if (! valid_name (name))
    error_with_id ("MathLab:ScopeBridgeName", "Invalid local function name");
  if (! function.is_user_function ())
    error_with_id ("MathLab:ScopeBridgeFunction", "Could not obtain a named local function object");
  std::error_code ec;
  // Display filenames may already be original paths. Capsule membership always
  // uses the immutable lookup scope, including after reflection is installed.
  const auto file = std::filesystem::canonical (
    function.user_code_value ()->scope ().fcn_file_name (), ec);
  if (ec || file.parent_path () != private_directory || file.filename () != name + ".m"
      || function.function_value ()->name () != name)
    error_with_id ("MathLab:ScopeBridgeCapsule", "Local definitions must belong to this generated private capsule");
}

inline void reflect_functions (bindings_type& bindings, const octave_value& path,
                        const octave_value& source, octave_user_code *script = nullptr)
{
  if (! path.is_string () || ! source.is_string ())
    error_with_id ("MathLab:ScopeBridgeArguments", "Expected original source path and source text");
  const auto original = std::filesystem::path (path.string_value ());
  if (! original.is_absolute () || original.filename ().empty ())
    error_with_id ("MathLab:ScopeBridgeOriginal", "Original source path must be absolute");
  const std::string original_path = original.lexically_normal ().string ();
  const std::string text = source.string_value ();
  std::set<octave_user_code *> visited;
  std::function<void(octave_user_code *)> reflect = [&] (octave_user_code *code) {
    if (! code || ! visited.insert (code).second) return;
    const auto identity = code->scope ().fcn_file_name ();
    code->stash_fcn_file_name (original_path);
    code->cache_function_text (text, code->time_parsed ());
    if (code->scope ().fcn_file_name () != identity)
      error_with_id ("MathLab:ScopeBridgeIdentity", "Original-source metadata changed immutable scope identity");
    for (const auto& child : code->subfunctions ())
      reflect (child.second.user_code_value (true));
  };
  for (const auto& binding : bindings)
    reflect (binding.second.user_code_value ());
  reflect (script);
}

inline void install_bindings (octave::interpreter& interp, const std::filesystem::path& file,
                       const bindings_type& bindings)
{
  std::set<std::string> contexts {file.string ()};
  for (const auto& binding : bindings)
    contexts.insert (binding.second.user_code_value ()->scope ().fcn_file_name ());
  auto& symbols = interp.get_symbol_table ();
  for (const auto& context : contexts)
    for (const auto& binding : bindings)
      symbols.install_local_function (binding.first, binding.second, context);
}
inline octave_value parse_script (octave::interpreter& interp, const std::filesystem::path& file)
{
  auto script = octave::parse_fcn_file (interp, file.string (), file.stem ().string (),
    file.parent_path ().string (), "", "", true, true, false, false);
  if (! script.is_user_script ())
    error_with_id ("MathLab:ScopeBridgeScript", "Source entry requires a generated script");
  return script;
}

inline bindings_type bindings_from_handles (const std::filesystem::path& file,
                                             const Cell& handles)
{
  bindings_type bindings;
  std::set<std::string> names;
  const auto private_directory = file.parent_path () / "private";
  bindings.reserve (handles.numel ());
  for (octave_idx_type index = 0; index < handles.numel (); ++index)
    {
      if (! handles(index).is_function_handle ())
        error_with_id ("MathLab:ScopeBridgeHandle", "Every local binding must be a function handle");
      auto *handle = handles(index).fcn_handle_value ();
      if (handle->is_anonymous () || handle->is_nested ())
        error_with_id ("MathLab:ScopeBridgeHandle", "Only named outer local functions can be bound");
      octave_value function = handle->fcn_val ();
      if (function.is_undefined ())
        {
          auto *pointer = handle->function_value (true);
          if (pointer) function = octave_value (pointer, true);
        }
      const auto name = handle->fcn_name ();
      if (! names.insert (name).second)
        error_with_id ("MathLab:ScopeBridgeName", "Duplicate local function binding");
      validate_function (function, name, private_directory);
      bindings.emplace_back (name, std::move (function));
    }
  return bindings;
}

inline void retain_registry (octave::interpreter& interp, const octave_value& path)
{
  const auto file = source_path (path);
  static constexpr const char *name = "mathlab_scope_register";
  if (file.filename () != std::string (name) + ".m")
    error_with_id ("MathLab:ScopeBridgeRegistry", "Expected the engine's registry source file");
  auto& symbols = interp.get_symbol_table ();
  std::string directory;
  const auto visible = interp.get_load_path ().find_fcn (name, directory);
  if (! visible.empty ())
    {
      std::error_code visible_error;
      const auto visible_file = std::filesystem::canonical (visible, visible_error);
      // Octave can retain a directory-cache entry after a foreign file is
      // removed. Its absence must not prevent the next request from recovering.
      if ((! visible_error && visible_file != file)
          || (visible_error && visible_error != std::errc::no_such_file_or_directory))
        error_with_id ("MathLab:ScopeBridgeRegistry", "The internal registry is shadowed; no function was replaced");
      if (visible_error)
        interp.get_load_path ().rehash ();
    }
  // Resolve the internal global helper explicitly, including from a paused
  // user's private/local scope. A foreign command-line definition is rejected.
  auto registry = symbols.find_cmdline_function (name);
  if (registry.is_undefined ())
    {
      registry = symbols.find_user_function (name);
      if (registry.is_undefined ())
        registry = octave::parse_fcn_file (interp, file.string (), name,
          file.parent_path ().string (), "", "", true, false, false, false);
    }
  std::error_code identity_error;
  const auto identity = registry.is_user_function ()
    ? std::filesystem::canonical (registry.user_code_value ()->scope ().fcn_file_name (), identity_error)
    : std::filesystem::path ();
  if (! registry.is_user_function () || identity_error || identity != file)
    error_with_id ("MathLab:ScopeBridgeRegistry", "The internal registry is shadowed; no function was replaced");
  // Stabilize only the genuine transport helper against path-cache reloads.
  // Normal clear functions/all remove this unlocked command-line definition.
  // Its existing persistent keys are retained; no numerical name is installed.
  symbols.install_cmdline_function (name, registry);
  // The same object must not also live in the on-path slot. Class-constructor
  // lookup probes that slot before command-line functions; a path-cache reload
  // can erase the host's breakpoint-table membership while its alias ASTs live.
  // The worker removes this helper-only directory from the native load path.
  symbols.install_user_function (name, octave_value ());
}

}

#endif
