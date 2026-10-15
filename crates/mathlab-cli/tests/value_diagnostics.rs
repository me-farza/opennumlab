// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual doctor subprocess checks. These establish session diagnostics only;
//! checked value operations and installed desktop acceptance are separate gates.

use mathlab_engine::discover_octave;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const NATIVE_VALUE_CAPABILITY: &str = "dense_complex_native_components.v1";
const BASE_CAPABILITIES: [&str; 5] = [
    "binding_snapshot.v1",
    "checked_scalar_write.v1",
    "display_page.v1",
    "typed_nd_page.v1",
    "atomic_range_edit.v1",
];

fn native_required() -> bool {
    std::env::var("MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS").as_deref() == Ok("1")
}

fn runtime() -> Option<PathBuf> {
    // An explicit selection is authoritative even if it is broken. Discovery
    // must not replace a configured, unusable runtime with another installation.
    let selected = ["MATHLAB_OCTAVE", "OCTAVE_EXECUTABLE"]
        .iter()
        .find_map(std::env::var_os)
        .map(PathBuf::from)
        .or_else(discover_octave);
    let Some(selected) = selected else {
        assert!(
            !native_required(),
            "required value diagnostics need an installed GNU Octave executable"
        );
        eprintln!("SKIP doctor runtime diagnostics: no installed GNU Octave executable.");
        return None;
    };
    let executable = selected
        .canonicalize()
        .unwrap_or_else(|error| panic!("cannot resolve selected GNU Octave {selected:?}: {error}"));
    let output = Command::new(&executable)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|error| panic!("cannot start selected GNU Octave {executable:?}: {error}"));
    assert!(
        output.status.success(),
        "selected GNU Octave --version failed: {}",
        diagnostic(&output)
    );
    Some(executable)
}

fn configured_value_adapter() -> Option<PathBuf> {
    let Some(path) = std::env::var_os("MATHLAB_VALUE_BRIDGE").map(PathBuf::from) else {
        assert!(
            !native_required(),
            "MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS=1 requires MATHLAB_VALUE_BRIDGE"
        );
        eprintln!("SKIP native doctor diagnostics: MATHLAB_VALUE_BRIDGE is not configured.");
        return None;
    };
    let canonical = path.canonicalize().unwrap_or_else(|error| {
        panic!("cannot resolve configured value adapter {path:?}: {error}")
    });
    assert!(canonical.is_file(), "value adapter must be a file");
    Some(canonical)
}

fn directory() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("mathlab-cli-value-diagnostics-")
        .tempdir()
        .unwrap()
}

fn command(
    executable: &Path,
    requested_directory: &Path,
    current_directory: &Path,
    adapter: Option<&Path>,
) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mathlab"));
    command
        .arg("--octave")
        .arg(executable)
        .arg("--directory")
        .arg(requested_directory)
        .args(["--json", "--timeout", "30"])
        .current_dir(current_directory)
        .stdin(Stdio::null());
    for variable in [
        "MATHLAB_DEBUG_BRIDGE",
        "MATHLAB_SCOPE_BRIDGE",
        "MATHLAB_INPUT_BRIDGE",
        "MATHLAB_VALUE_BRIDGE",
        "MATHLAB_OCTAVE",
        "OCTAVE_EXECUTABLE",
    ] {
        command.env_remove(variable);
    }
    if let Some(adapter) = adapter {
        command.env("MATHLAB_VALUE_BRIDGE", adapter);
    }
    command
}

fn doctor(executable: &Path, directory: &Path, adapter: Option<&Path>) -> (Output, Value) {
    doctor_command(command(executable, directory, directory, adapter))
}

fn doctor_command(mut command: Command) -> (Output, Value) {
    let output = command
        .arg("doctor")
        .output()
        .expect("doctor CLI subprocess must start");
    let report = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "doctor must emit clean JSON, including failures: {error}; {}",
            diagnostic(&output)
        )
    });
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

fn capabilities(report: &Value) -> BTreeSet<&str> {
    let values = report["worker_capabilities"]
        .as_array()
        .expect("worker_capabilities must be the observed string array");
    let names: BTreeSet<_> = values
        .iter()
        .map(|value| {
            let name = value.as_str().expect("capabilities must be strings");
            assert!(!name.is_empty(), "capabilities cannot be empty strings");
            name
        })
        .collect();
    assert_eq!(names.len(), values.len(), "capabilities must be unique");
    names
}

fn assert_path(report: &Value, field: &str, expected: &Path) {
    let observed = Path::new(
        report[field]
            .as_str()
            .unwrap_or_else(|| panic!("{field} must be an absolute canonical path")),
    );
    assert!(observed.is_absolute(), "{field} must be absolute");
    assert_eq!(observed, observed.canonicalize().unwrap());
    assert_eq!(observed, expected, "{field} must name the selected input");
}

