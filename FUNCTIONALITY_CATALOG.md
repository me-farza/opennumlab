# MATLAB compatible platform functionality catalog

This catalog turns the platform ambition into implementation workstreams. Stages S0–S7 refer to the [development plan](DEVELOPMENT_PLAN.md). Each row must be decomposed into API variants and executable acceptance tests before being claimed complete. Function lists below are seed scope, not an exhaustive enumeration of MATLAB or its toolboxes.

The default interface must let existing MATLAB users perform ordinary work without a tutorial. Language compatibility, familiar workflows, and data integrity are release requirements; new conveniences are additions to that foundation.

## Language and runtime

The language/data-type inventory should be reconciled against the selected release's [language fundamentals](https://www.mathworks.com/help/matlab/language-fundamentals.html), [data types](https://www.mathworks.com/help/matlab/data-types.html), and individual reference pages. The following requirements are the proposed implementation breakdown.

| ID | Capability | Required behavior and evidence | Stage |
| --- | --- | --- | --- |
| L01 | Source syntax | Expressions, comments, continuation, sections, command syntax, matrix/cell literals; parser tests include ambiguous quotes and whitespace. | S1 |
| L02 | Array shape | Scalar, row, column, N-dimensional and empty arrays; retain shape through operations and serialization. | S1 |
| L03 | Indexing | One-based indexing, column-major linear indexing, colon, `end`, logical indexing, mixed subscripts, repeated indices, growth and deletion. | S1 |
| L04 | Operators | Matrix and elementwise arithmetic; transpose and conjugate transpose; comparison, logical operators, short-circuit behavior, precedence. | S1 |
| L05 | Expansion and dimensions | Match singleton expansion, concatenation, dimension defaults, trailing dimensions and empty-array behavior per operation. | S1 |
| L06 | Primitive values | Double, single, integer widths, logical, complex; match casting, saturation/promotion rules and supported combinations. | S1–S3 |
| L07 | Text | Character arrays and string arrays with distinct shapes and missing values; Unicode indexing and conversion according to profile. | S1–S3 |
| L08 | Containers | Cell arrays, scalar and nonscalar structs, field/index access, comma-separated lists, nested assignment. | S1–S3 |
| L09 | Flow control | Conditionals, switch, for/while, break/continue, return, try/catch; compare truth-value and exception behavior. | S1 |
| L10 | Functions | Multiple outputs, optional inputs, `nargin`, `nargout`, `varargin`, `varargout`, local/nested functions and recursion. | S1–S3 |
| L11 | Workspaces | Script versus function workspace, global/persistent variables, clearing and lifecycle rules. | S1–S3 |
| L12 | Closures | Anonymous functions, captured values, nested shared state, function handle identity and calls. | S3 |
| L13 | Name resolution | Search path, shadowing, private directories, package/class directories, imports, current-folder effects. | S1–S3 |
| L14 | Dynamic execution | `eval`, `evalin`, `assignin`, `feval`, dynamic fields; preserve behavior across interpreter and optimized code. | S3 |
| L15 | Classes | `classdef`, value/handle classes, inheritance, properties, access control, methods, constructors and destruction. | S3 |
| L16 | Advanced objects | Operator/index overloading, events/listeners, enumeration, metadata/reflection and supported legacy class forms. | S3–S4 |
| L17 | Argument contracts | Validation blocks, defaults, validators and name-value argument resolution. | S3 |
| L18 | Diagnostics | Errors, warning IDs, stack traces, source locations, warning controls and caught exceptions. | S1–S3 |
| L19 | State and lifecycle | RNG streams, resource cleanup, timers, callbacks, interrupts, object cycles and exception unwinding. | S1–S3 |
| L20 | Execution engines | CLI, REPL, scripts, function calls, bytecode and later JIT; equivalent supported observable behavior. | S1–S5 |

Critical fixtures must check negative and zero indices, empty dimensions, complex transpose, aliased assignment, integer boundaries, NaNs, signed zero, trailing singleton dimensions, handle identity, and evaluation order. These are central language contracts, not rare cleanup tasks.

