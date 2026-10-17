// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit, caller-pinned configuration for qualified frontend profiles.
//! Pins express the caller's selected files, not vendor authentication. Parsing
//! does not admit those files or establish Ready; `start` uses Engine admission.

use crate::{
    AdapterRole, Engine, Error, PinnedFile, QUALIFIED_GRAPHICS_STARTUP_PROFILE,
    QUALIFIED_STARTUP_PROFILE, Result, TrustedStartupConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_CONFIG_BYTES: usize = 1024 * 1024;
const PROVIDER_BASENAME: &str = "mathlab_intrinsic_bridge.oct";
const CORE_ADAPTERS: [(AdapterRole, &str); 4] = [
    (AdapterRole::Input, "mathlab_input_bridge.oct"),
    (AdapterRole::Scope, "mathlab_scope_bridge.oct"),
    (AdapterRole::Value, "mathlab_value_bridge.oct"),
    (AdapterRole::Debug, "mathlab_debug_bridge.oct"),
];
const GRAPHICS_ADAPTERS: [(AdapterRole, &str); 5] = [
    CORE_ADAPTERS[0],
    CORE_ADAPTERS[1],
    CORE_ADAPTERS[2],
    CORE_ADAPTERS[3],
    (AdapterRole::Graphics, "mathlab_graphics_bridge.oct"),
];

/// A strictly decoded selection. Only `start` admits its files and accepts
/// a live qualified Ready identity; callers cannot construct unchecked fields.
#[derive(Clone, Debug)]
pub struct QualifiedStartupConfig {
    profile: &'static str,
    startup: TrustedStartupConfig,
}

/// One selected native pin, suitable for binding a report to the configuration.
/// This selection alone is not evidence of accepted startup.
#[derive(Clone, Debug, Serialize)]
pub struct NativeModuleIdentity {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigDocument {
    schema_version: u32,
    profile: String,
    startup: TrustedStartupConfig,
}

impl QualifiedStartupConfig {
    /// Read strict UTF-8 JSON bounded to one MiB. Every configuration object
    /// rejects unknown and duplicate fields; no environment selection is used.
    pub fn load(path: &Path) -> Result<Self> {
        // Avoid opening a selected FIFO/device. Replacement can still race the
        // pathname check; verify the opened handle again before reading.
        if !std::fs::metadata(path)?.is_file() {
            return Err(Error::Protocol(
                "Qualified startup configuration must be a regular file".into(),
            ));
        }
        let file = File::open(path)?;
        if !file.metadata()?.is_file() {
            return Err(Error::Protocol(
                "Qualified startup configuration must be a regular file".into(),
            ));
        }
        let mut bytes = Vec::new();
        file.take((MAX_CONFIG_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        Self::parse(&bytes)
    }

    /// Load the current installed bundle without consulting the native paths
    /// recorded at packaging time. `contents` must be the physical canonical
    /// Contents directory derived by the frontend from its running executable.
    /// Only the selected native paths change; pins and external runtime selections
    /// remain literal. Admission and Ready acceptance still happen in `start`.
    /// Both profiles use the existing fixed `qualified-core-v2.json` slot; the
    /// strict envelope selects the profile, never the existence of another file.
    pub fn load_installed(contents: &Path) -> Result<Self> {
        if contents.file_name() != Some(std::ffi::OsStr::new("Contents")) {
            return Err(Error::Protocol(
                "Installed startup requires a Contents directory".into(),
            ));
        }
        installed_path(contents, true)?;
        let resources = contents.join("Resources");
        installed_path(&resources, true)?;
        let native = resources.join("native");
        installed_path(&native, true)?;
        let path = resources.join("qualified-core-v2.json");
        installed_path(&path, false)?;
        let mut config = Self::load(&path)?;
        // Parse first, then resolve only this fixed map. In particular, never
        // stat, open, or recover modules from the old document paths.
        config.startup.provider.path = installed_path(&native.join(PROVIDER_BASENAME), false)?;
        for &(role, basename) in config.adapter_specs() {
            let adapter = config
                .startup
                .adapters
                .iter_mut()
                .find(|adapter| adapter.role == role)
                .expect("configuration has every selected adapter");
            adapter.file.path = installed_path(&native.join(basename), false)?;
        }
        Ok(config)
    }

    fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err(Error::LimitExceeded(
                "Qualified startup configuration exceeds one MiB".into(),
            ));
        }
        let source = std::str::from_utf8(bytes)
            .map_err(|_| Error::Protocol("Qualified startup configuration must be UTF-8".into()))?;
        let document: ConfigDocument = serde_json::from_str(source)?;
        let profile = match document.profile.as_str() {
            QUALIFIED_STARTUP_PROFILE => QUALIFIED_STARTUP_PROFILE,
            QUALIFIED_GRAPHICS_STARTUP_PROFILE => QUALIFIED_GRAPHICS_STARTUP_PROFILE,
            _ => return Err(Error::Protocol("Unknown qualified startup profile".into())),
        };
        if document.schema_version != 2 {
            return Err(Error::Protocol(
                "Qualified startup configuration requires schema 2 with explicit Ghostscript selection".into(),
            ));
        }
        let startup = &document.startup;
        if std::iter::once(&startup.installation_root)
            .chain([
                &startup.executable.path,
                &startup.provider.path,
                &startup.gnuplot.path,
                &startup.ghostscript.path,
            ])
            .chain(startup.runtime_directories.iter())
            .chain(startup.adapters.iter().map(|adapter| &adapter.file.path))
            .any(|path| !path.is_absolute())
        {
            return Err(Error::Protocol(
                "Qualified startup configuration requires absolute paths".into(),
            ));
        }
        let specs = adapter_specs(profile);
        if document.startup.adapters.len() != specs.len() {
            return Err(roles_error(profile));
        }
        require_basename(&document.startup.provider.path, PROVIDER_BASENAME)?;
        for &(role, basename) in specs {
            let mut selected = document
                .startup
                .adapters
                .iter()
                .filter(|adapter| adapter.role == role);
            let adapter = selected.next().ok_or_else(|| roles_error(profile))?;
            if selected.next().is_some() {
                return Err(roles_error(profile));
            }
            require_basename(&adapter.file.path, basename)?;
        }
        Ok(Self {
            profile,
            startup: document.startup,
        })
    }

    pub fn executable(&self) -> &Path {
        &self.startup.executable.path
    }

    /// The selected profile name, which alone does not establish activation.
    pub fn profile(&self) -> &'static str {
        self.profile
    }

    fn adapter_specs(&self) -> &'static [(AdapterRole, &'static str)] {
        adapter_specs(self.profile)
    }

    /// The five or six expected basenames map to selected paths and pins.
    /// Admission and readiness must still be established by `start`.
    pub fn native_modules(&self) -> BTreeMap<String, NativeModuleIdentity> {
        let mut modules = BTreeMap::from([(
            PROVIDER_BASENAME.into(),
            native_identity(&self.startup.provider),
        )]);
        for &(role, basename) in self.adapter_specs() {
            let adapter = self
                .startup
                .adapters
                .iter()
                .find(|adapter| adapter.role == role)
                .expect("qualified configuration has each validated selected adapter");
            modules.insert(basename.into(), native_identity(&adapter.file));
        }
        modules
    }

    /// Reuse pre-spawn admission and enable every selected adapter before user
    /// execution. Return only with an accepted selected profile and activation.
    /// Any failure drops the Engine and its child; there is no fallback route.
    pub fn start(&self, working_directory: PathBuf) -> Result<Engine> {
        let mut engine = Engine::start_qualified(working_directory, self.startup.clone())?;
        for &(role, _) in self.adapter_specs() {
            let adapter = self
                .startup
                .adapters
                .iter()
                .find(|adapter| adapter.role == role)
                .expect("configuration has every selected adapter");
            match role {
                AdapterRole::Input => {
                    engine.enable_input_adapter(&adapter.file.path)?;
                }
                AdapterRole::Scope => engine.enable_scope_bridge(&adapter.file.path)?,
                AdapterRole::Value => engine.enable_value_bridge(&adapter.file.path)?,
                AdapterRole::Debug => {
                    engine.enable_debugger(&adapter.file.path)?;
                }
                AdapterRole::Graphics => engine.enable_graphics_service(&adapter.file.path)?,
            }
        }
        self.require_active(&engine)?;
        Ok(engine)
    }

    /// Require the live accepted identity to match this selection. The graphics
    /// profile additionally requires its currently active registry capability;
    /// an admitted graphics role alone is not activation.
    pub fn require_active(&self, engine: &Engine) -> Result<()> {
        self.require_selection(engine)?;
        let status = engine.qualified_startup_status().ok_or_else(|| {
            Error::Protocol("Configured startup has no current accepted status".into())
        })?;
        let ready = &status.intrinsics;
        if status.profile != self.profile
            || ready.provider_file
                != crate::wire_path(&self.startup.provider.path, "selected provider")?
            || ready.adapters.len() != self.startup.adapters.len()
            || self.startup.adapters.iter().any(|adapter| {
                let role = match adapter.role {
                    AdapterRole::Input => "input",
                    AdapterRole::Scope => "scope",
                    AdapterRole::Value => "value",
                    AdapterRole::Debug => "debug",
                    AdapterRole::Graphics => "graphics",
                };
                ready
                    .adapters
                    .get(role)
                    .is_none_or(|observed| Path::new(&observed.path) != adapter.file.path)
            })
        {
            return Err(Error::Protocol(
                "Configured startup does not match the current accepted profile/native selection"
                    .into(),
            ));
        }
        if self.profile == QUALIFIED_GRAPHICS_STARTUP_PROFILE {
            engine.require_capability(crate::graphics::CAP_GRAPHICS_REGISTRY)?;
        }
        Ok(())
    }

    fn require_selection(&self, engine: &Engine) -> Result<()> {
        if !engine
            .trusted_startup
            .as_ref()
            .is_some_and(|startup| startup.matches_selection(&self.startup))
        {
            return Err(Error::Protocol(
                "Configuration differs from the Engine's admitted startup selection".into(),
            ));
        }
        Ok(())
    }

    /// Restart the selected frontend session and explicitly reactivate graphics.
    /// `Engine::restart` itself retains its existing graphics-disabled contract.
    /// A different selection is rejected before restart. A failed restart or
    /// activation retires the child instead of exposing a partial route.
    pub fn restart(&self, engine: &mut Engine) -> Result<()> {
        self.require_selection(engine)?;
        let result = (|| {
            engine.restart()?;
            if self.profile == QUALIFIED_GRAPHICS_STARTUP_PROFILE {
                let adapter = self
                    .startup
                    .adapters
                    .iter()
                    .find(|adapter| adapter.role == AdapterRole::Graphics)
                    .expect("graphics configuration has its validated adapter");
                engine.enable_graphics_service(&adapter.file.path)?;
            }
            self.require_active(engine)
        })();
        if result.is_err() {
            engine.stop();
        }
        result
    }
}

