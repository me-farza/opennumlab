# Bounded range edits and undo protocol

The development checkpoint integrates the Rust edit codec,
Engine write/undo/redo APIs, and owned Octave worker/history service behind the
optional `atomic_range_edit.v1` capability. The desktop has a literal clipboard
parser and conversion to the shared edit request; it has no range selection,
Paste, Undo or Redo actions wired to this service. The accepted typed-read
checkpoint is described separately in [Typed value pages](VALUE_PAGE_PROTOCOL.md).

The newest working source routes dense complex pages, edit construction and
history through separate real/imaginary buffers from an optional native helper.
It rejects those operations explicitly when the helper is unavailable. This
integration passed the headless native profile in the
[follow-up report](../artifacts/cp03-followup-validation/RESULTS.json): 404 passing
entrypoints across all 19 compiled workspace test targets, with no ignored or
filtered tests. This includes 98 Engine unit, 119 desktop unit, 13 edit-session,
three adapter lifecycle and four precision entrypoints, plus the original
session/binding/typed-page/input targets. Workspace check, strict all-target
Clippy and formatting passed; four crate doctest invocations contained no tests.
Earlier probes and historical passes retain their own source identities.
Installed and complete-product acceptance remain separate.

The earlier retained runs below passed **50 targeted test entry points**. A later
cleanup variant passed nine additional lifetime checks. These runs identify
different source snapshots; they are not one qualification of the current full
workspace, installed application, native visual workflow, or complete CP03 milestone.

| Retained run | Passed | Scope |
| --- | ---: | --- |
| [Edit codec](../artifacts/cp03-edit-codec-rerun.log) | 11 | Pure codec, exact typed components, bounds, strict JSON and receipt identity checks |
| [Clipboard parser](../artifacts/cp03-paste-first.log) | 16 | Literal TSV, rectangularity/budgets, exact integers, direct binary32 midpoint rounding and shared request conversion |
| [Engine edit sessions](../artifacts/cp03-edit-session-rerun.log) | 13 | Actual-worker selected write/undo/redo, N-D/nested/sparse/type, invalidation, reset and Stop journeys |
| [Private worker/fault checks](../artifacts/cp03-edit-worker-first.log) | 10 | Actual-worker raw validation, construction admission and project-shadow checks, plus six SIGINT cases using private worker-copy gates before revision or after assignment |
| [Temporary lifetime checks](../artifacts/cp03-edit-lifetime-all-first.log) | 9 | Actual-worker last-owner deletion, alias ownership, undo/redo, rejection, uncertainty, SIGINT boundaries and protection of user variables/functions; exact inputs in the [cleanup checkpoint](../artifacts/cp03-edit-lifetime-checkpoint.json) |

These test counts do not establish all complex signed-zero variants or general
resource accounting; see the known limitations below. Desktop writes
remain unwired. No installed-package qualification is claimed.

## Commands and wire

Optional capability: `atomic_range_edit.v1`; CP02 required capabilities remain unchanged. Ordinary engine envelopes provide the command and request ID. The following JSON objects are their command-specific payloads.

`write_value_range_checked`:

```json
{"schema_version":1,"path":{"root":{"context":{"worker_id":"worker","generation":"1","invocation":"1","revision":"9"},"name":"A"},"steps":[]},"window":{"row_start":"2","column_start":"3","rows":2,"columns":2,"higher_indices":["4"]},"expected_leaf":{"class":"single","source_shape":["3","4","5"],"source_storage":"dense","complex":false},"data":{"kind":"dense","encoding":"ieee754_hex","real":["3f800001","80000000","00000001","7f800000"],"imag":[]}}
```

`path`/`window` use the existing typed read schema and genuine builtin cell/struct traversal. No arbitrary expressions, object getters, implicit field creation, conversion, reshaping or growth. Every higher axis needs an explicit selector. V1 edits a rectangle in axes 1/2 of one N-D plane, not a thick higher-axis box. The full range is starts `[row_start,column_start,...higher_indices]`, extents `[rows,columns,1,...]`. Every requested element must fit; **write requests never clip**. Empty leaf axes cannot be grown through editing. `[3,4,5]` selector4 is supported; an empty higher axis has no valid plane and rejects.

