// SPDX-License-Identifier: GPL-3.0-or-later
//! Real-process fault injection at the worker's console commit boundaries.
use super::*;
use std::thread;

struct Barrier {
    reached: PathBuf,
    release: PathBuf,
    cancellation: CancellationHandle,
    _directory: TempDir,
}

impl Drop for Barrier {
    fn drop(&mut self) {
        let _ = fs::write(&self.release, []);
        let _ = self.cancellation.terminate();
    }
}

#[derive(Clone, Copy)]
enum Stage {
    BeforeBegin,
    BeforeEnd,
    AfterEnd,
}

fn with_barrier(stage: Stage) -> Option<(Engine, Barrier)> {
    let executable = discover_octave()?;
    let directory = tempfile::tempdir().unwrap();
    let reached = directory.path().join("reached");
    let release = directory.path().join("release");
    let hook = format!(
        "file=fopen({},'w'); fclose(file); while(!exist({},'file')); builtin('pause',.005); endwhile;",
        octave_string(&reached).unwrap(),
        octave_string(&release).unwrap()
    );
    let worker = match stage {
        Stage::BeforeBegin => WORKER.replace(
            "        mathlab_begin (request_id, response_prefix, generation, console_helper_path, invocation);",
            &format!("        if (strcmp(request.command,'execute')); {hook} endif\n        mathlab_begin (request_id, response_prefix, generation, console_helper_path, invocation);")),
        Stage::BeforeEnd | Stage::AfterEnd => {
            let condition = "strcmp(kind,'E') && isstruct(response.result) && isfield(response.result,'duration_ms')";
            let call = "    mathlab_fence (kind, response.id, nonce, generation, console_helper_path, invocation);";
            let injected = format!("    if ({condition}); {hook} endif");
            let replacement = if matches!(stage,Stage::BeforeEnd) {format!("{injected}\n{call}")} else {format!("{call}\n{injected}")};
            WORKER.replace(call,&replacement)
        }
    };
    assert_ne!(
        worker, WORKER,
        "worker commit boundary moved; update the fault injector"
    );
    let engine = Engine::start_with_worker(
        EngineConfig {
            executable,
            working_directory: directory.path().to_owned(),
        },
        &worker,
    )
    .unwrap();
    let barrier = Barrier {
        reached,
        release,
        cancellation: engine.cancellation_handle(),
        _directory: directory,
    };
    Some((engine, barrier))
}

