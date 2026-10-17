// SPDX-License-Identifier: GPL-3.0-or-later
//! Selected startup admission, not vendor authentication.
//! The trusted caller supplies every installation/native path and SHA-256 pin.
//! Only those files are hashed; native artifacts may be outside the installation.
//! Runtime directory contents and executable/module dependencies remain a caller
//! trust boundary. No native artifact directory is added to the function path.
//! Revalidation narrows replacement races but cannot freeze the filesystem or
//! eliminate the interval between checking a pathname and the child opening it.

use super::qualified_ready::{AdapterReady, IntrinsicsReady, QualifiedReadyProfile};
use super::{Error, Result, canonical_wire_path, wire_path};
use serde::{Deserialize, Deserializer};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

const VERSION: &str = "11.3.0";
const API: &str = "api-v61";
const MAX_DIRECTORIES: usize = 511; // Native path budget includes the session.
const MAX_PATH_BYTES: usize = 65_536;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedFile {
    pub path: PathBuf,
    #[serde(deserialize_with = "deserialize_sha256")]
    pub sha256: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterRole {
    Input,
    Scope,
    Value,
    Debug,
    Graphics,
}

impl AdapterRole {
    fn identity(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Input => ("input", "mathlab-input", "1"),
            Self::Scope => ("scope", "mathlab-scope", "2"),
            Self::Value => ("value", "mathlab-value", "1"),
            Self::Debug => ("debug", "mathlab-debugger", "4"),
            Self::Graphics => ("graphics", "mathlab-graphics", "1"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterConfig {
    #[serde(deserialize_with = "deserialize_adapter_role")]
    pub role: AdapterRole,
    pub file: PinnedFile,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedStartupConfig {
    pub installation_root: PathBuf,
    pub executable: PinnedFile,
    pub provider: PinnedFile,
    // Explicit auxiliary needed by the worker's pre-entry gnuplot warmup.
    // It may be in a separate installation; it is not discovered through PATH.
    pub gnuplot: PinnedFile,
    // Explicit export auxiliary selected through Octave's supported GSC setting.
    // It remains external and is never discovered through the utility PATH.
    pub ghostscript: PinnedFile,
    pub runtime_directories: Vec<PathBuf>,
    pub adapters: Vec<AdapterConfig>,
    pub octave_version: String,
    pub octave_api_version: String,
}

fn deserialize_adapter_role<'de, D>(deserializer: D) -> std::result::Result<AdapterRole, D::Error>
where
    D: Deserializer<'de>,
{
    AdapterRole::deserialize(serde::de::value::StringDeserializer::new(
        String::deserialize(deserializer)?,
    ))
}

fn deserialize_sha256<'de, D>(deserializer: D) -> std::result::Result<[u8; 32], D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(serde::de::Error::custom(
            "SHA-256 pin must contain exactly 64 lowercase hexadecimal digits",
        ));
    }
    let digit = |byte: u8| {
        if byte.is_ascii_digit() {
            byte - b'0'
        } else {
            byte - b'a' + 10
        }
    };
    let mut hash = [0; 32];
    for (slot, pair) in hash.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        *slot = digit(pair[0]) * 16 + digit(pair[1]);
    }
    Ok(hash)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Stamp {
    length: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    unix: (u64, u64, u32, u32, i64, i64, i64, i64),
}

impl Stamp {
    fn of(metadata: &Metadata) -> Self {
        Self {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            unix: (
                metadata.dev(),
                metadata.ino(),
                metadata.mode(),
                metadata.uid(),
                metadata.mtime(),
                metadata.mtime_nsec(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            ),
        }
    }
}

#[derive(Debug, Clone)]
struct AdmittedFile {
    path: PathBuf,
    hash: [u8; 32],
    stamp: Stamp,
    executable: bool,
}

impl AdmittedFile {
    fn admit(pin: PinnedFile, executable: bool) -> Result<Self> {
        let file = fingerprint(&pin.path, executable)?;
        require(
            file.hash == pin.sha256,
            "Selected file differs from its caller-supplied SHA-256 pin",
        )?;
        Ok(file)
    }

    fn revalidate(&self) -> Result<()> {
        let current = fingerprint(&self.path, self.executable)?;
        require(
            current.hash == self.hash && current.stamp == self.stamp,
            "An admitted file changed; select and admit the installation again",
        )
    }
}

#[derive(Debug, Clone)]
struct AdmittedDirectory {
    path: PathBuf,
    stamp: Stamp,
}

impl AdmittedDirectory {
    fn admit(path: &Path) -> Result<Self> {
        let path = directory(path, true)?;
        Ok(Self {
            stamp: Stamp::of(&fs::metadata(&path)?),
            path,
        })
    }

    fn revalidate(&self) -> Result<()> {
        let path = directory(&self.path, true)?;
        require(
            Stamp::of(&fs::metadata(path)?) == self.stamp,
            "An admitted directory changed; admit the installation again",
        )
    }
}

/// Immutable selection. No PATH lookup, environment bridge discovery, or extra
/// caller-provided CLI arguments participate in admission or command creation.
#[derive(Debug, Clone)]
pub(super) struct AdmittedStartup {
    root: AdmittedDirectory,
    executable: AdmittedFile,
    provider: AdmittedFile,
    gnuplot: AdmittedFile,
    ghostscript: AdmittedFile,
    directories: Vec<AdmittedDirectory>,
    adapters: BTreeMap<AdapterRole, AdmittedFile>,
    runtime_path: std::ffi::OsString,
}

impl TrustedStartupConfig {
    pub(super) fn admit(self) -> Result<AdmittedStartup> {
        require(
            cfg!(unix),
            "Qualified startup currently requires Unix executable/session semantics",
        )?;
        require(
            self.octave_version == VERSION && self.octave_api_version == API,
            "Qualified startup requires the explicitly selected Octave 11.3.0/api-v61 installation",
        )?;
        require(
            !self.runtime_directories.is_empty()
                && self.runtime_directories.len() <= MAX_DIRECTORIES,
            "Runtime directory allowlist is empty or exceeds its admission budget",
        )?;
        require(
            self.adapters.len() <= 5,
            "Too many native adapter selections",
        )?;
        let root = AdmittedDirectory::admit(&self.installation_root)?;
        require(
            root.path.parent().is_some(),
            "Filesystem root cannot be the selected runtime installation",
        )?;
        let executable = AdmittedFile::admit(self.executable, true)?;
        let provider = AdmittedFile::admit(self.provider, false)?;
        let gnuplot = AdmittedFile::admit(self.gnuplot, true)?;
        let ghostscript = AdmittedFile::admit(self.ghostscript, true)?;
        // Selected Octave M code launches these auxiliaries through a shell.
        // Keep their admitted paths representable as one literal shell word.
        for (file, label) in [(&gnuplot, "gnuplot"), (&ghostscript, "Ghostscript")] {
            require(
                wire_path(&file.path, "selected export executable")?
                    .bytes()
                    .all(|byte| {
                        byte.is_ascii_alphanumeric()
                            || matches!(byte, b'/' | b'_' | b'-' | b'+' | b'.')
                    }),
                &format!(
                    "The selected Octave {label} launcher cannot safely represent this executable path"
                ),
            )?;
        }
        native_extension(&provider.path)?;
        let mut files = BTreeSet::new();
        contained(&executable.path, &root.path)?;
        for file in [&executable, &provider] {
            require(
                files.insert(file.path.clone()),
                "Duplicate selected executable/native path",
            )?;
        }
        for file in [&gnuplot, &ghostscript] {
            require(
                files.insert(file.path.clone()),
                "Duplicate selected executable/native path",
            )?;
        }
        let mut directories = Vec::new();
        let mut directory_paths = BTreeSet::new();
        let mut bytes = 0usize;
        for input in self.runtime_directories {
            let entry = AdmittedDirectory::admit(&input)?;
            contained(&entry.path, &root.path)?;
            require(
                directory_paths.insert(entry.path.clone()),
                "Duplicate runtime directory",
            )?;
            bytes += wire_path(&entry.path, "trusted runtime directory")?.len() + 1;
            require(
                bytes <= MAX_PATH_BYTES,
                "Runtime directory allowlist exceeds its byte budget",
            )?;
            directories.push(entry);
        }
        let runtime_path = std::env::join_paths(directories.iter().map(|entry| &entry.path))
            .map_err(|_| {
                Error::Protocol(
                    "Runtime directory cannot be represented in the explicit function path".into(),
                )
            })?;
        let mut adapters = BTreeMap::new();
        for adapter in self.adapters {
            require(
                !adapters.contains_key(&adapter.role),
                "Duplicate native adapter role",
            )?;
            let file = AdmittedFile::admit(adapter.file, false)?;
            native_extension(&file.path)?;
            require(
                files.insert(file.path.clone()),
                "Duplicate selected executable/native path",
            )?;
            adapters.insert(adapter.role, file);
        }
        Ok(AdmittedStartup {
            root,
            executable,
            provider,
            gnuplot,
            ghostscript,
            directories,
            adapters,
            runtime_path,
        })
    }
}

impl AdmittedStartup {
    pub(super) fn executable(&self) -> &Path {
        &self.executable.path
    }

    pub(super) fn has_adapter(&self, role: AdapterRole) -> bool {
        self.adapters.contains_key(&role)
    }

    pub(super) fn required_primitives(&self) -> &'static [&'static str] {
        if self.has_adapter(AdapterRole::Graphics) {
            super::trusted_worker::GRAPHICS_REQUIRED_PRIMITIVES
        } else {
            super::trusted_worker::REQUIRED_PRIMITIVES
        }
    }

    pub(super) fn prepare(&self, session_path: &Path) -> Result<PreparedStartup> {
        self.revalidate()?;
        let session = directory(session_path, false)?;
        private_session(&session)?;
        require(
            !self.directories.iter().any(|entry| entry.path == session),
            "Private session must not duplicate an installation function directory",
        )?;
        let worker = fingerprint(&session.join("mathlab_worker.m"), false)?;
        let registry = fingerprint(&session.join("registry/mathlab_scope_register.m"), false)?;
        let helper = fingerprint(&session.join("console-fence"), true)?;
        let nonce = tempfile::Builder::new()
            .prefix("__mlentry_")
            .rand_bytes(32)
            .tempfile_in(&session)?;
        let entry_alias = wire_path(
            Path::new(
                nonce
                    .path()
                    .file_name()
                    .ok_or_else(|| Error::Protocol("Missing generated entry identity".into()))?,
            ),
            "generated entry alias",
        )?;
        require(
            entry_alias.len() <= 60
                && entry_alias.starts_with("__mlentry_")
                && entry_alias
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'),
            "Generated entry alias is not a native identifier",
        )?;
        let shutdown_alias = format!("__mlshutdown_{}", &entry_alias[10..]);
        let mut adapters = BTreeMap::new();
        for (role, file) in &self.adapters {
            let (role, protocol, version) = role.identity();
            adapters.insert(
                role.into(),
                AdapterReady {
                    path: wire_path(&file.path, "admitted adapter")?,
                    protocol: protocol.into(),
                    version: version.into(),
                },
            );
        }
        let ready = QualifiedReadyProfile {
            expected: IntrinsicsReady {
                protocol: "mathlab-intrinsics".into(),
                version: "1".into(),
                phase: "ready".into(),
                provider_file: wire_path(&self.provider.path, "admitted provider")?,
                worker_file: wire_path(&worker.path, "owned worker")?,
                registry_file: wire_path(&registry.path, "owned registry")?,
                entry_alias: entry_alias.clone(),
                octave_version: VERSION.into(),
                octave_api_version: API.into(),
                initial_path_admitted: true,
                shutdown_alias,
                shutdown_registered: true,
                adapters,
            },
        };
        let temporary_alias = TemporaryAlias::for_session(&session)?;
        Ok(PreparedStartup {
            admitted: self.clone(),
            session,
            temporary_alias,
            worker,
            registry,
            helper,
            entry_alias,
            ready,
            _nonce: nonce,
        })
    }

    fn revalidate(&self) -> Result<()> {
        self.root.revalidate()?;
        for directory in &self.directories {
            directory.revalidate()?;
        }
        for file in [
            &self.executable,
            &self.provider,
            &self.gnuplot,
            &self.ghostscript,
        ] {
            file.revalidate()?;
        }
        for file in self.adapters.values() {
            file.revalidate()?;
        }
        Ok(())
    }
}

/// Existing ten worker arguments, with enabled adapter paths kept distinct from
/// prebound adapters. Session is derived from the prepared profile, never input.
pub(super) struct WorkerArguments<'a> {
    pub(super) announce_ready: bool,
    pub(super) debugger_enabled: bool,
    pub(super) scope_bridge: Option<&'a Path>,
    pub(super) response_prefix: &'a str,
    pub(super) generation: u64,
    pub(super) console_helper: &'a Path,
    pub(super) invocation: u64,
    pub(super) input_bridge: Option<&'a Path>,
    pub(super) value_bridge: Option<&'a Path>,
}

