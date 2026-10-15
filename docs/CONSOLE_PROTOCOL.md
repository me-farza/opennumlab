# Ordered session console

Updated October 14, 2026. The engine, desktop and CLI implement a live console
byte stream with invocation-aware interruption, completion and optional authentic
`input()` replies. The debugger's protocol-4 scoped evaluation no longer uses
`evalc`. These are selected implemented behaviors, not complete MATLAB console or
input compatibility, and no performance advantage has been established.

Runtime evidence uses GNU Octave 11.3.0 (Homebrew 11.3.0_1) on macOS arm64. Targeted
input and frontend evidence is listed below. Earlier full-suite/package records
in [VALIDATION.md](../artifacts/VALIDATION.md) and
[verification.log](../artifacts/verification.log) must be read with their recorded
scope; they do not by themselves validate a later input/debugger revision. The
actual desktop Stop-after-input-consumption regression passes against the final
native interrupt adapter; its bounded acceptance scope is described below.

Windows pipe/helper source exists, but Engine startup explicitly rejects Windows
until argument dispatch and recovery are validated.

## Implementation and ownership

- [Rust console transport](../crates/mathlab-engine/src/console.rs): merged pipe,
  byte framing, bounded capture, subscriptions, diagnostics and reader shutdown.
- [Engine session](../crates/mathlab-engine/src/lib.rs): mailbox requests,
  structured metadata, completion, cancellation and worker re-entry.
- [Octave worker](../octave/mathlab_worker.m): actual user evaluation, source
  errors, workspace/figure results, metadata publication and fences.
- [Rust fence helper](../crates/mathlab-engine/helpers/console_fence.rs), compiled
  by [build.rs](../crates/mathlab-engine/build.rs): one short OS write per fence.
- [Input state and reply handle](../crates/mathlab-engine/src/input.rs) and
  [native input adapter](../native/mathlab_input_bridge.cc): authentic read
  notifications and a reply mailbox independent of engine execution and stdin.
- [Desktop console](../crates/mathlab-desktop/src/console.rs),
  [desktop input controller](../crates/mathlab-desktop/src/input.rs),
  [CLI printer](../crates/mathlab-cli/src/streaming.rs) and
  [CLI stdin router](../crates/mathlab-cli/src/stdin_input.rs): bounded consumers
  and frontend reply delivery.

User execution no longer runs inside the worker's evalc. The engine connects
Octave stdout and stderr to the **same OS pipe**, which inherited subprocess
descriptors also use. A dedicated reader consumes bounded byte chunks, including
output without a terminating newline. There is no second interpreter-output
string to prepend or sort afterward, and no unbounded stderr.log file.

Deliberate internal `evalc` captures remain, including graphics bootstrap and
formatting. User execution and scoped debugger evaluation use different paths;
removing their `evalc` wrappers does not remove every internal capture.

## Byte-order contract

Console order is the order of bytes read from the common OS pipe. It includes
language output, stderr, warnings and inherited system() output without later
reordering. Output can arrive during execution and while the engine owner is idle.

This does not reconstruct universal source-statement order. Application buffering
can delay an earlier operation, concurrent writers can interleave, and a large
write can span several reader chunks. The merged pipe also loses stdout/stderr
channel identity and does not identify the originating subprocess. The API calls
the result console; it does not invent per-byte provenance.

The request_id field is transport context, not proof that every byte was produced
by that request. A background child can write between requests or during a later
request. Idle output remains visible with no active request ID. Bytes that arrive
during a request contribute to its compatibility aggregate even if a background
child was the actual writer.

## Fences and metadata

The wire fence is a short ASCII record, shown here with named control characters:

    RS MATHLAB:<nonce>:<generation>:<kind>:<invocation>:<request_id> US

RS is byte 0x1e, US is byte 0x1f, and these kinds are B (Begin), E (normal End)
and R (interruption/recovery). Authentic input adds an input ID:

    RS MATHLAB:<nonce>:<generation>:I:<invocation>:<request_id>:<input_id> US
    RS MATHLAB:<nonce>:<generation>:F:<invocation>:<request_id>:<input_id> US

