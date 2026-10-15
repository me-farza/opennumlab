// SPDX-License-Identifier: GPL-3.0-or-later
//! Lifecycle tests for the optional dense-complex value adapter.
//! Native cases require MATHLAB_VALUE_BRIDGE to a matching .oct.

use mathlab_engine::{
    ConsoleEventKind, Engine, EngineConfig, Error, NumericClass, NumericComponents, PageData,
    PageWindow, ValuePage, ValuePageRequest, ValuePath, discover_octave,
};
use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

const CAPABILITY: &str = "dense_complex_native_components.v1";

fn session(native_required: bool) -> Option<(Engine, tempfile::TempDir)> {
    if native_required && std::env::var_os("MATHLAB_VALUE_BRIDGE").is_none() {
        assert_ne!(
            std::env::var("MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS").as_deref(),
            Ok("1"),
            "required value bridge tests need MATHLAB_VALUE_BRIDGE"
        );
        eprintln!("SKIP native value adapter lifecycle: set MATHLAB_VALUE_BRIDGE.");
        return None;
    }
    let Some(executable) = discover_octave() else {
        assert_ne!(
            std::env::var("MATHLAB_REQUIRE_VALUE_BRIDGE_TESTS").as_deref(),
            Ok("1"),
            "required value bridge tests need an installed Octave"
        );
        eprintln!("SKIP value adapter lifecycle: no installed Octave.");
        return None;
    };
    let directory = tempfile::Builder::new()
        .prefix("mathlab-value-bridge-")
        .tempdir()
        .unwrap();
    let engine = Engine::start(EngineConfig {
        executable,
        working_directory: directory.path().to_owned(),
    })
    .expect("configured optional adapters must complete startup");
    Some((engine, directory))
}

fn execute(engine: &mut Engine, source: &str) {
    let result = engine.execute(source).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
}

fn read_complex(engine: &mut Engine) -> ValuePage {
    let token = engine
        .workspace_snapshot()
        .unwrap()
        .variables
        .into_iter()
        .find(|binding| binding.variable.name == "Z")
        .unwrap()
        .token;
    let request = ValuePageRequest {
        path: ValuePath {
            root: token,
            steps: Vec::new(),
        },
        window: PageWindow {
            row_start: 1.into(),
            column_start: 1.into(),
            rows: 1,
            columns: 2,
            higher_indices: Vec::new(),
        },
    };
    let page = engine.read_value_page_checked(&request).unwrap();
    assert_eq!(page.token, request.path.root);
    assert_eq!(page.class, NumericClass::Single);
    assert!(page.complex);
    assert!(matches!(&page.data, PageData::Dense {
        real: NumericComponents::Single(real), imag: Some(NumericComponents::Single(imag))
    } if real.as_slice() == [1.0, 2.0] && imag.as_slice() == [3.0, 4.0]));
    page
}

#[test]
fn optional_value_bridge_startup_and_restart_refresh_capabilities() {
    let Some((mut engine, _directory)) = session(true) else {
        return;
    };
    let expected = fs::canonicalize(PathBuf::from(
        std::env::var_os("MATHLAB_VALUE_BRIDGE").unwrap(),
    ))
    .unwrap();
    assert_eq!(engine.value_bridge_path(), Some(expected.as_path()));
    engine.require_capability(CAPABILITY).unwrap();
    engine.require_capability("typed_nd_page.v1").unwrap();
    engine.require_capability("atomic_range_edit.v1").unwrap();
    execute(&mut engine, "Z=complex(single([1,2]),single([3,4]));");
    let before = read_complex(&mut engine);
    let context = engine.workspace_snapshot().unwrap().context;
    engine.enable_value_bridge(&expected).unwrap();
    assert_eq!(
        engine.workspace_snapshot().unwrap().context,
        context,
        "an idempotent enable must not enqueue another effectful installation"
    );

    engine.restart().unwrap();
    assert_eq!(engine.value_bridge_path(), Some(expected.as_path()));
    engine.require_capability(CAPABILITY).unwrap();
    let fresh = engine.workspace_snapshot().unwrap();
    assert!(fresh.context.generation > before.token.context.generation);
    assert!(
        !fresh
            .variables
            .iter()
            .any(|binding| binding.variable.name == "Z")
    );
    execute(&mut engine, "Z=complex(single([1,2]),single([3,4]));");
    let after = read_complex(&mut engine);
    assert_ne!(after.token.context, before.token.context);
}

