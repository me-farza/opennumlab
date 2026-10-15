// SPDX-License-Identifier: GPL-3.0-or-later
//! Import the independent reference producer's retained evidence as its own
//! format. This does not manufacture a fresh application-runner observation.

use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

const KIND: &str = "independent_fresh_licensed_CP01_reference_bundle";
const MAX_SOURCE: u64 = 256 * 1024;
const MAX_SNAPSHOT: u64 = 1024 * 1024;
const MAX_RECORD: u64 = 1024 * 1024;

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .with_context(|| format!("Independent bundle {key} missing"))
}

fn hex(text: &str, length: usize) -> bool {
    text.len() == length
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn relative(text: &str) -> Result<&Path> {
    ensure!(
        !text.is_empty()
            && !text.contains(['\\', ':', '\0', '\r', '\n'])
            && text
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."),
        "Independent bundle path must contain normal relative components"
    );
    let path = Path::new(text);
    ensure!(
        path.components()
            .all(|part| matches!(part, Component::Normal(_))),
        "Independent bundle path is not relative"
    );
    Ok(path)
}

fn confined(root: &Path, name: &str) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .context("Find independent bundle root")?;
    let candidate = root
        .join(relative(name)?)
        .canonicalize()
        .context("Find independent bundle evidence")?;
    ensure!(
        candidate.starts_with(&root),
        "Independent bundle path escapes its root"
    );
    Ok(candidate)
}

fn checked_bytes(root: &Path, entry: &Value, path_key: &str, limit: u64) -> Result<Vec<u8>> {
    let expected = entry["bytes"]
        .as_u64()
        .context("Independent bundle byte count missing")?;
    ensure!(
        expected <= limit,
        "Independent bundle evidence exceeds its byte limit"
    );
    let expected_hash = text(entry, "sha256")?;
    ensure!(
        hex(expected_hash, 64),
        "Invalid independent bundle evidence hash"
    );
    let path = confined(root, text(entry, path_key)?)?;
    let bytes = super::read_limit(&path, limit)?;
    ensure!(
        bytes.len() as u64 == expected,
        "Independent bundle evidence byte count mismatch"
    );
    ensure!(
        super::hash(&bytes) == expected_hash,
        "Independent bundle evidence hash mismatch"
    );
    Ok(bytes)
}

fn header(report: &Value) -> Result<()> {
    ensure!(
        report.get("synthetic_test_fixture").is_none(),
        "Synthetic test data is not an authentic MATLAB reference"
    );
    ensure!(
        report["schema_version"] == 1 && report["kind"] == KIND,
        "Unsupported independent reference bundle schema/kind"
    );
    ensure!(
        report["fresh_process"] == true && report["state"] == "exited" && report["exit_code"] == 0,
        "Independent reference process did not reach an exited/exit-zero terminal state"
    );
    ensure!(
        hex(text(report, "fixture_commit")?, 40),
        "Invalid independent fixture commit"
    );
    let runtime = &report["runtime_identity"];
    ensure!(
        runtime["release"] == "R2026b" && !text(runtime, "version")?.is_empty(),
        "Independent reference is outside MATLAB R2026b or lacks its version"
    );
    let command = report["command"]
        .as_array()
        .context("Independent reference command missing")?;
    ensure!(
        command
            .first()
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty()),
        "Independent reference executable missing"
    );
    ensure!(
        hex(text(report, "launcher_sha256")?, 64),
        "Invalid independent launcher hash"
    );
    let drivers = report["driver_sources"]
        .as_object()
        .context("Independent driver source hashes missing")?;
    for required in ["run_cp01_reference.py", "run_cp01_reference.m"] {
        ensure!(
            drivers
                .get(required)
                .and_then(Value::as_str)
                .is_some_and(|s| hex(s, 64)),
            "Independent driver source identity missing/invalid"
        );
    }
    Ok(())
}

