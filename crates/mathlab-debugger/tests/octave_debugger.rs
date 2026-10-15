use anyhow::{bail, Context, Result};
use mathlab_debugger::{DebugState, Debugger, ResumeAction};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn quote(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

fn wait_pause(debugger: &Debugger, line: usize, after_revision: u64) -> Result<DebugState> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(state) = debugger.state()? {
            if state.paused && state.line == line && state.revision > after_revision {
                return Ok(state);
            }
        }
        if Instant::now() >= deadline {
            bail!("Did not pause at line {line}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn real_breakpoint_scopes_step_and_continue() -> Result<()> {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        eprintln!("Skipped native debugger proof: build the adapter and set MATHLAB_DEBUG_BRIDGE");
        return Ok(());
    };
    let bridge = PathBuf::from(bridge)
        .canonicalize()
        .context("Find debugger adapter")?;
    let octave = std::env::var_os("MATHLAB_OCTAVE").unwrap_or_else(|| "octave-cli".into());
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    let script = root.join("proof.m");
    fs::write(
        &script,
        "function y = proof(x)\n a = x + 1;\n y = a * 2;\nend\n",
    )?;
    let driver = format!(
        "addpath('{}'); sentinel=123; mathlab_debug_bridge('{}'); while(!exist('{}','file')); mathlab_debug_bridge('poll'); pause(.02); endwhile; result=proof(5); assert(result==12); assert(sentinel==123); mathlab_debug_bridge();",
        quote(bridge.parent().unwrap()),
        quote(&root),
        quote(&root.join("start"))
    );
    let mut process = Process(
        Command::new(octave)
            .args(["--quiet", "--no-init-file", "--eval", &driver])
            .current_dir(&root)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?,
    );
    let debugger = Debugger::new(&root);
    assert!(debugger
        .set_breakpoint(&root.join("missing.m"), 2)
        .unwrap_err()
        .to_string()
        .contains("existing absolute .m file"));
    assert_eq!(debugger.set_breakpoint(&script, 2)?, vec![2]);
    fs::write(root.join("start"), "start")?;
    let state = wait_pause(&debugger, 2, 0)?;
    assert_eq!(state.file, script);
    let variables = debugger.workspace(state.revision)?;
    assert!(variables.iter().any(|v| v.name == "x"));
    assert!(!variables.iter().any(|v| v.name == "sentinel"));
    assert!(debugger
        .stack(state.revision)?
        .iter()
        .any(|f| f.name == "proof" && f.line == 2));
    assert_eq!(debugger.evaluate("disp(x);", state.revision)?, "5\n");
    assert!(debugger
        .evaluate("disp(x);", state.revision + 100)
        .unwrap_err()
        .to_string()
        .contains("scope changed"));
    debugger.resume(ResumeAction::Step, state.revision)?;
    let next = wait_pause(&debugger, 3, state.revision)?;
    assert_eq!(debugger.evaluate("disp(a);", next.revision)?, "6\n");
    let streamed = debugger.evaluate_output("fprintf('live:%g\\n', a);", next.revision)?;
    assert!(streamed.streamed);
    assert!(!streamed.truncated);
    assert_eq!(streamed.output, "live:6\n");
    let clipped =
        debugger.evaluate_output("fprintf('%s', repmat('x', 1, 1024*1024+1));", next.revision)?;
    assert!(clipped.streamed && clipped.truncated);
    assert_eq!(
        clipped.output.bytes().filter(|byte| *byte == b'x').count(),
        1024 * 1024
    );
    assert!(clipped
        .output
        .ends_with("[MathLab: returned evaluation output clipped at 1 MiB]\n"));
    let unicode_clipped = debugger.evaluate_output(
        "fprintf('%s', [repmat('x', 1, 1024*1024-1), char([206,187])]);",
        next.revision,
    )?;
    assert!(unicode_clipped.truncated);
    assert!(unicode_clipped.output.contains('\u{fffd}'));
    debugger.resume(ResumeAction::Continue, next.revision)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = process.0.try_wait()? {
            assert!(status.success(), "Octave debugger process failed: {status}");
            break;
        }
        if Instant::now() >= deadline {
            bail!("Octave did not finish after continue");
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!debugger.state()?.unwrap().paused);
    Ok(())
}

#[test]
fn guarded_debug_quit_preserves_assignments_and_allows_more_commands() -> Result<()> {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        eprintln!("Skipped native quit proof: MATHLAB_DEBUG_BRIDGE is not set");
        return Ok(());
    };
    let bridge = PathBuf::from(bridge).canonicalize()?;
    let octave = std::env::var_os("MATHLAB_OCTAVE").unwrap_or_else(|| "octave-cli".into());
    for operation in ["source", "evaluate"] {
        let directory = tempfile::tempdir()?;
        let root = directory.path().canonicalize()?;
        let script = root.join("quit_proof.m");
        fs::write(&script, "before_quit=7;\nshould_not_execute=99;\n")?;
        let source = if operation == "source" {
            script.to_string_lossy().into_owned()
        } else {
            format!("source('{}','base');", quote(&script))
        }
        .replace('\'', "''");
        let driver = format!(
            "addpath('{}'); mathlab_debug_bridge('{}'); dbstop('quit_proof','2'); completed=mathlab_debug_bridge('{operation}','{source}'); assert(!completed); assert(before_quit==7); assert(!exist('should_not_execute','var')); dbclear all; assert(mathlab_debug_bridge('evaluate','after_quit=42;')); assert(after_quit==42); mathlab_debug_bridge();",
            quote(bridge.parent().unwrap()),
            quote(&root)
        );
        let mut process = Process(
            Command::new(&octave)
                .args(["--quiet", "--no-init-file", "--eval", &driver])
                .current_dir(&root)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()?,
        );
        let debugger = Debugger::new(&root);
        let state = wait_pause(&debugger, 2, 0)?;
        debugger.resume(ResumeAction::Quit, state.revision)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = process.0.try_wait()? {
                assert!(
                    status.success(),
                    "Guarded {operation} debugger quit failed: {status}"
                );
                break;
            }
            if Instant::now() >= deadline {
                bail!("Guarded quit did not finish");
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!debugger.state()?.unwrap().paused);
    }
    Ok(())
}

