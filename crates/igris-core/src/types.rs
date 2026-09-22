//! Core identity + authorization types.
//!
//! Master Sec10 (risk) + Sec13 (capability tokens):
//! - [`RiskLevel`] ordering: SAFE < LOW < MEDIUM < HIGH < CRITICAL.
//! - [`CapabilityToken`] is deny-by-default: a token is valid only when
//!   ALL of scope / expiration / task-binding / device-binding /
//!   tool-binding / revocation checks pass. There are no wildcards in
//!   Phase 1 (TODO Phase 2: hierarchical scopes behind explicit grants).

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{ErrorKind, IgrisError};

/// Risk classification for a proposed action (master Sec10).
///
/// Least-privilege rule: anything at MEDIUM or above requires an
/// explicit approval path; HIGH/CRITICAL additionally require a
/// bound [`CapabilityToken`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RiskLevel {
    #[serde(rename = "SAFE")]
    Safe,
    #[serde(rename = "LOW")]
    Low,
    #[serde(rename = "MEDIUM")]
    Medium,
    #[serde(rename = "HIGH")]
    High,
    #[serde(rename = "CRITICAL")]
    Critical,
}

impl RiskLevel {
    /// Uppercase wire label (`SAFE`, `LOW`, ...).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "SAFE",
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        }
    }

    /// Whether the level needs an explicit user / policy approval.
    pub fn requires_approval(self) -> bool {
        self >= Self::Medium
    }

    /// Whether the level needs a bound capability token (not just approval).
    pub fn requires_capability_token(self) -> bool {
        self >= Self::High
    }

    /// Whether execution is allowed under deny-by-default without approval.
    pub fn is_auto_executable(self) -> bool {
        matches!(self, Self::Safe | Self::Low)
    }
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RiskLevel {
    type Err = IgrisError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "SAFE" => Ok(Self::Safe),
            "LOW" => Ok(Self::Low),
            "MEDIUM" => Ok(Self::Medium),
            "HIGH" => Ok(Self::High),
            "CRITICAL" => Ok(Self::Critical),
            other => Err(IgrisError::new(
                ErrorKind::Validation,
                format!("unknown RiskLevel: {other}"),
            )),
        }
    }
}

/// Unique device identity (UUID v4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceId(pub Uuid);

impl DeviceId {
    /// Generate a fresh random device id.
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wrap an existing UUID (e.g. loaded from trusted storage).
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// Borrow the inner UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for DeviceId {
    type Err = IgrisError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s.trim())
            .map(Self)
            .map_err(|e| IgrisError::validation(format!("invalid DeviceId: {e}")))
    }
}

/// Unique task identity (UUID v4). Tasks are the unit of cancellation
/// (see `runtime`: UI -> Task -> Agent -> Tool -> Device).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(pub Uuid);

impl TaskId {
    /// Generate a fresh random task id.
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wrap an existing UUID.
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// Borrow the inner UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TaskId {
    type Err = IgrisError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s.trim())
            .map(Self)
            .map_err(|e| IgrisError::validation(format!("invalid TaskId: {e}")))
    }
}

/// Tool identity (`name.version`-style string, e.g. `clipboard.push`).
///
/// Validated on construction: 1..128 chars, ASCII alphanumeric plus
/// `.` `-` `_` `/`, must not be empty or contain shell metacharacters.
/// This is part of "never expose unrestricted shell": tool names are an
/// allowlist key, not a command line.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolId(pub String);

impl ToolId {
    /// Validate and wrap a tool name.
    pub fn new(id: impl Into<String>) -> Result<Self, IgrisError> {
        let id = id.into();
        Self::validate_str(&id)?;
        Ok(Self(id))
    }

    /// Borrow the inner name.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate_str(id: &str) -> Result<(), IgrisError> {
        if id.is_empty() || id.len() > 128 {
            return Err(IgrisError::validation("ToolId must be 1..128 characters"));
        }
        let ok = id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '/'));
        if !ok {
            return Err(IgrisError::validation("ToolId allows only [A-Za-z0-9._-/]"));
        }
        Ok(())
    }
}

impl fmt::Display for ToolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ToolId {
    type Err = IgrisError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s.trim())
    }
}

