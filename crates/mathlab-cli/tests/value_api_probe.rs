// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual CLI/public Engine API route checks for the current Unix launcher.
//! Installed application and GUI qualification are separate acceptance gates.

#![cfg(unix)]

use mathlab_engine::discover_octave;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const NATIVE_CAPABILITY: &str = "dense_complex_native_components.v1";
const NATIVE_CASES: [&str; 7] = [
    "runtime_profile",
    "uint64_checked_read_write_undo_redo",
    "double_checked_read_write_undo_redo",
    "single_checked_read_write_undo_redo",
    "stale_identity_rejected",
    "restart_preserves_native_profile",
    "same_process_stop_recovers",
];
const UNAVAILABLE_CASES: [&str; 4] = [
    "runtime_profile",
    "uint64_checked_read_write_undo_redo",
    "complex_read_requires_native",
    "complex_write_requires_native",
];
const ADAPTER_VARIABLES: [&str; 4] = [
    "MATHLAB_VALUE_BRIDGE",
    "MATHLAB_SCOPE_BRIDGE",
    "MATHLAB_INPUT_BRIDGE",
    "MATHLAB_DEBUG_BRIDGE",
];

struct ConfiguredAdapter {
    variable: &'static str,
    kind: &'static str,
    path: PathBuf,
}

struct ArtifactBefore {
    kind: &'static str,
    configured_by: Option<&'static str>,
    path: PathBuf,
    sha256: String,
}

fn configured_adapters(value: &Path) -> Vec<ConfiguredAdapter> {
    let mut adapters = vec![ConfiguredAdapter {
        variable: "MATHLAB_VALUE_BRIDGE",
        kind: "value_adapter",
        path: value.to_owned(),
    }];
    for (variable, kind) in [
        ("MATHLAB_SCOPE_BRIDGE", "scope_adapter"),
        ("MATHLAB_INPUT_BRIDGE", "input_adapter"),
        ("MATHLAB_DEBUG_BRIDGE", "debug_adapter"),
    ] {
        if let Some(selected) = std::env::var_os(variable) {
            assert!(
                Path::new(&selected).to_str().is_some(),
                "configured {variable} spelling must be UTF-8"
            );
            let path = Path::new(&selected)
                .canonicalize()
                .unwrap_or_else(|error| panic!("cannot resolve configured {variable}: {error}"));
            assert!(path.is_file(), "configured {variable} must be a file");
            assert!(
                path.to_str().is_some(),
                "configured {variable} must have a UTF-8 identity"
            );
            adapters.push(ConfiguredAdapter {
                variable,
                kind,
                path,
            });
        }
    }
    adapters
}

fn native_required() -> bool {
    std::env::var("MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS").as_deref() == Ok("1")
}

fn runtime() -> Option<PathBuf> {
    let selected = ["MATHLAB_OCTAVE", "OCTAVE_EXECUTABLE"]
        .iter()
        .find_map(std::env::var_os)
        .map(PathBuf::from)
        .or_else(discover_octave);
    let Some(selected) = selected else {
        assert!(
            !native_required(),
            "required value API probe tests need an installed GNU Octave executable"
        );
        eprintln!("SKIP optional runtime journeys: no installed GNU Octave executable.");
        return None;
    };
    let executable = selected
        .canonicalize()
        .unwrap_or_else(|error| panic!("cannot resolve selected runtime {selected:?}: {error}"));
    let output = Command::new(&executable)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|error| panic!("cannot spawn available runtime {executable:?}: {error}"));
    assert!(output.status.success(), "{}", diagnostic(&output));
    Some(executable)
}

fn native_adapter() -> Option<PathBuf> {
    let Some(path) = std::env::var_os("MATHLAB_VALUE_BRIDGE").map(PathBuf::from) else {
        assert!(
            !native_required(),
            "MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS=1 requires MATHLAB_VALUE_BRIDGE"
        );
        eprintln!("SKIP optional native journeys: MATHLAB_VALUE_BRIDGE is not configured.");
        return None;
    };
    assert!(
        path.to_str().is_some(),
        "configured value adapter spelling must be UTF-8"
    );
    let path = path
        .canonicalize()
        .expect("configured value adapter must exist");
    assert!(path.is_file(), "configured value adapter must be a file");
    Some(path)
}

