# Headless tall least-squares preview

Use the explicit Rust class profile:

```sh
mathlab --json run --rust-program --profile class-v1 --workspace \
  --export-binding ls_validation --export-file validation.array \
  examples/rust-least-squares.m
```

This selected capability extends dense double real/complex `A\B` when A has more
rows than columns, at least one column, and B has matching rows and positive RHS
count. Results are in the ORIGINAL column order. Square inputs keep the existing
LU route. Legacy/scientific-v1/v2 APIs/dispatch and their quotas are unchanged.
There is no Octave retry, normal-equation production solver or alternative evaluator.
Public qr/decomposition objects are not yet exposed.

The original cooperative provider recomputes pivot norms, constructs complex
Householder reflectors, rotates diagonal phases to real positive values, transforms
all RHS and performs triangular backsolve. Per-vector maximum scaling protects
norms and reflector formation; input RHS values stay in their original scale.
There is no global B/A normalization or potentially overflowing output scale factor.
Common-scaled subnormal inputs are not banned; real/complex elementary operations
retain IEEE gradual underflow, with additive error floors in the analysis. Actual
overflow/nonfinite critical work refuses ArithmeticBreakdown rather than publishing
invented values. Broader extreme-range behavior remains a qualification to verify,
not an automatic MATLAB parity claim.

The project numerical-rank gate is explicit: relative pivot norm must exceed
64*max(m,n)*2^-53 of the original maximum column norm. Equality/zero/doubtful rank refuses Singular with no new X.
This is a conservative project policy, not a universal mathematical-rank certificate
or MATLAB's rank-warning/basic-solution behavior. Underdetermined/minimum-norm,
rank-deficient successful solutions, sparse least-squares, single/integer input
classes, empty tall variants, public QR options and automatic condition diagnostics
remain unsupported requirements. Nonfinite inputs refuse NonFiniteInput.

The existing64MiB live/16MiB value,1B primitive and5M transition bounds apply.
Input bindings, owned operand copies, provider scratch and candidate output are
admitted together; every actual reserve goes through the scoped hook and exact
capacity check. Preparation/norms/swaps/applications/backsolve poll and yield in
bounded chunks. Stop/fault/drop keeps completed caller publications and never
publishes an unfinished result. No allocator or system-call hard timeout/global OOM
recovery is claimed. Provider diagnostics expose charged units/rank-threshold/pivot
ratio separately; old CLI solve-growth fields still describe LU and are not filled
with fake QR pivot-growth values. Overall CLI primitive/reservation/live metrics
include the new operation.

The example is an original4096x32, eight-RHS noisy-fit family. It uses bulk row
doubling and broadcasting; its globally orthogonal noise weights1..127,-8128 detect
incomplete-row minimizers under the documented independent certificate. It exports
all A/B/X/residual words in one complex vector of shape196864x1 (3149824 bytes),
through the unchanged lossless array format. The exported residual sign is AX-B.
`tests/least_squares_oracle.py` binds source/outcome/export identities and uses exact
Fraction equations to verify every word, true stationarity/forward/objective and
rounded residual. This is not an output-fitted tolerance or a manufactured-X solve.

Execution/compatibility/GUI support claims require their separate recorded acceptance.
Headless numerical work has priority; actual desktop/installed/publication checks
remain separate. SVD/Cholesky/minimum norm/sparse factors, broad numerics, language/
classes and simulation remain required parts of the full goal.