#[derive(Debug)]
pub(super) struct PreparedStartup {
    admitted: AdmittedStartup,
    session: PathBuf,
    temporary_alias: Option<TemporaryAlias>,
    worker: AdmittedFile,
    registry: AdmittedFile,
    helper: AdmittedFile,
    entry_alias: String,
    ready: QualifiedReadyProfile,
    _nonce: tempfile::NamedTempFile,
}

/// Octave's print helpers interpolate temporary EPS paths into shell commands.
/// Keep the actual files in the caller-selected session, but give those helpers
/// a literal shell word when its canonical pathname contains spaces or syntax.
#[derive(Debug)]
struct TemporaryAlias {
    directory: tempfile::TempDir,
    stamp: Stamp,
}

impl TemporaryAlias {
    fn for_session(session: &Path) -> Result<Option<Self>> {
        if literal_shell_path(session)? {
            return Ok(None);
        }
        #[cfg(not(unix))]
        return Err(Error::Protocol(
            "Temporary export aliases require Unix session semantics".into(),
        ));
        #[cfg(unix)]
        {
            // Only a private directory and symlink live here. EPS/PS files still
            // consume space on the caller-selected session filesystem. Resolve
            // macOS's /tmp alias once; do not inherit a second ambient temp root.
            let root = directory(Path::new("/tmp"), false)?;
            require(
                literal_shell_path(&root)?,
                "The system temporary alias root must be a literal shell path",
            )?;
            let metadata = fs::metadata(&root)?;
            require(
                (metadata.uid() == 0 || metadata.uid() == unsafe { libc::geteuid() })
                    && (metadata.mode() & 0o022 == 0 || metadata.mode() & 0o1000 != 0),
                "The system temporary alias root must have a trusted owner and protected entries",
            )?;
            let directory = tempfile::Builder::new()
                .prefix("mathlab-export-")
                .permissions(fs::Permissions::from_mode(0o700))
                .tempdir_in(root)?;
            private_session(directory.path())?;
            std::os::unix::fs::symlink(session, directory.path().join("session"))?;
            let alias = Self {
                stamp: Stamp::of(&fs::metadata(directory.path())?),
                directory,
            };
            alias.revalidate(session)?;
            Ok(Some(alias))
        }
    }