The expected descriptor must exactly match the live leaf before preparation: primitive class, full source shape, dense/sparse storage, and complex state. Complex is restricted to single/double and requires an equally sized imaginary component array, including all-zero imaginary data; real leaves require an empty imaginary array. The backend's required behavior is to preserve the declared state or reject before commit. Earlier selected 2×2 complex cases passed, but singleton/all-real selections with imaginary negative zero exposed a narrowing gap. The native component route now passes selected full worker/history journeys for those shapes; broader storage variants remain unqualified.

Dense numeric RHS is column-major and same-class, up to 2,000 cells. Integer/logical components use `encoding:"decimal"`, canonical decimal strings; logical only `0`/`1`. Signed/unsigned 64-bit extrema never pass through f64. Single/double components use `encoding:"ieee754_hex"` with exactly 8/16 lowercase IEEE hex digits. Rust uses `to_bits`; native folds exact unsigned bits and genuine builtin `typecast`. This avoids decimal-to-double-to-single rounding. Signed zero and nonfinite wire components retain exact bits in the codec; this is not blanket qualification of subsequent native selection, reconstruction or assignment, especially the complex narrowing case below. No promise is made about NaN payloads across subsequent numerical operations. Typed read wire remains unchanged (decimal floating text).

Sparse leaves initially support rank2 double/logical only (complex only double). Only the bounded RHS patch is converted to ordered local nonzero COO. Missing COO positions replace the selected source rectangle with zeros; they must not merely merge with prior nonzeros. Whole sparse sources never densify. Sparse zero signs are not retained as explicit entries.

`undo_value_edit_checked` / `redo_value_edit_checked`:

```json
{"schema_version":1,"expected":{"context":{"worker_id":"worker","generation":"1","invocation":"1","revision":"10"},"name":"A"},"history_epoch":"4294967297","transaction_id":"7"}
```

Receipt:

```json
{"schema_version":1,"operation":"write","transaction_id":"7","token":{"context":{"worker_id":"worker","generation":"1","invocation":"1","revision":"10"},"name":"A"},"history":{"epoch":"4294967297","undo":{"transaction_id":"7","root_name":"A"},"redo":null},"retained_patch_bytes":1234,"retained_transactions":1,"evicted_undo_entries":0}
```

All identity integers, source extents and starts/selectors are canonical decimal JSON strings. Bounded counts are JSON numbers. Receipt validation compares the full expected/new binding and settled context (same worker/generation/invocation/name, revision exactly previous+1), operation, known transaction/epoch and consistent head linkage. Undo/redo require exact known transaction and epoch; write IDs are owner-assigned. Rust codecs reject unknown/duplicate fields. Engine keeps original response JSON through strict decode so duplicate fields cannot disappear into a generic JSON Value first.

## Bounds and ownership

Wire admission: request 1MiB; history request and receipt 16KiB each; rank32; structured path depth16; identifier63 bytes; worker ID256 bytes; component token32 bytes; patch2,000 cells. Typed public callers receive borrowed preflight bounds before path/window/token/descriptor copies or formatting. Source/index conversion is initially limited to integers below 2^53 by the Octave backend; integer **data values** retain full native ranges.

The integrated worker serializes full token validation, private preparation and commit in one interpreter-owner operation. Its required contract excludes callbacks/event pumping and user-code dispatch. A Rust borrow of `Engine` alone does not prove ownership. Protected builtin path traversal, expected descriptor verification, native class/range checks, old-patch capture, complete RHS validation, root construction, candidate history and receipt construction all occur before publishing. The worker rebuilds ancestors privately, then advances revision once and performs **one genuine base-root replacement**. Selected nested/alias and rejected-validation journeys passed. Later lifetime checks qualify scoped cleanup for their exact cases; complex narrowing and broader resource behavior remain open.

Separate construction admission is essential: a small patch may trigger a full dense leaf copy, sparse capacity/column allocation or large ancestor reference-array copy. The integrated helper admits at most 128 MiB of logical leaf construction, 1,048,576 aggregate ancestor reference units, and a sparse capacity-plus-worst-case-insertion count of at most 1,048,576 with at most 1,048,575 columns. It separately caps old/new native patch accounting at 256 KiB. Selected admission rejections passed actual-worker checks. These logical byte/reference/capacity measures **are not physical allocation or peak-RSS bounds**; allocator overhead, native COW peaks and sparse growth transients remain unmeasured here. The private SIGINT gates qualify the stated before-revision and after-assignment boundaries, not every possible cancellation instant.

## Linear history

