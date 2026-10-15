use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use mathlab_engine::{discover_octave, Engine, EngineConfig, ExecutionResult};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

mod conformance;
mod stdin_input;
mod streaming;
use stdin_input::{InputResponder, InputSupport, StdinRouter};
use streaming::ConsolePrinter;

#[derive(Parser)]
#[command(
    name = "mathlab",
    version,
    about = "Open-source scientific computing with an Octave engine"
)]
struct Cli {
    /// Path to octave-cli or octave. Otherwise use MATHLAB_OCTAVE and PATH.
    #[arg(long, global = true)]
    octave: Option<PathBuf>,
    /// Start the computation session in this directory.
    #[arg(long, global = true)]
    directory: Option<PathBuf>,
    /// Print machine-readable results.
    #[arg(long, global = true)]
    json: bool,
    /// Limit execution in seconds. Compatibility checks default to 30 seconds per fixture.
    #[arg(long, global = true)]
    timeout: Option<std::num::NonZeroU64>,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Check the runtime and report installed packages.
    Doctor,
    /// Execute MATLAB/Octave code in a new session.
    Eval { code: String },
    /// Execute an existing .m script.
    Run {
        file: PathBuf,
        /// Display the resulting workspace.
        #[arg(long)]
        workspace: bool,
    },
    /// Start a persistent command-line session.
    Repl,
    /// Run the assertion fixtures listed in a compatibility manifest.
    Check { manifest: PathBuf },
    /// Compare shared typed fixtures through raw Octave, MathLab and an explicit reference.
    Conformance {
        manifest: PathBuf,
        /// Run a licensed MATLAB reference explicitly; never autodiscovered.
        #[arg(long, conflicts_with = "reference")]
        matlab: Option<PathBuf>,
        /// Replay an identified conformance report or independent bundle's process-report.json.
        #[arg(long)]
        reference: Option<PathBuf>,
        /// Save the complete report in addition to terminal output.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Fail acceptance when any comparison is unavailable or unsupported.
        #[arg(long)]
        strict: bool,
    },
    /// List packages available in the selected engine.
    Packages {
        /// Load a package and verify it can initialize.
        #[arg(long)]
        load: Option<String>,
    },
}

#[derive(Deserialize)]
struct Manifest {
    schema_version: u32,
    #[serde(default)]
    reference_profile: String,
    features: Vec<Feature>,
}

