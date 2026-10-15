//! Ordinary worker capabilities and conservative workspace-binding snapshots.
//!
//! A token identifies a binding in a particular observed workspace revision. It
//! is not a physical-value identity or a replacement for the independent input
//! and debugger protocols. Every counter is a decimal string on the wire so an
//! Octave JSON number cannot round an otherwise valid identity.
// SPDX-License-Identifier: GPL-3.0-or-later

use serde::{Deserialize, Serialize};

use super::{Error, Result, Variable, VariableSlice};

pub const PROTOCOL_NAME: &str = "mathlab-worker";
pub const PROTOCOL_VERSION: u32 = 1;
pub const CAP_BINDING_SNAPSHOT: &str = "binding_snapshot.v1";
pub const CAP_CHECKED_SCALAR_WRITE: &str = "checked_scalar_write.v1";
pub const CAP_DISPLAY_PAGE: &str = "display_page.v1";
pub const REQUIRED_CAPABILITIES: [&str; 3] = [
    CAP_BINDING_SNAPSHOT,
    CAP_CHECKED_SCALAR_WRITE,
    CAP_DISPLAY_PAGE,
];

const VARIABLE_CELLS: usize = 2000;
const RESPONSE_BYTES: usize = 32 * 1024 * 1024;

/// The ordinary worker contract, separate from optional native adapter gates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerCapabilities {
    pub name: String,
    pub version: u32,
    pub capabilities: Vec<String>,
    pub limits: WorkerLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerLimits {
    pub variable_cells: usize,
    pub response_bytes: usize,
}

impl WorkerCapabilities {
    /// Validate the exact contract implemented by this client. Additional
    /// advertised capabilities are allowed; they do not imply client support.
    pub fn validate(&self) -> Result<()> {
        if self.name != PROTOCOL_NAME {
            return Err(Error::Protocol(format!(
                "Expected worker protocol {PROTOCOL_NAME:?}, received {:?}.",
                self.name
            )));
        }
        if self.version != PROTOCOL_VERSION {
            return Err(Error::Protocol(format!(
                "Expected worker protocol version {PROTOCOL_VERSION}, received {}.",
                self.version
            )));
        }
        for capability in REQUIRED_CAPABILITIES {
            self.require(capability)?;
        }
        if self.limits.variable_cells != VARIABLE_CELLS
            || self.limits.response_bytes != RESPONSE_BYTES
        {
            return Err(Error::Protocol(format!(
                "Worker limits must be {VARIABLE_CELLS} variable cells and \
                 {RESPONSE_BYTES} response bytes; received {} cells and {} bytes.",
                self.limits.variable_cells, self.limits.response_bytes
            )));
        }
        Ok(())
    }

    /// Whether the worker advertises this exact capability identifier.
    pub fn supports(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|value| value == capability)
    }

    /// Reject a dependent operation before dispatch when its gate is absent.
    /// The session owner must also validate the enclosing protocol at startup.
    pub fn require(&self, capability: &str) -> Result<()> {
        if self.supports(capability) {
            Ok(())
        } else {
            Err(Error::Protocol(format!(
                "Worker does not support required capability {capability:?}."
            )))
        }
    }
}