## Core numerical and data functions

| ID | Function family | Initial named API inventory | Stage |
| --- | --- | --- | --- |
| N01 | Construction | `zeros`, `ones`, `eye`, `nan`, `inf`, `linspace`, `logspace`, `meshgrid`, `ndgrid` | S1–S3 |
| N02 | Shape and rearrangement | `size`, `length`, `numel`, `ndims`, `reshape`, `squeeze`, `permute`, `ipermute`, `repmat`, `cat`, `vertcat`, `horzcat`, `flip`, `rot90`, `circshift` | S1–S3 |
| N03 | Selection and sets | `find`, `sort`, `sortrows`, `unique`, `union`, `intersect`, `setdiff`, `ismember`, `any`, `all` | S1–S3 |
| N04 | Elementary math | Trigonometric, inverse and hyperbolic functions; exponentials/logarithms; `sqrt`, `abs`, `angle`, `real`, `imag`, `conj`, rounding, `mod`, `rem` | S1–S3 |
| N05 | Reductions | `sum`, `prod`, `mean`, `median`, `min`, `max`, `std`, `var`, `cumsum`, `cumprod`, `diff`, `movmean` | S1–S3 |
| N06 | Dense linear algebra | `mldivide`, `mrdivide`, `mtimes`, `norm`, `rank`, `cond`, `rcond`, `det`, `trace`, `lu`, `qr`, `chol`, `svd`, `eig`, `schur`, `pinv`, `inv` | S1–S3 |
| N07 | Advanced matrix algorithms | Generalized eigenproblems, matrix functions, Sylvester/Lyapunov solves where applicable, decompositions and reusable factorizations | S3–S4 |
| N08 | Sparse computation | `sparse`, `full`, `speye`, `spdiags`, `nnz`, `find`, sparse arithmetic/factorization, `eigs`, `svds`, `pcg`, `gmres`, preconditioners | S3–S4 |
| N09 | Transforms and convolution | `fft`, `ifft`, `fft2`, `ifft2`, `fftn`, `ifftn`, `fftshift`, `ifftshift`, `conv`, `conv2`, `convn` | S2–S3 |
| N10 | Interpolation and polynomials | `interp1`, `interp2`, `interpn`, `spline`, interpolant objects, `polyfit`, `polyval`, `roots`, `poly`, `polyder`, `polyint` | S3 |
| N11 | Calculus and equations | `integral`, `integral2`, `integral3`, `trapz`, `cumtrapz`, `gradient`, `fzero`, nonstiff/stiff ODE and DAE families | S3–S4 |
| N12 | Random values | `rng`, `rand`, `randn`, `randi`, `randperm`, streams and distributions; exact-sequence compatibility tracked separately | S1–S4 |
| N13 | Special functions | Gamma/beta/error functions, Bessel/Airy families, combinatorics and relevant complex domains | S3–S4 |
| N14 | Text processing | `string`, `char`, `strcat`, `split`, `join`, `contains`, `replace`, `regexp`, `sprintf`, `sscanf`, conversions | S2–S3 |
| N15 | Tables | `table`, `readtable`, `writetable`, joins, sorting, grouping, row/variable indexing and metadata | S3 |
| N16 | Time and categories | `datetime`, `duration`, `calendarDuration`, `categorical`, `timetable`, `retime`, `synchronize`; time zones and missing values | S3 |
| N17 | Data preparation | `ismissing`, `fillmissing`, `rmmissing`, `normalize`, `isoutlier`, `filloutliers`, grouping and moving windows | S3–S4 |
| N18 | Large data | Datastores, chunked arrays, tall/distributed operations, bounded-memory evaluation and spill behavior | S5 |

Confirm product ownership for every symbol during S0; a listed algorithm family can cross MATLAB/toolbox boundaries. Supporting a family does not imply every member is shipped together.

