// SPDX-License-Identifier: GPL-3.0-or-later
//! Ready identity validation for explicit qualified construction, and public
//! snapshots of the current accepted identity. Ordinary startup stays separate.

use super::{Error, Result, SessionContext};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// The admitted core startup profile. This profile version is independent of
/// the provider and adapter wire protocol versions reported in `intrinsics`.
pub const QUALIFIED_STARTUP_PROFILE: &str = "qualified-core-v2";

/// The optional admitted graphics extension of the qualified startup profile.
/// This name does not change provider or adapter wire protocol versions.
pub const QUALIFIED_GRAPHICS_STARTUP_PROFILE: &str = "qualified-graphics-v2";

/// A snapshot of a live qualified session's accepted Ready identity.
///
/// Obtain this from `Engine::qualified_startup_status`; selecting a startup
/// configuration alone does not establish accepted readiness. The snapshot
/// records acceptance for its context, rather than continuously attesting files
/// or promising that later recovery and restart will succeed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QualifiedStartupStatus {
    pub profile: &'static str,
    pub context: SessionContext,
    pub intrinsics: IntrinsicsReady,
}

/// The explicit qualified constructor creates this from its pre-spawn identity
/// record, never from worker JSON. Keep it selected across recovery and restart.
#[derive(Debug, Clone)]
pub(super) struct QualifiedReadyProfile {
    pub(super) expected: IntrinsicsReady,
}

/// Exact private `intrinsic_status` payload. These versions are STRING values;
/// the ordinary worker protocol retains its existing numeric version/limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntrinsicsReady {
    pub protocol: String,
    pub version: String,
    pub phase: String,
    pub provider_file: String,
    pub worker_file: String,
    pub registry_file: String,
    pub entry_alias: String,
    pub octave_version: String,
    pub octave_api_version: String,
    pub initial_path_admitted: bool,
    pub shutdown_alias: String,
    pub shutdown_registered: bool,
    pub adapters: BTreeMap<String, AdapterReady>,
}

/// The exact admitted identity of one native adapter role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterReady {
    pub path: String,
    pub protocol: String,
    pub version: String,
}

impl QualifiedReadyProfile {
    /// The caller must already have established a current accepted Ready context
    /// and live transport; a configured expected identity is not acceptance.
    pub(super) fn accepted_status(&self, context: SessionContext) -> QualifiedStartupStatus {
        QualifiedStartupStatus {
            profile: if self.expected.adapters.contains_key("graphics") {
                QUALIFIED_GRAPHICS_STARTUP_PROFILE
            } else {
                QUALIFIED_STARTUP_PROFILE
            },
            context,
            intrinsics: self.expected.clone(),
        }
    }

    pub(super) fn validate(&self, ready: &Value, octave_version: &str) -> Result<()> {
        self.expected.validate_shape()?;
        let status: IntrinsicsReady = serde_json::from_value(ready["intrinsics"].clone())?;
        status.validate_shape()?;
        if status != self.expected || octave_version != self.expected.octave_version {
            return Err(Error::Protocol(
                "Qualified Ready does not match the selected runtime/provider profile".into(),
            ));
        }
        Ok(())
    }
}

impl IntrinsicsReady {
    fn validate_shape(&self) -> Result<()> {
        if self.protocol != "mathlab-intrinsics"
            || self.version != "1"
            || self.phase != "ready"
            || !self.initial_path_admitted
            || !self.shutdown_registered
        {
            return Err(Error::Protocol(
                "Qualified Ready omitted complete provider admission/shutdown readiness".into(),
            ));
        }
        for path in [&self.provider_file, &self.worker_file, &self.registry_file] {
            validate_path(path)?;
        }
        for text in [&self.entry_alias, &self.shutdown_alias] {
            validate_text(text, 128)?;
        }
        if !self.entry_alias.starts_with("__mlentry_")
            || !self.shutdown_alias.starts_with("__mlshutdown_")
            || !self
                .entry_alias
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            || !self
                .shutdown_alias
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(Error::Protocol(
                "Invalid qualified provider alias identity".into(),
            ));
        }
        validate_text(&self.octave_version, 128)?;
        validate_text(&self.octave_api_version, 128)?;
        if self.adapters.len() > 5 {
            return Err(Error::Protocol("Too many qualified adapter roles".into()));
        }
        for (role, adapter) in &self.adapters {
            let (protocol, version) = match role.as_str() {
                "input" => ("mathlab-input", "1"),
                "scope" => ("mathlab-scope", "2"),
                "value" => ("mathlab-value", "1"),
                "debug" => ("mathlab-debugger", "4"),
                "graphics" => ("mathlab-graphics", "1"),
                _ => return Err(Error::Protocol("Unknown qualified adapter role".into())),
            };
            if adapter.protocol != protocol || adapter.version != version {
                return Err(Error::Protocol(
                    "Wrong qualified adapter protocol identity".into(),
                ));
            }
            validate_path(&adapter.path)?;
        }
        Ok(())
    }
}

pub(super) fn validate_path(text: &str) -> Result<()> {
    validate_text(text, 4096)?;
    if !Path::new(text).is_absolute() {
        return Err(Error::Protocol(
            "Qualified Ready path must be absolute".into(),
        ));
    }
    Ok(())
}

fn validate_text(text: &str, maximum: usize) -> Result<()> {
    if text.is_empty()
        || text.len() > maximum
        || text.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n'))
    {
        return Err(Error::Protocol(
            "Invalid bounded qualified Ready identity".into(),
        ));
    }
    Ok(())
}
