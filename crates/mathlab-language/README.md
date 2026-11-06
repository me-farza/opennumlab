# MATLAB script local functions

`prepare_script(path, source)` recognizes MATLAB script-local function definitions
using a lexer and a block stack. It leaves ordinary function and class files
unchanged. For scripts, it removes each outer local function by replacing its
bytes with whitespace and creates one padded function source per definition.
Comments, character/string literals, transposes, matrix/cell expressions, indexing
`end`, continuation lines, multiple outputs, control blocks, and nested functions
are handled without rewriting calls or identifiers.

`PreparedScript::source_kind()` uses the same lexer to report `SourceKind::Script`,
`Function`, or `Class` from the first executable token, skipping leading comments
and a UTF-8 BOM. Extracting local functions does not turn a script into a function
file. The engine uses this classification when an unsaved debug buffer must be
saved before native function/class breakpoints can be bound.

`PreparedScript::write_to(unique_session_directory)` writes a uniquely named
`.mlsource` script and `private/name.m` function files. Octave may otherwise parse
a same-named original in the current directory while resolving private scope.
Octave's `source` accepts non-`.m` source files, while load-path function lookup
ignores them: the generated script cannot be called globally by its name. The
engine retains that script directory at the end of Octave's load path. The
private directory itself must **never** be added to the path.
All files must survive unchanged until session shutdown so returned `@local`
handles can still call their siblings and local `persistent` state survives
repeated script execution. Every returned `FileMapping` points a
generated filename to the original source; line and column numbers are retained
by padding. Materialized paths are canonicalized to match Octave stack frames,
which may resolve `/tmp` to `/private/tmp` on macOS. Reuse the same generated
directory for unchanged source; new source versions need new directories to
preserve existing handles. Removing the parent from Octave's path or deleting and
recreating a cached script can evict its private scope, resetting local
`persistent` state on a later dispatch. Keep cached `.mlsource` files immutable;
do not delete/recreate them between requests or add artificial `mlock` calls.

Octave can also release private function scopes when their last handle disappears.
The engine therefore registers local handles in an inactive transport helper
outside the base workspace before executing the script. It does not inject a
registration statement or temporary variable into user source. The portable
backend uses a return-only, uniquely named factory `.m` in the generated capsule,
verifies the factory's exact canonical identity before calling it, and retains the
returned private handles. This internal factory can appear in raw global lookup;
user local numerical names remain private. The optional native adapter creates
scoped handles through exact private-file parsing without a factory call.
The registry retains each generation's first bindings until `clear functions`,
`clear all`, or restart; clearing workspace variables alone preserves state.
`scope_registration_offset()` remains a lexical helper for callers, but the engine
no longer uses source insertion. Full original snapshots are retained separately
as `.mlsnapshot` files for exact old/new debugger source display.

This strategy gives forward definitions, separate local-function workspaces,
local precedence over same-name global functions, base-variable precedence, and
script-specific private scopes. Actual Octave tests cover two saved originals
with the same basename, retained handles after path restoration, siblings,
nested closures, persistent state, and hidden generated script names. The default
fallback uses Octave's existing private-function resolution. The optional native
adapter adds explicit per-file local bindings; both strategies retain the original
user call identifiers and keep local names out of global lookup.

## Optional native scope bindings

The integrated [`mathlab_scope_bridge`](../../native/SCOPE_BRIDGE.md) binds the
registry's retained function objects to the canonical `.mlsource` script and
every extracted outer function file through Octave's public
`symbol_table::install_local_function` API. This avoids the private-cache
rebinding deviation after load-path changes, package loading, or switching source
generations. Sibling calls and nested closures keep their original scopes; base
variables retain their native precedence.

Build using the SDK belonging to the selected numerical runtime:

```sh
bash scripts/build-debugger.sh
MATHLAB_SCOPE_BRIDGE="$PWD/native/mathlab_scope_bridge.oct" \
  cargo test -p mathlab-engine --test octave_session
```

`scripts/build-debugger.sh` currently builds both native adapters. The engine
activates scope binding through `Engine::enable_scope_bridge(path)` or
`MATHLAB_SCOPE_BRIDGE` at startup. It validates the `mathlab-scope` version 2
protocol and its pre-entry/reflection/source-entry capabilities, retains the selected adapter path across restart, and reapplies runtime
configuration after `clear functions`/`clear all`. Those clear operations still
reset local persistent state; Clear Workspace preserves it. Without explicit
activation the interpreted registry fallback remains in use. A requested invalid
adapter produces a diagnostic rather than silently claiming native behavior.
The native adapters retain the canonical internal registry in Octave's public
command-line function table, preserving its cells through path-cache refreshes
without `mlock`. Ordinary `clear functions`/`clear all` removes it; the worker
reinstalls/configures a fresh helper on the next entry. A foreign same-name global
helper is rejected, and no local numerical function name is installed globally.