fn directory() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("mathlab-cli-value-api-probe-")
        .tempdir()
        .unwrap()
}

fn command(
    executable: &Path,
    invocation_directory: &Path,
    adapter: Option<&Path>,
    unavailable: bool,
) -> Command {
    command_with_selection(Some(executable), invocation_directory, adapter, unavailable)
}

fn command_with_selection(
    executable: Option<&Path>,
    invocation_directory: &Path,
    adapter: Option<&Path>,
    unavailable: bool,
) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mathlab"));
    if let Some(executable) = executable {
        command.arg("--octave").arg(executable);
    }
    command
        // The diagnostic owns a separate workspace and intentionally ignores
        // this ordinary execution option. The nonexistent owned path catches
        // dispatch through ordinary project-directory validation.
        .arg("--directory")
        .arg(invocation_directory.join("unused-probe-project"))
        .args(["--json", "probe-value-api"])
        .current_dir(invocation_directory)
        .stdin(Stdio::null());
    for variable in ADAPTER_VARIABLES
        .into_iter()
        .chain(["MATHLAB_OCTAVE", "OCTAVE_EXECUTABLE"])
    {
        command.env_remove(variable);
    }
    if let Some(adapter) = adapter {
        command.env("MATHLAB_VALUE_BRIDGE", adapter);
    }
    if unavailable {
        command.arg("--expect-unavailable");
    }
    command
}

fn run(mut command: Command) -> (Output, Value) {
    let output = command.output().expect("probe CLI subprocess must start");
    let report = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "probe must emit exactly one clean JSON report on success or failure: {error}; {}",
            diagnostic(&output)
        )
    });
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("panicked at"),
        "{}",
        diagnostic(&output)
    );
    (output, report)
}

fn run_bound(
    command: Command,
    executable: &Path,
    adapters: &[ConfiguredAdapter],
) -> (Output, Value) {
    let cli = Path::new(env!("CARGO_BIN_EXE_mathlab"))
        .canonicalize()
        .unwrap();
    let mut expected = vec![
        ArtifactBefore {
            kind: "cli_executable",
            configured_by: None,
            sha256: hash_file(&cli),
            path: cli,
        },
        ArtifactBefore {
            kind: "octave_runtime",
            configured_by: None,
            sha256: hash_file(executable),
            path: executable.to_owned(),
        },
    ];
    expected.extend(adapters.iter().map(|adapter| ArtifactBefore {
        kind: adapter.kind,
        configured_by: Some(adapter.variable),
        path: adapter.path.clone(),
        sha256: hash_file(&adapter.path),
    }));
    let (output, report) = run(command);
    if output.status.success() {
        assert_artifact_bindings(&report, &expected);
    }
    // Failed reports may contain partial bindings. Their callers still assert
    // nonzero status and the precise setup/profile failure; no pass is inferred.
    (output, report)
}

