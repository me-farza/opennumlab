// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual subprocess checks: collecting an aggregate after completion cannot
//! satisfy the release-file handshake in the first test.

#![cfg(unix)]

use mathlab_engine::discover_octave;
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "mathlab-cli-console-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        Self(directory)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct RunningCli {
    child: Child,
    release: PathBuf,
}

impl Drop for RunningCli {
    fn drop(&mut self) {
        // Release the Octave calculation even when an assertion fails. Its own
        // execution deadline provides a second bound before forcefully killing
        // the CLI; normally this lets Engine::drop reap Octave itself.
        let _ = fs::write(&self.release, b"release");
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn runtime() -> Option<PathBuf> {
    let executable = discover_octave();
    if executable.is_none() {
        eprintln!("Skipping CLI console integration: GNU Octave was not found.");
    }
    executable
}

fn input_runtime() -> Option<PathBuf> {
    if std::env::var_os("MATHLAB_INPUT_BRIDGE").is_none() {
        eprintln!("Skipping CLI input integration: set MATHLAB_INPUT_BRIDGE.");
        return None;
    }
    runtime()
}

fn piped_input(
    executable: &Path,
    directory: &Path,
    args: &[&str],
    input: &[u8],
) -> std::process::Output {
    let mut child = command(executable, directory)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn command(executable: &Path, directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mathlab"));
    command
        .arg("--octave")
        .arg(executable)
        .arg("--directory")
        .arg(directory);
    command
}

fn literal(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "''"))
}

#[test]
fn cli_emits_non_newline_output_before_completion_without_repeating_aggregate() {
    let Some(executable) = runtime() else { return };
    let directory = TestDirectory::new();
    let release = directory.0.join("release");
    let source = format!(
        "fprintf(stdout,'CLI_LIVE'); while exist({},'file') != 2; pause(.01); endwhile; fprintf(stdout,'CLI_DONE');",
        literal(&release)
    );
    let child = command(&executable, &directory.0)
        .args(["--timeout", "8", "eval", &source])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut running = RunningCli { child, release };
    let mut stdout = running.child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut buffer = [0_u8; 256];
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if sender.send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => panic!("CLI stdout read failed: {error}"),
            }
        }
    });
    let mut output = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !output.windows(8).any(|bytes| bytes == b"CLI_LIVE") {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let bytes = receiver.recv_timeout(remaining).unwrap_or_else(|error| {
            panic!("live output was not received before release: {error}; bytes={output:?}")
        });
        output.extend(bytes);
    }
    assert!(running.child.try_wait().unwrap().is_none());
    assert!(!output.windows(8).any(|bytes| bytes == b"CLI_DONE"));
    fs::write(&running.release, b"release").unwrap();
    loop {
        match receiver.recv_timeout(Duration::from_secs(15)) {
            Ok(bytes) => output.extend(bytes),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(error) => panic!("CLI did not finish after release: {error}"),
        }
    }
    reader.join().unwrap();
    let status = running.child.wait().unwrap();
    let mut diagnostics = String::new();
    running
        .child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut diagnostics)
        .unwrap();
    assert!(status.success(), "{status}: {diagnostics}");
    let output = String::from_utf8(output).unwrap();
    assert_eq!(output.matches("CLI_LIVE").count(), 1, "{output}");
    assert_eq!(output.matches("CLI_DONE").count(), 1, "{output}");
}

