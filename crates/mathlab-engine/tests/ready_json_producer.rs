// SPDX-License-Identifier: GPL-3.0-or-later
//! Qualify the actual Octave producer directly against the strict Rust decoder.
//! No JSON Value/floating intermediate may repair or round emitted integers.

use mathlab_engine::{WorkerCapabilities, discover_octave};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::{path::Path, process::Command};

const PASS: &str = "PASS ready_json_probe explicit owner, canonical integers, escaping, dynamic fields, exact extrema, rejection and ordinary passthrough";

#[derive(Deserialize)]
struct RawEnvelope {
    id: u64,
    ok: bool,
    result: RawResult,
}

#[derive(Deserialize)]
struct RawResult {
    protocol: Box<RawValue>,
}

#[derive(Deserialize)]
struct RawIntegerProtocol {
    version: Box<RawValue>,
    limits: RawIntegerLimits,
}

#[derive(Deserialize)]
struct RawIntegerLimits {
    variable_cells: Box<RawValue>,
    response_bytes: Box<RawValue>,
}

#[derive(Deserialize)]
struct ExactExtremaProtocol {
    version: u32,
    limits: ExactExtremaLimits,
}

#[derive(Deserialize)]
struct ExactExtremaLimits {
    variable_cells: u64,
    response_bytes: u64,
}

fn octave_string(path: &Path) -> String {
    format!(
        "'{}'",
        path.to_str()
            .expect("probe source paths must be valid UTF-8")
            .replace('\'', "''")
    )
}

fn protocol_marker(stdout: &str, marker: &str) -> Box<RawValue> {
    let prefix = format!("{marker} ");
    let mut records = stdout
        .lines()
        .filter_map(|line| line.strip_prefix(prefix.as_str()));
    let raw = records
        .next()
        .unwrap_or_else(|| panic!("producer omitted {marker}: {stdout}"));
    assert!(
        records.next().is_none(),
        "producer duplicated {marker}: {stdout}"
    );
    let envelope: RawEnvelope = serde_json::from_str(raw)
        .unwrap_or_else(|error| panic!("invalid raw {marker} envelope: {error}: {raw}"));
    assert_eq!(envelope.id, 0, "{marker} must belong to the Ready probe");
    assert!(envelope.ok, "{marker} must be a successful producer result");
    envelope.result.protocol
}

fn assert_integer_tokens(protocol: &RawValue, expected: [&str; 3]) {
    let raw: RawIntegerProtocol = serde_json::from_str(protocol.get()).unwrap();
    assert_eq!(raw.version.get(), expected[0]);
    assert_eq!(raw.limits.variable_cells.get(), expected[1]);
    assert_eq!(raw.limits.response_bytes.get(), expected[2]);
}

#[test]
fn actual_ready_json_producer_preserves_unsigned_tokens_for_strict_rust_decoder() {
    let Some(executable) = discover_octave() else {
        assert_ne!(
            std::env::var("MATHLAB_REQUIRE_READY_JSON_TESTS").as_deref(),
            Ok("1"),
            "required Ready JSON producer tests need an installed Octave executable"
        );
        eprintln!("SKIP Ready JSON producer: no installed Octave executable.");
        return;
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("probe repository must exist");
    let work = tempfile::tempdir().unwrap();
    let source = format!(
        "addpath({}, '-begin'); addpath({}, '-begin'); ready_json_probe();",
        octave_string(&repo.join("octave")),
        octave_string(&repo.join("tests/fixtures/protocol"))
    );
    let output = Command::new(&executable)
        .args([
            "--no-gui",
            "--no-init-file",
            "--no-site-file",
            "--no-history",
            "--quiet",
            "--eval",
            &source,
        ])
        .current_dir(work.path())
        .output()
        .unwrap_or_else(|error| panic!("failed to launch {}: {error}", executable.display()));
    assert!(
        output.status.success(),
        "Ready JSON probe failed ({}): stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = std::str::from_utf8(&output.stdout).expect("probe stdout must be UTF-8");
    let passes: Vec<_> = stdout
        .lines()
        .filter(|line| line.starts_with("PASS ready_json_probe"))
        .collect();
    assert_eq!(
        passes,
        [PASS],
        "missing, altered or duplicate PASS: {stdout}"
    );

    let base = protocol_marker(stdout, "READY_JSON_BASE");
    assert_integer_tokens(&base, ["1", "2000", "33554432"]);
    let capabilities: WorkerCapabilities = serde_json::from_str(base.get())
        .expect("BASE must decode directly into the strict public protocol type");
    for required in [
        "binding_snapshot.v1",
        "checked_scalar_write.v1",
        "display_page.v1",
    ] {
        assert!(capabilities.supports(required), "BASE omitted {required}");
    }
    assert!(capabilities.supports("future_dynamic.v2"));
    capabilities
        .validate()
        .expect("BASE must satisfy the real worker capability/limit contract");

    let different = protocol_marker(stdout, "READY_JSON_DIFFERENT");
    assert_integer_tokens(&different, ["2", "1999", "33554431"]);
    let different: WorkerCapabilities = serde_json::from_str(different.get())
        .expect("alternate integral tokens must decode without normalization");
    assert_eq!(different.version, 2);
    assert_eq!(different.limits.variable_cells, 1999);
    assert_eq!(different.limits.response_bytes, 33_554_431);
    assert!(
        different.validate().is_err(),
        "alternate limits/version were repaired"
    );

    let extrema = protocol_marker(stdout, "READY_JSON_EXTREMA");
    assert_integer_tokens(
        &extrema,
        ["4294967295", "9007199254740993", "18446744073709551615"],
    );
    let exact: ExactExtremaProtocol = serde_json::from_str(extrema.get()).unwrap();
    assert_eq!(exact.version, u32::MAX);
    assert_eq!(exact.limits.variable_cells, 9_007_199_254_740_993);
    assert_eq!(exact.limits.response_bytes, u64::MAX);
    let native = serde_json::from_str::<WorkerCapabilities>(extrema.get());
    if usize::BITS >= 64 {
        let native = native.expect("64-bit usize must preserve both extrema exactly");
        assert_eq!(
            native.limits.variable_cells,
            usize::try_from(9_007_199_254_740_993_u64).unwrap()
        );
        assert_eq!(
            native.limits.response_bytes,
            usize::try_from(u64::MAX).unwrap()
        );
        assert!(
            native.validate().is_err(),
            "extrema must fail the safe protocol limits"
        );
    } else {
        assert!(
            native.is_err_and(|error| error.is_data()),
            "32-bit usize must reject overflowing limits, not truncate them"
        );
    }
}
