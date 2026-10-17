// SPDX-License-Identifier: GPL-3.0-or-later
//! Selected capture-backed cases through the explicit Rust CLI boundary.
#![cfg(unix)]

#[path = "../../mathlab-language/tests/fixtures/operator_native_reference_v1.rs"]
mod native;

use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const CHILD_LIMIT: Duration = Duration::from_secs(5);
const LOG_LIMIT: u64 = 64 * 1024;

struct ReapChild(Child);

impl Drop for ReapChild {
    fn drop(&mut self) {
        // Reap even when an assertion or I/O operation unwinds the harness.
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn bounded_read(path: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    File::open(path)
        .unwrap()
        .take(LOG_LIMIT + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() as u64 <= LOG_LIMIT, "CLI log exceeded limit");
    bytes
}

fn eval(source: &str) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let stdout = directory.path().join("stdout.json");
    let stderr = directory.path().join("stderr.log");
    let started = Instant::now();
    let mut child = ReapChild(
        Command::new(env!("CARGO_BIN_EXE_mathlab"))
            .args(["eval", "--backend", "rust", "--json", "--", source])
            .env_clear()
            .env("PATH", directory.path())
            .env("TMPDIR", directory.path())
            .current_dir(directory.path())
            .stdin(Stdio::null())
            .stdout(File::create(&stdout).unwrap())
            .stderr(File::create(&stderr).unwrap())
            .spawn()
            .unwrap(),
    );
    let status = loop {
        for path in [&stdout, &stderr] {
            assert!(
                fs::metadata(path).unwrap().len() <= LOG_LIMIT,
                "CLI log exceeded limit"
            );
        }
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            started.elapsed() < CHILD_LIMIT,
            "Rust CLI exceeded five seconds"
        );
        thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: bounded_read(&stdout),
        stderr: bounded_read(&stderr),
    }
}

fn record(output: &Output, success: bool) -> Value {
    assert_eq!(
        output.status.code(),
        Some(if success { 0 } else { 1 }),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let record: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(record["schema_version"], 1);
    assert_eq!(record["kind"], "mathlab-rust-expression");
    assert_eq!(record["backend"], "rust");
    assert_eq!(record["profile"], "arithmetic-v1");
    assert_eq!(record["outcome"], if success { "success" } else { "error" });
    record
}

#[test]
fn native_o01_signed_zero_survives_rust_cli_json() {
    let case = native::case("O01");
    let native::NativeExpectedV1::Exact {
        class,
        shape,
        words,
    } = &case.expected
    else {
        panic!("O01 must retain its exact native value");
    };
    let result = record(&eval(case.source), true);
    assert_eq!(result["value"]["class"], *class);
    assert_eq!(result["value"]["shape"], json!(shape));
    assert_eq!(result["value"]["storage_order"], "column-major");
    let expected: Vec<_> = words.iter().map(|word| format!("{word:016x}")).collect();
    assert_eq!(result["value"]["binary64_words"], json!(expected));
    assert!(result.get("error").is_none());
}

#[test]
fn native_o04_shape_failure_preserves_project_cli_diagnostic() {
    let case = native::case("O04");
    let native::NativeExpectedV1::ShapeError { native_line, .. } = &case.expected else {
        panic!("O04 must retain its native shape-error observation");
    };
    let result = record(&eval(case.source), false);
    assert_eq!(result["error"]["code"], "ArrayOperation");
    assert_eq!(result["error"]["stage"], "Evaluation");
    assert_eq!(
        result["error"]["cause"],
        json!({"kind":"IncompatibleShape", "context":"rdivide.compatibility",
            "axis":0, "lhs_extent":2, "rhs_extent":3})
    );
    // The native frame supplies the line only. The byte extent and end line
    // below assert the project's original UTF-8/CRLF source-span contract.
    assert_eq!(result["error"]["span"]["start_line"], *native_line);
    assert_eq!(
        result["error"]["span"],
        json!({"start":10, "end":12, "start_line":2, "end_line":2})
    );
    assert!(result.get("value").is_none());
}