For each numerical API, specify input/output classes, shapes, dimensions, empty cases, complex support, sparse support, missing-value options, algorithm defaults, precision, convergence, warnings and errors. For `A\b`, test square, rectangular, rank-deficient, ill-conditioned, sparse, complex and multiple-right-hand-side cases. Do not implement it as `inv(A)*b`.

## Desktop and programming tools

| ID | Component | Required functionality | Stage |
| --- | --- | --- | --- |
| U01 | Familiar desktop | Familiar panel names and placement, docking, undocking, layouts, menus/toolbars and settings. | S2 |
| U02 | Command Window | Prompt, completion, history, multiline input, formatted results, clickable errors, interrupt and clear. | S1–S2 |
| U03 | Current Folder | File tree, navigation, path controls, recent files, creation/rename, contextual actions and search. | S2 |
| U04 | Workspace | Name, class, dimensions, size, preview, sorting, filtering, rename/delete/save selected variables. | S2 |
| U05 | Variable Editor | Numeric, logical, text, cell, struct and table inspection; slice N-D arrays; copy/paste and validated edits. | S2–S3 |
| U06 | Code Editor | Syntax highlighting, completion, indentation, sections, tabs, search/replace, navigation and formatting. | S2–S3 |
| U07 | Run actions | Run file, selection, section and function; working-folder/path behavior; visible execution state. | S2 |
| U08 | Debugger | Breakpoints, conditions, step in/over/out, call stack, scope inspection, pause on errors and evaluation while paused. | S2–S3 |
| U09 | Language tooling | Diagnostics, signature help, symbol lookup, references, safe rename and editor protocol integration. | S3 |
| U10 | Profiler | CPU time, call counts, allocations, memory peaks, hot lines and reproducible reports. | S3–S5 |
| U11 | Tests and coverage | Test runner, assertions, fixtures, parameterization, reports and code coverage. | S3 |
| U12 | Live documents | Code, text, equations, images, output, sections, interactive controls and deterministic rerun. | S3–S4 |
| U13 | Documentation | Contextual function help, searchable offline manual, executable examples and compatibility notes. | S2–S3 |
| U14 | Projects | Project paths, dependencies, environment lockfiles, startup tasks, source-control diffs and reproducibility. | S3 |
| U15 | App building | Programmatic controls first; later visual layout editor, callbacks, component properties, packaging and app import subsets. | S3–S5 |
| U16 | Desktop quality | MATLAB-style shortcut profile, keyboard access, screen readers, input methods, fonts, scaling and multiple monitors. | S2–S3 |
| U17 | Recovery | Autosave/recovery for source, explicit session snapshots, kernel restart and clear unsaved-work indicators. | S2–S3 |

A UI screenshot is not a completion criterion. Test the actual workflows with existing MATLAB users under the usability gate in the development plan. Shortcut behavior must be verified per platform rather than assumed universal.

## Visualization and graphics

| ID | Capability | Required functionality | Stage |
| --- | --- | --- | --- |
| G01 | Figure object system | Figures, axes, children, handle identity/lifecycle, current figure/axes, properties, defaults and callbacks. | S2–S3 |
| G02 | Common 2D plots | `plot`, `scatter`, `bar`, `barh`, `histogram`, `stairs`, `stem`, `area`, `errorbar`, `pie` and multiple series. | S2–S3 |
| G03 | Axes and layout | `subplot`, `tiledlayout`, labels, titles, legends, limits, ticks, grid, linked axes and dual axes. | S2–S3 |
| G04 | Log and polar plots | `semilogx`, `semilogy`, `loglog`, polar axes and coordinate handling. | S3 |
| G05 | Matrix and image views | `image`, `imagesc`, image display, colormaps, colorbars, scaling and alpha. | S2–S3 |
| G06 | 3D graphics | `plot3`, `scatter3`, `mesh`, `surf`, contours, patches, quiver, cameras, lights and materials. | S3–S4 |
| G07 | Advanced visualization | Volumes, slices, isosurfaces, streams, geographic plots, graphs and domain-specific charts. | S4–S7 |
| G08 | Interaction | Zoom, pan, rotate, data tips, brushing, selection, property editing and keyboard interactions. | S2–S3 |
| G09 | Dynamic rendering | `drawnow`, animated lines, timers, callbacks, streaming updates and bounded redraw cost. | S3–S5 |
| G10 | Export | PNG, SVG, PDF and selected publication formats; fonts, sizing, transparency, clipping and reproducible headless output. | S2–S4 |
| G11 | Large plots | Level-of-detail rendering and decimation that preserve inspection/export access to original data; benchmark latency. | S3–S5 |
| G12 | Figure persistence | Supported object serialization, `.fig` import/export subsets and explicit unknown-property reporting. | S3–S4 |