#[test]
fn step_into_and_out_preserve_function_scopes() -> Result<()> {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        eprintln!("Skipped native stepping proof: MATHLAB_DEBUG_BRIDGE is not set");
        return Ok(());
    };
    let bridge = PathBuf::from(bridge).canonicalize()?;
    let octave = std::env::var_os("MATHLAB_OCTAVE").unwrap_or_else(|| "octave-cli".into());
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    let outer = root.join("outer.m");
    let inner = root.join("inner.m");
    fs::write(
        &outer,
        "function result=outer(x)\n input=x+1;\n intermediate=inner(input);\n result=intermediate+1;\nend\n",
    )?;
    fs::write(&inner, "function value=inner(x)\n value=x*2;\nend\n")?;
    let driver = format!(
        "addpath('{}'); mathlab_debug_bridge('{}'); dbstop('outer','3'); result=outer(5); assert(result==13); mathlab_debug_bridge();",
        quote(bridge.parent().unwrap()),
        quote(&root)
    );
    let mut process = Process(
        Command::new(octave)
            .args(["--quiet", "--no-init-file", "--eval", &driver])
            .current_dir(&root)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?,
    );
    let debugger = Debugger::new(&root);
    let state = wait_pause(&debugger, 3, 0)?;
    assert_eq!(state.file, outer);
    debugger.resume(ResumeAction::StepIn, state.revision)?;
    let inside = wait_pause(&debugger, 2, state.revision)?;
    assert_eq!(inside.file, inner);
    assert_eq!(debugger.evaluate("disp(x);", inside.revision)?, "6\n");
    let frames = debugger.stack(inside.revision)?;
    assert!(frames.iter().any(|frame| frame.name == "inner"));
    assert!(frames.iter().any(|frame| frame.name == "outer"));
    debugger.resume(ResumeAction::StepOut, inside.revision)?;
    let outside = wait_pause(&debugger, 4, inside.revision)?;
    assert_eq!(outside.file, outer);
    assert_eq!(
        debugger.evaluate("disp(intermediate);", outside.revision)?,
        "12\n"
    );
    debugger.resume(ResumeAction::Continue, outside.revision)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = process.0.try_wait()? {
            assert!(status.success(), "Step into/out process failed: {status}");
            break;
        }
        if Instant::now() >= deadline {
            bail!("Stepping process did not finish");
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

#[test]
fn nested_debug_evaluation_can_resume_and_restores_enclosing_pause() -> Result<()> {
    let Some(bridge) = std::env::var_os("MATHLAB_DEBUG_BRIDGE") else {
        eprintln!("Skipped nested debugger proof: MATHLAB_DEBUG_BRIDGE is not set");
        return Ok(());
    };
    let bridge = PathBuf::from(bridge).canonicalize()?;
    let octave = std::env::var_os("MATHLAB_OCTAVE").unwrap_or_else(|| "octave-cli".into());
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    let outer = root.join("outer.m");
    let inner = root.join("inner.m");
    fs::write(
        &outer,
        "function result=outer(x)\n input=x+1;\n intermediate=inner(input);\n result=intermediate+1;\nend\n",
    )?;
    fs::write(&inner, "function value=inner(x)\n value=x*2;\nend\n")?;
    let driver = format!(
        "addpath('{}'); mathlab_debug_bridge('{}'); dbstop('outer','3'); result=outer(5); assert(result==13); mathlab_debug_bridge();",
        quote(bridge.parent().unwrap()),
        quote(&root)
    );
    let mut process = Process(
        Command::new(octave)
            .args(["--quiet", "--no-init-file", "--eval", &driver])
            .current_dir(&root)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?,
    );
    let debugger = Debugger::new(&root);
    let original = wait_pause(&debugger, 3, 0)?;
    assert_eq!(debugger.set_breakpoint(&inner, 2)?, vec![2]);
    let evaluation_client = debugger.clone();
    let evaluation =
        thread::spawn(move || evaluation_client.evaluate("disp(inner(20));", original.revision));
    let nested = wait_pause(&debugger, 2, original.revision)?;
    assert_eq!(nested.file, inner);
    assert_eq!(debugger.evaluate("disp(x);", nested.revision)?, "20\n");
    debugger.resume(ResumeAction::Continue, nested.revision)?;
    let output = evaluation.join().unwrap()?;
    assert!(output.ends_with("40\n"), "{output}");
    let enclosing = wait_pause(&debugger, 3, nested.revision)?;
    assert_eq!(enclosing.file, outer);
    assert_eq!(debugger.evaluate("disp(x);", enclosing.revision)?, "5\n");
    // A watch expression can pause inside a function and then throw after it
    // returns. The enclosing debugger scope must still be available afterward.
    let evaluation_client = debugger.clone();
    let failed_evaluation = thread::spawn(move || {
        evaluation_client.evaluate(
            "inner(21); error('MathLab:WatchFailure', 'watch expression failed');",
            enclosing.revision,
        )
    });
    let nested_failure = wait_pause(&debugger, 2, enclosing.revision)?;
    assert_eq!(
        debugger.evaluate("disp(x);", nested_failure.revision)?,
        "21\n"
    );
    debugger.resume(ResumeAction::Continue, nested_failure.revision)?;
    assert!(failed_evaluation
        .join()
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("watch expression failed"));
    let restored = wait_pause(&debugger, 3, nested_failure.revision)?;
    assert_eq!(restored.file, outer);
    assert_eq!(debugger.evaluate("disp(x);", restored.revision)?, "5\n");
    debugger.clear_breakpoint(&inner, 2)?;
    debugger.resume(ResumeAction::Continue, restored.revision)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = process.0.try_wait()? {
            assert!(status.success(), "Nested debugger process failed: {status}");
            break;
        }
        if Instant::now() >= deadline {
            bail!("Nested debugger did not finish");
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
