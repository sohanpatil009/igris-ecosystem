// src/core/mod.rs - Core voice processing modules

pub mod stt;
pub mod tts;
pub mod vad;
pub mod wake_word;
pub mod audio_capture;
pub mod about;
#[cfg(all(target_os = "windows", piper_ffi))]
pub mod piper_ffi;

// Re-exports for convenience
pub use stt::{init_stt_engine, SttEngine, transcribe_audio, hybrid_transcribe_audio};
pub use tts::{speak, speak_compat, TTS_ENGINE};
pub use audio_capture::{capture_audio_vad, CaptureConfig, CaptureResult, CaptureMode};
pub use wake_word::{listen_for_wake_word, listen_for_wake_word_async};
pub use about::{IgrisAbout, AboutSection, is_about_query, wants_detailed_info};