Maintain independent tests for object behavior and rendered appearance. Pixel identity across font engines and GPUs is a separate claim from compatible chart behavior. For publication export, test vector content, labels, dimensions and clipping directly.

## Files and interoperability

“Open MATLAB files” must name formats, versions, stored types, and actions. Reading a container, rendering its content, executing its code and round-trip writing it are separate milestones.

| File or interface | Planned behavior | Stage and limitation |
| --- | --- | --- |
| `.m` | Edit and execute scripts, functions and classes; support live-document annotations for the selected profile. | S1–S3; only declared language/API subsets initially. |
| `.mat` Version 4 | Read/write supported numeric, character and sparse data. | S2. |
| Level 5 MAT family, including Version 6/7 saves | Read/write numeric/complex/logical arrays, chars, cells, structs, sparse arrays and compressed content as specified. | S2–S3; serialized objects require separate class support. |
| `.mat` Version 7.3 | HDF5-based read/write with MATLAB-specific metadata, references, chunking and partial access. | S3; tables, objects and advanced containers have separate fixtures. |
| `.mlx` | Import code, text, equations and available outputs; execute supported code; report conversion losses. | S3–S4; full fidelity and export are separate gates. |
| `.fig` | Restore supported graphics trees; inspect unsupported objects without silently dropping them. | S3–S4; not merely image loading. |
| `.mlapp` | Inspect/import supported app code, layout and callbacks; run through compatible UI components. | S4–S5; no universal binary/layout fidelity promise. |
| `.slx`, `.mdl` | Parse supported models, map blocks, validate parameters and execute declared model subsets. | S6–S7; parsing alone never implies simulation support. |
| Project and package files | Recognize declared project/toolbox manifests; map paths, dependencies and resources. | S3–S5; proprietary deployment archives are separately assessed. |
| `.p` P-code | Identify and explain unsupported proprietary executable content; request available source or an alternative workflow. | Blocked/unassessed for execution; no blanket compatibility promise. |
| MEX source | Recompile a declared source API subset against the project's extension interface. | S5; track legacy and modern APIs separately. |
| Existing MEX binaries | Investigate platform-specific ABI/runtime compatibility in isolated workers. | S5 research; source compatibility is not binary compatibility. |
| Protected models, vendor runtime artifacts | Recognize files and report prerequisites or incompatibilities. | S7 research; may remain unavailable without proprietary dependencies. |
| CSV/text/JSON/XML/binary | Typed reading/writing, encoding, delimiter/date options, partial reads and streaming. | S2–S3. |
| Scientific and tabular formats | HDF5, NetCDF, spreadsheets, Parquet/Arrow and databases, each behind tested adapters. | S3–S5; check exact type/date/metadata fidelity. |
| Media and domain formats | Images, audio, video, DICOM, geospatial and specialized formats as their modules arrive. | S3–S7; codecs and licenses are tracked dependencies. |
| External languages | Rust embedding API, C interface, then optional Python, Java, .NET and platform-specific automation adapters. | S3–S5; optional foreign runtimes are not core requirements. |
| Devices and services | HTTP, serial, TCP/UDP, instruments, acquisition, cameras, robotics middleware and database services. | S4–S7; test with actual target devices/services. |

