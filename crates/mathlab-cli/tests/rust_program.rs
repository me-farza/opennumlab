// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(unix)]
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;
const LOG: u64 = 1024 * 1024;
struct Process {
    child: Child,
    out: PathBuf,
    err: PathBuf,
}
impl Drop for Process {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
impl Process {
    fn bounded(&self) {
        for p in [&self.out, &self.err] {
            assert!(fs::metadata(p).unwrap().len() <= LOG, "childoutputbound");
        }
    }
    fn ready(&mut self) -> Value {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            self.bounded();
            let bytes = fs::read(&self.out).unwrap();
            let text = String::from_utf8_lossy(&bytes);
            if let Some(end) = bytes.iter().position(|b| *b == b'\n') {
                if let Ok(value) = serde_json::from_slice::<Value>(&bytes[..end]) {
                    assert_eq!(value["event"], "ready", "{text}");
                    return value;
                }
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "CLI ended before ready: {text};{}",
                fs::read_to_string(&self.err).unwrap()
            );
            assert!(Instant::now() < deadline, "readydeadline");
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn signal(&self, number: i32) {
        assert_eq!(
            unsafe { libc::kill(self.child.id() as libc::pid_t, number) },
            0
        );
    }
    fn finish(mut self) -> (i32, String, String) {
        let deadline = Instant::now() + Duration::from_secs(4);
        let status = loop {
            self.bounded();
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline, "CLI childdeadline");
            thread::sleep(Duration::from_millis(2));
        };
        self.bounded();
        (
            status.code().unwrap_or(-1),
            fs::read_to_string(&self.out).unwrap(),
            fs::read_to_string(&self.err).unwrap(),
        )
    }
}
struct Case {
    dir: TempDir,
    serial: usize,
}
impl Case {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("octave-invoked");
        let sentinel = dir.path().join("octave-cli");
        fs::write(
            &sentinel,
            format!(
                "#!/bin/sh\nprintf invoked > '{}'\nexit 86\n",
                marker.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&sentinel, fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir, serial: 0 }
    }
    fn source(&self, name: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
        let p = self.dir.path().join(name);
        fs::write(&p, bytes).unwrap();
        p
    }
    fn spawn(&mut self, args: &[&str], file: &Path) -> Process {
        self.serial += 1;
        let out = self.dir.path().join(format!("stdout{}", self.serial));
        let err = self.dir.path().join(format!("stderr{}", self.serial));
        let child = Command::new(env!("CARGO_BIN_EXE_mathlab"))
            .args(args)
            .arg(file)
            .env_clear()
            .env("PATH", self.dir.path())
            .env("MATHLAB_OCTAVE", self.dir.path().join("octave-cli"))
            .env("TMPDIR", self.dir.path())
            .current_dir(self.dir.path())
            .stdin(Stdio::null())
            .stdout(File::create(&out).unwrap())
            .stderr(File::create(&err).unwrap())
            .spawn()
            .unwrap();
        Process { child, out, err }
    }
    fn json(&mut self, source: &str) -> Value {
        let p = self.source("program.m", source);
        let (code, text, err) = self
            .spawn(&["--json", "run", "--rust-program", "--workspace"], &p)
            .finish();
        assert_eq!(code, 0, "{text}\n{err}");
        let records = records(&text);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["event"], "ready");
        assert_eq!(records[1]["status"], "success");
        self.no_octave();
        records[1].clone()
    }
    fn no_octave(&self) {
        assert!(!self.dir.path().join("octave-invoked").exists());
    }
}
fn records(text: &str) -> Vec<Value> {
    text.lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn binding<'a>(result: &'a Value, name: &str) -> Option<&'a Value> {
    result["workspace"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .map(|e| &e["value"])
}
fn word(result: &Value, name: &str) -> String {
    binding(result, name).unwrap()["binary64_words"][0]
        .as_str()
        .unwrap()
        .to_owned()
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[test]
fn ca_scientific_a_cli() {
    let mut c = Case::new();
    let result=c.json("A=[1,2;3,4];caller=A;result=sum_squares(4);\nfunction total=sum_squares(n)\ntotal=0;\nfor k=1:n,total=total+k*k;end\nreturn;\nend\n");
    assert_eq!(word(&result, "result"), "403e000000000000");
    assert!(binding(&result, "total").is_none());
    assert!(binding(&result, "n").is_none());
    assert_eq!(
        binding(&result, "caller").unwrap()["shape"],
        serde_json::json!([2, 2])
    );
}
#[test]
fn ca_scientific_b_cli() {
    let mut c = Case::new();
    let result=c.json("x=1;a=2;iterations=0;\nwhile iterations<8&&x>0\nnext=newton_step(x,a);iterations=iterations+1;\nif next==x,break;end\nx=next;\nend\nfunction y=newton_step(x,a)\ny=(x+a./x)./2;\nend\n");
    assert_eq!(word(&result, "x"), "3ff6a09e667f3bcc");
    assert!(binding(&result, "y").is_none());
}
#[test]
fn ca_scientific_c_cli() {
    let mut c = Case::new();
    let result=c.json("A=[4,-1;-1,4];b=[3;3];x=[0;0];caller=x;history=zeros(2,8);\nfor k=1:8\n[x,residual_sq]=jacobi(A,b,x);\nfor row=1:2,history(row,k)=x(row);end\nend\nfunction [next,residual_sq]=jacobi(A,b,x)\nnext=[(3+x(2))./4;(3+x(1))./4];\nresidual_sq=sum((b-A*next).*(b-A*next));\nend\n");
    assert_eq!(
        binding(&result, "x").unwrap()["binary64_words"],
        serde_json::json!(["3fefffe000000000", "3fefffe000000000"])
    );
    assert_eq!(word(&result, "residual_sq"), "3e32000000000000");
    assert_eq!(
        binding(&result, "history").unwrap()["shape"],
        serde_json::json!([2, 8])
    );
    assert_eq!(
        binding(&result, "caller").unwrap()["binary64_words"],
        serde_json::json!(["0000000000000000", "0000000000000000"])
    );
    assert!(binding(&result, "next").is_none());
}
#[test]
fn cb_typed_events_suppression_empty_and_human_workspace() {
    let mut c = Case::new();
    let result = c.json("mask=[1 2]>1\nsilent=2;empty=[];mask\n");
    assert_eq!(result["events"].as_array().unwrap().len(), 2);
    let v = binding(&result, "mask").unwrap();
    assert_eq!(v["class"], "logical");
    assert_eq!(v["logical_values"], serde_json::json!([false, true]));
    assert!(v.get("binary64_words").is_none());
    assert_eq!(
        binding(&result, "empty").unwrap()["shape"],
        serde_json::json!([0, 0])
    );
    let p = c.source("human.m", "x=1; mask=1<2;\n");
    let (code, text, err) = c
        .spawn(&["run", "--rust-program", "--workspace"], &p)
        .finish();
    assert_eq!(code, 0, "{err}");
    assert!(text.contains("Workspace:"));
    assert!(text.contains("1.0000000000000000e0"));
    assert!(text.contains("logical 1x1"));
    let result = c.json("");
    assert!(result["events"].as_array().unwrap().is_empty());
    assert!(result["workspace"].as_array().unwrap().is_empty());
}
#[test]
fn cb_runtime_prefix_error_hash_spans_stack() {
    let mut c = Case::new();
    let source = "\u{feff}% α\r\nprior=1;[a,b]=bad();later=3;\r\nfunction [a,b]=bad\r\nlocal=7\r\na=1;b=missing;\r\nend\r\n";
    let p = c.source("unicode α file.m", source);
    let (code, text, err) = c
        .spawn(&["run", "--json", "--rust-program", "--workspace"], &p)
        .finish();
    assert_eq!(code, 1, "{err}");
    let r = records(&text);
    assert_eq!(r.len(), 2);
    let out = &r[1];
    assert_eq!(out["source"]["sha256"], digest(source.as_bytes()));
    assert_eq!(out["source"]["bytes"], source.len());
    assert_eq!(out["error"]["code"], "UnknownBinding");
    assert_eq!(
        out["error"]["location"]["span"]["start"],
        source.find("missing").unwrap()
    );
    assert_eq!(out["error"]["location"]["span"]["start_line"], 5);
    assert_eq!(out["error"]["call_stack"].as_array().unwrap().len(), 1);
    assert_eq!(word(out, "prior"), "3ff0000000000000");
    for name in ["a", "b", "local", "later"] {
        assert!(binding(out, name).is_none());
    }
    assert_eq!(out["events"][0]["name"], "local");
    assert_eq!(out["after"]["revision"], 1);
    c.no_octave();
}
#[test]
fn cb_source_bounds_utf8_parse_and_nonregular() {
    let mut c = Case::new();
    for (n, size, expected) in [("max.m", 65536, 0), ("over.m", 65537, 1)] {
        let p = c.source(n, format!("%{}", "a".repeat(size - 1)));
        let (code, text, _) = c.spawn(&["run", "--rust-program", "--json"], &p).finish();
        assert_eq!(code, expected, "{text}");
        let r = records(&text);
        if size == 65536 {
            assert_eq!(r.len(), 2);
            assert_eq!(r[1]["source"]["bytes"], 65536);
        } else {
            assert_eq!(r.len(), 1);
            assert_eq!(r[0]["error"]["code"], "SourceLimit");
        }
    }
    let p = c.source("invalid.m", [b'x', b'=', 0xff]);
    let (code, text, _) = c.spawn(&["run", "--rust-program", "--json"], &p).finish();
    assert_eq!(code, 1);
    assert_eq!(records(&text)[0]["error"]["code"], "InvalidUtf8");
    let p = c.source("syntax.m", "x=(\n");
    let (code, text, _) = c.spawn(&["run", "--rust-program", "--json"], &p).finish();
    assert_eq!(code, 1);
    assert_eq!(records(&text)[0]["phase"], "parse");
    let directory = c.dir.path().to_path_buf();
    let (code, text, _) = c
        .spawn(&["run", "--rust-program", "--json"], &directory)
        .finish();
    assert_eq!(code, 1);
    assert_eq!(records(&text)[0]["error"]["code"], "NotRegularFile");
    let fifo = c.dir.path().join("input.fifo");
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let (code, text, _) = c
        .spawn(&["run", "--rust-program", "--json"], &fifo)
        .finish();
    assert_eq!(code, 1);
    assert_eq!(records(&text)[0]["error"]["code"], "NotRegularFile");
    let missing = c.dir.path().join("missing.m");
    let (code, text, _) = c
        .spawn(&["run", "--rust-program", "--json"], &missing)
        .finish();
    assert_eq!(code, 1);
    assert_eq!(records(&text)[0]["error"]["code"], "SourceIo");
    c.no_octave();
}
#[test]
fn cb_option_conflicts_and_low_fuel() {
    let mut c = Case::new();
    let p = c.source("program.m", "prior=1;while 1,end\n");
    for args in [
        vec!["run", "--rust-program", "--json", "--octave", "missing"],
        vec!["run", "--rust-program", "--startup-config", "missing"],
        vec!["run", "--rust-program", "--directory", "missing"],
        vec!["run", "--program-transition-fuel", "1"],
    ] {
        let (code, text, err) = c.spawn(&args, &p).finish();
        assert_eq!(code, 2, "{text}{err}");
    }
    for flag in ["--program-transition-fuel", "--program-primitive-fuel"] {
        let (code, text, err) = c
            .spawn(
                &["--json", "run", "--rust-program", flag, "10", "--workspace"],
                &p,
            )
            .finish();
        assert_eq!(code, 1, "{err}");
        let r = records(&text);
        assert_eq!(r[1]["status"], "resource");
        assert_eq!(word(&r[1], "prior"), "3ff0000000000000");
    }
    c.no_octave();
}
#[test]
fn cc_real_sigint_after_ready_keeps_snapshot_and_exact_prefix() {
    let mut c = Case::new();
    let source = "prior=7\nwhile 1,end\n";
    let p = c.source("interrupt.m", source);
    let mut process = c.spawn(&["run", "--rust-program", "--json", "--workspace"], &p);
    let ready = process.ready();
    fs::write(&p, "should_not_run=999;").unwrap();
    process.signal(libc::SIGINT);
    let (code, text, err) = process.finish();
    assert_eq!(code, 130, "{text}{err}");
    let r = records(&text);
    assert_eq!(r.len(), 2);
    let out = &r[1];
    assert_eq!(out["status"], "cancelled");
    assert_eq!(out["reason"], "UserStop");
    assert_eq!(out["key"], ready["key"]);
    assert_eq!(out["source"]["sha256"], digest(source.as_bytes()));
    assert!(binding(out, "should_not_run").is_none());
    let revision = out["after"]["revision"].as_u64().unwrap();
    assert!(revision <= 1);
    assert_eq!(out["events"].as_array().unwrap().len(), revision as usize);
    assert_eq!(binding(out, "prior").is_some(), revision == 1);
    if revision == 1 {
        assert_eq!(word(out, "prior"), "401c000000000000");
    }
    let recovered = c.json("recovered=3;");
    assert_eq!(word(&recovered, "recovered"), "4008000000000000");
    c.no_octave();
}
#[test]
fn cc_real_deadline_during_owned_stop_and_resume() {
    let mut c = Case::new();
    let p = c.source("deadline.m", "prior=1;while 1,end\n");
    let mut process = c.spawn(
        &[
            "run",
            "--rust-program",
            "--json",
            "--timeout",
            "1",
            "--workspace",
        ],
        &p,
    );
    process.ready();
    process.signal(libc::SIGSTOP);
    thread::sleep(Duration::from_millis(1100));
    process.signal(libc::SIGCONT);
    let (code, text, err) = process.finish();
    assert_eq!(code, 124, "{text}{err}");
    let r = records(&text);
    assert_eq!(r[1]["status"], "cancelled");
    assert_eq!(r[1]["reason"], "Deadline");
    c.no_octave();
}
#[test]
fn cb_reference_f05_f15_f16_reconciliation() {
    let mut c = Case::new();
    let result=c.json("pf_a=1;\n2;\npf_saved_ans=ans;\npf_out=pf_count();\npf_count()\nfunction y=pf_count()\ny=nargout;\nend\n");
    assert_eq!(word(&result, "ans"), "0000000000000000");
    assert_eq!(word(&result, "pf_out"), "3ff0000000000000");
    assert_eq!(word(&result, "pf_saved_ans"), "4000000000000000");
    let result =
        c.json("pf_a=1;\n[pf_a,pf_a]=pf_pair();\nfunction [a,b]=pf_pair()\na=11;\nb=22;\nend\n");
    assert_eq!(word(&result, "pf_a"), "4036000000000000");
    assert_eq!(result["workspace"].as_array().unwrap().len(), 1);
    let source = "pf_forward=sum(1);\nfunction y=sum(x)\ny=x+100;\nend\npf_late=sum(2);\nsum=[7,8];\npf_index=sum(2);\n";
    let path = c.source("F16.m", source);
    let (code, text, err) = c
        .spawn(&["run", "--rust-program", "--json", "--workspace"], &path)
        .finish();
    assert_eq!(code, 1, "{text}{err}");
    let r = records(&text);
    assert_eq!(r.len(), 1);
    let out = &r[0];
    assert_eq!(out["event"], "outcome");
    assert_eq!(out["phase"], "parse");
    assert_eq!(out["error"]["code"], "DuplicateName");
    assert_eq!(out["error"]["location"]["span"]["start_line"], 6);
    assert_eq!(out["source"]["sha256"], digest(source.as_bytes()));
    assert_eq!(out["effect_sequence"], 0);
    assert_eq!(out["completed_leaf_statement_count"], 0);
    assert!(out["key"].is_null());
    assert!(out["before"].is_null());
    assert!(out["after"].is_null());
    assert!(out["events"].as_array().unwrap().is_empty());
    assert!(out["workspace"].as_array().unwrap().is_empty());
    c.no_octave();
    let valid = c.json("pf_forward=sum(1);\nfunction y=sum(x)\ny=x+100;\nend\npf_late=sum(2);\n");
    assert_eq!(word(&valid, "pf_forward"), "4059400000000000");
    assert_eq!(word(&valid, "pf_late"), "4059800000000000");
}

#[test]
fn cb_control_reconciliation_power_empty_loops_and_output_counts() {
    let mut c = Case::new();
    let result = c.json("pf_unparenthesized=2^-2^3;pf_left=(2^-2)^3;pf_grouped=2^-(2^3);k=7;count=0;\nfor k=zeros(3,0)\ncount=count+1;\nend\nfor col=zeros(0,3)\ncount=count+1;\nend\nfirst_unset();many();one=many();[a,b]=many();\nfunction [a,b]=first_unset\nb=22;end\nfunction [a,b]=many\nseen=nargout\na=11;b=22;end\n");
    assert_eq!(word(&result, "pf_unparenthesized"), "3f90000000000000");
    assert_eq!(word(&result, "pf_left"), "3f90000000000000");
    assert_eq!(word(&result, "pf_grouped"), "3f70000000000000");
    assert_eq!(binding(&result, "k").unwrap()["shape"], serde_json::json!([0, 0]));
    assert_eq!(binding(&result, "col").unwrap()["shape"], serde_json::json!([0, 1]));
    assert_eq!(word(&result, "count"), "4008000000000000");
    assert_eq!(word(&result, "ans"), "4026000000000000");
    assert_eq!(result["events"].as_array().unwrap().iter().map(|e| e["value"]["binary64_words"][0].as_str().unwrap()).collect::<Vec<_>>(), ["0000000000000000", "3ff0000000000000", "4000000000000000"]);
}
#[test]
fn cb_derived_nan_condition_is_located_runtime_error_without_later_effects() {
    let mut c = Case::new();
    let source = "before=7;\nif 0./0\nwrong=1;\nelse\nwrong=2;\nend\nafter=9;";
    let path = c.source("derived_nan.m", source);
    let (code, text, err) = c.spawn(&["run", "--rust-program", "--json", "--workspace"], &path).finish();
    assert_eq!(code, 1, "{text}{err}");
    let records = records(&text);
    assert_eq!(records.len(), 2);
    let out = &records[1];
    assert_eq!(out["phase"], "execution");
    assert_eq!(out["error"]["code"], "Type");
    assert_eq!(out["error"]["location"]["span"]["start_line"], 2);
    assert_eq!(out["source"]["sha256"], digest(source.as_bytes()));
    assert_eq!(word(out, "before"), "401c000000000000");
    assert!(binding(out, "wrong").is_none());
    assert!(binding(out, "after").is_none());
    assert_eq!(out["completed_leaf_statement_count"], 1);
    c.no_octave();
}