/// Validate retained independent producer metadata when an application report
/// carrying this imported route is itself replayed. The caller retains the
/// original report-file hash; this metadata is provenance, not an attestation.
pub(super) fn validate_origin(origin: &Value) -> Result<()> {
    header(origin)?;
    ensure!(
        hex(text(origin, "snapshot_inventory_sha256")?, 64),
        "Invalid independent origin source inventory identity"
    );
    let record = &origin["original_record"];
    relative(text(record, "path")?)?;
    ensure!(
        hex(text(record, "sha256")?, 64)
            && record["bytes"]
                .as_u64()
                .is_some_and(|bytes| bytes <= MAX_RECORD),
        "Invalid independent origin record identity"
    );
    if let Some(hash) = origin.get("source_report_sha256") {
        ensure!(
            hash.as_str().is_some_and(|hash| hex(hash, 64)),
            "Invalid independent origin report hash"
        );
    }
    Ok(())
}

fn inventory(report: &Value) -> Result<BTreeMap<&str, &Value>> {
    let entries = report["fixture_snapshot"]
        .as_array()
        .context("Independent source inventory missing")?;
    ensure!(
        !entries.is_empty() && entries.len() <= 128,
        "Independent source inventory exceeds its entry limit"
    );
    let mut result = BTreeMap::new();
    let mut total = 0u64;
    let mut canonical = Vec::new();
    for item in entries {
        let path = text(item, "path")?;
        relative(path)?;
        ensure!(
            result.insert(path, item).is_none(),
            "Duplicate independent source inventory path"
        );
        ensure!(
            hex(text(item, "sha256")?, 64) && hex(text(item, "git_blob_id")?, 40),
            "Independent source inventory hash invalid"
        );
        let bytes = item["bytes"]
            .as_u64()
            .context("Independent source size missing")?;
        ensure!(
            bytes <= MAX_SOURCE,
            "Independent source exceeds its byte limit"
        );
        total = total
            .checked_add(bytes)
            .context("Independent source inventory byte overflow")?;
        let object = item
            .as_object()
            .context("Independent source entry is not an object")?;
        // Python's producer hashes json.dumps(sort_keys=True,separators=(',',':')).
        // The original producer paths/metadata are ASCII. BTreeMap keeps ordering
        // explicit even when serde_json is compiled with preserve_order.
        ensure!(
            path.is_ascii(),
            "Independent source inventory path is outside the ASCII producer contract"
        );
        canonical.push(
            object
                .iter()
                .map(|(k, v)| (k.as_str(), v))
                .collect::<BTreeMap<_, _>>(),
        );
    }
    ensure!(
        total <= MAX_SNAPSHOT && report["snapshot_input_bytes"] == total,
        "Independent source inventory byte total mismatch/limit"
    );
    ensure!(
        report["snapshot_inventory_sha256"] == super::hash(&serde_json::to_vec(&canonical)?),
        "Independent source inventory fingerprint mismatch"
    );
    Ok(result)
}

fn cases(report: &Value) -> Result<BTreeMap<&str, &Value>> {
    let entries = report["cases"]
        .as_array()
        .context("Independent bundle cases missing")?;
    ensure!(
        !entries.is_empty() && entries.len() <= 128,
        "Independent case inventory exceeds its limit"
    );
    let mut result = BTreeMap::new();
    for case in entries {
        let id = text(case, "id")?;
        ensure!(
            id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
            "Invalid independent case ID"
        );
        ensure!(
            result.insert(id, case).is_none(),
            "Duplicate independent bundle case ID"
        );
        relative(text(case, "manifest")?)?;
        ensure!(
            hex(text(case, "fixture_sha256")?, 64) && hex(text(case, "oracle_sha256")?, 64),
            "Independent case source identity invalid"
        );
    }
    Ok(result)
}