/// Short-lived, least-privilege capability grant (master Sec13).
///
/// A token binds ALL of: scope list, expiration, task, device, tool.
/// Validation is deny-by-default: any mismatch / expiry / revocation
/// yields `E_FORBIDDEN` — never a silent pass.
///
/// Phase 1 stores no cryptographic signature (TODO Phase 2: Ed25519
/// sign + verify in a `license`/crypto module; until then tokens are
/// in-process only and must never cross trust boundaries).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityToken {
    /// Token identifier (for revocation lists / audit).
    pub token_id: Uuid,
    /// Granted scopes, e.g. `clipboard.read`. Exact match only in Phase 1.
    pub scope: Vec<String>,
    /// UTC expiry. Tokens must be short-lived (Phase 1 default: 15 min).
    pub expires_at: DateTime<Utc>,
    /// Task this token is bound to.
    pub task_id: TaskId,
    /// Device this token is bound to.
    pub device_id: DeviceId,
    /// Tool this token is bound to.
    pub tool_id: ToolId,
    /// Revocation flag. `true` = permanently invalid.
    pub revoked: bool,
    /// UTC issuance time (for audit).
    pub issued_at: DateTime<Utc>,
}

impl CapabilityToken {
    /// Issue a token. Rejects empty scopes and non-future expirations
    /// (deny-by-default at issuance).
    pub fn issue(
        scope: Vec<String>,
        expires_at: DateTime<Utc>,
        task_id: TaskId,
        device_id: DeviceId,
        tool_id: ToolId,
    ) -> Result<Self, IgrisError> {
        if scope.is_empty() {
            return Err(IgrisError::validation(
                "CapabilityToken requires at least one scope",
            ));
        }
        let now = Utc::now();
        if expires_at <= now {
            return Err(IgrisError::validation(
                "CapabilityToken expiration must be in the future",
            ));
        }
        for s in &scope {
            if s.trim().is_empty() || s.len() > 128 {
                return Err(IgrisError::validation(
                    "CapabilityToken scope entries must be 1..128 chars",
                ));
            }
        }
        Ok(Self {
            token_id: Uuid::new_v4(),
            scope,
            expires_at,
            task_id,
            device_id,
            tool_id,
            revoked: false,
            issued_at: now,
        })
    }

    /// Whether the token is past expiry (UTC).
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }

    /// Whether the token was revoked.
    pub fn is_revoked(&self) -> bool {
        self.revoked
    }

    /// Permanently revoke this token.
    pub fn revoke(&mut self) {
        self.revoked = true;
    }

    /// Whether `scope` is granted (exact match, Phase 1 — no wildcards).
    pub fn grants(&self, scope: &str) -> bool {
        self.scope.iter().any(|s| s == scope)
    }

    /// Full deny-by-default validation: revocation, expiration,
    /// task-binding, device-binding, tool-binding, then scope.
    /// Returns `Ok(())` only when every check passes.
    pub fn validate(
        &self,
        scope: &str,
        task_id: &TaskId,
        device_id: &DeviceId,
        tool_id: &ToolId,
    ) -> Result<(), IgrisError> {
        if self.is_revoked() {
            return Err(IgrisError::forbidden("capability token revoked"));
        }
        if self.is_expired() {
            return Err(IgrisError::forbidden("capability token expired"));
        }
        if &self.task_id != task_id {
            return Err(IgrisError::forbidden("capability token task mismatch"));
        }
        if &self.device_id != device_id {
            return Err(IgrisError::forbidden("capability token device mismatch"));
        }
        if &self.tool_id != tool_id {
            return Err(IgrisError::forbidden("capability token tool mismatch"));
        }
        if !self.grants(scope) {
            return Err(IgrisError::forbidden("capability token scope denied"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    fn fixture_token() -> CapabilityToken {
        CapabilityToken::issue(
            vec!["clipboard.read".to_string()],
            Utc::now() + TimeDelta::minutes(15),
            TaskId::generate(),
            DeviceId::generate(),
            ToolId::new("clipboard.pull").unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn risk_ordering_and_gates() {
        assert!(RiskLevel::Safe < RiskLevel::Critical);
        assert!(RiskLevel::Medium.requires_approval());
        assert!(!RiskLevel::Low.requires_approval());
        assert!(RiskLevel::High.requires_capability_token());
    }

    #[test]
    fn token_rejects_binding_mismatch() {
        let t = fixture_token();
        let other_task = TaskId::generate();
        let err = t
            .validate("clipboard.read", &other_task, &t.device_id, &t.tool_id)
            .unwrap_err();
        assert_eq!(err.error_code(), "E_FORBIDDEN");
    }

    #[test]
    fn token_revocation_denies() {
        let mut t = fixture_token();
        t.revoke();
        let err = t
            .validate("clipboard.read", &t.task_id, &t.device_id, &t.tool_id)
            .unwrap_err();
        assert_eq!(err.error_code(), "E_FORBIDDEN");
    }
}
