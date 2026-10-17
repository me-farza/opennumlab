// SPDX-License-Identifier: GPL-3.0-or-later
//! Checked-value diagnostic through public Engine APIs.
//! It owns an isolated temporary workspace, never an existing user session.
//! Ordinary evaluation is used only for fixtures and independent final oracles.

use anyhow::{Context, Result, bail, ensure};
use mathlab_engine::startup_config::QualifiedStartupConfig;
use mathlab_engine::value_edit::{
    EditOperation, HistoryRequest, LeafDescriptor, MAX_EDIT_RECEIPT_BYTES, MAX_EDIT_REQUEST_BYTES,
    MAX_HISTORY_BYTES, MAX_HISTORY_RECORDS, MAX_PATCH_CELLS, RangeEditReceipt, RangeEditRequest,
};
use mathlab_engine::{
    BindingToken, DecimalU64, Engine, Error as EngineError, NumericClass, NumericComponents,
    PageData, PageWindow, QUALIFIED_GRAPHICS_STARTUP_PROFILE, SourceStorage, ValuePage,
    ValuePageError, ValuePageRequest, ValuePath, ValuePathStep, WorkerCapabilities,
    WorkspaceSnapshot,
};
#[cfg(unix)]
use mathlab_engine::{CancellationHandle, ConsoleEventKind};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
#[cfg(unix)]
use std::{thread, time::Instant};

const NATIVE_CAPABILITY: &str = "dense_complex_native_components.v1";
const MAX_ARTIFACT_IDENTITIES: usize = 8;
const PAGE_BYTES: usize = 1024 * 1024;
const FIXTURE_EXECUTE_TIMEOUT: Duration = Duration::from_secs(20);
const CHECKED_API_TIMEOUT_SECONDS: u64 = 30;
const STOP_BARRIER_TIMEOUT: Duration = Duration::from_secs(15);
const STOP_SETTLE_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Serialize)]
struct Case {
    id: String,
    status: &'static str,
    error_identifier: Option<String>,
    details: Value,
}

#[derive(Serialize)]
struct ArtifactIdentity {
    kind: &'static str,
    configured_by: Option<&'static str>,
    path: String,
    sha256_before: Option<String>,
    sha256_after: Option<String>,
    unchanged: Option<bool>,
    error: Option<String>,
}

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    kind: &'static str,
    profile: &'static str,
    status: &'static str,
    build: Value,
    runtime: Value,
    runtime_details: Value,
    adapter_preflight: Vec<Value>,
    value_adapter: Value,
    adapter_details: Value,
    worker_capabilities: Value,
    worker_protocol: Value,
    artifact_identities: Vec<ArtifactIdentity>,
    limits: Value,
    cases: Vec<Case>,
    scope: Value,
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    native_modules: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    qualified_startup_status: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_context: Option<Value>,
}

/// Produce exactly one report, including on a failed startup/profile/case, and
/// return Err after reporting a failed probe. No missing-runtime success/skip.
pub fn run(
    executable: Option<PathBuf>,
    startup_config: Result<Option<QualifiedStartupConfig>>,
    as_json: bool,
    expect_unavailable: bool,
) -> Result<()> {
    let mut report = Report {
        schema_version: 1,
        kind: "mathlab.value-api-probe",
        profile: if expect_unavailable {
            "native-unavailable"
        } else {
            "native-required"
        },
        status: "failed",
        build: build_identity(),
        runtime: Value::Null,
        runtime_details: json!({"requested_runtime":executable.as_deref().map(path_observation),
            "canonical_identity_verified":false,"started":false}),
        adapter_preflight: Vec::new(),
        value_adapter: Value::Null,
        adapter_details: Value::Null,
        worker_capabilities: json!([]),
        worker_protocol: Value::Null,
        artifact_identities: Vec::new(),
        limits: json!({
            "page_cells":2000,"page_result_bytes":PAGE_BYTES,
            "edit_cells":MAX_PATCH_CELLS,"edit_request_bytes":MAX_EDIT_REQUEST_BYTES,
            "edit_receipt_bytes":MAX_EDIT_RECEIPT_BYTES,
            "history_records":MAX_HISTORY_RECORDS,"history_logical_patch_bytes":MAX_HISTORY_BYTES,
            "fixture_cells_per_complex_leaf":3,
            "fixture_execute_timeout_seconds":FIXTURE_EXECUTE_TIMEOUT.as_secs(),
            "checked_api_timeout_seconds":CHECKED_API_TIMEOUT_SECONDS,
            "stop_barrier_timeout_seconds":STOP_BARRIER_TIMEOUT.as_secs(),
            "stop_settle_timeout_seconds":STOP_SETTLE_TIMEOUT.as_secs(),
            "meaning":"Protocol/admission and per-request/wait limits; not a whole-case watchdog, peak RSS or physical allocation bounds."
        }),
        cases: Vec::new(),
        scope: json!({
            "public_engine_api_only":true,"owned_temporary_workspace":true,
            "native_precision_requested":!expect_unavailable,
            "artifact_hashes_are_endpoint_identity_not_abi_or_load_proof":true,
            "selected_subset":if expect_unavailable {"Real exact uint64 checked read/write/undo/redo plus profile observation and two precise dense double-complex unavailable rejections."}else{"Real exact uint64 and dense single/double complex, scalar struct/cell paths, bounded range history, identity/reset/restart/ordinary Stop."},
            "excluded":["sparse precision","arbitrary object getters","GUI/clipboard","commit-window fault injection","general MATLAB parity","speed claims"]
        }),
        error: None,
        native_modules: None,
        qualified_startup_status: None,
        session_context: None,
    };
    let mut result = (|| {
        // Keep malformed explicit configurations inside the same retained
        // diagnostic report as admission, startup and case failures.
        let startup_config = startup_config?;
        ensure!(
            !(expect_unavailable && startup_config.is_some()),
            "--expect-unavailable conflicts with --startup-config"
        );
        ensure!(
            report.build["executable_identity_verified"] == true,
            "Cannot establish the CLI executable's canonical UTF-8 identity: {}",
            report.build["executable_identity_error"]
        );
        ensure!(
            report.build["binary_identity_verified"].as_bool() == Some(true)
                && report.build["binary_sha256"]
                    .as_str()
                    .is_some_and(is_sha256),
            "Cannot establish the CLI executable's SHA-256 identity: {}",
            report.build["binary_identity_error"]
        );
        let path = report.build["executable"]
            .as_str()
            .context("missing CLI path identity")?
            .to_owned();
        let hash = report.build["binary_sha256"]
            .as_str()
            .context("missing CLI byte identity")?
            .to_owned();
        bind_artifact(
            &mut report,
            "cli_executable",
            None,
            Path::new(&path),
            Some(hash),
        )?;
        probe(
            &mut report,
            executable.context("No selected probe runtime")?,
            startup_config.as_ref(),
            expect_unavailable,
        )
    })();
    // probe owns every Engine and returns only after its final Engine drops.
    // A passing report therefore requires stable canonical bytes at both ends.
    if result.is_ok() {
        result = verify_artifacts(&mut report);
    }
    if let Err(error) = &result {
        report.error = Some(format!("{error:#}"));
        if !report.cases.iter().any(|case| case.status == "failed") {
            report.cases.push(failed_case("probe_setup", error));
        }
    } else {
        report.status = "passed";
    }
    if as_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Value API probe: {} ({})", report.status, report.profile);
        println!("  runtime: {}", report.runtime);
        println!("  value adapter: {}", report.value_adapter);
        for case in &report.cases {
            println!("  {}: {}", case.id, case.status);
        }
        if let Some(error) = &report.error {
            println!("  failure: {error}");
        }
    }
    result
}