/// Require an exact selected manifest, while the bundle may also retain the
/// other independent producer manifest (common cases plus known negatives).
pub(super) fn validate_cases(report: &Value, manifest: &super::Manifest) -> Result<()> {
    header(report)?;
    let sources = inventory(report)?;
    let observed = cases(report)?;
    let expected: BTreeSet<_> = manifest.cases.iter().map(|case| case.id.as_str()).collect();
    ensure!(
        expected.len() == manifest.cases.len() && !expected.is_empty(),
        "Invalid selected case set"
    );
    let mut selected_manifest = None;
    for id in &expected {
        let item = observed
            .get(id)
            .context("Independent bundle is missing a selected case")?;
        let name = text(item, "manifest")?;
        ensure!(
            sources.contains_key(name),
            "Independent case manifest is absent from source inventory"
        );
        match selected_manifest {
            Some(previous) => ensure!(
                previous == name,
                "Selected cases span independent manifests"
            ),
            None => selected_manifest = Some(name),
        }
    }
    let name = selected_manifest.context("No selected independent manifest")?;
    let actual: BTreeSet<_> = observed
        .iter()
        .filter(|(_, case)| case["manifest"] == name)
        .map(|(id, _)| *id)
        .collect();
    ensure!(
        actual == expected,
        "Independent manifest case set differs from selected manifest"
    );
    // Every retained case must have an identified source manifest, including
    // extras belonging to the other manifest, rather than arbitrary unknown IDs.
    for case in observed.values() {
        ensure!(
            sources.contains_key(text(case, "manifest")?),
            "Independent case has an unidentified manifest"
        );
        let id = text(case, "id")?;
        for (name, reported_hash) in [
            (format!("fixtures/{id}.m"), &case["fixture_sha256"]),
            (format!("oracles/{id}.json"), &case["oracle_sha256"]),
        ] {
            ensure!(
                sources
                    .get(name.as_str())
                    .is_some_and(|source| source["sha256"] == *reported_hash),
                "Independent retained case is not bound to its inventoried source/oracle"
            );
        }
    }
    Ok(())
}

fn source(
    root: &Path,
    sources: &BTreeMap<&str, &Value>,
    name: &str,
    expected: &str,
) -> Result<Vec<u8>> {
    relative(name)?;
    let item = sources
        .get(name)
        .context("Selected independent source is missing from inventory")?;
    ensure!(
        item["sha256"] == expected,
        "Selected independent source hash mismatch"
    );
    checked_bytes(root, item, "path", MAX_SOURCE)
}