I announces an actual read; F reports consumption or an aborted read. Both must
match the active invocation/request and validated input metadata. These records
are not ordinary Begin/End spans and do not complete the numerical request.

A fresh random nonce and generation identify each new
Octave process. Re-entering the worker after an interrupt increments invocation
within the same process. Request IDs increase within the engine session; ID zero
is reserved for each invocation's readiness handshake.

The worker flushes language stdout and stderr, then invokes the extracted Rust
helper with exactly one UTF-8 fence argument. The helper validates framing and the
512-byte maximum, writes inherited stdout once, and prints no diagnostics. Unix
uses write(1, ...), retrying only EINTR with no bytes written; a positive short write
fails instead of splitting the fence. The size is within the conservative POSIX
minimum PIPE_BUF. Windows source uses one WriteFile call and remains an unverified
platform path. No Octave C++ module or mkoctfile is needed for fences.

Cargo builds the standalone, standard-library-only helper for TARGET, embeds its
bytes in the engine executable, and extracts it into the private session directory
with mode 0700 on Unix. The worker uses builtin popen(command + ' 1>&2', 'r') and
pclose so waiting for the helper preserves SIGINT handling. The shell redirects
helper stdout to inherited stderr, which is the same common console pipe; no fence
bytes enter popen's unused capture pipe. The earlier system() path temporarily
ignored SIGINT and could lose an early Stop, so it is not used for fences.

Large structured replies never share the console byte stream. The worker writes a
bounded UTF-8 JSON temporary file, closes it, renames it, then publishes its fence.
Normal files use `response-<invocation>-<request_id>.json`; recovery files use
`interruption-<invocation>-<request_id>.json`. The engine derives these paths from
validated identifiers. A fence cannot supply an arbitrary filename. Recovery
metadata is separate so interruption cannot overwrite a normal response whose End
may already be visible.

Seeing a metadata file alone is not completion. The engine reads it after the
corresponding fence has passed through the same pipe as prior output. This is the
ordering barrier between console data and structured results. Metadata is checked
before parsing with a bounded read, including growth beyond its initial file size.

The reader scans framing on raw bytes before any UTF-8 decoding. It retains a
bounded possible prefix across reads. Other nonces or generations, public
`__MATHLAB_JSON__` lookalikes, NULs and invalid UTF-8 remain ordinary data. Once the
full current nonce/generation prefix is recognized, malformed, oversized and
incomplete control bodies are protocol failures. EOF after only a partial prefix
flushes those undecidable bytes as data; EOF after a full recognized prefix reports
an incomplete fence. Bytes preceding a malformed fence are still published.

The nonce prevents accidental control collisions. It is not a security boundary
against trusted code that deliberately discovers and writes the session framing.

## Completion and interruption

Each invocation first publishes Begin/End for request zero with real version and
working-directory metadata. Ordinary sequential requests then have one active
capture between Begin and their normal End or recovery fence.

The recovery state machine handles the fence-helper race explicitly:

- Recovery before a request's End finishes its existing capture and retains all
  already emitted bytes. If SIGINT ran cleanup before Begin could be written, the
  reader exposes an empty interrupted Begin/End span. User evaluation has not run
  before successful Begin publication.
- Recovery after a normal End does not replace that result or publish a second
  public End. It still requires re-entry of the unwound worker.
- Recovery closes the invocation. Only request-zero Begin for exactly the next
  invocation opens another one. Well-formed late fences from older invocations or
  already completed spans cannot reopen or reset a capture.
- A new ordinary End without its matching Begin is an error. The special orphan
  recovery case does not make normal orphan completion valid. Syntax validation
  happens before late-frame filtering, so a malformed fence with the current
  nonce/generation is still an error even if its invocation would have been old.