fn build_identity() -> Value {
    let observed = std::env::current_exe().and_then(fs::canonicalize);
    let executable = observed.as_ref().ok();
    let exact_path = executable.and_then(|path| path.to_str());
    let binary_hash = executable.map(|path| hash_file(path));
    let binary_identity_verified = binary_hash
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .is_some_and(|hash| is_sha256(hash));
    let identity_error = match &observed {
        Err(error) => Some(error.to_string()),
        Ok(path) if !path.is_absolute() => Some("CLI executable identity is not absolute".into()),
        Ok(_) if exact_path.is_none() => Some("CLI executable identity is not UTF-8".into()),
        Ok(_) => None,
    };
    json!({
        "package_version":env!("CARGO_PKG_VERSION"),
        "revision":env!("MATHLAB_BUILD_REVISION"),
        "source_sha256":env!("MATHLAB_BUILD_SOURCE_SHA256"),
        "target":env!("MATHLAB_BUILD_TARGET"),"executable":exact_path,
        "executable_observation":executable.map(|path|path_observation(path)),
        "executable_identity_verified":identity_error.is_none(),
        "executable_identity_error":identity_error,
        "binary_sha256":binary_hash.as_ref().and_then(|result|result.as_ref().ok()),
        "binary_identity_verified":binary_identity_verified,
        "binary_identity_error":binary_hash.as_ref().and_then(|result|result.as_ref().err()).map(ToString::to_string),
        "provenance":"Cargo-captured build identity and actual running executable hash; no current repository HEAD substitution."
    })
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn bind_artifact(
    report: &mut Report,
    kind: &'static str,
    configured_by: Option<&'static str>,
    path: &Path,
    cached_hash: Option<String>,
) -> Result<()> {
    ensure!(
        report.artifact_identities.len() < MAX_ARTIFACT_IDENTITIES,
        "artifact identity budget exceeded"
    );
    ensure!(
        path.is_absolute() && fs::canonicalize(path)? == path,
        "artifact path changed before binding"
    );
    let mut identity = ArtifactIdentity {
        kind,
        configured_by,
        path: checked_utf8(path, "Artifact path")?,
        sha256_before: None,
        sha256_after: None,
        unchanged: None,
        error: None,
    };
    let result = cached_hash
        .map(Ok)
        .unwrap_or_else(|| hash_file(path))
        .and_then(|hash| {
            ensure!(is_sha256(&hash), "artifact hash is not lowercase SHA-256");
            Ok(hash)
        });
    match result {
        Ok(hash) => {
            identity.sha256_before = Some(hash);
            report.artifact_identities.push(identity);
            Ok(())
        }
        Err(error) => {
            identity.error = Some(format!("{error:#}"));
            report.artifact_identities.push(identity);
            Err(error.context(format!("binding {kind} bytes")))
        }
    }
}

fn verify_artifacts(report: &mut Report) -> Result<()> {
    for identity in &mut report.artifact_identities {
        verify_artifact(identity)?;
    }
    Ok(())
}

fn verify_artifact(identity: &mut ArtifactIdentity) -> Result<()> {
    let result: Result<()> = (|| {
        let path = Path::new(&identity.path);
        ensure!(
            fs::canonicalize(path)? == path,
            "canonical artifact path changed"
        );
        let hash = hash_file(path)?;
        ensure!(is_sha256(&hash), "artifact hash is not lowercase SHA-256");
        identity.sha256_after = Some(hash.clone());
        ensure!(
            identity.sha256_before.as_deref() == Some(hash.as_str()),
            "artifact bytes changed during probe"
        );
        Ok(())
    })();
    identity.unchanged = Some(result.is_ok());
    if let Err(error) = result {
        identity.error = Some(format!("{error:#}"));
        return Err(error.context(format!(
            "verifying {} bytes after Engine shutdown",
            identity.kind
        )));
    }
    Ok(())
}

/// Lossy display is diagnostic text ONLY, explicitly marked as such. Never use
/// it as a successful executable/adapter identity or pass it to Engine.
fn path_observation(path: &Path) -> Value {
    json!({"display":path.to_string_lossy().into_owned(),
        "path_utf8":path.to_str().is_some(),"absolute":path.is_absolute(),
        "display_is_lossy":path.to_str().is_none()})
}

fn checked_utf8(path: &Path, label: &str) -> Result<String> {
    path.to_str().map(str::to_owned).with_context(|| {
        format!(
            "{label} cannot be represented as an exact UTF-8 JSON identity: {}",
            path.display()
        )
    })
}

fn canonical_runtime(executable: &Path) -> Result<PathBuf> {
    let search_path = std::env::var_os("PATH");
    resolve_runtime(executable, search_path.as_deref())
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

fn resolve_runtime(executable: &Path, search_path: Option<&std::ffi::OsStr>) -> Result<PathBuf> {
    // Resolve PATH and relative inputs in the caller's process cwd. Engine
    // receives only the exact absolute target before entering its temporary cwd.
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
                    "Requested runtime {} was not found as an executable on PATH",
                    executable.display()
                )
            })?
    };
    ensure!(
        runtime_is_executable(&candidate),
        "Requested runtime {} is not an executable file",
        candidate.display()
    );
    let resolved = fs::canonicalize(&candidate)
        .with_context(|| format!("Cannot resolve requested runtime {}", executable.display()))?;
    ensure!(
        resolved.is_absolute() && runtime_is_executable(&resolved),
        "Resolved runtime is not an absolute existing executable file"
    );
    checked_utf8(&resolved, "Runtime path")?;
    Ok(resolved)
}

fn canonical_adapter(path: &Path) -> Result<PathBuf> {
    // Check both the environment spelling and its resolved target before
    // Engine can serialize a configured native path in a request payload.
    checked_utf8(path, "Configured adapter path")?;
    let resolved = fs::canonicalize(path)
        .with_context(|| format!("Cannot resolve configured adapter {}", path.display()))?;
    ensure!(
        resolved.is_absolute() && resolved.is_file(),
        "Configured adapter is not an absolute existing file"
    );
    ensure!(
        resolved
            .extension()
            .is_some_and(|extension| extension == "oct"),
        "Configured adapter target must have the .oct extension"
    );
    checked_utf8(&resolved, "Canonical adapter path")?;
    Ok(resolved)
}

fn preflight_adapters(report: &mut Report) -> Result<()> {
    // Inspect every configured native path, even the currently nonautomatic
    // debugger setting. Path admission is not evidence of loaded ABI/protocol.
    for (variable, kind) in [
        ("MATHLAB_VALUE_BRIDGE", "value_adapter"),
        ("MATHLAB_SCOPE_BRIDGE", "scope_adapter"),
        ("MATHLAB_INPUT_BRIDGE", "input_adapter"),
        ("MATHLAB_DEBUG_BRIDGE", "debug_adapter"),
    ] {
        let Some(raw) = std::env::var_os(variable) else {
            continue;
        };
        let requested = PathBuf::from(raw);
        let mut observation = json!({"environment_variable":variable,
            "requested_path":path_observation(&requested),"canonical_path":null,
            "identity_verified":false,"abi_or_protocol_qualified":false});
        match canonical_adapter(&requested) {
            Ok(resolved) => {
                observation["canonical_path"] =
                    json!(checked_utf8(&resolved, "Canonical adapter path")?);
                observation["identity_verified"] = json!(true);
                report.adapter_preflight.push(observation);
                bind_artifact(report, kind, Some(variable), &resolved, None)?;
            }
            Err(error) => {
                observation["error"] = json!(format!("{error:#}"));
                report.adapter_preflight.push(observation);
                return Err(error.context(format!("native adapter preflight for {variable}")));
            }
        }
    }
    Ok(())
}

fn hash_file(path: &std::path::Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            return Ok(format!("{:x}", digest.finalize()));
        }
        digest.update(&buffer[..count]);
    }
}

fn failed_case(id: &str, error: &anyhow::Error) -> Case {
    let error_identifier = match error.downcast_ref::<EngineError>() {
        Some(EngineError::Worker(error)) => Some(error.identifier.clone()),
        _ => None,
    };
    Case {
        id: id.into(),
        status: "failed",
        error_identifier,
        details: json!({"error":format!("{error:#}")}),
    }
}

fn case(report: &mut Report, id: &str, action: impl FnOnce() -> Result<Value>) -> Result<()> {
    match action() {
        Ok(details) => {
            report.cases.push(Case {
                id: id.into(),
                status: "passed",
                error_identifier: details
                    .get("observed_error_identifier")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                details,
            });
            Ok(())
        }
        Err(error) => {
            report.cases.push(failed_case(id, &error));
            Err(error.context(format!("value API case {id}")))
        }
    }
}

fn qualified_artifact_kind(name: &str) -> Result<&'static str> {
    match name {
        "mathlab_intrinsic_bridge.oct" => Ok("intrinsic_provider"),
        "mathlab_input_bridge.oct" => Ok("input_adapter"),
        "mathlab_scope_bridge.oct" => Ok("scope_adapter"),
        "mathlab_value_bridge.oct" => Ok("value_adapter"),
        "mathlab_debug_bridge.oct" => Ok("debug_adapter"),
        "mathlab_graphics_bridge.oct" => Ok("graphics_adapter"),
        _ => bail!("Unexpected qualified native module {name}"),
    }
}

fn probe(
    report: &mut Report,
    executable: PathBuf,
    startup_config: Option<&QualifiedStartupConfig>,
    unavailable: bool,
) -> Result<()> {
    let executable = match startup_config {
        Some(config) => config.executable().to_owned(),
        None => canonical_runtime(&executable)?,
    };
    report.runtime = json!(checked_utf8(&executable, "Runtime path")?);
    bind_artifact(report, "octave_runtime", None, &executable, None)?;
    report.runtime_details["canonical_identity_verified"] = json!(true);
    // Engine::start currently serializes optional module paths internally.
    // Preflight first so a non-UTF-8 environment path cannot panic below it
    // before this probe emits its structured failure report.
    if let Some(config) = startup_config {
        for (name, identity) in config.native_modules() {
            let kind = qualified_artifact_kind(&name)?;
            bind_artifact(report, kind, Some("startup_config"), &identity.path, None)?;
            ensure!(
                report
                    .artifact_identities
                    .last()
                    .and_then(|artifact| artifact.sha256_before.as_deref())
                    == Some(identity.sha256.as_str()),
                "Qualified native module bytes differ from the selected pin: {name}"
            );
        }
    } else {
        preflight_adapters(report)?;
    }
    let directory = tempfile::Builder::new()
        .prefix("mathlab-api-probe-")
        .tempdir()?;
    let mut engine =
        match super::start_engine(&executable, directory.path().to_owned(), startup_config) {
            Ok(engine) => engine,
            Err(error) => {
                let error = anyhow::Error::new(error);
                report.cases.push(failed_case("startup", &error));
                return Err(error.context("starting isolated value API probe"));
            }
        };
    if let Some(config) = startup_config {
        report.native_modules = Some(serde_json::to_value(config.native_modules())?);
        observe_qualified_status(report, &engine, config)?;
    }
    report.runtime_details["started"] = json!(true);
    report.runtime_details["version"] = json!(engine.version());
    report.runtime_details["process_id"] = json!(engine.process_id());
    report.runtime_details["working_directory_observation"] =
        path_observation(engine.working_directory());
    report.runtime_details["working_directory"] = json!(checked_utf8(
        engine.working_directory(),
        "Probe working directory"
    )?);
    report.runtime_details["temporary_workspace"] = json!(true);
    record_worker_protocol(report, Some(engine.capabilities()))?;
    report.adapter_details = adapter(&engine, startup_config.is_some());
    report.value_adapter = json!(
        engine
            .value_bridge_path()
            .map(|path| checked_utf8(path, "Value adapter path"))
            .transpose()?
    );
    if let Some(path) = engine.value_bridge_path() {
        let accepted = checked_utf8(path, "Accepted value adapter path")?;
        ensure!(
            report
                .artifact_identities
                .iter()
                .any(|identity| identity.kind == "value_adapter" && identity.path == accepted),
            "Accepted value adapter path does not match the preflight byte identity"
        );
    }
    let result = (|| {
        case(report, "runtime_profile", || {
            require_startup_profile(&engine, unavailable, startup_config)?;
            Ok(
                json!({"adapter":adapter(&engine, startup_config.is_some()),"context":engine.workspace_snapshot()?.context}),
            )
        })?;
        case(report, "uint64_checked_read_write_undo_redo", || {
            uint64_case(&mut engine)
        })?;
        if unavailable {
            // Beyond the common real integer case, this profile qualifies only
            // the two precise unavailable rejections.
            // Native precision/restart/Stop cases must never be implied by it.
            return unavailable_cases(report, &mut engine);
        }
        for precision in [Precision::Double, Precision::Single] {
            case(report, precision.case_id(), || {
                precision_case(&mut engine, precision)
            })?;
        }
        case(report, "stale_identity_rejected", || {
            stale_case(&mut engine)
        })?;
        case(report, "restart_preserves_native_profile", || {
            restart_case(&mut engine, unavailable, startup_config)
        })?;
        Ok(())
    })();
    // Capture after restart and failed cases while this probe owns the Engine.
    record_worker_protocol(report, Some(engine.capabilities()))?;
    let observation = match startup_config {
        Some(config) => observe_qualified_status(report, &engine, config),
        None => Ok(()),
    };
    result.and(observation)?;
    if unavailable {
        return Ok(());
    }
    let continuation = directory.path().join("unexpected-stop-continuation");
    let starts = directory.path().join("stop-body-starts");
    let mut stopped_status = None;
    let mut stopped_protocol = None;
    let result = case(report, "same_process_stop_recovers", || {
        stop_case(
            engine,
            unavailable,
            continuation,
            starts,
            startup_config,
            &mut stopped_status,
            &mut stopped_protocol,
        )
    });
    // Stop owns the final Engine. Refresh its actual protocol even on failure;
    // a failed/retired session must not keep an earlier Ready or capability.
    record_worker_protocol(report, stopped_protocol.as_ref())?;
    let observation = if startup_config.is_some() {
        record_qualified_status(report, stopped_status)
    } else {
        Ok(())
    };
    result.and(observation)
}

