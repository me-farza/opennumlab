# OpenNumLab

*Scientific computing, freely.*

<img src="assets/opennumlab-logo.png" alt="OpenNumLab: a golden lamp releasing a blue mathematical surface" width="480">

OpenNumLab is an open-source, **Rust-based scientific desktop and command-line application with GNU Octave as its execution engine**. It is an early alpha intended to grow into a familiar environment for MATLAB users. The desktop, session management, and CLI are written in Rust; GNU Octave and its numerical dependencies are not Rust implementations.

The project was previously called NumJinn and Open MathLab. Existing `mathlab` commands, `mathlab-*` crates and `MATHLAB_*` environment variables remain stable during this display-name change. Development uses the private [me-farza/opennumlab repository](https://github.com/me-farza/opennumlab).

This repository implements the first integrated foundation. It does **not** provide the complete MATLAB/Simulink ecosystem, establish exact MATLAB compatibility, or demonstrate a speed advantage over MATLAB. GNU Octave describes its language as mostly compatible with MATLAB; compatibility still needs to be assessed per project and API variant. [GNU Octave](https://octave.org/about)

## Start it

Install a stable Rust toolchain, GNU Octave, and a graphics toolkit. On macOS with Homebrew:

```sh
brew install octave gnuplot
cargo run -p mathlab-cli -- doctor
cargo run -p mathlab-desktop
```

Run `./scripts/setup.sh` for platform guidance. That script prints commands and reports installed tools; it does not install anything. On Linux, the desktop also needs the development libraries listed by the script. The application finds `octave-cli` or `octave` on `PATH`, recognizes common Homebrew locations, or uses `MATHLAB_OCTAVE=/absolute/path/to/octave-cli`.

To open a specific script in the desktop:

```sh
cargo run -p mathlab-desktop -- --open examples/getting_started.m
```

## Familiar workflow

The desktop uses the names and arrangement familiar to MATLAB users: Current Folder, Editor, Command Window, Workspace, Command History, and Figures. Write MATLAB/Octave source in the Editor, run it in one persistent Octave session, inspect and edit matrix pages in the Variable Editor, and use the Command Window to continue working with the same variables. Numeric and logical 2-D values, including sparse matrices, support class-preserving scalar edits. Other supported containers have bounded read-only previews. **Apps → Packages** opens the installed package list with versions, descriptions, and a Load button; packages loaded there become available in that desktop session.

| Action | Shortcut |
| --- | --- |
| Run script / continue paused execution | F5 |
| Toggle breakpoint while idle | F12 or click the line gutter |
| Step / step in / step out | F10 / F11 / Shift+F11 |
| Quit debugging | Shift+F5 |
| Run current `%%` section | Ctrl+Enter |
| Run selection | F9 |
| Find / replace | Ctrl+F / Ctrl+H (Cmd on macOS) |
| Go to line | Ctrl+G (Cmd+G on macOS) |
| Open a script | Ctrl+O (Cmd+O on macOS) |
| Save a script | Ctrl+S (Cmd+S on macOS) |
| Send EOF to an active input prompt | Ctrl+D |

The startup Help view describes the available controls. This alpha uses figure image previews exported by Octave. Full MATLAB figure editing, callbacks, graphics interaction and docking remain development work. Full-script runs prepare MATLAB-style local functions, including calls before their definitions, private sibling calls and retained handles. Source errors map back to the original file; clickable Command Window frames navigate saved, dirty and recovered buffers. This preparation has documented introspection and path limitations; see [the language adapter](crates/mathlab-language/README.md). Selection/section execution and user-issued `run`/`source` calls bypass it. Set Current Folder or `--directory` to the project folder when scripts reference sibling files.

The optional native scope adapter preserves named local persistent state through load-path changes, package loads and edited source generations. Build it with `bash scripts/build-debugger.sh` and set `MATHLAB_SCOPE_BRIDGE="$PWD/native/mathlab_scope_bridge.oct"` before starting OpenNumLab. The interpreted fallback remains available, with a documented path-change rebinding gap. Both modes reset the local registry on `clear functions`/`clear all`; clearing variables alone preserves it.

The Command Window receives live console bytes, including partial lines and inherited subprocess stdout/stderr, while a calculation is running or the session is idle. The desktop retains at most 16 MiB of console text and 4,096 entries; slower consumers receive an explicit output-loss notice. Final execution results do not append that output twice. This is common-pipe byte order: user buffering and concurrent background processes can still delay or interleave writes.

The optional native input adapter connects genuine `input()` reads to a separate reply field in the desktop and stdin replies in the CLI. Build the adapters with `bash scripts/build-debugger.sh`, then set `MATHLAB_INPUT_BRIDGE="$PWD/native/mathlab_input_bridge.oct"` before launching. Expression replies evaluate in the actual caller's scope; `input(prompt,'s')` preserves the entered text. An empty reply and EOF are distinct. Replies are single UTF-8 lines up to 1 MiB; multiline paste receives a diagnostic. The desktop preserves the normal command draft and history while replying. The macOS bundle built with `--with-debugger` enables all four bundled adapters automatically.

Input must be enabled before the first user execution because previously captured original `@input` handles cannot be safely redirected. Without the adapter, `input()` can wait until Stop instead of receiving a frontend reply. The adapter delegates behavior to Octave: invalid input expressions raise an error after one read, rather than automatically reprompting as MATLAB does. See the [input contract](native/INPUT_BRIDGE.md) for tested behavior and limits.

**Stop preserves the workspace on Unix/macOS** by interrupting execution and recovering the same Octave process. Already received console output remains visible. Native routines may defer interruption. Force Restart, ordinary Restart, and automation timeouts discard the workspace; existing figure previews refresh after the next successful execution. Native Windows engine startup is explicitly unsupported until helper argument dispatch and cancellation have been validated there. Script and command execution has the same access to local files and programs as a normal Octave session.

Tabs, unsaved source, cursor positions, history, directory and preferences recover from the startup project's `.mathlab/session.json`. Recovery does not serialize the numerical workspace; save variables to MAT files for that. Screenshot runs skip recovery unless `--state FILE` is supplied. Dirty and unsaved script buffers execute without overwriting their saved files.

## Use the command line

```sh
cargo run -p mathlab-cli -- doctor
cargo run -p mathlab-cli -- eval 'A = [3 1; 1 2]; b = [9; 8]; x = A\b;'
cargo run -p mathlab-cli -- run examples/getting_started.m --workspace
cargo run -p mathlab-cli -- repl
cargo run -p mathlab-cli -- packages
cargo run -p mathlab-cli -- packages --load signal --json
cargo run -p mathlab-cli -- check compatibility/core.json
cargo run -p mathlab-cli -- doctor --json
```

The CLI supports `--octave PATH`, `--directory DIRECTORY`, `--json`, and `--timeout SECONDS`. Compatibility checks have a 30 second deadline per fixture by default. `eval` and `run` each create a new session. `repl` keeps variables between commands and supports `:workspace`, `:clear`, `:restart`, `:cd PATH`, `:run FILE`, and `:quit`; execution deadlines also apply to REPL evaluations. With the input adapter enabled, one stdin reader routes lines to active input requests or REPL commands, preserving empty lines and whitespace. Terminal replies use terminal echo; piped replies use native echo. Human-readable commands stream raw console bytes; JSON execution returns the bounded aggregate with truncation and decoding-loss fields. JSON REPL emits records without prompts or a banner, and input replies still come from stdin. `packages --load NAME` verifies that an installed package can be loaded in that command's session; packages loaded there do not persist into a separate `eval` or `run` process. Figures from CLI execution are preserved in `.mathlab/figures/` inside the selected working directory.

## Try the examples

| Script | Demonstrates | Prerequisite |
| --- | --- | --- |
| `examples/getting_started.m` | Solve a linear system, plot a damped oscillation, save/load a Version 7 MAT file | Core Octave + graphics toolkit |
| `examples/signal_fft.m` | Two-tone signal, normalized FFT, two-panel plot | Core Octave + graphics toolkit |
| `examples/control_response.m` | Transfer function and first-order step response checked against an analytic result | Optional `control` package + graphics toolkit |
| `examples/local_functions.m` | Forward local calls, private scopes, retained function handles and nested closures | OpenNumLab script preparation |
| `examples/debugging.m` | Break at line 4, inspect variables, step and continue | Optional native debugger adapter |
| `examples/interactive_input.m` | Exact string replies, caller-scope expressions, empty reply and a resulting plot | Optional native input adapter |

The getting-started script saves its MAT file under a temporary path and prints that path. The examples contain assertions so incorrect numerical results become explicit failures. The control example loads an already installed package; it does not install one. The suggested optional profile includes `struct`, `datatypes`, `control`, `signal`, `image`, `statistics`, and `optim`; `./scripts/setup.sh` prints installation guidance. Packages can require native build tools and should be loaded when needed rather than all at startup. Optional packages extend Octave functionality without establishing equivalent coverage of MathWorks toolboxes. [Octave package catalog](https://packages.octave.org/)

To try the optional debugger with the runtime it was built against:

```sh
bash scripts/build-debugger.sh
MATHLAB_SCOPE_BRIDGE="$PWD/native/mathlab_scope_bridge.oct" cargo run -p mathlab-desktop -- --debugger "$PWD/native/mathlab_debug_bridge.oct" --open examples/debugging.m --run --break-at 4
```

The Debugger window shows the paused original file/line, Call Stack and Local Workspace. Continue, Step, Step In, Step Out, Quit and scoped evaluation use an independent control channel while the main execution waits. User evaluations stream output and support the same input replies when the input adapter is enabled; automatic variable previews remain quiet. Debugger evaluations capture the first 1 MiB of observed raw output, separately from live console retention; UTF-8 replacement can expand the returned text. Saved and unsaved script buffers support breakpoints in script statements, local functions and nested functions, including retained handles from older source generations. Ordinary function/class buffers require saving before breakpoint binding. Native saved-file breakpoints add the file's folder to the end of the session search path if needed. The selected Octave version and platform must match the compiled adapter ABI.

Click the editor's line gutter or press F12 to toggle a breakpoint while idle. Red markers show confirmed lines. A green arrow identifies the paused line when the editor exactly matches the executing source snapshot; edited buffers show a warning and the Debugger offers a read-only snapshot. Ordinary function/class files currently lack that snapshot. F5 and the ribbon Run button become Continue while paused; F10 steps, F11 steps in, Shift+F11 steps out, and Shift+F5 quits debugging. Run prepares the current buffer and confirms all its breakpoints before executing it, including after edits. A changed or closed buffer cancels pending execution. These function-key assignments follow the [MATLAB Editor shortcuts](https://www.mathworks.com/help/matlab/matlab_prog/editor-keyboard-shortcuts.html); platform-specific modifier aliases and breakpoint editing while paused remain future work.

Prepared breakpoints use Octave's public breakpoint table through aliases in the inactive runtime registry. Raw `dbstatus` exposes that helper's metadata; frontend locations map to the original source. Registration occurs before user source executes, so local functions and base variables may use `mathlab_scope_register`. Native setup rejects a foreign command-line helper or visible function file with that name. The genuine helper lives outside the native load path with an unlocked command-line binding. Reconciliation does not cover a single evaluation that clears all breakpoints, installs an ordinary breakpoint and immediately calls an old handle. These limits and the checked clear, parse-error and Quit recovery behavior are described in the [kernel readiness audit](docs/KERNEL_READINESS.md#checked-prepared-source-debugger-behavior). This is selected Octave evidence, not MATLAB debugger parity.

## Verify the foundation

```sh
./scripts/verify.sh
./scripts/verify.sh --require-octave
./scripts/verify.sh --require-octave --with-control
./scripts/verify.sh --require-octave --with-toolboxes
./scripts/verify.sh --require-octave --with-toolboxes --with-debugger
```

The script checks formatting, builds/tests the Rust workspace, and runs original Octave fixtures for array semantics, functions/classes/closures, dense/sparse linear algebra, FFT, declared MAT types, and SVG figure export. It also runs fixtures through the Rust adapter, including prepared local functions. Without a runtime, the default script explicitly skips Octave checks; `--require-octave` makes absence a failure. `--with-control` requires the optional control package. `--with-toolboxes` additionally checks signal filtering, control responses, binary image connectivity/morphology, normal-distribution values/regression, and least-squares fitting using the optional scientific profile. `--with-debugger` builds the optional native Octave adapter with `mkoctfile` and runs real paused-scope debugger proofs. CI installs Octave, gnuplot, fonts, and the scientific packages available from Ubuntu before running its checks. CI runtime/package versions can differ from the local development installation and remain visible in the logs.

[Compatibility manifests](compatibility/README.md) list the exact smoke-test scope. A passing fixture is evidence for its checked behavior on that runtime; it is not a percentage of MATLAB support. The [selected three-route conformance cases](compatibility/conformance/README.md) distinguish fresh MATLAB execution, saved reference replay, raw Octave and the actual application. Missing reference evidence remains explicit; the separate MATLAB investigation does not automatically qualify application behavior.

## Build a local macOS app

```sh
./scripts/package-macos.sh
open dist/OpenNumLab.app
```

The script requires clean committed source by default, exports an identified source snapshot and builds release binaries into private storage. It assembles a new unsigned Development alpha bundle with original examples, allowlisted corresponding project source, build/provenance records and the selected macOS Cargo dependency license texts. Raw research evidence and local validation artifacts are excluded. The notice inventory verifies retained exact-commit upstream notices, including bundled fonts; the recorded `dispatch` notice gap is described in `THIRD_PARTY.md`. GNU Octave and gnuplot remain separate installations. This is a local development package; installed-candidate qualification, signing/notarization and self-contained distribution remain separate gates.

The launcher copies editable examples into `~/Library/Application Support/OpenNumLab/Projects/Getting Started` and keeps generated output and recovery outside app Resources. Edited examples are preserved. A successful bundle replacement retains the previous app beside the destination; existing MathLab/NumJinn apps and project directories remain preserved. See the [alpha packaging contract](docs/release/DEVELOPMENT_ALPHA.md) for prerequisites, writable-project overrides, source policy, checksums and current validation limits. Build serially and allow enough free disk space for the private build.

Use `./scripts/package-macos.sh --with-debugger` to include all four native adapters (debugger, scope, input and value) built for the local Octave runtime. The launcher discovers them through `MATHLAB_DEBUG_BRIDGE`, `MATHLAB_SCOPE_BRIDGE`, `MATHLAB_INPUT_BRIDGE` and `MATHLAB_VALUE_BRIDGE`; a mismatched runtime still needs its own adapter build.

## Architecture and next steps

The Cargo workspace separates `mathlab-engine` (Octave process protocol and persistent session), `mathlab-language` (script-local function preparation and source mapping), `mathlab-debugger` (typed debugger transport), `mathlab-cli` (batch/REPL/diagnostics/conformance commands), and `mathlab-desktop` (native Rust UI). `octave/mathlab_worker.m` runs user source in the Octave base workspace and returns output, errors, workspace metadata, and exported figures. The optional native debugger adapter uses Octave's public event and breakpoint APIs; it must be built for the selected runtime ABI. See [native debugger development](native/README.md).

The [kernel readiness audit](docs/KERNEL_READINESS.md) describes the checked session contract and the remaining compatibility, resource-accounting and performance work. The [console contract](docs/CONSOLE_PROTOCOL.md) explains output bounds, interruption fences, and frontend completion barriers.

The next stages are robust debugger workflows, interactive graphics, modern MATLAB language and type gaps, a pinned toolbox profile, reproducible installers, reference-based language/file conformance, and later simulation integrations. Existing planning documents retain the full intended scope:

- [Public product roadmap](ROADMAP.md) — canonical task, dependency and release-profile plan.
- [Development plan](DEVELOPMENT_PLAN.md)
- [Functionality catalog](FUNCTIONALITY_CATALOG.md)
- [Integration strategy](INTEGRATION_STRATEGY.md)

The application and original examples/fixtures are licensed under **GPL-3.0-or-later**; see [LICENSE](LICENSE). Third-party components retain their own licenses and notices; see [THIRD_PARTY.md](THIRD_PARTY.md). This is an independent project and is not affiliated with MathWorks. MATLAB and Simulink names describe intended interoperability.
