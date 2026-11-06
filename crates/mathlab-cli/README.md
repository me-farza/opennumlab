# Rust command-line session

Use `mathlab repl --backend rust` to start an experimental persistent Rust session.
Plain `mathlab repl` continues to use Octave. The Rust route does not discover or
start Octave and never falls back to it.

Enter one statement per line:

```matlab
A = [1,2;3,4];
B = A + 1
B
ans
```

Assignments retain variables between commands. An expression updates `ans`;
displaying a variable or assigning another name does not update `ans`. A trailing
semicolon suppresses human-readable value output while retaining the update.
The accepted bounded real arithmetic profile supports literals, identifiers,
parentheses, `+`, `-`, `*`, `.*`, `./`, and `.'`. The closed builtins `zeros(n)`
and `ones(n)` create n-by-n real double arrays; `zeros(m,n)` and `ones(m,n)`
create m-by-n arrays filled with positive zeros and exact ones respectively.
`eye(n)` and `eye(m,n)` create square or rectangular identity arrays, with exact
ones on the main diagonal and positive zeros elsewhere. Dimensions
may be scalar variables or admitted expressions. Each must be finite, integral,
with each normalized extent at most 16. Finite negative integers become zero;
negative fractions remain invalid. Both signs of zero create an empty axis.
Empty shapes such as 0-by-3 and 3-by-0 stay distinct and contain no stored words.

Integer ranges use `a:b` or `a:s:b`, with default step +1 and colon below sums
and products. Operands are evaluated in source order, then must be finite integral
scalars within inclusive +/-2^53. Results are real-double rows of at most 16
elements; zero step or incompatible direction gives 1-by-0, transposing to 0-by-1.
Negative operands retain their value. A nonempty range preserves the evaluated
first word (including -0); later computed zero is +0. Invalid operands or length
return source-spanned project errors and preserve the workspace for recovery.
Standalone colon, `colon(...)`, floating ranges, extra colons and ranges
inside numeric-only matrix literals remain unsupported.

`size(A)` returns a 1-by-2 double row `[rows,columns]`; `numel(A)` returns a
double scalar element count. Both require exactly one argument, accept any admitted
rank-2 expression, and inspect geometry even when payloads contain Inf, NaN or
signed zero. Empty-axis orientation is preserved by `size`, while `numel` is
positive zero for every empty array. Query results can be assigned, read and used
in further admitted expressions. Dimension-selector forms such as `size(A,1)`,
multiple outputs and overloaded/indexed queries remain unsupported.

`reshape(A,m,n)` copies an admitted double array into the requested rank-2
shape, preserving column-major payload bits, including signed zeros, Inf and NaN.
Element counts must match. All three expressions evaluate before dimensions are
validated from left to right. Reshape dimensions must be finite nonnegative
integers at most 16; unlike constructors, negative integers are rejected. Both
signs of zero allow empty axes. Bare `reshape` can be read or assigned normally.
Inferred `[]`, size vectors, extra dimensions and overloads are unsupported.
Failures preserve the workspace and allow the next command.

Bindings named `zeros`, `ones`, `eye`, `size`, `numel` or `reshape` can be read and
assigned normally. Every bound identifier selects read indexing before builtin
checks: `A(P)` reads one-based column-major positions (up to 256), and `A(R,C)`
forms a Cartesian gather with columns outermost. Nonempty selectors must be
scalar/row/column double vectors of at most 16 finite positive integer words;
zero-word rank-2 selectors are also admitted.
Complete child expressions evaluate left to right. Per argument, shape validation
precedes all word domains, then all bounds, before validating the next argument.

Linear output takes selector shape, except that a nonscalar target row/column
vector preserves its orientation for vector selectors. An empty `1x0` or `0x1`
selector inherits that orientation, including from an empty vector target; other
empty selectors keep their shape. Scalar targets take selector shape. Two-selector
output is `[numel(R),numel(C)]`. Duplicates and order are retained; independent
output copies preserve signed zero, infinities and NaN bits and follow normal ans,
suppression and atomic revision rules. Maximum output remains 16-by-16, 256 words,
2048 bytes. Indexing errors never fall back to builtins.