    fn path(&self) -> PathBuf {
        self.directory.path().join("session")
    }

    fn revalidate(&self, session: &Path) -> Result<()> {
        private_session(self.directory.path())?;
        require(
            Stamp::of(&fs::metadata(self.directory.path())?) == self.stamp,
            "The owned temporary alias directory changed",
        )?;
        let path = self.path();
        require(
            literal_shell_path(&path)?
                && fs::symlink_metadata(&path)?.file_type().is_symlink()
                && fs::read_link(&path)? == session
                && fs::canonicalize(&path)? == session,
            "The owned temporary alias no longer names this private session",
        )
    }
}

fn literal_shell_path(path: &Path) -> Result<bool> {
    Ok(wire_path(path, "temporary export path")?
        .bytes()
        .all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'+' | b'.')
        }))
}

impl PreparedStartup {
    pub(super) fn ready_profile(&self) -> QualifiedReadyProfile {
        self.ready.clone()
    }

    /// Compare caller selection with the immutable admission behind this Engine.
    /// Admission already requires exact canonical spelling; this comparison does
    /// not rehash files or turn an old Ready into a current filesystem attestation.
    pub(super) fn matches_selection(&self, selected: &TrustedStartupConfig) -> bool {
        let admitted = &self.admitted;
        let matches_file = |file: &AdmittedFile, pin: &PinnedFile| {
            file.path == pin.path && file.hash == pin.sha256
        };
        selected.octave_version == VERSION
            && selected.octave_api_version == API
            && selected.installation_root == admitted.root.path
            && matches_file(&admitted.executable, &selected.executable)
            && matches_file(&admitted.provider, &selected.provider)
            && matches_file(&admitted.gnuplot, &selected.gnuplot)
            && matches_file(&admitted.ghostscript, &selected.ghostscript)
            && selected.runtime_directories.len() == admitted.directories.len()
            && selected
                .runtime_directories
                .iter()
                .zip(&admitted.directories)
                .all(|(path, directory)| path == &directory.path)
            && selected.adapters.len() == admitted.adapters.len()
            && admitted.adapters.iter().all(|(role, file)| {
                selected
                    .adapters
                    .iter()
                    .find(|adapter| adapter.role == *role)
                    .is_some_and(|adapter| matches_file(file, &adapter.file))
            })
    }

