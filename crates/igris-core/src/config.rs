//! Central configuration registry (Phase 1).
//!
//! CENTRAL-REGISTRY RULE: every port number and every `pkg/...` filesystem
//! path in the product must originate here. Do not hardcode ports or model
//! paths in eco / fastswap / voice / UI code — resolve them through
//! [`AppConfig`].
//!
//! Port map (must stay stable for discovery):
//! - Eco HTTP `53327`, Eco TLS `53328` (clipboard / presence / pairing).
//! - FastSwap HTTP `53317`, FastSwap TLS `53318` (file transfer).
//!
//! Model-path concepts reused ONLY from
//! `D:/ecosystem/jev/igrisv4/src/core/stt.rs` (SenseVoice `model.onnx` +
//! `tokens.txt`, 16 kHz, 2 threads) and `tts.rs` (Piper exe + voice model
//! + espeak-ng-data + audio dir + per-playback output stream). No
//! recognizer / synthesizer / NLU / orchestration code is copied.
//!
//! TODO Phase 2:
//! - `voice` crate: construct SenseVoice recognizer from
//!   `ModelPaths::stt_model/tokens` on a dedicated blocking thread pool
//!   (sherpa-onnx recognizer is `!Send`; never share across threads).
//! - `voice` crate: Piper subprocess / FFI synthesis from
//!   `piper_exe/tts_model/espeak_data_dir`, one `OutputStream` per
//!   playback, `TTS_PLAYING`-style mic suppression.
//! - Persist `AppConfig` to `pkg/config.json` with JSON-schema validation.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{ErrorKind, IgrisError, Result};

// ---------------------------------------------------------------------------
// Port + audio constants (single source of truth)
// ---------------------------------------------------------------------------

/// Eco (presence / clipboard / pairing) plain HTTP port.
pub const ECO_HTTP_PORT: u16 = 53327;
/// Eco TLS port (wraps the HTTP server).
pub const ECO_TLS_PORT: u16 = 53328;
/// FastSwap (file transfer) plain HTTP port.
pub const FASTSWAP_HTTP_PORT: u16 = 53317;
/// FastSwap TLS port.
pub const FASTSWAP_TLS_PORT: u16 = 53318;

/// STT sample rate concept from `jev/.../stt.rs` (SenseVoice, 16 kHz).
pub const STT_SAMPLE_RATE_HZ: u32 = 16_000;
/// STT thread count concept from `jev/.../stt.rs`.
pub const STT_NUM_THREADS: usize = 2;
/// STT language concept from `jev/.../stt.rs` (`auto`).
pub const STT_LANGUAGE: &str = "auto";

// ---------------------------------------------------------------------------
// EcoConfig
// ---------------------------------------------------------------------------

/// Network surface for eco + fastswap. Phase 1 only *describes* the ports;
/// nothing here binds a socket (never bind ports from this crate's tests).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EcoConfig {
    /// Eco HTTP port (default 53327).
    pub http_port: u16,
    /// Eco TLS port (default 53328).
    pub tls_port: u16,
    /// FastSwap HTTP port (default 53317).
    pub fastswap_http_port: u16,
    /// FastSwap TLS port (default 53318).
    pub fastswap_tls_port: u16,
    /// Whether subnet discovery may run (Phase 2+; Phase 1 never scans).
    pub auto_discovery: bool,
}

impl Default for EcoConfig {
    fn default() -> Self {
        Self {
            http_port: ECO_HTTP_PORT,
            tls_port: ECO_TLS_PORT,
            fastswap_http_port: FASTSWAP_HTTP_PORT,
            fastswap_tls_port: FASTSWAP_TLS_PORT,
            auto_discovery: false,
        }
    }
}

