//! Structured error model (master Sec30).
//!
//! Every error carries: `error_code`, `message`, `retryable`, `severity`,
//! plus optional `step_id` (which pipeline step failed) and `tool`
//! (which tool was executing, if any).
//!
//! Rules:
//! - Deny-by-default: unknown / unmapped failures become
//!   `ErrorKind::Internal`, never a silent `Ok`.
//! - No fake security: permission failures are explicit
//!   (`Unauthorized` / `Forbidden`), never downgraded to warnings.
//! - Callers needing pipeline correlation must set `step_id`; tool
//!   executors must set `tool`.

use serde::{Deserialize, Serialize};

/// How urgently an error needs operator attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}

impl Severity {
    /// Machine-readable label for logs / wire payloads.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Error => "ERROR",
            Self::Critical => "CRITICAL",
        }
    }
}

/// Coarse category for programmatic handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorKind {
    Config,
    Validation,
    Io,
    Network,
    Unauthorized,
    Forbidden,
    NotFound,
    Cancelled,
    Timeout,
    Device,
    Tool,
    Voice,
    Storage,
    Internal,
}

impl ErrorKind {
    /// Stable default `error_code` for this kind.
    pub fn default_code(self) -> &'static str {
        match self {
            Self::Config => "E_CONFIG",
            Self::Validation => "E_VALIDATION",
            Self::Io => "E_IO",
            Self::Network => "E_NETWORK",
            Self::Unauthorized => "E_UNAUTHORIZED",
            Self::Forbidden => "E_FORBIDDEN",
            Self::NotFound => "E_NOT_FOUND",
            Self::Cancelled => "E_CANCELLED",
            Self::Timeout => "E_TIMEOUT",
            Self::Device => "E_DEVICE",
            Self::Tool => "E_TOOL",
            Self::Voice => "E_VOICE",
            Self::Storage => "E_STORAGE",
            Self::Internal => "E_INTERNAL",
        }
    }

    /// Whether retrying *might* help (transient classes only).
    pub fn default_retryable(self) -> bool {
        matches!(
            self,
            Self::Network | Self::Timeout | Self::Io | Self::Storage
        )
    }

    /// Default severity per kind. Auth failures are always errors —
    /// never warnings (no fake security).
    pub fn default_severity(self) -> Severity {
        match self {
            Self::Unauthorized | Self::Forbidden | Self::Internal => Severity::Critical,
            Self::Config | Self::Device | Self::Tool | Self::Voice => Severity::Error,
            Self::Network | Self::Timeout | Self::Io | Self::Storage => Severity::Warning,
            Self::Validation | Self::NotFound | Self::Cancelled => Severity::Info,
        }
    }
}

/// Structured IGRIS error.
///
/// Field contract (Sec30): `error_code` / `message` / `retryable` /
/// `severity` / `step_id` / `tool`.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{error_code}: {message}")]
pub struct IgrisError {
    /// Coarse category.
    pub kind: ErrorKind,
    /// Stable machine-readable code (e.g. `E_VALIDATION`).
    pub error_code: String,
    /// Human-readable detail (must not contain secrets — see `logging`).
    pub message: String,
    /// Whether the operation may succeed on retry.
    pub retryable: bool,
    /// Operator severity.
    pub severity: Severity,
    /// Pipeline step that failed (e.g. `pair-verify`, `stt-decode`).
    pub step_id: Option<String>,
    /// Tool executing when the failure occurred (e.g. `clipboard.push`).
    pub tool: Option<String>,
}

impl IgrisError {
    /// Build a new error with kind defaults.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            kind,
            error_code: kind.default_code().to_string(),
            message,
            retryable: kind.default_retryable(),
            severity: kind.default_severity(),
            step_id: None,
            tool: None,
        }
    }

    /// Override the machine-readable code (must stay `E_*` shaped).
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.error_code = code.into();
        self
    }

    /// Attach the pipeline step id.
    pub fn with_step(mut self, step_id: impl Into<String>) -> Self {
        self.step_id = Some(step_id.into());
        self
    }

    /// Attach the executing tool id.
    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }

    /// Override retryability explicitly.
    pub fn with_retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    /// Override severity explicitly.
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    // -- Common constructors (least-privilege defaults) --

    pub fn config(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Config, message)
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Validation, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, message)
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unauthorized, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        // Deny-by-default: forbidden is never retryable, always critical.
        Self::new(ErrorKind::Forbidden, message)
            .with_retryable(false)
            .with_severity(Severity::Critical)
    }

    pub fn cancelled(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Cancelled, message).with_retryable(false)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Timeout, message)
    }

    pub fn device(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Device, message)
    }

    pub fn tool_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Tool, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, message).with_retryable(false)
    }

    // -- Sec30 field accessors --

    /// Stable machine-readable code.
    pub fn error_code(&self) -> &str {
        &self.error_code
    }

    /// Human-readable detail (secret-free by construction).
    pub fn error_message(&self) -> &str {
        &self.message
    }

    /// Whether retrying might help.
    pub fn is_retryable(&self) -> bool {
        self.retryable
    }

    /// Operator severity.
    pub fn error_severity(&self) -> Severity {
        self.severity
    }

    /// Pipeline step id, if attached.
    pub fn error_step_id(&self) -> Option<&str> {
        self.step_id.as_deref()
    }

    /// Executing tool id, if attached.
    pub fn error_tool(&self) -> Option<&str> {
        self.tool.as_deref()
    }
}

impl From<std::io::Error> for IgrisError {
    fn from(err: std::io::Error) -> Self {
        Self::new(ErrorKind::Io, err.to_string())
    }
}

impl From<serde_json::Error> for IgrisError {
    fn from(err: serde_json::Error) -> Self {
        Self::new(ErrorKind::Validation, format!("JSON schema error: {err}"))
    }
}

/// Fallible result with a structured [`IgrisError`].
pub type Result<T> = std::result::Result<T, IgrisError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_is_never_retryable_critical() {
        let e = IgrisError::forbidden("deny").with_tool("shell.exec");
        assert_eq!(e.error_code(), "E_FORBIDDEN");
        assert!(!e.is_retryable());
        assert_eq!(e.error_severity(), Severity::Critical);
        assert_eq!(e.error_tool(), Some("shell.exec"));
    }

    #[test]
    fn step_id_attaches() {
        let e = IgrisError::validation("bad").with_step("pair-verify");
        assert_eq!(e.error_step_id(), Some("pair-verify"));
    }
}