#[test]
fn cli_keeps_received_partial_output_when_execution_times_out() {
    let Some(executable) = runtime() else { return };
    let directory = TestDirectory::new();
    let output = command(&executable, &directory.0)
        .args([
            "--timeout",
            "1",
            "eval",
            "fprintf(stdout,'CLI_PARTIAL_FAIL'); fflush(stdout); while true; end;",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.matches("CLI_PARTIAL_FAIL").count(), 1, "{stdout}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("timed out"));
}

#[test]
fn cli_json_is_clean_and_retains_aggregate_truncation_metadata() {
    let Some(executable) = runtime() else { return };
    let directory = TestDirectory::new();
    let output = command(&executable, &directory.0)
        .args([
            "--timeout",
            "8",
            "--json",
            "eval",
            "fprintf(stdout,'CLI_JSON');",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["output"], "CLI_JSON");
    assert_eq!(value["output_truncated"], false);
    assert_eq!(value["output_dropped_bytes"], 0);
    assert_eq!(value["output_decoding_loss"], false);
}

#[test]
fn cli_repl_json_has_no_banner_prompt_or_live_output_and_survives_restart() {
    let Some(executable) = runtime() else { return };
    let directory = TestDirectory::new();
    let mut child = command(&executable, &directory.0)
        .args(["--timeout", "8", "--json", "repl"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(
        b"fprintf(stdout,'REPL_ONE');\n:workspace\n:restart\nfprintf(stdout,'REPL_TWO');\n:quit\n"
    ).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let records = serde_json::Deserializer::from_slice(&output.stdout)
        .into_iter::<Value>()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records.len(), 4, "{records:?}");
    assert_eq!(records[0]["output"], "REPL_ONE");
    assert!(records[1]["workspace"].is_array());
    assert_eq!(records[2]["session"], "restarted");
    assert_eq!(records[3]["output"], "REPL_TWO");
}

#[test]
fn cli_repl_keeps_streaming_idle_background_output_and_after_restart() {
    let Some(executable) = runtime() else { return };
    let directory = TestDirectory::new();
    let child = command(&executable, &directory.0)
        .args(["--timeout", "8", "repl"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut running = RunningCli {
        child,
        release: directory.0.join("release"),
    };
    let mut stdin = running.child.stdin.take().unwrap();
    let mut stdout = running.child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut buffer = [0_u8; 256];
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    if sender.send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => panic!("CLI stdout read failed: {error}"),
            }
        }
    });
    stdin
        .write_all(b"system('(sleep .2; printf CLI_IDLE)&');\n")
        .unwrap();
    let mut output = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !output.windows(8).any(|bytes| bytes == b"CLI_IDLE") {
        output.extend(
            receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap(),
        );
    }
    // No second command was submitted: the printer remains active while the
    // REPL is waiting on stdin. The same subscription must survive restart.
    assert!(running.child.try_wait().unwrap().is_none());
    stdin
        .write_all(b":restart\nfprintf(stdout,'CLI_RESTART');\n:quit\n")
        .unwrap();
    drop(stdin);
    loop {
        match receiver.recv_timeout(Duration::from_secs(15)) {
            Ok(bytes) => output.extend(bytes),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(error) => panic!("CLI REPL did not finish: {error}"),
        }
    }
    reader.join().unwrap();
    let status = running.child.wait().unwrap();
    let mut diagnostics = String::new();
    running
        .child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut diagnostics)
        .unwrap();
    assert!(status.success(), "{status}: {diagnostics}");
    let output = String::from_utf8(output).unwrap();
    assert_eq!(output.matches("CLI_IDLE").count(), 1, "{output}");
    assert_eq!(output.matches("CLI_RESTART").count(), 1, "{output}");
}

#[test]
fn cli_input_expression_and_exact_string_use_native_echo_once() {
    let Some(executable) = input_runtime() else {
        return;
    };
    let directory = TestDirectory::new();
    let output=piped_input(&executable,&directory.0,&["--timeout","8","eval","seed=40; x=input('X? '); text=input('Text? ','s'); assert(x==42); assert(strcmp(text,'  λ exact  '));"],"seed+2\n  λ exact  \n".as_bytes());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.matches("X? seed+2").count(), 1, "{stdout}");
    assert_eq!(stdout.matches("Text?   λ exact  ").count(), 1, "{stdout}");
}

