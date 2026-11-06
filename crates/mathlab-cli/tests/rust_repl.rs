// SPDX-License-Identifier: GPL-3.0-or-later
//! Persistent CLI journeys; these do not repeat the native operator fixtures.
#![cfg(unix)]

use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Output};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const LOG_LIMIT: u64 = 64 * 1024;

struct ReapChild(Child);
impl Drop for ReapChild {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

struct CliCase {
    directory: TempDir,
}
impl CliCase {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("octave-invoked");
        let escaped = marker.to_str().unwrap().replace('\'', "'\\''");
        let sentinel = directory.path().join("octave-cli");
        fs::write(
            &sentinel,
            format!("#!/bin/sh\nprintf invoked > '{escaped}'\nexit 86\n"),
        )
        .unwrap();
        fs::set_permissions(&sentinel, fs::Permissions::from_mode(0o700)).unwrap();
        Self { directory }
    }

    fn run(&self, input: &str, args: &[&str]) -> Output {
        let stdin = self.directory.path().join("stdin.txt");
        let stdout = self.directory.path().join("stdout.txt");
        let stderr = self.directory.path().join("stderr.txt");
        fs::write(&stdin, input).unwrap();
        let started = Instant::now();
        let mut child = ReapChild(
            Command::new(env!("CARGO_BIN_EXE_mathlab"))
                .args(args)
                .env_clear()
                .env("PATH", self.directory.path())
                .env("TMPDIR", self.directory.path())
                .env("MATHLAB_OCTAVE", self.directory.path().join("octave-cli"))
                .current_dir(self.directory.path())
                .stdin(File::open(stdin).unwrap())
                .stdout(File::create(&stdout).unwrap())
                .stderr(File::create(&stderr).unwrap())
                .spawn()
                .unwrap(),
        );
        let status = loop {
            for path in [&stdout, &stderr] {
                assert!(
                    fs::metadata(path).unwrap().len() <= LOG_LIMIT,
                    "CLI log limit"
                );
            }
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(started.elapsed() < Duration::from_secs(5), "CLI time limit");
            thread::sleep(Duration::from_millis(10));
        };
        let output = Output {
            status,
            stdout: bounded_read(&stdout),
            stderr: bounded_read(&stderr),
        };
        assert!(
            !self.directory.path().join("octave-invoked").exists(),
            "Rust route invoked Octave"
        );
        output
    }

    fn json(&self, input: &str) -> Vec<Value> {
        records(self.run(input, &["repl", "--backend", "rust", "--json"]))
    }
}

fn bounded_read(path: &Path) -> Vec<u8> {
    let mut data = Vec::new();
    File::open(path)
        .unwrap()
        .take(LOG_LIMIT + 1)
        .read_to_end(&mut data)
        .unwrap();
    assert!(data.len() as u64 <= LOG_LIMIT);
    data
}

fn records(output: Output) -> Vec<Value> {
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            let record: Value = serde_json::from_str(line).unwrap();
            assert_eq!(record["schema_version"], 1);
            assert_eq!(record["kind"], "mathlab-rust-session");
            assert_eq!(record["backend"], "rust");
            assert_eq!(record["profile"], "rust-session-arithmetic-v1");
            record
        })
        .collect()
}

fn scalar(record: &Value, word: &str) {
    assert_eq!(record["outcome"], "success");
    assert_eq!(record["value"]["class"], "double");
    assert_eq!(record["value"]["shape"], json!([1, 1]));
    assert_eq!(record["value"]["storage_order"], "column-major");
    assert_eq!(record["value"]["binary64_words"], json!([word]));
}

#[test]
fn variables_ans_and_request_identity_persist_across_commands() {
    let r = CliCase::new().json("A=3;\nA+2\nB=A+4\nB\nans\n:workspace\n:quit\n");
    assert_eq!(r.len(), 7);
    assert!(r[0]["display_name"].is_null());
    scalar(&r[1], "4014000000000000"); // 5
    scalar(&r[2], "401c000000000000"); // 7
    scalar(&r[3], "401c000000000000");
    scalar(&r[4], "4014000000000000");
    assert_eq!(r[3]["effect"]["kind"], "none");
    let session = &r[0]["key"]["session_id"];
    for (index, record) in r[..5].iter().enumerate() {
        assert_eq!(&record["key"]["session_id"], session);
        assert_eq!(record["key"]["epoch"], 1);
        assert_eq!(record["key"]["request_id"], (index + 1) as u64);
    }
    assert_eq!(r[4]["after"]["workspace_revision"], 3);
    let mut names: Vec<_> = r[5]["workspace"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    names.sort();
    assert_eq!(names, ["A", "B", "ans"]);
    assert_eq!(r[6]["event"], "exit");
}

#[test]
fn failed_assignments_publish_neither_replacements_nor_new_bindings() {
    let r = CliCase::new()
        .json("A=7\n1+1\nA=missing\nB=missing\nA\nans\n:workspace\nA=A+1\nA\n:quit\n");
    assert_eq!(r.len(), 10);
    for record in &r[2..4] {
        assert_eq!(record["outcome"], "error");
        assert_eq!(record["error"]["code"], "UnknownBinding");
        assert_eq!(record["context"], r[1]["after"]);
        assert!(record.get("value").is_none());
    }
    scalar(&r[4], "401c000000000000");
    scalar(&r[5], "4000000000000000");
    assert_eq!(r[6]["workspace"].as_array().unwrap().len(), 2);
    assert!(r[6]["workspace"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["name"] != "B"));
    scalar(&r[8], "4020000000000000");
    assert_eq!(r[8]["after"]["workspace_revision"], 3);
}

#[test]
fn unsupported_source_and_commands_report_errors_without_fallback() {
    let r = CliCase::new()
        .json("A=7\nA=3;B=4\nsin(1)\nA(1)=\n:run missing.m\n:cd /missing\n:unknown\nA+1\n:quit\n");
    assert_eq!(r.len(), 9);
    for record in &r[1..7] {
        assert_eq!(record["outcome"], "error");
        assert!(record["error"]["code"].is_string());
        assert_eq!(record["context"], r[0]["after"]);
        assert!(record.get("value").is_none());
    }
    for record in &r[4..7] {
        assert_eq!(record["error"]["code"], "UnsupportedCommand");
    }
    scalar(&r[7], "4020000000000000");
}

#[test]
fn clear_and_restart_reset_workspace_with_new_epochs() {
    let r = CliCase::new()
        .json("A=5\n:clear\n:workspace\nA\nA=6\n:restart\n:workspace\nA\n1+1\n:quit\n");
    assert_eq!(r.len(), 10);
    for (index, epoch) in [(1, 2), (5, 3)] {
        assert_eq!(r[index]["event"], "reset");
        assert_eq!(r[index]["after"]["session_id"], r[0]["after"]["session_id"]);
        assert_eq!(r[index]["after"]["epoch"], epoch);
        assert_eq!(r[index]["after"]["workspace_revision"], 0);
        assert_eq!(r[index + 1]["workspace"], json!([]));
        assert_eq!(r[index + 2]["error"]["code"], "UnknownBinding");
    }
    scalar(&r[8], "4000000000000000");
    assert_eq!(r[8]["after"]["epoch"], 3);
}

#[test]
fn quit_exit_and_eof_end_cleanly_and_ignore_unread_commands() {
    for command in [":quit", ":exit"] {
        let r = CliCase::new().json(&format!("A=1\n{command}\nunsupported(\n"));
        assert_eq!(r.len(), 2);
        assert_eq!(r[1]["event"], "exit");
        assert_eq!(r[1]["reason"], "command");
    }
    let r = CliCase::new().json("\n \r\nA=5\r\nA");
    assert_eq!(r.len(), 3);
    scalar(&r[1], "4014000000000000");
    assert_eq!(r[2]["reason"], "eof");
    let empty = CliCase::new().json("");
    assert_eq!(empty.len(), 1);
    assert_eq!(empty[0]["event"], "exit");
}

#[test]
fn human_output_suppresses_semicolons_and_recovers_after_errors() {
    let output = CliCase::new().run(
        "A=7;\nA\nmissing\nA+1\n:quit\n",
        &["repl", "--backend", "rust"],
    );
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("experimental persistent Rust session"));
    assert_eq!(stdout.matches("A =\n7\n").count(), 1);
    assert!(stdout.contains("ans =\n8\n"));
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("UnknownBinding"));
}

