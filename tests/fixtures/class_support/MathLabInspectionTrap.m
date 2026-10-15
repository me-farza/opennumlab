% SPDX-License-Identifier: GPL-3.0-or-later
classdef MathLabInspectionTrap
    % Any accidental user-method dispatch during inspection must fail loudly.
    methods
        function varargout = size(varargin)
            builtin('assignin', 'base', 'inspection_trap_called', true);
            error('MathLab:InspectionTrap', 'Inspection called overloaded size.');
        end
        function disp(varargin)
            builtin('assignin', 'base', 'inspection_trap_called', true);
            error('MathLab:InspectionTrap', 'Inspection called overloaded disp.');
        end
        function varargout = subsref(varargin)
            builtin('assignin', 'base', 'inspection_trap_called', true);
            error('MathLab:InspectionTrap', 'Inspection called overloaded subsref.');
        end
    end
end