Unix cancellation writes a generation/request marker and sends SIGINT while holding
the process lock. The marker covers cancellation before the worker consumes its
mailbox, so re-entry does not accidentally execute the canceled source. Repeated
Stop calls do not repeatedly signal the same pending interruption. A normal End
and cancellation activation share that lock. If a signal was actually sent, the
engine additionally settles recovery or services a fresh ping before publishing
completion to the caller. Recovery readiness is proved using request zero before
the session is treated as ready again.

Public completion cursors can therefore extend through readiness or cancellation
settlement. Consumers use the returned cursor rather than assuming that the
original request's first End is the last required session event.

Actual Octave regressions verify cancellation before Begin, before mailbox
consumption, between metadata and End, and after End while settling recovery. They
also verify that unrelated idle recovery does not disarm a queued request's
cancellation; see [the boundary-test log](../artifacts/console-engine-boundary-tests.log).
These are tested cases on the stated runtime and platform, not an exhaustive
cross-platform guarantee. Hard termination or deadline failure retains already
delivered console events but loses the numerical workspace. Native routines may
defer SIGINT until they return. Non-Unix interruption currently requires force
termination.

## Public subscriptions and result compatibility

Engine::subscribe_console() returns an independent ConsoleSubscription. It is valid
while the numerical owner is blocked in execution, while it is idle, and across a
hard engine restart. Each ConsoleEvent contains generation, optional request ID,
a monotonically increasing session sequence and one of:

    Begin
    Output       { bytes }
    OutputLimit   { dropped_bytes }
    InputRequested { request }
    InputFinished  { finished }
    End
    Disconnected  { message }

The public stream has no separate Recovering, WorkerReady or DebugPaused event.
Readiness and debugger state retain their structured control paths. Input events
come only from genuine interpreter reads, never from recognizing printed text.
The reliable pending-input snapshot described below remains authoritative if a
console subscriber loses either input notification.

The initial_sequence() value is fixed when subscribing. Events before that point
are not replayed. The latest_sequence() method snapshots the current published
position and retains the final cursor if the hub is gone. The try_recv() and
recv_timeout() methods operate on that subscriber's bounded queue. A slow subscriber
loses old events with an explicit loss marker and does not block another subscriber,
numerical computation, cancellation or the completion owner. A loss marker can
report zero dropped bytes when only control/input metadata events were evicted.
Queue storage weight includes retained input prompts, while `dropped_bytes` counts
only evicted raw Output bytes. Losing metadata does not invent console-text loss.

For aggregate-only callers, ExecutionResult.output is assembled from the same
ordered raw bytes. It reports output_truncated, output_dropped_bytes and
output_decoding_loss, plus console_generation and console_end_sequence for consumers
that also subscribed. Invalid or incomplete UTF-8 becomes replacement characters
in this display string; subscriptions keep raw bytes within their delivery limits.
The aggregate never includes framing bytes.

## Actual resource bounds