`IndexArity`, `IndexOperand` and `IndexBounds` retain original spans and scalar
behavior. Vector error messages add argument/element ordinal, selector shape and
original binary64 word. Constructor JSON dimension diagnostics also include
reason, argument index, shape and scalar word. These are project diagnostics.
Empty results allocate no payload, but still evaluate/validate the other selector
and apply output extent, metadata reservation, polling and revision rules.
Nonempty matrix selectors, zero or three-plus subscripts, contextual `end`,
expression targets and chained indexing remain unsupported.
Session numeric matrices accept commas or nonempty ASCII space/tab gaps.
Implicit signed entries require sign/number adjacency: `[1 -2]` is two entries;
`[1 - 2]` and `[1-2]` remain unsupported binary matrix entries. Existing first-entry
and explicit comma/row signed spacing such as `[- 2]` and `[1,- 2]` is preserved.
`[1 2;3 4]` retains column-major geometry and normal ans/transaction rules.
Other profiles retain their grammar. General matrix expressions, newlines/comments
as separators, `[;]` and `[1;]` remain excluded. Native evidence is bounded; no
exhaustive matrix grammar equivalence is claimed.
Unbound general functions, size vectors, class/`like` options, scripts and multiline
statements remain unsupported. Selected native evidence is scoped and qualified;
no complete indexing or native custom-NaN parity is claimed.

`:workspace` lists names, classes and shapes. `:clear` and `:restart` both clear
the in-process workspace and advance its epoch. `:quit`, `:exit`, or EOF end the
session. File and directory commands return explicit errors. `--octave`,
`--startup-config`, and `--directory` conflict with the Rust route.

`--timeout SECONDS` applies a cooperative deadline to each evaluation. An accepted
cancellation or evaluation error leaves the workspace unchanged and permits the
next command. This route uses the existing stdin owner; it does not add an
interactive Stop or Ctrl+C handler. Transport or output I/O failures terminate
the process. Handled evaluation errors do not make a later clean exit fail.

`--json` emits one JSON object per handled command and a final `exit` object,
without a banner or prompts. Each record identifies the Rust backend and profile.
Evaluation records carry the request/session/epoch identifiers and either the
before/after workspace revision or the unchanged error/cancellation context.
Values use column-major binary64 hexadecimal words, including when a semicolon
suppresses human output. Source spans refer to the submitted line with only its
transport line ending removed. The language source limit is 4,096 bytes; the
existing stdin transport additionally limits logical lines to 1 MiB.

This is a library-backed CLI subset. It does not enable persistent Rust execution
in the desktop or claim general MATLAB compatibility.

Full-axis reads `A(:)`, `A(:,C)`, `A(R,:)` and `A(:,:)` are also admitted for bound
arrays. Linear colon returns a column `[numel(A),1]`, including empty `[0,1]`;
positional colon selects the matching target extent. Numeric/empty selectors keep
all validation and original argument ordinals. The axis-16 cap still rejects
18/256-word linear columns before copying; a `16x16` two-axis read remains valid.
Word bits, independent storage, zero-payload results, ans, suppression and atomic
revision/recovery rules are preserved.

Builtin-named bindings still index. For ordinary known builtins, valid arity and
all real child expressions precede `UnsupportedCall` at the first colon; unknown
callees retain preflight rejection. This project policy is separate from the
qualified ordered native records. Bare colon is complete-argument metadata only:
parenthesized/arithmetic/matrix colon values and direct builtin markers remain
excluded. Inner read values may compose normally. Other profiles/ranges are unchanged.

Session bound reads accept lowercase `end` in call-argument arithmetic, groups
and integer ranges. One subscript uses the target element count; two use each
original argument axis, including beside `:`. Nested bound reads replace that
extent; ordinary builtin argument expressions inherit it. `A(reshape(end,1,1))`
therefore selects the last linear element. Without a bound indexing context,
`numel(end)` reports the existing Evaluation/UnsupportedCall after preflight.
Other keywords retain lexical validation; standalone `end` and `end` as callee remain Lex/InvalidIdentifier.
Construction entries preserve enclosing call context. NonSession profiles and caps
(axes 16, elements 256, bytes 2048) are unchanged. Empty ranges are valid but
a direct zero-valued `end` selector fails index domain validation. This is bounded
project behavior; all-builtin/deep native parity is not qualified.

Session brackets now construct arrays from existing expressions and nested brackets.
Rows join horizontally before later rows evaluate; the final join is vertical.
Numeric-only literals retain their original parser/admission behavior. Whitespace
and attached signs distinguish `[A -A]` from `[A - A]`; `[A (1)]` separates a group.
Bracket-contained `end` inherits an enclosing bound read. Whole-list empty geometry
preserves bracket boundaries, omits empties when a nonempty operand exists, and
classifies crossed empty axes before checking actual row/final caps16/256/2048.
Bitwise independent values and existing assignment/ans/suppression/atomicity,
cancellation, deadlines, quotas and recovery remain. No general/chained targets,
newlines, or horzcat/vertcat builtin promotion is included.

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