impl EcoConfig {
    /// Validate port shape: non-zero, non-privileged, no collisions.
    pub fn validate(&self) -> Result<()> {
        for (name, port) in [
            ("http_port", self.http_port),
            ("tls_port", self.tls_port),
            ("fastswap_http_port", self.fastswap_http_port),
            ("fastswap_tls_port", self.fastswap_tls_port),
        ] {
            if port == 0 {
                return Err(IgrisError::validation(format!(
                    "EcoConfig.{name} must be non-zero"
                )));
            }
            if port < 1024 {
                return Err(IgrisError::validation(format!(
                    "EcoConfig.{name}={port} is privileged (<1024); least-privilege forbids it"
                )));
            }
        }
        if self.http_port == self.tls_port || self.fastswap_http_port == self.fastswap_tls_port {
            return Err(IgrisError::validation("HTTP and TLS ports must differ"));
        }
        if self.http_port == self.fastswap_http_port || self.tls_port == self.fastswap_tls_port {
            return Err(IgrisError::validation(
                "eco and fastswap ports must not collide",
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ModelPaths
// ---------------------------------------------------------------------------

/// All model / audio / executable paths. Relative paths resolve against
/// the process working directory (or the bundled `exe_dir` in Phase 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPaths {
    /// Base models dir (`pkg/models`).
    pub base_dir: PathBuf,
    /// Laya LLM dir (`pkg/models/laya`). TODO Phase 2: confirm layout.
    pub laya_dir: PathBuf,
    /// Qwen LLM path (`pkg/models/qwen`). TODO Phase 2: confirm filename.
    pub qwen_path: PathBuf,
    /// SenseVoice ONNX model (`pkg/models/sense-voice/model.onnx`).
    pub stt_model: PathBuf,
    /// SenseVoice tokens (`pkg/models/sense-voice/tokens.txt`).
    pub stt_tokens: PathBuf,
    /// Legacy whisper path, kept only so old configs migrate instead of
    /// silently breaking. `None` on fresh installs.
    pub whisper_legacy_model: Option<PathBuf>,
    /// Piper voice model (`pkg/models/bold_voice/en_US-libritts_r-medium.onnx`).
    pub tts_model: PathBuf,
    /// Piper voice config (`<tts_model>.json`).
    pub tts_config: PathBuf,
    /// Piper executable (`pkg/piper/piper.exe`).
    pub piper_exe: PathBuf,
    /// espeak-ng-data dir (`pkg/piper/espeak-ng-data`).
    pub espeak_data_dir: PathBuf,
    /// Synthesized-audio dir (`pkg/audio`).
    pub audio_dir: PathBuf,
}

impl Default for ModelPaths {
    fn default() -> Self {
        let base_dir = PathBuf::from("pkg/models");
        let stt_dir = base_dir.join("sense-voice");
        let tts_model = base_dir.join("bold_voice/en_US-libritts_r-medium.onnx");
        Self {
            base_dir: base_dir.clone(),
            laya_dir: base_dir.join("laya"),
            qwen_path: base_dir.join("qwen"),
            stt_model: stt_dir.join("model.onnx"),
            stt_tokens: stt_dir.join("tokens.txt"),
            whisper_legacy_model: None,
            tts_config: tts_model.with_extension("onnx.json"),
            tts_model,
            piper_exe: PathBuf::from("pkg/piper/piper.exe"),
            espeak_data_dir: PathBuf::from("pkg/piper/espeak-ng-data"),
            audio_dir: PathBuf::from("pkg/audio"),
        }
    }
}

impl ModelPaths {
    /// Validate path *shape* (JSON-schema style). Does NOT require files
    /// to exist — models may not be downloaded yet. Use
    /// [`Self::missing_files`] for existence probing.
    pub fn validate(&self) -> Result<()> {
        for (name, path) in [
            ("base_dir", self.base_dir.as_path()),
            ("laya_dir", self.laya_dir.as_path()),
            ("qwen_path", self.qwen_path.as_path()),
            ("stt_model", self.stt_model.as_path()),
            ("stt_tokens", self.stt_tokens.as_path()),
            ("tts_model", self.tts_model.as_path()),
            ("tts_config", self.tts_config.as_path()),
            ("piper_exe", self.piper_exe.as_path()),
            ("espeak_data_dir", self.espeak_data_dir.as_path()),
            ("audio_dir", self.audio_dir.as_path()),
        ] {
            if path.as_os_str().is_empty() {
                return Err(IgrisError::validation(format!(
                    "ModelPaths.{name} must not be empty"
                )));
            }
            if path.is_absolute() {
                return Err(IgrisError::validation(format!(
                    "ModelPaths.{name} must be relative (central registry resolves exe_dir at runtime)"
                )));
            }
        }
        // Legacy whisper mapping: whisper -> sense-voice. If a legacy path
        // is present it must point at the old whisper location, never at
        // the live sense-voice model (prevents alias confusion).
        if let Some(legacy) = &self.whisper_legacy_model {
            if legacy == &self.stt_model {
                return Err(IgrisError::validation(
                    "whisper_legacy_model must not alias stt_model",
                ));
            }
        }
        // STT threading concept guard (from jev stt.rs: exactly 2 threads).
        if STT_NUM_THREADS == 0 {
            return Err(IgrisError::new(
                ErrorKind::Internal,
                "STT_NUM_THREADS must be non-zero",
            ));
        }
        Ok(())
    }

    /// List configured files that are absent from disk. Empty = ready.
    /// Callers must treat missing models as "not ready", never fall back
    /// to an untrusted path (no fake readiness).
    pub fn missing_files(&self) -> Vec<PathBuf> {
        [
            self.stt_model.clone(),
            self.stt_tokens.clone(),
            self.tts_model.clone(),
            self.piper_exe.clone(),
        ]
        .into_iter()
        .filter(|p| !p.exists())
        .collect()
    }
}

// ---------------------------------------------------------------------------
// AppConfig
// ---------------------------------------------------------------------------

/// Top-level Phase 1 configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    /// Config schema version (`"1"` for Phase 1).
    pub version: String,
    /// Eco / fastswap network surface.
    pub eco: EcoConfig,
    /// Model / audio / executable paths.
    pub models: ModelPaths,
    /// Data dir for persistent state (`pkg/ecosystem`).
    /// Never points at `pkg/device_id`, `pkg/ecosystem/*.json`, or certs
    /// directly — those are managed by their Phase 2 owners.
    pub data_dir: PathBuf,
    /// Default log level (`info`, `debug`, ...).
    pub log_level: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: "1".to_string(),
            eco: EcoConfig::default(),
            models: ModelPaths::default(),
            data_dir: PathBuf::from("pkg/ecosystem"),
            log_level: "info".to_string(),
        }
    }
}

impl AppConfig {
    /// Validate the full schema (ports + paths + version + log level).
    pub fn validate(&self) -> Result<()> {
        if self.version.trim().is_empty() {
            return Err(IgrisError::validation("AppConfig.version is required"));
        }
        self.eco.validate()?;
        self.models.validate()?;
        if self.data_dir.as_os_str().is_empty() {
            return Err(IgrisError::validation(
                "AppConfig.data_dir must not be empty",
            ));
        }
        if self.data_dir.is_absolute() {
            return Err(IgrisError::validation(
                "AppConfig.data_dir must be relative",
            ));
        }
        match self.log_level.to_ascii_lowercase().as_str() {
            "trace" | "debug" | "info" | "warn" | "warning" | "error" => Ok(()),
            other => Err(IgrisError::validation(format!(
                "unknown log_level: {other}"
            ))),
        }
    }

