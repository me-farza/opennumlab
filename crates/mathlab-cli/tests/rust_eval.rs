// SPDX-License-Identifier: GPL-3.0-or-later
//! Real CLI processes; a sentinel detects accidental Octave execution.
#![cfg(unix)]

use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output};
use tempfile::TempDir;

struct CliCase {
    directory: TempDir,
}

impl CliCase {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("octave-invoked");
        let escaped = marker.to_str().unwrap().replace('\'', "'\\''");
        let script = format!("#!/bin/sh\nprintf invoked > '{escaped}'\nexit 86\n");
        let sentinel = directory.path().join("octave-cli");
        fs::write(&sentinel, script).unwrap();
        fs::set_permissions(&sentinel, fs::Permissions::from_mode(0o700)).unwrap();
        Self { directory }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_mathlab"));
        command
            .env_clear()
            .current_dir(self.directory.path())
            .env("PATH", self.directory.path())
            .env("TMPDIR", self.directory.path())
            .env("MATHLAB_OCTAVE", self.directory.path().join("octave-cli"));
        command
    }

    fn eval(&self, source: &str) -> Output {
        self.command()
            .args(["eval", "--backend", "rust", "--json", "--", source])
            .output()
            .unwrap()
    }

    fn untouched(&self) {
        assert!(
            !self.directory.path().join("octave-invoked").exists(),
            "Rust invoked Octave"
        );
    }
}

fn record(output: &Output, success: bool) -> Value {
    assert_eq!(
        output.status.code(),
        Some(if success { 0 } else { 1 }),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["kind"], "mathlab-rust-expression");
    assert_eq!(value["backend"], "rust");
    assert_eq!(value["profile"], "arithmetic-v1");
    assert_eq!(value["outcome"], if success { "success" } else { "error" });
    assert!(value.get("workspace").is_none());
    assert!(value.get("console_generation").is_none());
    value
}

#[test]
fn rust_cli_executes_matrix_arithmetic_without_octave() {
    let cli = CliCase::new();
    // Independent small exact-integer expectations, in column-major order.
    for (source, shape, expected) in [
        (
            "[1,2;3,4]*[1,2;3,4]",
            [2, 2],
            vec![7.0_f64, 15.0, 10.0, 22.0],
        ),
        ("[1,2].*[10;20]", [2, 2], vec![10.0, 20.0, 20.0, 40.0]),
        ("[2,4]./2", [1, 2], vec![1.0, 2.0]),
        (
            "[1,2,3;4,5,6].'",
            [3, 2],
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        ),
        ("-(1+2)*3+10", [1, 1], vec![1.0]),
        ("[].'", [0, 0], vec![]),
    ] {
        let result = record(&cli.eval(source), true);
        assert_eq!(result["value"]["class"], "double");
        assert_eq!(result["value"]["shape"], json!(shape), "{source}");
        assert_eq!(result["value"]["storage_order"], "column-major");
        let expected: Vec<_> = expected
            .iter()
            .map(|v| format!("{:016x}", v.to_bits()))
            .collect();
        assert_eq!(
            result["value"]["binary64_words"],
            json!(expected),
            "{source}"
        );
    }
    cli.untouched();
}

#[test]
fn rust_cli_json_keeps_signed_zero_infinities_and_nan_bits() {
    let cli = CliCase::new();
    let result = record(&cli.eval("[-0,1e10000,-1e10000]"), true);
    assert_eq!(
        result["value"]["binary64_words"],
        json!(["8000000000000000", "7ff0000000000000", "fff0000000000000"])
    );
    let result = record(&cli.eval("0./0"), true);
    let word = result["value"]["binary64_words"][0].as_str().unwrap();
    assert_eq!(word.len(), 16);
    assert!(f64::from_bits(u64::from_str_radix(word, 16).unwrap()).is_nan());
    // NaN payload is returned losslessly, but no specific arithmetic NaN is promised.
    cli.untouched();
}

#[test]
fn rust_cli_errors_preserve_stage_span_and_cause_without_fallback() {
    let cli = CliCase::new();
    let result = record(&cli.eval("[1,2]*[1,2]"), false);
    assert_eq!(result["error"]["stage"], "Evaluation");
    assert_eq!(result["error"]["code"], "ArrayOperation");
    assert_eq!(
        result["error"]["span"],
        json!({"start":5,"end":6,"start_line":1,"end_line":1})
    );
    assert_eq!(
        result["error"]["cause"],
        json!({"kind":"IncompatibleInnerDimensions","lhs_columns":2,"rhs_rows":1})
    );
    let result = record(&cli.eval("[1,2]./[1,2,3]"), false);
    assert_eq!(
        result["error"]["cause"],
        json!({"kind":"IncompatibleShape","context":"rdivide.compatibility","axis":1,"lhs_extent":2,"rhs_extent":3})
    );
    let result = record(&cli.eval("[1,2;3]"), false);
    assert_eq!(result["error"]["stage"], "MatrixAdmission");
    assert_eq!(
        result["error"]["cause"],
        json!({"kind":"RaggedRows","row_lengths":[2,1]})
    );
    for source in ["A=1", "sin(1)", "1+", "1\n2", "1e+"] {
        let result = record(&cli.eval(source), false);
        assert!(result["error"]["code"].is_string());
        assert!(result["error"]["cause"]["kind"].is_string());
        assert!(result.get("value").is_none());
    }
    cli.untouched();
}

#[test]
fn rust_cli_resource_failure_is_explicit() {
    let cli = CliCase::new();
    let result = record(&cli.eval(&"1".repeat(4097)), false);
    assert_eq!(result["error"]["stage"], "Framing");
    assert_eq!(result["error"]["code"], "ResourceLimit");
    assert_eq!(
        result["error"]["cause"],
        json!({"kind":"Resource","name":"source_bytes","limit":4096,"attempted_count":4097})
    );
    cli.untouched();
}

#[test]
fn rust_cli_rejects_explicit_incompatible_flags_before_startup() {
    let cli = CliCase::new();
    for (flag, argument) in [
        ("--timeout", "1"),
        ("--octave", "/missing/octave"),
        ("--startup-config", "/missing/config.json"),
        ("--directory", "/missing/directory"),
    ] {
        for before in [false, true] {
            let mut args = Vec::new();
            if before {
                args.extend([flag, argument]);
            }
            args.extend(["eval", "--backend", "rust", "1", "--json"]);
            if !before {
                args.extend([flag, argument]);
            }
            let output = cli.command().args(args).output().unwrap();
            assert_eq!(output.status.code(), Some(2));
            assert!(output.stdout.is_empty());
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(
                error.contains(flag) && error.contains("--backend rust"),
                "{error}"
            );
        }
    }
    cli.untouched();
}

#[test]
fn octave_remains_default_and_explicit_octave_uses_existing_route() {
    for explicit in [false, true] {
        let cli = CliCase::new();
        let mut command = cli.command();
        command.arg("eval");
        if explicit {
            command.args(["--backend", "octave"]);
        }
        let output = command.arg("1").output().unwrap();
        assert!(!output.status.success()); // The deliberate sentinel exits86.
        assert!(cli.directory.path().join("octave-invoked").exists());
    }
}

#[test]
fn rust_cli_text_displays_rows_in_matrix_order() {
    let cli = CliCase::new();
    let output = cli
        .command()
        .args(["eval", "--backend", "rust", "[1,2;3,4]"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "ans =\n1 2\n3 4\n"
    );
    cli.untouched();
}