fn observe_qualified_status(
    report: &mut Report,
    engine: &Engine,
    config: &QualifiedStartupConfig,
) -> Result<()> {
    record_worker_protocol(report, Some(engine.capabilities()))?;
    if let Err(error) = config.require_active(engine) {
        // Selection is not a current accepted profile when its active paths or
        // required live graphics capability no longer match the configuration.
        report.qualified_startup_status = None;
        report.session_context = None;
        return Err(error.into());
    }
    record_qualified_status(report, engine.qualified_startup_status())
}

fn record_worker_protocol(
    report: &mut Report,
    protocol: Option<&WorkerCapabilities>,
) -> Result<()> {
    report.worker_capabilities = protocol
        .map(|protocol| serde_json::to_value(&protocol.capabilities))
        .transpose()?
        .unwrap_or_else(|| json!([]));
    report.worker_protocol = protocol
        .map(serde_json::to_value)
        .transpose()?
        .unwrap_or(Value::Null);
    Ok(())
}

fn record_qualified_status(
    report: &mut Report,
    status: Option<mathlab_engine::QualifiedStartupStatus>,
) -> Result<()> {
    let status = status.filter(|status| {
        status.profile != QUALIFIED_GRAPHICS_STARTUP_PROFILE
            || report
                .worker_capabilities
                .as_array()
                .is_some_and(|capabilities| {
                    capabilities.iter().any(|capability| {
                        capability.as_str() == Some(mathlab_engine::graphics::CAP_GRAPHICS_REGISTRY)
                    })
                })
    });
    report.session_context = status
        .as_ref()
        .map(|status| serde_json::to_value(&status.context))
        .transpose()?;
    report.qualified_startup_status = status.as_ref().map(serde_json::to_value).transpose()?;
    ensure!(
        status.is_some(),
        "Qualified probe has no current accepted startup status"
    );
    Ok(())
}

fn adapter(engine: &Engine, qualified: bool) -> Value {
    let configured = if qualified {
        None
    } else {
        std::env::var_os("MATHLAB_VALUE_BRIDGE").map(PathBuf::from)
    };
    let mut observation = json!({"path":engine.value_bridge_path().and_then(Path::to_str),
        "path_observation":engine.value_bridge_path().map(path_observation),
        "capability":NATIVE_CAPABILITY,"capability_observed":engine.capabilities().supports(NATIVE_CAPABILITY),
        "configured_environment_path":configured.as_deref().and_then(Path::to_str),
        "configured_environment_observation":configured.as_deref().map(path_observation)});
    if qualified {
        observation["configured_startup_path"] = json!(engine.value_bridge_path());
        observation["configured_startup_observation"] =
            json!(engine.value_bridge_path().map(path_observation));
    }
    observation
}

fn require_profile(engine: &Engine, unavailable: bool) -> Result<()> {
    engine.require_capability("typed_nd_page.v1")?;
    engine.require_capability("atomic_range_edit.v1")?;
    let native = engine.capabilities().supports(NATIVE_CAPABILITY);
    if unavailable {
        ensure!(
            !native && engine.value_bridge_path().is_none(),
            "native-unavailable requires BOTH absent observed capability and absent configured adapter path"
        );
    } else {
        ensure!(
            native,
            "native-required probe did not observe {NATIVE_CAPABILITY}"
        );
        let path = engine
            .value_bridge_path()
            .context("native capability has no observed adapter path")?;
        checked_utf8(path, "Value adapter path")?;
        ensure!(
            path.is_file() && path.extension().is_some_and(|extension| extension == "oct"),
            "observed adapter path is not a .oct file"
        );
        ensure!(
            fs::canonicalize(path)? == path,
            "observed adapter path is not canonical"
        );
    }
    Ok(())
}

fn require_startup_profile(
    engine: &Engine,
    unavailable: bool,
    startup_config: Option<&QualifiedStartupConfig>,
) -> Result<()> {
    require_profile(engine, unavailable)?;
    if let Some(config) = startup_config {
        config.require_active(engine)?;
    }
    Ok(())
}

fn execute(engine: &mut Engine, source: &str) -> Result<()> {
    let result = engine.execute_with_timeout(source, FIXTURE_EXECUTE_TIMEOUT)?;
    if let Some(error) = result.error {
        return Err(
            anyhow::Error::new(EngineError::Worker(error)).context("fixture/final bit oracle")
        );
    }
    Ok(())
}

fn token(snapshot: &WorkspaceSnapshot, name: &str) -> Result<BindingToken> {
    snapshot
        .variables
        .iter()
        .find(|value| value.token.name == name)
        .map(|value| value.token.clone())
        .with_context(|| format!("missing probe variable {name}"))
}

fn nested_steps() -> Vec<ValuePathStep> {
    vec![
        ValuePathStep::Field {
            indices: vec![1.into(), 1.into()],
            name: "cell".into(),
        },
        ValuePathStep::Cell {
            indices: vec![1.into(), 1.into()],
        },
        ValuePathStep::Field {
            indices: vec![1.into(), 1.into()],
            name: "leaf".into(),
        },
    ]
}

fn window(rows: u32, columns: u32) -> PageWindow {
    PageWindow {
        row_start: 1.into(),
        column_start: 1.into(),
        rows,
        columns,
        higher_indices: vec![],
    }
}

fn page_request(
    snapshot: &WorkspaceSnapshot,
    name: &str,
    steps: Vec<ValuePathStep>,
    rows: u32,
    columns: u32,
) -> Result<ValuePageRequest> {
    Ok(ValuePageRequest {
        path: ValuePath {
            root: token(snapshot, name)?,
            steps,
        },
        window: window(rows, columns),
    })
}

fn read(
    engine: &mut Engine,
    name: &str,
    steps: Vec<ValuePathStep>,
    rows: u32,
    columns: u32,
) -> Result<ValuePage> {
    let before = engine.workspace_snapshot()?;
    let request = page_request(&before, name, steps, rows, columns)?;
    let (page, metrics) = engine.read_value_page_checked_with_metrics(&request)?;
    ensure!(
        page.token == request.path.root && page.path == request.path.steps,
        "checked page identity mismatch"
    );
    ensure!(
        metrics.result_json_bytes <= PAGE_BYTES,
        "typed result exceeded its declared byte limit"
    );
    ensure!(
        engine.workspace_snapshot()?.context == before.context,
        "pure page revoked binding/history"
    );
    Ok(page)
}

fn edit_request(
    page: &ValuePage,
    selected: PageWindow,
    real: &NumericComponents,
    imag: Option<&NumericComponents>,
) -> Result<RangeEditRequest> {
    Ok(RangeEditRequest::from_components(
        ValuePath {
            root: page.token.clone(),
            steps: page.path.clone(),
        },
        selected,
        LeafDescriptor {
            class: page.class,
            source_shape: page.source_shape.clone(),
            source_storage: page.source_storage,
            complex: page.complex,
        },
        real,
        imag,
    )?)
}

fn history(receipt: &RangeEditReceipt, redo: bool) -> Result<HistoryRequest> {
    let head = if redo {
        receipt.history.redo.as_ref()
    } else {
        receipt.history.undo.as_ref()
    }
    .context("missing accepted history head")?;
    ensure!(
        head.root_name == receipt.token.name,
        "history root mismatch"
    );
    Ok(HistoryRequest {
        schema_version: 1,
        expected: receipt.token.clone(),
        history_epoch: receipt.history.epoch,
        transaction_id: head.transaction_id,
    })
}

fn receipt(
    before: &BindingToken,
    actual: &RangeEditReceipt,
    operation: EditOperation,
) -> Result<()> {
    let mut expected = before.clone();
    expected.context.revision = expected
        .context
        .revision
        .checked_add(1)
        .context("revision exhausted")?;
    ensure!(
        actual.token == expected && actual.operation == operation,
        "receipt did not advance the exact expected context once"
    );
    ensure!(
        (1..=MAX_HISTORY_RECORDS as u32).contains(&actual.retained_transactions),
        "history count outside bounded contract"
    );
    ensure!(
        (1..=MAX_HISTORY_BYTES).contains(&actual.retained_patch_bytes),
        "logical patch charge outside bounded contract"
    );
    Ok(())
}

