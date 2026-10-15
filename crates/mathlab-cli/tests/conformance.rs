// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual CLI-route checks. Synthetic saved references below exist only inside
//! test temporary directories; they are not licensed MATLAB observations.

use mathlab_engine::discover_octave;
use serde_json::Value;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "mathlab-cli-conformance-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn manifest(&self, case_ids: &[&str]) -> PathBuf {
        self.named_manifest("common.json", case_ids)
    }

    fn named_manifest(&self, name: &str, case_ids: &[&str]) -> PathBuf {
        let source = repository().join("compatibility/conformance");
        let destination = self.0.join("conformance");
        fs::create_dir(&destination).unwrap();
        copy_directory(&source.join("fixtures"), &destination.join("fixtures"));
        copy_directory(&source.join("support"), &destination.join("support"));
        copy_directory(&source.join("oracles"), &destination.join("oracles"));
        let mut manifest: Value =
            serde_json::from_slice(&fs::read(source.join(name)).unwrap()).unwrap();
        if !case_ids.is_empty() {
            manifest["cases"]
                .as_array_mut()
                .unwrap()
                .retain(|case| case_ids.contains(&case["id"].as_str().unwrap()));
            assert_eq!(manifest["cases"].as_array().unwrap().len(), case_ids.len());
        }
        let path = destination.join(name);
        write_json(&path, &manifest);
        path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn copy_directory(source: &Path, destination: &Path) {
    fs::create_dir(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            copy_directory(&entry.path(), &destination.join(entry.file_name()));
        } else if kind.is_file() {
            fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}

fn runtime() -> Option<PathBuf> {
    let Some(executable) = discover_octave() else {
        eprintln!("Skipping CP01 runtime integration: GNU Octave was not found.");
        return None;
    };
    let output = Command::new(&executable)
        .arg("--version")
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "Failed to spawn GNU Octave {executable:?} for CP01 runtime integration: {error}"
            )
        });
    if !output.status.success() || !String::from_utf8_lossy(&output.stdout).contains("11.3.0") {
        eprintln!(
            "Skipping CP01 runtime integration: the declared reference profile requires Octave 11.3.0."
        );
        return None;
    }
    Some(executable)
}

fn command(executable: &Path, directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mathlab"));
    command
        .arg("--octave")
        .arg(executable)
        .arg("--directory")
        .arg(directory)
        .args(["--timeout", "30", "--json"])
        .stdin(Stdio::null())
        // The shared fixture needs the actual application's source preparation,
        // but not an optional native ABI or interactive reply installation.
        .env_remove("MATHLAB_SCOPE_BRIDGE")
        .env_remove("MATHLAB_DEBUG_BRIDGE")
        .env_remove("MATHLAB_INPUT_BRIDGE")
        .env_remove("MATHLAB_MATLAB");
    command
}

fn run(executable: &Path, directory: &Path, manifest: &Path, extra: &[&OsStr]) -> (Output, Value) {
    let output = command(executable, directory)
        .arg("conformance")
        .arg(manifest)
        .args(extra)
        .output()
        .unwrap();
    let report = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "conformance did not return clean JSON: {error}; status={}; stdout={}; stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output, report)
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "{}: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}

fn case<'a>(report: &'a Value, id: &str) -> &'a Value {
    report["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == id)
        .unwrap()
}

fn write_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn synthetic_reference(baseline: &Value) -> Value {
    let mut reference = baseline.clone();
    reference["synthetic_test_fixture"] = Value::Bool(true);
    for case in reference["cases"].as_array_mut().unwrap() {
        let mut route = case["routes"]["octave"].clone();
        assert!(matches!(
            route["status"].as_str(),
            Some("success" | "expected_error")
        ));
        route["mode"] = Value::from("fresh");
        route["runtime"] = serde_json::json!({
            "name": "MATLAB",
            "version": "test-only-synthetic-no-reference-execution",
            "release": "R2026b",
            "executable": "/TEST_ONLY_SYNTHETIC/no-MATLAB-was-executed"
        });
        route["detail"] = Value::from(
            "Synthetic schema fixture for automated tests only; not a MATLAB observation.",
        );
        case["routes"]["matlab"] = route;
    }
    reference
}

