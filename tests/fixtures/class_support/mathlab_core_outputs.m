% SPDX-License-Identifier: GPL-3.0-or-later
function [total, metadata, varargout] = mathlab_core_outputs(first, varargin)
    total = first;
    for index = 1:numel(varargin)
        total = total + varargin{index};
    end
    metadata = struct('input_count', nargin, 'output_count', nargout, ...
                      'first_name', inputname(1), ...
                      'tail', {varargin});
    for index = 1:max(0, nargout - 2)
        varargout{index} = total + index;
    end
end