MAT Version 7.3 is HDF5-based, while earlier MAT variants require other encodings; support must be tested by version. See [MathWorks MAT file versions](https://www.mathworks.com/help/matlab/import_export/mat-file-versions.html). Live documents can use `.mlx` or a plain-text `.m` representation in current MATLAB; see [live code formats](https://www.mathworks.com/help/matlab/matlab_prog/file-format-for-live-scripts.html). P-code is a proprietary obfuscated format, and MEX binaries have platform requirements: [P-code](https://www.mathworks.com/help/matlab/ref/pcode.html), [MEX compatibility](https://www.mathworks.com/help/matlab/matlab_external/platform-compatibility.html).

For unknown serialized classes, preserve the source file and offer bounded inspection where possible; do not claim successful restoration. Deserializers must not execute embedded code automatically during preview.

## Toolbox programs

These programs retain the user's whole-ecosystem ambition. Every program requires a public API inventory, class/object behavior, algorithms, data formats, interactive apps, examples, migration fixtures and specialist review. APIs listed here are representative seed targets, and stage ranges describe program start and expansion rather than full parity deadlines.

| ID | Program | Concrete scope | Stage |
| --- | --- | --- | --- |
| T01 | Statistics and machine learning | Distributions, estimation, hypothesis tests, regression/GLMs, ANOVA, PCA, clustering, classification, ensembles, cross-validation and diagnostic apps. | S4–S7 |
| T02 | Optimization and curve fitting | Linear/quadratic/constrained nonlinear optimization, least squares, global and mixed-integer methods, splines, fit objects and interactive fitting. | S4–S7 |
| T03 | Symbolic mathematics | Expression trees, exact arithmetic, simplification, assumptions, differentiation, integration, equations, transforms, arbitrary precision and code generation. | S4–S7 |
| T04 | Signal, DSP, wavelets and audio | Filter design, `filter`/`filtfilt`, resampling, spectral analysis, streaming DSP, wavelets, audio devices and signal-analysis apps. | S4–S7 |
| T05 | Control and system identification | Transfer/state-space models, time/frequency response, stability, PID, identification, robust control, MPC and tuning apps. | S4–S7 |
| T06 | Images, vision and medical imaging | Filtering, morphology, segmentation, registration, calibration, detection, tracking, 3D/medical workflows and interactive analysis. | S4–S7 |
| T07 | Deep learning and reinforcement learning | Tensors, autodiff, model/layer interfaces, training/inference, optimizers, exchange formats, GPU backends and training tools. | S5–S7 |
| T08 | PDE and physical computation | Meshing, boundary conditions, finite elements, transient/steady solvers, multiphysics coupling and model/result visualization. | S4–S7 |
| T09 | Parallel and distributed computation | Parallel loops/futures, worker pools, distributed arrays, GPU arrays, scheduling, clusters and remote job recovery. | S5–S7 |
| T10 | Robotics and navigation | Kinematics, dynamics, motion planning, SLAM, localization, sensor fusion, ROS integration, UAV and lidar workflows. | S7 |
| T11 | Wireless, communications and RF | Modulation/coding, channel models, standards-specific waveforms, antennas, RF systems, radar, phased arrays and satellite links. | S7 |
| T12 | Automotive and aerospace | Driving scenarios, vehicle/powertrain models, flight models, navigation, calibration and domain visualization. | S7 |
| T13 | Finance and econometrics | Time-series models, pricing, portfolio/risk methods, instruments, data providers and reproducible backtesting. | S7 |
| T14 | Biology and scientific domains | Bioinformatics, biological systems models, geospatial/mapping, text analytics and specialist data import. | S7 |
| T15 | Acquisition and instrumentation | DAQ, instrument protocols, image acquisition, industrial connectivity and hardware support packages. | S7 |
| T16 | Deployment and embedded systems | Standalone apps, libraries/services, web app hosting, embedded C, fixed point, HDL, PLC and board-specific integration. | S5–S7 |
| T17 | Verification and engineering lifecycle | Requirements traceability, model testing/coverage, static analysis, architecture models, reports and tool qualification evidence. | S7 |
| T18 | Teaching and assisted workflows | Course/exercise tooling, guided tasks, optional coding assistance and agent integrations with swappable providers. | S7 |

This domain map groups the [official MathWorks product catalog](https://www.mathworks.com/products/alphabetical.html). S0 must map every product in the frozen catalog to an owner and scope record, including remaining products, blocksets, service features and support packages. A grouping table alone is not proof of full product coverage.

A symbolic engine, robust optimizer, physical solver and certification program each require their own specialist plan. Similar function names or a GUI wrapper around a different solver do not establish equivalent behavior. Hardware support and qualification require actual devices and external evidence; ordinary automated tests cannot substitute for them.

## Simulation and model based design

| ID | Capability | Implementation requirements and acceptance | Stage |
| --- | --- | --- | --- |
| S01 | Diagram editing | Library browser, canvas, ports, wiring, routing, subsystems, annotations, masks, undo/redo and parameter inspectors. | S6 |
| S02 | Model representation | Typed ports/signals, dimensions, parameters, hierarchy, references, variants, buses and versioned native storage. | S6 |
| S03 | Execution scheduling | Sample times, multirate scheduling, direct feedthrough, algebraic loops, initialization and deterministic ordering. | S6 |
| S04 | Continuous solvers | Fixed/adaptive steps, ODE/DAE support, tolerances, stiffness, zero crossings, resets and diagnostic events. | S6 |
| S05 | Discrete and hybrid systems | Delays, enabled/triggered subsystems, mixed continuous/discrete state and reproducible event ordering. | S6 |
| S06 | Blocks and observation | Math, sources, sinks, state, routing, control and signal libraries; scopes, logging and simulation inspection. | S6–S7 |
| S07 | MATLAB integration | Shared variables/parameters, function blocks, callbacks and declared language subsets. | S6–S7 |
| S08 | Simulink model import | `.slx`/`.mdl` structure and parameters, explicit unsupported-block reports and output/time comparisons. | S6–S7 |
| S09 | State machines | Hierarchical/parallel states, guards, transitions, actions, history and temporal logic. | S6–S7 |
| S10 | Physical modeling | Equation-based components, units, constraints, consistent initialization, coupling, electrical/mechanical/fluid domains. | S7 |
| S11 | Discrete events | Queues, resources, entities, event calendars and reproducible stochastic simulation. | S7 |
| S12 | Interchange | FMI/co-simulation and Modelica integration as separately tested standards targets. | S6–S7 |
| S13 | Generated deployment | Supported model subset to executable code; traceability, fixed point, target integration and verification. | S7 |
| S14 | Real-time and industrial testing | Scheduling deadlines, I/O adapters, hardware-in-the-loop and target-specific performance evidence. | S7 |

First simulation acceptance models: a discrete filter, a continuous oscillator, a feedback-controlled plant, a triggered subsystem and a model with a zero crossing. Compare time grids, trajectories within stated tolerances, event sequences and initialization behavior. More complex physical modeling follows solver and event correctness.

## Improvements beyond compatibility

Keep improvements compatible with familiar default workflows:

- Reproducible project environments and dependency lockfiles.
- Visible compatibility reports before migrating a project.
- Actionable errors with links to the relevant source and local documentation.
- Responsive inspection of large arrays and independently restartable kernels.
- Auditable numerical backends, published correctness/performance reports and explicit precision settings.
- Straightforward command-line automation, headless plots and portable project archives.
- Rust embedding and extension APIs alongside existing scientific workflows.
- Transparent package ownership, licenses, source links and reproducible releases.

The measurable promise is progressively broader tested compatibility and better results on specific usability/performance measures. “Everything, exactly, always faster” remains an aspiration rather than a release claim.
