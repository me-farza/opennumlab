## SPDX-License-Identifier: GPL-3.0-or-later
## Original test-only handle: never load this fixture in production.
classdef MathLabEditLifetimePeer < handle
  properties (Access = private)
    marker
    mode
  endproperties
  methods
    function self = MathLabEditLifetimePeer (marker, mode = 'marker')
      self.marker = marker;
      self.mode = mode;
    endfunction
    function delete (self)
      stream = builtin('fopen', self.marker, 'ab');
      if (stream < 0)
        builtin('error', 'MathLab:LifetimeMarker', 'Cannot publish the delete marker.');
      endif
      unwind_protect
        builtin('fprintf', stream, 'deleted\n');
      unwind_protect_cleanup
        builtin('fclose', stream);
      end_unwind_protect
      if (builtin('strcmp', self.mode, 'assignin') || builtin('strcmp', self.mode, 'error'))
        builtin('assignin', 'base', 'edit_lifetime_called', builtin('true'));
        builtin('assignin', 'base', 'A', builtin('uint64', 900));
      endif
      if (builtin('strcmp', self.mode, 'error'))
        builtin('error', 'MathLab:LifetimeDelete', 'Intentional test-only destructor failure.');
      endif
    endfunction
  endmethods
endclassdef