One worker-owned global undo+redo history spans roots. It stores bounded old/new selected native patches, full descriptor, path and full-axis range, rather than entire roots. Current limits are 32 transactions, 2 MiB of combined encoded-record charge and 512 KiB per record; the helper's separate old/new native patch accounting cap is 256 KiB. The retained charge is the UTF-8 JSON size of record identity, metadata and exact old/new component representations, not native allocator usage or a peak-memory ceiling. New successful writes clear redo and may evict oldest undo records, reported in the receipt. Selected rejection and eviction/history journeys passed.

An actual before-fix regression confirmed that the loop-local `replacement`
retained a rebuilt root after an edit: clearing the last user-visible root did
not run an opaque sibling's delete marker. The cleanup variant now clears exact
dispatch-local candidates before response publication and request/result/error
data afterward, using nested `unwind_protect` guards and variable-only builtin
clearing. Committed history and recovery control state remain live.

Nine later checks passed, including deletion inside the user's ordinary clear,
alias ownership, undo/redo, rejection, post-assignment uncertainty, both SIGINT
boundaries, and unchanged user variable/function and persistent-state controls.
They also check a destructor's explicit base assignment during ordinary clear.
The proposed old failure in which a delayed destructor mutates a later checked
edit was not separately executed; do not claim that narrower causal observation.
These selected lifetime results do not measure peak RSS or prove reclamation of
every interpreter allocation. The follow-up workspace run includes these nine
lifetime checks and the original session regressions with the cleanup variant.

Every successful write/undo/redo advances revision once and returns fresh context and current head IDs/root names. The next multi-undo request uses that fresh context and the returned next head, never the revision from the original write receipt. Undo moves the exact undo head onto redo; redo reverses that move. Transaction IDs are monotonic and not reused within one worker invocation; recovery starts a new history namespace, so authority always includes the complete binding context and epoch as well as the ID.

For current worker invocation `i`, the initial epoch is `(uint64(i) << 32) + 1`. Invocation must be in `1..=u32::MAX`; the low 32-bit invalidation counter starts at one and may not carry into the next invocation's range. Invalidation increments that counter while preserving the next transaction ID. Exhausting the invocation range, low counter or transaction identity rejects editing/history and requires restart. Thus a same-process recovery cannot revive an old epoch/transaction pair merely because its local transaction counter restarts at one. The JSON examples use illustrative identities and are not a prescribed request sequence.

Arbitrary potentially effectful dispatches revoke history before execution, including later failure, equal-value rebinding, package/path operations and effectful legacy inspection. An accepted checked scalar edit also clears both stacks and increments the epoch before its commit. Stop/recovery/restart and unexpected context invalidate old history authority. Pure typed snapshots/pages preserve history; nonmutating precommit rejection preserves it. A worker exception after edit revocation retires the stacks, while client-side receipt uncertainty quarantines further edits until explicit reconciliation. There is **no automatic retry** after a lost receipt and no persistent idempotency journal.

Engine also latches an uncertain submitted edit outcome. `has_value_edit_uncertainty()` reports it; another write/undo/redo is refused while it is set, while reads remain available. Known precommit worker rejections stay ordinary errors. Transport loss, an untrusted receipt or a post-revocation exception is treated as potentially committed. An ordinary read or fresh snapshot does not clear this latch. Explicit `reset_value_edit_history()` preserves numerical values, retires history, advances the binding revision and returns fresh tokens; only a successful reset or restart clears the latch. Reset failure leaves uncertainty set. Reset never retries or rolls back the ambiguous edit. The private six-case SIGINT journey checks old/new root outcomes and history revocation at its two causal boundaries.

Atomic mailbox publication is the client uncertainty boundary. Closed transport,
local validation and request-file creation failures occur before publication and
remain ordinary errors; they do not create a new uncertainty latch. A locally
failed reset preserves any existing latch. Completion or receipt failure after
successful publication remains uncertain. Four actual-worker groups cover these
distinctions, same-token recovery after a local I/O failure, and a submitted reset
whose snapshot is corrupted. Before the correction, three local-failure groups
failed by incorrectly reporting possible commits; all four now pass.

## Public codec surface

