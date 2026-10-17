use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use mathlab_engine::startup_config::QualifiedStartupConfig;
use mathlab_engine::{discover_octave, Engine, EngineConfig, ExecutionResult};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

mod conformance;
mod rust_eval;
mod stdin_input;
mod streaming;
mod value_probe;
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
    #[arg(long, global = true, conflicts_with = "startup_config")]
    octave: Option<PathBuf>,
    /// Start the explicit qualified core or graphics profile from this pinned JSON configuration.
    #[arg(long, global = true, value_name = "JSON")]
    startup_config: Option<PathBuf>,
    /// Start the computation session in this directory.
    #[arg(long, global = true)]
    directory: Option<PathBuf>,
    /// Print machine-readable results.
    #[arg(long, global = true)]
    json: bool,
    /// Limit execution in seconds. Compatibility checks default to 30 seconds per fixture.
    /// probe-value-api uses its reported fixed budgets.
    #[arg(long, global = true)]
    timeout: Option<std::num::NonZeroU64>,
    #[command(subcommand)]
    command: Action,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
enum EvalBackend {
    #[default]
    Octave,
    /// Experimental bounded real arithmetic, without an Octave fallback.
    Rust,
}

#[derive(Subcommand)]
enum Action {
    /// Check the runtime and report installed packages.
    Doctor,
    /// Exercise checked value APIs in an owned temporary workspace; native adapter required by default.
    /// Uses reported fixed budgets and ignores --directory and --timeout.
    ProbeValueApi {
        /// Verify precise missing-native errors instead of native precision/lifecycle success.
        #[arg(long, conflicts_with = "startup_config")]
        expect_unavailable: bool,
    },
    /// Execute code in Octave, or opt into experimental bounded Rust arithmetic.
    Eval {
        code: String,
        /// Select the execution backend for this one-shot expression.
        #[arg(long, value_enum, default_value_t = EvalBackend::Octave)]
        backend: EvalBackend,
    },
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

fn parse_cli_from<I, T>(arguments: I) -> std::result::Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::try_parse_from(arguments)?;
    // Clap can validate global conflicts before propagating values between
    // command levels. Check the final parsed selection for every placement.
    if cli.startup_config.is_some() {
        let incompatible = if cli.octave.is_some() {
            Some("--octave")
        } else if matches!(
            cli.command,
            Action::ProbeValueApi {
                expect_unavailable: true
            }
        ) {
            Some("probe-value-api --expect-unavailable")
        } else {
            None
        };
        if let Some(incompatible) = incompatible {
            return Err(Cli::command().error(
                clap::error::ErrorKind::ArgumentConflict,
                format!("--startup-config cannot be used with {incompatible}"),
            ));
        }
    }
    if matches!(
        &cli.command,
        Action::Eval {
            backend: EvalBackend::Rust,
            ..
        }
    ) {
        let incompatible = if cli.timeout.is_some() {
            Some("--timeout")
        } else if cli.octave.is_some() {
            Some("--octave")
        } else if cli.startup_config.is_some() {
            Some("--startup-config")
        } else if cli.directory.is_some() {
            Some("--directory")
        } else {
            None
        };
        if let Some(flag) = incompatible {
            return Err(Cli::command().error(
                clap::error::ErrorKind::ArgumentConflict,
                format!("eval --backend rust cannot be used with {flag}"),
            ));
        }
    }
    Ok(cli)
}

