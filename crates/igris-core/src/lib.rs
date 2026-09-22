//! igris-core — Phase 1 foundation.
//!
//! Deny-by-default, least-privilege scaffolding for the IGRIS ecosystem.
//! This crate intentionally contains **no** eco / fastswap / voice / NLU /
//! reflex logic yet. Those arrive in Phase 2+ behind capability checks.
//!
//! Central-registry rule: all ports and filesystem paths live in
//! [`config`]. Do not hardcode ports or `pkg/...` paths elsewhere —
//! resolve them through [`config::AppConfig`].
//!
//! STT/TTS note: only the *path + threading concepts* from
//! `D:/ecosystem/jev/igrisv4/src/core/stt.rs` (SenseVoice model + tokens,
//! 16 kHz, 2 threads) and `tts.rs` (Piper exe + voice model +
//! espeak-ng-data + per-playback output stream + `TTS_PLAYING` suppression
//! flag) are reflected here as configuration. No recognizer / synthesizer
//! engines are constructed in Phase 1 (see TODOs in [`config`]).

pub mod config;
pub mod error;
pub mod events;
pub mod logging;
pub mod runtime;
pub mod types;

// Convenient re-exports for Phase 2 consumers.
pub use config::{AppConfig, EcoConfig, ModelPaths};
pub use error::{ErrorKind, IgrisError, Result, Severity};
pub use events::{EventKind, IgrisEvent};
pub use runtime::{CancellationNode, CancellationTier, IgrisRuntime};
pub use types::{CapabilityToken, DeviceId, RiskLevel, TaskId, ToolId};
