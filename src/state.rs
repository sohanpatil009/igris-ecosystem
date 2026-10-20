use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;

pub(crate) static MESH_STATUS: Lazy<Arc<Mutex<String>>> =
    Lazy::new(|| Arc::new(Mutex::new("Mesh starting...".to_string())));

pub(crate) static UI_PANEL_STATE: Lazy<Arc<Mutex<UiPanelState>>> =
    Lazy::new(|| Arc::new(Mutex::new(UiPanelState::default())));

#[derive(Default)]
pub(crate) struct UiPanelState {
    pub(crate) show_fastswap: bool,
}

pub(crate) fn update_status(status: &str) {
    if let Ok(mut s) = MESH_STATUS.lock() {
        *s = status.to_string();
    }
}