fn run() -> Result<()> {
    let cli = parse_cli_from(std::env::args_os()).unwrap_or_else(|error| error.exit());
    if let Action::Eval {
        code,
        backend: EvalBackend::Rust,
    } = &cli.command
    {
        return rust_eval::run(code, cli.json);
    }
    // Load explicit selection once, including on diagnostic failure paths. An
    // invalid selection must never cause runtime discovery or a legacy start.
    let startup_config = cli
        .startup_config
        .as_deref()
        .map(QualifiedStartupConfig::load)
        .transpose()
        .map_err(anyhow::Error::new);
    let executable = if cli.startup_config.is_some() {
        startup_config
            .as_ref()
            .ok()
            .and_then(Option::as_ref)
            .map(|config| config.executable().to_owned())
    } else {
        cli.octave.clone().or_else(discover_octave)
    };
    if matches!(cli.command, Action::Doctor) {
        return doctor(
            executable,
            startup_config,
            cli.directory.as_deref().unwrap_or(Path::new(".")),
            cli.json,
        );
    }
    if let Action::ProbeValueApi { expect_unavailable } = &cli.command {
        // The probe resolves PATH/relative selections before entering its own
        // temporary cwd and reports every handled resolution/startup failure.
        // Never validate or use a user's --directory for this diagnostic.
        // A configured runtime is an explicit selection even when invalid;
        // never let discovery silently replace it with another installation.
        let selected = if cli.startup_config.is_some() {
            executable
        } else {
            Some(
                cli.octave
                    .clone()
                    .or_else(|| {
                        ["MATHLAB_OCTAVE", "OCTAVE_EXECUTABLE"]
                            .iter()
                            .find_map(std::env::var_os)
                            .map(PathBuf::from)
                    })
                    .or(executable)
                    .unwrap_or_else(|| PathBuf::from("octave-cli")),
            )
        };
        return value_probe::run(selected, startup_config, cli.json, *expect_unavailable);
    }
    let startup_config = startup_config?;
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
            startup_config: startup_config.as_ref(),
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
            start_engine(&executable, directory, startup_config.as_ref())?,
            startup_config.as_ref(),
            cli.json,
            cli.timeout
                .map(|seconds| std::time::Duration::from_secs(seconds.get())),
        ),
        Action::Eval { code, .. } => {
            let mut engine = start_engine(&executable, directory.clone(), startup_config.as_ref())?;
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
            let mut engine = start_engine(&executable, directory.clone(), startup_config.as_ref())?;
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
            let mut engine = start_engine(&executable, directory, startup_config.as_ref())?;
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
            startup_config.as_ref(),
            &directory,
            &manifest,
            cli.json,
            std::time::Duration::from_secs(cli.timeout.map_or(30, |seconds| seconds.get())),
        ),
        Action::Doctor | Action::ProbeValueApi { .. } | Action::Conformance { .. } => {
            unreachable!()
        }
    }
}

fn start_engine(
    executable: &Path,
    directory: PathBuf,
    startup_config: Option<&QualifiedStartupConfig>,
) -> mathlab_engine::Result<Engine> {
    match startup_config {
        Some(config) => config.start(directory),
        None => Engine::start(EngineConfig {
            executable: executable.to_owned(),
            working_directory: directory,
        }),
    }
}

