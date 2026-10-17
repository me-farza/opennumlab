// SPDX-License-Identifier: GPL-3.0-or-later
//! Original path-encoding and prepared-request checks.
//! No numerical process, fake worker or non-UTF-8 filesystem entry is created.

use super::{
    Error, LocalFunctionLocation, MaterializedScript, PreparedSource, SourceKind,
    canonical_wire_path, prepared_request, wire_path,
};
use std::path::{Path, PathBuf};

fn prepared_fixture() -> PreparedSource {
    PreparedSource {
        original_path: PathBuf::from("/tmp/project λ/it's original.m"),
        original_source: "x = 1; % λ\n".to_owned(),
        materialized: MaterializedScript {
            script_path: PathBuf::from("/tmp/prepared λ/immutable.mlsource"),
            function_paths: Vec::new(),
            source_mapping: Vec::new(),
        },
        local_functions: Vec::new(),
        factory_path: None,
        snapshot_path: PathBuf::from("/tmp/prepared λ/original.mlsnapshot"),
        source_kind: SourceKind::Script,
    }
}

#[test]
fn wire_path_preserves_unicode_and_literal_os_spelling() {
    for literal in [
        "relative/../δ path.m",
        "/tmp/it's a λ path.m",
        "relative\\folder\\δ.m",
    ] {
        let encoded = wire_path(Path::new(literal), "test source path").unwrap();
        assert_eq!(encoded, literal);
        let json = serde_json::to_string(&encoded).unwrap();
        let decoded: String = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, literal);
    }
}

#[test]
fn wire_path_returns_an_owned_string() {
    let path = PathBuf::from("relative/λ original.m");
    let encoded = wire_path(&path, "test source path").unwrap();
    drop(path);
    assert_eq!(encoded, "relative/λ original.m");
}

#[test]
fn prepared_request_emits_exact_strings_and_null_factory() {
    let source = prepared_fixture();
    let original = source.original_path.clone();
    let script = source.materialized.script_path.clone();
    let source_text = source.original_source.clone();
    let request = prepared_request(&source, false).unwrap();
    assert_eq!(request["command"].as_str(), Some("run_file"));
    assert_eq!(
        request["path"].as_str(),
        Some("/tmp/prepared λ/immutable.mlsource")
    );
    assert_eq!(
        request["source_directory"].as_str(),
        Some("/tmp/prepared λ")
    );
    assert_eq!(
        request["original_path"].as_str(),
        Some("/tmp/project λ/it's original.m")
    );
    assert_eq!(request["original_source"].as_str(), Some("x = 1; % λ\n"));
    assert!(request["factory_path"].is_null());
    assert_eq!(request["keep_source_directory"].as_bool(), Some(false));
    assert_eq!(request["source_kind"].as_str(), Some("script"));
    assert_eq!(source.original_path, original);
    assert_eq!(source.materialized.script_path, script);
    assert_eq!(source.original_source, source_text);
}

#[test]
fn debugger_plain_request_omits_directory_only_when_eligible() {
    let source = prepared_fixture();
    let normal = prepared_request(&source, false).unwrap();
    let debug = prepared_request(&source, true).unwrap();
    assert!(normal.get("source_directory").is_some());
    assert!(debug.get("source_directory").is_none());
    for key in [
        "path",
        "original_path",
        "original_source",
        "source_kind",
        "factory_path",
    ] {
        assert_eq!(normal[key], debug[key]);
    }
}

#[test]
fn prepared_request_retains_local_capsule_and_names() {
    let mut source = prepared_fixture();
    source
        .materialized
        .function_paths
        .push(PathBuf::from("/tmp/prepared λ/private/inner.m"));
    source.local_functions.push(LocalFunctionLocation {
        name: "inner".to_owned(),
        start_line: 3,
        end_line: 5,
    });
    source.factory_path = Some(PathBuf::from("/tmp/prepared λ/immutable_factory.m"));
    let request = prepared_request(&source, true).unwrap();
    assert_eq!(
        request["source_directory"].as_str(),
        Some("/tmp/prepared λ")
    );
    assert_eq!(
        request["factory_path"].as_str(),
        Some("/tmp/prepared λ/immutable_factory.m")
    );
    assert_eq!(request["local_names"][0].as_str(), Some("inner"));
    assert_eq!(request["local_names"].as_array().unwrap().len(), 1);
    assert_eq!(request["keep_source_directory"].as_bool(), Some(true));
}