| Resource | Implemented bound and behavior |
| --- | --- |
| One pipe read | 8 KiB. It does not wait for a newline. |
| One recognized fence | 512 bytes. A possible prefix/body is carried across read boundaries; processing temporarily also holds the current bounded read. |
| One request aggregate | The first 32 MiB of raw console bytes. Further bytes continue to stream and increase the explicit dropped-byte count instead of terminating the numerical request. Lossy decoding can expand the retained text to roughly three times the raw-byte size. |
| One metadata file | 32 MiB, checked by worker publication and a bounded engine read. This is not a bound on all allocations used to construct metadata inside Octave. |
| One subscriber queue | 1 MiB of retained event payload and 256 events, plus one coalesced loss marker. Old events are evicted; final/control events remain reachable through a later event or loss cursor. |
| Private owner-control queue | Four control slots, separate from console subscribers. Each completion can contain a bounded aggregate. Overflow is a fatal protocol error rather than blocking the reader or growing without limit. The four-slot count is not a 1 MiB byte cap. |
| Diagnostic tail | Last 8 KiB of raw session output; failure diagnostics retain at most 2,000 decoded characters. There is no growing stderr log. |
| Desktop scrollback | 16 MiB of retained UTF-8 text and 4,096 entries. Adjacent output coalesces into entries of at most 64 KiB; trimming invalidates affected source links. |
| Desktop drain per update | At most 256 events or 256 KiB of counted payload (raw output, retained input prompt or disconnect message), with a possible final-event overshoot. Continuous output cannot keep the GUI inside an unlimited drain loop. |
| Input text and prompt | One reply is at most 1 MiB of UTF-8 without NUL/CR/LF; one prompt is at most 64 KiB. Empty lines are valid. |
| Input files | Request/finish metadata is at most 128 KiB; a serialized reply file is at most 8 MiB to allow JSON escaping. Native and Rust readers validate bounds and full identity. |
| Reliable input state | One pending read, with Waiting/Submitted phase, behind a mutex independent of the numerical owner. A second simultaneous read is rejected. |
| CLI stdin routing | One raw reader; at most eight queued routing tasks. Reads are 8 KiB, logical lines are capped at 1 MiB, and an oversized line is discarded through its terminator without becoming a later command. |
| Scoped debugger output | Each evaluation captures at most 1 MiB of observed raw language/C++ output. UTF-8 replacement can expand returned text to 3 MiB plus a short clipping notice. Live forwarding continues after the capture fills. Nested evaluations have separate captures. |

The reader has an independent stop token and nonblocking/cancellable I/O. On Unix
it polls for at most 25 ms between checks. Restart and teardown do not wait for EOF
from a background descendant that retained the write descriptor. Control delivery
uses nonblocking try_send; it cannot hang reader shutdown on a full owner queue.

These bounds cover this transport and display history. They do not establish a
session-wide memory/disk budget for matrices, workspace count, figures, immutable
source generations, native objects or files written by user code. Multiple
subscribers and queued decoded aggregates have separate allocations.

## Desktop and CLI behavior

The desktop registers a session subscription, including idle/background output.
It decodes UTF-8 incrementally, carrying an incomplete codepoint across byte chunks
and replacing malformed input for display. Begin, End, generation changes,
disconnection and every loss marker finalize pending text so unrelated requests do
not accidentally combine UTF-8 tails. This includes zero-byte loss of control
fences; only a positive byte count adds a visible byte-loss notice. A completion
event waits until the console cursor reaches its returned boundary or an explicit
loss marker passes it. Existing
partial text remains when an error or disconnection arrives, and the final aggregate
is not appended a second time. Old-generation events cannot resurrect stale state.

Non-JSON CLI eval, run, REPL and package loading use an independent printer thread
that writes raw bytes and flushes while execution continues. It synchronizes with
the returned completion cursor before printing workspace/results. Error and other
metadata paths wait for a fixed latest-sequence snapshot, so continuing background
output cannot move the target forever. A REPL printer survives restart and remains
active while waiting for the next command. Its teardown does not join indefinitely
on a blocked stdout writer; ordinary terminal/pipe backpressure can still delay the
printer without blocking the numerical engine reader.

CLI JSON eval/run remain aggregate-only, with the output truncation and decoding
fields. JSON REPL emits JSON records without a banner, prompt or live raw bytes.
Package JSON preserves its existing output string API; it does not yet expose
independent package-output truncation metadata. Compatibility-manifest checks also
remain aggregate automation commands. Streaming clients must not append the
aggregate again on completion.

## Authentic input and reliable state

Input is opt-in through an ABI-matched
[`mathlab_input_bridge.oct`](../native/INPUT_BRIDGE.md). Build it with
`scripts/build-debugger.sh`, then set `MATHLAB_INPUT_BRIDGE` before starting a
session, or call `Engine::enable_input_adapter` before its first user execution.
The build script produces debugger, scope and input modules; the verification
script's `--with-debugger` mode enables all three. The macOS package's native
launcher exports its bundled input-module path when present. Without this module,
there is no supported desktop reply transport; worker recovery stdin is not a
fallback answer channel. Previously captured original `@input` handles cannot be
safely redirected by late activation, which is rejected until a kernel restart.