fn expect_worker<T>(
    result: std::result::Result<T, EngineError>,
    identifier: &str,
) -> Result<String> {
    match result {
        Err(EngineError::Worker(error)) if error.identifier == identifier => Ok(error.identifier),
        Err(error) => Err(anyhow::Error::new(error)
            .context(format!("expected exact worker identifier {identifier}"))),
        Ok(_) => {
            bail!("operation unexpectedly succeeded; expected exact worker identifier {identifier}")
        }
    }
}

#[derive(Clone, Copy)]
enum Precision {
    Double,
    Single,
}

impl Precision {
    fn name(self) -> &'static str {
        match self {
            Self::Double => "double",
            Self::Single => "single",
        }
    }
    fn integer(self) -> &'static str {
        match self {
            Self::Double => "uint64",
            Self::Single => "uint32",
        }
    }
    fn class(self) -> NumericClass {
        match self {
            Self::Double => NumericClass::Double,
            Self::Single => NumericClass::Single,
        }
    }
    fn case_id(self) -> &'static str {
        match self {
            Self::Double => "double_checked_read_write_undo_redo",
            Self::Single => "single_checked_read_write_undo_redo",
        }
    }
    // Independent literal words: old NaN payload,4,6,5,11,new NaN payload,-3,7,-0.
    fn words(self) -> [u64; 9] {
        match self {
            Self::Double => [
                0x7ff8_0000_0000_0042,
                0x4010_0000_0000_0000,
                0x4018_0000_0000_0000,
                0x4014_0000_0000_0000,
                0x4026_0000_0000_0000,
                0xfff8_0000_0000_0099,
                0xc008_0000_0000_0000,
                0x401c_0000_0000_0000,
                0x8000_0000_0000_0000,
            ],
            Self::Single => [
                0x7fc0_0042,
                0x4080_0000,
                0x40c0_0000,
                0x40a0_0000,
                0x4130_0000,
                0xffc0_0099,
                0xc040_0000,
                0x40e0_0000,
                0x8000_0000,
            ],
        }
    }
    fn components(self, words: &[u64]) -> NumericComponents {
        match self {
            Self::Double => {
                NumericComponents::Double(words.iter().copied().map(f64::from_bits).collect())
            }
            Self::Single => NumericComponents::Single(
                words
                    .iter()
                    .map(|&word| f32::from_bits(word as u32))
                    .collect(),
            ),
        }
    }
    // Independent oracle transport: decimal uint32 halves, NEVER edit hex data.
    fn octave_words(self, words: &[u64]) -> String {
        let join = |values: Vec<u64>| {
            values
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        };
        match self {
            Self::Double => format!(
                "builtin('bitor',builtin('bitshift',builtin('uint64',[{}]),32),builtin('uint64',[{}]))",
                join(words.iter().map(|word| word >> 32).collect()),
                join(words.iter().map(|word| word & 0xffff_ffff).collect())
            ),
            Self::Single => format!("builtin('uint32',[{}])", join(words.to_vec())),
        }
    }
}

fn equal_components(actual: &NumericComponents, expected: &NumericComponents) -> bool {
    macro_rules! equal {
        ($actual:expr,$expected:expr) => {
            $actual.len() == $expected.len()
                && $actual.iter().zip($expected).all(|(a, e)| {
                    if e.is_nan() {
                        a.is_nan()
                    } else {
                        a.to_bits() == e.to_bits()
                    }
                })
        };
    }
    match (actual, expected) {
        (NumericComponents::Double(a), NumericComponents::Double(e)) => equal!(a, e),
        (NumericComponents::Single(a), NumericComponents::Single(e)) => equal!(a, e),
        _ => actual == expected,
    }
}

fn complex_page(page: &ValuePage, precision: Precision, real: &[u64], imag: &[u64]) -> Result<()> {
    ensure!(
        page.class == precision.class()
            && page.complex
            && page.source_storage == SourceStorage::Dense,
        "wrong complex page class/storage/state"
    );
    ensure!(
        page.source_shape == vec![DecimalU64::new(3), DecimalU64::new(1)],
        "wrong source shape"
    );
    let PageData::Dense {
        real: actual_real,
        imag: Some(actual_imag),
    } = &page.data
    else {
        bail!("dense complex page lost a component")
    };
    ensure!(
        equal_components(actual_real, &precision.components(real))
            && equal_components(actual_imag, &precision.components(imag)),
        "page value/signed-zero mismatch (NaN payloads are checked only by the final native bit oracle)"
    );
    Ok(())
}

fn bit_oracle(
    engine: &mut Engine,
    expression: &str,
    precision: Precision,
    real: &[u64],
    imag: &[u64],
) -> Result<()> {
    // Called only before a journey or AFTER its last history action. Extract
    // full components BEFORE scalar indexing so the oracle avoids narrowing.
    execute(
        engine,
        &format!(
            "probe_value={expression}; if (!builtin('strcmp',builtin('class',probe_value),'{ty}') || builtin('isreal',probe_value) || builtin('any',builtin('size',probe_value)!=[3,1])); builtin('error','MathLab:ValueProbeOracle','Wrong class/shape/complex state.'); endif; probe_re=builtin('real',probe_value); probe_im=builtin('imag',probe_value); probe_actual=builtin('typecast',probe_re(:),'{integer}'); probe_expected={re}; if (builtin('numel',probe_actual)!=builtin('numel',probe_expected) || builtin('any',probe_actual(:)!=probe_expected(:))); builtin('error','MathLab:ValueProbeOracle','Wrong real component bits.'); endif; probe_actual=builtin('typecast',probe_im(:),'{integer}'); probe_expected={im}; if (builtin('numel',probe_actual)!=builtin('numel',probe_expected) || builtin('any',probe_actual(:)!=probe_expected(:))); builtin('error','MathLab:ValueProbeOracle','Wrong imaginary component bits.'); endif;",
            ty = precision.name(),
            integer = precision.integer(),
            re = precision.octave_words(real),
            im = precision.octave_words(imag)
        ),
    )
}

fn precision_case(engine: &mut Engine, precision: Precision) -> Result<Value> {
    let [
        nan_old,
        four,
        six,
        five,
        eleven,
        nan_new,
        minus_three,
        seven,
        negative_zero,
    ] = precision.words();
    let old_re = [nan_old, four, six];
    let old_im = [negative_zero, negative_zero, five];
    // The new NaN is in the FIRST selected cell, so direct-write singleton
    // and two-row journeys both qualify its exact accepted payload bits.
    let rhs_re = [nan_new, eleven];
    let rhs_im = [minus_three, seven];
    let mut journeys = Vec::new();
    for rows in [1_u32, 2] {
        for (journey, with_undo, with_redo) in [
            ("write-only", false, false),
            ("write-undo", true, false),
            ("write-undo-redo", true, true),
        ] {
            execute(
                engine,
                &format!(
                    "R=builtin('struct'); R.cell=builtin('cell',1,2); R.cell{{1}}=builtin('struct'); R.cell{{1}}.leaf=builtin('complex',builtin('reshape',builtin('typecast',{},'{}'),3,1),builtin('reshape',builtin('typecast',{},'{}'),3,1)); R.cell{{2}}=builtin('uint64',23); R.peer=builtin('uint64',19); RootAlias=R; CellAlias=R.cell; LeafAlias=R.cell{{1}}.leaf;",
                    precision.octave_words(&old_re),
                    precision.name(),
                    precision.octave_words(&old_im),
                    precision.name()
                ),
            )?;
            bit_oracle(engine, "R.cell{1}.leaf", precision, &old_re, &old_im)?;
            let selected = read(engine, "R", nested_steps(), rows, 1)?;
            complex_page(
                &selected,
                precision,
                &old_re[..rows as usize],
                &old_im[..rows as usize],
            )?;
            let request = edit_request(
                &selected,
                window(rows, 1),
                &precision.components(&rhs_re[..rows as usize]),
                Some(&precision.components(&rhs_im[..rows as usize])),
            )?;
            let written = engine.write_value_range_checked(&request)?;
            receipt(&request.path.root, &written, EditOperation::Write)?;
            let new_re = if rows == 1 {
                [nan_new, four, six]
            } else {
                [nan_new, eleven, six]
            };
            let new_im = if rows == 1 {
                [minus_three, negative_zero, five]
            } else {
                [minus_three, seven, five]
            };
            complex_page(
                &read(engine, "R", nested_steps(), 3, 1)?,
                precision,
                &new_re,
                &new_im,
            )?;
            let last = if with_undo {
                let undo_request = history(&written, false)?;
                let undone = engine.undo_value_edit_checked(&undo_request)?;
                receipt(&undo_request.expected, &undone, EditOperation::Undo)?;
                complex_page(
                    &read(engine, "R", nested_steps(), 3, 1)?,
                    precision,
                    &old_re,
                    &old_im,
                )?;
                if with_redo {
                    let redo_request = history(&undone, true)?;
                    let redone = engine.redo_value_edit_checked(&redo_request)?;
                    receipt(&redo_request.expected, &redone, EditOperation::Redo)?;
                    complex_page(
                        &read(engine, "R", nested_steps(), 3, 1)?,
                        precision,
                        &new_re,
                        &new_im,
                    )?;
                    redone
                } else {
                    undone
                }
            } else {
                written
            };
            ensure!(
                engine.workspace_snapshot()?.context == last.token.context,
                "pure history checks changed context"
            );
            // No ordinary execute occurred between write/undo/redo above.
            let (final_re, final_im) = if !with_undo || with_redo {
                (&new_re, &new_im)
            } else {
                (&old_re, &old_im)
            };
            bit_oracle(engine, "R.cell{1}.leaf", precision, final_re, final_im)?;
            for alias in ["RootAlias.cell{1}.leaf", "CellAlias{1}.leaf", "LeafAlias"] {
                bit_oracle(engine, alias, precision, &old_re, &old_im)?;
            }
            execute(
                engine,
                "if (!builtin('strcmp',builtin('class',R.peer),'uint64') || builtin('numel',builtin('size',R.peer))!=2 || builtin('any',builtin('size',R.peer)!=[1,1]) || R.peer!=builtin('uint64',19) || !builtin('strcmp',builtin('class',RootAlias.peer),'uint64') || builtin('numel',builtin('size',RootAlias.peer))!=2 || builtin('any',builtin('size',RootAlias.peer)!=[1,1]) || RootAlias.peer!=builtin('uint64',19) || !builtin('strcmp',builtin('class',R.cell{2}),'uint64') || builtin('numel',builtin('size',R.cell{2}))!=2 || builtin('any',builtin('size',R.cell{2})!=[1,1]) || R.cell{2}!=builtin('uint64',23) || !builtin('strcmp',builtin('class',CellAlias{2}),'uint64') || builtin('numel',builtin('size',CellAlias{2}))!=2 || builtin('any',builtin('size',CellAlias{2})!=[1,1]) || CellAlias{2}!=builtin('uint64',23)); builtin('error','MathLab:ValueProbeOracle','Changed an untouched peer class/shape/value.'); endif;",
            )?;
            journeys.push(json!({"selected_rows":rows,"selected_columns":1,"journey":journey,"final_context":last.token.context,"retained_logical_patch_bytes":last.retained_patch_bytes,"exact_final_bit_oracle":true}));
        }
    }
    let snapshot = engine.workspace_snapshot()?;
    let oversized = page_request(&snapshot, "R", nested_steps(), 2001, 1)?;
    match engine.read_value_page_checked(&oversized) {
        Err(EngineError::ValuePage(ValuePageError::Limit {
            field: "window.cells",
            actual: 2001,
            limit: 2000,
        })) => {}
        Err(error) => {
            return Err(anyhow::Error::new(error)
                .context("expected precise local 2001-cell request rejection"));
        }
        Ok(_) => bail!("oversized page request was accepted"),
    }
    ensure!(
        engine.workspace_snapshot()?.context == snapshot.context,
        "local limit rejection changed context"
    );
    Ok(
        json!({"class":precision.name(),"path":"R.cell{1}.leaf","source_shape":[3,1],"journeys":journeys,"validated_page_cell_limit":2000,"tested_rejected_page_cells":2001,"nan_payload_oracle":"Independent builtin typecast words after last action; decimal pages compare NaN category only."}),
    )
}

