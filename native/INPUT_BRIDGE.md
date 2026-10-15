# Authentic input adapter

`mathlab_input_bridge.cc` is an optional GPL-3.0-or-later adapter compiled against
GNU Octave's public interpreter API. The supported, exercised runtime is GNU
Octave 11.3.0 on macOS arm64. It delegates expression evaluation and return values
to Octave's original `input` builtin. It does not implement another evaluator.

The source uses public `octave_builtin::method()`, the builtin symbol table,
`command_editor` stream setters, `unwind_action`, the existing event manager, and
a platform FILE read callback. macOS uses `funopen`; Linux uses `fopencookie` but
that branch has not been exercised on this host. Windows session launch remains
unsupported by the engine's current console launcher.

## Operations

```octave
info = mathlab_input_bridge ('protocol', 0);
mathlab_input_bridge ('install', input_directory, nonce, generation, invocation, request_id);
mathlab_input_bridge ('context', generation, invocation, request_id);
```

Protocol version 1 reports `protocol='mathlab-input'` and logical true fields
`original_builtin`, `caller_scope`, `separate_reply`, and `genuine_reads`.
`protocol` also accepts no trailing dummy argument. The input directory must
already exist and have an absolute path. A nonce contains 1–128 ASCII letters,
digits, underscores, or hyphens. IDs must be exact positive integers representable
by Octave's JSON numeric values, up to `2^53-1`; request ID zero marks an idle
context. Calling the wrapped builtin without an active request raises
`MathLab:InputContext` before reading recovery stdin.

The engine enables the adapter before its first user execution. A previously
captured original `@input` handle cannot be redirected safely; enabling after user
execution therefore requires a kernel restart. `context` changes the request
identity without replacing the transport or numerical workspace.

An internal `detach` operation restores the original builtin. Handles already
captured from the wrapper remain valid native objects and raise `InputContext`
while detached. Detach is not a desktop input command: the worker owns the stdin
recovery route and should retain its installed adapter while active.

## Genuine reads and the wire contract

The wrapper validates and normalizes the original prompt/mode arguments, installs
a temporary FILE input stream, and invokes the saved original builtin callback
directly. This preserves the caller's scope and `builtin('input',...)` dispatch.
The original command editor prints and flushes the prompt. Only when it actually
reads does the FILE callback publish an input request and wait for a private
reply file. A string resembling a prompt in ordinary output never creates an
input request.

The short control records share the engine's common console pipe:

```text
RS MATHLAB:nonce:generation:I:invocation:request_id:input_id US
RS MATHLAB:nonce:generation:F:invocation:request_id:input_id US
```

Each fence is flushed and written in one OS write, at most 512 bytes. Metadata is
written to a bounded temporary file and atomically renamed before its fence:

| File in the input directory | Fields |
| --- | --- |
| `input-request-G-I-R-N.json` | `generation`, `invocation`, `request_id`, `input_id`, `prompt`, `mode` (`expression` or `string`) |
| `input-finish-G-I-R-N.json` | Full identity plus `outcome` (`line`, `eof`, `interrupted`, or `error`) |
| `input-reply-G-I-R-N.json` | Full identity plus logical `eof`, string `text`, and logical `echo` |

Reply `text` is at most 1 MiB of valid UTF-8 and contains no NUL, CR, or LF. Empty
text is a valid empty line. EOF requires `eof=true` and empty text. Serialized
reply files are bounded at 8 MiB to accommodate JSON escaping. Prompts are bounded
at 64 KiB of UTF-8, and each encoded metadata file at 128 KiB.

The native callback echoes an accepted line once when `echo=true`, then publishes
F when its bytes have been delivered through the FILE reader, before the original
builtin evaluates the expression. A terminal frontend can set `echo=false` when
its terminal already echoes input. EOF emits F with `outcome='eof'` and preserves
the original builtin's resulting error.

Finishing the read before expression evaluation allows entered code to call
`input` again with a fresh ID. While one read is still awaiting a reply, another
input read requested by a debugger callback raises `MathLab:InputBusy`. It cannot
replace the first waiter or emit a second simultaneous I.

## Cancellation, debugger cooperation, and lifetime

The wait checks `octave_quit()` and services the already installed event manager;
it does not install a second `interpreter_events` owner. Interpreter/debugger
exceptions are caught inside the FILE callback, which returns an I/O error to
libc. The wrapper rethrows the saved exception only after the original stdio
reader returns. This avoids unwinding through libc while its FILE lock is held.
After a genuine I, an aborted read emits F with `interrupted` or `error` only
after returning from libc, before rethrowing its original exception. This clears
the pending read even if user code catches a transport error and asks for input
again in the same execution. It does not claim that a line was consumed. If
publishing F itself fails, the engine's Recovery/End/disconnect path invalidates
the pending input.