fn assert_ready(report: &Value, executable: &Path, adapter: Option<&Path>) {
    assert_eq!(report["ready"], true);
    assert_path(report, "runtime", executable);
    let advertised = capabilities(report);
    for required in BASE_CAPABILITIES {
        assert!(
            advertised.contains(required),
            "ready worker did not advertise {required}"
        );
    }
    match adapter {
        Some(adapter) => {
            assert_path(report, "value_adapter", adapter);
            assert!(advertised.contains(NATIVE_VALUE_CAPABILITY));
        }
        None => {
            assert_eq!(report.get("value_adapter"), Some(&Value::Null));
            assert!(!advertised.contains(NATIVE_VALUE_CAPABILITY));
        }
    }
}

fn assert_not_ready(output: &Output, report: &Value) {
    assert!(!output.status.success(), "{}", diagnostic(output));
    assert_eq!(report["ready"], false, "{}", diagnostic(output));
    assert!(
        report["error"]
            .as_str()
            .is_some_and(|text| !text.is_empty()),
        "failure must contain a diagnostic: {report}"
    );
    assert_eq!(report.get("value_adapter"), Some(&Value::Null));
    assert!(capabilities(report).is_empty());
}

fn selected_runtime_alias(executable: &Path, directory: &Path) -> PathBuf {
    #[cfg(unix)]
    {
        let alias = directory.join("selected runtime with spaces");
        std::os::unix::fs::symlink(executable, &alias).unwrap();
        alias
    }
    #[cfg(not(unix))]
    {
        let _ = directory;
        executable
            .parent()
            .unwrap()
            .join(".")
            .join(executable.file_name().unwrap())
    }
}

#[test]
fn doctor_without_adapter_observes_capabilities_and_canonical_selected_runtime() {
    let Some(executable) = runtime() else { return };
    let directory = directory();
    let selected = selected_runtime_alias(&executable, directory.path());
    let (output, report) = doctor(&selected, directory.path(), None);
    assert!(output.status.success(), "{}", diagnostic(&output));
    assert_ready(&report, &executable, None);
}

#[test]
fn doctor_rejects_missing_and_bad_configured_modules_without_claiming_ready() {
    let Some(executable) = runtime() else { return };
    let directory = directory();
    let missing = directory.path().join("missing value module.oct");
    let bad = directory.path().join("bad value module.oct");
    fs::write(&bad, b"This is deliberately not an Octave module.\n").unwrap();
    for adapter in [&missing, &bad] {
        let (output, report) = doctor(&executable, directory.path(), Some(adapter));
        assert_not_ready(&output, &report);
        assert_path(&report, "runtime", &executable);
    }
    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let non_utf8 = directory
            .path()
            .join(OsString::from_vec(b"native-module-\xff.oct".to_vec()));
        // macOS filesystems reject invalid UTF-8 filenames. Passing the raw
        // path still exercises the CLI's actual JSON failure boundary there.
        // Other Unix filesystems also exercise canonical existing-file and
        // UTF-8 symlink-target checks; those proofs remain platform-specific.
        let selected_paths = {
            #[cfg(not(target_os = "macos"))]
            {
                fs::write(&non_utf8, b"Never load this non-UTF-8 adapter identity.\n").unwrap();
                let alias = directory.path().join("UTF-8 adapter alias.oct");
                std::os::unix::fs::symlink(&non_utf8, &alias).unwrap();
                vec![non_utf8, alias]
            }
            #[cfg(target_os = "macos")]
            {
                vec![non_utf8]
            }
        };
        for variable in [
            "MATHLAB_SCOPE_BRIDGE",
            "MATHLAB_INPUT_BRIDGE",
            "MATHLAB_VALUE_BRIDGE",
        ] {
            for selected in &selected_paths {
                let mut child = command(&executable, directory.path(), directory.path(), None);
                child.env(variable, selected);
                let (output, report) = doctor_command(child);
                assert_not_ready(&output, &report);
                assert_path(&report, "runtime", &executable);
                let error = report["error"].as_str().unwrap();
                assert!(error.contains(variable) && error.contains("UTF-8 JSON"));
                assert!(
                    !String::from_utf8_lossy(&output.stderr).contains("panicked at"),
                    "{}",
                    diagnostic(&output)
                );
            }
        }
    }
}

#[test]
fn doctor_rejects_an_invalid_explicit_runtime_without_discovery_fallback() {
    let directory = directory();
    let missing = directory.path().join("missing selected octave");
    let bad = directory.path().join("not an executable octave");
    fs::write(&bad, b"This is deliberately not an executable.\n").unwrap();
    for executable in [&missing, &bad] {
        let (output, report) = doctor(executable, directory.path(), None);
        assert_not_ready(&output, &report);
    }
}