- `RangeEditRequest::from_components(path, window, descriptor, &real, imag)` checks native variant identity and encodes bounded input.
- `EditData::from_components(&real, imag)` converts bounded typed components only.
- `RangeEditRequest::validate_decode()` returns `ValidatedRangeEdit` with explicit full-axis range and typed dense/local sparse replacement.
- `decode_edit_json`, `HistoryRequest::validate`, `decode_history_json` enforce strict JSON and byte gates.
- `RangeEditReceipt::validate`, `decode_receipt_json` compare `ReceiptExpectation` and settled `SessionContext`.
- `Engine::write_value_range_checked`, `undo_value_edit_checked`, `redo_value_edit_checked` submit checked owner operations and validate original receipt JSON.
- `Engine::has_value_edit_uncertainty` and `reset_value_edit_history` expose explicit reconciliation after ambiguity.
- A separate private reference model is not part of the product and does not supply production history behavior.

## Complex signed-zero repair and remaining work

The raw probes [complex zero](../artifacts/cp03-complex-zero-probe.log) and
[complex singleton](../artifacts/cp03-complex-singleton-probe.log) expose native
narrowing that the 13 Engine edit-session tests do not exhaust. One probe reports
single imaginary scalar bits `00000000` while the array component retains
`80000000`; another reports a duplicated selection as real with imaginary bits
`00000000` while the original has `80000000`. A singleton or an all-real selected
region of a complex array can therefore lose imaginary negative zero when native
selection/copy narrows its representation. An exact `ieee754_hex` request and
passing mixed-complex 2×2 cases do not establish preservation through that path.

The newer native component route avoids that narrowing for its declared dense
single/double leaf subset. Actual public Engine tests now cover singleton,
all-real and N-D plane reads, independent write/undo and write/undo/redo journeys,
forced zero-imaginary storage, nested containers, untouched NaN payloads/signed
zeros and aliases, plus missing-adapter rejection. These are exact selected
journeys, not an all-variant complex guarantee. Sparse explicit-zero signs remain
outside the value-level COO contract.

An added regression uses the original imaginary components `[-0,-0,5]` and
selects the 1×1 and 2×1 prefixes. Eight independent single/double journeys check
write→undo and write→undo→redo, original sign bits, untouched rows, aliases and
peers. Ordinary evaluation occurs only before a journey or after its final
history action. Complex diagonal storage remains outside the native API and
requires further implementation.

Follow-up coverage repeats those eight original-negative-zero journeys
through a struct/cell/struct leaf, with independent full-leaf IEEE-word oracles
and root/container/leaf alias checks. The five-entrypoint native precision target
passed on local Octave 11.3 with the explicitly selected matching value adapter,
including these eight nested journeys and the earlier root `Z` journeys. The
[scoped replay](../artifacts/ready-history-validation/RESULTS.json) retains its
source/executable/module identities; other runtime/storage variants stay open.

A private native component helper passed focused single/double probes for exact
signed-zero and quiet-NaN payload copying, alias preservation, component-pair
restoration, storage in genuine containers and rejection before object conversion.
The [private probe report](../artifacts/cp03-value-bridge-extensions/report.json)
records executable/module identities and actual commands. The same native body
now has an owned source copy and the newest working page/edit/history routes call
its component API. The [earlier 232-entrypoint report](../artifacts/cp03-current-validation/RESULTS.json)
records the first combined checks and standalone build; the follow-up records
the fresh four-module build and broader workspace run. The initial loader run failed all three
lifecycle cases because its metadata type label was wrong; a genuine runtime
probe established the `function` label, the canonical-file gate was retained,
and the unchanged lifecycle cases then passed. No packaged or installed helper
is claimed.

CP03 is still incomplete: desktop range selection/Paste/Undo/Redo actions and
their real clipboard/keyboard/stale-state journeys are absent; broader complex
variants, the shared dispatcher defect,
broader cancellation/receipt-loss cases, physical construction measurements,
other runtime/release profiles, native visual acceptance and
installed-package qualification remain. The retained targeted passes must not
be presented as completion of those gates or full MATLAB compatibility.

## Packaged native prerequisites

The launcher/CLI prerequisite slice adds `VALUE` to the installed launcher's
existing adapter discovery loop. A bundled `mathlab_value_bridge.oct` fills an
unset or empty `MATHLAB_VALUE_BRIDGE`; a nonempty explicit override remains
authoritative, including a missing path that must subsequently fail loading.
Omitted optional components are not silently supplied from another installation.

