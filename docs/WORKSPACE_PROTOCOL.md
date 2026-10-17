# Workspace binding protocol

The ordinary Octave worker advertises `mathlab-worker` version 1 at initial
readiness, recovery readiness and ping. Required capabilities are
`binding_snapshot.v1`, `display_page.v1` and `checked_scalar_write.v1`. Startup
rejects a different protocol, missing required capability or incompatible limits.
The current limits are 2,000 requested page cells and 32 MiB per ordinary response;
these are transport limits, not measurements of physical memory or execution
budgets. Native debugger, scope and input adapters retain their separate gates.

### Readiness JSON producer

The worker explicitly identifies its own Ready replies at initial startup,
same-process recovery, ping and value-adapter installation. A private
`mathlab_ready_json` helper assembles only the three unsigned numeric fields:
protocol version, variable-cell limit and response-byte limit. It validates
primitive dense real scalar input before producing canonical decimal integer
tokens; fractions, negatives, negative zero, nonfinite values and overflow are
errors. It preserves different valid integers for the strict Rust contract to
reject, rather than substituting the expected constants. Native `uint64`
formatting avoids a floating-point intermediate above 2^53.

The helper uses the genuine `__fieldnames__` primitive and the builtin JSON
encoder for field names, strings, capabilities, context and extension fields.
An explicit logical owner flag selects this behavior. All ordinary replies,
including user data with exactly the same structure as Ready, retain builtin
JSON encoding without readiness inspection. UTF-8 conversion and the existing
32 MiB response limit apply before physical publication.

Startup and recovery preflight the Ready response inside their validation
handler. A producer error disables worker recovery before encoding one ordinary
E0 diagnostic, preserving identifiers such as `MathLab:ReadyJsonInteger` instead
of replacing them with Interrupted and repeatedly re-entering startup. Chosen
bytes are published outside that handler; encoding validation must not catch a
file/fence failure and overwrite already committed metadata.

Local Octave 11.3 validation passed the direct producer probe, strict Rust raw
token decoding, startup rejection and recovery-producer failure regressions.
The recovery fault returns the producer-specific diagnostic, closes transport,
does not repeat re-entry and cannot publish subsequent user execution. Exact
uint64-max and 2^53+1 tokens, escaping/extensions, malformed values and ordinary
payload passthrough passed on that runtime. See the retained
scoped validation (local evidence unavailable in source exports: `../artifacts/ready-history-validation/RESULTS.json`).
`MATHLAB_REQUIRE_READY_JSON_TESTS=1` makes the direct producer probe and startup
regressions mandatory when verifying a runtime profile. Exact uint64-max and
2^53+1 output must be exercised on actual Octave 11.3 and hosted 8.4; source
inspection and a local pass alone do not qualify the older runtime. The shared
dispatcher/startup boundary described below remains a separate open issue.

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

`workspace_snapshot` is intended to return metadata and tokens without invoking property
getters, overloaded indexing or display. The selected tested paths enumerate names with the genuine
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

The primitive call chain requests builtins for decoding, classification, scalar
conversion, metadata/error construction and publication; its dispatcher identity
is not yet protected throughout startup and recovery, as detailed below.
It avoids M-file `repmat`/`isequal` in the checked path. Literal builtin-handle
invocation in the base workspace avoids variable precedence for names such as
`builtin`, `who`, `whos` and `feval`. The pinned Octave path uses its genuine
`__fieldnames__` and `__unicode2native__` implementations rather than their
shadowable M-file wrappers. Readiness/ping uses intrinsic `OCTAVE_VERSION`.
Automatic figure enumeration uses `__go_figure_handles__` rather than traversing
M-file plotting helpers for an otherwise numerical command. These dependencies require regression on other
runtime versions; a capability handshake alone does not qualify them.

### Dispatcher identity qualification

The later native graphics probe reproduced a gap in this implementation:
a project `builtin.m` can intercept both bare `builtin(...)` calls and a simple
`@builtin` handle. A literal handle avoids a base **variable** with that name;
it does not retain the intrinsic function object's identity against an M-file
with that name. Consequently the prior selected helper-shadow and object-trap
tests do not establish protection against every project dispatcher shadow.
This also affects the typed-page and pending range-edit services that share
the worker's dispatch path.

Current first startup uses the project directory and inherited search-path
environment before resolving `addpath` and `mathlab_worker`. Same-process Stop
re-entry resolves those names again. Base-workspace query strings contain
`@builtin`, and the separately generated scope registry has its own ordinary
function dependencies. Merely binding a dispatcher in the main worker, or
enabling an adapter after first Ready, does not close those paths. Identity
checks that themselves call shadowable helpers also require protection.

The native graphics probe retained the actual intrinsic function object and
used native base-workspace queries successfully for its declared cases. That
standalone result does not qualify the complete engine startup, registry,
inspection, mutation or recovery chain. A shared implementation must establish
trusted startup before exposing the project path, retain genuine primitive and
worker entry objects across clearing/recovery, cover the complete private-helper
and registry closure, and avoid project name resolution in base queries.
Project-defined functions must remain callable by ordinary user source.

Required causal checks include shadows present before `Engine::start`, inherited
search-path shadows, base variables with helper names, later shadows followed by
`clear functions`/`clear all`, and SIGINT re-entry. Each needs a user-call positive
control plus actual source/runtime identity and unchanged value/revision checks.
Until those pass, the existing capability names identify the implemented
protocol surface; they are not a claim that this transitive dispatch defect is
fixed. The source audit and preserved native probe identity are recorded in
`artifacts/intrinsic-dispatch-audit-20261015.json`.

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