Real engine regressions distinguish these modes explicitly and check optional
native named-call state through path changes, installed package loading,
edited-generation switches, variable/function clears, sibling calls, nested
closures, base-variable precedence, and restart. These are tested Octave cases,
not comparisons against a licensed MATLAB build. Build and test each runtime ABI.

Current limitations:

- With the native scope adapter, outer/nested function `mfilename`, source-derived
  relative resources, outer `functions(handle).file`, and cached function text use
  the original path/snapshot. Immutable lookup keys remain generated. Native nested
  `functions(handle).file` remains empty. Without the adapter, function introspection
  exposes generated paths. Error frames are mapped conservatively when a caught
  reflected stack lacks enough generation/name information.
- Native source entry reflects script `mfilename` and source-derived resources to
  the original path; the interpreted fallback exposes the generated basename/path.
  Session implementation directories appear in `path()` so private function caches can survive. User
  `restoredefaultpath` or removal of those directories resets the caches and may
  prevent later local calls in the same script from resolving.
- In the interpreted fallback, changing the load path (including adding another generated source generation)
  can cause Octave to rebind a later named private-function lookup to a fresh
  instance. Its persistent state then restarts even though a retained old handle
  keeps its original state. The registry fixes consecutive runs and workspace
  clearing; it does not emulate MATLAB's full lookup behavior across path changes.
  The optional native scope adapter addresses the checked named-call cases.
- In the interpreted fallback, relative resources obtained from a local function's
  `mfilename('fullpath')` need an explicit original project path.
- Local classes and function definitions inside control blocks are rejected.
  Scripts require explicit ends for every local function. Ordinary function
  files keep Octave's own parsing behavior.