`doctor --json` output reports `value_adapter` from the Engine's
accepted canonical installation and `worker_capabilities` from its negotiated
worker record. `runtime` identifies the same canonical executable used for both
the version check and session startup. A successful session without the optional
module reports a null path and no native component capability. These fields are
observations, not a hardcoded list of expected support or checked-edit proof.

Doctor preflight/startup failures emit `ready:false`, empty capabilities, a null
accepted value path and a diagnostic before a nonzero exit. Validated successful
identity paths require UTF-8. Failure-only path descriptions may be lossy for
non-UTF-8 Unix names; explicit `*_path_utf8` fields identify that limitation so
those strings are not mistaken for successful canonical artifact identities.
Configured scope/input/value adapter paths are checked for UTF-8 both before
canonicalization and at their canonical targets, before starting the worker.
This keeps an invalid identity out of Engine startup's JSON requests.

Local GNU Octave 11.3 validation on macOS arm64 passed seven doctor integration
entrypoints, sixteen CLI unit tests, ten conformance regressions, ten console/input
regressions and seven synthetic launcher scenarios. Seven separately retained
doctor profiles cover absent/present native value support, all three startup
adapters, invalid explicit runtimes and missing/bad value modules. The compiled
CLI's 35-input source fingerprint and actual child executable were retained.
The earlier 162-entrypoint checkpoint remains separate evidence.

Two initial doctor test attempts failed while creating invalid-byte filenames:
the sandbox rejected them, and APFS reported an illegal byte sequence when that
restriction was lifted. Those attempts are retained. The corrected macOS tests
exercise actual non-UTF-8 path arguments without creating impossible filenames.
Existing non-UTF-8 files and UTF-8 aliases to those targets remain authored for
other Unix filesystems and have not been executed on Linux in this checkpoint.

A separate shipped checked-API diagnostic is
being designed privately; it is not activated as a CLI command, installed test
or graphical workflow by these prerequisites. Actual installed read/write/history
acceptance, runtime/module fingerprints and complete desktop journeys remain
separate gates.

## Desktop transaction preparation

The newer working source connects the literal paste codec to an asynchronous
desktop controller and the ordinary worker actor. It retains the exact accepted
binding/path/descriptor/window, validates receipts before adopting history heads,
and distinguishes a confirmed commit from a failed subsequent refresh. Late
replies cannot replace another view, and a selector draft changed before submit
requires a fresh read. Ordinary execution retires frontend history. Uncertainty
blocks range and existing scalar writes until a validated explicit history reset
or successful restart; accepting a read alone cannot clear it.

A known connected precommit rejection clears page acceptance but preserves the
previous context and Undo/Redo heads. It starts no automatic read or retry.
An explicit fresh read restores use of those heads only when the complete
worker/generation/invocation/revision context is unchanged; any context change
retires them. Uncertain or disconnected failures still retire context/history
and retain their existing quarantine/offline safeguards. This correction's three
new controller/state groups passed in the final 122-entrypoint desktop replay,
including both Undo/Redo heads, all four context components and no automatic
read/retry. The earlier 119-test result below remains a separate historical run.

The Engine exposes `is_transport_open()` as a transport-ownership observation,
not a health or readiness check. Closing transport removes that ownership and
prevents further mailbox publication. The actor uses it when an ambiguous edit
has wrapped the original transport error, so a known closed session is reported
as disconnected while uncertain but live sessions can still be read.

Disconnection retains only an exact outstanding envelope long enough to settle a
late valid receipt. That receipt remains a known commit, while the offline view,
history and context stay unavailable. An apparently successful late refresh or
history reset cannot reopen the connection; successful restart clears the latch.

The 19 new entrypoints (nine model, eight controller, one scalar-editor and one
actual-worker journey) passed within the current 119-entrypoint desktop run.
Actor envelopes, outcomes and large success payloads use boxed storage; the
existing transaction tests pass with that indirection and strict Clippy enabled.
The initial run passed 118/119: an old test incorrectly required no quarantine
after a known failed channel send. Its correction asserts the known failed-send
message and separate offline guard, with production policy unchanged. No widget,
clipboard hook, shortcut or layout change calls the new controller yet.
The earlier source-only checkpoint is retained at
`artifacts/cp03-desktop-transaction-source-integration.json`; actual runs are in
the current report linked above. The shared dispatcher
defect documented in [the workspace qualification](WORKSPACE_PROTOCOL.md#dispatcher-identity-qualification)
also remains an integration and runtime gate.