    /// Parse + validate from a JSON string (schema-validated load).
    pub fn from_json(json: &str) -> Result<Self> {
        let cfg: Self = serde_json::from_str(json)?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// Serialize to pretty JSON.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| IgrisError::validation(format!("config encode: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ports_match_registry() {
        let eco = EcoConfig::default();
        assert_eq!(eco.http_port, 53327);
        assert_eq!(eco.tls_port, 53328);
        assert_eq!(eco.fastswap_http_port, 53317);
        assert_eq!(eco.fastswap_tls_port, 53318);
        eco.validate().unwrap();
    }

    #[test]
    fn default_models_point_at_sense_voice_and_piper() {
        let m = ModelPaths::default();
        assert!(m.stt_model.ends_with("sense-voice/model.onnx"));
        assert!(m.stt_tokens.ends_with("sense-voice/tokens.txt"));
        assert_eq!(STT_SAMPLE_RATE_HZ, 16_000);
        assert_eq!(STT_NUM_THREADS, 2);
        assert_eq!(STT_LANGUAGE, "auto");
        m.validate().unwrap();
    }

    #[test]
    fn app_config_json_roundtrip() {
        let cfg = AppConfig::default();
        cfg.validate().unwrap();
        let json = cfg.to_json().unwrap();
        let back = AppConfig::from_json(&json).unwrap();
        assert_eq!(cfg, back);
    }
}
