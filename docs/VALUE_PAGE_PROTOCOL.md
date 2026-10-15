# Typed value pages

This records the committed `3aa3db0` CP03 typed-read data-service slice. The Rust codec and private Octave helper
are connected to Engine's optional checked-page API and the desktop's numeric
inspector. Its combined macOS arm64/Octave 11.3 regression run passed 302 test
entrypoints with the scope, debugger and input adapters enabled, including 12
actual-worker typed-page groups and 84 desktop tests. These results qualify the
declared read subset. Native visual acceptance and
installed-package qualification remain separate.
It extends [the CP02 workspace protocol](WORKSPACE_PROTOCOL.md), committed and
validated separately as `6c575f5`. Complete CP03 acceptance remains unfinished.

The development checkpoint adds a native component route for dense
complex values to avoid losing imaginary negative zero through native selection.
It requires the optional `dense_complex_native_components.v1` capability for
that route and rejects dense complex requests explicitly when the helper is
unavailable. The [follow-up report](../artifacts/cp03-followup-validation/RESULTS.json)
records 404 passing entrypoints across all 19 compiled workspace test targets,
with freshly built scope, debugger, input and value adapters on macOS arm64 /
Octave 11.3. The original typed-page (12), binding (13) and session (56) targets
passed, alongside workspace check, strict all-target Clippy and formatting.
The four crate doctest invocations contained no tests. Exact source manifests,
preserved executables and earlier failed attempts are retained. These results
supersede neither historical evidence nor the separate installed, interactive,
dispatcher, storage-variant and resource gates.

## Ownership and scope

The optional capability is `typed_nd_page.v1`. It does not change the
three required CP02 capabilities or ordinary protocol version 1. The
`read_value_page_checked` dispatch compares the complete root `BindingToken`
with the current worker context immediately before resolving the path, in the
same owned ordinary operation. A stale read fails; it never reacquires a token or
falls back to an effectful preview. The original checkpoint covers reads;
the development write/history contract is described in
[bounded range edits](VALUE_EDIT_PROTOCOL.md). Its desktop actions remain unwired.

Initially supported leaves are primitive double, single, signed/unsigned integer
and logical arrays. Complex leaves require single or double. Sparse leaves are
2-D double or logical values; complex sparse double is allowed. A path can cross
only genuine cell/struct containers. Objects, properties, overloaded indexing,
tables, lazy values and arbitrary expressions are explicitly unsupported.

## Request and selection

The request carries a path with its root token and typed navigation steps:

```json
{
  "path": {
    "root": {
      "context": {
        "worker_id": "example-worker", "generation": "1",
        "invocation": "1", "revision": "2"
      },
      "name": "A"
    },
    "steps": []
  },
  "window": {
    "row_start": "1", "column_start": "1",
    "rows": 3, "columns": 4, "higher_indices": ["4"]
  }
}
```

The example worker identity is illustrative; callers submit the actual accepted
snapshot token. All source extents, absolute indices and starts use canonical unsigned
decimal strings. Requests use positive one-based indices. Bounded requested page
counts are JSON integers. The worker ID is limited to 256 UTF-8 bytes. View axes
are the first two source axes; provide one
explicit index for each remaining axis. The response retains the complete
runtime-reported source shape separately from its clipped page dimensions.

A cell step is `{"kind":"cell","indices":["1","1"]}`. A field step is
`{"kind":"field","indices":["1","1"],"name":"samples"}`. Each step names
exactly one existing container element with one index for every reported axis.
Field names are ASCII identifiers of at most 63 bytes. Paths contain at most
16 steps; rank is between 2 and 32. Initial field navigation supports structs
with at most 256 fields, bounding scalar-selection reference copies separately
from the final page size. The implementation proves the current value
is the required primitive container before querying its shape or indexing it.
An object encountered at any point produces an unsupported result without a
getter, display call or fallback.

The window clips at existing row/column boundaries. Start 1 is accepted for a
zero-length view axis, producing zero cells. Thus `[0,3,2]`, selector `["2"]`,
preserves that source shape and returns zero rows. A zero-length **higher** axis
has no selectable plane in v1 and returns an explicit unsupported diagnostic.
It is not silently collapsed or replaced with an invented plane.

The wire model can express `u64` extents. This Octave helper rejects source
extents and absolute indices at or above `2^53`, because its shape/index boundary
uses doubles. It checks decimal round trips before indexing. Expressible wire
values do not imply the selected backend can execute them.

## Response and exact components

Every response contains schema version 1, the unchanged root token, identical
path steps, `class`, `source_shape`, `source_storage`, `complex`, the accepted
selection and one payload kind. The client validates identity and the complete
selection against its original request before exposing decoded data.

Dense data uses `{"kind":"dense","real":["…"],"imag":[]}`. Components
are column-major: zero-based local offset is `row + column * page_rows`. For
`reshape(1:60,[3,4,5])`, selector `["4"]` returns components 37 through 48;
the displayed rows are `[37,40,43,46]`, `[38,41,44,47]`, `[39,42,45,48]`.
Display text is derived from decoded values and is not the storage codec.

