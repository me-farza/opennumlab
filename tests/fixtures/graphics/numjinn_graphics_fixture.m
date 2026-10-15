## SPDX-License-Identifier: GPL-3.0-or-later
## Original CP04 source fixture. Sample indices remain one based and the full
## source arrays remain authoritative if a renderer later decimates the line.
t = 0:0.02:10;
y = sin (t);
graphics_figure = figure ('visible', 'off', 'integerhandle', 'on', ...
                          'name', 'OpenNumLab original sine fixture');
graphics_axes = axes ('parent', graphics_figure, ...
                      'xscale', 'linear', 'yscale', 'linear');
graphics_line = line ('parent', graphics_axes, 'xdata', t, 'ydata', y, ...
                      'linewidth', 1.5, 'buttondownfcn', @numjinn_graphics_callback);
graphics_callback_mode = 'record';
graphics_callback_log = {};
graphics_callback_effect = 0;
graphics_peer = 19;