#[test]
fn source_limits_and_original_whitespace_diagnostics_recover() {
    let input = format!("A=7\n{}\n   missing\nA\n:quit\n", "1".repeat(4097));
    let r = CliCase::new().json(&input);
    assert_eq!(r.len(), 5);
    assert_eq!(r[1]["error"]["code"], "ResourceLimit");
    assert_eq!(r[1]["context"], r[0]["after"]);
    assert_eq!(r[2]["source"], "   missing");
    assert_eq!(r[2]["error"]["span"]["start"], 3);
    assert_eq!(r[2]["error"]["span"]["end"], 10);
    scalar(&r[3], "401c000000000000");
}

#[test]
fn incompatible_flags_fail_before_startup_and_supported_timeout_runs() {
    for (flag, value) in [
        ("--octave", "/missing/octave"),
        ("--startup-config", "/missing/config.json"),
        ("--directory", "/missing"),
    ] {
        for before in [false, true] {
            let mut args = Vec::new();
            if before {
                args.extend([flag, value]);
            }
            args.extend(["repl", "--backend", "rust", "--json"]);
            if !before {
                args.extend([flag, value]);
            }
            let output = CliCase::new().run(":quit\n", &args);
            assert_eq!(output.status.code(), Some(2));
            assert!(output.stdout.is_empty());
            let error = String::from_utf8(output.stderr).unwrap();
            assert!(error.contains(flag) && error.contains("--backend rust"));
        }
    }
    let r = records(CliCase::new().run(
        "A=2\nA+1\n:quit\n",
        &["repl", "--backend", "rust", "--json", "--timeout", "1"],
    ));
    scalar(&r[1], "4008000000000000");
}


#[test]
fn zeros_persistent_arrays_empty_axes_and_ans_without_native_fallback() {
    let r = CliCase::new().json("n=2;\nA=zeros(n,3);\nA\nzeros(2,0)*zeros(0,3)\nE=zeros(-1,2);\nE\nans\nzeros(-0,2)\nzeros(2,3).'\nans\n:workspace\n:quit\n");
    assert_eq!(r.len(), 12);
    assert!(r[1]["display_name"].is_null());
    assert!(r[4]["display_name"].is_null());
    for (index, shape, count) in [(1,[2,3],6), (2,[2,3],6), (3,[2,3],6),
                                  (4,[0,2],0), (5,[0,2],0), (6,[2,3],6),
                                  (7,[0,2],0), (8,[3,2],6), (9,[3,2],6)] {
        assert_eq!(r[index]["outcome"], "success");
        assert_eq!(r[index]["value"]["class"], "double");
        assert_eq!(r[index]["value"]["shape"], json!(shape));
        assert_eq!(r[index]["value"]["storage_order"], "column-major");
        assert_eq!(r[index]["value"]["binary64_words"], json!(vec!["0000000000000000";count]));
    }
    assert_eq!(r[2]["effect"]["kind"], "none");
    assert_eq!(r[2]["after"]["workspace_revision"], 2);
    assert_eq!(r[5]["effect"]["kind"], "none");
    assert_eq!(r[6]["after"]["workspace_revision"], 4); // assignment/read kept ans unchanged
    assert_eq!(r[9]["after"]["workspace_revision"], 6);
    let workspace = r[10]["workspace"].as_array().unwrap();
    assert_eq!(workspace.len(), 4);
    for (name, shape) in [("n",[1,1]), ("A",[2,3]), ("E",[0,2]), ("ans",[3,2])] {
        let binding = workspace.iter().find(|v| v["name"]==name).unwrap();
        assert_eq!(binding["shape"], json!(shape));
    }
    assert_eq!(r[11]["event"], "exit");
}

#[test]
fn zeros_diagnostics_preserve_values_and_recover_after_shadowing_reset() {
    let dimension_source = "  A=zeros(1.5); % π";
    let r = CliCase::new().json(&format!("A=7;\n9\n{dimension_source}\nB=zeros([1,2],missing);\nzeros(missing,1,2)\nfoo(missing)\nzeros=9;\nA=zeros(missing);\nzeros\nA\nans\n:workspace\n:clear\nzeros(1)\n:quit\n"));
    assert_eq!(r.len(), 15);
    for (index, code) in [(2,"ConstructorDimension"), (3,"UnknownBinding"),
                          (4,"CallArity"), (5,"UnsupportedCall"), (7,"UnknownBinding")] {
        assert_eq!(r[index]["outcome"], "error");
        assert_eq!(r[index]["error"]["code"], code);
        assert!(r[index].get("value").is_none());
    }
    for record in &r[2..6] {assert_eq!(record["context"], r[1]["after"]);}
    assert_eq!(r[7]["context"], r[6]["after"]);
    let start = dimension_source.find("1.5").unwrap();
    assert_eq!(r[2]["error"]["span"], json!({"start":start,"end":start+3,"start_line":1,"end_line":1}));
    assert_eq!(r[2]["error"]["detail"], json!({"argument_index":0,"reason":"Fractional",
        "shape":[1,1],"value_binary64_word":"3ff8000000000000"}));
    assert_eq!(r[4]["error"]["detail"]["actual_arguments"], 3);
    assert_eq!(r[4]["error"]["span"]["end"], "zeros(missing,1,2)".len());
    assert_eq!(r[5]["error"]["span"]["end"], 3);
    scalar(&r[8], "4022000000000000"); // bare binding zeros remains readable
    scalar(&r[9], "401c000000000000"); // A stayed 7
    scalar(&r[10], "4022000000000000"); // ans stayed 9
    assert_eq!(r[11]["workspace"].as_array().unwrap().len(), 3);
    assert!(!r[11]["workspace"].as_array().unwrap().iter().any(|v| v["name"]=="B"));
    assert_eq!(r[12]["event"], "reset");
    scalar(&r[13], "0000000000000000");
    assert_eq!(r[14]["event"], "exit");
}


