# OpenNumLab headless Rust CLI candidate

This package contains the normal `mathlab` executable for the recorded host target.
Its explicit Rust program route does not start or require Octave, MATLAB or the GUI:

```sh
/path/to/package/bin/mathlab --json run --rust-program --profile class-v1 --workspace entry.m
```

Classes use explicit immutable inputs: append `--module-file MyClass.m` for each
class definition. At most9 sources,64KiB/file and256KiB total are admitted. There is
no ambient class-path search. Workspace object values are metadata-only; numeric
views are bounded previews. Use `--export-binding NAME --export-file NEW_FILE` for
complete lossless dense or CSC values. Existing destinations are never overwritten.
Examples can be copied to a writable project; the installed package need not change.
The included `share/examples/opennumlab-cli/rust-least-squares.m` exercises the bounded
4096x32 complex,8-RHS noisy fit. The complete output's independent source-pinned oracle
is available in the corresponding project source archive.

Plain run/REPL retain their existing legacy defaults and can require external Octave.
This package does not change those defaults or include that runtime. Choose the
explicit Rust flags above for the advertised Rust path. The smaller Rust REPL and
other scientific profiles have different capability limits; they are not aliases
for the class program profile. There is no automatic fallback to another interpreter.

The bounded profile includes located functions/loops, value and handle classes,
complex N-D dense values, CSC construction/products, square LU and full-column-rank
tall Householder least squares. Numerical rank refusal is project policy, not MATLAB
rank-warning/basic-solution parity. Underdetermined/minimum-norm/rank-deficient success,
all extreme ranges, char/persistent/object-array/custom-indexing/overload behavior,
general debugger/input/graphics/simulation and the wider language/numerical/desktop
roadmap are not claimed complete. See the corresponding `docs/rust-least-squares.md`
for exact QR shape/rank/range policy. Every operation remains subject to fixed
primitive/transition/storage budgets; exceeding a limit refuses safely rather than
silently executing a prefix through another engine.

JSON schema4 emits ready and outcome with a source-graph hash and request key. Check
command exit, status, reason, errors and completeness before consuming workspace or
exports. Exit130 means SIGINT/UserStop;124 means execution deadline. Errors/resource
refusals and export failures normally use1; malformed CLI arguments are reported by
the argument parser. `--timeout SECONDS` bounds setup/execution, excluding final
encoding/export as specified by the profile. A cancelled or failing script preserves
its completed publication prefix; a subsequent CLI invocation is a fresh session.

The release manifest pins target/toolchain/default cfg, binary, docs, target-specific
original dependency notices and verified corresponding-source archive. This is unsigned
candidate consistency metadata, not authentication, notarization or installed/platform
acceptance. System libraries remain external; only separately tested platforms can be
advertised. Python/Rust toolchains are build/QA prerequisites, not normal CLI runtime
requirements. Extract the source archive outside the checkout to run
`scripts/qa-headless.py --bundle PACKAGE --output NEW_EVIDENCE_DIR`; it uses existing
bounded installed Runner and creates only owned projects and relocated copies. That
QA performs the complete independent fit oracle and real process cancellation; probe
build evidence cannot substitute for default-build latency/layout/resource acceptance.