pub(super) fn import(
    report: &Value,
    bundle_root: &Path,
    source_root: &Path,
    fixture: &super::Fixture<'_>,
    manifest_sha: &str,
    support_sha: &str,
) -> Result<Value> {
    header(report)?;
    ensure!(
        report["runtime_identity"]["release"] == fixture.profile.matlab_release,
        "Independent reference release differs from selected profile"
    );
    let sources = inventory(report)?;
    let observed = cases(report)?;
    let item = observed
        .get(fixture.case.id.as_str())
        .context("Independent reference case missing")?;
    let manifest_name = text(item, "manifest")?;
    // Research may retain the original snapshot beside its scratch bundle, or
    // distribute records with the exact sources available in the caller's
    // conformance directory. Both routes require actual bytes to match inventory.
    let snapshot = if bundle_root.join("snapshot").is_dir() {
        confined(bundle_root, "snapshot")?
    } else {
        source_root.canonicalize()?
    };
    let manifest_bytes = source(&snapshot, &sources, manifest_name, manifest_sha)?;
    let manifest: super::Manifest = serde_json::from_slice(&manifest_bytes)?;
    validate_cases(report, &manifest)?;
    let support_name = manifest.support.join("mathlab_conformance_write.m");
    source(
        &snapshot,
        &sources,
        support_name
            .to_str()
            .context("Reference support path is not UTF-8")?,
        support_sha,
    )?;
    let fixture_name = fixture
        .case
        .fixture
        .to_str()
        .context("Reference fixture path is not UTF-8")?;
    source(&snapshot, &sources, fixture_name, &fixture.sha)?;
    let oracle_name = fixture
        .case
        .oracle
        .to_str()
        .context("Reference oracle path is not UTF-8")?;
    source(&snapshot, &sources, oracle_name, &fixture.oracle_sha)?;
    ensure!(
        item["fixture_sha256"] == fixture.sha && item["oracle_sha256"] == fixture.oracle_sha,
        "Independent case source/oracle hash mismatch"
    );
    ensure!(
        item["record_status"] == "PRESENT",
        "Independent typed record is not present"
    );
    let record_metadata = json!({"path":text(item, "record_relative_path")?,
        "bytes":item["record_bytes"],"sha256":item["record_sha256"]});
    let bytes = checked_bytes(bundle_root, &record_metadata, "path", MAX_RECORD)?;
    let record: Value =
        serde_json::from_slice(&bytes).context("Invalid independent typed record")?;
    let status = super::record_status(&record, fixture.case, false)?;
    ensure!(
        matches!(status.as_str(), "success" | "expected_error"),
        "Independent typed record did not meet the selected expected outcome"
    );

    let mut artifacts = Vec::new();
    let mut artifact_names = BTreeSet::new();
    let mut mat_artifact = Value::Null;
    if let Some(list) = item.get("mat_artifacts") {
        let list = list
            .as_array()
            .context("Independent MAT artifact inventory is not an array")?;
        ensure!(
            list.len() <= 8,
            "Independent MAT artifact count exceeds its limit"
        );
        for artifact in list {
            let name = text(artifact, "path")?;
            ensure!(
                artifact_names.insert(name),
                "Duplicate independent MAT artifact path"
            );
            let bytes = checked_bytes(bundle_root, artifact, "path", MAX_RECORD)?;
            let mut encoded = super::encode_artifact(&bytes)?;
            encoded["original_relative_path"] = json!(name);
            encoded["original_experiment"] = artifact["experiment"].clone();
            if Path::new(name)
                .file_name()
                .is_some_and(|s| s == "conformance-values.mat")
            {
                ensure!(
                    mat_artifact.is_null(),
                    "Multiple independent interchange MAT artifacts"
                );
                mat_artifact = encoded.clone();
            }
            artifacts.push(encoded);
        }
    }
    let mut runtime = report["runtime_identity"].clone();
    runtime["name"] = json!("MATLAB");
    runtime["executable"] = report["command"][0].clone();
    let origin = json!({
        "schema_version":report["schema_version"],"kind":report["kind"],
        "fresh_process":report["fresh_process"],"state":report["state"],"exit_code":report["exit_code"],
        "fixture_commit":report["fixture_commit"],"launcher_sha256":report["launcher_sha256"],
        "driver_sources":report["driver_sources"],"runtime_identity":report["runtime_identity"],
        "command":report["command"],"host":report["host"],
        "snapshot_inventory_sha256":report["snapshot_inventory_sha256"],
        "original_record":record_metadata,
        "created_utc":report["created_utc"],"started_utc":report["started_utc"],"finished_utc":report["finished_utc"],
        "source_report_provenance":"Retained independent producer metadata. The importing report records the original input file SHA256; this is not a fresh application-runner observation."
    });
    validate_origin(&origin)?;
    Ok(json!({
        "status":status,"mode":"saved_reference","runtime":runtime,"record":record,
        "fixture_sha256":fixture.sha,"profile_matches":true,"reference_format":KIND,
        "mat_artifact":mat_artifact,"mat_artifacts":artifacts,"reference_origin":origin,
        "process":{
            "success":report["state"]=="exited" && report["exit_code"]==0,
            "exit_code":report["exit_code"],"timed_out":report["state"]=="timeout",
            "capture_incomplete":null,
            "identity_scope":"Terminal outcome imported from the independent producer; application pipe-capture completeness was not reported",
            "original_state":report["state"],"original_exit_code":report["exit_code"]
        },
        "bundle_provenance":{
            "kind":report["kind"],"fixture_commit":report["fixture_commit"],
            "snapshot_inventory_sha256":report["snapshot_inventory_sha256"],
            "driver_sources":report["driver_sources"],"launcher_sha256":report["launcher_sha256"],
            "host":report["host"],"manifest":manifest_name,
            "original_record":record_metadata,"original_case":item,
            "created_utc":report["created_utc"],"started_utc":report["started_utc"],"finished_utc":report["finished_utc"],
            "elapsed_seconds":report["elapsed_seconds"],"original_fresh_process":report["fresh_process"],
            "oracle_verdict_policy":"Reported oracle verdict is observational; the application compares the validated typed record independently"
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // Synthetic unit inputs remain private to these tests. They are not saved
    // reference bundles, application reports, or licensed runtime observations.
    fn terminal_metadata() -> Value {
        json!({"schema_version":1,"kind":KIND,"fresh_process":true,
            "state":"exited","exit_code":0,"fixture_commit":"1".repeat(40),
            "runtime_identity":{"release":"R2026b","version":"UNIT_TEST_ONLY"},
            "command":["/UNIT_TEST_ONLY/not-a-runtime"],"launcher_sha256":"2".repeat(64),
            "driver_sources":{"run_cp01_reference.py":"3".repeat(64),"run_cp01_reference.m":"4".repeat(64)}})
    }

    #[test]
    fn terminal_failure_and_synthetic_markers_cannot_be_imported() {
        assert!(header(&terminal_metadata()).is_ok());
        for (field, value) in [
            ("state", json!("running")),
            ("state", json!("timeout")),
            ("exit_code", json!(1)),
            ("exit_code", Value::Null),
            ("fresh_process", json!(false)),
            ("synthetic_test_fixture", json!(true)),
        ] {
            let mut report = terminal_metadata();
            report[field] = value;
            assert!(header(&report).is_err(), "{field}");
        }
    }

    #[test]
    fn retained_origin_keeps_its_independent_terminal_and_record_identity() {
        let mut origin = terminal_metadata();
        origin["snapshot_inventory_sha256"] = json!("5".repeat(64));
        origin["original_record"] =
            json!({"path":"evidence/probe/record.json","bytes":8,"sha256":"6".repeat(64)});
        origin["source_report_sha256"] = json!("7".repeat(64));
        assert!(validate_origin(&origin).is_ok());
        let mut changed = origin.clone();
        changed["state"] = json!("output_limit");
        assert!(validate_origin(&changed).is_err());
        changed = origin.clone();
        changed["original_record"]["path"] = json!("../record.json");
        assert!(validate_origin(&changed).is_err());
        changed = origin;
        changed["source_report_sha256"] = json!("changed");
        assert!(validate_origin(&changed).is_err());
    }

    #[test]
    fn evidence_paths_are_relative_confined_and_hash_checked() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("record.json"), b"original").unwrap();
        let metadata =
            json!({"path":"record.json","bytes":8,"sha256":super::super::hash(b"original")});
        assert_eq!(
            checked_bytes(directory.path(), &metadata, "path", 64).unwrap(),
            b"original"
        );
        for name in [
            "../record.json",
            "/record.json",
            "./record.json",
            "a/../record.json",
            "a//record.json",
            "C:\\record.json",
        ] {
            assert!(relative(name).is_err(), "{name}");
        }
        let mut changed = metadata.clone();
        changed["sha256"] = json!("0".repeat(64));
        assert!(checked_bytes(directory.path(), &changed, "path", 64).is_err());
        changed = metadata;
        changed["bytes"] = json!(7);
        assert!(checked_bytes(directory.path(), &changed, "path", 64).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected_after_canonicalization() {
        let bundle = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("record.json"), b"outside").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("record.json"),
            bundle.path().join("record.json"),
        )
        .unwrap();
        assert!(confined(bundle.path(), "record.json").is_err());
    }
}