Persistent Rust Sessions accept unbound `sum(A)` with one dense real double
argument. Existing bindings named `sum` still select indexing. Reduction follows
the first extent different from one, preserves scalar words, returns positive
zeros for empty fibers (including scalar zero for `0x0`), and retains `1x0` for
`Rx0` with `R>1`. Output uses the existing display, binary-word JSON, ans,
suppression and transactional revision behavior; errors preserve the workspace.
No dimension/type/nan flags or public schema changes are included.

Sum now accumulates evaluated finite binary64 words exactly and rounds once,
nearest with ties to even. This deliberate same-profile private amendment removes
forward cancellation/intermediate overflow loss; true final overflow remains.
U22 changes +0 to 1, U24/U25 change `3d06800000000000` to
`3d06849b86a12b9b`, U26 changes +0 to 14, and U27 changes +Inf to +0.
These five retained cases are not an exhaustive compatibility inventory.
Synthetic `[+max,+max,-Inf]` changes forward NaN to -Inf, mirrored signs to +Inf;
these are policy deductions, not new native observations. The old forward/gamma
contract and retained captures/binaries are historical evidence.

Exact zero is negative only when every input is negative zero. Any NaN or
opposing infinities produces NaN; otherwise an input infinity dominates finite
inputs. Nonscalar NaN sign/payload and universal native words/internal order
remain unpromised. Scalar words, empty geometry, display/JSON/ans/suppression,
revisions, errors and atomic recovery are unchanged. Mean/product/extrema and
the indexed-empty-write policy are preserved. The bounded exact helper uses
528-byte limb state and at most 11712 counted sum steps within the existing
32768 quantum; no wall-latency promise or public schema/grammar change follows.
The [language contract](../mathlab-language/README.md) records precise word and
resource qualifications. Version 0.1.0 is internal only with new source/build
identities; distribution requires an explicitly versioned release and applicable
packaging, installed, CI, GUI and publication gates.


Persistent Rust Sessions accept default one-argument `min(A)` and `max(A)` for
current dense real double matrices. Bound names still index; bare names still
read bindings. Nonempty reductions follow the first extent different from one.
Empty geometry has no identity: `0x0` remains `0x0`, `0xC` remains `0xC`, and
`Rx0` for `R>0` becomes `1x0`. Existing JSON words/shapes, display, ans,
suppression, revisions, quotas and atomic errors/recovery are unchanged.

The project selects the first numeric word on ties (including signed zero),
skipping NaNs when numeric words exist. All-NaN fibers and scalar NaNs retain
the first exact word, including payload/sign. These distinct-word choices are
typed project policy; selected native captures do not establish universal
MATLAB NaN word parity or internal selection order. Finite extrema select input
words without accumulation/overflow arithmetic. Dimension/flag/two-input/
second-output forms and new grammar/schema are excluded. Indexed assignment of
`min([])` or `max([])` remains `UnsupportedEmptyAssignment`; all prior external
compatibility, publication and release holds remain.


Persistent Rust Sessions accept default one-argument `mean(A)` and `prod(A)`.
For evaluated finite binary64 inputs they correctly round the exact result once
(nearest, ties to even), avoiding cancellation and intermediate range failures.
Only genuine final product overflow/underflow is allowed. Mean/product and extrema
selection semantics are unchanged by the exact-sum amendment; flags/dimensions/second outputs,
new grammar/classes and public schema changes remain excluded.

Empty `0x0`/`1x0` inputs produce scalars; `0xC` produces `1xC`; `Rx0` for `R>1`
produces `1x0`. Empty mean is positive quiet NaN, empty product is+1. Scalars copy
bits. The first input NaN preserves its payload/sign. Mean opposite infinities
produce canonical positive quiet NaN; equal-sign infinities persist. Exact zero
mean is negative only for all-negative-zero inputs. Product XORs signs; zero
with infinity produces canonical positive quiet NaN. Nonzero exact targets
rounding to zero retain their sign. These are project policies, not universal
native word/internal-algorithm promises.

Seven selected native-source/model results deliberately change: E15/E17/E18
and P15/P16/P17/P18. The [language contract](../mathlab-language/README.md)
records each observed native word and exact modeled RN word. Native operand/
intermediate words were not observed; literal conversion and internal order are
not inferred. Existing display/JSON, ans, suppression, revisions, workspace,
allocation/error atomicity and recovery remain; all prior release/compatibility
holds still apply.

Run an explicit bounded Rust program file with
`mathlab run --rust-program FILE --workspace`. Add `--json` for the separate
`mathlab-rust-program` schema 1 JSONL protocol: a ready record, then an outcome
with source SHA-256, typed values, ordered displays, committed-prefix counters,
optional final workspace and located errors/call frames. Source/parse failures
emit only an outcome. Double payloads use exact binary64 words; logical payloads
use booleans. Text doubles use 17 significant digits in scientific notation.

