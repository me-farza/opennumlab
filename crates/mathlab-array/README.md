# Scientific array profile (draft)

`scientific` adds original GPL-3.0-or-later Rust dense real, complex and logical
storage alongside unchanged `DenseDouble2d`/v1 kernels. It uses no new dependency,
Octave implementation, BLAS fallback or native source copy.

`ScientificShapeV1` admits 2–8 supplied axes, at most 2^20 elements, with checked
column-major strides. Trailing dimensions equal to one normalize down to rank 2;
zero axes and interior ones remain. Each successful payload has at most 16 MiB
of exposed vector capacity. `ScientificDenseV1::read_range` borrows the complete
requested range as real, complex-component or logical slices; it never clips,
projects, allocates or silently drops imaginary components.

Every actual `Vec::try_reserve_exact` first calls
`ScientificControlV1::reserve(bytes)` for byte admission and a distinct fault
site. Returned vector capacity must equal the admitted capacity. Requested-byte
preflight and successful exposed-capacity accounting are not claims about the
allocator's physical size, overhead, transient failed reservations, RSS or global
OOM recovery. Copy/fill/validation futures suspend and call their control checkpoint before
each chunk of at most 128 scalar units. Synchronous convenience methods drive
those same state machines without heap allocation. A callback error in a borrowed-input copy or solve leaves those borrowed inputs
intact and drops private candidates. Consuming reshape and into_complex variants
take ownership; error or dropping their future may give up that owned input.

Complex64V1 add/sub/mul expose raw IEEE binary64 arithmetic and can produce
nonfinite results; they do not themselves return ArithmeticBreakdown. Checked
division and the solver provide the checked failure policies described below.
Complex division independently scales each numerator product term and quotient
component. It does not normalize small numerator components by a neighboring
huge component. It handles subnormal inputs, gradual underflow, division by one
or i and extreme finite component ranges. Rounded dot/cross cancellation is not
an exact-rounding promise. Nonfinite division input is `NonFiniteInput`, zero
denominator is `Singular`, and generated nonfinite/unrepresentable arithmetic
is `ArithmeticBreakdown`. Regression cases and an independent edge corpus must
be executed before accepting these claims for release.

`ScientificSolveV1` performs right-looking elimination, row permutations and
back substitution on owned candidates. The first maximum of
`max(abs(real),abs(imag))` selects each pivot; ties keep the first row. This is a
specified component pivot metric, not MATLAB modulus-pivot equivalence.
Separate products/additions and fixed traversal order are used. `step` accepts
128–32768 charged units, checks its callback before each mutation stage, and
returns pending/complete. Division stages charge 128 units. Initial and maximum
U component metrics, scalar work and the resulting growth factor are exposed.
No universal GEPP stability, correctly rounded result, or native bit parity is
claimed. Singular/nonfinite/breakdown cases are explicit failures.

The independent `tests/scientific_oracle.py` checks every binary64 component in
a complete 128×32×8 export using exact Fractions. Each page has a different
manufactured solution. It constructs analytic tridiagonal A/B/X independently,
checks rectangular component-L1 infinity norms and requires backward error
≤2^-40 and relative forward error ≤2^-36. The A-family condition bound ≤5 is a
family proof, not a general solver waiver. The scalar `norm_inf` diagnostic uses
specified sequential nonnegative additions; it is not the legacy exact sum
kernel or the independent rational oracle.

The cooperative constructors/copies/solve preparation return allocation-free Rust
futures; the default control checkpoint returns Pending before each <=128-unit
CPU block. A host overriding checkpoint must preserve its required cancellation,
charging and suspension policy. Synchronous convenience wrappers drive these
same futures to completion on the caller stack; they do not provide an executor
turn between blocks. Program integration must await the cooperative variants.
Dropping a preparation future releases its private candidate vectors. Borrowed
copy/solve inputs remain unchanged; consuming into_complex_cooperative takes
ownership and a dropped/erroring future may relinquish that input. Reshape also
consumes its input. This array API has no workspace publication or request
key, and makes no claim about program frame/layout equivalence. Those are
separate engine integration gates. Allocation-system-call latency and arbitrary
caller control/waker code are outside the CPU quantum guarantee.

The current array test source also binds all 64 independently reviewed complex
division records and twelve independent tiny solver systems. The division policy
requires exact words only for its selected identity/subnormal/structured cases,
numerical zero with either sign where specified, six whole-operation breakdowns,
and exact-rational component-L1 normalized residual <=64u for nine selected
finite cases. It is not a universal correctly-rounded division promise. For the
eight nonsingular systems, exact-rational backward error must be <=64*n*u;
S01-S07 also use the independently certified condition bound in
2*kappa*eta_limit/(1-kappa*eta_limit), with a positive denominator. S04/S06/S07
add the <=2^-36 forward threshold. S08 has no tight forward guarantee. S09 is
Singular; S10-S12 are NonFiniteInput. These are prospective acceptance criteria,
not results from running Rust.

Growth diagnostics expose raw f64 initial/max-U component metrics. The array
API growth_factor() returns Some(+Inf) when their ratio overflows and None when
the initial metric is zero; ratio overflow does not fail a successful solve.
Hex-word output and an explicit ratio classification belong to the later CLI
codec and remain a separate integration gate.