fn lifecycle_complex_fixture() -> String {
    let precision = Precision::Double;
    let words = precision.words();
    format!(
        "Z=builtin('complex',[2;4;6],builtin('reshape',builtin('typecast',{},'double'),3,1));",
        precision.octave_words(&[words[8], words[8], words[3]])
    )
}

fn unavailable_cases(report: &mut Report, engine: &mut Engine) -> Result<()> {
    execute(engine, &lifecycle_complex_fixture())?;
    let before = engine.workspace_snapshot()?;
    let request = page_request(&before, "Z", vec![], 1, 1)?;
    case(report, "complex_read_requires_native", || {
        let observed = expect_worker(
            engine.read_value_page_checked(&request),
            "MathLab:ValuePagePrecisionUnavailable",
        )?;
        ensure!(
            engine.workspace_snapshot()?.context == before.context,
            "unavailable read changed context"
        );
        Ok(
            json!({"expected_error_identifier":"MathLab:ValuePagePrecisionUnavailable","observed_error_identifier":observed,"native_precision_success":false}),
        )
    })?;
    case(report, "complex_write_requires_native", || {
        let edit = RangeEditRequest::from_components(
            request.path.clone(),
            request.window.clone(),
            LeafDescriptor {
                class: NumericClass::Double,
                source_shape: vec![3.into(), 1.into()],
                source_storage: SourceStorage::Dense,
                complex: true,
            },
            &NumericComponents::Double(vec![11.]),
            Some(&NumericComponents::Double(vec![-3.])),
        )?;
        let observed = expect_worker(
            engine.write_value_range_checked(&edit),
            "MathLab:ValueEditPrecisionUnavailable",
        )?;
        ensure!(
            !engine.has_value_edit_uncertainty(),
            "known missing adapter rejection latched uncertainty"
        );
        ensure!(
            engine.workspace_snapshot()?.context == before.context,
            "unavailable write changed context"
        );
        // BOTH rejection/context checks have settled. This ordinary terminal
        // oracle now retires their contexts; no post-oracle unchanged-context
        // claim is made. It requires no adapter and distinguishes -0 from +0.
        bit_oracle(
            engine,
            "Z",
            Precision::Double,
            &[
                0x4000_0000_0000_0000,
                0x4010_0000_0000_0000,
                0x4018_0000_0000_0000,
            ],
            &[
                0x8000_0000_0000_0000,
                0x8000_0000_0000_0000,
                0x4014_0000_0000_0000,
            ],
        )?;
        Ok(
            json!({"expected_error_identifier":"MathLab:ValueEditPrecisionUnavailable","observed_error_identifier":observed,"native_precision_success":false,"exact_final_component_bits":true,"unchanged_values":true,"rejection_context_verified_before_terminal_oracle":true,"oracle_timeline":"Whole-leaf builtin typecast after both rejection/context checks; this final ordinary evaluation retires their contexts."}),
        )
    })
}

fn scalar_edit(engine: &mut Engine, name: &str, value: u64) -> Result<RangeEditRequest> {
    let page = read(engine, name, vec![], 1, 1)?;
    edit_request(
        &page,
        window(1, 1),
        &NumericComponents::Uint64(vec![value]),
        None,
    )
}

fn uint64_page(engine: &mut Engine, name: &str, expected: &[u64]) -> Result<()> {
    let page = read(engine, name, vec![], 1, expected.len() as u32)?;
    ensure!(
        page.class == NumericClass::Uint64
            && !page.complex
            && matches!(page.data,PageData::Dense {real:NumericComponents::Uint64(ref values),imag:None} if values.as_slice()==expected),
        "wrong exact uint64 values for {name}"
    );
    Ok(())
}

fn uint64_case(engine: &mut Engine) -> Result<Value> {
    // Every Octave literal is a uint32-sized half. No >2^53 integer passes
    // through a floating literal, and this is independent of edit decimal data.
    execute(
        engine,
        "U64Probe=builtin('bitor',builtin('bitshift',builtin('uint64',[2097152 4294967295 0]),32),builtin('uint64',[1 4294967295 17])); U64Alias=U64Probe; U64Peer=builtin('uint64',31);",
    )?;
    let old = [9_007_199_254_740_993_u64, u64::MAX, 17];
    let new = [u64::MAX, 9_007_199_254_740_995_u64, 17];
    let page = read(engine, "U64Probe", vec![], 1, 3)?;
    ensure!(
        page.source_shape == vec![1.into(), 3.into()],
        "wrong uint64 source shape"
    );
    uint64_page(engine, "U64Probe", &old)?;
    let request = edit_request(
        &page,
        window(1, 2),
        &NumericComponents::Uint64(new[..2].to_vec()),
        None,
    )?;
    let written = engine.write_value_range_checked(&request)?;
    receipt(&request.path.root, &written, EditOperation::Write)?;
    uint64_page(engine, "U64Probe", &new)?;
    uint64_page(engine, "U64Alias", &old)?;
    let undo_request = history(&written, false)?;
    let undone = engine.undo_value_edit_checked(&undo_request)?;
    receipt(&undo_request.expected, &undone, EditOperation::Undo)?;
    uint64_page(engine, "U64Probe", &old)?;
    let redo_request = history(&undone, true)?;
    let redone = engine.redo_value_edit_checked(&redo_request)?;
    receipt(&redo_request.expected, &redone, EditOperation::Redo)?;
    uint64_page(engine, "U64Probe", &new)?;
    uint64_page(engine, "U64Alias", &old)?;
    uint64_page(engine, "U64Peer", &[31])?;
    ensure!(
        engine.workspace_snapshot()?.context == redone.token.context,
        "pure uint64 checks invalidated history"
    );
    // Only after Redo: whole-value equality against literal u32 halves checks
    // exact integer identity, aliases, and the untouched third cell/peer.
    execute(
        engine,
        "probe_u64_expected=builtin('bitor',builtin('bitshift',builtin('uint64',[4294967295 2097152 0]),32),builtin('uint64',[4294967295 3 17])); probe_u64_old=builtin('bitor',builtin('bitshift',builtin('uint64',[2097152 4294967295 0]),32),builtin('uint64',[1 4294967295 17])); if (!builtin('strcmp',builtin('class',U64Probe),'uint64') || !builtin('strcmp',builtin('class',U64Alias),'uint64') || builtin('any',builtin('size',U64Probe)!=[1,3]) || builtin('any',builtin('size',U64Alias)!=[1,3]) || builtin('any',U64Probe!=probe_u64_expected) || builtin('any',U64Alias!=probe_u64_old) || !builtin('strcmp',builtin('class',U64Peer),'uint64') || builtin('numel',builtin('size',U64Peer))!=2 || builtin('any',builtin('size',U64Peer)!=[1,1]) || U64Peer!=builtin('uint64',31)); builtin('error','MathLab:ValueProbeOracle','Wrong exact uint64 values/alias/peer.'); endif;",
    )?;
    Ok(
        json!({"class":"uint64","source_shape":[1,3],"selected_rows":1,"selected_columns":2,
        "journey":"read-write-undo-redo","old_values":["9007199254740993","18446744073709551615","17"],
        "new_values":["18446744073709551615","9007199254740995","17"],
        "final_context":redone.token.context,"exact_terminal_integer_oracle":true,
        "original_alias_preserved":true,"untouched_cell_and_peer_preserved":true,
        "native_adapter_required":false,"oracle_timeline":"Literal uint32 halves after final Redo; no ordinary evaluation between history actions."}),
    )
}