#[cfg(unix)]
#[test]
fn optional_value_bridge_stop_restores_the_captured_helper_in_the_same_process() {
    let Some((engine, _directory)) = session(true) else {
        return;
    };
    let path = engine.value_bridge_path().unwrap().to_owned();
    let pid = engine.process_id();
    let cancel = engine.cancellation_handle();
    struct TerminateOnDrop(Option<mathlab_engine::CancellationHandle>);
    impl Drop for TerminateOnDrop {
        fn drop(&mut self) {
            if let Some(cancel) = &self.0 {
                let _ = cancel.terminate();
            }
        }
    }
    let mut guard = TerminateOnDrop(Some(cancel.clone()));
    let stream = engine.subscribe_console();
    let task = thread::spawn(move || {
        let mut engine = engine;
        let result = engine.execute_with_timeout(
            "value_stop_marker=19; Z=complex(single([1,2]),single([3,4])); clear functions; fprintf('VALUE_BRIDGE_STOP_READY'); while (true); pause(.01); endwhile;",
            Duration::from_secs(20));
        (engine, result)
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut bytes = Vec::new();
    while !bytes
        .windows(b"VALUE_BRIDGE_STOP_READY".len())
        .any(|part| part == b"VALUE_BRIDGE_STOP_READY")
    {
        let event = stream
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        if let ConsoleEventKind::Output { bytes: chunk } = event.kind {
            bytes.extend(chunk);
        }
        assert!(
            bytes.len() < 64 * 1024,
            "unexpected unbounded output before causal barrier"
        );
    }
    cancel.interrupt().unwrap();
    let (mut engine, result) = task.join().unwrap();
    guard.0 = None;
    let result = result.unwrap();
    assert_eq!(result.error.unwrap().identifier, "MathLab:Interrupted");
    assert_eq!(engine.process_id(), pid);
    assert_eq!(engine.value_bridge_path(), Some(path.as_path()));
    engine.require_capability(CAPABILITY).unwrap();
    let page = read_complex(&mut engine);
    assert!(page.token.context.invocation > 1);
    execute(&mut engine, "assert(value_stop_marker==19);");
    let _ = read_complex(&mut engine);
}

#[test]
fn value_bridge_invalid_paths_and_configured_hot_swap_fail_without_dispatch() {
    let Some((mut engine, directory)) = session(false) else {
        return;
    };
    let expected_path = engine.value_bridge_path().map(ToOwned::to_owned);
    let expected_capability = engine.capabilities().supports(CAPABILITY);
    let context = engine.workspace_snapshot().unwrap().context;
    assert!(matches!(
        engine.enable_value_bridge(&directory.path().join("missing.oct")),
        Err(Error::Io(_))
    ));
    let wrong_suffix = directory.path().join("not-a-module.txt");
    fs::write(&wrong_suffix, []).unwrap();
    assert!(matches!(
        engine.enable_value_bridge(&wrong_suffix),
        Err(Error::Protocol(_))
    ));
    if expected_path.is_some() {
        // This file must be rejected by the configured-path guard before the
        // worker can attempt to load it; it is deliberately not a native copy.
        let other = directory.path().join("another-value-adapter.oct");
        fs::write(&other, []).unwrap();
        assert!(
            matches!(engine.enable_value_bridge(&other), Err(Error::Protocol(message)) if message.contains("already loaded"))
        );
    }
    assert_eq!(engine.value_bridge_path(), expected_path.as_deref());
    assert_eq!(
        engine.capabilities().supports(CAPABILITY),
        expected_capability
    );
    assert_eq!(engine.workspace_snapshot().unwrap().context, context);
    execute(&mut engine, "after_bad_value_path=7;");
}
