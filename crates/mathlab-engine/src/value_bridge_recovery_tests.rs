// SPDX-License-Identifier: GPL-3.0-or-later
//! Private WORKER-copy fault cases for native loader recovery. No production
//! hook, callback, native module copy or process-global environment mutation.

use super::*;
use crate::value_edit::{EditError, LeafDescriptor, RangeEditRequest};
use std::{
    fs, thread,
    time::{Duration, Instant},
};

#[test]
fn retained_value_bridge_path_without_current_capability_is_not_successful_installation() {
    let Some(executable) = discover_octave() else {
        assert_ne!(
            std::env::var("MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS").as_deref(),
            Ok("1"),
            "required value-bridge state test needs installed Octave"
        );
        eprintln!("SKIP retained value path test: no installed Octave.");
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let mut engine = Engine::start(EngineConfig {
        executable,
        working_directory: directory.path().to_owned(),
    })
    .unwrap();
    assert!(engine.is_transport_open());
    let candidate = directory.path().join("retained-value-adapter.oct");
    fs::write(&candidate, []).unwrap();
    let candidate = fs::canonicalize(candidate).unwrap();
    // Model the state after restart accepted an ordinary Ready but adapter
    // reinstallation failed: the configured path survives, capability does not.
    engine.value_bridge = Some(candidate.clone());
    if !engine
        .capabilities()
        .supports("dense_complex_native_components.v1")
    {
        engine
            .capabilities
            .as_mut()
            .unwrap()
            .capabilities
            .push("dense_complex_native_components.v1".into());
    }
    let expected_other_capabilities: Vec<_> = engine
        .capabilities()
        .capabilities
        .iter()
        .filter(|capability| *capability != "dense_complex_native_components.v1")
        .cloned()
        .collect();
    engine.retire_value_bridge_capability();
    engine.retire_value_bridge_capability();
    assert!(
        engine.is_transport_open(),
        "capability retirement alone does not close transport"
    );
    assert_eq!(
        engine.capabilities().capabilities,
        expected_other_capabilities,
        "retirement is idempotent and preserves other negotiated capabilities"
    );
    let next_id = engine.next_id;
    assert!(matches!(
        engine.enable_value_bridge(&candidate),
        Err(Error::Protocol(message)) if message.contains("dense_complex_native_components.v1")
    ));
    assert_eq!(engine.next_id, next_id, "no implicit load retry is allowed");
    assert_eq!(engine.value_bridge_path(), Some(candidate.as_path()));
}

#[test]
fn failed_recovery_loader_closes_transport_and_latches_a_postcommit_edit_as_uncertain() {
    let Some(executable) = discover_octave() else {
        assert_ne!(
            std::env::var("MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS").as_deref(),
            Ok("1"),
            "required value-bridge recovery test needs installed Octave"
        );
        eprintln!("SKIP failed recovery loader test: no installed Octave.");
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let reached = directory.path().join("value-bridge-postassign");
    let reached_temporary = directory.path().join("value-bridge-postassign.pending");
    let release = directory.path().join("value-bridge-release");
    let ready_anchor = "## MATHLAB_VALUE_BRIDGE_READY_VALIDATION";
    let assign_anchor = "## MATHLAB_VALUE_EDIT_POSTASSIGN_BOUNDARY";
    assert_eq!(WORKER.matches(ready_anchor).count(), 1);
    assert_eq!(WORKER.matches(assign_anchor).count(), 1);
    let failure = format!(
        "{ready_anchor}\n      if (invocation>1); builtin('error','MathLab:ValueBridgeProtocol','private failed recovery loader'); endif;"
    );
    let worker = WORKER.replacen(ready_anchor, &failure, 1);
    let gate = format!(
        "{assign_anchor}\n            value_bridge_test_file=builtin('fopen',{},'w'); builtin('fprintf',value_bridge_test_file,'%u',builtin('evalin','base','A')); builtin('fclose',value_bridge_test_file); builtin('rename',{},{}); while (! builtin('exist',{},'file')); builtin('pause',0.005); endwhile;",
        octave_string(&reached_temporary).unwrap(),
        octave_string(&reached_temporary).unwrap(),
        octave_string(&reached).unwrap(),
        octave_string(&release).unwrap()
    );
    let worker = worker.replacen(assign_anchor, &gate, 1);
    let mut engine = Engine::start_with_worker(
        EngineConfig {
            executable,
            working_directory: directory.path().to_owned(),
        },
        &worker,
    )
    .unwrap();
    assert!(engine.is_transport_open());
    let initial = engine.execute("A=uint64(1);").unwrap();
    assert!(initial.error.is_none(), "{:?}", initial.error);
    let token = engine
        .workspace_snapshot()
        .unwrap()
        .variables
        .into_iter()
        .find(|binding| binding.variable.name == "A")
        .unwrap()
        .token;
    let request = RangeEditRequest::from_components(
        ValuePath {
            root: token,
            steps: Vec::new(),
        },
        PageWindow {
            row_start: 1.into(),
            column_start: 1.into(),
            rows: 1,
            columns: 1,
            higher_indices: Vec::new(),
        },
        LeafDescriptor {
            class: NumericClass::Uint64,
            source_shape: vec![1.into(), 1.into()],
            source_storage: SourceStorage::Dense,
            complex: false,
        },
        &NumericComponents::Uint64(vec![99]),
        None,
    )
    .unwrap();
    let retry = request.clone();
    let cancellation = engine.cancellation_handle();
    struct Gate {
        cancellation: CancellationHandle,
        release: PathBuf,
    }
    impl Drop for Gate {
        fn drop(&mut self) {
            let _ = fs::write(&self.release, []);
            let _ = self.cancellation.terminate();
        }
    }
    let cleanup = Gate {
        cancellation: cancellation.clone(),
        release,
    };
    let task = thread::spawn(move || {
        let result = engine.write_value_range_checked(&request);
        (engine, result)
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    while !reached.exists() {
        assert!(
            Instant::now() < deadline,
            "edit never reached postassignment barrier"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        fs::read_to_string(&reached).unwrap(),
        "99",
        "the root was already committed before interruption"
    );
    cancellation.interrupt().unwrap();
    let deadline = Instant::now() + Duration::from_secs(25);
    while !task.is_finished() {
        assert!(Instant::now() < deadline, "failed recovery did not settle");
        thread::sleep(Duration::from_millis(5));
    }
    let (mut engine, result) = task.join().unwrap();
    let error = result.unwrap_err();
    assert!(
        matches!(&error, Error::ValueEdit(EditError::UncertainCommit(message)) if message.contains("MathLab:ValueBridgeProtocol") && message.contains("private failed recovery loader")),
        "{error:?}"
    );
    assert!(engine.has_value_edit_uncertainty());
    assert!(
        !engine.is_transport_open(),
        "the caller must observe closed transport even through an uncertain-edit error"
    );
    let next_id = engine.next_id;
    assert!(matches!(
        engine.send_request(json!({"command":"ping"}), false),
        Err(Error::Protocol(message)) if message.contains("transport is closed")
    ));
    assert_eq!(
        engine.next_id, next_id,
        "closed transport cannot consume a new request ID"
    );
    assert!(!engine.session.path().join("request.tmp").exists());
    assert!(!engine.session.path().join("request.json").exists());
    assert!(
        !engine
            .capabilities()
            .supports("dense_complex_native_components.v1"),
        "failed recovery must retire the old native capability"
    );
    if let Some(configured_path) = engine.value_bridge_path().map(ToOwned::to_owned) {
        let next_id = engine.next_id;
        assert!(matches!(
            engine.enable_value_bridge(&configured_path),
            Err(Error::Protocol(message)) if message.contains("dense_complex_native_components.v1")
        ));
        assert_eq!(
            engine.next_id, next_id,
            "failed recovery cannot retry loading"
        );
    }
    assert!(
        engine.input.handle().pending().is_err(),
        "failed recovery must close independent input"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let exited = engine
            .child
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .try_wait()
            .unwrap()
            .is_some();
        if exited {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "failed readiness left Octave at an idle prompt"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(
        engine.write_value_range_checked(&retry),
        Err(Error::ValueEdit(EditError::UncertainCommit(_)))
    ));
    assert!(
        !engine.session.path().join("request.json").exists(),
        "uncertain edit was automatically resubmitted"
    );
    // The injected loader fault applies only to a recovery invocation. A full
    // restart uses invocation1 and must independently reinstall configured paths.
    engine.restart().unwrap();
    assert!(!engine.has_value_edit_uncertainty());
    assert!(engine.is_transport_open());
    if engine.value_bridge_path().is_some() {
        engine
            .require_capability("dense_complex_native_components.v1")
            .unwrap();
    }
    let result = engine.execute("after_failed_value_recovery=1;").unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
    drop(cleanup);
}