#[test]
fn actual_three_route_report_preserves_unavailable_and_source_preparation_boundaries() {
    let Some(executable) = runtime() else { return };
    let directory = TestDirectory::new();
    let manifest = directory.manifest(&[]);
    let result_path = directory.0.join("actual-result.json");
    let (output, report) = run(
        &executable,
        &directory.0,
        &manifest,
        &[OsStr::new("--output"), result_path.as_os_str()],
    );
    assert_success(&output);
    let saved: Value = serde_json::from_slice(&fs::read(result_path).unwrap()).unwrap();
    assert_eq!(saved, report);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["kind"], "mathlab-conformance");
    assert_eq!(report["summary"]["status"], "incomplete");
    assert_eq!(report["summary"]["mismatches"], 0);
    assert_eq!(report["cases"].as_array().unwrap().len(), 5);
    let repository_revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repository())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    assert_eq!(
        report["repository_revision_at_start"],
        serde_json::json!(repository_revision)
    );
    assert_eq!(
        report["binary_identity"]["sha256"].as_str().unwrap().len(),
        64
    );
    assert!(!report["binary_identity"]["build_revision"]
        .as_str()
        .unwrap()
        .is_empty());
    assert_eq!(report["manifest_sha256"].as_str().unwrap().len(), 64);

    for current in report["cases"].as_array().unwrap() {
        let id = current["id"].as_str().unwrap();
        assert_eq!(current["fixture_sha256"].as_str().unwrap().len(), 64);
        assert_eq!(current["oracle_sha256"].as_str().unwrap().len(), 64);
        assert_eq!(current["routes"]["matlab"]["status"], "unavailable");
        assert_eq!(
            current["routes"]["mathlab"]["oracle_comparison"]["status"],
            "match"
        );
        for comparison in ["octave_to_reference", "mathlab_to_reference"] {
            assert_eq!(current["comparisons"][comparison]["status"], "unavailable");
        }
        if id == "closure_forward_local" {
            assert_eq!(current["routes"]["octave"]["status"], "unsupported");
            assert_eq!(current["routes"]["mathlab"]["status"], "success");
            assert_eq!(
                current["comparisons"]["mathlab_to_octave"]["status"],
                "unsupported"
            );
        } else {
            let status = if id == "intentional_error" {
                "expected_error"
            } else {
                "success"
            };
            assert_eq!(current["routes"]["octave"]["status"], status);
            assert_eq!(current["routes"]["mathlab"]["status"], status);
            assert_eq!(
                current["routes"]["octave"]["oracle_comparison"]["status"],
                "match"
            );
            assert_eq!(
                current["comparisons"]["mathlab_to_octave"]["status"],
                "match"
            );
        }
    }

    let values = case(&report, "exact_nd_uint64")["routes"]["mathlab"]["record"]["values"]
        .as_array()
        .unwrap();
    assert!(values.iter().any(|value| {
        value["class"] == "uint64"
            && value["real"]
                .as_array()
                .unwrap()
                .contains(&Value::from("9007199254741009"))
    }));
    assert!(values
        .iter()
        .any(|value| value["shape"] == serde_json::json!([3, 4, 5])));
    let complex = case(&report, "complex_sparse")["routes"]["mathlab"]["record"]["values"]
        .as_array()
        .unwrap();
    assert!(complex.iter().any(|value| value["complex"] == true));
    assert!(complex.iter().any(|value| {
        value["storage"] == "sparse" && !value["rows"].as_array().unwrap().is_empty()
    }));
    assert_eq!(
        case(&report, "intentional_error")["routes"]["mathlab"]["record"]["error"]["identifier"],
        "MathLab:ConformanceIntentional"
    );
}