fn diagnostic(output: &Output) -> String {
    format!(
        "status={}; stdout={}; stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn cases(report: &Value) -> BTreeMap<&str, &Value> {
    let entries = report["cases"].as_array().expect("cases must be an array");
    let mut result = BTreeMap::new();
    for entry in entries {
        let id = entry["id"].as_str().expect("case id must be a string");
        assert!(!id.is_empty());
        assert!(result.insert(id, entry).is_none(), "duplicate case {id}");
        assert!(matches!(
            entry["status"].as_str(),
            Some("passed" | "failed")
        ));
        let identifier = entry
            .get("error_identifier")
            .expect("cases must retain the actual worker identifier or null");
        assert!(identifier.is_null() || identifier.is_string());
        assert!(entry["details"].is_object());
    }
    result
}

fn assert_case_ids(report: &Value, expected: &[&str]) {
    let actual: BTreeSet<_> = cases(report).keys().copied().collect();
    let expected: BTreeSet<_> = expected.iter().copied().collect();
    assert_eq!(
        actual, expected,
        "unexecuted or unrelated cases must not be implied"
    );
}

fn assert_header(report: &Value, profile: &str, status: &str) {
    assert_eq!(report["schema_version"].as_u64(), Some(1));
    assert_eq!(report["kind"], "mathlab.value-api-probe");
    assert_eq!(report["profile"], profile);
    assert_eq!(report["status"], status);
    assert!(report["adapter_preflight"].is_array());
    assert!(report["artifact_identities"].is_array());
    let _ = capabilities(report);
}

fn assert_failure(output: &Output, report: &Value, profile: &str) {
    assert!(!output.status.success(), "{}", diagnostic(output));
    assert_header(report, profile, "failed");
    assert!(report["error"]
        .as_str()
        .is_some_and(|text| !text.is_empty()));
    assert!(cases(report)
        .values()
        .any(|case| case["status"] == "failed"));
}

fn assert_prestartup_failure(output: &Output, report: &Value) {
    assert_failure(output, report, "native-required");
    assert_build_identity(report);
    assert_case_ids(report, &["probe_setup"]);
    assert_eq!(report["runtime_details"]["started"], false);
    assert!(capabilities(report).is_empty());
    assert_eq!(report.get("value_adapter"), Some(&Value::Null));
    assert_eq!(report.get("worker_protocol"), Some(&Value::Null));
}

fn capabilities(report: &Value) -> BTreeSet<&str> {
    let entries = report["worker_capabilities"].as_array().unwrap();
    let names: BTreeSet<_> = entries
        .iter()
        .map(|entry| {
            let name = entry.as_str().expect("capabilities must be strings");
            assert!(!name.is_empty());
            name
        })
        .collect();
    assert_eq!(names.len(), entries.len());
    names
}

fn assert_requested_runtime(report: &Value, requested: &Path) {
    let observation = &report["runtime_details"]["requested_runtime"];
    assert_eq!(
        observation["display"].as_str(),
        Some(requested.to_string_lossy().as_ref())
    );
    assert_eq!(observation["path_utf8"], requested.to_str().is_some());
    assert_eq!(observation["absolute"], requested.is_absolute());
    assert_eq!(
        observation["display_is_lossy"],
        requested.to_str().is_none()
    );
}

fn assert_path(value: &Value, expected: &Path) {
    let observed = Path::new(value.as_str().expect("expected an exact path identity"));
    assert!(observed.is_absolute());
    assert_eq!(observed, observed.canonicalize().unwrap());
    assert_eq!(observed, expected);
}

fn lowercase_sha256(value: &Value) -> &str {
    let text = value.as_str().expect("expected a SHA-256 string");
    assert_eq!(text.len(), 64);
    assert!(text
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    text
}

fn hash_file(path: &Path) -> String {
    let mut actual = Sha256::new();
    let mut file = fs::File::open(path)
        .unwrap_or_else(|error| panic!("cannot hash durable artifact {path:?}: {error}"));
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        actual.update(&buffer[..count]);
    }
    format!("{:x}", actual.finalize())
}

fn assert_artifact_bindings(report: &Value, expected: &[ArtifactBefore]) {
    assert_eq!(
        report["scope"]["artifact_hashes_are_endpoint_identity_not_abi_or_load_proof"],
        true
    );
    let entries = report["artifact_identities"].as_array().unwrap();
    assert_eq!(entries.len(), expected.len());
    assert!((2..=6).contains(&entries.len()));
    let mut by_kind = BTreeMap::new();
    for entry in entries {
        let kind = entry["kind"]
            .as_str()
            .expect("artifact kind must be a string");
        assert!(
            by_kind.insert(kind, entry).is_none(),
            "duplicate artifact kind {kind}"
        );
    }
    for artifact in expected {
        let entry = by_kind
            .get(artifact.kind)
            .unwrap_or_else(|| panic!("missing artifact binding {}", artifact.kind));
        assert_path(&entry["path"], &artifact.path);
        match artifact.configured_by {
            Some(variable) => assert_eq!(entry["configured_by"], variable),
            None => assert_eq!(entry.get("configured_by"), Some(&Value::Null)),
        }
        let before = lowercase_sha256(&entry["sha256_before"]);
        let after = lowercase_sha256(&entry["sha256_after"]);
        assert_eq!(before, artifact.sha256, "{} before digest", artifact.kind);
        assert_eq!(
            after,
            hash_file(&artifact.path),
            "{} after digest",
            artifact.kind
        );
        assert_eq!(before, after, "{} changed while probing", artifact.kind);
        assert_eq!(entry["unchanged"], true);
        assert_eq!(entry.get("error"), Some(&Value::Null));
        if artifact.kind == "cli_executable" {
            assert_eq!(entry["sha256_before"], report["build"]["binary_sha256"]);
        }
        if artifact.kind == "octave_runtime" {
            assert_eq!(entry["path"], report["runtime"]);
        }
        if let Some(variable) = artifact.configured_by {
            let preflight = report["adapter_preflight"]
                .as_array()
                .unwrap()
                .iter()
                .find(|observation| observation["environment_variable"] == variable)
                .expect("each configured artifact requires matching admitted preflight");
            assert_eq!(preflight["identity_verified"], true);
            assert_eq!(preflight["abi_or_protocol_qualified"], false);
            assert_eq!(entry["path"], preflight["canonical_path"]);
        }
        if artifact.kind == "value_adapter" {
            assert_eq!(entry["path"], report["value_adapter"]);
        }
    }
}

fn assert_build_identity(report: &Value) {
    let build = &report["build"];
    let executable = Path::new(env!("CARGO_BIN_EXE_mathlab"))
        .canonicalize()
        .unwrap();
    assert_eq!(build["executable_identity_verified"], true);
    assert_eq!(build["binary_identity_verified"], true);
    assert_path(&build["executable"], &executable);
    assert_eq!(build.get("executable_identity_error"), Some(&Value::Null));
    assert_eq!(build.get("binary_identity_error"), Some(&Value::Null));
    let reported = lowercase_sha256(&build["binary_sha256"]);
    assert_eq!(reported, hash_file(&executable));
    let _ = lowercase_sha256(&build["source_sha256"]);
    for field in ["package_version", "revision", "target"] {
        assert!(build[field].as_str().is_some_and(|text| !text.is_empty()));
    }
}

fn assert_observation(report: &Value, executable: &Path, adapter: Option<&Path>, cwd: &Path) {
    assert_path(&report["runtime"], executable);
    assert_eq!(
        report["runtime_details"]["canonical_identity_verified"],
        true
    );
    assert_eq!(report["runtime_details"]["started"], true);
    assert_eq!(report["runtime_details"]["temporary_workspace"], true);
    assert_eq!(report["scope"]["public_engine_api_only"], true);
    assert_eq!(report["scope"]["owned_temporary_workspace"], true);
    assert!(report["runtime_details"]["process_id"]
        .as_u64()
        .is_some_and(|id| id > 0));
    let session_directory = Path::new(
        report["runtime_details"]["working_directory"]
            .as_str()
            .unwrap(),
    );
    // Engine has already dropped its private directory before stdout is read;
    // do not mistake an intentionally removed path for a failed identity.
    assert!(session_directory.is_absolute());
    assert_ne!(session_directory, cwd.canonicalize().unwrap());
    assert_ne!(session_directory, cwd.join("unused-probe-project"));
    assert_eq!(
        report["runtime_details"]["working_directory_observation"]["path_utf8"],
        true
    );
    assert_eq!(
        report["runtime_details"]["working_directory_observation"]["display_is_lossy"],
        false
    );
    let names = capabilities(report);
    for name in [
        "binding_snapshot.v1",
        "checked_scalar_write.v1",
        "display_page.v1",
        "typed_nd_page.v1",
        "atomic_range_edit.v1",
    ] {
        assert!(names.contains(name));
    }
    assert_eq!(
        report["worker_protocol"]["capabilities"],
        report["worker_capabilities"]
    );
    assert_eq!(report["worker_protocol"]["name"], "mathlab-worker");
    assert_eq!(report["worker_protocol"]["version"].as_u64(), Some(1));
    match adapter {
        Some(adapter) => {
            assert_path(&report["value_adapter"], adapter);
            assert!(names.contains(NATIVE_CAPABILITY));
        }
        None => {
            assert_eq!(report.get("value_adapter"), Some(&Value::Null));
            assert!(!names.contains(NATIVE_CAPABILITY));
        }
    }
}

fn assert_admitted_adapter(report: &Value, requested: &Path, canonical: &Path) {
    let preflight = report["adapter_preflight"].as_array().unwrap();
    assert_eq!(preflight.len(), 1);
    assert_eq!(preflight[0]["environment_variable"], "MATHLAB_VALUE_BRIDGE");
    assert_eq!(preflight[0]["identity_verified"], true);
    assert_eq!(preflight[0]["abi_or_protocol_qualified"], false);
    assert_eq!(
        preflight[0]["requested_path"]["display"].as_str(),
        requested.to_str()
    );
    assert_path(&preflight[0]["canonical_path"], canonical);
}

fn assert_rejected_adapter(report: &Value, variable: &str, requested: &Path) {
    let preflight = report["adapter_preflight"].as_array().unwrap();
    assert_eq!(preflight.len(), 1);
    let observed = &preflight[0];
    assert_eq!(observed["environment_variable"], variable);
    assert_eq!(observed["identity_verified"], false);
    assert_eq!(observed["abi_or_protocol_qualified"], false);
    assert_eq!(observed.get("canonical_path"), Some(&Value::Null));
    assert_eq!(
        observed["requested_path"]["display"].as_str(),
        Some(requested.to_string_lossy().as_ref())
    );
    assert_eq!(
        observed["requested_path"]["path_utf8"],
        requested.to_str().is_some()
    );
    assert_eq!(
        observed["requested_path"]["display_is_lossy"],
        requested.to_str().is_none()
    );
    assert!(observed["error"]
        .as_str()
        .is_some_and(|text| !text.is_empty()));
    assert!(report["error"].as_str().unwrap().contains(variable));
}

fn counter(context: &Value, field: &str) -> u64 {
    let text = context[field]
        .as_str()
        .expect("binding counter must remain a string");
    let value: u64 = text.parse().unwrap();
    assert_eq!(text, value.to_string());
    value
}

fn assert_common_uint64_case(report: &Value) {
    let cases = cases(report);
    let case = cases["uint64_checked_read_write_undo_redo"];
    assert_eq!(case["status"], "passed");
    assert_eq!(case.get("error_identifier"), Some(&Value::Null));
    let details = &case["details"];
    assert_eq!(details["class"], "uint64");
    assert_eq!(details["source_shape"], serde_json::json!([1, 3]));
    assert_eq!(details["selected_rows"], 1);
    assert_eq!(details["selected_columns"], 2);
    assert_eq!(details["journey"], "read-write-undo-redo");
    // Keep exact integer identities as strings; no JSON floating conversion.
    assert_eq!(
        details["old_values"],
        serde_json::json!(["9007199254740993", "18446744073709551615", "17"])
    );
    assert_eq!(
        details["new_values"],
        serde_json::json!(["18446744073709551615", "9007199254740995", "17"])
    );
    for field in [
        "exact_terminal_integer_oracle",
        "original_alias_preserved",
        "untouched_cell_and_peer_preserved",
    ] {
        assert_eq!(details[field], true);
    }
    assert_eq!(details["native_adapter_required"], false);
}

#[test]
fn configured_native_profile_exercises_exact_checked_api_journeys() {
    let Some(adapter) = native_adapter() else {
        return;
    };
    let executable = runtime().expect("a configured adapter requires an available runtime");
    let adapters = configured_adapters(&adapter);
    let cwd = directory();
    symlink(&executable, cwd.path().join("selected runtime with spaces")).unwrap();
    symlink(&adapter, cwd.path().join("selected value adapter.oct")).unwrap();
    let selected_runtime = Path::new("./selected runtime with spaces");
    let selected_adapter = Path::new("./selected value adapter.oct");
    let mut child = command(selected_runtime, cwd.path(), Some(selected_adapter), false);
    for extra in adapters.iter().skip(1) {
        child.env(extra.variable, &extra.path);
    }
    let (output, report) = run_bound(child, &executable, &adapters);
    assert!(output.status.success(), "{}", diagnostic(&output));
    assert_header(&report, "native-required", "passed");
    assert_eq!(report.get("error"), Some(&Value::Null));
    assert_build_identity(&report);
    assert_requested_runtime(&report, selected_runtime);
    assert_observation(&report, &executable, Some(&adapter), cwd.path());
    let preflight = report["adapter_preflight"].as_array().unwrap();
    assert_eq!(preflight.len(), adapters.len());
    for selected in &adapters {
        let observation = preflight
            .iter()
            .find(|entry| entry["environment_variable"] == selected.variable)
            .unwrap();
        let requested = if selected.variable == "MATHLAB_VALUE_BRIDGE" {
            selected_adapter
        } else {
            &selected.path
        };
        assert_eq!(
            observation["requested_path"]["display"].as_str(),
            requested.to_str()
        );
        assert_eq!(observation["identity_verified"], true);
        assert_eq!(observation["abi_or_protocol_qualified"], false);
        assert_path(&observation["canonical_path"], &selected.path);
    }
    assert_case_ids(&report, &NATIVE_CASES);
    assert_common_uint64_case(&report);
    assert_eq!(report["scope"]["native_precision_requested"], true);
    let cases = cases(&report);
    assert!(cases.values().all(|case| case["status"] == "passed"));
    for (id, class) in [
        ("double_checked_read_write_undo_redo", "double"),
        ("single_checked_read_write_undo_redo", "single"),
    ] {
        let details = &cases[id]["details"];
        assert_eq!(details["class"], class);
        assert_eq!(details["path"], "R.cell{1}.leaf");
        assert_eq!(details["source_shape"], serde_json::json!([3, 1]));
        let journeys = details["journeys"].as_array().unwrap();
        assert_eq!(journeys.len(), 6);
        let mut identities = BTreeSet::new();
        for journey in journeys {
            let rows = journey["selected_rows"].as_u64().unwrap();
            let kind = journey["journey"].as_str().unwrap();
            assert!([1, 2].contains(&rows));
            assert!(["write-only", "write-undo", "write-undo-redo"].contains(&kind));
            assert_eq!(journey["selected_columns"], 1);
            assert_eq!(journey["exact_final_bit_oracle"], true);
            assert!(journey["retained_logical_patch_bytes"]
                .as_u64()
                .is_some_and(|bytes| bytes > 0));
            assert!(
                identities.insert((rows, kind)),
                "duplicate precision journey"
            );
        }
    }
    let restart = &cases["restart_preserves_native_profile"]["details"];
    assert!(
        counter(&restart["after_context"], "generation")
            > counter(&restart["before_context"], "generation")
    );
    assert_eq!(restart["workspace_cleared"], true);
    assert_eq!(restart["fresh_workspace_variable_count"].as_u64(), Some(0));
    assert_eq!(restart["profile_reinstalled"], true);
    let stale = &cases["stale_identity_rejected"]["details"];
    for field in [
        "stale_page_error_identifier",
        "stale_write_error_identifier",
        "stale_binding_history_error_identifier",
    ] {
        assert_eq!(stale[field], "MathLab:StaleBinding");
    }
    assert_eq!(
        stale["retired_history_error_identifier"],
        "MathLab:ValueEditHistoryConflict"
    );
    assert_eq!(stale["automatic_retry_performed_by_probe"], false);
    let stop = &cases["same_process_stop_recovers"]["details"];
    assert_eq!(stop["observed_stop_identifier"], "MathLab:Interrupted");
    assert_eq!(stop["source_start_count"], 1);
    assert_eq!(stop["continuation_absent"], true);
    assert_eq!(stop["old_authority_rejected"], true);
    assert_eq!(stop["commit_window_test"], false);
    assert_eq!(
        stop["before_context"]["worker_id"],
        stop["recovered_context"]["worker_id"]
    );
    assert_eq!(
        counter(&stop["before_context"], "generation"),
        counter(&stop["recovered_context"], "generation")
    );
    assert!(
        counter(&stop["recovered_context"], "invocation")
            > counter(&stop["before_context"], "invocation")
    );
}

#[test]
fn unavailable_profile_has_exact_rejection_cases_and_resolves_runtime_aliases() {
    let Some(executable) = runtime() else { return };
    let cwd = directory();
    let bin = cwd.path().join("owned path entries");
    fs::create_dir(&bin).unwrap();
    let bare_name = "owned runtime alias";
    symlink(&executable, bin.join(bare_name)).unwrap();
    symlink(&executable, cwd.path().join("relative runtime alias")).unwrap();
    let mut path_entries = vec![bin];
    if let Some(inherited) = std::env::var_os("PATH") {
        path_entries.extend(std::env::split_paths(&inherited));
    }
    let search_path = std::env::join_paths(path_entries).unwrap();
    for selected in [
        executable.as_path(),
        Path::new(bare_name),
        Path::new("./relative runtime alias"),
    ] {
        let mut child = command(selected, cwd.path(), None, true);
        child.env("PATH", &search_path);
        let (output, report) = run_bound(child, &executable, &[]);
        assert!(output.status.success(), "{}", diagnostic(&output));
        assert_header(&report, "native-unavailable", "passed");
        assert_build_identity(&report);
        assert_requested_runtime(&report, selected);
        assert_observation(&report, &executable, None, cwd.path());
        assert!(report["adapter_preflight"].as_array().unwrap().is_empty());
        assert_case_ids(&report, &UNAVAILABLE_CASES);
        assert_common_uint64_case(&report);
        assert_eq!(report["scope"]["native_precision_requested"], false);
        let cases = cases(&report);
        assert!(cases.values().all(|case| case["status"] == "passed"));
        for (id, identifier) in [
            (
                "complex_read_requires_native",
                "MathLab:ValuePagePrecisionUnavailable",
            ),
            (
                "complex_write_requires_native",
                "MathLab:ValueEditPrecisionUnavailable",
            ),
        ] {
            assert_eq!(cases[id]["error_identifier"], identifier);
            assert_eq!(
                cases[id]["details"]["observed_error_identifier"],
                identifier
            );
            assert_eq!(cases[id]["details"]["native_precision_success"], false);
        }
        assert_eq!(
            cases["complex_write_requires_native"]["details"]["unchanged_values"],
            true
        );
        assert_eq!(
            cases["complex_write_requires_native"]["details"]["exact_final_component_bits"],
            true
        );
        assert_eq!(
            cases["complex_write_requires_native"]["details"]
                ["rejection_context_verified_before_terminal_oracle"],
            true
        );
    }
}

#[test]
fn setup_failures_keep_clean_failed_json_for_runtime_and_adapter_paths() {
    let cwd = directory();
    let missing_runtime = cwd.path().join("missing selected runtime");
    let (output, report) = run(command(&missing_runtime, cwd.path(), None, false));
    assert_prestartup_failure(&output, &report);
    assert_requested_runtime(&report, &missing_runtime);
    assert_eq!(report.get("runtime"), Some(&Value::Null));
    assert_eq!(
        report["runtime_details"]["canonical_identity_verified"],
        false
    );
    assert!(report["adapter_preflight"].as_array().unwrap().is_empty());

    let invalid_runtime = cwd
        .path()
        .join(OsString::from_vec(b"missing-runtime-\xff".to_vec()));
    let (output, report) = run(command(&invalid_runtime, cwd.path(), None, false));
    assert_prestartup_failure(&output, &report);
    assert_requested_runtime(&report, &invalid_runtime);
    assert_eq!(report.get("runtime"), Some(&Value::Null));

    // A real executable is unnecessary to prove adapter preflight: this owned
    // executable-shaped file must never be launched by these failing requests.
    let preflight_runtime = cwd.path().join("runtime that preflight must not launch");
    fs::write(&preflight_runtime, b"not an executable image\n").unwrap();
    fs::set_permissions(&preflight_runtime, fs::Permissions::from_mode(0o700)).unwrap();
    let preflight_runtime = preflight_runtime.canonicalize().unwrap();
    let decoy_bin = cwd.path().join("owned fallback candidates");
    fs::create_dir(&decoy_bin).unwrap();
    symlink(&preflight_runtime, decoy_bin.join("octave-cli")).unwrap();
    let mut path_entries = vec![decoy_bin];
    if let Some(inherited) = std::env::var_os("PATH") {
        path_entries.extend(std::env::split_paths(&inherited));
    }
    let search_path = std::env::join_paths(path_entries).unwrap();
    for variable in ["MATHLAB_OCTAVE", "OCTAVE_EXECUTABLE"] {
        let mut child = command_with_selection(None, cwd.path(), None, false);
        child
            .env(variable, &missing_runtime)
            .env("PATH", &search_path);
        if variable == "MATHLAB_OCTAVE" {
            // First-present selection must win over this executable alternate,
            // even though the selected primary runtime is missing.
            child.env("OCTAVE_EXECUTABLE", &preflight_runtime);
        }
        let (output, report) = run(child);
        assert_prestartup_failure(&output, &report);
        assert_requested_runtime(&report, &missing_runtime);
        assert_eq!(report.get("runtime"), Some(&Value::Null));
    }
    let missing_module = cwd.path().join("missing module.oct");
    let raw_module = cwd
        .path()
        .join(OsString::from_vec(b"missing-module-\xff.oct".to_vec()));
    for variable in ADAPTER_VARIABLES {
        for selected in [&missing_module, &raw_module] {
            let mut child = command(&preflight_runtime, cwd.path(), None, false);
            child.env(variable, selected);
            let (output, report) = run(child);
            assert_prestartup_failure(&output, &report);
            assert_path(&report["runtime"], &preflight_runtime);
            assert_rejected_adapter(&report, variable, selected);
        }
    }
    // APFS cannot create invalid-byte names. Existing canonical-target coverage
    // is explicitly non-macOS Unix; its Linux qualification remains unrun.
    #[cfg(not(target_os = "macos"))]
    {
        fs::write(&raw_module, b"not a loadable module\n").unwrap();
        let alias = cwd.path().join("UTF-8 module alias.oct");
        symlink(&raw_module, &alias).unwrap();
        for variable in ADAPTER_VARIABLES {
            let mut child = command(&preflight_runtime, cwd.path(), None, false);
            child.env(variable, &alias);
            let (output, report) = run(child);
            assert_prestartup_failure(&output, &report);
            assert_rejected_adapter(&report, variable, &alias);
            assert!(report["error"]
                .as_str()
                .unwrap()
                .contains("Canonical adapter path"));
        }
    }

    if let Some(executable) = runtime() {
        let junk_module = cwd.path().join("UTF-8 junk value module.oct");
        fs::write(&junk_module, b"not a compiled Octave module\n").unwrap();
        let (output, report) = run(command(&executable, cwd.path(), Some(&junk_module), false));
        assert_failure(&output, &report, "native-required");
        assert_case_ids(&report, &["startup"]);
        assert_eq!(report["runtime_details"]["started"], false);
        assert!(capabilities(&report).is_empty());
        assert_eq!(report.get("value_adapter"), Some(&Value::Null));
        // Path admission is not a fabricated ABI/load success.
        assert_admitted_adapter(&report, &junk_module, &junk_module.canonicalize().unwrap());
    }
}

#[test]
fn profile_conflicts_fail_closed_without_successful_skips_or_profile_changes() {
    let Some(executable) = runtime() else { return };
    let cwd = directory();
    let (output, report) = run(command(&executable, cwd.path(), None, false));
    assert_failure(&output, &report, "native-required");
    assert_case_ids(&report, &["runtime_profile"]);
    assert_observation(&report, &executable, None, cwd.path());
    assert_eq!(cases(&report)["runtime_profile"]["status"], "failed");
    assert!(report["adapter_preflight"].as_array().unwrap().is_empty());

    let Some(adapter) = native_adapter() else {
        return;
    };
    let (output, report) = run(command(&executable, cwd.path(), Some(&adapter), true));
    assert_failure(&output, &report, "native-unavailable");
    assert_case_ids(&report, &["runtime_profile"]);
    assert_observation(&report, &executable, Some(&adapter), cwd.path());
    assert_admitted_adapter(&report, &adapter, &adapter);
    assert_eq!(cases(&report)["runtime_profile"]["status"], "failed");
}