- Octave `localfunctions` reports function-file context, so script-local
  `localfunctions` enumeration and native help/reflection metadata are not
  emulated here. Prepared debugger source mapping uses a separate optional
  adapter. Its enabled engine tests cover saved/unsaved script, local, nested,
  and cold retained-generation pauses; error-frame mapping alone does not enable
  it. Raw `dbstatus` still exposes registry-host metadata. See the
  [checked debugger contract](../../docs/KERNEL_READINESS.md#checked-prepared-source-debugger-behavior).
- Direct Octave `run`/`source` calls made by user code bypass engine preparation;
  only files submitted through the MathLab execution API receive this adaptation.
- Worker and registry helpers remain internal global runtime functions; a foreign
  global file shadowing the canonical helper is rejected. User local functions and
  base function-handle variables named `mathlab_scope_register` no longer intercept
  execution entry. Explicitly clearing the internal registry helper resets its
  retained bindings.

These checked adaptations do not establish complete MATLAB language equivalence.
See the
[kernel readiness assessment](../../docs/KERNEL_READINESS.md) for session resource
budgets, output/figure lifetimes, and current native-debugger boundaries.

Reference behavior: [MATLAB script local functions](https://www.mathworks.com/help/matlab/matlab_prog/local-functions-in-scripts.html)
and [Octave private functions](https://docs.octave.org/latest/Private-Functions.html).


## Rust literal-expression component

The explicit `literal-expression-v1` library component keeps the original UTF-8 source and byte spans while parsing signed integer spellings, parentheses, rectangular comma/semicolon matrix literals, `[]`, and binary `+`. `expression::lower_literal_expression_v1` produces admitted IR; `literal_evaluation::evaluate_literal_expression_v1` consumes that IR and returns an owned double array with its original AST/source. It composes the `mathlab-array` constructors and addition without invoking Octave.

This first profile admits integer magnitudes through 2^53 with at most 16 digits, one physical code line with surrounding line comments, arrays up to 16 by 16 and 256 elements, and the other resource bounds in the N01 contract. Project syntax/resource diagnostics and host allocation failures are distinct; leaf errors preserve their original context and code. These are project rules for an incremental language component, not MATLAB's complete syntax or native error identifiers.

The Engine, desktop and CLI still use their existing execution path. This library is not an application backend selection. Its original fixture predictions require affected Rust checks; fresh MATLAB source oracles and full N01/R03/K04/N03 acceptance remain separate. See `docs/research/matlab-r2026b/N01_LITERAL_EXPRESSION_HANDOFF_V1.md` at the repository root for the exact contract.


## Persistent Rust session constructor subset

The separate `session_expression` profile admits closed `zeros(n)` /
`zeros(m,n)`, `ones(n)` / `ones(m,n)`, and `eye(n)` / `eye(m,n)` builtins. Calls compose with its existing arithmetic, grouping and
transpose operators. Arguments are evaluated left to right; after all succeed,
scalar dimensions are validated left to right. Dimensions must be finite real
doubles and integral. Finite negative integers normalize to zero before the
positive extent cap of 16 and unsigned conversion. Negative fractions are rejected
with the project `Fractional` reason. Both signs of zero are zero extents.
`zeros` stores positive-zero words; `ones` stores exact binary64 1.0 words.
`eye` stores exact 1.0 on the main diagonal and positive zero elsewhere, including
rectangular identity geometry with min(rows, columns) diagonal entries.
The 0-by-n, m-by-0 and 0-by-0 shapes retain their distinct geometry. Construction reuses the array layout limits and fallible
payload reservation without a temporary result vector.

Bare `zeros`, `ones` and `eye` are ordinary binding reads. A workspace binding selects read indexing
for its matching call target; it leaves the other constructors callable. A bare unbound `eye`
returns `UnknownBinding`; default bare-eye syntax is outside this subset. After
ordinary parsing and admission, unsupported callees and invalid arity also fail
before argument evaluation. Dimension errors carry an argument span, reason,
shape and scalar value bits where present. These are project policies, including
rejection of size vectors and fractional dimensions; they do not assert MATLAB's native
error identifiers or ordering. N-D forms, options, general selectors and other functions remain outside this subset.

Calls and argument slots count toward the existing 1024-node admission cap, with
32 open delimiters, 256 number nodes and 4096 source bytes. Results remain at most
16 by 16, 256 elements and 2048 logical payload bytes after extent normalization. Session cancellation,
transactional publication and workspace quotas remain in force. These limits do
not bound aggregate process RSS. Existing literal, decimal and arithmetic one-shot
profiles retain their earlier grammar; the default application backend is unchanged.


The session profile also admits exactly one argument to `size(A)` and `numel(A)`.
`size` returns an owned double row of shape 1-by-2 containing the row and column
lengths; `numel` returns an owned double scalar containing the element count. They
inspect geometry, including for nonfinite or signed-zero payloads. Distinct empty
shapes remain visible through `size`; every empty input has positive-zero `numel`.
The session's 16-per-axis/256-element bounds make all conversions exact.

Arguments evaluate normally before queries run. Query dispatch precedes constructor
dimension validation; when a query result is later used as a constructor dimension,
the ordinary scalar/finite/integral/extent rules still apply. Both queries share
checked row/scalar construction, cancellation, ownership, ans and transactional
workspace behavior. Unbound builtin arity is checked before argument evaluation; bindings select
indexing before builtin arity checks. Bare `size`/`numel` remain ordinary variable reads/assignments.
`size(A,dimension)`, multiple outputs, overloaded query forms, other classes and
N-D data remain outside this subset. One-shot profiles and default dispatch retain
their existing behavior; no complete MATLAB shape-query compatibility is claimed.


Session integer ranges admit `a:b` and `a:s:b`, with default step +1. Colon binds
below sums and products: `1+1:2:2*4` is 2,4,6,8; `(1:3).'` transposes the range,
while `1:3.'` transposes only scalar 3. Operands may be existing expressions, names,
groups or calls. All operand expressions finish in source order before operands
are validated left to right as finite integral real-double scalars within inclusive
+/-2^53. Constructor negative-size normalization does not apply to range values.

Zero step (including -0) and incompatible direction return an owned 1-by-0 row;
its transpose is 0-by-1. Nonempty ranges have at most 16 elements, checked using
i128 differences, absolute step and cardinality before result generation or
reservation. The original evaluated start word is retained, including negative
zero; later words come from checked integer arithmetic and exact binary64
conversion, so computed zero is positive zero. No repeated floating addition or
unchecked floating count conversion is used. Selected native observations support
these bounded outcomes, not a general MATLAB floating-colon algorithm.

`RangeOperand` retains operand position/span, shape, reason and scalar bits;
`RangeLength` retains the exact requested count and row cap. These are project
diagnostics/order. Each colon charges a logical node; source/depth/number bounds,
fallible checked row construction, cancellation and atomic publication still
apply. Fractional/wider operands, standalone colon, general selectors, `colon(...)`, extra
colons and ranges inside numeric-only matrix literals remain excluded. Existing
one-shot profiles reject colon, and default/desktop dispatch remains unchanged.


The session profile admits exactly `reshape(A,m,n)` for existing rank-2 doubles.
All three expressions evaluate left to right before validating scalar dimensions
m then n (original argument indices 1 and 2). Dimensions must be finite integral
values from 0 through 16. Negative reshape dimensions are rejected rather than
normalized; negative fractions report `Fractional`, negative integers `Negative`.
Both axes are validated even for empty input, and both signs of zero mean zero.
The checked column-major import requires matching element counts, preserves every
payload bit (including signed zero, Inf and NaN), and creates independent storage.
Count mismatches retain the original Construct/StorageLengthMismatch array cause
at the full call span. Empty reshapes preserve their requested axis geometry.
Binding-first indexing, cancellation and atomic publication still apply;
bare `reshape` remains a binding read/assignment. Inferred `[]`, size vectors,
extra dimensions and overloads remain unsupported. Selected native observations
qualify only their explicit sources; custom host-seeded NaN tests verify Rust copy
semantics and do not establish native custom-payload parity.


The session profile admits reads `A(P)` and `A(R,C)` from identifier-bound rank-2
real double arrays. A nonempty selector is a scalar, row or column vector with
at most 16 words; zero-word rank-2 selectors are also admitted. A binding selects indexing before builtin allowlist/arity checks,
including all six builtin names; indexing failures never fall back. Exactly one
or two arguments are required before complete child evaluation left to right.
Per argument, validate shape, then every word's domain in column-major order
(finite, integral, strictly positive), then every bound before the next argument.
Scalar diagnostics and their domain/bound order retain the accepted behavior.

Linear indices are one-based column-major positions up to 256. Matrix and scalar
targets take selector geometry; a nonscalar row/column target vector preserves its
own orientation for vector selectors. For empty linear reads, only a `1x0` or
`0x1` selector inherits a nonscalar row/column target orientation, including an
empty `1x0` or `0x1` target. Other empty selectors retain their exact shape.
Two selectors produce `[numel(R),numel(C)]`, with C outer/R inner
traversal. Duplicate occurrences and selector order are retained. Output axis,
element and byte caps (16/256/2048) are checked before gathering target words.
Checked offsets, slice access and fallible import create independent storage,
copying only selected words without arithmetic, including signed zero, infinities
and host NaN payloads. This is not a native arbitrary-payload parity claim.

`IndexArity`, `IndexOperand` and `IndexBounds` retain original argument spans.
Nonempty matrix selector rejection and scalar error fields remain unchanged;
vector element errors add zero-based element ordinal, selector shape and exact
word bits through `IndexElementOperand`/`IndexElementBounds`. Existing polling,
quotas, atomic ans publication and recovery apply. Empty reads still validate all
children and both selectors, check each output extent, and construct independent
zero-payload storage without reading target words. Nonempty matrix selectors, zero or
three-plus subscripts, logical/N-D indexing,
expression targets and chained indexing remain excluded, including forms accepted
natively. Session numeric matrices also admit ASCII space/tab gaps as described below;
historical native source scripts remain separate evidence. One-shot profiles and default
routing are unchanged. Selected native observations qualify their explicit sources; no complete indexing
compatibility is claimed.


Session numeric matrices accept nonempty ASCII space/tab gaps between numeric
entries, for example `[1 2;3 4]`, mixed `[1,2 3;4 5,6]` and `[1 -2]`. An implicit
signed numeric entry requires an attached sign/number; `[1 - 2]` and `[1-2]`
now use the Session construction expression grammar below. Existing first-entry and explicit comma/row
signed forms such as `[- 2]`, `[1,- 2]` and `[1;- 2]` retain their grammar.
Numeric decoding, original spans, node/number charges, caps, geometry and
transaction behavior are unchanged. Other public profiles still require commas.
Blocks, newline/comment gaps,
`[;]` and `[1;]` remain excluded. Selected native observations support this bounded
rule; they do not establish exhaustive MATLAB matrix grammar compatibility.

Session bound reads also accept a bare full-axis marker in `A(:)`, `A(:,C)`,
`A(R,:)` and `A(:,:)`. The marker is private call-argument metadata with its
original span, one existing argument charge and no Number charge. It produces
no value, range or selector vector. Arity counts all arguments; evaluation and
completion schedule/drain only real expression children, preserving their order
and original argument ordinals. Ordinary numeric/empty selectors keep their rules.

`A(:)` always returns `[numel(A),1]`, including `[0,1]` for every empty target.
Positional full axes contribute the matching target extent, preserving column-major
order. Output caps are checked before gathering: 16 linear words fit; 18 or 256
linear words fail the existing axis-16 limit, while `16x16` two-axis reads fit.
Checked copies retain host word bits and independent storage; empty results read
no target words and allocate no payload. Existing polls and transactions apply.

Binding-first dispatch includes builtin-named arrays. For an unbound known builtin,
valid name/arity preflight and all real children precede `UnsupportedCall` at the
first colon. Unknown callees keep preflight rejection. This is the approved project
context policy; the ordered native L37/L38/L40 records do not establish fresh-session
diagnostic or composition parity. An admitted inner read value may compose normally.
`A((:))`, `A(:+1)`, `A([:])`, general colon values and direct builtin markers remain
excluded. NonSession profiles and integer-range grammar are unchanged.

Session bound reads accept lowercase `end` in call-argument arithmetic, groups
and integer ranges. One subscript uses the target element count; two use each
original argument axis, including beside `:`. Nested bound reads replace that
extent; ordinary builtin argument expressions inherit it. `A(reshape(end,1,1))`
therefore selects the last linear element. Without a bound indexing context,
`numel(end)` reports the existing Evaluation/UnsupportedCall after preflight.
Other keywords retain lexical validation; standalone `end` and `end` as callee remain Lex/InvalidIdentifier.
Construction entries preserve an enclosing call context for `end`. NonSession profiles and caps
(axes 16, elements 256, bytes 2048) are unchanged. Empty ranges are valid but
a direct zero-valued `end` selector fails index domain validation. This is bounded
project behavior; all-builtin/deep native parity is not qualified.

Session brackets accept complete existing expressions, including bindings, calls,
ranges, nested brackets and arithmetic. Numeric-only direct Number/Signed rows
retain their original Matrix nodes, charges, admission and exact bits. Other
brackets use private construction rows: evaluate one row completely, join/check
it before visiting later rows, then join all rows vertically. There is no scalar
expansion. Errors use existing ArrayOperation/Construct diagnostics at the original
bracket span; numeric Matrix admission and Number quotas still precede evaluation.

An ASCII space/tab gap separates operands. A contiguous attached sign chain after
a gap starts the next operand (`[A -A]`, `[A --A]`); `[A - A]` is subtraction.
Inside bracket entries, `[A (1)]` separates a binding and group while `[A(1)]`
is a read. Groups and call arguments reset this boundary. Bracket-contained `end`
inherits the enclosing indexing extent, with existing nested-call rebinding.
Standalone `[end]`, keyword callees, general/chained targets and newlines remain excluded.

Each complete join classifies its whole operand list. If any operand is nonempty,
zero-element operands are omitted and surviving fixed extents must match. If all
are empty, take maximum input row/column extents; positive maxima on both axes
produce 0x0. Otherwise sum the join axis and retain the other maximum. Nested
brackets remain separate values, so flat and nested crossed-empty lists may differ.
Actual row/final axes are capped at16 even for zero payload, with 256 elements and
2048 bytes; checked independent column-major copies preserve all word bits.
One private continuation queues only the current row; existing fallible AST-sized
task/value reserves, polls and Session transactions remain. This is bounded project
behavior supported by selected native examples, not exhaustive diagnostic parity.

Session fixed-shape indexed assignment accepts `A(I)=R` and `A(I,J)=R` for an
existing dense double binding. The RHS finishes first, then target/arity preflight,
then all selector children, existing per-argument selector validation and RHS
compatibility. Scalars expand, linear writes use equal element counts, and two
selectors compare ordered nonscalar lengths against RHS axes after removing ones.
Columns form the outer traversal and rows the inner traversal; later duplicate visits win. Aliased RHS
reads finish before independent target staging. Writes return the complete target
through the existing Assignment effect, preserve ans/suppression and advance one
revision, including compatible ordinary empty no-ops. Full-colon writes can update
all 256 words of a 16x16 while the existing 256x1 read gather and ranges with 256 values remain
outside the axis-16 profile. Growth, new bindings, deletion, general/chained targets
and blocks remain excluded. Existing publication, quota and control checks stay atomic.

Every evaluated 0x0 RHS is rejected after selector validation, including literals,
constructors, aliases and groups, even when no words are selected. The new public
`SessionExpressionErrorV1::UnsupportedEmptyAssignment { source, span }` reports
Evaluation/UnsupportedEmptyAssignment at the complete RHS span. This explicit
0x0 restriction is stricter than MATLAB; compatible non-0x0 empty values such as 0x1
remain eligible. Adding this variant to the closed error enum breaks downstream
exhaustive source matches: consumers must add an arm. No other public result,
effect, error payload, wire field or profile changes. External release still needs
an explicit breaking-change compatibility/version policy; no stable enum ABI or
unchanged exhaustive-match compatibility is claimed. Native setup/write/read
translations support bounded behavior, not exact Rust block-source or error-ID parity.


The private Session profile accepts unbound one-argument `sum(A)` for dense real
rank-two doubles. Bound `sum` still indexes and bare `sum` reads a binding.
Row vectors reduce across columns; other arrays reduce down rows. Scalars copy
the complete input word. `0x0`, `0x1` and `1x0` produce scalar positive zero;
`0xC` produces `1xC` positive zeros; `Rx0` for `R>1` produces `1x0`.

This is an intentional numerical amendment within the unreleased private
`rust-session-arithmetic-v1` profile. Sum now accumulates finite evaluated
binary64 inputs exactly in integer units of 2^-1074 and rounds once, nearest
with ties to even. Cancellation and intermediate overflow no longer lose the
result. Genuine final overflow is allowed; a nonzero exact finite sum cannot
underflow below the minimum subnormal. This concerns evaluated words, not exact
decimal literals or upstream expression arithmetic. The previous first-term
forward loop and its gamma error waiver are historical behavior, not the current
accuracy contract. Old captures, frozen exports, binaries and failures remain.

Exact zero is negative only if every input is negative zero. Any NaN or opposing
infinities yields NaN; otherwise an input infinity dominates finite inputs with
its sign. Nonscalar NaN payload/sign are unpromised; singleton payload/sign are
copied. Internally first-NaN/canonical opposing-infinity handling is reused.

Five selected retained finite outputs change deliberately (not an exhaustive
compatibility inventory):

| Case | Prior native/forward word | Exact RN word |
|---|---|---|
| U22 | `0000000000000000` | `3ff0000000000000` |
| U24 | `3d06800000000000` | `3d06849b86a12b9b` |
| U25 | `3d06800000000000` | `3d06849b86a12b9b` |
| U26 | `0000000000000000` | `402c000000000000` |
| U27 | `7ff0000000000000` | `0000000000000000` |

Synthetic `[+max,+max,-Inf]` changes forward NaN to `-Inf`; mirrored signs give
`+Inf`. These are policy deductions, not new native observations. Native input
words/internal order are not universally promised. Native-only grammar stays
excluded. The word fixture verifies all 43 retained finite fibers, 43 focused
policy vectors and 320 independent exact-rational RN cases. The unchanged
mean/product word fixture runs again because mean shares additive accumulation.

Two borrowed 33-u64 subtotals use 528 logical bytes. Sum bypasses mean's division;
its bound is `33N+202F+32 <= 11712` counted fixed-word steps, at most 256 input
reads/classifications and 16 output words at existing call/publication polls.
Mean/product bounds remain 12768/5808, all below the 32768 ceiling. Sixteen
scratch plus sixteen fallibly owned output words and constant scalars fit the
logical 2 KiB state limit; this is not an ABI stack or wall-latency claim.
Existing errors, ownership, admission, transactions, cancellation/recovery,
mean/product/extrema and indexed-empty-write policies remain. No API/error,
parser/node/task/schema/profile/dependency expansion is included. Version 0.1.0
is retained only internally with new source/build identities. Distribution needs
an explicitly versioned release and packaging/installed/CI/GUI/publication gates.


Private Session one-argument `min(A)` and `max(A)` share default-axis extrema
selection for dense real rank-two doubles. Existing bindings named `min`/`max`
retain indexing precedence; bare names remain binding reads. The first extent
different from one is reduced. Empty operated axes preserve their zero length:
`0x0` stays `0x0`, `0xC` stays `0xC`, and `Rx0` with `R>0` becomes `1x0`.
There is no identity word for empty extrema. Nonempty columns produce `1xC`;
a nonempty row or scalar produces `1x1`.

Each nonempty fiber seeds and classifies its first word once, then visits only
the remainder. NaNs are skipped when numeric values (including infinities)
exist. Selection changes only for strict improvement, so equal values retain
the first selected word, including either signed-zero order. An all-NaN fiber
copies its first word including payload/sign; scalars copy their exact word.
This is the deterministic project policy, not a universal MATLAB NaN payload,
sign or internal-algorithm claim. Finite extrema select evaluated input words
without approximation, accumulation or arithmetic overflow. The accepted native
evidence covers 48 targets and three controls; distinct NaN words are separate
typed project-policy fixtures. Native-only setup/division grammar is not added.

Bounds remain 16 per axis, 256 words and 2048 input bytes. One seeded scan uses
at most 256 input reads/NaN classifications, 240 strict comparisons, 16 scratch
words and 16 independently owned output words between existing call polls.
Existing fallible output errors, transactional publication, ans/suppression,
revision/quota/cancellation/recovery and `UnsupportedEmptyAssignment` for every
indexed-write `0x0` RHS remain. No flags, two-input/second-output form, public
API/error/schema, parser/node/task or nonSession profile changes are included.


Private Session one-argument `mean(A)` and `prod(A)` correctly round the exact
mean/product of evaluated finite binary64 words once, nearest with ties to even.
Exact integer accumulation/multiplication prevents cancellation and intermediate
range loss; a finite mean cannot overflow. Product can genuinely overflow or
underflow only at final rounding, preserving the sign when a nonzero target
rounds to zero. These functions use a compact private standard-library kernel;
mean/product and first-word extrema policies remain as specified here; sum now
shares exact additive accumulation and bypasses division.

Default geometry has the scalar `0x0` exception: `0x0` and `1x0` produce `1x1`,
`0xC` produces `1xC`, and `Rx0` for `R>1` produces `1x0`. Empty mean fibers yield
positive quiet NaN (`7ff8000000000000`); empty product fibers yield `+1`.
Scalars preserve exact input bits. Otherwise the first input NaN word wins,
including payload/sign. Mean same-sign infinities preserve that infinity;
opposite infinities produce canonical positive quiet NaN. An exact finite zero
mean is negative only when every input is negative zero. Product XORs all input
sign bits: zero with infinity produces canonical positive quiet NaN; otherwise
infinity/zero uses the XOR sign. These are project word policies, not universal
native NaN payload/sign or internal-order guarantees.

Mathematical precision deliberately differs from seven selected native-source/
input-model observations:

| Case | Observed native word | Exact modeled RN word |
|---|---|---|
| E15 | `0000000000000000` | `3fd5555555555555` |
| E17 | `7ff0000000000000` | `7fe1ccf385ebc8a0` |
| E18 | `7ff0000000000000` | `0000000000000000` |
| P15 | `7ff0000000000000` | `3fefffffffffffff` |
| P16 | `0000000000000000` | `3fefffffffffffff` |
| P17 | `7ff8000000000000` | `0000000000000000` |
| P18 | `0000000000000000` | `000730d67819e8d2` |

The exact targets concern the frozen modeled/evaluated binary64 inputs. Native
operand/intermediate words were not observed, so source conversion and native
algorithms are not inferred. Native-only division/setup grammar is not admitted;
no approximate/gamma waiver or fallback is used. The independent versioned word
fixture covers 42 model cases, 44 typed policy cases and 320 exact-rational RN
finite checks, including normal/subnormal ties, tiny remainders and range edges.

Existing input caps 16/256/2048 precede the helper. Its two 33-limb buffers use at
most 528 logical bytes, with a 14-limb product prefix, 16 scratch/16 independently
owned output words and constant scalar state. Bounded integer loops stay below
the newly reviewed 32,768-step call quantum at existing entry/exit/publication
polls; this is not a wall-latency or ABI-stack-size claim. Existing fallible leaf
errors, ownership, diagnostics, Answer/Assignment, suppression, revisions,
workspace quotas, cancellation/deadline, atomic recovery and the indexed-write
`UnsupportedEmptyAssignment` policy remain. No dimension/flag/second-output,
public API/error/schema/task/profile/dependency change is included.

The opt-in `program` module provides `parse_program_v1` and its controlled variant
for whole-file syntax preparation. It retains an immutable `Arc<str>`, caller
source identity, byte/line spans and a read-only linked node arena. Scripts and
interspersed forward local functions, blocks, assignments, indexing, ranges,
matrices and tagged logical expressions are parsed without expression reparsing.
This frontend does not execute programs or claim native diagnostic parity.

Admission limits are 65,536 source bytes, 4,096 total nodes (including linked-list,
block, row and branch nodes), 1,024 syntactic statements and number nodes,
32 functions and 32 recursive syntax/comment levels. All three parser vectors
reserve fallibly; statistics report logical reserved bytes and cancellation polls.
Identifiers borrow source spans; syntax nodes own no heap collections. Power and
transpose associate left to right above unary operators. Signed `+`/`-` power
operands bind before the next power: `2^-2^3` means `(2^-2)^3`, while leading
`-2^2` remains `-(2^2)`. Mixed `~` power chains and whitespace apostrophes
report `UnsupportedFeature`. Class definitions,
strings, cells, fields, closures and command syntax remain unsupported.

`program::ProgramValueV1` adds bounded double/logical values and controlled
numeric dispatch for the opt-in program runtime. Logical storage retains its
class. Existing double kernels and exact default sum/mean/prod are reused.
Integer powers use explicit repeated squaring; negative exponents first take
the reciprocal base, preserving the independently tested subnormal counterexample.
This algorithm is not an exact-rounded-power or native-bit-equivalence promise.

Whole-program admission rejects an explicit script write whose name matches a
local function in the same module, before the module can execute. The located
program error is `DuplicateName`. The lexical check covers simple/indexed and
multioutput assignments and for-variable bindings in all script control branches;
function-local bindings, reads and implicit `ans` are excluded. The direct
assignment conflict is observed in frozen native F16; broader lexical coverage
is conservative project policy, not a claim about every MATLAB namespace rule.
Native Phase B G24 disambiguates the signed power chain above; F19 alone did
not. Related signed forms follow the stated project grammar; this does not widen
the separate existing v1 expression parser or promise every native unary form.

The additive scientific program value/numeric modules use the existing located
AST, including left division. They do not extend or replace the separate v1
expression parser/value APIs. Scientific values retain double/logical class,
explicit complex components and canonical N-D shapes; conjugating and plain
transpose remain distinct. New numeric jobs use explicit reserve/work controls
and private candidates. They do not reuse fixed-16-fiber exact reduction scratch
for larger arrays. The scientific program API selects these modules explicitly;
legacy program execution remains the default.


### Scientific V2 storage boundary (opt-in)

`ScientificProgramValueV2` owns either the accepted dense value or canonical CSC. `storage_view()` borrows a typed dense/CSC view; `dense_view()` and `csc_view()` return `Unsupported` for the other representation. There is no implicit full conversion or changed V1 `as_dense()` contract. Dense axes retain canonical trailing-one normalization; CSC always has two logical axes, with logical element count separate from stored entries.

The five-argument `construct_scientific_sparse_v2` adapter accepts dense double row/column/value vectors or independent scalar expansion, converts one-based integer indices into typed zero-based buffers, and uses the accepted stable-input-order CSC constructor. Empty mixtures select zero entries even with unused scalar nonfinite/complex values. Final coalesced values determine real/complex class; NaN/Inf are retained. Capacity and all exported lengths are truthful: row/value capacity is at least max(nnz,1), whereas only nnz used entries are serialized. CSC actual vector capacities together are at most16MiB.

The adapter adds two index buffers (8*n bytes); constructor sorting adds two permutations (another8*n) plus the CSC candidate. Remaining host headroom is checked before each phase; reserve hooks occur immediately before each fallible reserve. The callback is not itself a lifetime ledger: an embedding host must reconcile discarded input/adapter/scratch buffers. The V2 engine supplies that ownership ledger. CPU loops checkpoint and suspend at most128 charged units per block; synchronous drive wrappers are absent from the program path.

The new IEEE sparse-left/dense-right multiply skips structural zeros and uses separate real/complex component paths, with real demotion when all final imaginary components are zero. The previously accepted finite-only CSC multiply remains unchanged. Matrix nonfinite behavior is qualified to the observed native cases; the scalar CSC result is supported for the observed1x1 form. Other sparse operations, including indexing, assignment, transpose, reductions and arithmetic, are explicitly unsupported. Conditional truth scans retained NaNs before any implicit-zero shortcut; complex truth remains unsupported.

V2 finite-real dense kernels preserve explicit column-major arithmetic order, use hardware binary64 real division, and account real operations directly. Complex/logical/unselected operations delegate to the unchanged scientific V1 numeric job: it polls at most128 charged units apart and yields before/after at most32768 charged units, while the new cooperative provider loops suspend at most128-unit blocks. This is an explicit new profile, not a native-bit guarantee or a V1 resource change.


### Explicit class source graphs

`parse_class_graph_controlled_v1` accepts one immutable entry followed by explicitly supplied class sources. It stores source-qualified common AST/member nodes and prelinked single-inheritance/property/method tables. Ordinary expressions and control bodies use the same scanner/Pratt/block grammar. Old `ProgramModuleV1`, node/token layouts, errors, counters and limits retain their accepted behavior; the language-only increment has separate baseline/regression evidence.

The initial class graph admits8 classes/9 sources,64KiB per UTF-8 file and256KiB aggregate,4096 total nodes,32 free functions/methods,32 nesting and64 inherited properties per class. Source text lookup always uses each node's source slot and local span. Duplicate/unknown/cyclic bases and unsupported class attributes, custom lifecycle/operator/indexing hooks, persistence, validators and events fail truthfully. Pure graph lookup convenience methods are host inspection helpers; runtime resolution must charge and poll bounded identifier bytes. Classes do not introduce filesystem discovery.