#[test]
fn synthetic_saved_references_are_rejected_without_publishing_a_reference_match() {
    let Some(executable) = runtime() else { return };
    let directory = TestDirectory::new();
    let manifest = directory.manifest(&["exact_nd_uint64"]);
    let (output, baseline) = run(&executable, &directory.0, &manifest, &[]);
    assert_success(&output);
    for mutate_value in [false, true] {
        let mut reference = synthetic_reference(&baseline);
        if mutate_value {
            let values = reference["cases"][0]["routes"]["matlab"]["record"]["values"]
                .as_array_mut()
                .unwrap();
            let value = values
                .iter_mut()
                .find(|value| value["name"] == "exact")
                .unwrap();
            assert_eq!(value["real"][0], "9007199254741009");
            value["real"][0] = Value::from("9007199254741008");
        }
        assert_eq!(reference["synthetic_test_fixture"], true);
        let path = directory
            .0
            .join(format!("SYNTHETIC-rejected-{mutate_value}.json"));
        write_json(&path, &reference);
        let (output, report) = run(
            &executable,
            &directory.0,
            &manifest,
            &[OsStr::new("--reference"), path.as_os_str()],
        );
        assert!(!output.status.success());
        let current = case(&report, "exact_nd_uint64");
        assert_eq!(current["routes"]["matlab"]["status"], "invalid_reference");
        assert!(current["routes"]["matlab"]["detail"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("synthetic"));
        for comparison in ["octave_to_reference", "mathlab_to_reference"] {
            assert_eq!(
                current["comparisons"][comparison]["status"],
                "invalid_reference"
            );
        }
        assert_ne!(report["summary"]["status"], "matched");
    }
}

#[test]
fn observed_unavailable_reference_and_tampered_hashes_are_rejected() {
    let Some(executable) = runtime() else { return };
    let directory = TestDirectory::new();
    let manifest = directory.manifest(&["exact_nd_uint64"]);
    let (output, baseline) = run(&executable, &directory.0, &manifest, &[]);
    assert_success(&output);
    assert_eq!(
        case(&baseline, "exact_nd_uint64")["routes"]["matlab"]["status"],
        "unavailable"
    );
    assert!(case(&baseline, "exact_nd_uint64")["routes"]["matlab"]["runtime"].is_null());
    for corruption in [
        "observed_unavailable",
        "fixture_hash",
        "manifest_hash",
        "support_hash",
        "oracle_hash",
    ] {
        // Start with an actual report that explicitly did NOT observe MATLAB.
        // No successful MATLAB status/runtime/record is invented or substituted.
        let mut reference = baseline.clone();
        match corruption {
            "fixture_hash" => {
                reference["cases"][0]["fixture_sha256"] = Value::from("0".repeat(64));
            }
            "manifest_hash" => reference["manifest_sha256"] = Value::from("0".repeat(64)),
            "support_hash" => reference["support_sha256"] = Value::from("0".repeat(64)),
            "oracle_hash" => {
                reference["cases"][0]["oracle_sha256"] = Value::from("0".repeat(64));
            }
            _ => {}
        }
        let path = directory
            .0
            .join(format!("unavailable-reference-{corruption}.json"));
        write_json(&path, &reference);
        let (output, report) = run(
            &executable,
            &directory.0,
            &manifest,
            &[OsStr::new("--reference"), path.as_os_str()],
        );
        assert!(!output.status.success(), "{corruption}: {report}");
        let current = case(&report, "exact_nd_uint64");
        assert_eq!(current["routes"]["matlab"]["status"], "invalid_reference");
        let detail = current["routes"]["matlab"]["detail"]
            .as_str()
            .unwrap()
            .to_lowercase();
        if corruption.ends_with("hash") {
            assert!(detail.contains("hash"), "{corruption}: {detail}");
        }
        for comparison in ["octave_to_reference", "mathlab_to_reference"] {
            assert_eq!(
                current["comparisons"][comparison]["status"],
                "invalid_reference"
            );
        }
        assert_ne!(report["summary"]["status"], "matched");
    }
}

#[test]
fn unexpected_fixture_assertion_failure_is_identified_and_exits_unsuccessfully() {
    let Some(executable) = runtime() else { return };
    let directory = TestDirectory::new();
    let manifest = directory.manifest(&["exact_nd_uint64"]);
    let fixture = manifest
        .parent()
        .unwrap()
        .join("fixtures/exact_nd_uint64.m");
    let source = fs::read_to_string(&fixture).unwrap();
    let altered = source.replacen("+ uint64(17);", "+ uint64(18);", 1);
    assert_ne!(source, altered);
    fs::write(fixture, altered).unwrap();
    let (output, report) = run(&executable, &directory.0, &manifest, &[]);
    assert!(!output.status.success());
    assert_eq!(report["summary"]["status"], "mismatch");
    let current = case(&report, "exact_nd_uint64");
    for route in ["octave", "mathlab"] {
        assert_eq!(current["routes"][route]["status"], "error");
        assert_eq!(current["routes"][route]["record"]["outcome"], "error");
        assert!(current["routes"][route]["record"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("assert"));
    }
    assert_eq!(current["routes"]["matlab"]["status"], "unavailable");
}

#[test]
fn two_agreeing_successful_routes_cannot_pass_with_an_empty_typed_result() {
    let Some(executable) = runtime() else { return };
    let directory = TestDirectory::new();
    let manifest = directory.manifest(&["exact_nd_uint64"]);
    let fixture = manifest
        .parent()
        .unwrap()
        .join("fixtures/exact_nd_uint64.m");
    let mut source = fs::read_to_string(&fixture).unwrap();
    source.push_str(
        "\nresult = struct(); % Test-only dropped typed payload after successful assertions.\n",
    );
    fs::write(fixture, source).unwrap();
    let (output, report) = run(&executable, &directory.0, &manifest, &[]);
    assert!(!output.status.success());
    assert_eq!(report["summary"]["status"], "mismatch");
    let current = case(&report, "exact_nd_uint64");
    for route in ["octave", "mathlab"] {
        assert_eq!(current["routes"][route]["status"], "success");
        assert_eq!(current["routes"][route]["record"]["outcome"], "success");
        assert!(current["routes"][route]["record"]["values"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(
            current["routes"][route]["oracle_comparison"]["status"],
            "mismatch"
        );
        assert!(
            !current["routes"][route]["oracle_comparison"]["differences"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(
        current["comparisons"]["mathlab_to_octave"]["status"],
        "match"
    );
    assert_eq!(current["routes"]["matlab"]["status"], "unavailable");
}

#[test]
fn known_sparse_logical_mat_gap_remains_a_failure_in_both_actual_routes() {
    let Some(executable) = runtime() else { return };
    let directory = TestDirectory::new();
    let manifest = directory.named_manifest("known-gaps.json", &[]);
    let (output, report) = run(&executable, &directory.0, &manifest, &[]);
    assert!(!output.status.success());
    assert_eq!(report["summary"]["status"], "mismatch");
    let current = case(&report, "mat_sparse_logical");
    for route in ["octave", "mathlab"] {
        assert_eq!(current["routes"][route]["status"], "error");
        assert_eq!(current["routes"][route]["record"]["outcome"], "error");
        assert_eq!(
            current["routes"][route]["oracle_comparison"]["status"],
            "mismatch"
        );
        let values = current["routes"][route]["record"]["values"]
            .as_array()
            .unwrap();
        assert!(values
            .iter()
            .any(|value| value["name"] == "original" && value["storage"] == "sparse"));
        assert!(values
            .iter()
            .any(|value| value["name"] == "loaded" && value["storage"] == "dense"));
    }
    assert_eq!(current["routes"]["matlab"]["status"], "unavailable");
}

#[test]
fn missing_octave_is_explicitly_unavailable_and_never_a_match() {
    let directory = TestDirectory::new();
    let manifest = directory.manifest(&["exact_nd_uint64"]);
    let missing_runtime = directory.0.join("NO_SUCH_OCTAVE_RUNTIME");
    let (output, report) = run(&missing_runtime, &directory.0, &manifest, &[]);
    assert_success(&output);
    assert_eq!(report["summary"]["status"], "incomplete");
    let current = case(&report, "exact_nd_uint64");
    for route in ["matlab", "octave", "mathlab"] {
        assert_eq!(current["routes"][route]["status"], "unavailable");
    }
    for comparison in [
        "octave_to_reference",
        "mathlab_to_reference",
        "mathlab_to_octave",
    ] {
        assert_eq!(current["comparisons"][comparison]["status"], "unavailable");
    }
}

#[test]
fn strict_incomplete_acceptance_fails_after_publishing_the_complete_report() {
    let directory = TestDirectory::new();
    let manifest = directory.manifest(&["exact_nd_uint64"]);
    let missing_runtime = directory.0.join("NO_SUCH_OCTAVE_RUNTIME");
    let (default_output, default_report) = run(&missing_runtime, &directory.0, &manifest, &[]);
    assert_success(&default_output);
    assert_eq!(default_report["summary"]["status"], "incomplete");

    let result_path = directory.0.join("strict-result.json");
    let (strict_output, strict_report) = run(
        &missing_runtime,
        &directory.0,
        &manifest,
        &[
            OsStr::new("--strict"),
            OsStr::new("--output"),
            result_path.as_os_str(),
        ],
    );
    assert!(!strict_output.status.success());
    let saved: Value = serde_json::from_slice(&fs::read(result_path).unwrap()).unwrap();
    assert_eq!(saved, strict_report);
    assert_eq!(strict_report["summary"], default_report["summary"]);
    assert_eq!(strict_report["summary"]["status"], "incomplete");
    assert_eq!(strict_report["summary"]["matches"], 0);
    assert_eq!(strict_report["summary"]["mismatches"], 0);
    assert_eq!(strict_report["summary"]["oracle_mismatches"], 0);
    let detail = String::from_utf8_lossy(&strict_output.stderr).to_lowercase();
    assert!(detail.contains("strict") && detail.contains("incomplete"));
    let current = case(&strict_report, "exact_nd_uint64");
    for route in ["matlab", "octave", "mathlab"] {
        assert_eq!(current["routes"][route]["status"], "unavailable");
    }
    for comparison in [
        "octave_to_reference",
        "mathlab_to_reference",
        "mathlab_to_octave",
    ] {
        assert_eq!(current["comparisons"][comparison]["status"], "unavailable");
    }
}

#[test]
fn malformed_reference_case_sets_are_rejected_before_route_provenance() {
    let directory = TestDirectory::new();
    let manifest = directory.manifest(&["exact_nd_uint64"]);
    let missing_runtime = directory.0.join("NO_SUCH_OCTAVE_RUNTIME");
    let (output, baseline) = run(&missing_runtime, &directory.0, &manifest, &[]);
    assert_success(&output);
    assert_eq!(
        case(&baseline, "exact_nd_uint64")["routes"]["matlab"]["status"],
        "unavailable"
    );

    for corruption in ["duplicate_case", "missing_case", "unknown_extra_case"] {
        // Preserve the observed unavailable routes. Structural rejection must
        // not require inventing a successful MATLAB execution or its provenance.
        let mut reference = baseline.clone();
        let cases = reference["cases"].as_array_mut().unwrap();
        match corruption {
            "duplicate_case" => cases.push(cases[0].clone()),
            "missing_case" => cases.clear(),
            "unknown_extra_case" => {
                let mut unknown = cases[0].clone();
                unknown["id"] = Value::from("TEST_ONLY_UNKNOWN_CASE");
                cases.push(unknown);
            }
            _ => unreachable!(),
        }
        let reference_path = directory.0.join(format!("{corruption}.json"));
        write_json(&reference_path, &reference);
        let (output, report) = run(
            &missing_runtime,
            &directory.0,
            &manifest,
            &[OsStr::new("--reference"), reference_path.as_os_str()],
        );
        assert!(!output.status.success(), "{corruption}: {report}");
        assert_eq!(report["summary"]["status"], "mismatch");
        assert_eq!(report["summary"]["matches"], 0);
        let current = case(&report, "exact_nd_uint64");
        assert_eq!(current["routes"]["matlab"]["status"], "invalid_reference");
        let detail = current["routes"]["matlab"]["detail"]
            .as_str()
            .unwrap()
            .to_lowercase();
        let expected_detail = if corruption == "duplicate_case" {
            "duplicate"
        } else {
            "case set"
        };
        assert!(detail.contains(expected_detail), "{corruption}: {detail}");
        for comparison in ["octave_to_reference", "mathlab_to_reference"] {
            assert_eq!(
                current["comparisons"][comparison]["status"],
                "invalid_reference"
            );
        }
    }
}

#[test]
fn original_core_smoke_runner_keeps_its_schema_and_unique_fixture_execution() {
    let Some(executable) = discover_octave() else {
        eprintln!("Skipping legacy core runner regression: GNU Octave was not found.");
        return;
    };
    let directory = TestDirectory::new();
    let manifest_path = repository().join("compatibility/core.json");
    let manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    let features = manifest["features"].as_array().unwrap();
    let fixtures: BTreeSet<_> = features
        .iter()
        .map(|feature| feature["fixture"].as_str().unwrap())
        .collect();
    let output = command(&executable, &directory.0)
        .arg("check")
        .arg(&manifest_path)
        .output()
        .unwrap();
    assert_success(&output);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["features"].as_u64(), Some(features.len() as u64));
    assert_eq!(report["fixtures"].as_array().unwrap().len(), fixtures.len());
    assert_eq!(report["failures"], 0);
    assert_eq!(report["reference_profile"], manifest["reference_profile"]);
    assert!(report.get("kind").is_none());
    assert!(report["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .all(|fixture| fixture["passed"] == true));
}