The native wrapper saves Octave's original builtin callback, installs a temporary
FILE reader and calls that callback directly. The original command editor prints
and flushes the prompt. The adapter announces I only when that reader actually
needs bytes. This preserves the original caller frame for expression input,
string input, `builtin('input', ...)` and nested entered expressions. Ordinary
prompt-like output creates no waiting state. The native adapter does not replace
the evaluator or infer whether an input call happened from its output.

I and F metadata live under the private input directory. Their filenames and
payloads include `InputKey { generation, invocation, request_id, input_id }`.
Normal reply files contain the same key, an EOF boolean, text and an echo flag.
The Rust reader validates the active console span, metadata identity and limits
before accepting a notification. It assigns `InputRequest.console_sequence` while
reserving the public I-event sequence and updating the reliable input state.
A frontend waits for that position, or a loss cursor past it, before displaying an
enabled reply control; a pending snapshot alone must not overtake the prompt.

`Engine::input_handle()` returns an optional cloneable handle independent of the
thread blocked in `execute`. Its `pending()` method returns the current
`PendingInput { request, phase }`, with Waiting or Submitted phase. This coalesced
snapshot is reliable even when a bounded console queue evicts I/F notifications.
`reply(&key, text)` and `eof(&key)` validate the complete current identity and
publish an atomic reply without using the serialized engine request queue. The
same state mutex protects validation, publication and lifecycle changes, so an
old key cannot publish an answer into a later waiter. Duplicate replies and stale
generation/invocation/request/input keys receive errors. A cloned handle survives
restart, while its old keys do not.

Submitted means the reply file was published, not that expression evaluation
succeeded. F with Line means the original reader consumed its bytes; it occurs
**before** Octave evaluates that expression. Entered code can therefore request
input again with a fresh key. While a read is still pending, a second simultaneous
read raises `MathLab:InputBusy` instead of replacing the waiter. EOF is a distinct
action; an empty string is a valid line. On the checked Octave runtime, EOF keeps
the original `input: reading user-input failed!` error. Invalid syntax and undefined
expressions also keep Octave's errors and do not automatically reprompt; MATLAB
retry behavior remains a compatibility gap.

Native F outcomes are Line, Eof, Interrupted and Error. Recovery, End or pipe
failure also invalidates pending state; the public API can report Disconnected.
The adapter restores the original FILE stream on return, error and interruption.
It defers exceptions until libc has released its FILE read frame, avoiding an
unwind through a locked stdio callback. Replies never share worker recovery stdin.
A verified input-adapter handle lets the debugger cancel an active read during
Quit. A canceled read and a long computation after Line consumption are separate
cases. The actual desktop regression checks Stop during that later computation,
after the reliable pending-input state has cleared.

## Frontend input behavior

The desktop polls the reliable handle while execution remains busy. Its Command
Window shows a separate reply field only for an authentic pending read and enables
submission after the prompt barrier. Replies preserve leading/trailing whitespace
and allow an empty line. They do not enter command history, replace the ordinary command draft,
add a new source request, or evaluate code in Rust. Native code interprets
expression input and echoes accepted text once. The desktop rejects multiline or
NUL paste rather than allowing the single-line widget to silently rewrite it.
Send EOF and Ctrl+D in the focused reply field send EOF, not an empty reply.
Submission disables duplicate Enter until the read finishes; errors retain the
reply for correction. Source dispatch, debugger resume/evaluate and breakpoint
changes stay blocked while waiting. Stop uses the independent cancellation route,
including a pending user Evaluate after input consumption. Completion, disconnect,
restart and a new session clear the input view and reject stale notifications.
The existing screenshot mode can capture an authentic waiting-input state after
its prompt barrier, as it can a debugger pause.