fn stale_case(engine: &mut Engine) -> Result<Value> {
    execute(engine, "A=builtin('uint64',[1,2]);")?;
    let request = scalar_edit(engine, "A", 9)?;
    let old_page = ValuePageRequest {
        path: request.path.clone(),
        window: window(1, 1),
    };
    let written = engine.write_value_range_checked(&request)?;
    receipt(&request.path.root, &written, EditOperation::Write)?;
    let stale_read = expect_worker(
        engine.read_value_page_checked(&old_page),
        "MathLab:StaleBinding",
    )?;
    let stale_write = expect_worker(
        engine.write_value_range_checked(&request),
        "MathLab:StaleBinding",
    )?;
    let old_history = history(&written, false)?;
    let before_reset = engine.workspace_snapshot()?;
    ensure!(
        before_reset.context == written.token.context,
        "rejected old token changed context"
    );
    let reset = engine.reset_value_edit_history()?;
    ensure!(
        reset.context.revision == before_reset.context.revision + 1
            && reset.context.invocation == before_reset.context.invocation,
        "reset did not return one fresh revision"
    );
    uint64_page(engine, "A", &[9, 2])?;
    let stale_history = expect_worker(
        engine.undo_value_edit_checked(&old_history),
        "MathLab:StaleBinding",
    )?;
    let mut retired_head = old_history;
    retired_head.expected = token(&reset, "A")?;
    let retired_history = expect_worker(
        engine.undo_value_edit_checked(&retired_head),
        "MathLab:ValueEditHistoryConflict",
    )?;
    ensure!(
        !engine.has_value_edit_uncertainty(),
        "known identity/history rejections latched uncertainty"
    );
    ensure!(
        engine.workspace_snapshot()?.context == reset.context,
        "rejected authority changed context"
    );
    uint64_page(engine, "A", &[9, 2])?;
    Ok(
        json!({"stale_page_error_identifier":stale_read,"stale_write_error_identifier":stale_write,"stale_binding_history_error_identifier":stale_history,"retired_history_error_identifier":retired_history,"reset_context":reset.context,"reset_preserved_values":[9,2],"automatic_retry_performed_by_probe":false}),
    )
}

fn restart_case(
    engine: &mut Engine,
    unavailable: bool,
    startup_config: Option<&QualifiedStartupConfig>,
) -> Result<Value> {
    require_startup_profile(engine, unavailable, startup_config)?;
    execute(engine, "probe_restart_marker=builtin('uint64',17);")?;
    let before = engine.workspace_snapshot()?;
    let old = page_request(&before, "probe_restart_marker", vec![], 1, 1)?;
    let path = engine.value_bridge_path().map(ToOwned::to_owned);
    let pid = engine.process_id();
    match startup_config {
        Some(config) => config.restart(engine)?,
        None => engine.restart()?,
    }
    require_startup_profile(engine, unavailable, startup_config)?;
    let after = engine.workspace_snapshot()?;
    ensure!(
        after.context.generation > before.context.generation,
        "restart did not advance generation"
    );
    ensure!(
        after.variables.is_empty(),
        "restart retained user variables in the owned isolated workspace"
    );
    ensure!(
        engine.value_bridge_path() == path.as_deref(),
        "restart changed configured adapter path"
    );
    expect_worker(engine.read_value_page_checked(&old), "MathLab:StaleBinding")?;
    execute(
        engine,
        &format!("A=builtin('uint64',1); {}", lifecycle_complex_fixture()),
    )?;
    uint64_page(engine, "A", &[1])?;
    check_recovered_complex(engine, unavailable)?;
    Ok(
        json!({"before_context":before.context,"after_context":after.context,"before_process_id":pid,"after_process_id":engine.process_id(),"adapter":adapter(engine, startup_config.is_some()),"profile_reinstalled":!unavailable,"workspace_cleared":true,"fresh_workspace_variable_count":after.variables.len(),"old_token_error_identifier":"MathLab:StaleBinding"}),
    )
}

fn check_recovered_complex(engine: &mut Engine, unavailable: bool) -> Result<()> {
    if unavailable {
        let request = page_request(&engine.workspace_snapshot()?, "Z", vec![], 1, 1)?;
        expect_worker(
            engine.read_value_page_checked(&request),
            "MathLab:ValuePagePrecisionUnavailable",
        )?;
    } else {
        let precision = Precision::Double;
        let words = precision.words();
        let page = read(engine, "Z", vec![], 2, 1)?;
        complex_page(
            &page,
            precision,
            &[0x4000_0000_0000_0000, words[1]],
            &[words[8], words[8]],
        )?;
    }
    Ok(())
}

#[cfg(unix)]
fn stop_case(
    mut engine: Engine,
    unavailable: bool,
    continuation: PathBuf,
    starts: PathBuf,
    startup_config: Option<&QualifiedStartupConfig>,
    qualified_status: &mut Option<mathlab_engine::QualifiedStartupStatus>,
    worker_protocol: &mut Option<WorkerCapabilities>,
) -> Result<Value> {
    require_startup_profile(&engine, unavailable, startup_config)?;
    execute(&mut engine, "A=builtin('uint64',1);")?;
    let write = scalar_edit(&mut engine, "A", 9)?;
    let written = engine.write_value_range_checked(&write)?;
    receipt(&write.path.root, &written, EditOperation::Write)?;
    let old_history = history(&written, false)?;
    let before = engine.workspace_snapshot()?.context;
    let pid = engine
        .process_id()
        .context("Stop needs an observed worker PID")?;
    let cancellation = engine.cancellation_handle();
    struct TerminateOnDrop(Option<CancellationHandle>);
    impl Drop for TerminateOnDrop {
        fn drop(&mut self) {
            if let Some(handle) = &self.0 {
                let _ = handle.terminate();
            }
        }
    }
    let mut cleanup = TerminateOnDrop(Some(cancellation.clone()));
    let subscription = engine.subscribe_console();
    let marker = b"MATHLAB_VALUE_API_STOP_READY";
    let source = format!(
        "probe_start_file=builtin('fopen',{},'a'); builtin('fprintf',probe_start_file,'start\\n'); builtin('fclose',probe_start_file); stop_partial=builtin('uint64',23); clear functions; builtin('fprintf',builtin('stdout'),'MATHLAB_VALUE_API_STOP_READY'); builtin('fflush',builtin('stdout')); probe_stop_clock=builtin('tic'); while (builtin('toc',probe_stop_clock)<12); builtin('pause',0.01); endwhile; probe_continuation=builtin('fopen',{},'w'); builtin('fclose',probe_continuation);",
        octave_quote(&starts)?,
        octave_quote(&continuation)?
    );
    let task = thread::spawn(move || {
        let result = engine.execute_with_timeout(&source, FIXTURE_EXECUTE_TIMEOUT);
        (engine, result)
    });
    let deadline = Instant::now() + STOP_BARRIER_TIMEOUT;
    let mut bytes = Vec::new();
    let mut reached = false;
    while Instant::now() < deadline && !reached {
        if let Ok(event) = subscription.recv_timeout(Duration::from_millis(50)) {
            match event.kind {
                ConsoleEventKind::Output { bytes: chunk } => bytes.extend(chunk),
                ConsoleEventKind::End | ConsoleEventKind::Disconnected { .. } => break,
                _ => {}
            }
            if bytes.len() > 64 * 1024 {
                break;
            }
            reached = bytes.windows(marker.len()).any(|part| part == marker);
        }
    }
    if !reached {
        let _ = cancellation.terminate();
        let _ = task.join();
        bail!("worker did not reach the bounded Stop output barrier");
    }
    if let Err(error) = cancellation.interrupt() {
        let _ = cancellation.terminate();
        let _ = task.join();
        return Err(error.into());
    }
    let deadline = Instant::now() + STOP_SETTLE_TIMEOUT;
    while !task.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    if !task.is_finished() {
        let _ = cancellation.terminate();
        let _ = task.join();
        bail!("Stop did not settle within its watchdog");
    }
    let (mut engine, result) = task
        .join()
        .map_err(|_| anyhow::anyhow!("Stop worker thread panicked"))?;
    cleanup.0 = None;
    let outcome = (|| {
        let result = result?;
        let error = result
            .error
            .context("Stop returned normal completion instead of Interrupted")?;
        ensure!(
            error.identifier == "MathLab:Interrupted",
            "wrong Stop identifier: {}",
            error.identifier
        );
        ensure!(
            engine.process_id() == Some(pid),
            "Stop replaced the numerical process"
        );
        require_startup_profile(&engine, unavailable, startup_config)?;
        let after = engine.workspace_snapshot()?;
        ensure!(
            after.context.worker_id == before.worker_id
                && after.context.generation == before.generation
                && after.context.invocation > before.invocation,
            "Stop did not recover a fresh invocation in the same generation"
        );
        ensure!(
            !continuation.exists(),
            "ordinary source continued after Stop"
        );
        ensure!(
            fs::read_to_string(starts)? == "start\n",
            "canceled ordinary body was replayed"
        );
        uint64_page(&mut engine, "A", &[9])?;
        uint64_page(&mut engine, "stop_partial", &[23])?;
        let stale_read = ValuePageRequest {
            path: ValuePath {
                root: written.token.clone(),
                steps: vec![],
            },
            window: window(1, 1),
        };
        expect_worker(
            engine.read_value_page_checked(&stale_read),
            "MathLab:StaleBinding",
        )?;
        let mut obsolete = old_history;
        obsolete.expected = token(&after, "A")?;
        expect_worker(
            engine.undo_value_edit_checked(&obsolete),
            "MathLab:ValueEditHistoryConflict",
        )?;
        ensure!(
            engine.workspace_snapshot()?.context == after.context,
            "history rejection changed recovered context"
        );
        let reset = engine.reset_value_edit_history()?;
        ensure!(
            reset.context.revision == after.context.revision + 1,
            "explicit recovered history reset did not advance revision once"
        );
        uint64_page(&mut engine, "A", &[9])?;
        uint64_page(&mut engine, "stop_partial", &[23])?;
        check_recovered_complex(&mut engine, unavailable)?;
        Ok(
            json!({"process_id":pid,"before_context":before,"recovered_context":after.context,"reset_context":reset.context,"adapter":adapter(&engine, startup_config.is_some()),"observed_stop_identifier":error.identifier,"source_start_count":1,"continuation_absent":true,"old_authority_rejected":true,"retained_values":{"A":9,"stop_partial":23},"bounded_source_wait_seconds":12,"commit_window_test":false}),
        )
    })();
    *worker_protocol = Some(engine.capabilities().clone());
    *qualified_status = startup_config
        .filter(|config| config.require_active(&engine).is_ok())
        .and_then(|_| engine.qualified_startup_status());
    outcome
}