#[test]
fn ones_persistent_values_ans_shadowing_and_error_recovery() {
    let r = CliCase::new().json("A=ones(2,3);\nA\nA*ones(3,2)\nB=ones(-2,3);\nB\nans\nA=ones(1.5);\nones=9;\nA=ones(missing);\nzeros(1)\nA\nones\nans\n:workspace\n:quit\n");
    assert_eq!(r.len(), 15);
    assert!(r[0]["display_name"].is_null());
    assert!(r[3]["display_name"].is_null());
    for (index, shape, word, count) in [
        (0,[2,3],"3ff0000000000000",6), (1,[2,3],"3ff0000000000000",6),
        (2,[2,2],"4008000000000000",4), (3,[0,3],"3ff0000000000000",0),
        (4,[0,3],"3ff0000000000000",0), (5,[2,2],"4008000000000000",4),
        (10,[2,3],"3ff0000000000000",6),
    ] {
        assert_eq!(r[index]["outcome"], "success");
        assert_eq!(r[index]["value"]["class"], "double");
        assert_eq!(r[index]["value"]["shape"], json!(shape));
        assert_eq!(r[index]["value"]["storage_order"], "column-major");
        assert_eq!(r[index]["value"]["binary64_words"], json!(vec![word;count]));
    }
    assert_eq!(r[5]["after"]["workspace_revision"], 3);
    assert_eq!(r[6]["outcome"], "error");
    assert_eq!(r[6]["error"]["code"], "ConstructorDimension");
    assert_eq!(r[6]["error"]["detail"]["reason"], "Fractional");
    assert_eq!(r[6]["error"]["detail"]["value_binary64_word"], "3ff8000000000000");
    assert_eq!(r[6]["context"], r[5]["after"]);
    assert_eq!(r[8]["outcome"], "error");
    assert_eq!(r[8]["error"]["code"], "UnknownBinding");
    assert_eq!(r[8]["error"]["span"]["start"], 7);
    assert_eq!(r[8]["error"]["span"]["end"], 14);
    assert_eq!(r[8]["context"], r[7]["after"]);
    scalar(&r[9], "0000000000000000"); // ones binding leaves zeros callable
    scalar(&r[11], "4022000000000000"); // ordinary ones binding remains readable
    scalar(&r[12], "0000000000000000"); // reads and errors preserve ans
    assert_eq!(r[12]["after"]["workspace_revision"], 5);
    let workspace = r[13]["workspace"].as_array().unwrap();
    assert_eq!(workspace.len(), 4);
    for (name, shape) in [("A",[2,3]), ("B",[0,3]), ("ones",[1,1]), ("ans",[1,1])] {
        assert_eq!(workspace.iter().find(|v| v["name"]==name).unwrap()["shape"], json!(shape));
    }
    assert_eq!(r[14]["event"], "exit");
}


#[test]
fn eye_persistent_identity_rectangular_empty_ans_and_recovery() {
    let r = CliCase::new().json("I=eye(2);\nI\nR=eye(2,3);\nR*[1;2;3]\nE=eye(3,0);\nE\nans\nI=eye(1.5);\neye=9;\nR=eye(missing);\nones(1)\nzeros(1)\nI\neye\nans\n:workspace\n:quit\n");
    assert_eq!(r.len(),17);
    for index in [0,2,4] {assert!(r[index]["display_name"].is_null());}
    let one = "3ff0000000000000";
    let zero = "0000000000000000";
    for (index,shape,words) in [
        (0,[2,2],vec![one,zero,zero,one]), (1,[2,2],vec![one,zero,zero,one]),
        (2,[2,3],vec![one,zero,zero,one,zero,zero]),
        (3,[2,1],vec![one,"4000000000000000"]),
        (4,[3,0],vec![]), (5,[3,0],vec![]), (6,[2,1],vec![one,"4000000000000000"]),
        (12,[2,2],vec![one,zero,zero,one]),
    ] {
        assert_eq!(r[index]["outcome"],"success");
        assert_eq!(r[index]["value"]["class"],"double");
        assert_eq!(r[index]["value"]["shape"],json!(shape));
        assert_eq!(r[index]["value"]["storage_order"],"column-major");
        assert_eq!(r[index]["value"]["binary64_words"],json!(words));
    }
    assert_eq!(r[6]["after"]["workspace_revision"],4);
    assert_eq!(r[7]["outcome"],"error");
    assert_eq!(r[7]["error"]["code"],"ConstructorDimension");
    assert_eq!(r[7]["error"]["detail"]["reason"],"Fractional");
    assert_eq!(r[7]["context"],r[6]["after"]);
    assert_eq!(r[9]["outcome"],"error");
    assert_eq!(r[9]["error"]["code"],"UnknownBinding");
    assert_eq!(r[9]["error"]["span"]["start"],6);
    assert_eq!(r[9]["error"]["span"]["end"],13);
    assert_eq!(r[9]["context"],r[8]["after"]);
    scalar(&r[10],one);
    scalar(&r[11],zero);
    scalar(&r[13],"4022000000000000");
    scalar(&r[14],zero);
    assert_eq!(r[14]["after"]["workspace_revision"],7);
    let workspace = r[15]["workspace"].as_array().unwrap();
    assert_eq!(workspace.len(),5);
    for (name,shape) in [("I",[2,2]),("R",[2,3]),("E",[3,0]),("eye",[1,1]),("ans",[1,1])] {
        assert_eq!(workspace.iter().find(|v| v["name"]==name).unwrap()["shape"],json!(shape));
    }
    assert_eq!(r[16]["event"],"exit");
}


#[test]
fn shape_queries_persistent_rows_counts_nested_results_shadowing_and_recovery() {
    let r = CliCase::new().json("A=eye(2,3);\nS=size(A);\nS\nnumel(A)\nB=zeros(numel(size(A)));\nsize(zeros(0,3))\nnumel(0./0)\nA=size(missing,2);\nsize=9;\nA=size(missing);\nnumel(A)\nnumel=8;\nA=numel(missing);\nzeros(1)\nsize\nnumel\nans\nA\n:workspace\n:quit\n");
    assert_eq!(r.len(),20);
    for index in [0,1,4] {assert!(r[index]["display_name"].is_null());}
    let one = "3ff0000000000000";
    let zero = "0000000000000000";
    for (index,shape,words) in [
        (1,[1,2],vec!["4000000000000000","4008000000000000"]),
        (2,[1,2],vec!["4000000000000000","4008000000000000"]),
        (4,[2,2],vec![zero;4]), (5,[1,2],vec![zero,"4008000000000000"]),
        (17,[2,3],vec![one,zero,zero,one,zero,zero]),
    ] {
        assert_eq!(r[index]["outcome"],"success");
        assert_eq!(r[index]["value"]["class"],"double");
        assert_eq!(r[index]["value"]["shape"],json!(shape));
        assert_eq!(r[index]["value"]["storage_order"],"column-major");
        assert_eq!(r[index]["value"]["binary64_words"],json!(words));
    }
    scalar(&r[3],"4018000000000000");
    scalar(&r[6],one); // NaN payload is still one element
    assert_eq!(r[7]["outcome"],"error");
    assert_eq!(r[7]["error"]["code"],"CallArity");
    assert_eq!(r[7]["error"]["detail"]["actual_arguments"],2);
    assert_eq!(r[7]["context"],r[6]["after"]);
    for (index,start,end) in [(9,7,14),(12,8,15)] {
        assert_eq!(r[index]["outcome"],"error");
        assert_eq!(r[index]["error"]["code"],"UnknownBinding");
        assert_eq!(r[index]["error"]["span"]["start"],start);
        assert_eq!(r[index]["error"]["span"]["end"],end);
        assert_eq!(r[index]["context"],r[index-1]["after"]);
    }
    scalar(&r[10],"4018000000000000"); // size binding does not shadow numel
    scalar(&r[13],zero); // query bindings do not shadow constructors
    scalar(&r[14],"4022000000000000");
    scalar(&r[15],"4020000000000000");
    scalar(&r[16],zero);
    assert_eq!(r[16]["after"]["workspace_revision"],10);
    let workspace = r[18]["workspace"].as_array().unwrap();
    assert_eq!(workspace.len(),6);
    for (name,shape) in [("A",[2,3]),("S",[1,2]),("B",[2,2]),("size",[1,1]),("numel",[1,1]),("ans",[1,1])] {
        assert_eq!(workspace.iter().find(|v| v["name"]==name).unwrap()["shape"],json!(shape));
    }
    assert_eq!(r[19]["event"],"exit");
}