#[test]
fn doctor_reports_invalid_working_directories_as_structured_failures() {
    let directory = directory();
    let missing = directory.path().join("missing project folder");
    let regular_file = directory
        .path()
        .join("regular file is not a project folder");
    fs::write(&regular_file, b"not a folder\n").unwrap();
    // Neither case should reach runtime selection. Keeping the process cwd
    // valid also prevents spawn failure from disguising the CLI's result.
    let runtime = Path::new("unreachable-runtime-not-needed-for-directory-validation");
    for requested in [&missing, &regular_file] {
        let (output, report) = doctor_command(command(runtime, requested, directory.path(), None));
        assert_not_ready(&output, &report);
        assert!(report["error"]
            .as_str()
            .unwrap()
            .contains("working directory"));
        assert_eq!(
            report["working_directory"].as_str(),
            Some(requested.to_str().unwrap())
        );
        assert_eq!(report["working_directory_path_utf8"], true);
        assert_eq!(report.get("runtime"), Some(&Value::Null));
    }
}

#[cfg(unix)]
#[test]
fn doctor_reports_non_utf8_runtime_paths_without_panicking_or_launching_them() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    #[cfg(not(target_os = "macos"))]
    use std::os::unix::fs::PermissionsExt;

    let directory = directory();
    let missing = directory
        .path()
        .join(OsString::from_vec(b"missing-runtime-\xff".to_vec()));
    let executable_junk = directory
        .path()
        .join(OsString::from_vec(b"executable-runtime-\xfe".to_vec()));
    #[cfg(not(target_os = "macos"))]
    {
        fs::write(&executable_junk, b"deliberately invalid executable\n").unwrap();
        fs::set_permissions(&executable_junk, fs::Permissions::from_mode(0o700)).unwrap();
    }

    for selected in [&missing, &executable_junk] {
        let (output, report) = doctor(selected, directory.path(), None);
        assert_not_ready(&output, &report);
        assert_eq!(report["requested_runtime_path_utf8"], false);
        assert_eq!(
            report["requested_runtime"].as_str(),
            Some(selected.to_string_lossy().as_ref())
        );
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("panicked at"),
            "{}",
            diagnostic(&output)
        );
        if selected == &executable_junk && executable_junk.is_file() {
            assert_eq!(report["runtime_path_utf8"], false);
            assert!(report["error"]
                .as_str()
                .unwrap()
                .contains("UTF-8 runtime path"));
        }
    }
}

#[cfg(unix)]
#[test]
fn doctor_loads_bare_path_alias_and_relative_alias_as_the_selected_canonical_runtime() {
    let Some(executable) = runtime() else { return };
    let directory = directory();
    let bin = directory.path().join("owned path entries");
    fs::create_dir(&bin).unwrap();
    let bare_name = "owned octave alias";
    std::os::unix::fs::symlink(&executable, bin.join(bare_name)).unwrap();
    let relative_name = "relative octave alias";
    std::os::unix::fs::symlink(&executable, directory.path().join(relative_name)).unwrap();

    let mut path_entries = vec![bin];
    if let Some(inherited) = std::env::var_os("PATH") {
        path_entries.extend(std::env::split_paths(&inherited));
    }
    let search_path = std::env::join_paths(path_entries).unwrap();
    let mut bare = command(
        Path::new(bare_name),
        directory.path(),
        directory.path(),
        None,
    );
    bare.env("PATH", search_path);
    let (output, report) = doctor_command(bare);
    assert!(output.status.success(), "{}", diagnostic(&output));
    assert_ready(&report, &executable, None);

    let relative = Path::new(".").join(relative_name);
    let (output, report) =
        doctor_command(command(&relative, directory.path(), directory.path(), None));
    assert!(output.status.success(), "{}", diagnostic(&output));
    assert_ready(&report, &executable, None);
}

#[test]
fn doctor_observes_the_configured_native_adapter_and_its_capability() {
    let Some(adapter) = configured_value_adapter() else {
        return;
    };
    let executable = runtime().expect("a configured native adapter requires an available runtime");
    let directory = directory();
    let selected = selected_runtime_alias(&executable, directory.path());
    // The explicit dot also verifies that a configured path is reported from
    // Engine's canonical installation, rather than copied from the environment.
    let selected_adapter = adapter
        .parent()
        .unwrap()
        .join(".")
        .join(adapter.file_name().unwrap());
    let (output, report) = doctor(&selected, directory.path(), Some(&selected_adapter));
    assert!(output.status.success(), "{}", diagnostic(&output));
    assert_ready(&report, &executable, Some(&adapter));
}