#[derive(Deserialize)]
struct Feature {
    id: String,
    fixture: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("mathlab: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let executable = cli.octave.clone().or_else(discover_octave);
    if matches!(cli.command, Action::Doctor) {
        return doctor(
            executable,
            cli.directory.as_deref().unwrap_or(Path::new(".")),
            cli.json,
        );
    }
    let directory = cli.directory.clone().unwrap_or(std::env::current_dir()?);
    let directory = directory
        .canonicalize()
        .context("The working directory does not exist")?;
    if !directory.is_dir() {
        bail!("The working directory must be a folder");
    }
    if let Action::Conformance {
        manifest,
        matlab,
        reference,
        output,
        strict,
    } = &cli.command
    {
        return conformance::run(conformance::Options {
            manifest,
            octave: executable.as_deref(),
            matlab: matlab.as_deref(),
            reference: reference.as_deref(),
            output: output.as_deref(),
            directory: &directory,
            as_json: cli.json,
            strict: *strict,
            timeout: std::time::Duration::from_secs(cli.timeout.map_or(30, |s| s.get())),
        });
    }
    let executable = executable.context(
        "Octave was not found. Install GNU Octave, set MATHLAB_OCTAVE, or pass --octave /path/to/octave-cli. See scripts/setup-runtime.sh.",
    )?;
    match cli.command {
        Action::Repl => repl(
            Engine::start(EngineConfig {
                executable,
                working_directory: directory,
            })?,
            cli.json,
            cli.timeout
                .map(|seconds| std::time::Duration::from_secs(seconds.get())),
        ),
        Action::Eval { code } => {
            let mut engine = Engine::start(EngineConfig {
                executable,
                working_directory: directory.clone(),
            })?;
            let printer = start_printer(&engine, cli.json)?;
            let _input =
                InputSupport::start(&engine, printer.as_ref().map(ConsolePrinter::cursor))?;
            let outcome = match cli.timeout {
                Some(seconds) => engine
                    .execute_with_timeout(&code, std::time::Duration::from_secs(seconds.get())),
                None => engine.execute(&code),
            };
            let mut result = finish_execution(printer.as_ref(), outcome)?;
            preserve_figures(&mut result, &directory)?;
            print_result(&result, cli.json, true, printer.is_some())?;
            fail_on_execution_error(&result)
        }
        Action::Run { file, workspace } => {
            let file = file.canonicalize().context("The script does not exist")?;
            let mut engine = Engine::start(EngineConfig {
                executable,
                working_directory: directory.clone(),
            })?;
            let printer = start_printer(&engine, cli.json)?;
            let _input =
                InputSupport::start(&engine, printer.as_ref().map(ConsolePrinter::cursor))?;
            let outcome = match cli.timeout {
                Some(seconds) => engine
                    .run_file_with_timeout(&file, std::time::Duration::from_secs(seconds.get())),
                None => engine.run_file(&file),
            };
            let mut result = finish_execution(printer.as_ref(), outcome)?;
            preserve_figures(&mut result, &directory)?;
            print_result(&result, cli.json, workspace, printer.is_some())?;
            fail_on_execution_error(&result)
        }
        Action::Packages { load } => {
            let mut engine = Engine::start(EngineConfig {
                executable,
                working_directory: directory,
            })?;
            let printer = start_printer(&engine, cli.json)?;
            let _input =
                InputSupport::start(&engine, printer.as_ref().map(ConsolePrinter::cursor))?;
            let output = match load {
                Some(package) => finish_call(printer.as_ref(), engine.load_package(&package))?,
                None => String::new(),
            };
            let packages = finish_call(printer.as_ref(), engine.list_packages())?;
            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({"packages":packages,"output":output}))?
                );
            } else {
                if packages.is_empty() {
                    println!("No optional packages installed. See scripts/setup.sh.");
                }
                for package in packages {
                    println!(
                        "{:<20} {:<12} {}",
                        package.name,
                        package.version,
                        if package.loaded {
                            "loaded"
                        } else {
                            "available"
                        }
                    );
                }
            }
            Ok(())
        }
        Action::Check { manifest } => check_manifest(
            &executable,
            &directory,
            &manifest,
            cli.json,
            std::time::Duration::from_secs(cli.timeout.map_or(30, |seconds| seconds.get())),
        ),
        Action::Doctor | Action::Conformance { .. } => unreachable!(),
    }
}

fn doctor(executable: Option<PathBuf>, directory: &Path, as_json: bool) -> Result<()> {
    let mut resolved = None;
    let outcome = (|| {
        let working_directory = directory
            .canonicalize()
            .context("The working directory does not exist")?;
        if !working_directory.is_dir() {
            bail!("The working directory must be a folder");
        }
        let requested = executable
            .as_deref()
            .context("GNU Octave is required for execution; set MATHLAB_OCTAVE or pass --octave")?;
        let runtime = resolve_runtime(requested, std::env::var_os("PATH").as_deref())?;
        resolved = Some(runtime.clone());
        doctor_report(&runtime, &working_directory)
    })();
    let report = match outcome {
        Ok(report) => report,
        Err(error) => {
            if as_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "application":"MathLab", "version":env!("CARGO_PKG_VERSION"),
                        "ready":false,
                        "runtime":resolved.as_deref().map(|path| path.to_string_lossy()),
                        "runtime_path_utf8":resolved.as_deref().map(|path| path.to_str().is_some()),
                        "requested_runtime":executable.as_deref().map(|path| path.to_string_lossy()),
                        "requested_runtime_path_utf8":executable.as_deref().map(|path| path.to_str().is_some()),
                        "working_directory":directory.to_string_lossy(),
                        "working_directory_path_utf8":directory.to_str().is_some(), "value_adapter":null,
                        "worker_capabilities":[], "error":format!("{error:#}"),
                        "setup":"Check the selected runtime and configured native adapter paths. See scripts/setup-runtime.sh."
                    }))?
                );
            }
            return Err(error);
        }
    };
    if as_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("MathLab {}\nRuntime: {}\nExecutable: {}\nSession: ready\nWorking folder: {}\nOptional packages: {}",
            env!("CARGO_PKG_VERSION"), report["runtime_version"].as_str().unwrap_or("Unknown version"),
            resolved.as_ref().expect("a ready doctor resolved its runtime").display(),
            report["working_directory"].as_str().expect("a ready doctor resolved its working directory"),
            report["packages"].as_array().map_or(0, Vec::len));
        match report["value_adapter"].as_str() {
            Some(path) => println!("Value adapter: {path}"),
            None => println!("Value adapter: not loaded"),
        }
        if let Some(packages) = report["packages"].as_array() {
            for package in packages {
                println!(
                    "  {} {}",
                    package["name"].as_str().unwrap_or("?"),
                    package["version"].as_str().unwrap_or("?")
                );
            }
        }
    }
    Ok(())
}