#[test]
fn integer_ranges_persistent_shapes_signed_zero_errors_and_recovery() {
    let r = CliCase::new().json("x=7;\n9\nx=1:17;\nx\nans\nr=-0:1:1;\nr\n(5:1).'\nq=5:-2:0;\nq\nx=1:0.5:3;\nx\n1+1:2:2*4\nnumel(ans)\nx=1:16;\nsize(x)\n:workspace\n:quit\n");
    assert_eq!(r.len(),18);
    assert_eq!(r[2]["outcome"],"error");
    assert_eq!(r[2]["error"]["code"],"RangeLength");
    assert_eq!(r[2]["error"]["span"]["start"],2);
    assert_eq!(r[2]["error"]["span"]["end"],6);
    assert_eq!(r[2]["context"],r[1]["after"]);
    scalar(&r[3],"401c000000000000");
    scalar(&r[4],"4022000000000000");
    for index in [5,6] {
        assert_eq!(r[index]["value"]["shape"],json!([1,2]));
        assert_eq!(r[index]["value"]["binary64_words"],json!(["8000000000000000","3ff0000000000000"]));
    }
    assert_eq!(r[7]["value"]["shape"],json!([0,1]));
    assert_eq!(r[7]["value"]["binary64_words"],json!([]));
    for index in [8,9] {
        assert_eq!(r[index]["value"]["shape"],json!([1,3]));
        assert_eq!(r[index]["value"]["binary64_words"],json!(["4014000000000000","4008000000000000","3ff0000000000000"]));
    }
    assert_eq!(r[10]["outcome"],"error");
    assert_eq!(r[10]["error"]["code"],"RangeOperand");
    assert!(r[10]["error"]["message"].as_str().unwrap().contains("Fractional"));
    assert_eq!(r[10]["error"]["span"]["start"],4);
    assert_eq!(r[10]["error"]["span"]["end"],7);
    assert_eq!(r[10]["context"],r[9]["after"]);
    scalar(&r[11],"401c000000000000");
    assert_eq!(r[12]["value"]["shape"],json!([1,4]));
    assert_eq!(r[12]["value"]["binary64_words"],json!(["4000000000000000","4010000000000000","4018000000000000","4020000000000000"]));
    scalar(&r[13],"4010000000000000");
    assert_eq!(r[14]["value"]["shape"],json!([1,16]));
    assert_eq!(r[15]["value"]["binary64_words"],json!(["3ff0000000000000","4030000000000000"]));
    assert_eq!(r[15]["after"]["workspace_revision"],9);
    let workspace = r[16]["workspace"].as_array().unwrap();
    assert_eq!(workspace.len(),4);
    for (name,shape) in [("x",[1,16]),("r",[1,2]),("q",[1,3]),("ans",[1,2])] {
        assert_eq!(workspace.iter().find(|v| v["name"]==name).unwrap()["shape"],json!(shape));
    }
    assert_eq!(r[17]["event"],"exit");
}


#[test]
fn reshape_persistent_column_major_bits_errors_shadowing_and_recovery() {
    let r=CliCase::new().json("A=[1,2,3;4,5,6];\nB=reshape(A,3,2);\nB\nreshape([-0,0],2,1)\nreshape([],3,0)\nnumel(reshape(A,1,6))\nB=reshape(A,1,1);\nB=reshape([], -1, 0);\nB\nreshape=9;\nB=reshape(missing,1,1);\nreshape\nsize(A)\n:restart\nreshape(1:4,2,2)\n:quit\n");
    assert_eq!(r.len(),16);
    let words=vec!["3ff0000000000000","4010000000000000","4000000000000000","4014000000000000","4008000000000000","4018000000000000"];
    for index in [1,2,8] {
        assert_eq!(r[index]["outcome"],"success");
        assert_eq!(r[index]["value"]["shape"],json!([3,2]));
        assert_eq!(r[index]["value"]["storage_order"],"column-major");
        assert_eq!(r[index]["value"]["binary64_words"],json!(words));
    }
    assert!(r[1]["display_name"].is_null());
    assert_eq!(r[3]["value"]["binary64_words"],json!(["8000000000000000","0000000000000000"]));
    assert_eq!(r[4]["value"]["shape"],json!([3,0]));
    assert_eq!(r[4]["value"]["binary64_words"],json!([]));
    scalar(&r[5],"4018000000000000");
    for (index,code) in [(6,"ArrayOperation"),(7,"ConstructorDimension"),(10,"IndexArity")] {
        assert_eq!(r[index]["outcome"],"error");
        assert_eq!(r[index]["error"]["code"],code);
        let context=if index==7{&r[6]["context"]}else{&r[index-1]["after"]};
        assert_eq!(&r[index]["context"],context);
    }
    assert_eq!(r[7]["error"]["detail"]["argument_index"],1);
    assert_eq!(r[7]["error"]["span"]["start"],14);
    assert_eq!(r[7]["error"]["span"]["end"],16);
    scalar(&r[11],"4022000000000000");
    assert_eq!(r[12]["value"]["binary64_words"],json!(["4000000000000000","4008000000000000"]));
    assert_eq!(r[14]["value"]["shape"],json!([2,2]));
    assert_eq!(r[14]["value"]["binary64_words"],json!(["3ff0000000000000","4000000000000000","4008000000000000","4010000000000000"]));
    assert_eq!(r[15]["event"],"exit");
}

#[test]
fn scalar_index_persistent_binding_first_bits_ans_bounds_and_recovery() {
    let r=CliCase::new().json("A=[1,2,3;4,5,6];\nA(1+1)\nA([1],2);\nans\nA(7,1./0)\nA(0,missing)\nA(1,4)\nA([1,2;1,2])\nA()\nA(1,1,1)\nZ=[-0,1];\nZ(1)\nans\nsize=A;\nsize(1,2)\nreshape=A;\nreshape(2)\nreshape(1,1,1)\nA(2,3)\n:quit\n");
    assert_eq!(r.len(),20);
    scalar(&r[1],"4010000000000000");
    scalar(&r[2],"4000000000000000");
    assert!(r[2]["display_name"].is_null());
    scalar(&r[3],"4000000000000000");
    assert_eq!(r[1]["after"]["workspace_revision"],2);
    assert_eq!(r[2]["after"]["workspace_revision"],3);
    for (index,code) in [(4,"IndexBounds"),(5,"UnknownBinding"),(6,"IndexBounds"),(7,"IndexOperand"),(8,"IndexArity"),(9,"IndexArity")] {
        assert_eq!(r[index]["outcome"],"error");
        assert_eq!(r[index]["error"]["code"],code);
        assert_eq!(r[index]["context"],r[3]["after"]);
    }
    for index in [11,12] {scalar(&r[index],"8000000000000000");}
    scalar(&r[14],"4000000000000000");
    scalar(&r[16],"4010000000000000");
    assert_eq!(r[17]["error"]["code"],"IndexArity");
    assert_eq!(r[17]["context"],r[16]["after"]);
    scalar(&r[18],"4018000000000000");
    assert_eq!(r[19]["event"],"exit");
}