#[test]
fn cli_input_json_stays_clean_and_retains_native_prompt_and_answers_in_aggregate() {
    let Some(executable) = input_runtime() else {
        return;
    };
    let directory = TestDirectory::new();
    let output=piped_input(&executable,&directory.0,&["--timeout","8","--json","eval","n=input('Number> '); s=input('String> ','s'); assert(n==42); assert(strcmp(s,'text'));"],b"6*7\ntext\n");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let record: Value = serde_json::from_slice(&output.stdout).unwrap();
    let console = record["output"].as_str().unwrap();
    assert!(console.contains("Number> 6*7\nString> text\n"), "{console}");
    assert_eq!(record["output_truncated"], false);
}

#[test]
fn cli_repl_routes_commands_answers_and_restart_through_one_stdin_owner() {
    let Some(executable) = input_runtime() else {
        return;
    };
    let directory = TestDirectory::new();
    let output=piped_input(&executable,&directory.0,&["--timeout","8","repl"],b"a=input('A> '); assert(a==7);\n3+4\n:restart\ns=input('B> ','s'); assert(strcmp(s,' text '));\n text \nfprintf(stdout,'ROUTER_DONE');\n:quit\n");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.matches("A> 3+4").count(), 1, "{stdout}");
    assert_eq!(stdout.matches("B>  text ").count(), 1, "{stdout}");
    assert_eq!(stdout.matches("ROUTER_DONE").count(), 1, "{stdout}");
}

#[test]
fn cli_timeout_waiting_for_input_does_not_steal_the_next_restart_command() {
    timeout_input_then_restart(None);
}

#[test]
fn cli_timeout_discards_partial_input_answer_without_executing_it_as_repl_code() {
    timeout_input_then_restart(Some(b"fprintf(stdout,'STALE_EXECUTED');"));
}

fn timeout_input_then_restart(partial_answer: Option<&[u8]>) {
    let Some(executable) = input_runtime() else {
        return;
    };
    let directory = TestDirectory::new();
    let child = command(&executable, &directory.0)
        .args(["--timeout", "1", "repl"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut running = RunningCli {
        child,
        release: directory.0.join("release"),
    };
    let mut stdin = running.child.stdin.take().unwrap();
    let mut stdout = running.child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut bytes = [0_u8; 256];
        loop {
            match stdout.read(&mut bytes) {
                Ok(0) => break,
                Ok(count) => {
                    if sender.send(bytes[..count].to_vec()).is_err() {
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => panic!("stdout read failed: {error}"),
            }
        }
    });
    stdin.write_all(b"input('Wait> ','s');\n").unwrap();
    let mut output = Vec::new();
    let mut partial_sent = false;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let text = String::from_utf8_lossy(&output);
        if text.contains("Wait> ") && !partial_sent {
            if let Some(answer) = partial_answer {
                stdin.write_all(answer).unwrap();
            }
            partial_sent = true;
        }
        if text.contains("Wait> ") && text.matches(">> ").count() >= 2 {
            break;
        }
        output.extend(
            receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap(),
        );
    }
    if partial_answer.is_some() {
        // This newline closes only the expired answer. Commands after it must
        // remain available to the sole stdin owner.
        stdin.write_all(b"\n").unwrap();
    }
    stdin
        .write_all(b":restart\nfprintf(stdout,'AFTER_INPUT_TIMEOUT');\n:quit\n")
        .unwrap();
    drop(stdin);
    loop {
        match receiver.recv_timeout(Duration::from_secs(15)) {
            Ok(bytes) => output.extend(bytes),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(error) => panic!("CLI did not finish after input timeout: {error}"),
        }
    }
    reader.join().unwrap();
    assert!(running.child.wait().unwrap().success());
    let output = String::from_utf8(output).unwrap();
    assert_eq!(output.matches("AFTER_INPUT_TIMEOUT").count(), 1, "{output}");
    assert!(!output.contains("STALE_EXECUTED"), "{output}");
}