// Exact canonical spelling rejects symlinked directories/files and escapes,
// including aliases that resolve back inside the bundle. No writes are needed.
fn installed_path(path: &Path, directory: bool) -> Result<PathBuf> {
    if !path.is_absolute() || path.canonicalize()?.as_os_str() != path.as_os_str() {
        return Err(Error::Protocol(
            "Installed startup paths must have their physical canonical spelling".into(),
        ));
    }
    let metadata = std::fs::symlink_metadata(path)?;
    let valid = if directory {
        metadata.is_dir()
    } else {
        metadata.is_file()
    };
    if !valid {
        return Err(Error::Protocol(
            "Installed startup layout requires directories and regular files".into(),
        ));
    }
    Ok(path.to_owned())
}

fn adapter_specs(profile: &str) -> &'static [(AdapterRole, &'static str)] {
    if profile == QUALIFIED_GRAPHICS_STARTUP_PROFILE {
        &GRAPHICS_ADAPTERS
    } else {
        &CORE_ADAPTERS
    }
}

fn roles_error(profile: &str) -> Error {
    Error::Protocol(format!(
        "{profile} configuration requires exactly one input, scope, value and debug adapter{}",
        if profile == QUALIFIED_GRAPHICS_STARTUP_PROFILE {
            ", plus one graphics adapter"
        } else {
            ""
        },
    ))
}