#[test]
fn vector_index_persistent_geometry_order_bits_shadowing_errors_and_recovery() {
    let r=CliCase::new().json("A=[1,2,3;4,5,6];\nA([2;1;2])\nA([2,1],[3;1;2]);\nans\nVR=[10,20,30,40];\nVR([4;1;2])\nVC=VR.';\nVC([4,1,2])\nS=9;\nS([1;1])\nA([7,1e309])\nA([1;7;0])\nA([3,1],[1e309,1])\nA([2,1],[4,1e309])\nA([],4)\nA([1,2;1,2])\nA([0,1],missing)\nA(7,1e309)\nZ=[-0,1];\nZ([1,1,2])\nsize=A;\nsize([2,1],[3,1])\nreshape=A;\nreshape([2,1])\nA=A([2,1]);\nA\nans\nA(2)\n:quit\n");
    assert_eq!(r.len(),29);
    for (index,shape,words) in [
        (1,[3,1],vec!["4010000000000000","3ff0000000000000","4010000000000000"]),
        (2,[2,3],vec!["4018000000000000","4008000000000000","4010000000000000","3ff0000000000000","4014000000000000","4000000000000000"]),
        (3,[2,3],vec!["4018000000000000","4008000000000000","4010000000000000","3ff0000000000000","4014000000000000","4000000000000000"]),
        (5,[1,3],vec!["4044000000000000","4024000000000000","4034000000000000"]),
        (7,[3,1],vec!["4044000000000000","4024000000000000","4034000000000000"]),
        (9,[2,1],vec!["4022000000000000","4022000000000000"]),
        (19,[1,3],vec!["8000000000000000","8000000000000000","3ff0000000000000"]),
        (21,[2,2],vec!["4018000000000000","4008000000000000","4010000000000000","3ff0000000000000"]),
        (23,[1,2],vec!["4010000000000000","3ff0000000000000"]),
        (24,[1,2],vec!["4010000000000000","3ff0000000000000"]),
        (25,[1,2],vec!["4010000000000000","3ff0000000000000"]),
        (26,[1,2],vec!["4010000000000000","3ff0000000000000"]),
    ] {
        assert_eq!(r[index]["outcome"],"success");assert_eq!(r[index]["value"]["shape"],json!(shape));
        assert_eq!(r[index]["value"]["binary64_words"],json!(words));
        assert_eq!(r[index]["value"]["storage_order"],"column-major");
    }
    assert!(r[2]["display_name"].is_null());assert!(r[24]["display_name"].is_null());
    for (index,code) in [(10,"IndexOperand"),(11,"IndexOperand"),(12,"IndexBounds"),(13,"IndexOperand"),(14,"IndexBounds"),(15,"IndexOperand"),(16,"UnknownBinding"),(17,"IndexBounds")] {
        assert_eq!(r[index]["outcome"],"error");assert_eq!(r[index]["error"]["code"],code);assert_eq!(r[index]["context"],r[9]["after"]);
    }
    for (index,text) in [(10,"element 1: NonFinite"),(11,"element 2: NonPositive"),(12,"argument 0, element 0"),(13,"argument 1, element 1")] {
        assert!(r[index]["error"]["message"].as_str().unwrap().contains(text));
    }
    scalar(&r[27],"3ff0000000000000");assert_eq!(r[28]["event"],"exit");
}

#[test]
fn matrix_whitespace_persistent_values_signs_error_spans_and_index_recovery() {
    let bad="  [1 +] % π";
    let r=CliCase::new().json(&format!("A=[1 2 3;4 5 6];\nA([2 1],[3;1;2])\n[-0 +0]\nB=[- 2 3];\n[1 -2]\n{bad}\nA=[1 2;3];\n[1e -2]\n[;]\n[1;]\n[1+]\nA\nans\n[1,- 2]\nA([2 1])\n:quit\n"));
    assert_eq!(r.len(),16);assert!(r[0]["display_name"].is_null());assert!(r[3]["display_name"].is_null());
    for (index,shape,words) in [
        (0,[2,3],vec!["3ff0000000000000","4010000000000000","4000000000000000","4014000000000000","4008000000000000","4018000000000000"]),
        (1,[2,3],vec!["4018000000000000","4008000000000000","4010000000000000","3ff0000000000000","4014000000000000","4000000000000000"]),
        (2,[1,2],vec!["8000000000000000","0000000000000000"]),(3,[1,2],vec!["c000000000000000","4008000000000000"]),
        (4,[1,2],vec!["3ff0000000000000","c000000000000000"]),
        (11,[2,3],vec!["3ff0000000000000","4010000000000000","4000000000000000","4014000000000000","4008000000000000","4018000000000000"]),
        (12,[1,2],vec!["3ff0000000000000","c000000000000000"]),(13,[1,2],vec!["3ff0000000000000","c000000000000000"]),
        (14,[1,2],vec!["4010000000000000","3ff0000000000000"]),
    ] {
        assert_eq!(r[index]["outcome"],"success");assert_eq!(r[index]["value"]["shape"],json!(shape));assert_eq!(r[index]["value"]["binary64_words"],json!(words));
    }
    for (index,code) in [(5,"UnexpectedToken"),(6,"MatrixNotAdmitted"),(7,"MalformedNumber"),(8,"UnexpectedToken"),(9,"UnexpectedToken"),(10,"UnexpectedToken")] {
        assert_eq!(r[index]["outcome"],"error");assert_eq!(r[index]["error"]["code"],code);assert_eq!(r[index]["context"],r[4]["after"]);
    }
    assert_eq!(r[5]["error"]["span"]["start"],bad.find(']').unwrap());assert_eq!(r[5]["error"]["span"]["end"],bad.find(']').unwrap()+1);
    assert_eq!(r[15]["event"],"exit");
}

#[test]
fn empty_index_persistent_geometry_assignment_ans_errors_and_recovery() {
    let bad="  B=A([],[4,1e309]); % π";
    let commands=["A=[1 2 3;4 5 6];","A([])","A(zeros(1,0))","A(zeros(0,1))","A(zeros(0,3))","A(zeros(3,0))",
        "VR=[1 2 3];","VR(zeros(0,1))","VC=VR.';","VC(zeros(1,0))","ER=zeros(1,0);","ER(zeros(0,1))",
        "EC=zeros(0,1);","EC(zeros(1,0))","A([],ones(1,16));","B=A(ones(16,1),[]);","ans",bad,
        "A([],4)","A([],missing)","ER([],1)","A([],[1,2;1,2])","B","A(2)","A=A([]);","A","ans",":quit"];
    let r=CliCase::new().json(&(commands.join("\n")+"\n"));assert_eq!(r.len(),28);
    for (index,shape) in [(1,[0,0]),(2,[1,0]),(3,[0,1]),(4,[0,3]),(5,[3,0]),(7,[1,0]),(9,[0,1]),
        (10,[1,0]),(11,[1,0]),(12,[0,1]),(13,[0,1]),(14,[0,16]),(15,[16,0]),(16,[0,16]),(22,[16,0]),(24,[0,0]),(25,[0,0])] {
        assert_eq!(r[index]["outcome"],"success","{}",commands[index]);assert_eq!(r[index]["value"]["shape"],json!(shape));
        assert_eq!(r[index]["value"]["binary64_words"],json!([]));assert_eq!(r[index]["value"]["storage_order"],"column-major");
    }
    for index in [0,6,8,10,12,14,15,24] {assert!(r[index]["display_name"].is_null());}
    for (index,code) in [(17,"IndexOperand"),(18,"IndexBounds"),(19,"UnknownBinding"),(20,"IndexBounds"),(21,"IndexOperand")] {
        assert_eq!(r[index]["outcome"],"error");assert_eq!(r[index]["error"]["code"],code);assert_eq!(r[index]["context"],r[16]["after"]);
    }
    assert!(r[17]["error"]["message"].as_str().unwrap().contains("argument 1, element 1: NonFinite"));
    assert_eq!(r[17]["error"]["span"]["start"],bad.find("[4").unwrap());assert_eq!(r[17]["error"]["span"]["end"],bad.find("[4").unwrap()+9);
    scalar(&r[23],"4010000000000000");scalar(&r[26],"4010000000000000");assert_eq!(r[27]["event"],"exit");
}