    pub(super) fn startup_expression(
        &self,
        args: WorkerArguments<'_>,
        initial_cwd: &Path,
    ) -> Result<String> {
        require(
            args.announce_ready,
            "Initial invocation must announce Ready",
        )?;
        let initial_cwd = directory(initial_cwd, false)?;
        let arguments = self.arguments(args, &quoted_path(&initial_cwd)?)?;
        let required = self
            .admitted
            .required_primitives()
            .iter()
            .map(|name| quoted(name))
            .collect::<Vec<_>>()
            .join(",");
        let directories = self
            .admitted
            .directories
            .iter()
            .map(|entry| quoted_path(&entry.path))
            .collect::<Result<Vec<_>>>()?
            .join(",");
        let adapters = self
            .admitted
            .adapters
            .iter()
            .map(|(role, file)| {
                Ok(format!(
                    "builtin('struct','role',{},'path',{})",
                    quoted(role.identity().0),
                    quoted_path(&file.path)?
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join(",");
        let provider = quoted_path(&self.admitted.provider.path)?;
        // Octave's PKG_ADD probes gnuplot before this pinned selection is set.
        // Its early PATH probe can omit the toolkit under our constrained PATH.
        // Register the selected runtime's toolkit before the worker's existing
        // cold warmup; never reapply a default during same-PID recovery.
        Ok(format!(
            "builtin('autoload','mathlab_intrinsic_bridge',{provider}); mathlab_intrinsic_bridge('intrinsic_bootstrap_v2',{provider},{},{},{},{{{required}}},{{{directories}}},{{{adapters}}}); gnuplot_binary({}); builtin('register_graphics_toolkit','gnuplot'); {}('intrinsic_run',{{{arguments}}});",
            quoted_path(&self.worker.path)?,
            quoted_path(&self.registry.path)?,
            quoted(&self.entry_alias),
            quoted_path(&self.admitted.gnuplot.path)?,
            self.entry_alias,
        ))
    }

    pub(super) fn recovery_expression(&self, args: WorkerArguments<'_>) -> Result<String> {
        require(
            !args.announce_ready,
            "Recovery must preserve the existing worker startup argument",
        )?;
        Ok(format!(
            "{}('intrinsic_run',{{{}}});\n",
            self.entry_alias,
            self.arguments(args, "''")?
        ))
    }

    /// Only pass an expression returned by this prepared profile. This private
    /// integration boundary does not authorize caller-defined code or CLI flags.
    /// Spawn immediately after return; a standard Command cannot freeze paths.
    pub(super) fn command(&self, expression: &str) -> Result<Command> {
        self.admitted.revalidate()?;
        private_session(&self.session)?;
        let temporary_directory = if let Some(alias) = &self.temporary_alias {
            alias.revalidate(&self.session)?;
            alias.path()
        } else {
            self.session.clone()
        };
        for file in [&self.worker, &self.registry, &self.helper] {
            file.revalidate()?;
        }
        // User programs and Octave M files can invoke system utilities (for
        // example, movefile uses mv). Startup and native/export admission use
        // absolute paths, independently of this fixed utility PATH.
        let utility_directories = std::iter::once(
            self.admitted
                .executable
                .path
                .parent()
                .ok_or_else(|| Error::Protocol("Missing selected executable parent".into()))?,
        );
        #[cfg(unix)]
        let utility_directories =
            utility_directories.chain([Path::new("/usr/bin"), Path::new("/bin")]);
        let utility_path = std::env::join_paths(utility_directories).map_err(|_| {
            Error::Protocol(
                "Selected executable directory cannot be represented in the user-program PATH"
                    .into(),
            )
        })?;
        let mut command = Command::new(&self.admitted.executable.path);
        command
            .args([
                "--quiet",
                "--no-gui",
                "--no-line-editing",
                "--no-history",
                "--no-init-file",
                "--no-site-file",
                "--no-init-path",
                "--path",
            ])
            .arg(&self.admitted.runtime_path)
            .args(["--persist", "--eval", expression])
            .current_dir(&self.session)
            .env_clear()
            .env("PATH", utility_path)
            .env("GSC", &self.admitted.ghostscript.path)
            .env("HOME", &self.session)
            .env("TMPDIR", temporary_directory)
            .env("XDG_CONFIG_HOME", self.session.join("config"))
            .env("XDG_CACHE_HOME", self.session.join("cache"))
            .env("QT_QPA_PLATFORM", "offscreen")
            .env("LC_ALL", "C");
        Ok(command)
    }

    /// Check activation against the prebound role before publishing Engine state.
    pub(super) fn validate_adapter_path(&self, role: AdapterRole, path: &Path) -> Result<()> {
        self.enabled(role, Some(path)).map(|_| ())
    }

    fn arguments(&self, args: WorkerArguments<'_>, initial_directory: &str) -> Result<String> {
        require(
            args.generation > 0
                && args.invocation > 0
                && args.generation <= 9_007_199_254_740_991
                && args.invocation <= 9_007_199_254_740_991,
            "Worker counters must be positive exactly representable integers",
        )?;
        bounded(args.response_prefix, 128)?;
        require(
            args.response_prefix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')),
            "Invalid framing nonce",
        )?;
        require(
            !args.debugger_enabled || self.admitted.adapters.contains_key(&AdapterRole::Debug),
            "Debugger activation was not admitted",
        )?;
        let helper = checked_path(args.console_helper, false)?;
        require(
            helper == self.helper.path,
            "Worker selected a different console helper",
        )?;
        Ok(format!(
            "{},{},{},{},{},{},{},{},{},{},{}",
            quoted_path(&self.session)?,
            boolean(args.announce_ready),
            boolean(args.debugger_enabled),
            self.enabled(AdapterRole::Scope, args.scope_bridge)?,
            quoted(args.response_prefix),
            args.generation,
            quoted_path(&helper)?,
            args.invocation,
            self.enabled(AdapterRole::Input, args.input_bridge)?,
            self.enabled(AdapterRole::Value, args.value_bridge)?,
            initial_directory,
        ))
    }

    fn enabled(&self, role: AdapterRole, path: Option<&Path>) -> Result<String> {
        let Some(path) = path else {
            return Ok("''".into());
        };
        let selected = self
            .admitted
            .adapters
            .get(&role)
            .ok_or_else(|| Error::Protocol("Enabled adapter was not admitted".into()))?;
        let actual = checked_path(path, true)?;
        require(
            actual == selected.path,
            "Enabled adapter differs from the admitted role identity",
        )?;
        quoted_path(&actual)
    }
}

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error::Protocol(message.into()))
    }
}

fn bounded(text: &str, limit: usize) -> Result<()> {
    require(
        !text.is_empty()
            && text.len() <= limit
            && !text.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n')),
        "Invalid bounded exact UTF-8 startup identity",
    )
}

fn checked_path(path: &Path, canonical_spelling: bool) -> Result<PathBuf> {
    let raw = wire_path(path, "selected startup path")?;
    bounded(&raw, 4096)?;
    require(path.is_absolute(), "Selected startup path must be absolute")?;
    let resolved = canonical_wire_path(path, "resolved startup path")?;
    let resolved_text = wire_path(&resolved, "resolved startup path")?;
    bounded(&resolved_text, 4096)?;
    require(
        !canonical_spelling || raw == resolved_text,
        "Selected startup path must already have its canonical spelling",
    )?;
    Ok(resolved)
}

fn directory(path: &Path, canonical_spelling: bool) -> Result<PathBuf> {
    let resolved = checked_path(path, canonical_spelling)?;
    require(
        fs::metadata(&resolved)?.is_dir(),
        "Selected startup directory is not a directory",
    )?;
    Ok(resolved)
}

fn contained(path: &Path, root: &Path) -> Result<()> {
    require(
        path.starts_with(root),
        "Selected executable/runtime path is outside the selected installation root",
    )
}

fn native_extension(path: &Path) -> Result<()> {
    require(
        path.extension().is_some_and(|extension| extension == "oct"),
        "Selected native module must have the .oct extension",
    )
}

fn private_session(path: &Path) -> Result<()> {
    directory(path, true)?;
    let metadata = fs::metadata(path)?;
    #[cfg(unix)]
    require(
        metadata.uid() == unsafe { libc::geteuid() } && metadata.permissions().mode() & 0o077 == 0,
        "Startup session directory must be owned by this user with owner-only access",
    )?;
    #[cfg(not(unix))]
    {
        let _ = metadata;
        return Err(Error::Protocol(
            "Private startup session requires Unix permissions".into(),
        ));
    }
    Ok(())
}

fn fingerprint(path: &Path, executable: bool) -> Result<AdmittedFile> {
    let path = checked_path(path, true)?;
    let metadata = fs::symlink_metadata(&path)?;
    require(
        metadata.is_file(),
        "Selected startup file must be a regular file",
    )?;
    #[cfg(unix)]
    require(
        !executable || metadata.permissions().mode() & 0o111 != 0,
        "Selected executable has no executable permission bits",
    )?;
    let before = Stamp::of(&metadata);
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let mut file: File = options.open(&path)?;
    require(
        file.metadata()?.is_file() && Stamp::of(&file.metadata()?) == before,
        "Selected startup file changed while opening",
    )?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 16_384];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    require(
        Stamp::of(&file.metadata()?) == before
            && Stamp::of(&fs::symlink_metadata(&path)?) == before
            && checked_path(&path, true)? == path,
        "Selected startup file changed while hashing",
    )?;
    Ok(AdmittedFile {
        path,
        hash: hash.finalize().into(),
        stamp: before,
        executable,
    })
}

fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}
fn quoted_path(path: &Path) -> Result<String> {
    Ok(quoted(&wire_path(path, "serialized startup path")?))
}
fn boolean(value: bool) -> &'static str {
    if value { "(1 == 1)" } else { "(1 == 0)" }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    struct Fixture {
        _directory: tempfile::TempDir,
        config: TrustedStartupConfig,
        session: PathBuf,
    }

    fn pinned(path: PathBuf, bytes: &[u8], executable: bool) -> PinnedFile {
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(
            &path,
            fs::Permissions::from_mode(if executable { 0o700 } else { 0o600 }),
        )
        .unwrap();
        PinnedFile {
            path,
            sha256: Sha256::digest(bytes).into(),
        }
    }

    fn fixture() -> Fixture {
        let directory = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(directory.path()).unwrap();
        let root = base.join("installation");
        let runtime = root.join("m");
        let session = base.join("session");
        for path in [&root, &runtime, &session, &session.join("registry")] {
            fs::create_dir(path).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let executable = pinned(root.join("octave"), b"selected executable", true);
        // Native files and auxiliary tools deliberately live outside runtime root.
        let provider = pinned(base.join("provider.oct"), b"selected provider", false);
        let gnuplot = pinned(base.join("gnuplot"), b"selected plotting tool", true);
        let ghostscript = pinned(base.join("gs"), b"selected Ghostscript", true);
        pinned(session.join("mathlab_worker.m"), b"owned worker", false);
        pinned(
            session.join("registry/mathlab_scope_register.m"),
            b"owned registry",
            false,
        );
        pinned(session.join("console-fence"), b"owned console helper", true);
        let config = TrustedStartupConfig {
            installation_root: root,
            executable,
            provider,
            gnuplot,
            ghostscript,
            runtime_directories: vec![runtime],
            adapters: vec![],
            octave_version: VERSION.into(),
            octave_api_version: API.into(),
        };
        Fixture {
            _directory: directory,
            config,
            session,
        }
    }

    fn arguments<'a>(helper: &'a Path, announce_ready: bool) -> WorkerArguments<'a> {
        WorkerArguments {
            announce_ready,
            debugger_enabled: false,
            scope_bridge: None,
            response_prefix: "nonce-123",
            generation: 1,
            console_helper: helper,
            invocation: 1,
            input_bridge: None,
            value_bridge: None,
        }
    }

    #[test]
    fn prepared_selection_compares_every_immutable_pin_runtime_directory_and_role() {
        let mut fixture = fixture();
        let second_directory = fixture.config.installation_root.join("m2");
        fs::create_dir(&second_directory).unwrap();
        fixture.config.runtime_directories.push(second_directory);
        for role in [
            AdapterRole::Input,
            AdapterRole::Scope,
            AdapterRole::Value,
            AdapterRole::Debug,
            AdapterRole::Graphics,
        ] {
            let file = pinned(
                fixture
                    .session
                    .parent()
                    .unwrap()
                    .join(format!("{}.oct", role.identity().0)),
                b"selected adapter",
                false,
            );
            fixture.config.adapters.push(AdapterConfig { role, file });
        }
        let selected = fixture.config.clone();
        let prepared = fixture
            .config
            .admit()
            .unwrap()
            .prepare(&fixture.session)
            .unwrap();
        assert!(prepared.matches_selection(&selected));
        let mut reordered = selected.clone();
        reordered.adapters.reverse();
        assert!(
            prepared.matches_selection(&reordered),
            "admission keys adapter roles independently of document order"
        );
        let changes: [fn(&mut TrustedStartupConfig); 16] = [
            |config| config.installation_root = PathBuf::from("/different-installation"),
            |config| config.executable.path = PathBuf::from("/different-executable"),
            |config| config.executable.sha256[0] ^= 1,
            |config| config.provider.path = PathBuf::from("/different-provider.oct"),
            |config| config.provider.sha256[0] ^= 1,
            |config| config.gnuplot.path = PathBuf::from("/different-gnuplot"),
            |config| config.gnuplot.sha256[0] ^= 1,
            |config| config.ghostscript.path = PathBuf::from("/different-ghostscript"),
            |config| config.ghostscript.sha256[0] ^= 1,
            |config| config.runtime_directories.reverse(),
            |config| config.runtime_directories[0] = PathBuf::from("/different-runtime-directory"),
            |config| {
                config.runtime_directories.pop();
            },
            |config| config.octave_version = "11.2.0".into(),
            |config| config.octave_api_version = "api-v60".into(),
            |config| {
                config.adapters.pop();
            },
            |config| config.adapters[0].role = AdapterRole::Graphics,
        ];
        for (index, change) in changes.into_iter().enumerate() {
            let mut other = selected.clone();
            change(&mut other);
            assert!(
                !prepared.matches_selection(&other),
                "selection mutation {index}"
            );
        }
        for index in 0..selected.adapters.len() {
            let mut other = selected.clone();
            other.adapters[index].file.sha256[0] ^= 1;
            assert!(!prepared.matches_selection(&other), "adapter pin {index}");
            let mut other = selected.clone();
            other.adapters[index].file.path = PathBuf::from("/different-adapter.oct");
            assert!(!prepared.matches_selection(&other), "adapter path {index}");
        }
    }

    #[test]
    fn admission_rejects_wrong_pins_versions_outside_paths_and_duplicates() {
        let fixture = fixture();
        let mut bad = fixture.config.clone();
        bad.provider.sha256[0] ^= 1;
        assert!(bad.admit().is_err());
        let mut bad = fixture.config.clone();
        bad.octave_version = "11.2.0".into();
        assert!(bad.admit().is_err());
        let mut bad = fixture.config.clone();
        bad.runtime_directories
            .push(bad.runtime_directories[0].clone());
        assert!(bad.admit().is_err());
        let mut bad = fixture.config.clone();
        bad.runtime_directories = vec![fixture.session.clone()];
        assert!(bad.admit().is_err());
        let mut bad = fixture.config.clone();
        bad.installation_root = PathBuf::from("/");
        assert!(bad.admit().is_err());
        let mut bad = fixture.config.clone();
        bad.adapters.push(AdapterConfig {
            role: AdapterRole::Graphics,
            file: bad.provider.clone(),
        });
        // Graphics is selectable, but cannot reuse another admitted native file.
        assert!(bad.admit().is_err());
        assert!(fixture.config.admit().is_ok());
    }

    #[test]
    fn ghostscript_admission_requires_a_unique_pinned_safe_absolute_executable() {
        let fixture = fixture();
        let selected = &fixture.config.ghostscript;
        let mut bad = fixture.config.clone();
        bad.ghostscript.sha256[0] ^= 1;
        assert!(bad.admit().is_err());
        let mut bad = fixture.config.clone();
        bad.ghostscript.path = selected.path.with_file_name("missing-gs");
        assert!(bad.admit().is_err());
        let mut bad = fixture.config.clone();
        bad.ghostscript.path = PathBuf::from("gs");
        assert!(bad.admit().is_err());
        let mut bad = fixture.config.clone();
        bad.ghostscript.path = selected.path.parent().unwrap().join("session/../gs");
        assert!(bad.admit().is_err());
        let mut bad = fixture.config.clone();
        bad.ghostscript = bad.gnuplot.clone();
        assert!(bad.admit().is_err());
        let alias = selected.path.with_file_name("gs-alias");
        std::os::unix::fs::symlink(&selected.path, &alias).unwrap();
        let mut bad = fixture.config.clone();
        bad.ghostscript.path = alias;
        assert!(bad.admit().is_err());
        for name in ["gs with space", "gs;command", "gs$(command)", "gs'quote"] {
            let mut bad = fixture.config.clone();
            bad.ghostscript = pinned(selected.path.with_file_name(name), b"unsafe gs", true);
            assert!(bad.admit().is_err(), "{name}");
        }
        fs::set_permissions(&selected.path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(fixture.config.admit().is_err());
    }

    #[test]
    fn ghostscript_replacement_or_permission_change_fails_before_spawn() {
        for replace in [false, true] {
            let fixture = fixture();
            let selected = fixture.config.ghostscript.path.clone();
            let prepared = fixture
                .config
                .admit()
                .unwrap()
                .prepare(&fixture.session)
                .unwrap();
            assert!(prepared.command("unused").is_ok());
            if replace {
                fs::write(&selected, b"replaced Ghostscript").unwrap();
            } else {
                fs::set_permissions(&selected, fs::Permissions::from_mode(0o600)).unwrap();
            }
            assert!(prepared.command("unused").is_err());
        }
    }

    #[test]
    fn graphics_selection_controls_bootstrap_primitives_and_ready_identity() {
        use super::super::SessionContext;
        use super::super::qualified_ready::{
            QUALIFIED_GRAPHICS_STARTUP_PROFILE, QUALIFIED_STARTUP_PROFILE,
        };

        for graphics in [false, true] {
            let mut fixture = fixture();
            let graphics_pin = pinned(
                fixture.session.parent().unwrap().join("graphics.oct"),
                b"selected graphics",
                false,
            );
            if graphics {
                fixture.config.adapters.push(AdapterConfig {
                    role: AdapterRole::Graphics,
                    file: graphics_pin.clone(),
                });
            }
            let admitted = fixture.config.admit().unwrap();
            assert_eq!(admitted.has_adapter(AdapterRole::Graphics), graphics);
            let required = admitted.required_primitives();
            assert_eq!(required.len(), if graphics { 120 } else { 115 });
            for name in ["all", "fprintf", "fread", "functions", "rethrow"] {
                assert_eq!(required.contains(&name), graphics, "{name}");
            }
            let prepared = admitted.prepare(&fixture.session).unwrap();
            let helper = fixture.session.join("console-fence");
            let startup = prepared
                .startup_expression(arguments(&helper, true), &fixture.session)
                .unwrap();
            let serialized_primitives = required
                .iter()
                .map(|name| quoted(name))
                .collect::<Vec<_>>()
                .join(",");
            assert!(startup.contains(&format!(",{{{serialized_primitives}}},")));
            assert_eq!(startup.contains("'role','graphics','path',"), graphics);
            let ready = prepared.ready_profile();
            assert_eq!(ready.expected.adapters.contains_key("graphics"), graphics);
            let context = SessionContext {
                worker_id: "selected-profile".into(),
                generation: 1,
                invocation: 1,
                revision: 0,
            };
            let snapshot = ready.accepted_status(context.clone());
            assert_eq!(snapshot.context, context);
            assert_eq!(
                snapshot.profile,
                if graphics {
                    QUALIFIED_GRAPHICS_STARTUP_PROFILE
                } else {
                    QUALIFIED_STARTUP_PROFILE
                }
            );
            assert_eq!(snapshot.intrinsics.version, "1");
            if graphics {
                let selected = &snapshot.intrinsics.adapters["graphics"];
                assert_eq!(
                    selected.path,
                    wire_path(&graphics_pin.path, "test").unwrap()
                );
                assert_eq!(selected.protocol, "mathlab-graphics");
                assert_eq!(selected.version, "1");
                assert!(
                    prepared
                        .validate_adapter_path(AdapterRole::Graphics, &graphics_pin.path)
                        .is_ok()
                );
            } else {
                assert!(
                    prepared
                        .validate_adapter_path(AdapterRole::Graphics, &graphics_pin.path)
                        .is_err()
                );
            }
            let payload = serde_json::json!({"intrinsics": snapshot.intrinsics});
            ready.validate(&payload, VERSION).unwrap();
            let mut mismatched = payload;
            if graphics {
                mismatched["intrinsics"]["adapters"]
                    .as_object_mut()
                    .unwrap()
                    .remove("graphics");
            } else {
                mismatched["intrinsics"]["adapters"]["graphics"] = serde_json::json!({
                    "path": graphics_pin.path, "protocol": "mathlab-graphics", "version": "1"
                });
            }
            assert!(ready.validate(&mismatched, VERSION).is_err());
        }
    }

    #[test]
    fn selected_graphics_retains_pin_role_extension_and_revalidation_checks() {
        let mut fixture = fixture();
        let graphics_pin = pinned(
            fixture.session.parent().unwrap().join("graphics.oct"),
            b"selected graphics",
            false,
        );
        fixture.config.adapters.push(AdapterConfig {
            role: AdapterRole::Graphics,
            file: graphics_pin.clone(),
        });
        let mut bad = fixture.config.clone();
        bad.adapters[0].file.sha256[0] ^= 1;
        assert!(bad.admit().is_err());
        let mut bad = fixture.config.clone();
        bad.adapters.push(bad.adapters[0].clone());
        assert!(bad.admit().is_err());
        let mut bad = fixture.config.clone();
        bad.adapters[0].file = pinned(
            fixture.session.parent().unwrap().join("not-native.m"),
            b"selected graphics",
            false,
        );
        assert!(bad.admit().is_err());
        let prepared = fixture
            .config
            .admit()
            .unwrap()
            .prepare(&fixture.session)
            .unwrap();
        assert!(
            prepared
                .validate_adapter_path(AdapterRole::Input, &graphics_pin.path)
                .is_err()
        );
        let other = pinned(
            fixture.session.parent().unwrap().join("other-graphics.oct"),
            b"another graphics",
            false,
        );
        assert!(
            prepared
                .validate_adapter_path(AdapterRole::Graphics, &other.path)
                .is_err()
        );
        let helper = fixture.session.join("console-fence");
        let startup = prepared
            .startup_expression(arguments(&helper, true), &fixture.session)
            .unwrap();
        fs::write(&graphics_pin.path, b"replaced graphics").unwrap();
        assert!(prepared.command(&startup).is_err());
    }

    #[test]
    fn raw_and_resolved_path_spelling_are_checked_without_lossy_conversion() {
        use std::os::unix::ffi::OsStringExt;
        let fixture = fixture();
        let mut bad = fixture.config.clone();
        bad.provider.path =
            PathBuf::from(std::ffi::OsString::from_vec(b"/invalid-\xff.oct".to_vec()));
        assert!(bad.admit().is_err());
        // macOS rejects this filename during fixture creation.
        #[cfg(target_os = "linux")]
        {
            let invalid_target = fixture
                .session
                .join(std::ffi::OsString::from_vec(b"target-\xff".to_vec()));
            fs::write(&invalid_target, b"target").unwrap();
            let alias = fixture.session.join("valid-alias");
            std::os::unix::fs::symlink(&invalid_target, &alias).unwrap();
            assert!(checked_path(&alias, false).is_err());
        }
        let alias = fixture.session.join("provider-alias.oct");
        std::os::unix::fs::symlink(&fixture.config.provider.path, &alias).unwrap();
        let mut bad = fixture.config.clone();
        bad.provider.path = alias;
        assert!(bad.admit().is_err());
        assert_eq!(quoted("a'b"), "'a''b'");
    }

    #[test]
    fn fixed_command_and_expressions_preserve_the_v2_argument_contract() {
        let fixture = fixture();
        let admitted = fixture.config.admit().unwrap();
        let prepared = admitted.prepare(&fixture.session).unwrap();
        let helper = fixture.session.join("console-fence");
        let startup = prepared
            .startup_expression(arguments(&helper, true), &fixture.session)
            .unwrap();
        assert!(startup.starts_with("builtin('autoload','mathlab_intrinsic_bridge',"));
        assert!(startup.contains("'intrinsic_bootstrap_v2'"));
        let selected_toolkit = format!(
            "gnuplot_binary({}); builtin('register_graphics_toolkit','gnuplot'); {}('intrinsic_run'",
            quoted_path(&admitted.gnuplot.path).unwrap(),
            prepared.entry_alias
        );
        assert!(startup.contains(&selected_toolkit));
        assert!(!startup.contains("addpath ("));
        let expected_arguments = format!(
            "{},(1 == 1),(1 == 0),'','nonce-123',1,{},1,'','',{}",
            quoted_path(&fixture.session).unwrap(),
            quoted_path(&helper).unwrap(),
            quoted_path(&fixture.session).unwrap()
        );
        assert!(startup.ends_with(&format!(
            "{}('intrinsic_run',{{{expected_arguments}}});",
            prepared.entry_alias
        )));
        let recovery = prepared
            .recovery_expression(arguments(&helper, false))
            .unwrap();
        assert!(recovery.starts_with(&prepared.entry_alias));
        assert!(
            !recovery.contains("autoload")
                && !recovery.contains("bootstrap")
                && !recovery.contains("mathlab_worker")
        );
        assert!(!recovery.contains("true") && !recovery.contains("false"));
        assert!(!recovery.contains("register_graphics_toolkit"));
        assert!(recovery.ends_with(",1,'','',''});\n"));
        let command = prepared.command(&startup).unwrap();
        let flags = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect::<Vec<_>>();
        for flag in [
            "--no-init-file",
            "--no-site-file",
            "--no-history",
            "--no-init-path",
            "--path",
        ] {
            assert!(flags.contains(&flag));
        }
        assert_eq!(command.get_current_dir(), Some(fixture.session.as_path()));
        let temporary_directory = command
            .get_envs()
            .find(|(key, _)| *key == std::ffi::OsStr::new("TMPDIR"))
            .and_then(|(_, value)| value)
            .map(Path::new)
            .unwrap();
        assert!(literal_shell_path(temporary_directory).unwrap());
        assert_eq!(
            fs::canonicalize(temporary_directory).unwrap(),
            fixture.session
        );
        assert_eq!(
            command
                .get_envs()
                .find(|(key, _)| *key == std::ffi::OsStr::new("GSC"))
                .and_then(|(_, value)| value),
            Some(admitted.ghostscript.path.as_os_str())
        );
        let path_environment = command
            .get_envs()
            .find(|(key, _)| *key == std::ffi::OsStr::new("PATH"))
            .and_then(|(_, value)| value);
        assert_eq!(
            std::env::split_paths(path_environment.unwrap()).collect::<Vec<_>>(),
            vec![
                admitted.executable.path.parent().unwrap().to_path_buf(),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
            ]
        );
        assert!(
            !command
                .get_envs()
                .any(|(key, _)| key == std::ffi::OsStr::new("OCTAVE_PATH"))
        );
        assert!(
            !command
                .get_envs()
                .any(|(key, _)| key == std::ffi::OsStr::new("OCTAVE_HOME"))
        );
        assert_eq!(super::super::trusted_worker::REQUIRED_PRIMITIVES.len(), 115);
    }

    #[test]
    fn user_program_path_contains_only_selected_bin_and_fixed_system_directories() {
        let fixture = fixture();
        let admitted = fixture.config.admit().unwrap();
        let prepared = admitted.prepare(&fixture.session).unwrap();
        let helper = fixture.session.join("console-fence");
        let startup = prepared
            .startup_expression(arguments(&helper, true), &fixture.session)
            .unwrap();
        let command = prepared.command(&startup).unwrap();
        let path = command
            .get_envs()
            .find(|(key, _)| *key == std::ffi::OsStr::new("PATH"))
            .and_then(|(_, value)| value)
            .unwrap();
        let directories = std::env::split_paths(path).collect::<Vec<_>>();
        let selected_bin = admitted.executable.path.parent().unwrap();
        assert_eq!(
            directories,
            vec![
                selected_bin.to_path_buf(),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
            ]
        );
        // Exact equality also rejects appended ambient PATH entries without
        // changing the global process environment in a parallel test runner.
        for excluded in [
            fixture.session.as_path(),
            fixture.session.parent().unwrap(),
            admitted.provider.path.parent().unwrap(),
            admitted.gnuplot.path.parent().unwrap(),
            admitted.ghostscript.path.parent().unwrap(),
            admitted.directories[0].path.as_path(),
        ] {
            assert!(!directories.iter().any(|directory| directory == excluded));
        }
        assert_eq!(command.get_program(), admitted.executable.path.as_os_str());
    }

    #[test]
    fn user_program_path_rejects_selected_bin_with_path_separator() {
        let mut fixture = fixture();
        let selected_bin = fixture.config.installation_root.join("bin:separator");
        fs::create_dir(&selected_bin).unwrap();
        let executable = selected_bin.join("octave");
        fs::rename(&fixture.config.executable.path, &executable).unwrap();
        fixture.config.executable.path = executable;
        let admitted = fixture.config.admit().unwrap();
        let prepared = admitted.prepare(&fixture.session).unwrap();
        let error = prepared.command("unused").unwrap_err();
        assert!(matches!(error, Error::Protocol(message) if message.contains("user-program PATH")));
    }

    #[test]
    fn prebinding_does_not_enable_an_adapter_and_file_changes_fail_revalidation() {
        let mut fixture = fixture();
        let pin = pinned(
            fixture.session.parent().unwrap().join("input.oct"),
            b"selected input",
            false,
        );
        fixture.config.adapters.push(AdapterConfig {
            role: AdapterRole::Input,
            file: pin.clone(),
        });
        let admitted = fixture.config.admit().unwrap();
        let prepared = admitted.prepare(&fixture.session).unwrap();
        let helper = fixture.session.join("console-fence");
        let startup = prepared
            .startup_expression(arguments(&helper, true), &fixture.session)
            .unwrap();
        assert!(startup.contains("'role','input','path',"));
        let disabled_tail = format!(",1,'','',{}", quoted_path(&fixture.session).unwrap()) + "});";
        assert!(startup.ends_with(&disabled_tail));
        assert!(
            prepared
                .validate_adapter_path(AdapterRole::Input, &pin.path)
                .is_ok()
        );
        assert!(
            prepared
                .validate_adapter_path(AdapterRole::Value, &pin.path)
                .is_err()
        );
        let mut enabled = arguments(&helper, false);
        enabled.input_bridge = Some(&pin.path);
        assert!(
            prepared
                .recovery_expression(enabled)
                .unwrap()
                .contains(&quoted_path(&pin.path).unwrap())
        );
        fs::write(&pin.path, b"replaced input").unwrap();
        assert!(prepared.command(&startup).is_err());
    }

    #[test]
    fn temporary_alias_preserves_storage_and_cleans_only_its_owned_link() {
        let mut fixture = fixture();
        let session = fixture.session.with_file_name("session with spaces");
        fs::rename(&fixture.session, &session).unwrap();
        fixture.session = session;
        let prepared = fixture
            .config
            .admit()
            .unwrap()
            .prepare(&fixture.session)
            .unwrap();
        let alias = prepared.temporary_alias.as_ref().unwrap();
        let alias_path = alias.path();
        let alias_directory = alias.directory.path().to_owned();
        assert!(literal_shell_path(&alias_path).unwrap());
        assert_eq!(fs::canonicalize(&alias_path).unwrap(), fixture.session);
        assert_eq!(
            fs::metadata(&alias_directory).unwrap().mode() & 0o777,
            0o700
        );
        fs::write(alias_path.join("export.eps"), b"owned export bytes").unwrap();
        assert_eq!(
            fs::read(fixture.session.join("export.eps")).unwrap(),
            b"owned export bytes"
        );
        let helper = fixture.session.join("console-fence");
        let startup = prepared
            .startup_expression(arguments(&helper, true), &fixture.session)
            .unwrap();
        for _ in 0..2 {
            let command = prepared.command(&startup).unwrap();
            assert_eq!(command.get_current_dir(), Some(fixture.session.as_path()));
            assert_eq!(
                command
                    .get_envs()
                    .find(|(key, _)| *key == std::ffi::OsStr::new("TMPDIR"))
                    .and_then(|(_, value)| value),
                Some(alias_path.as_os_str())
            );
            assert_eq!(
                command
                    .get_envs()
                    .find(|(key, _)| *key == std::ffi::OsStr::new("HOME"))
                    .and_then(|(_, value)| value),
                Some(fixture.session.as_os_str())
            );
        }
        drop(prepared);
        assert!(!alias_directory.exists());
        assert_eq!(
            fs::read(fixture.session.join("export.eps")).unwrap(),
            b"owned export bytes"
        );
    }

    #[test]
    fn temporary_alias_replacement_or_permission_change_fails_before_spawn() {
        for replace_link in [false, true] {
            let mut fixture = fixture();
            let session = fixture.session.with_file_name("session with spaces");
            fs::rename(&fixture.session, &session).unwrap();
            fixture.session = session;
            let outside = fixture.session.with_file_name("outside");
            fs::create_dir(&outside).unwrap();
            fs::write(outside.join("sentinel"), b"keep").unwrap();
            let prepared = fixture
                .config
                .admit()
                .unwrap()
                .prepare(&fixture.session)
                .unwrap();
            let alias = prepared.temporary_alias.as_ref().unwrap();
            let alias_directory = alias.directory.path().to_owned();
            let helper = fixture.session.join("console-fence");
            let startup = prepared
                .startup_expression(arguments(&helper, true), &fixture.session)
                .unwrap();
            if replace_link {
                fs::remove_file(alias.path()).unwrap();
                std::os::unix::fs::symlink(&outside, alias.path()).unwrap();
            } else {
                fs::set_permissions(&alias_directory, fs::Permissions::from_mode(0o755)).unwrap();
            }
            assert!(prepared.command(&startup).is_err());
            drop(prepared);
            assert!(!alias_directory.exists());
            assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"keep");
        }
    }

    #[test]
    fn private_source_and_executable_permissions_are_rechecked() {
        let fixture = fixture();
        let executable = fixture.config.executable.path.clone();
        let prepared = fixture
            .config
            .admit()
            .unwrap()
            .prepare(&fixture.session)
            .unwrap();
        let helper = fixture.session.join("console-fence");
        let startup = prepared
            .startup_expression(arguments(&helper, true), &fixture.session)
            .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(prepared.command(&startup).is_err());
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(fixture.session.join("mathlab_worker.m"), b"changed worker").unwrap();
        assert!(prepared.command(&startup).is_err());
        fs::set_permissions(&fixture.session, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(private_session(&fixture.session).is_err());
    }
}
