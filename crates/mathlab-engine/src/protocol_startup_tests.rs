// SPDX-License-Identifier: GPL-3.0-or-later
//! Real startup negotiation failures must precede any dependent user command.
use super::*;

#[test]
fn startup_rejects_wrong_protocol_capabilities_limits_and_context() {
    let Some(executable) = discover_octave() else {
        eprintln!("SKIP ordinary startup negotiation: Octave unavailable");
        return;
    };
    for (before, after) in [
        ("'name', 'mathlab-worker'", "'name', 'foreign-worker'"),
        (
            "'name', 'mathlab-worker', 'version', 1",
            "'name', 'mathlab-worker', 'version', 2",
        ),
        ("'checked_scalar_write.v1', ", ""),
        ("'variable_cells', 2000", "'variable_cells', 1999"),
        (
            "builtin('sprintf', '%.0f', generation)",
            "builtin('sprintf', '%.0f', generation + 1)",
        ),
    ] {
        let worker = WORKER.replace(before, after);
        assert_ne!(worker, WORKER, "Startup injection anchor moved: {before}");
        let directory = tempfile::tempdir().unwrap();
        let result = Engine::start_with_worker(
            EngineConfig {
                executable: executable.clone(),
                working_directory: directory.path().to_owned(),
            },
            &worker,
        );
        match result {
            Err(Error::Protocol(_)) => {}
            Err(error) => panic!("Expected protocol rejection for {before}, received {error}"),
            Ok(_) => panic!("Invalid worker advertised {before} was accepted"),
        }
    }
}

#[test]
fn pure_ping_preserves_tokens_without_calling_a_project_version_function() {
    let Some(executable) = discover_octave() else {
        eprintln!("SKIP intrinsic version negotiation: Octave unavailable");
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let mut engine = Engine::start(EngineConfig {
        executable,
        working_directory: directory.path().to_owned(),
    })
    .unwrap();
    let version = engine.version().to_owned();
    fs::write(directory.path().join("version.m"),
        "function out = version(varargin)\nassignin('base','version_hook_called',1);\nerror('MathLab:VersionHook','Project version function called.');\nendfunction\n").unwrap();
    assert!(
        engine
            .execute("rehash; clear version; A=5;")
            .unwrap()
            .error
            .is_none()
    );
    let snapshot = engine.workspace_snapshot().unwrap();
    let token = snapshot
        .variables
        .iter()
        .find(|entry| entry.variable.name == "A")
        .unwrap()
        .token
        .clone();
    let (ready, _) = engine
        .request(json!({"command":"ping"}), Some(COMMAND_TIMEOUT))
        .unwrap();
    assert_eq!(ready["version"], version);
    assert_eq!(
        serde_json::from_value::<SessionContext>(ready["context"].clone()).unwrap(),
        snapshot.context
    );
    assert_eq!(
        engine
            .read_variable_checked(&token, 1, 1, 1, 1)
            .unwrap()
            .slice
            .cells[0][0],
        "5"
    );
    let after = engine.workspace_snapshot().unwrap();
    assert_eq!(after.context, snapshot.context);
    assert!(
        !after
            .variables
            .iter()
            .any(|entry| entry.variable.name == "version_hook_called")
    );
    // Prove that ordinary user resolution still reaches the project function.
    let control = engine.execute("try; version(); catch caught; assert(strcmp(caught.identifier,'MathLab:VersionHook')); end;").unwrap();
    assert!(control.error.is_none());
    assert!(
        engine
            .workspace_snapshot()
            .unwrap()
            .variables
            .iter()
            .any(|entry| entry.variable.name == "version_hook_called")
    );
}
