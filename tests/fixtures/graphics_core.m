% Graphics-object properties and an SVG export; requires a graphics toolkit.
% SPDX-License-Identifier: GPL-3.0-or-later
fixture_svg = [tempname(), '.svg'];
fixture_figure = figure('Visible', 'off');
try
    fixture_x = linspace(0, 1, 11);
    fixture_y = fixture_x .^ 2;
    fixture_line = plot(fixture_x, fixture_y, 'LineWidth', 2);
    title('MathLab graphics fixture');
    xlabel('x');
    ylabel('x squared');
    grid on;
    assert(isequal(get(fixture_line, 'XData'), fixture_x));
    assert(isequal(get(fixture_line, 'YData'), fixture_y));
    assert(get(fixture_line, 'LineWidth') == 2);
    set(fixture_line, 'LineWidth', 1.5);
    assert(get(fixture_line, 'LineWidth') == 1.5);
    print(fixture_figure, fixture_svg, '-dsvg');
    fixture_info = dir(fixture_svg);
    assert(~isempty(fixture_info));
    assert(fixture_info.bytes > 100);
    assert(~isempty(strfind(fileread(fixture_svg), '<svg')));
catch fixture_error
    close(fixture_figure);
    if exist(fixture_svg, 'file')
        delete(fixture_svg);
    end
    rethrow(fixture_error);
end
close(fixture_figure);
delete(fixture_svg);
disp('PASS graphics_core');
