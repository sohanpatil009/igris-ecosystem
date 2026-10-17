// src/setup_manager/mod.rs - Setup system module exports
//
// The installer logic (downloads, extraction, validation, per-platform
// tooling, runtime permissions) lives in the igris-ai crate — the assistant
// carries its own setup. Only the dioxus rendering (progress GUI + the
// permissions panel) stays here, plus re-exports so every `setup_manager::`
// path in the app keeps resolving.

pub mod gui;
pub mod permissions_ui;

// Re-exports for convenience
pub use gui::{SetupUI, SetupGui, is_setup_complete};
pub use igris_ai::setup_manager::{
    SetupEvent, SetupManager, FileDownloader, FileExtractor, SetupValidator,
    PermissionsConfig, PlatformSetup, current_platform, command_exists,
    uninstall_igris, cleanup_temp_files, verify_installation,
};
pub use igris_ai::setup_manager::permissions;