Sparse data uses `{"kind":"sparse_coo","rows":[1],"columns":[2],
"real":["5"],"imag":[]}`. Coordinates are local, one-based, unique and
strictly ordered by column then row. They refer only to the selected page;
global row/column is the corresponding start plus local index minus one.
Missing coordinates mean zero. Explicit zero entries are rejected. This is a
logical nonzero representation, not a promise about stored-zero slots,
allocation capacity, `nzmax` or a native CSC memory layout. The helper indexes
the bounded sparse page before calling `find`; it never requests a full dense
conversion of the source value.

Integer components are canonical range-checked decimal strings and decode
directly into their declared Rust integer vectors. `9007199254741009`, unsigned
maximum and signed minimum never pass through `f64`. Logical tokens are exactly
`"0"` or `"1"`. Real arrays have an empty imaginary component list; complex
arrays have matching real/imaginary lengths in their declared floating class.

Single components use nine significant decimal digits; double uses seventeen.
The decoder accepts a strict finite decimal grammar plus exactly `NaN`, `Inf`
and `-Inf`. It rejects finite overflow and a nonzero lexical value that underflows
to zero. Genuine `-0` retains its sign. NaN payloads and NaN signs are outside
this initial value contract. Empty payloads retain their class, shape, storage
and complex metadata rather than becoming untyped empty lists.

## Bounds and qualification

Before materializing data, validate supported type/path/rank, exact indices,
extent arithmetic and a maximum of 2,000 requested cells. Each component token
is at most 32 ASCII bytes. Sparse entries cannot exceed the page cell count.
The independent typed result budget is 1 MiB, with an additional 4 KiB allowance
for the ordinary response envelope. The engine checks that scoped file/read
budget before parsing its JSON envelope and retains the original raw result JSON
until cancellation settlement. The strict typed decoder checks its result budget
before parsing, preserving duplicate-key rejection as well as unknown fields/tags, malformed
counts, storage mismatches, bad sparse coordinates and inconsistent selections.
The worker's final serialized-size check supplements its earlier construction
bounds; it is not the allocation policy by itself.

The helper is appended as private subfunctions in the owned worker source,
not installed as a shadowable load-path helper. Primitive classification,
conversion, serialization and publication must preserve the CP02 dispatch
policy. The executed cases below qualify selected sparse, floating-point and
inspection paths on the declared local runtime. General allocation behavior,
other runtime versions and platform portability remain unqualified; the selected
measurements do not establish a performance advantage or full compatibility.

The original N-D/integer/complex/sparse/empty fixtures run through the real page
API in `crates/mathlab-engine/tests/value_page_session.rs`, using independent
CP01 oracles. These checks include stale/rebound tokens, rejected object paths,
helper shadowing and exact single/double floating edges. Desktop tests cover
selectors, typed display and the actual worker route. The combined run also
retains enabled console/input/debugger/cancellation regressions. CP03 is not
complete until its atomic range/undo and declared unsupported/error cases also
pass.

## Selected sparse-page measurement

`read_value_page_checked_with_metrics` returns the same checked page plus its
actual result-JSON byte count and elapsed serialization/request/settlement/decode
time. It excludes local request validation, UI rendering and console bytes;
these fields are observations, not allocator counters or numerical-kernel time.

Run the original selected profile with:

```sh
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo run --offline -p mathlab-engine \
  --example value_page_profile -- artifacts/cp03-sparse-profile.json
```

It reads a 30-by-12 window of a 100,000-by-1,000 sparse double array with two
nonzeros, 64 times. The source's dense equivalent is 800,000,000 bytes, while
the selected page contains one nonzero. The example reports actual result sizes
and worker `getrusage` peak RSS before/after the journey, including metadata and
statistic queries. It normalizes Darwin's bytes using the [Darwin manual source](https://github.com/apple/darwin-xnu/blob/main/bsd/man/man2/getrusage.2)
and Linux's KiB using the [GNU resource-usage documentation](https://www.gnu.org/software/libc/manual/html_node/Resource-Usage.html).
Other platforms require their own normalization evidence.

The first macOS arm64/Octave 11.3 development-build observation returned 418
result-JSON bytes per read. The worker peak RSS stayed at 64,569,344 bytes across
this selected journey; median elapsed request time was about 34.1 ms. This does
not mean zero allocation, measure client memory, qualify every sparse input or
compare speed with MATLAB. The retained report and log identify this observation;
the broader API, desktop and regression qualification remains separate.

## Desktop read behavior

The desktop retains the decoded typed payload and derives at most 2,000 display
cells for the current window. Dense indexing is column-major; sparse coordinates
remain sparse and only implicit zeros in that bounded window receive display
strings. Integer formatting uses its native Rust integer type. Higher-axis
controls select explicit planes and show the accepted displayed plane separately
from a pending selection. A late reply cannot replace a newer request or plane.

Only root 2-D values can use the existing CP02 scalar writer. N-D and structured
paths remain read-only until their full-index/path transaction service exists.
A typed error does not refresh, retry or fall back to a first-plane preview.
If an explicit fresh read first acquires metadata but its selectors no longer
fit the new shape, a non-disconnection error can return that metadata alone.
The editor clears the old values and token, corrects its selectors and remains
stale and read-only until another explicit read. An accepted-token error never
reacquires metadata or authority through this recovery path.
Engines without the optional capability retain the declared legacy first-plane
view; explicitly nonprimitive values retain read-only previews. Byte counts come
from safe workspace metadata, not from the displayed window.