#[cfg(not(unix))]
fn stop_case(
    _engine: Engine,
    _unavailable: bool,
    _continuation: PathBuf,
    _starts: PathBuf,
    _startup_config: Option<&QualifiedStartupConfig>,
    _qualified_status: &mut Option<mathlab_engine::QualifiedStartupStatus>,
    _worker_protocol: &mut Option<WorkerCapabilities>,
) -> Result<Value> {
    bail!(
        "same_process_stop_recovers requires the supported Unix signal/session profile; this platform is not a successful skipped case"
    )
}

#[cfg(unix)]
fn octave_quote(path: &std::path::Path) -> Result<String> {
    let path = path.to_str().context("probe path is not UTF-8")?;
    ensure!(
        !path
            .chars()
            .any(|value| matches!(value, '\0' | '\n' | '\r')),
        "probe path contains a forbidden control character"
    );
    Ok(format!("'{}'", path.replace('\'', "''")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report_fixture() -> Report {
        Report {
            schema_version: 1,
            kind: "mathlab.value-api-probe",
            profile: "native-required",
            status: "failed",
            build: Value::Null,
            runtime: Value::Null,
            runtime_details: Value::Null,
            adapter_preflight: vec![],
            value_adapter: Value::Null,
            adapter_details: Value::Null,
            worker_capabilities: json!([]),
            worker_protocol: Value::Null,
            artifact_identities: vec![],
            limits: Value::Null,
            cases: vec![],
            scope: Value::Null,
            error: None,
            native_modules: None,
            qualified_startup_status: None,
            session_context: None,
        }
    }

    fn qualified_status_fixture(profile: &'static str) -> mathlab_engine::QualifiedStartupStatus {
        // Serialization data only; this fixture does not establish admission or
        // a live runtime/capability observation.
        let mut status = mathlab_engine::QualifiedStartupStatus {
            profile,
            context: mathlab_engine::SessionContext {
                worker_id: "serialization-test-worker".into(),
                generation: u64::MAX,
                invocation: 9_007_199_254_740_993,
                revision: 0,
            },
            intrinsics: serde_json::from_value(json!({
                "protocol":"mathlab-intrinsics", "version":"1", "phase":"ready",
                "provider_file":"/test/mathlab_intrinsic_bridge.oct", "worker_file":"/test/worker.m",
                "registry_file":"/test/registry.m", "entry_alias":"__mlentry_test",
                "octave_version":"11.3.0", "octave_api_version":"api-v61",
                "initial_path_admitted":true, "shutdown_alias":"__mlshutdown_test",
                "shutdown_registered":true, "adapters":{}
            }))
            .unwrap(),
        };
        if profile == QUALIFIED_GRAPHICS_STARTUP_PROFILE {
            for (role, protocol, version) in [
                ("input", "mathlab-input", "1"),
                ("scope", "mathlab-scope", "2"),
                ("value", "mathlab-value", "1"),
                ("debug", "mathlab-debugger", "4"),
                ("graphics", "mathlab-graphics", "1"),
            ] {
                status.intrinsics.adapters.insert(
                    role.into(),
                    mathlab_engine::AdapterReady {
                        path: format!("/test/mathlab_{role}_bridge.oct"),
                        protocol: protocol.into(),
                        version: version.into(),
                    },
                );
            }
        }
        status
    }

    fn worker_protocol_fixture(graphics: bool) -> WorkerCapabilities {
        let mut capabilities = vec![NATIVE_CAPABILITY.to_owned()];
        if graphics {
            capabilities.push(mathlab_engine::graphics::CAP_GRAPHICS_REGISTRY.into());
        }
        WorkerCapabilities {
            name: "mathlab-worker".into(),
            version: 1,
            capabilities,
            limits: mathlab_engine::WorkerLimits {
                variable_cells: 2000,
                response_bytes: 32 * 1024 * 1024,
            },
        }
    }

    #[test]
    fn qualified_artifact_kind_accepts_exact_core_and_graphics_basenames() {
        for (name, kind) in [
            ("mathlab_intrinsic_bridge.oct", "intrinsic_provider"),
            ("mathlab_input_bridge.oct", "input_adapter"),
            ("mathlab_scope_bridge.oct", "scope_adapter"),
            ("mathlab_value_bridge.oct", "value_adapter"),
            ("mathlab_debug_bridge.oct", "debug_adapter"),
            ("mathlab_graphics_bridge.oct", "graphics_adapter"),
        ] {
            assert_eq!(qualified_artifact_kind(name).unwrap(), kind);
        }
    }

    #[test]
    fn qualified_artifact_kind_rejects_unknown_or_nonbasename_names() {
        for name in [
            "graphics_bridge.oct",
            "mathlab_graphics_bridge.OCT",
            "mathlab_graphics_bridge.oct.bak",
            "/test/mathlab_graphics_bridge.oct",
            "../mathlab_graphics_bridge.oct",
            "mathlab_unknown_bridge.oct",
            "",
        ] {
            assert!(qualified_artifact_kind(name).is_err(), "accepted {name}");
        }
    }

    #[test]
    fn legacy_report_serialization_does_not_claim_qualified_admission() {
        let report = report_fixture();
        let value = serde_json::to_value(report).unwrap();
        for field in [
            "native_modules",
            "qualified_startup_status",
            "session_context",
        ] {
            assert!(
                value.get(field).is_none(),
                "unexpected legacy field: {field}"
            );
        }
    }

    #[test]
    fn retired_qualified_session_clears_its_previous_report_observation() {
        let mut report = report_fixture();
        report.qualified_startup_status = Some(json!({"profile":"stale-test-observation"}));
        report.session_context = Some(json!({"worker_id":"retired-test-worker"}));
        assert!(record_qualified_status(&mut report, None).is_err());
        let value = serde_json::to_value(report).unwrap();
        assert!(value.get("qualified_startup_status").is_none());
        assert!(value.get("session_context").is_none());
    }

    #[test]
    fn qualified_report_preserves_engine_status_and_decimal_session_context() {
        let mut report = report_fixture();
        let status = qualified_status_fixture("qualified-core-v2");
        let expected = serde_json::to_value(&status).unwrap();
        record_qualified_status(&mut report, Some(status)).unwrap();
        let value = serde_json::to_value(report).unwrap();
        assert_eq!(value["qualified_startup_status"], expected);
        assert_eq!(value["session_context"], expected["context"]);
        assert_eq!(value["session_context"]["generation"], u64::MAX.to_string());
        assert_eq!(value["session_context"]["invocation"], "9007199254740993");
        assert_eq!(value["session_context"]["revision"], "0");
    }

    #[test]
    fn graphics_report_preserves_six_module_pins_and_actual_status() {
        let mut report = report_fixture();
        let names = [
            "mathlab_intrinsic_bridge.oct",
            "mathlab_input_bridge.oct",
            "mathlab_scope_bridge.oct",
            "mathlab_value_bridge.oct",
            "mathlab_debug_bridge.oct",
            "mathlab_graphics_bridge.oct",
        ];
        let modules: std::collections::BTreeMap<_, _> = names
            .iter()
            .map(|name| {
                (
                    name.to_string(),
                    mathlab_engine::startup_config::NativeModuleIdentity {
                        path: PathBuf::from(format!("/test/{name}")),
                        sha256: "a0".repeat(32),
                    },
                )
            })
            .collect();
        let expected_modules = serde_json::to_value(modules).unwrap();
        report.native_modules = Some(expected_modules.clone());
        let protocol = worker_protocol_fixture(true);
        record_worker_protocol(&mut report, Some(&protocol)).unwrap();
        let status = qualified_status_fixture(QUALIFIED_GRAPHICS_STARTUP_PROFILE);
        let expected_status = serde_json::to_value(&status).unwrap();
        record_qualified_status(&mut report, Some(status)).unwrap();
        let value = serde_json::to_value(report).unwrap();
        assert_eq!(value["profile"], "native-required");
        assert_eq!(value["status"], "failed");
        assert_eq!(value["native_modules"], expected_modules);
        assert_eq!(value["native_modules"].as_object().unwrap().len(), 6);
        assert_eq!(value["qualified_startup_status"], expected_status);
        assert_eq!(value["session_context"], expected_status["context"]);
        assert_eq!(
            value["worker_protocol"],
            serde_json::to_value(protocol).unwrap()
        );
        assert_eq!(
            value["worker_capabilities"],
            json!([NATIVE_CAPABILITY, "graphics_registry.v1"])
        );
    }

    #[test]
    fn graphics_report_clears_status_when_final_capability_is_absent() {
        let mut report = report_fixture();
        let status = qualified_status_fixture(QUALIFIED_GRAPHICS_STARTUP_PROFILE);
        record_worker_protocol(&mut report, Some(&worker_protocol_fixture(true))).unwrap();
        record_qualified_status(&mut report, Some(status.clone())).unwrap();
        record_worker_protocol(&mut report, Some(&worker_protocol_fixture(false))).unwrap();
        assert!(record_qualified_status(&mut report, Some(status)).is_err());
        let value = serde_json::to_value(report).unwrap();
        assert!(value.get("qualified_startup_status").is_none());
        assert!(value.get("session_context").is_none());
        assert_eq!(value["worker_capabilities"], json!([NATIVE_CAPABILITY]));
        assert_eq!(
            value["worker_protocol"]["capabilities"],
            json!([NATIVE_CAPABILITY])
        );
    }

    #[test]
    fn missing_final_worker_observation_clears_previous_capabilities() {
        let mut report = report_fixture();
        record_worker_protocol(&mut report, Some(&worker_protocol_fixture(true))).unwrap();
        record_worker_protocol(&mut report, None).unwrap();
        assert_eq!(report.worker_capabilities, json!([]));
        assert_eq!(report.worker_protocol, Value::Null);
    }

    #[test]
    fn artifact_budget_accepts_six_qualified_modules_and_rejects_extra() {
        let mut report = report_fixture();
        let directory = tempfile::tempdir().unwrap();
        for (name, kind) in [
            ("mathlab", "cli_executable"),
            ("octave-cli", "octave_runtime"),
            ("mathlab_intrinsic_bridge.oct", "intrinsic_provider"),
            ("mathlab_input_bridge.oct", "input_adapter"),
            ("mathlab_scope_bridge.oct", "scope_adapter"),
            ("mathlab_value_bridge.oct", "value_adapter"),
            ("mathlab_debug_bridge.oct", "debug_adapter"),
            ("mathlab_graphics_bridge.oct", "graphics_adapter"),
        ] {
            let selected = directory.path().join(name);
            fs::write(&selected, name).unwrap();
            let canonical = selected.canonicalize().unwrap();
            bind_artifact(&mut report, kind, None, &canonical, None).unwrap();
        }
        assert_eq!(report.artifact_identities.len(), 8);
        let graphics = report.artifact_identities.last().unwrap();
        assert_eq!(graphics.kind, "graphics_adapter");
        let extra = PathBuf::from(&graphics.path);
        assert!(bind_artifact(&mut report, "graphics_adapter", None, &extra, None).is_err());
        assert_eq!(report.artifact_identities.len(), 8);
    }

    #[test]
    fn independent_double_oracle_uses_exact_u32_halves() {
        let words = Precision::Double.octave_words(&[0x8000_0000_0000_0000, 0x7ff8_0000_0000_0042]);
        assert!(words.contains("[2147483648 2146959360]"));
        assert!(words.contains("[0 66]"));
        assert!(!words.contains("9223372036854775808"));
    }

    #[test]
    fn page_comparison_preserves_zero_sign_but_does_not_claim_nan_payloads() {
        assert!(!equal_components(
            &NumericComponents::Single(vec![0.]),
            &NumericComponents::Single(vec![-0.])
        ));
        assert!(equal_components(
            &NumericComponents::Double(vec![f64::from_bits(0x7ff8_0000_0000_0042)]),
            &NumericComponents::Double(vec![f64::from_bits(0x7ff8_0000_0000_0099)])
        ));
    }

    #[test]
    fn wrong_worker_identifier_and_success_are_not_expected_rejections() {
        assert!(expect_worker::<()>(Ok(()), "MathLab:ValuePagePrecisionUnavailable").is_err());
        assert!(
            expect_worker::<()>(
                Err(EngineError::Protocol("missing adapter".into())),
                "MathLab:ValuePagePrecisionUnavailable"
            )
            .is_err()
        );
        assert!(
            expect_worker::<()>(
                Err(EngineError::Worker(mathlab_engine::EngineError {
                    message: "missing adapter".into(),
                    identifier: "MathLab:Interrupted".into(),
                    stack: vec![],
                })),
                "MathLab:ValuePagePrecisionUnavailable"
            )
            .is_err()
        );
        assert_eq!(
            expect_worker::<()>(
                Err(EngineError::Worker(mathlab_engine::EngineError {
                    message: "exact missing adapter".into(),
                    identifier: "MathLab:ValuePagePrecisionUnavailable".into(),
                    stack: vec![],
                })),
                "MathLab:ValuePagePrecisionUnavailable"
            )
            .unwrap(),
            "MathLab:ValuePagePrecisionUnavailable"
        );
    }

    #[test]
    fn executable_digest_requires_exact_lowercase_sha256() {
        assert!(is_sha256(&"a0".repeat(32)));
        assert!(!is_sha256(&"A0".repeat(32)));
        assert!(!is_sha256(&"a".repeat(63)));
        assert!(!is_sha256(&"a".repeat(65)));
        assert!(!is_sha256(&"g0".repeat(32)));
        assert!(!is_sha256(""));
    }

    #[test]
    fn artifact_verification_detects_real_file_mutation_and_retains_both_hashes() {
        let directory = tempfile::tempdir().unwrap();
        let selected = directory.path().join("artifact");
        fs::write(&selected, b"before").unwrap();
        let path = selected.canonicalize().unwrap();
        let mut identity = ArtifactIdentity {
            kind: "octave_runtime",
            configured_by: None,
            path: checked_utf8(&path, "test artifact").unwrap(),
            sha256_before: Some(hash_file(&path).unwrap()),
            sha256_after: None,
            unchanged: None,
            error: None,
        };
        verify_artifact(&mut identity).unwrap();
        assert_eq!(identity.unchanged, Some(true));
        fs::write(&path, b"after").unwrap();
        assert!(verify_artifact(&mut identity).is_err());
        assert_eq!(identity.unchanged, Some(false));
        assert_eq!(identity.sha256_after, Some(hash_file(&path).unwrap()));
        assert_ne!(identity.sha256_before, identity.sha256_after);
        assert!(
            identity
                .error
                .as_ref()
                .is_some_and(|error| error.contains("bytes changed"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn runtime_identity_resolves_symlinks_before_cwd_change_and_rejects_bad_targets() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("runtime-target");
        fs::write(&target, []).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
        let selected = directory.path().join("runtime-selected");
        symlink(&target, &selected).unwrap();
        let resolved = canonical_runtime(&selected).unwrap();
        assert!(resolved.is_absolute());
        assert_eq!(resolved, target.canonicalize().unwrap());
        assert!(checked_utf8(&resolved, "runtime").is_ok());
        assert!(canonical_runtime(directory.path()).is_err());
        assert!(canonical_runtime(&directory.path().join("missing")).is_err());
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(canonical_runtime(&selected).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_path_reporting_is_safe_and_cannot_become_successful_identity() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let directory = tempfile::tempdir().unwrap();
        let target = directory
            .path()
            .join(OsString::from_vec(b"runtime-\xff".to_vec()));
        let observation = path_observation(&target);
        assert_eq!(observation["path_utf8"], false);
        assert_eq!(observation["display_is_lossy"], true);
        assert!(serde_json::to_vec(&observation).is_ok());
        assert!(checked_utf8(&target, "runtime").is_err());
        // Raw invalid arguments are safe even on APFS, which refuses creating
        // such directory entries with EILSEQ before the identity guard runs.
        assert!(canonical_runtime(&target).is_err());
        #[cfg(not(target_os = "macos"))]
        {
            use std::os::unix::fs::PermissionsExt;
            // Existing invalid-byte target qualification is pending on Linux;
            // never mistake the raw-argument case above for that stronger case.
            fs::write(&target, []).unwrap();
            fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
            assert!(canonical_runtime(&target).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn bare_runtime_uses_first_executable_path_entry_before_temporary_cwd() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        let name = Path::new("probe-runtime");
        fs::write(first.join(name), []).unwrap();
        fs::write(second.join(name), []).unwrap();
        fs::set_permissions(first.join(name), fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(second.join(name), fs::Permissions::from_mode(0o700)).unwrap();
        let search_path = std::env::join_paths([&first, &second]).unwrap();
        assert_eq!(
            resolve_runtime(name, Some(&search_path)).unwrap(),
            second.join(name).canonicalize().unwrap()
        );
        assert!(resolve_runtime(name, None).is_err());
        assert!(resolve_runtime(Path::new("not-present"), Some(&search_path)).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn adapter_preflight_rejects_non_utf8_spelling_and_target_before_engine_start() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let directory = tempfile::tempdir().unwrap();
        let valid = directory.path().join("valid.oct");
        fs::write(&valid, []).unwrap();
        assert_eq!(
            canonical_adapter(&valid).unwrap(),
            valid.canonicalize().unwrap()
        );
        let invalid = directory
            .path()
            .join(OsString::from_vec(b"invalid-\xff.oct".to_vec()));
        assert!(canonical_adapter(&invalid).is_err());
        let invalid_alias = directory
            .path()
            .join(OsString::from_vec(b"alias-\xff.oct".to_vec()));
        assert!(canonical_adapter(&invalid_alias).is_err());
        #[cfg(not(target_os = "macos"))]
        {
            use std::os::unix::fs::symlink;
            // APFS cannot create these invalid-byte entries. These stronger
            // existing-target/alias cases remain unrun Linux qualification.
            fs::write(&invalid, []).unwrap();
            assert!(canonical_adapter(&invalid).is_err());
            let visible_link = directory.path().join("visible-link.oct");
            symlink(&invalid, &visible_link).unwrap();
            assert!(canonical_adapter(&visible_link).is_err());
            symlink(&valid, &invalid_alias).unwrap();
            assert!(canonical_adapter(&invalid_alias).is_err());
        }
        let wrong_suffix = directory.path().join("plain.m");
        fs::write(&wrong_suffix, []).unwrap();
        assert!(canonical_adapter(&wrong_suffix).is_err());
        assert!(canonical_adapter(directory.path()).is_err());
        assert!(canonical_adapter(&directory.path().join("missing.oct")).is_err());
    }
}
