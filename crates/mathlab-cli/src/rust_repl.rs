// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit persistent Rust route. It never constructs or discovers an Engine.

use crate::stdin_input::StdinRouter;
use anyhow::Result;
use mathlab_engine::rust_session::{
    RustBindingEffectV1, RustEvalOutcomeV1, RustRequestKeyV1, RustSessionContextV1,
    RustSessionErrorV1, RustSessionV1, RUST_SESSION_PROFILE_V1,
};
use mathlab_language::session_expression::{SessionExpressionErrorV1, SessionValueV1};
use serde_json::{json, Value};
use std::io::{self, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Reply {
    record: Value,
    text: String,
    error: bool,
}

pub(super) fn run(as_json: bool, timeout: Option<Duration>) -> Result<()> {
    let mut session = RustSessionV1::new()?;
    let router = StdinRouter::start()?;
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    if !as_json {
        writeln!(
            stdout,
            "MathLab {} — experimental persistent Rust session",
            env!("CARGO_PKG_VERSION")
        )?;
        writeln!(
            stdout,
            "Single-statement real arithmetic; :workspace  :clear  :restart  :quit"
        )?;
    }
    loop {
        if !as_json {
            write!(stdout, ">> ")?;
            stdout.flush()?;
        }
        let Some(line) = router.read_command()? else {
            write_exit(&mut stdout, as_json, session.context(), "eof")?;
            return Ok(());
        };
        // The shared stdin router handles LF/CRLF; preserve its source exactly.
        let source = line.as_str();
        let command = source.trim();
        if command.is_empty() {
            continue;
        }
        if matches!(command, ":quit" | ":exit") {
            write_exit(&mut stdout, as_json, session.context(), "command")?;
            return Ok(());
        }
        let reply = match command {
            ":workspace" => workspace_reply(&session),
            ":clear" | ":restart" => reset_reply(&mut session, command),
            _ if command.starts_with(':') => cli_error(
                session.context(),
                "UnsupportedCommand",
                "Rust sessions support :workspace, :clear, :restart, :quit and :exit; file and directory commands are unavailable.",
            ),
            _ => match timeout.map(|duration| Instant::now().checked_add(duration)) {
                Some(None) => cli_error(session.context(), "InvalidDeadline", "The requested timeout exceeds the clock range."),
                deadline => evaluate(&mut session, source, deadline.flatten()),
            },
        };
        if as_json {
            writeln!(stdout, "{}", serde_json::to_string(&reply.record)?)?;
        } else if reply.error {
            writeln!(stderr, "{}", reply.text)?;
        } else {
            write!(stdout, "{}", reply.text)?;
        }
        stdout.flush()?;
    }
}

fn envelope(event: &str, outcome: &str) -> Value {
    json!({"schema_version": 1, "kind": "mathlab-rust-session", "backend": "rust",
        "profile": RUST_SESSION_PROFILE_V1, "event": event, "outcome": outcome})
}

fn context(value: RustSessionContextV1) -> Value {
    json!({"session_id": value.session_id, "epoch": value.epoch,
        "workspace_revision": value.workspace_revision})
}

fn key(value: RustRequestKeyV1) -> Value {
    json!({"session_id": value.session_id, "epoch": value.epoch, "request_id": value.request_id})
}

fn evaluate(session: &mut RustSessionV1, source: &str, deadline: Option<Instant>) -> Reply {
    let before = session.context();
    // The engine remains the sole owner of evaluation, cancellation and publication.
    let outcome = session
        .prepare(Arc::from(source), deadline)
        .map(|request| request.execute());
    let mut reply = match outcome {
        Ok(RustEvalOutcomeV1::Success(value)) => {
            let mut record = envelope("evaluation", "success");
            record["key"] = key(value.key);
            record["before"] = context(value.before);
            record["after"] = context(value.after);
            record["effect"] = match &value.effect {
                RustBindingEffectV1::None => json!({"kind": "none"}),
                RustBindingEffectV1::Write(name) => json!({"kind": "write", "name": name}),
            };
            record["display_name"] = json!(value.display_name);
            let words: Vec<_> = value
                .value
                .as_slice()
                .iter()
                .map(|number| format!("{:016x}", number.to_bits()))
                .collect();
            record["value"] = json!({"class": value.value.class(), "shape": value.value.shape(),
                "storage_order": "column-major", "binary64_words": words});
            Reply {
                record,
                text: display_value(value.display_name.as_deref(), &value.value),
                error: false,
            }
        }
        Ok(RustEvalOutcomeV1::Error {
            key: request,
            context: state,
            error,
        }) => {
            let mut reply = session_error(state, &error);
            reply.record["key"] = key(request);
            reply
        }
        Ok(RustEvalOutcomeV1::Cancelled {
            key: request,
            context: state,
            reason,
        }) => {
            let mut record = envelope("evaluation", "cancelled");
            record["key"] = key(request);
            record["context"] = context(state);
            record["reason"] = json!(format!("{reason:?}"));
            Reply {
                record,
                text: format!("Rust evaluation cancelled: {reason:?}. Workspace unchanged."),
                error: true,
            }
        }
        Err(error) => session_error(before, &error),
    };
    reply.record["source"] = json!(source);
    reply
}

fn display_value(name: Option<&str>, value: &SessionValueV1) -> String {
    let Some(name) = name else {
        return String::new();
    };
    let [rows, columns] = value.shape();
    let mut text = format!("{name} =\n");
    if rows == 0 || columns == 0 {
        text.push_str(&format!("[] ({rows}x{columns})\n"));
    } else {
        for row in 0..rows as usize {
            for column in 0..columns as usize {
                if column != 0 {
                    text.push(' ');
                }
                text.push_str(&value.as_slice()[column * rows as usize + row].to_string());
            }
            text.push('\n');
        }
    }
    text
}

fn session_error(state: RustSessionContextV1, error: &RustSessionErrorV1) -> Reply {
    let detail = match error {
        RustSessionErrorV1::Language(SessionExpressionErrorV1::Arithmetic(error)) => {
            crate::rust_eval::error_record(error)
        }
        RustSessionErrorV1::Language(error) => {
            let span = error.span();
            let mut record = json!({"code": error.code().unwrap_or("Allocation"), "stage": format!("{:?}", error.stage()),
                "span": {"start": span.start, "end": span.end, "start_line": span.start_line, "end_line": span.end_line},
                "message": error.to_string()});
            match error {
                SessionExpressionErrorV1::ConstructorDimension {
                    argument_index, reason, shape, value_bits, ..
                } => {
                    record["detail"] = json!({"argument_index": argument_index,
                        "reason": format!("{reason:?}"), "shape": shape,
                        "value_binary64_word": value_bits.map(|bits| format!("{bits:016x}"))});
                }
                SessionExpressionErrorV1::CallArity { actual, .. } => {
                    record["detail"] = json!({"actual_arguments": actual});
                }
                _ => {}
            }
            record
        }
        RustSessionErrorV1::Allocation { site, cause } => {
            json!({"code": "Allocation", "stage": "Session", "message": format!("{site}: {cause}")})
        }
        RustSessionErrorV1::ValueCopy(error) => {
            json!({"code": "ArrayOperation", "stage": "Session", "message": format!("{:?}", error.cause)})
        }
        RustSessionErrorV1::WorkspaceLimit => {
            json!({"code": "WorkspaceLimit", "stage": "Session", "message": "The bounded Rust workspace limit was exceeded."})
        }
        RustSessionErrorV1::WrongSession => {
            json!({"code": "WrongSession", "stage": "Session", "message": "The request belongs to another session."})
        }
        RustSessionErrorV1::StaleEpoch => {
            json!({"code": "StaleEpoch", "stage": "Session", "message": "The request belongs to an earlier session epoch."})
        }
        RustSessionErrorV1::CounterExhausted => {
            json!({"code": "CounterExhausted", "stage": "Session", "message": "A Rust session identity counter is exhausted."})
        }
    };
    let text = format!(
        "Rust {}: {}",
        detail["code"].as_str().unwrap_or("error"),
        detail["message"].as_str().unwrap_or("Evaluation failed.")
    );
    let mut record = envelope("evaluation", "error");
    record["context"] = context(state);
    record["error"] = detail;
    Reply {
        record,
        text,
        error: true,
    }
}

fn cli_error(state: RustSessionContextV1, code: &str, message: &str) -> Reply {
    let mut record = envelope("command", "error");
    record["context"] = context(state);
    record["error"] = json!({"code": code, "stage": "CLI", "message": message});
    Reply {
        record,
        text: format!("{code}: {message}"),
        error: true,
    }
}

fn workspace_reply(session: &RustSessionV1) -> Reply {
    let mut record = envelope("workspace", "success");
    record["context"] = context(session.context());
    let mut text = String::new();
    let variables: Vec<_> = session
        .workspace()
        .map(|variable| {
            text.push_str(&format!(
                "{}\t{}\t{:?}\n",
                variable.name, variable.class, variable.shape
            ));
            json!({"name": variable.name, "class": variable.class, "shape": variable.shape})
        })
        .collect();
    record["workspace"] = json!(variables);
    Reply {
        record,
        text,
        error: false,
    }
}

fn reset_reply(session: &mut RustSessionV1, command: &str) -> Reply {
    let before = session.context();
    match session.reset() {
        Ok(after) => {
            let mut record = envelope("reset", "success");
            record["command"] = json!(command);
            record["before"] = context(before);
            record["after"] = context(after);
            record["workspace"] = json!([]);
            Reply {
                record,
                text: "Rust workspace cleared; session epoch advanced.\n".into(),
                error: false,
            }
        }
        Err(error) => session_error(before, &error),
    }
}

fn write_exit(
    output: &mut impl Write,
    as_json: bool,
    state: RustSessionContextV1,
    reason: &str,
) -> Result<()> {
    if as_json {
        let mut record = envelope("exit", "success");
        record["reason"] = json!(reason);
        record["context"] = context(state);
        writeln!(output, "{}", serde_json::to_string(&record)?)?;
    }
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_deadline_reports_cancellation_without_publishing_and_recovers() {
        let mut session = RustSessionV1::new().unwrap();
        assert!(!evaluate(&mut session, "A=7", None).error);
        let before = session.context();
        let reply = evaluate(&mut session, "A=99", Some(Instant::now()));
        assert_eq!(reply.record["outcome"], "cancelled");
        assert_eq!(reply.record["reason"], "Deadline");
        assert_eq!(reply.record["context"], context(before));
        assert_eq!(session.context(), before);
        assert!(reply.record.get("value").is_none());
        let recovered = evaluate(&mut session, "A+1", None);
        assert_eq!(recovered.record["outcome"], "success");
        assert_eq!(
            recovered.record["value"]["binary64_words"],
            json!(["4020000000000000"])
        );
    }

    #[test]
    fn text_projection_honors_suppression_and_column_major_storage() {
        let mut session = RustSessionV1::new().unwrap();
        assert_eq!(evaluate(&mut session, "A=[1,2;3,4];", None).text, "");
        assert_eq!(evaluate(&mut session, "A", None).text, "A =\n1 2\n3 4\n");
        assert_eq!(evaluate(&mut session, "[]", None).text, "ans =\n[] (0x0)\n");
    }
}
