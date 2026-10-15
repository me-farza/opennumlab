# Installed CLI acceptance

Run this after the release owner has assembled a clean candidate. The harness
uses Python's standard library and launches only the bundle's `mathlab` CLI.
It does not build the application or invoke MATLAB, the desktop or the launcher.

```sh
python3 -B scripts/qa-installed.py \
  --bundle /absolute/path/to/NumJinn.app \
  --report-dir /absolute/path/to/qa-reports \
  --octave /absolute/path/to/octave-cli
```

`--octave` is optional; without it, the CLI uses its normal runtime discovery.
An explicit symlink/launcher path is forwarded intact as an absolute path;
canonical paths are used for runtime identity comparison and hashing.
`--timeout` bounds each complete process group and defaults to 90 seconds.
Output is also bounded. Each invocation creates a new report subdirectory and
preserves existing files. The command prints its `report.json` path and returns
zero only when every required check in this CLI subset passes.

Before execution, the harness requires a clean frozen source identity, matching
source/payload hashes and modes, final checksums, both packaged executable
identities, and every advertised native adapter. These are consistency checks;
they do not authenticate a signature or independently audit source authorship.

The commands run in an owned temporary project outside the checkout and app,
with temporary home/cache/runtime-output directories. Retained logs, original
QA sources, the installed manifest/checksums, PNGs and the example's MAT file
remain in the report directory. Temporary execution files are removed. Neither
the app's example files nor existing user projects are edited.
Every report also identifies the executed QA script by path and SHA-256 and
records the Python executable and version used to run it.

The report records actual results for:

- Packaged CLI version, doctor readiness, runtime identity and an explicit
  missing-runtime diagnostic.
- An original linear-system/reshape/forward-local-function script and an error
  mapped to original source line 2.
- A persistent JSON REPL session whose value changes from 41 to 42.
- Plot property/data assertions and PNG signature, chunk CRC and dimensions.
- A byte-identical copy of the bundled getting-started example, including its
  known solution, figure and same-runtime MAT save/load assertions.
- Exact Unicode/whitespace, caller-scope expression and empty-string stdin
  replies when the bundled native input adapter is included and activated.

If native input is omitted, that check stays `not_run` with an unsupported-profile
reason. It never becomes native evidence. MAT evidence is explicitly an Octave
self-round-trip, not MATLAB file interchange. Intentionally failing commands
retain their nonzero exit codes; the corresponding diagnostic check can pass.

Every report keeps `release_qualified: false`. Desktop/UI, Stop and recovery,
debugger, read-only Resources/relocation, clean-machine installation and lifecycle
journeys remain explicit required `not_run` entries. A successful CLI subset
does not close those release gates. POSIX process-group cleanup does not cover
foreign processes that deliberately detach into another session.

Test the harness without a real runtime or application:

```sh
python3 -B scripts/qa-installed-tests.py
```

Those tests use temporary fake executables. They validate the harness's checks
and failure handling; they are not application, numerical-engine or native-ABI
acceptance evidence.