The debugger retains the verified adapter function handle and invokes its
internal `cancel_wait` operation before Quit. It marks only the current reader;
the read then unwinds with an interpreter interrupt. This lets Octave's debugger
loop regain control and honor its authentic Quit state. Calling `dbquit` alone
only sets that state and cannot release an input waiter. No cancellation bytes
are written to recovery stdin. During that unwind, the debugger must temporarily
disable Octave's per-statement debug-mode interrupt recovery, which would
otherwise run the next watch statement after cancellation. Its evaluation guard
restores the prior mode afterward; user `unwind_protect` cleanup still runs.
Actual SIGINT during watch input or entered expression evaluation follows the
same stop boundary through a temporary `octave_signal_hook` at Octave's normal
interpreter safe point. Only the outermost watch owns that hook and mode restore,
so nested watches cannot restore debug mode while an older watch is unwinding.
The hook marks the expected stop and requests `dbquit(true)` before Octave throws
its real interrupt. Once the watch unwinds, Octave's debugger loop performs its
genuine Quit transition, stopping the enclosing execution too. The worker maps
that result to `MathLab:Interrupted` when the durable cancellation marker records
Stop; an explicit debugger Quit retains `MathLab:DebugQuit`. The debugger chains
the previous signal hook and restores it after the evaluation.

A temporary input stream is always closed and the previous command-editor stream
restored on normal return, error, SIGINT, or debugger unwinding. Replies never use
the stdin stream reserved for worker recovery.

The module locks its transport entry and also retains one process-lifetime
`dynamic_library` reference. Builtin callbacks and saved `@input` handles therefore
cannot point into an unloaded module after explicit `munlock`, `clear functions`,
or detach. This bounded lifetime pin owns adapter code and metadata only; it does
not pin numerical functions, user handles, persistent variables, or source
registries.

## Errors and compatibility boundaries

`InputContext`, `InputIdentity`, `InputProtocol`, `InputBusy`, `InputPrompt`,
`InputReply`, `InputTransport`, and `InputPlatform` use the `MathLab:` identifier
prefix. Original builtin validation and evaluation errors retain their own
identifiers/messages.

An empty expression line returns an empty double matrix. An empty string line
returns an empty char value. EOF in either mode raises the original message
`input: reading user-input failed!` with an empty identifier on the tested runtime.
Invalid syntax and undefined expressions raise their original parse/evaluation
errors after one input request. Octave 11.3.0 does **not** automatically reprompt
these expressions; MATLAB-style retry behavior remains a compatibility gap.

The old debugger watch implementation used `evalc`. A paused `input` reproduced a
segmentation fault even with the unmodified original Octave builtin: input resets
the pager output stream that `evalc` later tries to restore. The debugger adapter
must use its safe streaming evaluation path rather than wrapping input in evalc.

The implementation follows the official versioned
[input source](https://raw.githubusercontent.com/gnu-octave/octave/release-11-3-0/libinterp/corefcn/input.cc)
and [command editor source](https://raw.githubusercontent.com/gnu-octave/octave/release-11-3-0/liboctave/util/cmd-edit.cc),
with debugger cancellation behavior verified against the
[evaluator source](https://raw.githubusercontent.com/gnu-octave/octave/release-11-3-0/libinterp/parse-tree/pt-eval.cc).
Actual engine integration tests exercise caller scope, string/empty input,
`builtin('input')`, nested entered input, EOF, clear behavior, interruption,
restart, and obsolete-key rejection. Temporary native probes additionally verified
multi-output `deal`, retained input handles after explicit module unlock/clear,
malformed-reply error followed by another input in the same execution, and the
unwrapped paused-evalc failure used to diagnose the debugger change. Paused native
probes exercise both quiet and live watch output, caller-local expression input,
the busy guard, Quit without continuation, user cleanup, and same-process reuse.
Six direct SIGINT probes cover quiet/live evaluation crossed with a waiting read,
an entered expression running after F(line), and a nested watch inside the outer
read. Each stops the enclosing execution, releases all pending watch responses,
runs cleanup without continuation, retains base variables, and accepts fresh
input in the same process. The checked native evidence is retained in
[`debug-input-stop-report.json`](../artifacts/debug-input-stop-report.json) with
its [probe](../artifacts/debug-input-stop-probe.py), plus the separate quiet/live
Quit [report](../artifacts/debug-input-quit-report.json) and
[probe](../artifacts/debug-input-quit-probe.py). These checks establish the tested
adapter behavior on the runtime above; they do not establish complete MATLAB
compatibility.
