// SPDX-License-Identifier: GPL-3.0-or-later
// Narrow Octave 11.3.0 graphics lifetime witness. A retained native object owns
// its properties allocation, so recycled numerical handles cannot authorize a
// replacement after listeners, DeleteFcn, or dynamic properties are removed.
#include "mathlab_adapter_binding_v2.h"
#include <octave/oct.h>
#include <octave/defun-dld.h>
#include <octave/error.h>
#include <octave/gh-manager.h>
#include <octave/graphics.h>
#include <octave/interpreter.h>
#include <octave/oct-mutex.h>
#include <octave/ov-fcn-handle.h>
#include <octave/ov-fcn.h>
#include <octave/pt-eval.h>
#include <octave/symtab.h>
#include <octave/unwind-prot.h>
#include <octave/version.h>

#include <cmath>
#include <cstddef>
#include <cstdint>
#include <exception>
#include <limits>
#include <map>
#include <random>
#include <string>
#include <utility>

namespace {
mathlab_adapter_binding_v2::binding& intrinsic_binding () {
  // Transport objects only; the existing graphics ledger owns numerical state.
  static auto *value = new mathlab_adapter_binding_v2::binding;
  return *value;
}

constexpr std::size_t max_entries = 4096;
constexpr std::size_t max_epoch_bytes = 512;
constexpr std::size_t max_key_bytes = 128;

struct lifetime_ledger {
  std::string epoch;
  std::string incarnation;
  std::uint64_t next_key = 1;
  octave::interpreter *owner = nullptr;
  std::map<std::string, octave::graphics_object> objects;
};

lifetime_ledger& ledger () {
  static lifetime_ledger value;
  return value;
}

std::string primitive_text (const octave_value& value, std::size_t limit,
                            const char *field) {
  if (! value.is_string () || value.rows () != 1)
    error_with_id ("MathLab:GraphicsBridgeArguments",
                   "%s must be a primitive character row", field);
  if (value.numel () == 0
      || static_cast<std::size_t> (value.numel ()) > limit)
    error_with_id ("MathLab:GraphicsBridgeArguments",
                   "%s is empty or exceeds its byte budget", field);
  const auto text = value.string_value ();
  if (text.empty () || text.size () > limit
      || text.find ('\0') != std::string::npos)
    error_with_id ("MathLab:GraphicsBridgeArguments",
                   "%s is empty, too long, or contains a NUL byte", field);
  return text;
}

double primitive_handle (const octave_value& value) {
  if (! value.is_double_type () || ! value.is_real_scalar () || value.issparse ())
    error_with_id ("MathLab:GraphicsBridgeArguments",
                   "The private graphics handle must be a real double scalar");
  const double handle = value.double_value ();
  if (! std::isfinite (handle))
    error_with_id ("MathLab:GraphicsBridgeArguments",
                   "The private graphics handle must be finite");
  return handle;
}

std::string random_incarnation () {
  static constexpr char hex[] = "0123456789abcdef";
  std::random_device entropy;
  std::string result;
  result.reserve (32);
  // Four independently requested 32-bit words identify this loaded ledger.
  // Failure to obtain entropy aborts opening; no deterministic fallback exists.
  for (unsigned word = 0; word < 4; ++word) {
    const auto value = static_cast<std::uint32_t> (entropy ());
    for (int shift = 28; shift >= 0; shift -= 4)
      result.push_back (hex[(value >> shift) & 15]);
  }
  return result;
}

bool supported_object (const octave::graphics_object& object) {
  if (! object.valid_object () || object.get_properties ().is_beingdeleted ())
    return false;
  const auto type = object.get_properties ().graphics_object_name ();
  return type == "figure" || type == "axes" || type == "line";
}
} // namespace