#[test]
fn prepared_request_preserves_function_and_class_kind_behavior() {
    for (kind, expected) in [
        (SourceKind::Function, "function"),
        (SourceKind::Class, "class"),
    ] {
        let mut source = prepared_fixture();
        source.source_kind = kind;
        let request = prepared_request(&source, true).unwrap();
        assert_eq!(request["source_kind"].as_str(), Some(expected));
        assert_eq!(
            request["source_directory"].as_str(),
            Some("/tmp/prepared λ")
        );
    }
}

#[cfg(any(unix, windows))]
fn invalid_path() -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_vec(vec![
            b'/', b'n', b'o', b't', b'-', b'c', b'r', b'e', b'a', b't', b'e', b'd', b'/', 0xff,
            b'.', b'm',
        ]))
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_wide(&[
            67, 58, 92, 110, 111, 116, 92, 0xd800, 46, 109,
        ]))
    }
}

#[cfg(any(unix, windows))]
fn assert_encoding_error(error: Error, expected_role: &'static str, expected_path: &Path) {
    match error {
        Error::NonUtf8Path { role, path } => {
            assert_eq!(role, expected_role);
            assert_eq!(path.as_path(), expected_path);
        }
        other => panic!("Expected an encoding error, received {other:?}"),
    }
}

#[cfg(any(unix, windows))]
#[test]
fn wire_path_rejects_non_utf8_and_retains_owned_identity() {
    let path = invalid_path();
    assert!(path.to_str().is_none());
    let error = wire_path(&path, "test source path").unwrap_err();
    assert_encoding_error(error, "test source path", &path);
}

#[cfg(any(unix, windows))]
#[test]
fn canonical_preflight_rejects_raw_encoding_before_filesystem_lookup() {
    let path = invalid_path();
    // If canonicalize were reached first, this would produce an IO error instead.
    // No path is created; this check is safe on APFS and other Unicode filesystems.
    let error = canonical_wire_path(&path, "test adapter path").unwrap_err();
    assert_encoding_error(error, "test adapter path", &path);
}

#[cfg(any(unix, windows))]
#[test]
fn prepared_request_rejects_original_path_without_replacing_its_identity() {
    let mut source = prepared_fixture();
    source.original_path = invalid_path();
    let before = source.original_path.clone();
    let before_source = source.original_source.clone();
    let error = prepared_request(&source, false).unwrap_err();
    assert_encoding_error(error, "original source path", &before);
    assert_eq!(source.original_path, before);
    assert_eq!(source.original_source, before_source);
}

#[cfg(any(unix, windows))]
#[test]
fn prepared_request_rejects_a_preserved_non_utf8_script_filename() {
    for debug in [false, true] {
        let mut source = prepared_fixture();
        source.materialized.script_path = invalid_path();
        let before = source.materialized.script_path.clone();
        let error = prepared_request(&source, debug).unwrap_err();
        assert_encoding_error(error, "prepared script path", &before);
        assert_eq!(source.materialized.script_path, before);
    }
}

#[cfg(any(unix, windows))]
#[test]
fn prepared_request_rejects_a_non_utf8_generated_parent() {
    let mut source = prepared_fixture();
    source.materialized.script_path = invalid_path().join("valid_ascii_script.m");
    let before = source.materialized.script_path.clone();
    let error = prepared_request(&source, false).unwrap_err();
    // The complete script identity is checked before its parent field.
    assert_encoding_error(error, "prepared script path", &before);
}

#[cfg(any(unix, windows))]
#[test]
fn prepared_request_rejects_non_utf8_factory_without_mutating_source() {
    let mut source = prepared_fixture();
    source.factory_path = Some(invalid_path());
    let before = source.factory_path.clone();
    let before_source = source.original_source.clone();
    let error = prepared_request(&source, false).unwrap_err();
    assert_encoding_error(error, "prepared factory path", before.as_deref().unwrap());
    assert_eq!(source.factory_path, before);
    assert_eq!(source.original_source, before_source);
}

#[cfg(any(unix, windows))]
#[test]
fn error_display_is_diagnostic_only_and_retains_the_original_path() {
    let path = invalid_path();
    let error = wire_path(&path, "test adapter path").unwrap_err();
    let display = error.to_string();
    assert!(display.contains("test adapter path"));
    assert!(display.contains("exact UTF-8 JSON path"));
    assert_encoding_error(error, "test adapter path", &path);
}