// Resolve once before both the version command and Engine startup. A bare
// program name follows PATH; a relative path follows the CLI invocation cwd.
// Reporting this same canonical executable avoids identifying a different
// symlink spelling or a second auto-discovered runtime as the loaded engine.
fn resolve_runtime(executable: &Path, search_path: Option<&std::ffi::OsStr>) -> Result<PathBuf> {
    let candidate = if executable.is_absolute() || executable.components().count() != 1 {
        executable.to_owned()
    } else {
        search_path
            .into_iter()
            .flat_map(std::env::split_paths)
            .map(|directory| directory.join(executable))
            .find(|candidate| runtime_is_executable(candidate))
            .with_context(|| {
                format!(
                    "The selected runtime {} was not found on PATH",
                    executable.display()
                )
            })?
    };
    if !runtime_is_executable(&candidate) {
        bail!(
            "The selected runtime {} is not an executable file",
            candidate.display()
        );
    }
    candidate.canonicalize().with_context(|| {
        format!(
            "Cannot resolve the selected runtime {}",
            candidate.display()
        )
    })
}

fn runtime_is_executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn doctor_report(executable: &Path, directory: &Path) -> Result<serde_json::Value> {
    let runtime_path = executable
        .to_str()
        .context("Doctor requires a UTF-8 runtime path for JSON identity reporting")?;
    let directory_path = directory
        .to_str()
        .context("Doctor requires a UTF-8 working directory path")?;
    // Engine startup serializes canonical adapter paths into JSON requests.
    // Validate those identities before starting it so doctor can report a
    // structured failure for filenames JSON cannot represent.
    for variable in [
        "MATHLAB_SCOPE_BRIDGE",
        "MATHLAB_INPUT_BRIDGE",
        "MATHLAB_VALUE_BRIDGE",
    ] {
        if let Some(path) = std::env::var_os(variable) {
            utf8_adapter_path(Some(Path::new(&path)))
                .with_context(|| format!("Invalid configured {variable} adapter identity"))?;
            let canonical = Path::new(&path)
                .canonicalize()
                .with_context(|| format!("Cannot resolve configured {variable} adapter"))?;
            utf8_adapter_path(Some(&canonical))
                .with_context(|| format!("Invalid configured {variable} adapter identity"))?;
        }
    }
    let output = Command::new(executable)
        .arg("--version")
        .output()
        .context("Cannot start the selected Octave executable")?;
    if !output.status.success() {
        bail!("Octave --version failed");
    }
    let version = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("Unknown version")
        .to_string();
    let mut engine = Engine::start(EngineConfig {
        executable: executable.to_owned(),
        working_directory: directory.into(),
    })?;
    let probe = engine.execute_with_timeout(
        "assert(norm([4 1;1 3]*([4 1;1 3]\\[1;2])-[1;2]) < 1e-12);",
        std::time::Duration::from_secs(15),
    )?;
    fail_on_execution_error(&probe)?;
    let packages = engine.list_packages()?;
    Ok(
        json!({"application":"MathLab", "version":env!("CARGO_PKG_VERSION"), "ready":true,
        "runtime":runtime_path, "runtime_version":version, "working_directory":directory_path, "packages":packages,
        "scope_adapter":utf8_adapter_path(engine.scope_bridge_path())?,
        "input_adapter":utf8_adapter_path(engine.input_bridge_path())?,
        "value_adapter":utf8_adapter_path(engine.value_bridge_path())?,
        "worker_capabilities":engine.capabilities().capabilities,
        "compatibility":"Octave-based alpha; MATLAB release conformance has not been established."}),
    )
}

fn utf8_adapter_path(path: Option<&Path>) -> Result<Option<&str>> {
    path.map(|path| {
        path.to_str().with_context(|| {
            format!(
                "Native adapter path cannot be represented in UTF-8 JSON: {}",
                path.display()
            )
        })
    })
    .transpose()
}

fn start_printer(engine: &Engine, as_json: bool) -> Result<Option<ConsolePrinter>> {
    if as_json {
        Ok(None)
    } else {
        Ok(Some(ConsolePrinter::start(engine)?))
    }
}