DEFMETHOD_DLD (mathlab_graphics_bridge, interp, args, nargout,
              "Retain and compare native graphics lifetimes in an owned epoch")
{
  if (mathlab_adapter_binding_v2::is_bind_operation (args)) {
    mathlab_adapter_binding_v2::bind (intrinsic_binding (), interp, args, nargout,
                                    "graphics", "mathlab_graphics_bridge");
    return octave_value_list ();
  }
  mathlab_adapter_binding_v2::active (intrinsic_binding (), interp);
  if (args.length () == 0 && nargout == 0) {
    // The provider's retained shutdown proxy calls this before interpreter
    // teardown. Graphics properties can retain user callbacks, so releasing
    // them from the static ledger's destructor is too late.
    auto& state = ledger ();
    if (state.owner && state.owner != &interp)
      error_with_id ("MathLab:GraphicsBridgeArguments",
                     "Native graphics cleanup belongs to another interpreter");
    state.epoch.clear ();
    state.owner = nullptr;
    state.objects.clear ();
    return octave_value_list ();
  }
  if (args.length () < 1 || nargout > 1)
    error_with_id ("MathLab:GraphicsBridgeArguments",
                   "Expected protocol, open, capture, or check");
  const auto command = primitive_text (args(0), 16, "Command");
  if (command == "protocol") {
    if (args.length () != 1
        && ! (args.length () == 2 && args(1).is_double_type ()
              && args(1).is_real_scalar () && args(1).double_value () == 0))
      error_with_id ("MathLab:GraphicsBridgeArguments",
                     "Protocol accepts no arguments or a primitive zero");
    octave_scalar_map metadata;
    metadata.assign ("protocol", "mathlab-graphics");
    metadata.assign ("version", 1);
    metadata.assign ("octave_version", OCTAVE_VERSION);
    metadata.assign ("octave_api_version", OCTAVE_API_VERSION);
    metadata.assign ("native_object_lifetime", true);
    metadata.assign ("bound_primitive_dispatcher", true);
    metadata.assign ("max_entries", static_cast<double> (max_entries));
    return ovl (metadata);
  }
  if (args.length () < 2)
    error_with_id ("MathLab:GraphicsBridgeArguments", "An owned epoch is required");
  const auto epoch = primitive_text (args(1), max_epoch_bytes, "Epoch");
  auto& state = ledger ();

  if (command == "open") {
    if (args.length () != 2)
      error_with_id ("MathLab:GraphicsBridgeArguments", "Open expects one epoch");
    // Opening is an effectful owner operation. Retired native references are
    // released here, not while checking/querying a live object.
    state.epoch.clear ();
    state.objects.clear ();
    if (state.incarnation.empty ()) {
      try {
        state.incarnation = random_incarnation ();
      } catch (const std::exception&) {
        error_with_id ("MathLab:GraphicsUnsupported",
                       "Native graphics lifetime entropy is unavailable");
      }
    }
    state.owner = &interp;
    state.epoch = epoch;
    return ovl (true);
  }

  if (command == "capture") {
    if (args.length () != 3)
      error_with_id ("MathLab:GraphicsBridgeArguments",
                     "Capture expects epoch and private graphics handle");
    const double handle = primitive_handle (args(2));
    if (state.owner != &interp || state.epoch != epoch || state.epoch.empty ())
      error_with_id ("MathLab:GraphicsBridgeArguments",
                     "The native graphics ledger is not open in this epoch");
    if (state.objects.size () >= max_entries
        || state.next_key == std::numeric_limits<std::uint64_t>::max ())
      error_with_id ("MathLab:GraphicsLimit",
                     "Native graphics lifetime retention is limited to 4096 entries per epoch");
    auto& manager = interp.get_gh_manager ();
    octave::autolock guard (manager.graphics_lock ());
    auto object = manager.get_object (handle);
    if (! supported_object (object))
      error_with_id ("MathLab:GraphicsUnsupported",
                     "Only a live native figure, axes, or line can be retained");
    const auto key = "b" + state.incarnation + "-" + std::to_string (state.next_key++);
    state.objects.emplace (key, std::move (object));
    return ovl (key);
  }

  if (command == "check") {
    if (args.length () != 4)
      error_with_id ("MathLab:GraphicsBridgeArguments",
                     "Check expects epoch, opaque native key, and private graphics handle");
    const auto key = primitive_text (args(2), max_key_bytes, "Native key");
    const double handle = primitive_handle (args(3));
    if (state.owner != &interp || state.epoch != epoch || state.epoch.empty ())
      return ovl (false);
    const auto held = state.objects.find (key);
    if (held == state.objects.end ())
      return ovl (false);
    auto& manager = interp.get_gh_manager ();
    octave::autolock guard (manager.graphics_lock ());
    const auto current = manager.get_object (handle);
    return ovl (supported_object (current) && supported_object (held->second)
                && &current.get_properties () == &held->second.get_properties ());
  }

  error_with_id ("MathLab:GraphicsBridgeArguments", "Unknown graphics bridge command");
  return ovl ();
}