fn doctor(
    executable: Option<PathBuf>,
    startup_config: Result<Option<QualifiedStartupConfig>>,
    directory: &Path,
    as_json: bool,
) -> Result<()> {
    let mut resolved = None;
    let outcome = (|| {
        let startup_config = startup_config?;
        let working_directory = directory
            .canonicalize()
            .context("The working directory does not exist")?;
        if !working_directory.is_dir() {
            bail!("The working directory must be a folder");
        }
        let requested = executable
            .as_deref()
            .context("GNU Octave is required for execution; set MATHLAB_OCTAVE or pass --octave")?;
        let runtime = match startup_config.as_ref() {
            Some(config) => config.executable().to_owned(),
            None => resolve_runtime(requested, std::env::var_os("PATH").as_deref())?,
        };
        resolved = Some(runtime.clone());
        doctor_report(&runtime, &working_directory, startup_config.as_ref())
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
        println!(
            "MathLab {}\nRuntime: {}\nExecutable: {}\nSession: ready\nWorking folder: {}\nOptional packages: {}",
            env!("CARGO_PKG_VERSION"),
            report["runtime_version"]
                .as_str()
                .unwrap_or("Unknown version"),
            resolved
                .as_ref()
                .expect("a ready doctor resolved its runtime")
                .display(),
            report["working_directory"]
                .as_str()
                .expect("a ready doctor resolved its working directory"),
            report["packages"].as_array().map_or(0, Vec::len)
        );
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

fn doctor_report(
    executable: &Path,
    directory: &Path,
    startup_config: Option<&QualifiedStartupConfig>,
) -> Result<serde_json::Value> {
    let runtime_path = executable
        .to_str()
        .context("Doctor requires a UTF-8 runtime path for JSON identity reporting")?;
    let directory_path = directory
        .to_str()
        .context("Doctor requires a UTF-8 working directory path")?;
    // Engine startup serializes canonical adapter paths into JSON requests.
    // Validate those identities before starting it so doctor can report a
    // structured failure for filenames JSON cannot represent.
    if startup_config.is_none() {
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
    }
    let legacy_version = if startup_config.is_none() {
        let output = Command::new(executable)
            .arg("--version")
            .output()
            .context("Cannot start the selected Octave executable")?;
        if !output.status.success() {
            bail!("Octave --version failed");
        }
        Some(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("Unknown version")
                .to_string(),
        )
    } else {
        None
    };
    let mut engine = start_engine(executable, directory.into(), startup_config)?;
    let version = legacy_version.unwrap_or_else(|| engine.version().to_owned());
    let probe = engine.execute_with_timeout(
        "assert(norm([4 1;1 3]*([4 1;1 3]\\[1;2])-[1;2]) < 1e-12);",
        std::time::Duration::from_secs(15),
    )?;
    fail_on_execution_error(&probe)?;
    let packages = engine.list_packages()?;
    let mut report = json!({"application":"MathLab", "version":env!("CARGO_PKG_VERSION"), "ready":true,
        "runtime":runtime_path, "runtime_version":version, "working_directory":directory_path, "packages":packages,
        "scope_adapter":utf8_adapter_path(engine.scope_bridge_path())?,
        "input_adapter":utf8_adapter_path(engine.input_bridge_path())?,
        "value_adapter":utf8_adapter_path(engine.value_bridge_path())?,
        "worker_capabilities":engine.capabilities().capabilities,
        "compatibility":"Octave-based alpha; MATLAB release conformance has not been established."});
    if let Some(config) = startup_config {
        let status = engine
            .qualified_startup_status()
            .context("Qualified doctor has no current accepted startup status")?;
        report["session_context"] = serde_json::to_value(&status.context)?;
        report["qualified_startup_status"] = serde_json::to_value(&status)?;
        report["native_modules"] = serde_json::to_value(config.native_modules())?;
    }
    Ok(report)
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
    startup_config: Option<&QualifiedStartupConfig>,
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
            let mut engine = start_engine(executable, directory.into(), startup_config)?;
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

fn repl(
    mut engine: Engine,
    startup_config: Option<&QualifiedStartupConfig>,
    as_json: bool,
    timeout: Option<std::time::Duration>,
) -> Result<()> {
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
        println!(
            "MathLab {} — persistent Octave session\n:quit  :workspace  :clear  :restart  :cd PATH  :run FILE",
            env!("CARGO_PKG_VERSION")
        );
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
                let outcome = match startup_config {
                    Some(config) => config.restart(&mut engine),
                    None => engine.restart(),
                };
                finish_call(printer.as_ref(), outcome)?;
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
    fn rust_eval_selector_is_local_and_octave_remains_default() {
        for args in [
            vec!["mathlab", "eval", "1+2"],
            vec!["mathlab", "eval", "--backend", "octave", "1+2"],
        ] {
            assert!(matches!(
                parse_cli_from(args).unwrap().command,
                Action::Eval {
                    backend: EvalBackend::Octave,
                    ..
                }
            ));
        }
        assert!(matches!(
            parse_cli_from(["mathlab", "eval", "--backend", "rust", "1+2"])
                .unwrap()
                .command,
            Action::Eval {
                backend: EvalBackend::Rust,
                ..
            }
        ));
        assert!(parse_cli_from(["mathlab", "--backend", "rust", "eval", "1+2"]).is_err());
        assert!(parse_cli_from(["mathlab", "repl", "--backend", "rust"]).is_err());
        assert!(parse_cli_from(["mathlab", "eval", "--backend", "auto", "1+2"]).is_err());
    }

    #[test]
    fn rust_eval_rejects_incompatible_globals_in_both_positions() {
        for (flag, value) in [
            ("--timeout", "1"),
            ("--octave", "missing"),
            ("--startup-config", "missing.json"),
            ("--directory", "missing"),
        ] {
            for before in [false, true] {
                let mut args = vec!["mathlab"];
                if before {
                    args.extend([flag, value]);
                }
                args.extend(["eval", "--backend", "rust", "1"]);
                if !before {
                    args.extend([flag, value]);
                }
                let error = parse_cli_from(args).err().unwrap();
                assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
                assert!(error.to_string().contains(flag));
            }
        }
    }

    #[test]
    fn public_commands_parse() {
        assert!(parse_cli_from(["mathlab", "eval", "A = eye(3);", "--json"]).is_ok());
        assert!(parse_cli_from(["mathlab", "run", "demo.m", "--workspace"]).is_ok());
        assert!(parse_cli_from(["mathlab", "--directory", "/tmp", "check", "core.json"]).is_ok());
    }

    #[test]
    fn explicit_startup_config_is_global_for_every_engine_command() {
        for action in [
            vec!["doctor"],
            vec!["probe-value-api"],
            vec!["eval", "A = 1;"],
            vec!["run", "demo.m"],
            vec!["repl"],
            vec!["check", "core.json"],
            vec!["conformance", "core.json"],
            vec!["packages"],
        ] {
            for before_command in [false, true] {
                let mut arguments = vec!["mathlab"];
                if before_command {
                    arguments.extend(["--startup-config", "selected.json"]);
                }
                arguments.extend(action.iter().copied());
                if !before_command {
                    arguments.extend(["--startup-config", "selected.json"]);
                }
                let cli = parse_cli_from(arguments.iter().copied())
                    .unwrap_or_else(|error| panic!("arguments: {arguments:?}; {error}"));
                assert_eq!(
                    cli.startup_config.as_deref(),
                    Some(Path::new("selected.json"))
                );
                assert!(cli.octave.is_none());
            }
        }
    }

    #[test]
    fn explicit_startup_config_rejects_runtime_and_unavailable_overrides() {
        for command_line in [
            "--startup-config selected.json --octave other doctor",
            "--octave other --startup-config selected.json doctor",
            "--octave other doctor --startup-config selected.json",
            "--startup-config selected.json doctor --octave other",
            "doctor --startup-config selected.json --octave other",
            "doctor --octave other --startup-config selected.json",
            "--startup-config selected.json probe-value-api --expect-unavailable",
            "probe-value-api --startup-config selected.json --expect-unavailable",
            "probe-value-api --expect-unavailable --startup-config selected.json",
        ] {
            let arguments: Vec<_> = std::iter::once("mathlab")
                .chain(command_line.split_whitespace())
                .collect();
            let error = match parse_cli_from(arguments.iter().copied()) {
                Ok(_) => panic!("accepted incompatible arguments: {arguments:?}"),
                Err(error) => error,
            };
            assert_eq!(
                error.kind(),
                clap::error::ErrorKind::ArgumentConflict,
                "arguments: {arguments:?}; {error}"
            );
        }
    }

    #[test]
    fn actual_cli_parser_preserves_help_and_version() {
        for (flag, expected) in [
            ("--help", clap::error::ErrorKind::DisplayHelp),
            ("--version", clap::error::ErrorKind::DisplayVersion),
        ] {
            let error = match parse_cli_from(["mathlab", flag]) {
                Ok(_) => panic!("expected display output for {flag}"),
                Err(error) => error,
            };
            assert_eq!(error.kind(), expected, "arguments: mathlab {flag}; {error}");
        }
    }

    #[test]
    fn value_api_probe_defaults_to_native_required_and_accepts_explicit_unavailable() {
        let required = parse_cli_from(["mathlab", "probe-value-api"]).unwrap();
        assert!(matches!(
            required.command,
            Action::ProbeValueApi {
                expect_unavailable: false
            }
        ));
        let unavailable = parse_cli_from([
            "mathlab",
            "--json",
            "--directory",
            "/unused-probe-workspace",
            "--timeout",
            "1",
            "probe-value-api",
            "--expect-unavailable",
        ])
        .unwrap();
        assert!(unavailable.json);
        assert_eq!(
            unavailable.directory.as_deref(),
            Some(Path::new("/unused-probe-workspace"))
        );
        assert_eq!(unavailable.timeout.unwrap().get(), 1);
        assert!(matches!(
            unavailable.command,
            Action::ProbeValueApi {
                expect_unavailable: true
            }
        ));
        assert!(parse_cli_from(["mathlab", "doctor", "--expect-unavailable"]).is_err());
    }
}