This Unix-only opt-in route never discovers or starts Octave. It accepts one
regular UTF-8 file up to 65,536 bytes, retains an immutable snapshot, and rejects
Octave startup/directory options. Ctrl+C requests cooperative UserStop. `--timeout`
covers parsing and execution (including intervening setup/ready output); final
outcome encoding and file input are excluded. Lower shared budgets with
`--program-transition-fuel` and `--program-primitive-fuel`. Exit codes are 0 for
success, 1 for source/parse/runtime/resource/I/O failure, 2 for usage, 130 for
UserStop and 124 for Deadline. Displays and workspace effects completed before
failure or Stop are retained; outcome encoding reports them after execution.

The bounded reply allowance is 8 MiB. Stable std source/module/control Arc
allocations retain their global-OOM qualification; explicit source/reply buffers
reserve fallibly. This exposes the initial 16-by-16 real/logical program API;
classes, broader types, scaling and desktop integration remain future milestones.
Existing commands, schemas and default Octave behavior are unchanged.

## Scientific program profile (draft)

The existing `run --rust-program FILE` command keeps its legacy profile/schema 1.
Select the additive profile explicitly:

```sh
mathlab run --rust-program --profile scientific-v1 --json --workspace \
  --export-binding Y --export-file ./Y.onlsci \
  examples/scientific/frequency_response.m
python3 crates/mathlab-array/tests/scientific_oracle.py ./Y.onlsci
```

Scientific JSONL uses schema 2 on the same bounded immutable UTF-8/source-hash,
located-AST, signal and deadline pipeline. Ready means prepared; setup, ready
encoding/flush and execution count toward the deadline. Final encoding/export
are outside the execution clock. Existing native/default routes are unchanged;
there is no Octave fallback.

Every value reports MATLAB class (`double` or `logical`), an explicit complex
flag, complete canonical rank/shape and column-major order. Workspace windows
contain at most 256 elements and event windows 32, with explicit offset/count
and `payload_complete`. Double elements contain one or two exact binary64 hex
components; logical elements remain booleans. Oversized output falls back to a
truthful incomplete report within the 8 MiB JSONL allowance. Human output also
labels clipped windows and prints 17 significant digits. Borrowed library ranges
remain available for complete inspection; a preview is never a full-value oracle.

`--scientific-transition-fuel` and `--scientific-primitive-fuel` set allowances
up to 5,000,000 and 1,000,000,000 respectively and require the scientific profile.
Existing lower legacy fuel flags retain their syntax/ranges. The engine's first
accepted Stop/deadline reason is authoritative; callback polling forwards SIGINT
during scalar kernels, allocation preflights and long copy/fill preparation.
Existing exit meanings remain 0 success, 1 execution/resource/export failure,
2 argument error, 130 Stop and 124 deadline.

`--export-binding NAME --export-file PATH` requires the scientific profile and
exports only after successful execution. It opens one explicit destination with
`create_new`, never overwrites/truncates existing files or follows an existing
symlink, and creates no directories. The file is checked as regular. A bound
name is ≤2048 UTF-8 bytes. There is no inferred multi-binding export.

Format `onl-scientific-array-v1` consists of a bounded JSON header plus LF, a
complete column-major payload, LF, then a complete JSON footer plus LF. The
header names class, complex flag, full shape, element count, source SHA256,
binding, and encoding. Double components are IEEE binary64 little-endian,
interleaved real/imaginary for complex values; logicals use 0/1 bytes. The footer
contains `complete:true`, exact payload byte count and SHA256 over payload bytes.
The payload is ≤16 MiB and header/footer together ≤32 KiB. No destination path
is embedded. Consumers must require complete footer, length and digest. This is
integrity/completeness, not crash-durable fsync. I/O failure may leave a partial
new file; it is retained, never deleted implicitly. On failure output reports
`complete:false` and an unknown/null payload byte count. A late flush failure is
still a command error even if a complete file exists. SIGINT during export is
cooperatively checked and can yield exit130 without rewriting the engine's
already-completed outcome. Export errors have their own report/command exit.

The full 128×32×8 example and exact Fraction oracle are project workloads.
Native H08 remains its immutable constant-solution reference; the stronger
page-dependent example is not claimed to be identical native evidence.

