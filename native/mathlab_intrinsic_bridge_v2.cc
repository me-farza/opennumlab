// SPDX-License-Identifier: GPL-3.0-or-later
// PRIVATE UNCOMPILED V2 CANDIDATE: the single standalone provider owns this core.
#include <octave/defun-dld.h>
#include "mathlab_intrinsic_core_v2.h"

DEFMETHOD_DLD (mathlab_intrinsic_bridge, interp, args, nargout,
              "Bind genuine startup primitives for the owned worker")
{
  octave_value_list result;
  if (mathlab_intrinsic_v2_proposal::dispatch (interp, args, nargout, result))
    return result;
  error_with_id ("MathLab:IntrinsicArguments", "Expected an intrinsic operation");
  return ovl ();
}
