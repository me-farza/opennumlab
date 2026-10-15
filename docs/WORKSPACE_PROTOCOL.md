# Workspace binding protocol

The ordinary Octave worker advertises `mathlab-worker` version 1 at initial
readiness, recovery readiness and ping. Required capabilities are
`binding_snapshot.v1`, `display_page.v1` and `checked_scalar_write.v1`. Startup
rejects a different protocol, missing required capability or incompatible limits.
The current limits are 2,000 requested page cells and 32 MiB per ordinary response;
these are transport limits, not measurements of physical memory or execution
budgets. Native debugger, scope and input adapters retain their separate gates.

## Identity and invalidation

`SessionContext` contains an opaque `worker_id` plus `generation`, `invocation`
and `revision`. Every counter is a canonical unsigned decimal **JSON string**,
decoded with checked `u64` arithmetic. A `BindingToken` contains that entire
context and a validated variable name. Neither class/shape equality nor equal
values establish binding identity. The random worker ID distinguishes independent
sessions whose numerical counters happen to coincide.

This first contract serves the idle inspector. The interpreter revokes every
old workspace token **before** any potentially effectful ordinary command:
arbitrary execution, file execution, legacy previews/writes, package operations,
path changes and adapter configuration. Failure and cancellation do not undo
assignments that already happened. Recovery changes invocation; restart changes
generation and worker ID. A failed or interrupted write must be refreshed, never
automatically retried.

Only ping, metadata snapshots, checked primitive reads and rejected checked
writes preserve the revision. A successful checked write advances it, invalidating
all previous tokens. The returned receipt identifies the accepted replacement.
The worker compares the complete expected context and name immediately before
lookup and validation in the same owned operation as the eventual assignment.
An old token returns `MathLab:StaleBinding` without assignment.

Input expressions and quiet/streamed paused debugger evaluations run inside the
outer execution interval, whose old tokens were already revoked. The ordinary
engine owner cannot issue new snapshots during that busy interval. Existing
independent input and debugger channels remain responsive; completion still waits
for their established recovery barrier. Tokens are refreshable only after that
settlement. This does not qualify unowned concurrent timer/foreign callbacks;
future paused inspectors and graphics callbacks need shared interpreter-side
invalidation before their capabilities can be advertised.

## Inspection and mutation

`workspace_snapshot` returns metadata and tokens without invoking property
getters, overloaded indexing or display. It enumerates names with the genuine
`who`; even genuine `whos` can dispatch an object's overloaded `size`, so byte
accounting is requested only after proving a primitive runtime class. Objects
report `dimensions_known=false` and `bytes_known=false`; their empty dimensions
and zero byte placeholder are unavailable metadata, not an empty/zero-byte value.
Containers retain their safe outer dimensions but leave recursive byte usage
unknown. Older wire records default both flags to true. The desktop labels
unknown metadata explicitly. Snapshot previews are empty. Checked display
pages support declared primitive numeric/logical values, including complex and
sparse values; they retain the existing first-plane display representation.
They are separate from the optional [lossless N-D service](VALUE_PAGE_PROTOCOL.md)
advertised as `typed_nd_page.v1`. Objects have metadata but
return an explicit unsupported-type error for checked pages or writes.

Checked scalar writes accept the existing validated literal codec, construct a
replacement of a 2-D numeric/logical value, validate bounds/class/shape, advance
the revision and perform one assignment. Malformed literals, unsupported types
and out-of-range indices leave the value and revision unchanged. Metadata in
the receipt uses the same safe snapshot collector. Atomic ranges and undo remain
CP03 work.

The primitive call chain uses genuine builtins, including request decoding,
classification, scalar conversion, metadata/error construction and publication.
It avoids M-file `repmat`/`isequal` in the checked path. Literal builtin-handle
invocation in the base workspace avoids variable precedence for names such as
`builtin`, `who`, `whos` and `feval`. The pinned Octave path uses its genuine
`__fieldnames__` and `__unicode2native__` implementations rather than their
shadowable M-file wrappers. Readiness/ping uses intrinsic `OCTAVE_VERSION`.
Automatic figure enumeration uses `__go_figure_handles__` rather than traversing
M-file plotting helpers for an otherwise numerical command. These dependencies require regression on other
runtime versions; a capability handshake alone does not qualify them.

Existing unconditional APIs remain available for callers that explicitly choose
them. Their effects conservatively revoke snapshot tokens. Desktop read-only
container previews can continue through that legacy path, while editable pages
must retain and submit their accepted token.

## Graphics integration seam

Future opaque graphics IDs must include the negotiated worker ID and generation,
plus an engine-owned object identity/lifetime revision. Raw numeric Octave
handles are not stable identity because deleted handles may be reused. Workspace
effect revision is separate from object lifetime: a property write can revoke
workspace tokens while preserving the same live graphics object. Invocation
and expected object revision must reject queued work from an earlier recovery
or an obsolete/deleted object.

The worker's pure-command whitelist is closed. New property mutations and callback
dispatch therefore revoke workspace revision **before** entering their service,
including error and interruption paths. A new query must remain effectful unless
its exact supported properties and transitive implementation are proven pure.
Callbacks delivered outside the ordinary owner need an interpreter-side shared
invalidation hook before they can be enabled; they cannot retain idle snapshot
tokens merely because no ordinary request was sent. CP04 owns that integration;
this protocol does not advertise a graphics service yet.

## Evidence

The pure protocol tests validate negotiation and lossless counter decoding.
`crates/mathlab-engine/tests/binding_session.rs` exercises actual Octave stale
operations, equal-value rebinding, partial mutation before errors, rejected
edits, cross-worker identity, restart and same-process Stop. Enabled native tests
must be run with matching adapter paths; missing adapters are not native passes.
These checks establish this declared session contract, not complete MATLAB
compatibility, a Rust numerical runtime or a performance advantage.