fn finish_execution(
    printer: Option<&ConsolePrinter>,
    outcome: mathlab_engine::Result<ExecutionResult>,
) -> Result<ExecutionResult> {
    match outcome {
        Ok(result) => {
            if let Some(printer) = printer {
                printer.synchronize_result(&result)?;
            }
            Ok(result)
        }
        Err(error) => {
            if let Some(printer) = printer {
                printer.synchronize_latest().with_context(|| {
                    format!("{error}; could not display received console output")
                })?;
            }
            Err(error.into())
        }
    }
}

fn finish_call<T>(
    printer: Option<&ConsolePrinter>,
    outcome: mathlab_engine::Result<T>,
) -> Result<T> {
    if let Some(printer) = printer {
        printer.synchronize_latest()?;
    }
    Ok(outcome?)
}

fn print_result(
    result: &ExecutionResult,
    as_json: bool,
    show_workspace: bool,
    already_streamed: bool,
) -> Result<()> {
    if as_json {
        println!("{}", serde_json::to_string_pretty(result)?);
        return Ok(());
    }
    if !already_streamed {
        print!("{}", result.output);
    }
    if let Some(error) = &result.error {
        eprintln!("Error: {}", error.message);
        for frame in &error.stack {
            eprintln!("  {}:{} ({})", frame.file, frame.line, frame.name);
        }
    }
    if show_workspace && !result.workspace.is_empty() {
        println!("\nWorkspace:");
        for variable in &result.workspace {
            let dimensions = variable
                .dimensions
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(" × ");
            println!(
                "  {:<20} {:<12} {:<16} {}",
                variable.name,
                variable.class_name,
                dimensions,
                variable.preview.trim()
            );
        }
    }
    for figure in &result.figures {
        println!("Figure: {}", figure.path.display());
    }
    Ok(())
}

fn fail_on_execution_error(result: &ExecutionResult) -> Result<()> {
    if let Some(error) = &result.error {
        bail!("Execution failed: {}", error.message);
    }
    Ok(())
}

fn preserve_figures(result: &mut ExecutionResult, directory: &Path) -> Result<()> {
    if result.figures.is_empty() {
        return Ok(());
    }
    let output_directory = directory.join(".mathlab").join("figures");
    std::fs::create_dir_all(&output_directory)?;
    for (index, figure) in result.figures.iter_mut().enumerate() {
        let destination = output_directory.join(format!("figure-{}.png", index + 1));
        std::fs::copy(&figure.path, &destination)
            .with_context(|| format!("Could not preserve {}", figure.path.display()))?;
        figure.path = destination;
    }
    Ok(())
}

fn check_manifest(
    executable: &Path,
    directory: &Path,
    path: &Path,
    as_json: bool,
    timeout: std::time::Duration,
) -> Result<()> {
    let path = path
        .canonicalize()
        .context("Compatibility manifest not found")?;
    let manifest: Manifest = serde_json::from_str(&std::fs::read_to_string(&path)?)
        .context("Invalid compatibility manifest")?;
    if manifest.schema_version != 1 {
        bail!("Unsupported manifest version {}", manifest.schema_version);
    }
    if manifest.features.is_empty() {
        bail!("The compatibility manifest has no features");
    }
    let project_root = path.parent().and_then(Path::parent).unwrap_or(directory);
    let mut fixtures = BTreeSet::new();
    let mut reports = Vec::new();
    let mut failures = 0;
    for feature in &manifest.features {
        if !fixtures.insert(feature.fixture.clone()) {
            continue;
        }
        let candidate = directory.join(&feature.fixture);
        let candidate = if candidate.exists() {
            candidate
        } else {
            project_root.join(&feature.fixture)
        };
        let start = Instant::now();
        let outcome = (|| -> Result<ExecutionResult> {
            let fixture = candidate
                .canonicalize()
                .with_context(|| format!("Fixture not found: {}", feature.fixture))?;
            let mut engine = Engine::start(EngineConfig {
                executable: executable.into(),
                working_directory: directory.into(),
            })?;
            let result = engine.run_file_with_timeout(&fixture, timeout)?;
            fail_on_execution_error(&result)?;
            Ok(result)
        })();
        let (passed, output, error) = match outcome {
            Ok(result) => (true, result.output, None),
            Err(error) => {
                failures += 1;
                (false, String::new(), Some(format!("{error:#}")))
            }
        };
        if !as_json {
            println!(
                "{} {} ({})",
                if passed { "PASS" } else { "FAIL" },
                feature.fixture,
                feature.id
            );
            if let Some(error) = &error {
                eprintln!("  {error}");
            }
        }
        reports.push(json!({"fixture":feature.fixture, "passed":passed, "elapsed_ms":start.elapsed().as_millis(), "output":output, "error":error}));
    }
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"reference_profile":manifest.reference_profile,
        "features":manifest.features.len(), "fixtures":reports, "failures":failures,
        "interpretation":"Passing fixtures establish this tested Octave behavior, not complete MATLAB compatibility."}))?
        );
    } else {
        println!(
            "\n{} fixtures, {} failed. MATLAB conformance remains separately assessed.",
            reports.len(),
            failures
        );
    }
    if failures > 0 {
        bail!("{failures} compatibility fixture(s) failed");
    }
    Ok(())
}