/// A settled workspace context. New workers, recovery invocations and potential
/// mutations revoke older contexts even when names, classes and shapes match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionContext {
    pub worker_id: String,
    #[serde(with = "decimal_u64")]
    pub generation: u64,
    #[serde(with = "decimal_u64")]
    pub invocation: u64,
    #[serde(with = "decimal_u64")]
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingToken {
    pub context: SessionContext,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundVariable {
    pub token: BindingToken,
    pub variable: Variable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub context: SessionContext,
    pub variables: Vec<BoundVariable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundVariableSlice {
    pub token: BindingToken,
    pub slice: VariableSlice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationReceipt {
    pub token: BindingToken,
    pub workspace: Vec<Variable>,
}

mod decimal_u64 {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        if text.is_empty()
            || !text.bytes().all(|byte| byte.is_ascii_digit())
            || (text.len() > 1 && text.starts_with('0'))
        {
            return Err(D::Error::custom(
                "Expected a canonical unsigned decimal string without leading zeroes",
            ));
        }
        text.parse().map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn capabilities() -> WorkerCapabilities {
        WorkerCapabilities {
            name: PROTOCOL_NAME.into(),
            version: PROTOCOL_VERSION,
            capabilities: REQUIRED_CAPABILITIES
                .iter()
                .map(|value| (*value).into())
                .collect(),
            limits: WorkerLimits {
                variable_cells: VARIABLE_CELLS,
                response_bytes: RESPONSE_BYTES,
            },
        }
    }

    #[test]
    fn context_round_trips_large_counters_as_exact_decimal_strings() {
        let context = SessionContext {
            worker_id: "nonce-worker-a".into(),
            generation: 9_007_199_254_741_009,
            invocation: u64::MAX,
            revision: 0,
        };
        let wire = json!({
            "worker_id": "nonce-worker-a",
            "generation": "9007199254741009",
            "invocation": "18446744073709551615",
            "revision": "0",
        });
        assert_eq!(serde_json::to_value(&context).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<SessionContext>(wire).unwrap(),
            context
        );
    }

    #[test]
    fn every_counter_rejects_noncanonical_strings_and_nonstring_json() {
        let invalid = [
            json!(""),
            json!("00"),
            json!("01"),
            json!("+1"),
            json!("-1"),
            json!(" 1"),
            json!("1 "),
            json!("1\n"),
            json!("1.0"),
            json!("1e3"),
            json!("١"),
            json!("18446744073709551616"),
            json!(0),
            json!(9_007_199_254_741_009_u64),
            json!(1.0),
            json!(true),
            Value::Null,
            json!([]),
            json!({}),
        ];
        for field in ["generation", "invocation", "revision"] {
            for value in &invalid {
                let mut wire = json!({
                    "worker_id": "worker-a",
                    "generation": "1",
                    "invocation": "1",
                    "revision": "0",
                });
                wire[field] = value.clone();
                assert!(
                    serde_json::from_value::<SessionContext>(wire).is_err(),
                    "accepted {field}={value}"
                );
            }
        }
    }

    #[test]
    fn capability_validation_accepts_the_contract_with_optional_extensions() {
        let mut value = capabilities();
        value.capabilities.push("future_optional.v2".into());
        value.validate().unwrap();
        assert!(value.supports(CAP_BINDING_SNAPSHOT));
        assert!(!value.supports("binding_snapshot.v2"));
        value.require(CAP_CHECKED_SCALAR_WRITE).unwrap();
        assert!(matches!(
            value.require("typed_nd_page.v1"),
            Err(Error::Protocol(message)) if message.contains("typed_nd_page.v1")
        ));
    }

    #[test]
    fn capability_validation_rejects_wrong_protocol_name_or_version() {
        let mut value = capabilities();
        value.name = "mathlab-debugger".into();
        assert!(matches!(value.validate(), Err(Error::Protocol(_))));
        for version in [0, PROTOCOL_VERSION + 1, u32::MAX] {
            let mut value = capabilities();
            value.version = version;
            assert!(matches!(value.validate(), Err(Error::Protocol(_))));
        }
    }

    #[test]
    fn capability_validation_requires_each_exact_capability() {
        for required in REQUIRED_CAPABILITIES {
            let mut value = capabilities();
            value.capabilities.retain(|item| item != required);
            value.capabilities.push(format!("{required}.other"));
            assert!(matches!(
                value.validate(),
                Err(Error::Protocol(message)) if message.contains(required)
            ));
        }
    }

    #[test]
    fn capability_validation_rejects_smaller_or_larger_limits() {
        for cells in [0, VARIABLE_CELLS - 1, VARIABLE_CELLS + 1, usize::MAX] {
            let mut value = capabilities();
            value.limits.variable_cells = cells;
            assert!(matches!(value.validate(), Err(Error::Protocol(_))));
        }
        for bytes in [0, RESPONSE_BYTES - 1, RESPONSE_BYTES + 1, usize::MAX] {
            let mut value = capabilities();
            value.limits.response_bytes = bytes;
            assert!(matches!(value.validate(), Err(Error::Protocol(_))));
        }
    }
}
