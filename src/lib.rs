// src/lib.rs - IGRIS v3 library exports

use std::sync::{Arc, Mutex};

// Configuration system
pub mod config;

// UI components
pub mod ui;

// Command handlers
pub mod commands;

// Utility modules
pub mod utils;

// Platform abstraction
pub mod platform;
pub mod platform_utils;

// Media capture (FFmpeg camera) — device feature, not AI.
pub mod media;

// FastSwap file sharing integration — shared with the phone via igris-protocol.
pub use igris_protocol::fastswap;

// Desktop Ecosystem (cross-device clipboard, discovery, sync) — single source
// of truth in igris-protocol (phone + desktop consume the identical tree).
pub use igris_protocol::eco;

// Global state for search results UI (shared across modules)
#[derive(Clone, Debug, Default)]
pub struct SearchState {
    pub is_open: bool,
    pub is_searching: bool,
    pub query: String,
    pub results: Vec<SearchResultData>,
}

#[derive(Clone, Debug)]
pub struct SearchResultData {
    pub path: String,
    pub name: String,
    pub drive: String,
    pub score: u32,
    pub is_folder: bool,
}

pub static SEARCH_STATE: once_cell::sync::Lazy<Arc<Mutex<SearchState>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(SearchState::default())));

// Re-export camera panel state from commands module
pub use commands::ffmpeg_camera::{CameraPanelState, CAMERA_PANEL_STATE};

// Re-export commonly used types
pub use config::{CONFIG, AppConfig, Personality, Theme};
pub use commands::{system, files, web};
pub use ui::{SettingsPanel, SettingsButton};