Cooperative execution uses stack-pinned standard Rust futures and a nonallocating
yield primitive, without Box, thread, new dependency or host-stack recursion for
language calls. The internal suspended state is required to fit 1 MiB (checked
by the targeted future-size test); hosts must provide at least 2 MiB available
stack for the core plus their own caller/checkpoint/waker needs. Allocation
system-call latency and arbitrary caller callbacks/wakers remain outside CPU
quantum guarantees. Legacy layout/operation/fault equivalence is an execution
gate, not inferred from the mechanism. Dropping the cooperative future abandons
its key, restores the completed workspace prefix including an empty reserved
binding pool, and discards the deliberately abandoned result/event report.
Request Stop and continue polling to obtain the normal retained-events outcome.
Growth diagnostics preserve exact initial/max-U metric words. Ratio overflow is
reported as hex Inf with an explicit nonfinite/overflow classification and never
fails an otherwise successful solve; zero initial metric is explicitly undefined.

Scientific human output uses the same 32-element event and 256-element workspace
windows as JSON. An encoding ReplyLimit takes command-exit priority (1); the
report retains the engine cancellation reason separately and sets
`events_complete=false`. It does not rewrite the accepted engine reason.


### Opt-in sparse scientific programs

`mathlab --json run --rust-program examples/scientific/poisson_pcg.m --profile scientific-v2 --workspace --export-binding solution --export-file result.onl` uses the same immutable UTF-8 source, located parser, keyed cancellation and shared program machine. The default route and `scientific-v1` remain unchanged. V2 emits separate schema3 ready/outcome JSONL records. Its explicit fuel controls retain maxima1B primitive/5M transition units. Setup/ready encoding/execution share the selected cooperative deadline; final outcome/export encoding is outside that execution window, with SIGINT still checked during export.

Dense workspace windows carry `storage:"dense"`, `logical_numel`, `storage_order:"column-major"` and the existing typed binary64/logical fields. CSC windows carry logical shape/element count separately from `nnz`/`nzmax`, `storage:"csc"`, `storage_order:"CSC"`, and bounded stored row/column/value entries. Windows show32 event entries or256 workspace entries; they never allocate a logical dense projection. All double components use exact lowercase16-digit hexadecimal bit strings.

Dense exports reuse `onl-scientific-array-v1` unchanged. CSC exports use `onl-scientific-csc-v1`: a bounded JSON header followed by used little-endian u32 column pointers, used u32 row indices and used real/interleaved-complex binary64 values; the header lists each section's offset, length, encoding, zero index base, logical shape, nnz/nzmax, binding and immutable source SHA. Unused capacity is never serialized. A footer records completeness, total payload bytes and SHA256 over the raw payload. Metadata is at most32KiB and payload at most16MiB. Files are regular new destinations, opened without following links or overwriting existing data. I/O/interrupt failures may leave a partial file, which consumers must reject without a valid complete footer.

The independent `tests/sparse_oracle.py` verifies both complete CLI outcomes, source identity and both export formats before invoking the byte-pinned sibling Fraction core. The core checks every matrix entry and solution word against independent geometry/analytic RHS, exact-rational backward error2^-36 and relative forward error2^-24, plus flag0 and1..512 actual iterations. These checks passed for the bounded 32³ Poisson-PCG workload, checking all 223,232 stored matrix entries and 32,768 solution values. Other workloads retain their own acceptance requirements.


### Opt-in explicit class programs

`mathlab --json run --rust-program ENTRY.m --profile class-v1 --module-file ClassA.m --module-file ClassB.m --workspace` uses the same immutable regular-file input, located parser, keyed SIGINT/deadline and execution pipeline. It never searches ambient class paths. Source input uses the accepted nonblocking regular-handle check (regular symlink targets remain allowed); every source is bounded64KiB, at most9 files/256KiB total. Module flags require this class profile.

Schema4 reports the ordered source descriptors and a closure SHA256 as well as the entry source. Object workspace/event records contain class name, value/handle kind, opaque identity and validity; they do not claim a dense payload. Views are bounded and events snapshot metadata. Primitive projections and scientific numeric exports remain available. Object export is explicitly Unsupported. Existing dense/CSC export formats retain entry SHA only, so a consumer must verify the corresponding schema4 outcome/source_graph_sha256 to identify the complete module closure. Exports never overwrite.

Old/default/scientific-v1/scientific-v2 routes and schemas remain unchanged. Class-v1 is a new qualified profile; char values, object arrays/custom indexing/overloads/persistence and the remaining class/numerics/desktop roadmap are not claimed. The bounded class library/CLI path passed 191 acceptance cases, including 52 class-specific tests and unchanged legacy-profile layout, cancellation and numerical-export checks. Rust desktop integration and the listed unsupported features remain separate work.