#[test]
fn colon_index_persistent_shapes_order_context_policy_caps_bits_and_recovery() {
    let bad="  zeros(:) % π";
    let commands=["A=[1 2 3;4 5 6];","A(:)","A(:,:)","10+A(:,[3,1])","A(:,[])","E=zeros(1,0);","E(:)","E(:,:)","ans",
        "B=A([2,1],:);","B","numel(A(:))",bad,"zeros(:,missing)","A(:,4)","A(:,:,:)","A((:))","M=ones(16);","M(:)","M(:,:)",
        "size=A;","size(:)","H=[-0 1e309];","H(:)","A=A(:);","A","ans",":quit"];
    let r=CliCase::new().json(&(commands.join("\n")+"\n"));assert_eq!(r.len(),28);
    for (index,shape,words) in [
        (1,[6,1],vec!["3ff0000000000000","4010000000000000","4000000000000000","4014000000000000","4008000000000000","4018000000000000"]),
        (2,[2,3],vec!["3ff0000000000000","4010000000000000","4000000000000000","4014000000000000","4008000000000000","4018000000000000"]),
        (3,[2,2],vec!["402a000000000000","4030000000000000","4026000000000000","402c000000000000"]),
        (4,[2,0],vec![]),(6,[0,1],vec![]),(7,[1,0],vec![]),(8,[1,0],vec![]),
        (9,[2,3],vec!["4010000000000000","3ff0000000000000","4014000000000000","4000000000000000","4018000000000000","4008000000000000"]),
        (23,[2,1],vec!["8000000000000000","7ff0000000000000"]),(26,[2,1],vec!["8000000000000000","7ff0000000000000"]),
    ] {assert_eq!(r[index]["outcome"],"success");assert_eq!(r[index]["value"]["shape"],json!(shape));assert_eq!(r[index]["value"]["binary64_words"],json!(words));}
    assert!(r[9]["display_name"].is_null());assert!(r[24]["display_name"].is_null());
    scalar(&r[11],"4018000000000000");
    for (index,code) in [(12,"UnsupportedCall"),(13,"UnknownBinding"),(14,"IndexBounds"),(15,"IndexArity"),(16,"ExpectedToken")] {
        assert_eq!(r[index]["outcome"],"error");
        if index==16 {assert!(r[index]["error"]["code"].is_string());} else {assert_eq!(r[index]["error"]["code"],code);}
        assert_eq!(r[index]["context"],r[11]["after"]);
    }
    assert_eq!(r[12]["error"]["span"]["start"],bad.find(':').unwrap());assert_eq!(r[12]["error"]["span"]["end"],bad.find(':').unwrap()+1);
    assert_eq!(r[18]["error"]["code"],"ArrayOperation");assert_eq!(r[18]["context"],r[17]["after"]);
    assert_eq!(r[19]["value"]["shape"],json!([16,16]));assert_eq!(r[19]["value"]["binary64_words"].as_array().unwrap().len(),256);
    for index in [21,24,25] {assert_eq!(r[index]["value"]["shape"],json!([6,1]));}
    assert_eq!(r[27]["event"],"exit");
}

#[test]
fn contextual_end_persistent_axes_inheritance_rebinding_bits_and_recovery() {
    let commands=["A=[1 2 3;4 5 6];","A(end)","A(end,end)","B=A(:,end);","B","A(reshape(end,1,1))","C=[1 2 3];","A(C(end))","A(end+1)","A(end,missing)","A(end,missing,1)","numel(end)","[end]","A(end+)","ans","A(1:end-1,end)","H=[-0 1e309];","H(end:-1:1)","E=zeros(0,3);","E(:,end)","E(end,:)","A=A(end);","A","ans",":quit"];
    let r=CliCase::new().json(&(commands.join("\n")+"\n"));assert_eq!(r.len(),25);
    for i in [1,2,5,21,22] {scalar(&r[i],"4018000000000000");}
    assert!(r[3]["display_name"].is_null());assert_eq!(r[4]["value"]["shape"],json!([2,1]));assert_eq!(r[4]["value"]["binary64_words"],json!(["4008000000000000","4018000000000000"]));scalar(&r[7],"4000000000000000");
    for (i,code) in [(8,"IndexBounds"),(9,"UnknownBinding"),(10,"IndexArity"),(11,"UnsupportedCall"),(12,"InvalidIdentifier")] {assert_eq!(r[i]["outcome"],"error");assert_eq!(r[i]["error"]["code"],code);assert_eq!(r[i]["context"],r[7]["after"]);}
    assert_eq!(r[13]["outcome"],"error");assert_eq!(r[13]["context"],r[7]["after"]);scalar(&r[14],"4000000000000000");scalar(&r[15],"4008000000000000");
    assert_eq!(r[17]["value"]["binary64_words"],json!(["7ff0000000000000","8000000000000000"]));assert_eq!(r[19]["value"]["shape"],json!([0,1]));assert_eq!(r[20]["error"]["code"],"IndexOperand");assert_eq!(r[20]["context"],r[19]["after"]);assert_eq!(r[24]["event"],"exit");
}

#[test]
fn construction_persistent_operand_geometry_end_bits_errors_and_recovery() {
    let commands=["A=[1;2];","[A,A]","B=[A;A];","B","[1 - 2]","A([end])","[A (1)]","[ones(1,2);ones(1,1);missing]","[A,1;missing]","[zeros(0,2),zeros(3,0),zeros(0,4)]","[[zeros(0,2),zeros(3,0)],zeros(0,4)]","[zeros(0,16),zeros(0,1)]","[end]","[A](1)","H=[-0 1e309];","[H;H]","A=[A;A];","A","ans",":quit"];
    let r=CliCase::new().json(&(commands.join("\n")+"\n"));assert_eq!(r.len(),20);
    for (i,shape,words) in [(1,[2,2],vec!["3ff0000000000000","4000000000000000","3ff0000000000000","4000000000000000"]),(2,[4,1],vec!["3ff0000000000000","4000000000000000","3ff0000000000000","4000000000000000"]),(3,[4,1],vec!["3ff0000000000000","4000000000000000","3ff0000000000000","4000000000000000"]),(9,[0,0],vec![]),(10,[0,4],vec![]),(15,[2,2],vec!["8000000000000000","8000000000000000","7ff0000000000000","7ff0000000000000"])] {assert_eq!(r[i]["outcome"],"success");assert_eq!(r[i]["value"]["shape"],json!(shape));assert_eq!(r[i]["value"]["binary64_words"],json!(words));}
    assert!(r[2]["display_name"].is_null());scalar(&r[4],"bff0000000000000");scalar(&r[5],"4000000000000000");
    for (i,code,context) in [(6,"ArrayOperation",5),(7,"UnknownBinding",5),(8,"ArrayOperation",5),(11,"ArrayOperation",10),(12,"InvalidIdentifier",10)] {assert_eq!(r[i]["outcome"],"error");assert_eq!(r[i]["error"]["code"],code);assert_eq!(r[i]["context"],r[context]["after"]);}
    assert_eq!(r[13]["outcome"],"error");assert_eq!(r[13]["context"],r[10]["after"]);assert_eq!(r[16]["value"]["shape"],json!([4,1]));assert_eq!(r[17]["value"]["shape"],json!([4,1]));assert_eq!(r[18]["value"]["binary64_words"],r[15]["value"]["binary64_words"]);assert_eq!(r[19]["event"],"exit");
}

