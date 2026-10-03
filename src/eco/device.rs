use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Instant;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeviceStatus {
    Online,
    Offline,
    Pairing,
    Trusted,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Capabilities {
    pub clipboard_sync: bool,
    pub notification_sync: bool,
    pub remote_commands: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            clipboard_sync: true,
            notification_sync: false,
            remote_commands: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EcoDevice {
    pub id: Uuid,
    pub name: String,
    pub platform: String,
    pub hostname: String,
    pub version: String,
    pub capabilities: Capabilities,
    pub status: DeviceStatus,
    pub public_key: Option<String>,
    pub addr: Option<SocketAddr>,
    #[serde(skip)]
    pub last_seen: Option<Instant>,
    /// Peer's self-reported list of trusted device IDs (from `/info`).
    /// `None` = peer is an old binary that doesn't report this field.
    /// `Some([])` = peer reports it has no trusted peers (stale trust).
    #[serde(default)]
    pub trusted_peers: Option<Vec<String>>,
}

impl EcoDevice {
    pub fn new(name: String) -> Self {
        Self::with_id(Uuid::new_v4(), name)
    }

    pub fn with_id(id: Uuid, name: String) -> Self {
        Self {
            id,
            name,
            platform: std::env::consts::OS.to_string(),
            hostname: whoami::fallible::hostname().unwrap_or_default(),
            version: crate::eco::constants::ECO_PROTOCOL_VERSION.to_string(),
            capabilities: Capabilities::default(),
            status: DeviceStatus::Online,
            public_key: None,
            addr: None,
            last_seen: Some(Instant::now()),
            trusted_peers: None,
        }
    }

    pub fn is_trusted(&self) -> bool {
        self.status == DeviceStatus::Trusted
    }

    pub fn is_online(&self) -> bool {
        matches!(self.status, DeviceStatus::Online | DeviceStatus::Trusted)
    }

    pub fn mark_offline(&mut self) {
        self.status = DeviceStatus::Offline;
    }

    pub fn mark_trusted(&mut self) {
        self.status = DeviceStatus::Trusted;
    }

    pub fn touch(&mut self) {
        self.last_seen = Some(Instant::now());
    }
}

/// replica fix6: stable identity that survives restarts AND updates.
/// Loads `pkg/device_id` (plain UUID text); creates it once if missing.
/// Never generates a fresh random ID per launch — that silently broke
/// every existing link until a manual re-pair.
pub fn load_or_create_persistent_id(pkg_dir: &std::path::Path) -> Uuid {
    let path = pkg_dir.join("device_id");
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(id) = text.trim().parse::<Uuid>() {
            return id;
        }
    }
    let id = Uuid::new_v4();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Best-effort persist; a missing file just means a new ID next launch.
    let _ = std::fs::write(&path, id.to_string());
    id
}