fn require_basename(path: &Path, basename: &str) -> Result<()> {
    if path.file_name() != Some(std::ffi::OsStr::new(basename)) {
        return Err(Error::Protocol(format!(
            "Qualified native module must have basename {basename}"
        )));
    }
    Ok(())
}

fn native_identity(file: &PinnedFile) -> NativeModuleIdentity {
    NativeModuleIdentity {
        path: file.path.clone(),
        sha256: file
            .sha256
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn document() -> Value {
        let pin = |path: &str| json!({"path": path, "sha256": "0123456789abcdef".repeat(4)});
        let adapters: Vec<_> = CORE_ADAPTERS
            .iter()
            .zip(["input", "scope", "value", "debug"])
            .map(|((_, name), role)| {
                json!({"role": role, "file": pin(&format!("/selected/native/{name}"))})
            })
            .collect();
        json!({
            "schema_version": 2,
            "profile": QUALIFIED_STARTUP_PROFILE,
            "startup": {
                "installation_root": "/selected/octave",
                "executable": pin("/selected/octave/bin/octave-cli"),
                "provider": pin("/selected/native/mathlab_intrinsic_bridge.oct"),
                "gnuplot": pin("/selected/gnuplot/bin/gnuplot"),
                "ghostscript": pin("/selected/ghostscript/bin/gs"),
                "runtime_directories": ["/selected/octave/share/octave/11.3.0/m"],
                "adapters": adapters,
                "octave_version": "11.3.0",
                "octave_api_version": "api-v61"
            }
        })
    }

    fn parse(document: &Value) -> Result<QualifiedStartupConfig> {
        QualifiedStartupConfig::parse(&serde_json::to_vec(document).unwrap())
    }

    fn graphics_document() -> Value {
        let mut selected = document();
        selected["profile"] = json!(QUALIFIED_GRAPHICS_STARTUP_PROFILE);
        selected["startup"]["adapters"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "role": "graphics", "file": {
                    "path": "/selected/native/mathlab_graphics_bridge.oct",
                    "sha256": "0123456789abcdef".repeat(4),
                }
            }));
        selected
    }

    #[test]
    fn graphics_profile_requires_exact_roles_and_reports_six_selected_pins() {
        let mut selected = graphics_document();
        let config = parse(&selected).unwrap();
        assert_eq!(config.profile(), QUALIFIED_GRAPHICS_STARTUP_PROFILE);
        assert_eq!(config.native_modules().len(), 6);
        let identity = &config.native_modules()["mathlab_graphics_bridge.oct"];
        assert_eq!(
            identity.path,
            Path::new("/selected/native/mathlab_graphics_bridge.oct")
        );
        assert_eq!(identity.sha256, "0123456789abcdef".repeat(4));
        selected["startup"]["adapters"]
            .as_array_mut()
            .unwrap()
            .reverse();
        assert!(parse(&selected).is_ok());
        for profile in [
            QUALIFIED_STARTUP_PROFILE,
            QUALIFIED_GRAPHICS_STARTUP_PROFILE,
        ] {
            let mut mismatch = if profile == QUALIFIED_STARTUP_PROFILE {
                graphics_document()
            } else {
                document()
            };
            mismatch["profile"] = json!(profile);
            assert!(parse(&mismatch).is_err());
        }
        for role in ["input", "scope", "value", "debug", "unknown"] {
            let mut invalid = graphics_document();
            invalid["startup"]["adapters"][4]["role"] = json!(role);
            assert!(parse(&invalid).is_err(), "{role}");
        }
        for (field, value) in [
            ("path", json!("/selected/native/other.oct")),
            ("path", json!("relative/mathlab_graphics_bridge.oct")),
            ("sha256", json!("A".repeat(64))),
            ("extra", json!(true)),
        ] {
            let mut invalid = graphics_document();
            invalid["startup"]["adapters"][4]["file"][field] = value;
            assert!(parse(&invalid).is_err(), "{field}");
        }
    }

    #[test]
    fn loads_selection_without_admitting_files_and_reports_exact_pins() {
        let document = document();
        let directory = tempfile::tempdir().unwrap();
        assert!(
            QualifiedStartupConfig::load(directory.path())
                .unwrap_err()
                .to_string()
                .contains("regular file")
        );
        let path = directory.path().join("qualified.json");
        std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        let config = QualifiedStartupConfig::load(&path).unwrap();
        assert_eq!(
            config.executable(),
            Path::new("/selected/octave/bin/octave-cli")
        );
        let modules = config.clone().native_modules();
        let expected: std::collections::BTreeSet<_> = std::iter::once(PROVIDER_BASENAME)
            .chain(CORE_ADAPTERS.iter().map(|(_, basename)| *basename))
            .map(str::to_owned)
            .collect();
        assert_eq!(
            modules
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            expected
        );
        for (basename, identity) in modules {
            assert_eq!(identity.path, Path::new("/selected/native").join(basename));
            assert_eq!(identity.sha256, "0123456789abcdef".repeat(4));
            let serialized = serde_json::to_value(identity).unwrap();
            assert_eq!(serialized.as_object().unwrap().len(), 2);
            assert!(serialized["path"].is_string());
            assert!(serialized["sha256"].is_string());
        }
    }

    #[test]
    fn rejects_malformed_json_utf8_and_trailing_documents() {
        for bytes in [b"".as_slice(), b"{", b"null", b"[]", b"\xff"] {
            assert!(QualifiedStartupConfig::parse(bytes).is_err());
        }
        let source = document().to_string() + "{}";
        assert!(QualifiedStartupConfig::parse(source.as_bytes()).is_err());
        assert!(
            QualifiedStartupConfig::parse(b"\xff")
                .unwrap_err()
                .to_string()
                .contains("UTF-8")
        );
    }

    #[test]
    fn rejects_unknown_and_missing_fields_at_every_object_boundary() {
        for pointer in [
            "",
            "/startup",
            "/startup/executable",
            "/startup/adapters/0",
            "/startup/adapters/0/file",
            "/startup/ghostscript",
        ] {
            let mut invalid = document();
            invalid.pointer_mut(pointer).unwrap()["unexpected"] = json!(true);
            assert!(
                parse(&invalid)
                    .unwrap_err()
                    .to_string()
                    .contains("unknown field"),
                "{pointer}"
            );
        }
        for (pointer, field) in [
            ("", "startup"),
            ("/startup", "gnuplot"),
            ("/startup", "ghostscript"),
            ("/startup/provider", "sha256"),
            ("/startup/adapters/0", "role"),
        ] {
            let mut invalid = document();
            invalid
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                parse(&invalid)
                    .unwrap_err()
                    .to_string()
                    .contains("missing field"),
                "{pointer}/{field}"
            );
        }
    }

    #[test]
    fn rejects_duplicate_fields_including_nested_pins_and_adapters() {
        let source = document().to_string();
        for (from, to) in [
            (
                "\"schema_version\":2",
                "\"schema_version\":2,\"schema_version\":2",
            ),
            (
                "\"startup\":{",
                "\"startup\":{\"octave_version\":\"11.3.0\",",
            ),
            (
                "\"executable\":{",
                "\"executable\":{\"path\":\"/duplicate\",",
            ),
            (
                "\"ghostscript\":{",
                "\"ghostscript\":{\"sha256\":\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\",",
            ),
            (
                "\"role\":\"input\"",
                "\"role\":\"input\",\"role\":\"input\"",
            ),
            ("\"file\":{", "\"file\":{\"path\":\"/duplicate\","),
        ] {
            assert!(source.contains(from));
            let invalid = source.replacen(from, to, 1);
            assert!(
                QualifiedStartupConfig::parse(invalid.as_bytes())
                    .unwrap_err()
                    .to_string()
                    .contains("duplicate field"),
                "{from}"
            );
        }
    }

    #[test]
    fn rejects_unknown_schemas_profiles_and_graphics_without_its_role() {
        for schema in [
            json!(0),
            json!(1),
            json!(3),
            json!(-1),
            json!(1.0),
            json!("1"),
            Value::Null,
        ] {
            let mut invalid = document();
            invalid["schema_version"] = schema;
            assert!(parse(&invalid).is_err());
        }
        for profile in [
            json!("qualified-graphics-v2"),
            json!("qualified-core-v1"),
            json!(""),
            json!(1),
            Value::Null,
        ] {
            let mut invalid = document();
            invalid["profile"] = profile;
            assert!(parse(&invalid).is_err());
        }
    }

    #[test]
    fn requires_four_distinct_core_roles_and_expected_native_basenames() {
        let mut invalid = document();
        invalid["startup"]["adapters"].as_array_mut().unwrap().pop();
        assert!(parse(&invalid).is_err());
        let mut invalid = document();
        let extra = invalid["startup"]["adapters"][0].clone();
        invalid["startup"]["adapters"]
            .as_array_mut()
            .unwrap()
            .push(extra);
        assert!(parse(&invalid).is_err());
        for role in ["input", "graphics", "Input", "unknown"] {
            let mut invalid = document();
            invalid["startup"]["adapters"][3]["role"] = json!(role);
            assert!(parse(&invalid).is_err(), "{role}");
        }
        for role in [
            json!({"debug": null}),
            json!(["debug"]),
            json!(0),
            Value::Null,
        ] {
            let mut invalid = document();
            invalid["startup"]["adapters"][3]["role"] = role;
            assert!(parse(&invalid).is_err());
        }
        for pointer in [
            "/startup/provider/path",
            "/startup/adapters/0/file/path",
            "/startup/adapters/1/file/path",
            "/startup/adapters/2/file/path",
            "/startup/adapters/3/file/path",
        ] {
            let mut invalid = document();
            *invalid.pointer_mut(pointer).unwrap() = json!("/selected/native/wrong.oct");
            assert!(
                parse(&invalid)
                    .unwrap_err()
                    .to_string()
                    .contains("basename"),
                "{pointer}"
            );
        }
        let mut reordered = document();
        reordered["startup"]["adapters"]
            .as_array_mut()
            .unwrap()
            .reverse();
        assert!(parse(&reordered).is_ok());
    }

    #[test]
    fn rejects_noncanonical_sha256_strings_for_every_pin() {
        for pointer in [
            "/startup/executable/sha256",
            "/startup/provider/sha256",
            "/startup/gnuplot/sha256",
            "/startup/ghostscript/sha256",
            "/startup/adapters/0/file/sha256",
        ] {
            for hash in [
                json!(""),
                json!("a".repeat(63)),
                json!("a".repeat(65)),
                json!("A".repeat(64)),
                json!("g".repeat(64)),
                json!("é".repeat(32)),
                json!(vec![0; 32]),
                Value::Null,
            ] {
                let mut invalid = document();
                *invalid.pointer_mut(pointer).unwrap() = hash;
                assert!(parse(&invalid).is_err(), "{pointer}");
            }
        }
    }

    #[test]
    fn rejects_relative_paths_without_normalizing_or_admitting_files() {
        for pointer in [
            "/startup/installation_root",
            "/startup/executable/path",
            "/startup/provider/path",
            "/startup/gnuplot/path",
            "/startup/ghostscript/path",
            "/startup/runtime_directories/0",
            "/startup/adapters/0/file/path",
            "/startup/adapters/1/file/path",
            "/startup/adapters/2/file/path",
            "/startup/adapters/3/file/path",
        ] {
            for path in ["", "relative/path"] {
                let mut invalid = document();
                *invalid.pointer_mut(pointer).unwrap() = json!(path);
                assert!(
                    parse(&invalid)
                        .unwrap_err()
                        .to_string()
                        .contains("absolute paths"),
                    "{pointer}"
                );
            }
        }
        let mut selected = document();
        selected["startup"]["executable"]["path"] = json!("/selected/../unchanged/octave-cli");
        assert_eq!(
            parse(&selected).unwrap().executable(),
            Path::new("/selected/../unchanged/octave-cli")
        );
    }

    #[test]
    fn enforces_one_mib_limit_in_parser_and_bounded_file_load() {
        let mut bytes = serde_json::to_vec(&document()).unwrap();
        bytes.resize(MAX_CONFIG_BYTES, b' ');
        assert!(QualifiedStartupConfig::parse(&bytes).is_ok());
        bytes.push(b' ');
        assert!(matches!(
            QualifiedStartupConfig::parse(&bytes),
            Err(Error::LimitExceeded(_))
        ));
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("oversized.json");
        std::fs::write(&path, &bytes).unwrap();
        assert!(matches!(
            QualifiedStartupConfig::load(&path),
            Err(Error::LimitExceeded(_))
        ));
    }

    struct InstalledFixture {
        _directory: tempfile::TempDir,
        contents: PathBuf,
        old_native: PathBuf,
        document: Value,
    }

    impl InstalledFixture {
        fn new() -> Self {
            Self::for_profile(false)
        }

        fn for_profile(graphics: bool) -> Self {
            use sha2::{Digest, Sha256};
            let directory = tempfile::tempdir().unwrap();
            let base = directory.path().canonicalize().unwrap();
            let contents = base.join("Moved app λ space.app/Contents");
            let native = contents.join("Resources/native");
            let old_native = base.join("original.app/Contents/Resources/native");
            std::fs::create_dir_all(&native).unwrap();
            std::fs::create_dir_all(&old_native).unwrap();
            let runtime = base.join("runtime");
            std::fs::create_dir_all(runtime.join("m")).unwrap();
            let executable = runtime.join("octave-cli");
            let gnuplot = base.join("gnuplot");
            let ghostscript = base.join("gs");
            let pin = |path: &Path, bytes: &[u8]| {
                std::fs::write(path, bytes).unwrap();
                json!({"path": path, "sha256": format!("{:x}", Sha256::digest(bytes))})
            };
            let mut document = if graphics {
                graphics_document()
            } else {
                document()
            };
            document["startup"]["installation_root"] = json!(runtime);
            document["startup"]["runtime_directories"] = json!([runtime.join("m")]);
            document["startup"]["executable"] = pin(&executable, b"selected external executable");
            document["startup"]["gnuplot"] = pin(&gnuplot, b"selected external gnuplot");
            document["startup"]["ghostscript"] =
                pin(&ghostscript, b"selected external Ghostscript");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                for path in [&executable, &gnuplot, &ghostscript] {
                    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
                }
            }
            let bytes = b"selected module";
            document["startup"]["provider"] = pin(&old_native.join(PROVIDER_BASENAME), bytes);
            std::fs::write(native.join(PROVIDER_BASENAME), bytes).unwrap();
            for (adapter, (_, basename)) in document["startup"]["adapters"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .zip(
                    adapter_specs(if graphics {
                        QUALIFIED_GRAPHICS_STARTUP_PROFILE
                    } else {
                        QUALIFIED_STARTUP_PROFILE
                    })
                    .iter()
                    .copied(),
                )
            {
                adapter["file"] = pin(&old_native.join(basename), bytes);
                std::fs::write(native.join(basename), bytes).unwrap();
            }
            let fixture = Self {
                _directory: directory,
                contents,
                old_native,
                document,
            };
            fixture.write(&fixture.document);
            fixture
        }

        fn config_path(&self) -> PathBuf {
            self.contents.join("Resources/qualified-core-v2.json")
        }

        fn write(&self, document: &Value) {
            std::fs::write(self.config_path(), serde_json::to_vec(document).unwrap()).unwrap();
        }
    }

    #[test]
    fn installed_graphics_rebinds_six_pins_with_original_present_or_absent() {
        let fixture = InstalledFixture::for_profile(true);
        let literal = QualifiedStartupConfig::load(&fixture.config_path()).unwrap();
        for remove_original in [false, true] {
            if remove_original {
                std::fs::remove_dir_all(&fixture.old_native).unwrap();
            }
            let current = QualifiedStartupConfig::load_installed(&fixture.contents).unwrap();
            assert_eq!(current.profile(), QUALIFIED_GRAPHICS_STARTUP_PROFILE);
            assert_eq!(current.native_modules().len(), 6);
            assert_eq!(
                current.startup.installation_root,
                literal.startup.installation_root
            );
            assert_eq!(
                current.startup.executable.path,
                literal.startup.executable.path
            );
            assert_eq!(
                current.startup.executable.sha256,
                literal.startup.executable.sha256
            );
            assert_eq!(current.startup.gnuplot.path, literal.startup.gnuplot.path);
            assert_eq!(
                current.startup.gnuplot.sha256,
                literal.startup.gnuplot.sha256
            );
            assert_eq!(
                current.startup.ghostscript.path,
                literal.startup.ghostscript.path
            );
            assert_eq!(
                current.startup.ghostscript.sha256,
                literal.startup.ghostscript.sha256
            );
            assert_eq!(
                current.startup.runtime_directories,
                literal.startup.runtime_directories
            );
            assert_eq!(
                current.startup.octave_version,
                literal.startup.octave_version
            );
            assert_eq!(
                current.startup.octave_api_version,
                literal.startup.octave_api_version
            );
            for (basename, selected) in current.native_modules() {
                assert_eq!(
                    selected.path,
                    fixture.contents.join("Resources/native").join(&basename)
                );
                assert_eq!(selected.sha256, literal.native_modules()[&basename].sha256);
            }
            #[cfg(unix)]
            assert!(current.startup.admit().is_ok());
        }
        assert_eq!(
            QualifiedStartupConfig::load(&fixture.config_path())
                .unwrap()
                .native_modules()["mathlab_graphics_bridge.oct"]
                .path,
            fixture.old_native.join("mathlab_graphics_bridge.oct")
        );
    }

    #[test]
    #[cfg(unix)]
    fn installed_graphics_tamper_missing_file_and_aliases_never_use_valid_original() {
        use std::os::unix::fs::symlink;
        let fixture = InstalledFixture::for_profile(true);
        let selected = fixture
            .contents
            .join("Resources/native/mathlab_graphics_bridge.oct");
        std::fs::write(&selected, b"tampered graphics module").unwrap();
        assert!(
            QualifiedStartupConfig::load(&fixture.config_path())
                .unwrap()
                .startup
                .admit()
                .is_ok()
        );
        assert!(
            QualifiedStartupConfig::load_installed(&fixture.contents)
                .unwrap()
                .startup
                .admit()
                .unwrap_err()
                .to_string()
                .contains("SHA-256")
        );
        std::fs::remove_file(&selected).unwrap();
        assert!(QualifiedStartupConfig::load_installed(&fixture.contents).is_err());
        let internal = fixture.contents.join("Resources/native/alias.oct");
        std::fs::write(&internal, b"selected module").unwrap();
        for target in [
            fixture.old_native.join("mathlab_graphics_bridge.oct"),
            internal,
        ] {
            symlink(target, &selected).unwrap();
            assert!(QualifiedStartupConfig::load_installed(&fixture.contents).is_err());
            std::fs::remove_file(&selected).unwrap();
        }
    }

    #[test]
    fn installed_rebinds_only_native_paths_with_original_present_or_absent() {
        let fixture = InstalledFixture::new();
        let literal = QualifiedStartupConfig::load(&fixture.config_path()).unwrap();
        for remove_original in [false, true] {
            if remove_original {
                std::fs::remove_dir_all(&fixture.old_native).unwrap();
            }
            let installed = QualifiedStartupConfig::load_installed(&fixture.contents).unwrap();
            let native = fixture.contents.join("Resources/native");
            assert_eq!(
                installed.startup.installation_root,
                literal.startup.installation_root
            );
            assert_eq!(
                installed.startup.executable.path,
                literal.startup.executable.path
            );
            assert_eq!(
                installed.startup.executable.sha256,
                literal.startup.executable.sha256
            );
            assert_eq!(installed.startup.gnuplot.path, literal.startup.gnuplot.path);
            assert_eq!(
                installed.startup.gnuplot.sha256,
                literal.startup.gnuplot.sha256
            );
            assert_eq!(
                installed.startup.ghostscript.path,
                literal.startup.ghostscript.path
            );
            assert_eq!(
                installed.startup.ghostscript.sha256,
                literal.startup.ghostscript.sha256
            );
            assert_eq!(
                installed.startup.runtime_directories,
                literal.startup.runtime_directories
            );
            assert_eq!(
                installed.startup.octave_version,
                literal.startup.octave_version
            );
            assert_eq!(
                installed.startup.octave_api_version,
                literal.startup.octave_api_version
            );
            for (basename, selected) in installed.native_modules() {
                assert_eq!(selected.path, native.join(&basename));
                let original = &literal.native_modules()[&basename];
                assert_eq!(original.path, fixture.old_native.join(basename));
                assert_eq!(selected.sha256, original.sha256);
            }
            #[cfg(unix)]
            assert!(installed.startup.admit().is_ok());
        }
        // The ordinary explicit loader keeps the missing original paths literal.
        assert_eq!(
            QualifiedStartupConfig::load(&fixture.config_path())
                .unwrap()
                .startup
                .provider
                .path,
            fixture.old_native.join(PROVIDER_BASENAME)
        );
    }

    #[test]
    #[cfg(unix)]
    fn installed_tamper_is_rejected_while_original_still_admits() {
        let fixture = InstalledFixture::new();
        std::fs::write(
            fixture
                .contents
                .join("Resources/native")
                .join(PROVIDER_BASENAME),
            b"tampered current module",
        )
        .unwrap();
        assert!(
            QualifiedStartupConfig::load(&fixture.config_path())
                .unwrap()
                .startup
                .admit()
                .is_ok()
        );
        let current = QualifiedStartupConfig::load_installed(&fixture.contents).unwrap();
        let error = current.startup.admit().unwrap_err().to_string();
        assert!(error.contains("SHA-256"), "{error}");
    }

    #[test]
    #[cfg(unix)]
    fn installed_ghostscript_selection_remains_external_without_bundle_fallback() {
        use std::os::unix::fs::PermissionsExt;
        for graphics in [false, true] {
            let fixture = InstalledFixture::for_profile(graphics);
            let selected = QualifiedStartupConfig::load_installed(&fixture.contents).unwrap();
            let ghostscript = selected.startup.ghostscript.clone();
            let replacement = fixture.contents.join("Resources/native/gs");
            std::fs::copy(&ghostscript.path, &replacement).unwrap();
            std::fs::set_permissions(&replacement, std::fs::Permissions::from_mode(0o700)).unwrap();
            std::fs::write(&ghostscript.path, b"tampered external gs").unwrap();
            let selected = QualifiedStartupConfig::load_installed(&fixture.contents).unwrap();
            assert_eq!(selected.startup.ghostscript.path, ghostscript.path);
            assert!(selected.startup.admit().is_err());
            std::fs::remove_file(&ghostscript.path).unwrap();
            let selected = QualifiedStartupConfig::load_installed(&fixture.contents).unwrap();
            assert_eq!(selected.startup.ghostscript.sha256, ghostscript.sha256);
            assert!(selected.startup.admit().is_err());
            assert!(replacement.is_file());
        }
    }

    #[test]
    fn installed_loader_keeps_strict_schema_and_missing_layout_failures() {
        let fixture = InstalledFixture::new();
        for bytes in [b"{".as_slice(), b"\xff", b"{}{}"] {
            std::fs::write(fixture.config_path(), bytes).unwrap();
            assert!(QualifiedStartupConfig::load_installed(&fixture.contents).is_err());
        }
        for pointer in [
            "",
            "/startup",
            "/startup/provider",
            "/startup/adapters/0/file",
        ] {
            let mut document = fixture.document.clone();
            document.pointer_mut(pointer).unwrap()["extra"] = json!(true);
            fixture.write(&document);
            assert!(QualifiedStartupConfig::load_installed(&fixture.contents).is_err());
        }
        let source = fixture.document.to_string().replacen(
            "\"schema_version\":2",
            "\"schema_version\":2,\"schema_version\":2",
            1,
        );
        std::fs::write(fixture.config_path(), source).unwrap();
        assert!(QualifiedStartupConfig::load_installed(&fixture.contents).is_err());
        std::fs::write(fixture.config_path(), vec![b' '; MAX_CONFIG_BYTES + 1]).unwrap();
        assert!(matches!(
            QualifiedStartupConfig::load_installed(&fixture.contents),
            Err(Error::LimitExceeded(_))
        ));
        fixture.write(&fixture.document);
        for (_, basename) in CORE_ADAPTERS {
            let path = fixture.contents.join("Resources/native").join(basename);
            std::fs::remove_file(&path).unwrap();
            assert!(QualifiedStartupConfig::load_installed(&fixture.contents).is_err());
            std::fs::write(path, b"selected module").unwrap();
        }
        let native = fixture.contents.join("Resources/native");
        let provider = native.join(PROVIDER_BASENAME);
        std::fs::remove_file(&provider).unwrap();
        assert!(QualifiedStartupConfig::load_installed(&fixture.contents).is_err());
        std::fs::write(provider, b"selected module").unwrap();
        let saved_native = fixture.contents.join("saved-native");
        std::fs::rename(&native, &saved_native).unwrap();
        assert!(QualifiedStartupConfig::load_installed(&fixture.contents).is_err());
        std::fs::rename(saved_native, native).unwrap();
        std::fs::remove_file(fixture.config_path()).unwrap();
        assert!(QualifiedStartupConfig::load_installed(&fixture.contents).is_err());
        assert!(QualifiedStartupConfig::load_installed(Path::new("relative/Contents")).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn installed_rejects_directory_config_and_module_aliases_and_escapes() {
        use std::os::unix::fs::symlink;
        let fixture = InstalledFixture::new();
        let resources = fixture.contents.join("Resources");
        let native = resources.join("native");
        for target in [
            fixture.old_native.join(PROVIDER_BASENAME),
            native.join("other.oct"),
        ] {
            let selected = native.join(PROVIDER_BASENAME);
            std::fs::write(&target, b"selected module").unwrap();
            std::fs::remove_file(&selected).unwrap();
            symlink(&target, &selected).unwrap();
            assert!(QualifiedStartupConfig::load_installed(&fixture.contents).is_err());
            std::fs::remove_file(&selected).unwrap();
            std::fs::write(selected, b"selected module").unwrap();
        }
        let config_target = fixture.old_native.join("config.json");
        std::fs::rename(fixture.config_path(), &config_target).unwrap();
        symlink(&config_target, fixture.config_path()).unwrap();
        assert!(QualifiedStartupConfig::load_installed(&fixture.contents).is_err());
        std::fs::remove_file(fixture.config_path()).unwrap();
        std::fs::rename(config_target, fixture.config_path()).unwrap();
        let saved_native = resources.join("saved-native");
        std::fs::rename(&native, &saved_native).unwrap();
        for target in [&fixture.old_native, &saved_native] {
            symlink(target, &native).unwrap();
            assert!(QualifiedStartupConfig::load_installed(&fixture.contents).is_err());
            std::fs::remove_file(&native).unwrap();
        }
        std::fs::rename(saved_native, native).unwrap();
        let saved_resources = fixture
            .contents
            .parent()
            .unwrap()
            .join("resources-elsewhere");
        std::fs::rename(&resources, &saved_resources).unwrap();
        symlink(&saved_resources, &resources).unwrap();
        assert!(QualifiedStartupConfig::load_installed(&fixture.contents).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn installed_reads_unicode_space_bundle_without_writing_readonly_resources() {
        use std::os::unix::fs::PermissionsExt;
        for graphics in [false, true] {
            let fixture = InstalledFixture::for_profile(graphics);
            let resources = fixture.contents.join("Resources");
            let native = resources.join("native");
            let config_before = std::fs::read(fixture.config_path()).unwrap();
            let mut paths = vec![fixture.config_path()];
            paths.extend(
                QualifiedStartupConfig::load(&fixture.config_path())
                    .unwrap()
                    .native_modules()
                    .keys()
                    .map(|name| native.join(name)),
            );
            for path in &paths {
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o444)).unwrap();
            }
            for path in [&native, &resources] {
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o555)).unwrap();
            }
            let result = QualifiedStartupConfig::load_installed(&fixture.contents);
            let config_after = std::fs::read(fixture.config_path()).unwrap();
            let entries = std::fs::read_dir(&resources).unwrap().count();
            // Restore only test-owned permissions so TempDir cleanup is reliable.
            for path in [&resources, &native] {
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            assert!(result.is_ok());
            assert_eq!(config_before, config_after);
            assert_eq!(entries, 2);
        }
    }

    #[test]
    #[ignore = "UNRUN: requires MATHLAB_QUALIFIED_GRAPHICS_STARTUP_CONFIG and six admitted native modules"]
    fn configured_graphics_start_and_restart_require_actual_activation_and_fail_closed() {
        let path = std::env::var_os("MATHLAB_QUALIFIED_GRAPHICS_STARTUP_CONFIG")
            .expect("graphics configuration fixture must be explicitly selected");
        let config = QualifiedStartupConfig::load(Path::new(&path)).unwrap();
        assert_eq!(config.profile(), QUALIFIED_GRAPHICS_STARTUP_PROFILE);
        let directory = tempfile::tempdir().unwrap();
        let mut engine = config
            .start(directory.path().canonicalize().unwrap())
            .unwrap();
        config.require_active(&engine).unwrap();
        assert_eq!(config.native_modules().len(), 6);
        assert!(engine.graphics_snapshot().unwrap().nodes.is_empty());
        let scene = "gf=figure('visible','off'); ga=axes('parent',gf); gl=line('parent',ga,'xdata',[1,2,3],'ydata',[10,20,30]);";
        let result = engine.execute(scene).unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        let snapshot = engine.graphics_snapshot().unwrap();
        assert_eq!(snapshot.nodes.len(), 3);
        let old = snapshot
            .nodes
            .iter()
            .find(|node| {
                matches!(
                    node.properties,
                    crate::graphics::GraphicsProperties::Line { .. }
                )
            })
            .unwrap()
            .token
            .clone();
        let samples = engine.graphics_query(&old, 1, 2).unwrap().samples.unwrap();
        assert_eq!(samples.x, vec![1., 2.]);
        assert_eq!(samples.y, vec![10., 20.]);
        // Keep the accepted low-level restart contract explicit: an admitted
        // graphics profile is not a currently active registry.
        engine.restart().unwrap();
        assert_eq!(
            engine.qualified_startup_status().unwrap().profile,
            QUALIFIED_GRAPHICS_STARTUP_PROFILE
        );
        assert!(config.require_active(&engine).is_err());
        assert!(engine.graphics_snapshot().is_err());
        config.restart(&mut engine).unwrap();
        config.require_active(&engine).unwrap();
        assert!(engine.graphics_snapshot().unwrap().nodes.is_empty());
        assert!(engine.graphics_query(&old, 1, 2).is_err());
        let mut wrong_profile = config.clone();
        wrong_profile.profile = QUALIFIED_STARTUP_PROFILE;
        wrong_profile
            .startup
            .adapters
            .retain(|adapter| adapter.role != AdapterRole::Graphics);
        let mut wrong_path = config.clone();
        let other = directory
            .path()
            .canonicalize()
            .unwrap()
            .join("other_graphics.oct");
        std::fs::write(&other, b"unadmitted adapter").unwrap();
        wrong_path
            .startup
            .adapters
            .iter_mut()
            .find(|adapter| adapter.role == AdapterRole::Graphics)
            .unwrap()
            .file
            .path = other;
        let mut wrong_pin = config.clone();
        wrong_pin.startup.provider.sha256[0] ^= 1;
        let mut wrong_ghostscript_path = config.clone();
        wrong_ghostscript_path.startup.ghostscript.path = PathBuf::from("/different/gs");
        let mut wrong_ghostscript_pin = config.clone();
        wrong_ghostscript_pin.startup.ghostscript.sha256[0] ^= 1;
        let before = engine.qualified_startup_status().unwrap();
        let pid = engine.process_id();
        for wrong in [
            wrong_profile,
            wrong_path,
            wrong_pin,
            wrong_ghostscript_path,
            wrong_ghostscript_pin,
        ] {
            assert!(wrong.require_active(&engine).is_err());
            assert!(wrong.restart(&mut engine).is_err());
            assert_eq!(engine.qualified_startup_status().unwrap(), before);
            assert_eq!(engine.process_id(), pid);
        }
        config.require_active(&engine).unwrap();
    }
}
