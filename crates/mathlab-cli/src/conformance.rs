// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared original fixtures, three distinct execution routes and typed evidence.
//! A missing reference is incomplete evidence, never a successful comparison.

use anyhow::{bail, ensure, Context, Result};
use mathlab_engine::{Engine, EngineConfig};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

mod reference_bundle;

const MAX_FILE: u64 = 4 * 1024 * 1024;
const MAX_REPORT: u64 = 16 * 1024 * 1024;
const MAX_LOG: usize = 64 * 1024;

pub(crate) struct Options<'a> {
    pub manifest: &'a Path,
    pub octave: Option<&'a Path>,
    pub matlab: Option<&'a Path>,
    pub reference: Option<&'a Path>,
    pub output: Option<&'a Path>,
    pub directory: &'a Path,
    pub as_json: bool,
    pub strict: bool,
    pub timeout: Duration,
}

#[derive(Deserialize)]
struct Manifest {
    schema_version: u32,
    profile: Profile,
    support: PathBuf,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Profile {
    octave_version: String,
    matlab_release: String,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    fixture: PathBuf,
    oracle: PathBuf,
    expected_outcome: String,
    #[serde(default)]
    expected_error_identifier: Option<String>,
    float_absolute_tolerance: f64,
    float_relative_tolerance: f64,
    #[serde(default)]
    raw_octave_unsupported_identifiers: Vec<String>,
}

#[derive(Deserialize)]
struct Record {
    schema_version: u32,
    case_id: String,
    outcome: String,
    values: Vec<TypedValue>,
    error: RecordError,
}

#[derive(Deserialize)]
struct RecordError {
    identifier: String,
    message: String,
}

#[derive(Deserialize)]
struct TypedValue {
    name: String,
    class: String,
    shape: Vec<u64>,
    storage: String,
    complex: bool,
    real: Vec<String>,
    imag: Vec<String>,
    rows: Vec<u64>,
    columns: Vec<u64>,
}

fn open_regular(path: &Path) -> Result<File> {
    let mut options = File::options();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Opening a FIFO must not block before the regular-file check.
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .with_context(|| format!("Read {}", path.display()))?;
    ensure!(
        file.metadata()?.is_file(),
        "Expected a regular evidence file"
    );
    Ok(file)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    read_limit(path, MAX_FILE)
}

fn read_limit(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = open_regular(path)?;
    ensure!(
        file.metadata()?.len() <= limit,
        "Evidence file exceeds {limit} bytes"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Evidence file exceeds {limit} bytes"
    );
    Ok(bytes)
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = open_regular(path)?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 8192];
    loop {
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn adapter_file(path: Option<&Path>) -> Result<Value> {
    path.map(|path| -> Result<Value> {
        Ok(json!({"path":path.canonicalize()?,"sha256":hash_file(path)?,"identity_scope":"file sampled after runtime activation; not native build provenance"}))
    }).transpose().map(|value| value.unwrap_or(Value::Null))
}

fn encode_artifact(bytes: &[u8]) -> Result<Value> {
    ensure!(
        bytes.len() <= 1024 * 1024,
        "MAT reference artifact exceeds 1 MiB"
    );
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(json!({"encoding":"hex","sha256":hash(bytes),"bytes":bytes.len(),"data":hex}))
}

fn decode_artifact(artifact: &Value) -> Result<Vec<u8>> {
    ensure!(
        artifact["encoding"] == "hex",
        "Unsupported MAT artifact encoding"
    );
    let text = artifact["data"]
        .as_str()
        .context("MAT artifact bytes missing")?;
    ensure!(
        text.len() <= 2 * 1024 * 1024
            && text.len() % 2 == 0
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Invalid/bounded MAT artifact bytes"
    );
    let bytes = (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        artifact["sha256"] == hash(&bytes) && artifact["bytes"] == bytes.len(),
        "MAT artifact identity mismatch"
    );
    Ok(bytes)
}

fn literal(text: &str) -> Result<String> {
    ensure!(
        !text.contains(['\0', '\r', '\n']),
        "Conformance paths/IDs cannot contain NUL or line breaks"
    );
    Ok(format!("'{}'", text.replace('\'', "''")))
}

fn path_literal(path: &Path) -> Result<String> {
    literal(path.to_str().context("Conformance paths must be UTF-8")?)
}

fn unavailable(status: &str, detail: impl ToString) -> Value {
    json!({"status":status,"mode":"fresh","runtime":null,"record":null,"detail":detail.to_string()})
}

fn record_status(record: &Value, case: &Case, raw: bool) -> Result<String> {
    let parsed: Record =
        serde_json::from_value(record.clone()).context("Invalid typed fixture record")?;
    validate_record(&parsed, case)?;
    if parsed.outcome == case.expected_outcome {
        if parsed.outcome == "success" {
            return Ok("success".into());
        }
        if case.expected_error_identifier.as_deref() == Some(&parsed.error.identifier) {
            return Ok("expected_error".into());
        }
    }
    // The pinned fixture's intended forward local call is the only accepted
    // undefined-function boundary. A missing writer/setup helper is a failure.
    if raw
        && parsed.outcome == "error"
        && case
            .raw_octave_unsupported_identifiers
            .contains(&parsed.error.identifier)
        && case.id == "closure_forward_local"
        && parsed.error.message.contains("'make_offsetter'")
    {
        return Ok("unsupported".into());
    }
    Ok("error".into())
}

fn validate_record(record: &Record, case: &Case) -> Result<()> {
    ensure!(
        record.schema_version == 1 && record.case_id == case.id,
        "Wrong typed record version or case identity"
    );
    ensure!(
        matches!(record.outcome.as_str(), "success" | "error"),
        "Invalid fixture outcome"
    );
    ensure!(
        record.outcome != "success"
            || (record.error.identifier.is_empty() && record.error.message.is_empty()),
        "Success record contains an error"
    );
    let mut names = BTreeSet::new();
    for value in &record.values {
        ensure!(
            !value.name.is_empty() && names.insert(&value.name),
            "Duplicate/empty value name"
        );
        ensure!(
            (2..=32).contains(&value.shape.len()),
            "Value rank is outside the fixture contract"
        );
        let count = value
            .shape
            .iter()
            .try_fold(1u64, |n, d| n.checked_mul(*d))
            .context("Value shape overflow")?;
        ensure!(
            count <= 1_000_000,
            "Fixture value exceeds one million elements"
        );
        ensure!(
            matches!(
                value.class.as_str(),
                "double"
                    | "single"
                    | "int8"
                    | "uint8"
                    | "int16"
                    | "uint16"
                    | "int32"
                    | "uint32"
                    | "int64"
                    | "uint64"
                    | "logical"
            ),
            "Unsupported typed fixture class"
        );
        ensure!(
            !value.complex || matches!(value.class.as_str(), "single" | "double"),
            "Invalid complex integer/logical record"
        );
        ensure!(
            value.imag.len() == if value.complex { value.real.len() } else { 0 },
            "Invalid imaginary component count"
        );
        match value.storage.as_str() {
            "dense" => ensure!(
                value.real.len() as u64 == count
                    && value.rows.is_empty()
                    && value.columns.is_empty(),
                "Dense shape/component mismatch"
            ),
            "sparse" => {
                ensure!(
                    value.shape.len() == 2
                        && value.rows.len() == value.real.len()
                        && value.columns.len() == value.real.len(),
                    "Sparse coordinate/component mismatch"
                );
                let mut previous = None;
                for (&row, &column) in value.rows.iter().zip(&value.columns) {
                    ensure!(
                        row > 0 && row <= value.shape[0] && column > 0 && column <= value.shape[1],
                        "Sparse index out of bounds"
                    );
                    ensure!(
                        previous.is_none_or(|p| p < (column, row)),
                        "Sparse coordinates must be unique column-major entries"
                    );
                    previous = Some((column, row));
                }
            }
            _ => bail!("Unsupported fixture storage"),
        }
        for text in value.real.iter().chain(&value.imag) {
            if value.class.starts_with("uint") {
                let number: u64 = text.parse().context("Invalid exact unsigned integer")?;
                let bits: u32 = value.class[4..].parse()?;
                ensure!(
                    number.to_string() == *text && (bits == 64 || number < (1u64 << bits)),
                    "Unsigned integer out of range/noncanonical"
                );
            } else if value.class.starts_with("int") {
                let number: i64 = text.parse().context("Invalid exact signed integer")?;
                let bits: u32 = value.class[3..].parse()?;
                ensure!(
                    number.to_string() == *text
                        && (bits == 64
                            || (number >= -(1i64 << (bits - 1)) && number < (1i64 << (bits - 1)))),
                    "Signed integer out of range/noncanonical"
                );
            } else if value.class == "logical" {
                ensure!(
                    matches!(text.as_str(), "0" | "1"),
                    "Invalid logical component"
                );
            } else {
                parse_float(text)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case() -> Case {
        Case {
            id: "probe".into(),
            fixture: "probe.m".into(),
            oracle: "oracle.json".into(),
            expected_outcome: "success".into(),
            expected_error_identifier: None,
            float_absolute_tolerance: 0.0,
            float_relative_tolerance: 0.0,
            raw_octave_unsupported_identifiers: Vec::new(),
        }
    }

    fn record() -> Value {
        json!({"schema_version":1,"case_id":"probe","outcome":"success","error":{"identifier":"","message":""},"values":[
            {"name":"exact","class":"uint64","shape":[1,1],"storage":"dense","complex":false,"real":["9007199254741009"],"imag":[],"rows":[],"columns":[]},
            {"name":"z","class":"double","shape":[1,4],"storage":"dense","complex":true,"real":["NaN","Inf","-Inf","-0"],"imag":["1","2","3","4"],"rows":[],"columns":[]},
            {"name":"s","class":"double","shape":[3,4],"storage":"sparse","complex":true,"real":["2","-1"],"imag":["3","4"],"rows":[1,3],"columns":[2,4]}
        ]})
    }

    #[test]
    fn exact_integer_low_bits_and_typed_variants_cannot_compare_as_equal() {
        let original = record();
        assert_eq!(
            compare(&original, &original, &case()).unwrap()["status"],
            "match"
        );
        for (pointer, replacement) in [
            ("/values/0/real/0", json!("9007199254741008")),
            ("/values/1/class", json!("single")),
            ("/values/1/shape", json!([2, 2])),
            ("/values/1/imag/2", json!("9")),
            ("/values/1/real/0", json!("0")),
            ("/values/1/real/1", json!("-Inf")),
            ("/values/2/rows/0", json!(2)),
            ("/values", json!([])),
        ] {
            let mut changed = original.clone();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            let comparison = compare(&original, &changed, &case()).unwrap();
            assert_eq!(comparison["status"], "mismatch", "{pointer}");
            assert!(!comparison["differences"].as_array().unwrap().is_empty());
        }
    }

    #[test]
    fn invalid_sparse_and_integer_records_fail_validation() {
        for (pointer, replacement) in [
            ("/values/0/real/0", json!("18446744073709551616")),
            ("/values/0/real/0", json!("09007199254741009")),
            ("/values/2/rows/0", json!(0)),
            ("/values/2/columns/1", json!(5)),
            ("/values/1/imag", json!([])),
        ] {
            let mut changed = record();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            let parsed: Record = serde_json::from_value(changed).unwrap();
            assert!(validate_record(&parsed, &case()).is_err(), "{pointer}");
        }
    }

    #[test]
    fn floating_tolerance_and_nonfinite_policy_are_explicit() {
        let mut a = record();
        a["values"][1]["real"][3] = json!("1");
        let mut b = a.clone();
        b["values"][1]["real"][3] = json!("1.0000001");
        assert_eq!(compare(&a, &b, &case()).unwrap()["status"], "mismatch");
        let mut policy = case();
        policy.float_absolute_tolerance = 0.000001;
        assert_eq!(compare(&a, &b, &policy).unwrap()["status"], "match");
        b["values"][1]["real"][0] = json!("Inf");
        assert_eq!(compare(&a, &b, &policy).unwrap()["status"], "mismatch");
    }

    #[cfg(unix)]
    #[test]
    fn standalone_runtime_reaps_owned_descendants_on_exit_and_timeout() {
        for (script, timeout, expected_timeout) in [
            ("sleep 60 & printf done", Duration::from_secs(2), false),
            ("sleep 60 & wait", Duration::from_millis(100), true),
        ] {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", script]);
            let start = Instant::now();
            let result = process(&mut command, timeout).unwrap();
            assert_eq!(result["timed_out"], expected_timeout);
            assert_eq!(result["capture_incomplete"], false);
            assert_eq!(result["cleanup_policy"], "owned_process_group");
            assert!(start.elapsed() < Duration::from_secs(3));
        }
    }

    #[cfg(unix)]
    #[test]
    fn evidence_fifo_is_rejected_without_waiting_for_a_writer() {
        use std::os::unix::ffi::OsStrExt;
        let directory = tempfile::tempdir().unwrap();
        let fifo = directory.path().join("evidence");
        let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(read_bounded(&fifo)
            .unwrap_err()
            .to_string()
            .contains("regular"));
    }

    #[test]
    fn mat_artifact_hash_and_bytes_are_checked_before_replay() {
        let original = encode_artifact(b"original MAT bytes").unwrap();
        assert_eq!(decode_artifact(&original).unwrap(), b"original MAT bytes");
        for (field, value) in [
            ("sha256", json!("changed")),
            ("bytes", json!(999)),
            ("data", json!("ff")),
        ] {
            let mut changed = original.clone();
            changed[field] = value;
            assert!(decode_artifact(&changed).is_err());
        }
    }

    #[test]
    fn replay_rejects_contradictory_or_missing_process_completion() {
        let case = case();
        let fixture = Fixture {
            case: &case,
            path: "probe.m".into(),
            sha: "fixture".into(),
            oracle_sha: "oracle".into(),
            support: Path::new("support"),
            profile: &Profile {
                octave_version: "11.3.0".into(),
                matlab_release: "R2026b".into(),
            },
        };
        let report = json!({"schema_version":1,"kind":"mathlab-conformance","manifest_sha256":"manifest","support_sha256":"support","profile":{"octave_version":"11.3.0","matlab_release":"R2026b"},"cases":[{"id":"probe","fixture_sha256":"fixture","oracle_sha256":"oracle","routes":{"matlab":{"status":"success","mode":"fresh","fixture_sha256":"fixture","runtime":{"name":"MATLAB","release":"R2026b","version":"test-only","executable":"test-only"},"record":record(),"process":{"success":true,"timed_out":false,"capture_incomplete":false}}}}]});
        assert!(saved_reference(&report, &fixture, "manifest", "support").is_ok());
        for (field, value) in [
            ("success", json!(false)),
            ("timed_out", json!(true)),
            ("capture_incomplete", json!(true)),
            ("success", Value::Null),
        ] {
            let mut changed = report.clone();
            changed["cases"][0]["routes"]["matlab"]["process"][field] = value;
            assert!(saved_reference(&changed, &fixture, "manifest", "support").is_err());
        }
    }

    #[test]
    fn diagnostic_reader_retries_interruption_and_records_io_failure() {
        struct Reader(bool);
        impl Read for Reader {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                if !self.0 {
                    self.0 = true;
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
        }
        let (log, task) = drain(Reader(false));
        task.join().unwrap();
        let log = log.lock().unwrap();
        assert!(!log.eof);
        assert!(log.error.is_some());
    }
}

fn parse_float(text: &str) -> Result<f64> {
    match text {
        "NaN" => Ok(f64::NAN),
        "Inf" => Ok(f64::INFINITY),
        "-Inf" => Ok(f64::NEG_INFINITY),
        _ => {
            let n: f64 = text.parse().context("Invalid floating component")?;
            ensure!(
                n.is_finite(),
                "Nonfinite components need explicit NaN/Inf spelling"
            );
            Ok(n)
        }
    }
}

fn compare(expected: &Value, actual: &Value, case: &Case) -> Result<Value> {
    let a: Record = serde_json::from_value(expected.clone())?;
    let b: Record = serde_json::from_value(actual.clone())?;
    validate_record(&a, case)?;
    validate_record(&b, case)?;
    let mut differences = Vec::new();
    let mut difference_count = 0usize;
    let mut exact = |path: String, expected: Value, actual: Value| {
        if expected != actual {
            difference_count += 1;
            if differences.len() < 256 {
                differences.push(json!({"path":path,"expected":expected,"actual":actual}));
            }
        }
    };
    exact("outcome".into(), json!(a.outcome), json!(b.outcome));
    exact(
        "error.identifier".into(),
        json!(a.error.identifier),
        json!(b.error.identifier),
    );
    let names_a: BTreeSet<_> = a.values.iter().map(|v| &v.name).collect();
    let names_b: BTreeSet<_> = b.values.iter().map(|v| &v.name).collect();
    exact("values.names".into(), json!(names_a), json!(names_b));
    for av in &a.values {
        let Some(bv) = b.values.iter().find(|v| v.name == av.name) else {
            continue;
        };
        let prefix = format!("values.{}", av.name);
        exact(format!("{prefix}.class"), json!(av.class), json!(bv.class));
        exact(format!("{prefix}.shape"), json!(av.shape), json!(bv.shape));
        exact(
            format!("{prefix}.storage"),
            json!(av.storage),
            json!(bv.storage),
        );
        exact(
            format!("{prefix}.complex"),
            json!(av.complex),
            json!(bv.complex),
        );
        exact(format!("{prefix}.rows"), json!(av.rows), json!(bv.rows));
        exact(
            format!("{prefix}.columns"),
            json!(av.columns),
            json!(bv.columns),
        );
        for (part, left, right) in [("real", &av.real, &bv.real), ("imag", &av.imag, &bv.imag)] {
            exact(
                format!("{prefix}.{part}.length"),
                json!(left.len()),
                json!(right.len()),
            );
            for (i, (x, y)) in left.iter().zip(right).enumerate() {
                let equal =
                    if av.class == bv.class && matches!(av.class.as_str(), "double" | "single") {
                        let x = parse_float(x)?;
                        let y = parse_float(y)?;
                        (x.is_nan() && y.is_nan())
                            || x == y
                            || (x.is_finite()
                                && y.is_finite()
                                && ((x - y).abs() <= case.float_absolute_tolerance
                                    || (x - y).abs() / x.abs().max(y.abs())
                                        <= case.float_relative_tolerance))
                    } else {
                        x == y
                    };
                if !equal {
                    exact(format!("{prefix}.{part}[{i}]"), json!(x), json!(y));
                }
            }
        }
    }
    Ok(
        json!({"status":if difference_count==0{"match"}else{"mismatch"},"differences":differences,"omitted_differences":difference_count.saturating_sub(256)}),
    )
}

fn comparison(reference: &Value, actual: &Value, case: &Case) -> Value {
    for route in [reference, actual] {
        let status = route["status"].as_str().unwrap_or("invalid_reference");
        if !matches!(status, "success" | "expected_error") {
            return json!({"status":match status {"unavailable"|"timeout"=>"unavailable","unsupported"=>"unsupported","invalid_reference"=>"invalid_reference",_=>"not_run"},"differences":[]});
        }
    }
    compare(&reference["record"], &actual["record"], case).unwrap_or_else(
        |e| json!({"status":"invalid_reference","detail":e.to_string(),"differences":[]}),
    )
}

#[derive(Default)]
struct Log {
    bytes: Vec<u8>,
    dropped: u64,
    eof: bool,
    error: Option<String>,
}

fn drain(reader: impl Read + Send + 'static) -> (Arc<Mutex<Log>>, thread::JoinHandle<()>) {
    let log = Arc::new(Mutex::new(Log::default()));
    let copy = log.clone();
    let task = thread::spawn(move || {
        let mut reader = reader;
        let mut bytes = [0u8; 8192];
        loop {
            let count = match reader.read(&mut bytes) {
                Ok(0) => {
                    if let Ok(mut log) = copy.lock() {
                        log.eof = true;
                    }
                    break;
                }
                Ok(count) => count,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    if let Ok(mut log) = copy.lock() {
                        log.error = Some(error.to_string());
                    }
                    break;
                }
            };
            let Ok(mut log) = copy.lock() else { break };
            let keep = count.min(MAX_LOG.saturating_sub(log.bytes.len()));
            log.bytes.extend_from_slice(&bytes[..keep]);
            log.dropped += (count - keep) as u64;
        }
    });
    (log, task)
}

struct OwnedProcess(Child, bool);

impl OwnedProcess {
    fn terminate(&mut self) {
        if !self.1 {
            return;
        }
        self.1 = false;
        #[cfg(unix)]
        {
            // Every standalone route starts its own group. Reap/kill only that
            // owned group, including background children retaining log pipes.
            let group = self.0.id() as libc::pid_t;
            unsafe { libc::kill(-group, libc::SIGKILL) };
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn process(command: &mut Command, timeout: Duration) -> Result<Value> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .context("Runtime deadline overflow")?;
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = OwnedProcess(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?,
        true,
    );
    let (out, ot) = drain(child.0.stdout.take().unwrap());
    let (err, et) = drain(child.0.stderr.take().unwrap());
    let (status, timed_out) = loop {
        if let Some(status) = child.0.try_wait()? {
            break (status, false);
        }
        if Instant::now() >= deadline {
            child.terminate();
            break (child.0.wait()?, true);
        }
        thread::sleep(Duration::from_millis(10));
    };
    // Also close inherited pipes held by children after a normal parent exit.
    child.terminate();
    let until = Instant::now() + Duration::from_millis(100);
    while !(ot.is_finished() && et.is_finished()) && Instant::now() < until {
        thread::sleep(Duration::from_millis(5));
    }
    let incomplete = !(ot.is_finished() && et.is_finished());
    if ot.is_finished() {
        let _ = ot.join();
    }
    if et.is_finished() {
        let _ = et.join();
    }
    let out = out
        .lock()
        .map_err(|_| anyhow::anyhow!("stdout capture poisoned"))?;
    let err = err
        .lock()
        .map_err(|_| anyhow::anyhow!("stderr capture poisoned"))?;
    let incomplete = incomplete || !out.eof || !err.eof;
    Ok(
        json!({"success":status.success(),"exit_code":status.code(),"timed_out":timed_out,
        "stdout":String::from_utf8_lossy(&out.bytes),"stderr":String::from_utf8_lossy(&err.bytes),
        "stdout_dropped_bytes":out.dropped,"stderr_dropped_bytes":err.dropped,"stdout_read_error":out.error,"stderr_read_error":err.error,"capture_incomplete":incomplete,
        "cleanup_policy":if cfg!(unix){"owned_process_group"}else{"immediate_child_only"}}),
    )
}

struct Fixture<'a> {
    case: &'a Case,
    path: PathBuf,
    sha: String,
    oracle_sha: String,
    support: &'a Path,
    profile: &'a Profile,
}

fn native_route(
    executable: &Path,
    matlab: bool,
    fixture: &Fixture<'_>,
    directory: &Path,
    timeout: Duration,
    mat_input: Option<&Path>,
) -> Result<Value> {
    let output = directory.join("record.json");
    let runtime = directory.join("runtime.json");
    let mut setup=format!("addpath({});mathlab_conformance_work_directory={};result=struct();mlcf_id='';mlcf_message='';",path_literal(fixture.support)?,path_literal(directory)?);
    if let Some(path) = mat_input {
        setup += &format!("mathlab_conformance_mat_input={};", path_literal(path)?);
    }
    let invoke = if matlab { "run" } else { "source" };
    let runtime_code = if matlab {
        "struct('name','MATLAB','version',version,'release',['R',version('-release')])"
    } else {
        "struct('name','GNU Octave','version',version)"
    };
    // Identify a successfully started interpreter before fixture/writer code.
    // A fixture crash or writer failure must not become an unavailable runtime.
    let code=format!("{setup}mlcf_f=fopen({},'w');fprintf(mlcf_f,'%s',jsonencode({runtime_code}));fclose(mlcf_f);try;{invoke}({});catch caught;mlcf_id=caught.identifier;mlcf_message=caught.message;end;mathlab_conformance_write({},{},result,mlcf_id,mlcf_message);",path_literal(&runtime)?,path_literal(&fixture.path)?,path_literal(&output)?,literal(&fixture.case.id)?);
    let mut command = Command::new(executable);
    command.current_dir(directory);
    if matlab {
        command
            .args(["-batch", &code])
            .env("MATLAB_PREFDIR", directory.join("preferences"));
    } else {
        command.args([
            "--quiet",
            "--no-gui",
            "--no-history",
            "--no-init-file",
            "--no-site-file",
            "--eval",
            &code,
        ]);
    }
    let process = process(&mut command, timeout)?;
    if process["timed_out"] == true {
        let mut r = unavailable("timeout", "Runtime deadline expired");
        r["process"] = process;
        return Ok(r);
    }
    if process["success"] != true
        || process["capture_incomplete"] == true
        || !output.is_file()
        || !runtime.is_file()
    {
        let mut r = unavailable(
            if runtime.is_file() {
                "error"
            } else {
                "unavailable"
            },
            "Runtime did not produce a complete successful fixture execution record",
        );
        r["process"] = process;
        return Ok(r);
    }
    let record: Value = serde_json::from_slice(&read_bounded(&output)?)?;
    let mut identity: Value = serde_json::from_slice(&read_bounded(&runtime)?)?;
    identity["executable"] = json!(executable);
    let pinned = if matlab {
        identity["release"] == fixture.profile.matlab_release
    } else {
        identity["version"] == fixture.profile.octave_version
    };
    let status = if pinned {
        record_status(&record, fixture.case, !matlab)?
    } else {
        "unsupported".into()
    };
    Ok(
        json!({"status":status,"mode":"fresh","runtime":identity,"record":record,"fixture_sha256":fixture.sha,"process":process,"profile_matches":pinned}),
    )
}

fn engine_route(
    executable: &Path,
    fixture: &Fixture<'_>,
    directory: &Path,
    timeout: Duration,
    mat_input: Option<&Path>,
) -> Result<Value> {
    let mut engine = match Engine::start(EngineConfig {
        executable: executable.into(),
        working_directory: directory.into(),
    }) {
        Err(mathlab_engine::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(unavailable("unavailable", e))
        }
        other => other?,
    };
    let scope_adapter_file = adapter_file(engine.scope_bridge_path())?;
    let input_adapter_file = adapter_file(engine.input_bridge_path())?;
    let mut setup = format!(
        "addpath({});mathlab_conformance_work_directory={};result=struct();",
        path_literal(fixture.support)?,
        path_literal(directory)?
    );
    if let Some(path) = mat_input {
        setup += &format!("mathlab_conformance_mat_input={};", path_literal(path)?);
    }
    let result = engine.execute_with_timeout(&setup, timeout)?;
    ensure!(
        result.error.is_none(),
        "Engine fixture setup failed: {:?}",
        result.error
    );
    let result = engine.run_file_with_timeout(&fixture.path, timeout)?;
    let payload = directory.join("export.json");
    let output = directory.join("record.json");
    fs::write(
        &payload,
        serde_json::to_vec(
            &json!({"identifier":result.error.as_ref().map_or("",|e|&e.identifier),"message":result.error.as_ref().map_or("",|e|&e.message)}),
        )?,
    )?;
    let code=format!("mlcf_export=jsondecode(fileread({}));mathlab_conformance_write({},{},result,mlcf_export.identifier,mlcf_export.message);",path_literal(&payload)?,path_literal(&output)?,literal(&fixture.case.id)?);
    let export = engine.execute_with_timeout(&code, timeout)?;
    ensure!(
        export.error.is_none(),
        "Engine record export failed: {:?}",
        export.error
    );
    let record: Value = serde_json::from_slice(&read_bounded(&output)?)?;
    let pinned = engine.version() == fixture.profile.octave_version;
    let status = if pinned {
        record_status(&record, fixture.case, false)?
    } else {
        "unsupported".into()
    };
    let output_end = (0..=result.output.len().min(MAX_LOG))
        .rev()
        .find(|&end| result.output.is_char_boundary(end))
        .unwrap();
    let output_dropped = result.output.len() - output_end;
    Ok(
        json!({"status":status,"mode":"fresh","runtime":{"name":"MathLab/GNU Octave","version":engine.version(),"executable":executable,
        "scope_adapter":engine.scope_bridge_path(),"input_adapter":engine.input_bridge_path(),"scope_adapter_file":scope_adapter_file,"input_adapter_file":input_adapter_file},"record":record,"fixture_sha256":fixture.sha,"profile_matches":pinned,
        "execution":{"output":&result.output[..output_end],"diagnostic_dropped_bytes":output_dropped,"output_truncated":result.output_truncated || output_dropped > 0,"error":result.error}}),
    )
}

fn saved_reference(
    report: &Value,
    fixture: &Fixture<'_>,
    manifest_sha: &str,
    support_sha: &str,
) -> Result<Value> {
    ensure!(
        report.get("synthetic_test_fixture").is_none(),
        "Synthetic test data is not an authentic MATLAB reference"
    );
    ensure!(
        report["schema_version"] == 1 && report["kind"] == "mathlab-conformance",
        "Unsupported reference report schema/kind"
    );
    ensure!(
        report["manifest_sha256"] == manifest_sha && report["support_sha256"] == support_sha,
        "Reference manifest/support hash mismatch"
    );
    ensure!(
        report["profile"]["octave_version"] == fixture.profile.octave_version
            && report["profile"]["matlab_release"] == fixture.profile.matlab_release,
        "Reference profile mismatch"
    );
    let cases = report["cases"]
        .as_array()
        .context("Reference cases missing")?;
    let matching: Vec<_> = cases
        .iter()
        .filter(|c| c["id"] == fixture.case.id)
        .collect();
    ensure!(matching.len() == 1, "Missing/duplicate reference case");
    let case = matching[0];
    ensure!(
        case["fixture_sha256"] == fixture.sha,
        "Reference fixture hash mismatch"
    );
    ensure!(
        case["oracle_sha256"] == fixture.oracle_sha,
        "Reference oracle hash mismatch"
    );
    let mut route = case["routes"]["matlab"].clone();
    ensure!(
        matches!(route["status"].as_str(), Some("success" | "expected_error")),
        "Saved reference was not an executed successful/expected-error outcome"
    );
    ensure!(
        route["fixture_sha256"] == fixture.sha,
        "Reference route fixture hash missing/mismatched"
    );
    let runtime = &route["runtime"];
    ensure!(
        runtime["name"] == "MATLAB"
            && runtime["release"] == fixture.profile.matlab_release
            && runtime["version"].as_str().is_some_and(|s| !s.is_empty())
            && runtime["executable"]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
        "Reference runtime provenance missing or outside pinned release"
    );
    ensure!(
        matches!(route["mode"].as_str(), Some("fresh" | "saved_reference")),
        "Reference execution mode missing"
    );
    ensure!(
        route["process"]["success"] == true && route["process"]["timed_out"] == false,
        "Reference process did not complete successfully without timeout"
    );
    if route["reference_origin"].is_null() {
        ensure!(
            route["process"]["success"] == true
                && route["process"]["timed_out"] == false
                && route["process"]["capture_incomplete"] == false,
            "Reference process did not complete successfully with complete capture"
        );
    } else {
        ensure!(
            route["mode"] == "saved_reference"
                && route["reference_format"] == "independent_fresh_licensed_CP01_reference_bundle"
                && route["reference_origin"]["source_report_sha256"].is_string(),
            "Imported reference must preserve its independent producer and source-report identity"
        );
        reference_bundle::validate_origin(&route["reference_origin"])?;
    }
    let status = record_status(&route["record"], fixture.case, false)?;
    ensure!(
        matches!(status.as_str(), "success" | "expected_error"),
        "Reference fixture did not meet its original expected outcome"
    );
    if !route["mat_artifact"].is_null() {
        decode_artifact(&route["mat_artifact"])?;
    }
    route["status"] = json!(status);
    route["mode"] = json!("saved_reference");
    Ok(route)
}

fn validate_reference_cases(report: &Value, manifest: &Manifest) -> Result<()> {
    let cases = report["cases"]
        .as_array()
        .context("Reference cases missing")?;
    let mut observed = BTreeSet::new();
    for case in cases {
        let id = case["id"].as_str().context("Reference case ID missing")?;
        ensure!(observed.insert(id), "Duplicate reference case ID");
    }
    let expected: BTreeSet<_> = manifest.cases.iter().map(|c| c.id.as_str()).collect();
    ensure!(
        observed == expected,
        "Reference case set differs from manifest"
    );
    Ok(())
}

fn route_error(error: anyhow::Error) -> Value {
    let status = if error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
    {
        "unavailable"
    } else {
        "error"
    };
    unavailable(status, error)
}

pub(crate) fn run(options: Options<'_>) -> Result<()> {
    Instant::now()
        .checked_add(options.timeout)
        .context("Conformance deadline overflow")?;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&repo)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    let executable = std::env::current_exe()?;
    let executable_sha = hash_file(&executable)?;
    let path = options
        .manifest
        .canonicalize()
        .context("Conformance manifest not found")?;
    let bytes = read_bounded(&path)?;
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    ensure!(
        manifest.schema_version == 1 && !manifest.cases.is_empty() && manifest.cases.len() <= 64,
        "Unsupported/empty conformance manifest"
    );
    // Reject every invalid policy before starting any fixture or runtime.
    let mut ids = BTreeSet::new();
    for case in &manifest.cases {
        ensure!(
            !case.id.is_empty()
                && case
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && ids.insert(&case.id),
            "Invalid/duplicate case ID"
        );
        ensure!(
            matches!(case.expected_outcome.as_str(), "success" | "error")
                && (case.expected_outcome != "error"
                    || case
                        .expected_error_identifier
                        .as_ref()
                        .is_some_and(|id| !id.is_empty())),
            "Missing expected outcome/error policy"
        );
        ensure!(
            case.float_absolute_tolerance.is_finite()
                && case.float_absolute_tolerance >= 0.0
                && case.float_relative_tolerance.is_finite()
                && case.float_relative_tolerance >= 0.0,
            "Invalid comparison tolerance"
        );
    }
    let root = path.parent().unwrap();
    let support_source = root.join(&manifest.support).canonicalize()?;
    let support_bytes = read_bounded(&support_source.join("mathlab_conformance_write.m"))?;
    let support_sha = hash(&support_bytes);
    let manifest_sha = hash(&bytes);
    let saved_bytes = options
        .reference
        .map(|p| read_limit(p, MAX_REPORT))
        .transpose()?;
    let saved: Option<Value> = saved_bytes
        .as_deref()
        .map(serde_json::from_slice)
        .transpose()?;
    let saved_sha = saved_bytes.as_ref().map(|bytes| hash(bytes));
    let saved_case_error = saved.as_ref().and_then(|report| {
        (if report["kind"] == "independent_fresh_licensed_CP01_reference_bundle" {
            reference_bundle::validate_cases(report, &manifest)
        } else {
            validate_reference_cases(report, &manifest)
        })
        .err()
        .map(|e| e.to_string())
    });
    let mut cases = Vec::new();
    let mut failed = false;
    let mut incomplete = false;
    let mut matches = 0;
    let mut mismatches = 0;
    let mut oracle_mismatches = 0;
    let stage = tempfile::Builder::new()
        .prefix("mathlab-conformance-")
        .tempdir()?;
    // Own immutable copies before any route executes. The producer paths may
    // change concurrently; every backend sees these same captured bytes.
    let support = stage.path().join("support");
    fs::create_dir(&support)?;
    fs::write(support.join("mathlab_conformance_write.m"), &support_bytes)?;
    let inputs = stage.path().join("inputs");
    fs::create_dir(&inputs)?;
    let mut staged = Vec::new();
    for case in &manifest.cases {
        let original = root.join(&case.fixture).canonicalize()?;
        let bytes = read_bounded(&original)?;
        let oracle_bytes = read_bounded(&root.join(&case.oracle))?;
        let oracle: Value = serde_json::from_slice(&oracle_bytes)?;
        let parsed: Record = serde_json::from_value(oracle.clone())?;
        validate_record(&parsed, case)?;
        ensure!(
            parsed.outcome == case.expected_outcome
                && (parsed.outcome != "error"
                    || case.expected_error_identifier.as_deref() == Some(&parsed.error.identifier)),
            "Oracle disagrees with fixture outcome policy"
        );
        let folder = inputs.join(format!("case-{}", staged.len()));
        fs::create_dir(&folder)?;
        let path = folder.join(original.file_name().context("Fixture filename missing")?);
        fs::write(&path, &bytes)?;
        staged.push((path, hash(&bytes), oracle, hash(&oracle_bytes)));
    }
    for (case, (fixture_path, fixture_sha, oracle, oracle_sha)) in
        manifest.cases.iter().zip(&staged)
    {
        let fixture = Fixture {
            case,
            path: fixture_path.clone(),
            sha: fixture_sha.clone(),
            oracle_sha: oracle_sha.clone(),
            support: &support,
            profile: &manifest.profile,
        };
        let case_dir = stage.path().join(&case.id);
        fs::create_dir(&case_dir)?;
        let reference_dir = case_dir.join("matlab");
        fs::create_dir(&reference_dir)?;
        let mut matlab = if let Some(exe) = options.matlab {
            native_route(exe, true, &fixture, &reference_dir, options.timeout, None)
                .unwrap_or_else(route_error)
        } else if let Some(saved) = &saved {
            if let Some(error) = &saved_case_error {
                unavailable("invalid_reference", error)
            } else if saved["kind"] == "independent_fresh_licensed_CP01_reference_bundle" {
                reference_bundle::import(
                    saved,
                    options
                        .reference
                        .unwrap()
                        .parent()
                        .unwrap_or(Path::new(".")),
                    root,
                    &fixture,
                    &manifest_sha,
                    &support_sha,
                )
                .map(|mut route| {
                    route["reference_origin"]["source_report_sha256"] = json!(saved_sha);
                    route
                })
                .unwrap_or_else(|e| unavailable("invalid_reference", e))
            } else {
                saved_reference(saved, &fixture, &manifest_sha, &support_sha)
                    .unwrap_or_else(|e| unavailable("invalid_reference", e))
            }
        } else {
            unavailable(
                "unavailable",
                "No explicit licensed MATLAB executable or saved reference supplied",
            )
        };
        let reference_mat = reference_dir.join("conformance-values.mat");
        if case.id == "mat_v7"
            && matches!(
                matlab["status"].as_str(),
                Some("success" | "expected_error")
            )
        {
            if options.matlab.is_some() && reference_mat.is_file() {
                matlab["mat_artifact"] = encode_artifact(&read_bounded(&reference_mat)?)?;
            } else if options.reference.is_some() && !matlab["mat_artifact"].is_null() {
                fs::write(&reference_mat, decode_artifact(&matlab["mat_artifact"])?)?;
            }
        }
        let mat_input = (case.id == "mat_v7"
            && matches!(
                matlab["status"].as_str(),
                Some("success" | "expected_error")
            )
            && reference_mat.is_file())
        .then_some(reference_mat.as_path());
        if case.id == "mat_v7" && mat_input.is_none() {
            incomplete = true;
        }
        let mut routes = json!({"matlab":matlab});
        for (route, is_engine) in [("octave", false), ("mathlab", true)] {
            let directory = case_dir.join(route);
            fs::create_dir(&directory)?;
            let result = options.octave.map_or_else(
                || Ok(unavailable("unavailable", "GNU Octave was not found")),
                |exe| {
                    if is_engine {
                        engine_route(exe, &fixture, &directory, options.timeout, mat_input)
                    } else {
                        native_route(exe, false, &fixture, &directory, options.timeout, mat_input)
                    }
                },
            );
            routes[route] = result.unwrap_or_else(route_error);
        }
        for route in ["matlab", "octave", "mathlab"] {
            let status = routes[route]["status"].as_str().unwrap_or("error");
            let oracle_result = if matches!(status, "success" | "expected_error" | "error")
                && !routes[route]["record"].is_null()
            {
                compare(oracle, &routes[route]["record"], case).unwrap_or_else(
                    |e| json!({"status":"mismatch","detail":e.to_string(),"differences":[]}),
                )
            } else {
                json!({"status":if status=="unsupported"{"unsupported"}else{"not_run"},"differences":[]})
            };
            if oracle_result["status"] == "mismatch" {
                failed = true;
                oracle_mismatches += 1;
            }
            routes[route]["oracle_comparison"] = oracle_result;
        }
        let comparisons = json!({"octave_to_reference":comparison(&routes["matlab"],&routes["octave"],case),
            "mathlab_to_reference":comparison(&routes["matlab"],&routes["mathlab"],case),
            "mathlab_to_octave":comparison(&routes["octave"],&routes["mathlab"],case)});
        for route in ["matlab", "octave", "mathlab"] {
            if matches!(
                routes[route]["status"].as_str(),
                Some("error" | "invalid_reference" | "timeout")
            ) {
                failed = true;
            }
        }
        for key in [
            "octave_to_reference",
            "mathlab_to_reference",
            "mathlab_to_octave",
        ] {
            match comparisons[key]["status"].as_str() {
                Some("match") => matches += 1,
                Some("mismatch") => {
                    mismatches += 1;
                    failed = true;
                }
                _ => incomplete = true,
            }
        }
        cases.push(json!({"id":case.id,"fixture":case.fixture,"fixture_sha256":fixture.sha,"oracle_sha256":oracle_sha,"routes":routes,"comparisons":comparisons,
            "comparison_policy":{"float_absolute_tolerance":case.float_absolute_tolerance,"float_relative_tolerance":case.float_relative_tolerance,"nan":"same positions","signed_zero":"numerically equal","error_message":"recorded, not compared"},
            "mat_file_policy":if case.id=="mat_v7"{if mat_input.is_some(){"MATLAB-written v7 bytes supplied to raw Octave and MathLab; route outcomes and oracle comparisons determine verified loading"}else{"independent route self-round-trips; no reference-file interchange claim"}}else{"not_applicable"}}));
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dirty = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=normal"])
        .current_dir(&repo)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty());
    let report = json!({"schema_version":1,"kind":"mathlab-conformance","repository_revision_at_start":revision,"working_tree_dirty_at_report":dirty,
        "binary_identity":{"executable":executable,"sha256":executable_sha,"build_revision":env!("MATHLAB_BUILD_REVISION"),"build_source_sha256":env!("MATHLAB_BUILD_SOURCE_SHA256"),"target":env!("MATHLAB_BUILD_TARGET"),"provenance":"Build revision and selected source fingerprint captured by Cargo build script; repository revision is not substituted for binary identity."},
        "manifest_sha256":manifest_sha,"support_sha256":support_sha,"profile":{"octave_version":manifest.profile.octave_version,"matlab_release":manifest.profile.matlab_release},
        "reference_source":options.reference,"reference_source_sha256":saved_sha,"working_directory":options.directory,"cases":cases,
        "summary":{"status":if failed{"mismatch"}else if incomplete{"incomplete"}else{"matched"},"matches":matches,"mismatches":mismatches,"oracle_mismatches":oracle_mismatches},
        "interpretation":"Only these original typed cases and declared variants were compared. Saved reference replay is not a fresh MATLAB run; unavailable/unsupported routes never count as matches. Reports are provenance records, not signed attestations."});
    let mut report_bytes = serde_json::to_vec_pretty(&report)?;
    report_bytes.push(b'\n');
    ensure!(
        report_bytes.len() as u64 <= MAX_REPORT,
        "Conformance report exceeds 16 MiB; split the manifest"
    );
    if let Some(path) = options.output {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&report_bytes)?;
        file.persist(path)?;
    }
    if options.as_json {
        std::io::stdout().lock().write_all(&report_bytes)?;
    } else {
        for case in report["cases"].as_array().unwrap() {
            println!(
                "{}: MATLAB={} Octave={} MathLab={}; reference comparison={}",
                case["id"].as_str().unwrap(),
                case["routes"]["matlab"]["status"].as_str().unwrap(),
                case["routes"]["octave"]["status"].as_str().unwrap(),
                case["routes"]["mathlab"]["status"].as_str().unwrap(),
                case["comparisons"]["mathlab_to_reference"]["status"]
                    .as_str()
                    .unwrap()
            );
        }
        println!(
            "Conformance report: {} ({} matching comparisons, {} mismatches).",
            report["summary"]["status"].as_str().unwrap(),
            matches,
            mismatches
        );
    }
    ensure!(
        !failed,
        "Conformance mismatch, invalid reference or execution failure; inspect the route records"
    );
    ensure!(
        !options.strict || !incomplete,
        "Strict conformance acceptance requires every comparison; report is incomplete"
    );
    Ok(())
}