The CLI uses one raw stdin owner for REPL commands and input replies. Non-JSON
eval/run/package calls and JSON calls create an input responder when the adapter
is enabled; REPL commands and replies share its router. It polls real pending
state, waits for the console-printer cursor when live printing is enabled, and
submits a line directly through the input handle. LF terminates a logical line;
CRLF removes its one trailing CR without trimming the reply's other whitespace.
EOF is sent separately. Native echo is enabled for pipes and disabled when Unix
terminal settings show that the terminal already echoed the line. Neither frontend
prints another copy of the accepted reply. A line already classified as a canceled
answer is discarded instead of later becoming an executable REPL command;
unclassified bytes are preserved for the next routing task.

JSON output remains aggregate-only: prompts and native echoes appear inside the
result's output string rather than as raw stdout or guessed prompt records. JSON
REPL has no banner or `>>` prompt. Stdin routing currently requires Unix; line
editing, arbitrary terminal control, password-style reads and all MATLAB input
variants have not been established.

## Scoped debugger output

The debugger requires `mathlab-debugger` protocol 4 with `streamed_evaluation` and
`input_wait_cancel` in addition to its source/breakpoint capabilities. Both quiet
and live scoped evaluations call the actual paused evaluator directly, without
`evalc`. A bounded observer records language-output callbacks and stable C++
stdout/stderr sinks; it does not retain or replace the pager buffer that input
resets. The old paused-evalc path reproduced a segmentation fault even with
unmodified Octave input and has been removed from this path.

`Debugger::evaluate(code, revision)` remains the quiet string-returning action,
used for validated primitive previews. `evaluate_output(code, revision)` uses the
live `evaluate_stream` action and returns `EvaluationOutput { output, streamed,
truncated }`. Live mode forwards observed text to the same session pipe and also
keeps its bounded returned copy. The cap applies to observed raw bytes; the public
Octave UTF-8 validator replaces invalid bytes before JSON encoding, including a
clipped multibyte tail. Replacement can expand the returned text to 3 MiB plus
the notice. Quiet
mode suppresses forwarding of those observed sinks. It is not a sandbox or complete
OS-descriptor capture: direct stdio used by input, inherited subprocess writes and background output still
belong to the session stream. Original input prompts/echo remain visible in either
mode. The 1 MiB clipping flag describes the returned copy, not a cutoff of live
output; the public subscriber/scrollback limits apply independently.

The desktop uses live evaluation for user expressions and quiet evaluation for
its primitive previews. With a console subscription active, it shows a Command
Window destination notice instead of displaying the aggregate a second time.
Standalone/mock capture responses remain visible in the debugger pane. Revision
checks and the existing independent command worker still apply. Nested breakpoints
can be continued by another client thread while evaluation is pending. A timeout
alone does not terminate evaluation. Pending user evaluation can outlive the input
read itself, which is why its Stop regression checks after the entered expression
has begun. During that signal, the outermost watch guard also requests genuine
debugger Quit so the enclosing execution ends rather than remaining paused. The
worker uses the durable cancellation marker to report `MathLab:Interrupted`,
distinguishing Stop from an explicit debugger Quit. Failed watch responses release
all pending clients during unwind. Full MATLAB debugger compatibility remains
unproved.

## Platform and compatibility limits

Windows has Rust CreatePipe/PeekNamedPipe and helper WriteFile source, but Engine
startup explicitly rejects Windows. Worker helper dispatch uses POSIX shell
literal quoting, and same-process SIGINT recovery is specific to the checked Unix
implementation. Native input uses macOS funopen; Linux fopencookie source exists
but is not exercised on this host. Native modules require a matching Octave ABI.
No cross-platform atomic-write, MATLAB input parity or performance claim follows
from the macOS tests. Interactive terminal emulation, password input and other
readers such as arbitrary `fscanf(stdin, ...)` are outside the genuine `input()`
contract described here.

## Tests and evidence

[Transport unit tests](../crates/mathlab-engine/src/console.rs) exercise every
framing split, raw lookalikes, malformed/oversized/partial control records, bounded
subscriber loss, aggregate clipping, cancellable shutdown with an inherited writer,
retained final cursors and recovery ordering. They also distinguish orphan recovery
from an ordinary orphan End. Actual subprocess evidence is required in addition to
these tests.