#[test]
fn indexed_write_persistent_scatter_alias_ans_bits_empty_noop_and_recovery() {
    let commands=["A=[1 2 3;4 5 6];","91","A([2,1],[3,1])=[10,30;20,40]","ans","A([1,1])=[7,8];","A","A([2,1])=A([1,2]);","A","A(7)=1","New(1)=1","A(0,missing)=[]","A","M=ones(16);","M(:)=2;","M(:)","M","E=zeros(0,3);","Z=zeros(0,1);","E(:,1)=Z;","E","H=[-0 1e309];","H([2,1])=H;","H",":quit"];
    let r=CliCase::new().json(&(commands.join("\n")+"\n"));assert_eq!(r.len(),24);assert_eq!(r[2]["display_name"],"A");assert_eq!(r[2]["value"]["shape"],json!([2,3]));assert_eq!(r[2]["value"]["binary64_words"],json!(["4044000000000000","403e000000000000","4000000000000000","4014000000000000","4034000000000000","4024000000000000"]));scalar(&r[3],"4056c00000000000");assert!(r[4]["display_name"].is_null());assert_eq!(r[5]["value"]["binary64_words"][0],"4020000000000000");assert_eq!(r[7]["value"]["binary64_words"][0],"403e000000000000");assert_eq!(r[7]["value"]["binary64_words"][1],"4020000000000000");
    for (i,code) in [(8,"IndexBounds"),(9,"UnknownBinding"),(10,"UnknownBinding")] {assert_eq!(r[i]["outcome"],"error");assert_eq!(r[i]["error"]["code"],code);assert_eq!(r[i]["context"],r[7]["after"]);}
    assert_eq!(r[13]["value"]["shape"],json!([16,16]));assert_eq!(r[13]["value"]["binary64_words"].as_array().unwrap().len(),256);assert_eq!(r[14]["error"]["code"],"ArrayOperation");assert_eq!(r[14]["context"],r[13]["after"]);assert_eq!(r[15]["value"]["binary64_words"][255],"4000000000000000");assert_eq!(r[18]["value"]["shape"],json!([0,3]));assert_eq!(r[18]["after"]["workspace_revision"].as_u64().unwrap(),r[18]["before"]["workspace_revision"].as_u64().unwrap()+1);assert_eq!(r[22]["value"]["binary64_words"],json!(["7ff0000000000000","8000000000000000"]));assert_eq!(r[23]["event"],"exit");
}

#[test]
fn indexed_write_empty_diagnostic_exact_rhs_span_stage_display_and_atomic_context() {
    let bad="  E(:,1) = ((Z)); % π";
    let commands=["E=zeros(0,3);","Z=zeros(0,0);","91","E(:,1)=[]","E(:,1)=zeros(0,0)",bad,"E(:,4)=[]","E(0,missing)=[]","E(:,1)=zeros(0,1);","E","ans",":quit"];
    let r=CliCase::new().json(&(commands.join("\n")+"\n"));assert_eq!(r.len(),12);
    for i in 3..=5 {assert_eq!(r[i]["kind"],"mathlab-rust-session");assert_eq!(r[i]["error"]["code"],"UnsupportedEmptyAssignment");assert_eq!(r[i]["error"]["stage"],"Evaluation");assert_eq!(r[i]["context"],r[2]["after"]);}
    let start=bad.find("((Z))").unwrap();assert_eq!(r[5]["error"]["span"]["start"],start);assert_eq!(r[5]["error"]["span"]["end"],start+5);assert_eq!(r[5]["source"],bad);assert_eq!(r[5]["error"]["message"],format!("UnsupportedEmptyAssignment: 0-by-0 right-hand sides are unsupported for fixed-shape indexed assignment at bytes {}..{}.",start,start+5));
    assert_eq!(r[6]["error"]["code"],"IndexBounds");assert_eq!(r[7]["error"]["code"],"UnknownBinding");assert_eq!(r[8]["value"]["shape"],json!([0,3]));assert_eq!(r[8]["after"]["workspace_revision"].as_u64().unwrap(),r[8]["before"]["workspace_revision"].as_u64().unwrap()+1);assert!(r[8]["display_name"].is_null());scalar(&r[10],"4056c00000000000");assert_eq!(r[11]["event"],"exit");
}


#[test]
fn sum_default_persistent_geometry_exact_words_shadowing_and_recovery() {
    let u26=format!("sum([9007199254740992 {} -9007199254740992])",vec!["1";14].join(" "));
    let commands=["A=[1 2 3;4 5 6];","sum(A)","B=sum(A);","ans","A(1,:)=sum(A);","A","sum(missing)","sum(missing,missing)","sum(:)","sum(zeros(0,0))","sum(zeros(3,0))","sum([-0 -0])",&u26,"sum([1e308 1e308 -1e308 -1e308])","sum=[8 9];","sum(end)","sum",":quit"];
    let r=CliCase::new().json(&(commands.join("\n")+"\n"));assert_eq!(r.len(),18);
    for i in [1,2,3] {assert_eq!(r[i]["value"]["shape"],json!([1,3]));assert_eq!(r[i]["value"]["binary64_words"],json!(["4014000000000000","401c000000000000","4022000000000000"]));}
    assert!(r[2]["display_name"].is_null());assert_eq!(r[1]["display_name"],"ans");
    for i in [1,2,4] {assert_eq!(r[i]["after"]["workspace_revision"].as_u64().unwrap(),r[i]["before"]["workspace_revision"].as_u64().unwrap()+1);}
    for (i,code) in [(6,"UnknownBinding"),(7,"CallArity"),(8,"UnsupportedCall")] {assert_eq!(r[i]["error"]["code"],code);assert_eq!(r[i]["error"]["stage"],"Evaluation");assert_eq!(r[i]["context"],r[5]["after"]);}
    assert_eq!(r[6]["source"],"sum(missing)");assert_eq!(r[6]["error"]["span"]["start"],4);assert_eq!(r[6]["error"]["span"]["end"],11);
    scalar(&r[9],"0000000000000000");assert_eq!(r[10]["value"]["shape"],json!([1,0]));scalar(&r[11],"8000000000000000");
    // Approved same-profile amendment: U26 rounds exact14; U27 cancellation yields +0.
    scalar(&r[12],"402c000000000000");scalar(&r[13],"0000000000000000");scalar(&r[15],"4022000000000000");
    assert_eq!(r[16]["value"]["binary64_words"],json!(["4020000000000000","4022000000000000"]));assert_eq!(r[17]["event"],"exit");
}

#[test]
fn sum_default_text_display_and_suppression_use_existing_serializer() {
    let output=CliCase::new().run("sum([1 2 3])\nsum([4 5]);\n:quit\n",&["repl","--backend","rust"]);
    assert_eq!(output.status.code(),Some(0));assert!(output.stderr.is_empty());
    let stdout=String::from_utf8(output.stdout).unwrap();assert_eq!(stdout.matches("ans =\n6\n").count(),1);assert!(!stdout.contains("ans =\n9\n"));
}