fn repl(mut engine: Engine, as_json: bool, timeout: Option<std::time::Duration>) -> Result<()> {
    let printer = start_printer(&engine, as_json)?;
    let router = StdinRouter::start()?;
    let _input = engine
        .input_handle()
        .map(|handle| {
            InputResponder::start(
                handle,
                &router,
                printer.as_ref().map(ConsolePrinter::cursor),
            )
        })
        .transpose()?;
    if !as_json {
        println!("MathLab {} — persistent Octave session\n:quit  :workspace  :clear  :restart  :cd PATH  :run FILE", env!("CARGO_PKG_VERSION"));
    }
    loop {
        if !as_json {
            print!(">> ");
            io::stdout().flush()?;
        }
        let Some(line) = router.read_command()? else {
            break;
        };
        let input = line.trim();
        if input.is_empty() {
            continue;
        }
        match input {
            ":quit" | ":exit" => break,
            ":restart" => {
                finish_call(printer.as_ref(), engine.restart())?;
                if as_json {
                    println!("{}", json!({"session":"restarted"}));
                } else {
                    println!("Session restarted.");
                }
            }
            ":clear" => {
                finish_call(printer.as_ref(), engine.clear())?;
                if as_json {
                    println!("{}", json!({"workspace":[]}));
                }
            }
            ":workspace" => {
                let workspace = finish_call(printer.as_ref(), engine.workspace())?;
                if as_json {
                    println!("{}", json!({"workspace":workspace}));
                } else {
                    for variable in workspace {
                        println!(
                            "{}\t{}\t{:?}\t{}",
                            variable.name,
                            variable.class_name,
                            variable.dimensions,
                            variable.preview.trim()
                        );
                    }
                }
            }
            _ if input.starts_with(":cd ") => {
                match finish_call(
                    printer.as_ref(),
                    engine.set_directory(Path::new(input[4..].trim())),
                ) {
                    Ok(()) if as_json => println!(
                        "{}",
                        json!({"working_directory":engine.working_directory()})
                    ),
                    Ok(()) => {}
                    Err(error) if as_json => println!("{}", json!({"error":format!("{error:#}")})),
                    Err(error) => eprintln!("{error:#}"),
                }
            }
            _ => {
                let outcome = if let Some(file) = input.strip_prefix(":run ") {
                    match timeout {
                        Some(timeout) => {
                            engine.run_file_with_timeout(Path::new(file.trim()), timeout)
                        }
                        None => engine.run_file(Path::new(file.trim())),
                    }
                } else {
                    match timeout {
                        Some(timeout) => engine.execute_with_timeout(input, timeout),
                        None => engine.execute(input),
                    }
                };
                match finish_execution(printer.as_ref(), outcome) {
                    Ok(mut result) => {
                        let directory = result.working_directory.clone();
                        if let Err(error) = preserve_figures(&mut result, &directory) {
                            eprintln!("Could not save figure previews: {error:#}");
                        }
                        print_result(&result, as_json, false, printer.is_some())?;
                    }
                    Err(error) if as_json => println!("{}", json!({"error":format!("{error:#}")})),
                    Err(error) => eprintln!("{error:#}"),
                }
            }
        }
    }
    if let Some(printer) = &printer {
        printer.synchronize_latest()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_commands_parse() {
        assert!(Cli::try_parse_from(["mathlab", "eval", "A = eye(3);", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["mathlab", "run", "demo.m", "--workspace"]).is_ok());
        assert!(
            Cli::try_parse_from(["mathlab", "--directory", "/tmp", "check", "core.json"]).is_ok()
        );
    }
}