[Engine integration tests](../crates/mathlab-engine/tests/octave_session.rs) cover
live non-newline output before completion, interleaved language/system/stderr output,
split and invalid UTF-8, output flooding with a slow subscriber, idle background
output, hard-exit partial output, restart and interruption. The
[CLI integration tests](../crates/mathlab-cli/tests/console_stream.rs) use a
release-file gate to prove output arrives before evaluation can finish, then check
single delivery, timeout partial output, JSON metadata, idle background output and
REPL restart. Desktop tests cover its real worker subscription, event barriers,
decoding, loss and display limits. The
[boundary-test log](../artifacts/console-engine-boundary-tests.log) records targeted
request/fence interruption regressions.

[Input integration test sources](../crates/mathlab-engine/tests/input_session.rs)
include caller scope, Unicode/string/empty replies, builtin and nested input, EOF,
clear behavior, SIGINT/restart, malformed replies and late-activation rejection.
[Input engine evidence](../artifacts/input-engine-tests.log),
[CLI evidence](../artifacts/cli-input-tests.log),
[terminal echo probe](../artifacts/cli-input-tty-probe.json),
[desktop unit evidence](../artifacts/desktop-input-tests.log) and
[paused desktop reply evidence](../artifacts/desktop-paused-input-test.log) record
specific runs. The [final desktop worker group](../artifacts/desktop-input-native-tests.log)
passes all five actual-worker tests with the debugger and input adapters enabled. A successful Rust test that takes an optional-adapter skip branch
is not evidence that its native path ran. The [actual desktop Stop
regression](../artifacts/desktop-app-stop-test.log),
`app_stop_interrupts_a_paused_evaluation_after_input_has_already_been_consumed`,
passes after the entered expression has begun and the read has finished. It checks
outer `MathLab:Interrupted` completion, prompt release of the pending Evaluate,
cleared busy state, no queued source submission and preserved base workspace in a
subsequent request. The [native causal Stop report](../artifacts/debug-input-stop-report.json)
separately checks quiet/live modes at a waiting read, during the entered expression
and in nested watches, including cleanup and absence of later statements.

Full enabled/fallback verification, static analysis and packaged evidence are
recorded separately in [VALIDATION.md](../artifacts/VALIDATION.md); check their
revision and enabled modules before applying an older result to new behavior.

The original audit probes below explain why the former protocol was replaced.
They are **historical measurements of the old evalc implementation**, not current
regressions or evidence that the old behavior persists:

| Probe | Observed old behavior | Recorded output |
| --- | --- | --- |
| disp('A1'); system('printf B2'); disp('C3'); fprintf(stderr,'D4'); in evalc | Raw stdout was B2; captured output was A1\nC3\nD4. Prepending reordered foreground output. | [order](../artifacts/console-output-probe-order.json) |
| Public interpreter_events observer, server mode disabled/enabled | Only enabled mode forwarded language stdout fragments; stderr, warnings and system output bypassed it. | [events](../artifacts/console-output-probe-events.json) |
| Piped input('VALUE> ','s') | Prompt and response worked through raw stdin, but public input/prompt events did not prove an input-request hook. | [input](../artifacts/console-output-probe-input.json) |
| Two non-newline writes separated by pause(.4) with immediate output | Raw byte reads received PARTIAL about 405 ms before DONE; line buffering was unnecessary. | [partial bytes](../artifacts/console-output-probe-partial.json) |
| Print, assign interrupt_saved=7, loop, then SIGINT | Old captured output was empty while workspace value 7 survived. | [interrupt](../artifacts/console-output-probe-interrupt.json) |

Those timings are illustrative, not a latency benchmark. The temporary native
event observer used for the audit was not an installed product component. Public
Octave headers showed why observing language stdout alone could not capture all
inherited OS output; merging descriptors and removing execution-time capture
provides the present byte-order contract.