#[test]
fn extrema_default_persistent_geometry_words_shadowing_write_policy_and_recovery() {
    for operation in ["min","max"] {
        let q=format!("{operation}(A)");let assign=format!("B={operation}(A);");let write=format!("A(1,:)={operation}(A);");let unknown=format!("{operation}(missing)");let arity=format!("{operation}(missing,missing)");let marker=format!("{operation}(:)");let empty=format!("{operation}([])");let emptycols=format!("{operation}(zeros(0,3))");let emptyrows=format!("{operation}(zeros(3,0))");let negativezero=format!("{operation}([-0 0])");let positivezero=format!("{operation}([0 -0])");let badwrite=format!("A(1)={operation}([])");let shadow=format!("{operation}=[8 9];");let index=format!("{operation}(end)");
        let commands=["A=[1 2 3;4 5 6];",&q,&assign,"ans",&write,"A",&unknown,&arity,&marker,&empty,&emptycols,&emptyrows,&negativezero,&positivezero,&badwrite,"A",&shadow,&index,operation,":quit"];
        let r=CliCase::new().json(&(commands.join("\n")+"\n"));assert_eq!(r.len(),20);let expected=if operation=="min" {json!(["3ff0000000000000","4000000000000000","4008000000000000"])} else {json!(["4010000000000000","4014000000000000","4018000000000000"])};
        for i in [1,2,3] {assert_eq!(r[i]["value"]["shape"],json!([1,3]));assert_eq!(r[i]["value"]["binary64_words"],expected);}assert!(r[2]["display_name"].is_null());assert_eq!(r[1]["display_name"],"ans");
        for i in [1,2,4] {assert_eq!(r[i]["after"]["workspace_revision"].as_u64().unwrap(),r[i]["before"]["workspace_revision"].as_u64().unwrap()+1);}
        for (i,code) in [(6,"UnknownBinding"),(7,"CallArity"),(8,"UnsupportedCall")] {assert_eq!(r[i]["error"]["code"],code);assert_eq!(r[i]["error"]["stage"],"Evaluation");assert_eq!(r[i]["context"],r[5]["after"]);}assert_eq!(r[6]["source"],unknown);assert_eq!(r[6]["error"]["span"]["start"],4);assert_eq!(r[6]["error"]["span"]["end"],11);
        for (i,shape) in [(9,[0,0]),(10,[0,3]),(11,[1,0])] {assert_eq!(r[i]["value"]["shape"],json!(shape));assert_eq!(r[i]["value"]["binary64_words"],json!([]));}scalar(&r[12],"8000000000000000");scalar(&r[13],"0000000000000000");
        assert_eq!(r[14]["error"]["code"],"UnsupportedEmptyAssignment");assert_eq!(r[14]["error"]["span"]["start"],5);assert_eq!(r[14]["error"]["span"]["end"],12);assert_eq!(r[14]["context"],r[13]["after"]);assert_eq!(r[14]["error"]["message"],"UnsupportedEmptyAssignment: 0-by-0 right-hand sides are unsupported for fixed-shape indexed assignment at bytes 5..12.");scalar(&r[17],"4022000000000000");assert_eq!(r[18]["value"]["binary64_words"],json!(["4020000000000000","4022000000000000"]));assert_eq!(r[19]["event"],"exit");
    }
}

#[test]
fn extrema_default_text_display_and_suppression_use_existing_serializer() {
    let output=CliCase::new().run("min([3 1 2])\nmax([3 1 2])\nmax([4 5]);\n:quit\n",&["repl","--backend","rust"]);assert_eq!(output.status.code(),Some(0));assert!(output.stderr.is_empty());let stdout=String::from_utf8(output.stdout).unwrap();assert_eq!(stdout.matches("ans =\n1\n").count(),1);assert_eq!(stdout.matches("ans =\n3\n").count(),1);assert!(!stdout.contains("ans =\n5\n"));
}


#[test]
fn exact_mean_prod_persistent_words_geometry_policy_and_atomic_recovery() {
    for operation in ["mean","prod"] {
        let query=format!("{operation}(A)");let assign=format!("B={operation}(A);");let write=format!("A(1,:)={operation}(A);");let unknown=format!("{operation}(missing)");let arity=format!("{operation}(missing,missing)");let marker=format!("{operation}(:)");let empty=format!("{operation}([])");let emptycols=format!("{operation}(zeros(0,3))");let emptyrows=format!("{operation}(zeros(3,0))");let zero=format!("{operation}(-0)");let shadow=format!("{operation}=[8 9];");let index=format!("{operation}(end)");
        let commands=["A=[1 2 3;4 5 6];",&query,&assign,"ans",&write,"A",&unknown,&arity,&marker,&empty,&emptycols,&emptyrows,&zero,"A(1)=min([])","mean([9007199254740992 1 -9007199254740992])","mean([1e308 1e308 -1e308 -1e308])","sum([1e308 1e308 -1e308 -1e308])",&shadow,&index,operation,":quit"];
        let r=CliCase::new().json(&(commands.join("\n")+"\n"));assert_eq!(r.len(),21);let expected=if operation=="mean"{json!(["4004000000000000","400c000000000000","4012000000000000"])}else{json!(["4010000000000000","4024000000000000","4032000000000000"])};
        for i in [1,2,3] {assert_eq!(r[i]["value"]["shape"],json!([1,3]));assert_eq!(r[i]["value"]["binary64_words"],expected);}assert!(r[2]["display_name"].is_null());assert_eq!(r[1]["display_name"],"ans");for i in [1,2,4] {assert_eq!(r[i]["after"]["workspace_revision"].as_u64().unwrap(),r[i]["before"]["workspace_revision"].as_u64().unwrap()+1);}
        for (i,code) in [(6,"UnknownBinding"),(7,"CallArity"),(8,"UnsupportedCall")] {assert_eq!(r[i]["error"]["code"],code);assert_eq!(r[i]["context"],r[5]["after"]);}assert_eq!(r[6]["source"],unknown);assert_eq!(r[6]["error"]["span"]["start"],5);assert_eq!(r[6]["error"]["span"]["end"],12);
        scalar(&r[9],if operation=="mean"{"7ff8000000000000"}else{"3ff0000000000000"});assert_eq!(r[10]["value"]["shape"],json!([1,3]));assert_eq!(r[11]["value"]["shape"],json!([1,0]));scalar(&r[12],"8000000000000000");assert_eq!(r[13]["error"]["code"],"UnsupportedEmptyAssignment");assert_eq!(r[13]["context"],r[12]["after"]);scalar(&r[14],"3fd5555555555555");scalar(&r[15],"0000000000000000");scalar(&r[16],"0000000000000000");scalar(&r[18],"4022000000000000");assert_eq!(r[19]["value"]["binary64_words"],json!(["4020000000000000","4022000000000000"]));assert_eq!(r[20]["event"],"exit");
    }
}

#[test]
fn exact_mean_prod_text_display_and_suppression_use_existing_serializer() {
    let output=CliCase::new().run("mean([1 3])\nprod([2 3])\nmean([4 6]);\n:quit\n",&["repl","--backend","rust"]);assert_eq!(output.status.code(),Some(0));assert!(output.stderr.is_empty());let stdout=String::from_utf8(output.stdout).unwrap();assert_eq!(stdout.matches("ans =\n2\n").count(),1);assert_eq!(stdout.matches("ans =\n6\n").count(),1);assert!(!stdout.contains("ans =\n5\n"));
}

#[test]
fn exact_sum_synthetic_infinity_classification_and_prior_mean_prod_extrema() {
    // Synthetic evaluated-word deductions, not new native observations.
    let source="I=1e308+1e308;\nM=1.7976931348623157e308;\nsum([M M -I])\nsum([-M -M I])\nsum([I -I])\nsum([9007199254740992 1 -9007199254740992])\nmean([9007199254740992 1 -9007199254740992])\nprod([2 3])\nmin([-0 0])\nsum(missing)\nsum([1 2])\n:quit\n";
    let r=CliCase::new().json(source);assert_eq!(r.len(),12);scalar(&r[2],"fff0000000000000");scalar(&r[3],"7ff0000000000000");assert!(u64::from_str_radix(r[4]["value"]["binary64_words"][0].as_str().unwrap(),16).map(f64::from_bits).unwrap().is_nan());
    scalar(&r[5],"3ff0000000000000");scalar(&r[6],"3fd5555555555555");scalar(&r[7],"4018000000000000");scalar(&r[8],"8000000000000000");assert_eq!(r[9]["error"]["code"],"UnknownBinding");assert_eq!(r[9]["context"],r[8]["after"]);scalar(&r[10],"4008000000000000");
}
