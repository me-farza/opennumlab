// SPDX-License-Identifier: GPL-3.0-or-later
// Optional adapter: prepare and bind source-local function objects before entry.
// Uses public Octave APIs; numerical names stay local and no user variables are added.
#include <octave/oct.h>
#include <octave/defun-dld.h>
#include <octave/interpreter.h>
#include <octave/ov-fcn-handle.h>
#include <octave/ov-fcn.h>
#include <octave/ov-usr-fcn.h>
#include <octave/parse.h>
#include <octave/pt-eval.h>
#include <octave/symtab.h>
#include <octave/unwind-prot.h>
#include <octave/error.h>
#include <filesystem>
#include <functional>
#include <list>
#include <string>
#include <set>
#include <utility>
#include <vector>

#include "mathlab_source_context.h"

using mathlab_source_context::bindings_type;
using mathlab_source_context::source_path;
using mathlab_source_context::valid_name;
using mathlab_source_context::validate_function;
using mathlab_source_context::reflect_functions;
using mathlab_source_context::install_bindings;
using mathlab_source_context::parse_script;
using mathlab_source_context::bindings_from_handles;

DEFMETHOD_DLD (mathlab_scope_bridge, interp, args, nargout,
              "Prepare or bind local functions for an exact generated source file")
{
  if (args.length () == 2 && args(0).is_string ()
      && args(0).string_value () == "protocol" && args(1).isnumeric ()
      && args(1).is_scalar_type () && args(1).double_value () == 0)
    {
      octave_scalar_map metadata;
      metadata.assign ("protocol", "mathlab-scope");
      metadata.assign ("version", 2);
      metadata.assign ("pre_entry", true);
      metadata.assign ("original_source_reflection", true);
      metadata.assign ("source_entry", true);
      metadata.assign ("stable_registry", true);
      metadata.assign ("off_path_registry", true);
      return ovl (metadata); // Query only: no symbol-table changes.
    }
  if (args.length () == 2 && args(0).is_string ()
      && args(0).string_value () == "registry" && args(1).is_string () && nargout <= 1)
    {
      mathlab_source_context::retain_registry (interp, args(1));
      return ovl (true);
    }

  const bool prepare = (args.length () == 3 || args.length () == 5)
    && args(0).is_string () && args(0).string_value () == "prepare";
  const bool source_entry = args.length () == 5
    && args(0).is_string () && args(0).string_value () == "source";
  const int offset = (prepare || source_entry) ? 1 : 0;
  const bool reflection = args.length () == offset + 4;
  if ((! prepare && ! source_entry && args.length () != 2 && args.length () != 4)
      || nargout > 1 || ! args(offset).is_string () || ! args(offset+1).iscell ())
    error_with_id ("MathLab:ScopeBridgeArguments",
           "Expected generated path and named handles, or prepare with generated path and local names");

  const auto source_file = source_path (args(offset));
  const Cell entries = args(offset+1).cell_value ();
  const auto private_directory = source_file.parent_path () / "private";
  bindings_type bindings;
  std::set<std::string> names;
  bindings.reserve (entries.numel ());
  if (prepare)
    {
      // Validate all names/files before parsing. Parsing never executes script
      // statements; the caller's ordinary registry owns returned function handles.
      std::vector<std::pair<std::string, std::filesystem::path>> files;
      for (octave_idx_type index = 0; index < entries.numel (); ++index)
        {
          if (! entries(index).is_string ())
            error_with_id ("MathLab:ScopeBridgeName", "Every local name must be a character string");
          const auto name = entries(index).string_value ();
          if (! valid_name (name) || ! names.insert (name).second)
            error_with_id ("MathLab:ScopeBridgeName", "Invalid or duplicate local function name");
          const auto file = source_path (octave_value ((private_directory / (name + ".m")).string ()));
          if (file.parent_path () != private_directory)
            error_with_id ("MathLab:ScopeBridgeCapsule", "Local file is outside this generated private capsule");
          files.emplace_back (name, file);
        }
      auto script = parse_script (interp, source_file);
      for (const auto& [name, file] : files)
        {
          auto function = octave::parse_fcn_file (interp, file.string (), name,
            private_directory.string (), "", "", true, false, false, false);
          validate_function (function, name, private_directory);
          function.function_value ()->mark_as_private_function ();
          bindings.emplace_back (name, std::move (function));
        }
    }
  else
    bindings = bindings_from_handles (source_file, entries);
  octave_value script;
  if (source_entry)
    {
      // Octave source() may reparse after pre-entry binding, discarding its
      // filename-local maps. Parse the actual executing AST first, then bind.
      script = parse_script (interp, source_file);
    }
  if (reflection)
    reflect_functions (bindings, args(offset+2), args(offset+3),
      source_entry ? script.user_code_value () : nullptr);
  // Install only per-file bindings, after complete input validation. Never
  // install_cmdline_function/install_user_function: those expose local names.
  install_bindings (interp, source_file, bindings);
  if (source_entry)
    {
      auto& evaluator = interp.get_evaluator ();
      const auto frame = evaluator.current_call_stack_frame_number ();
      octave::unwind_action restore ([&evaluator, frame] () {
        evaluator.restore_frame (frame);
        evaluator.reset_debug_state ();
      });
      evaluator.goto_base_frame ();
      script.user_code_value ()->call (evaluator, 0, octave_value_list ());
      return ovl (true); // Temporary script AST only; errors/interrupts propagate.
    }
  if (prepare)
    {
      Cell handles (dim_vector (1, static_cast<octave_idx_type> (bindings.size ())));
      for (octave_idx_type index = 0; index < static_cast<octave_idx_type> (bindings.size ()); ++index)
        handles(index) = octave_value (new octave_fcn_handle (bindings[index].second,
          bindings[index].first, std::list<std::string> {}));
      return ovl (handles); // No native owning cache or artificial mlock.
    }
  return ovl (static_cast<double> (bindings.size ()));
}