fn wait_reached(barrier: &Barrier) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !barrier.reached.exists() {
        assert!(
            Instant::now() < deadline,
            "worker did not reach its commit barrier"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

fn join_request(
    task: thread::JoinHandle<(Engine, Result<ExecutionResult>)>,
) -> (Engine, ExecutionResult) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !task.is_finished() {
        assert!(Instant::now() < deadline, "canceled request did not settle");
        thread::sleep(Duration::from_millis(5));
    }
    let (engine, result) = task.join().unwrap();
    (engine, result.unwrap())
}

#[test]
fn stop_before_begin_keeps_the_canceled_body_out_of_the_workspace() {
    let Some((mut engine, barrier)) = with_barrier(Stage::BeforeBegin) else {
        return;
    };
    let pid = engine.process_id();
    let task = thread::spawn(move || {
        let result = engine
            .execute_with_timeout("must_not_run=91; while true; end;", Duration::from_secs(15));
        (engine, result)
    });
    wait_reached(&barrier);
    barrier.cancellation.interrupt().unwrap();
    let (mut engine, result) = join_request(task);
    assert_eq!(result.error.unwrap().identifier, "MathLab:Interrupted");
    assert!(
        !result
            .workspace
            .iter()
            .any(|value| value.name == "must_not_run")
    );
    assert_eq!(engine.process_id(), pid);
    fs::write(&barrier.release, []).unwrap();
    assert!(
        engine
            .execute("assert(exist('must_not_run','var')==0); after_early_stop=1;")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn stop_after_metadata_before_end_keeps_partial_output_and_recovers() {
    let Some((mut engine, barrier)) = with_barrier(Stage::BeforeEnd) else {
        return;
    };
    let pid = engine.process_id();
    let metadata = engine.session.path().join(format!(
        "response-{}-{}.json",
        engine.invocation, engine.next_id
    ));
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "saved_before_end=19; fprintf(stdout,'METADATA_PREFIX');",
            Duration::from_secs(15),
        );
        (engine, result)
    });
    wait_reached(&barrier);
    let normal: Response = serde_json::from_slice(&fs::read(&metadata).unwrap()).unwrap();
    assert!(normal.ok);
    let normal_value: Value = serde_json::from_str(normal.result.get()).unwrap();
    assert!(normal_value["error"].as_array().is_some_and(Vec::is_empty));
    barrier.cancellation.interrupt().unwrap();
    let (mut engine, result) = join_request(task);
    assert_eq!(result.error.unwrap().identifier, "MathLab:Interrupted");
    assert_eq!(result.output, "METADATA_PREFIX");
    assert_eq!(engine.process_id(), pid);
    fs::write(&barrier.release, []).unwrap();
    assert!(
        engine
            .execute("assert(saved_before_end==19); after_metadata_stop=1;")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn signal_winning_the_process_lock_after_end_preserves_one_result_and_settles_recovery() {
    let Some((mut engine, barrier)) = with_barrier(Stage::AfterEnd) else {
        return;
    };
    let pid = engine.process_id();
    let subscription = engine.subscribe_console();
    let child = engine.child.clone();
    // Hold deactivation before releasing the worker into this request. The
    // request's own send needs this lock, so first stage it through the sender.
    let request_id=engine.send_request(json!({"command":"execute","code":"saved_after_end=29; fprintf(stdout,'COMMITTED_PREFIX');"}),true).unwrap();
    let mut guard = child.lock().unwrap();
    let task = thread::spawn(move || {
        let result = engine
            .finish_request(request_id, Some(Duration::from_secs(15)))
            .and_then(|(value, capture)| engine.execution_result(value, capture));
        (engine, result)
    });
    wait_reached(&barrier);
    let mut committed_sequence = 0;
    while let Ok(event) = subscription.try_recv() {
        if matches!(event.kind, ConsoleEventKind::End) {
            committed_sequence = event.sequence;
        }
    }
    assert!(committed_sequence > 0);
    // Actual cancellation code wins the same mutex needed by End deactivation.
    barrier.cancellation.interrupt_locked(&mut guard).unwrap();
    drop(guard);
    let (mut engine, result) = join_request(task);
    assert!(
        result.error.is_none(),
        "committed result was replaced: {:?}",
        result.error
    );
    assert_eq!(result.output, "COMMITTED_PREFIX");
    assert!(result.console_end_sequence > committed_sequence);
    assert_eq!(engine.process_id(), pid);
    fs::write(&barrier.release, []).unwrap();
    assert!(
        engine
            .execute("assert(saved_after_end==29); after_committed_stop=1;")
            .unwrap()
            .error
            .is_none()
    );
}

struct ProcessGuard(CancellationHandle);
impl Drop for ProcessGuard {
    fn drop(&mut self) {
        let _ = signal(&self.0, libc::SIGCONT);
        let _ = self.0.terminate();
    }
}

fn signal(handle: &CancellationHandle, number: i32) -> Result<()> {
    let guard = handle
        .child
        .lock()
        .map_err(|_| Error::Protocol("process lock poisoned".into()))?;
    if let Some(child) = guard.as_ref()
        && unsafe { libc::kill(child.id() as libc::pid_t, number) } != 0
    {
        return Err(Error::Io(io::Error::last_os_error()));
    }
    Ok(())
}

fn plain_engine() -> Option<Engine> {
    Some(
        Engine::start(EngineConfig {
            executable: discover_octave()?,
            working_directory: std::env::current_dir().unwrap(),
        })
        .unwrap(),
    )
}

fn wait_active(handle: &CancellationHandle) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !handle.active.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline, "request was not staged");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn stop_while_worker_is_suspended_before_mailbox_consumption_does_not_run_canceled_source() {
    let Some(mut engine) = plain_engine() else {
        return;
    };
    let cancellation = engine.cancellation_handle();
    let _guard = ProcessGuard(cancellation.clone());
    let pid = engine.process_id();
    signal(&cancellation, libc::SIGSTOP).unwrap();
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "canceled_mailbox_body=71; while true;end;",
            Duration::from_secs(15),
        );
        (engine, result)
    });
    wait_active(&cancellation);
    cancellation.interrupt().unwrap();
    signal(&cancellation, libc::SIGCONT).unwrap();
    let (mut engine, result) = join_request(task);
    assert_eq!(result.error.unwrap().identifier, "MathLab:Interrupted");
    assert!(
        !result
            .workspace
            .iter()
            .any(|value| value.name == "canceled_mailbox_body")
    );
    assert_eq!(engine.process_id(), pid);
    assert!(
        engine
            .execute("assert(exist('canceled_mailbox_body','var')==0);")
            .unwrap()
            .error
            .is_none()
    );
}

#[test]
fn unrelated_idle_recovery_does_not_disarm_cancellation_for_the_queued_request() {
    let Some(mut engine) = plain_engine() else {
        return;
    };
    let cancellation = engine.cancellation_handle();
    let _guard = ProcessGuard(cancellation.clone());
    let subscription = engine.subscribe_console();
    let pid = engine.process_id();
    signal(&cancellation, libc::SIGSTOP).unwrap();
    // An external idle signal is pending before the next user request is staged.
    signal(&cancellation, libc::SIGINT).unwrap();
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(
            "queued_started=1; fprintf(stdout,'QUEUED_BODY_RUNNING');while true;end;",
            Duration::from_secs(15),
        );
        (engine, result)
    });
    wait_active(&cancellation);
    signal(&cancellation, libc::SIGCONT).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut output = Vec::new();
    while !output
        .windows(b"QUEUED_BODY_RUNNING".len())
        .any(|window| window == b"QUEUED_BODY_RUNNING")
    {
        if let Ok(event) = subscription.recv_timeout(Duration::from_millis(50))
            && let ConsoleEventKind::Output { bytes } = event.kind
        {
            output.extend(bytes);
        }
        assert!(
            Instant::now() < deadline,
            "queued body did not start after idle recovery"
        );
    }
    assert!(
        cancellation.active.load(Ordering::Acquire),
        "idle recovery disarmed the queued running body"
    );
    cancellation.interrupt().unwrap();
    let (mut engine, result) = join_request(task);
    assert_eq!(result.error.unwrap().identifier, "MathLab:Interrupted");
    assert_eq!(result.output, "QUEUED_BODY_RUNNING");
    assert!(
        result
            .workspace
            .iter()
            .any(|value| value.name == "queued_started")
    );
    assert_eq!(engine.process_id(), pid);
    assert!(
        engine
            .execute("assert(queued_started==1);")
            .unwrap()
            .error
            .is_none()
    );
}
