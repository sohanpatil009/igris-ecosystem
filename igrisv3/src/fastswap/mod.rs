// FastSwap integration module for IGRIS
// LocalSend v2 protocol implementation

pub mod crypto;
pub mod models;
pub mod network;
pub mod tls;
pub mod token;

pub use models::*;
pub use network::*;

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use anyhow::Result;

// Global progress tracker for UI access
static GLOBAL_PROGRESS_TRACKER: once_cell::sync::Lazy<ProgressTracker> =
    once_cell::sync::Lazy::new(|| models::progress::create_progress_tracker());

// Global pending transfers (for receiver approval)
#[derive(Clone, Debug)]
pub struct PendingTransfer {
    pub session_id: String,
    pub sender_name: String,
    pub sender_device: String,
    pub file_count: usize,
    pub total_size: u64,
    pub files: Vec<String>,
}

static PENDING_TRANSFERS: once_cell::sync::Lazy<Arc<RwLock<Vec<PendingTransfer>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(Vec::new())));

static APPROVED_SESSIONS: once_cell::sync::Lazy<Arc<RwLock<Vec<String>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(Vec::new())));

/// Files available for download (registered by the user before sharing).
static DOWNLOAD_FILES: once_cell::sync::Lazy<Arc<RwLock<Vec<DownloadFileEntry>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(Vec::new())));

/// FastSwap manager for IGRIS integration
pub struct FastSwapManager {
    server_handle: Option<tokio::task::JoinHandle<()>>,
    port: u16,
}

impl FastSwapManager {
    pub fn new(port: u16) -> Self {
        Self {
            server_handle: None,
            port,
        }
    }
    
    pub async fn start(&mut self, local_device: Device) -> Result<()> {
        let port = self.port;
        let (actual_port, handle) = network::start_server(port, local_device).await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        self.port = actual_port;
        self.server_handle = Some(handle);
        
        tracing::info!("[FastSwap] Started on port {}", self.port);
        Ok(())
    }
    
    pub async fn stop(&mut self) {
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
        }
        tracing::info!("[FastSwap] Stopped");
    }
}

/// Get global progress tracker for UI access
pub fn get_progress_tracker() -> ProgressTracker {
    Arc::clone(&GLOBAL_PROGRESS_TRACKER)
}

/// Add pending transfer for approval
pub async fn add_pending_transfer(transfer: PendingTransfer) {
    let mut pending = PENDING_TRANSFERS.write().await;
    pending.push(transfer);
}

/// Get all pending transfers
pub async fn get_pending_transfers() -> Vec<PendingTransfer> {
    PENDING_TRANSFERS.read().await.clone()
}

/// Approve a transfer
pub async fn approve_transfer(session_id: &str) {
    let mut approved = APPROVED_SESSIONS.write().await;
    approved.push(session_id.to_string());
    
    // Remove from pending
    let mut pending = PENDING_TRANSFERS.write().await;
    pending.retain(|t| t.session_id != session_id);
}

/// Deny a transfer
pub async fn deny_transfer(session_id: &str) {
    // Just remove from pending
    let mut pending = PENDING_TRANSFERS.write().await;
    pending.retain(|t| t.session_id != session_id);
}

/// Check if transfer is approved
pub async fn is_transfer_approved(session_id: &str) -> bool {
    let approved = APPROVED_SESSIONS.read().await;
    approved.contains(&session_id.to_string())
}

/// Set files available for download (the sender's shared files).
pub async fn set_download_files(files: Vec<DownloadFileEntry>) {
    let mut store = DOWNLOAD_FILES.write().await;
    *store = files;
}

/// Get the current download file entries.
pub async fn get_download_files() -> Vec<DownloadFileEntry> {
    DOWNLOAD_FILES.read().await.clone()
}

/// Clear download files after a session completes.
pub async fn clear_download_files() {
    let mut store = DOWNLOAD_FILES.write().await;
    store.clear();
}

/// Convenience: register files by path for downloading.
/// Each file gets a UUID id and auto-detected MIME type.
pub async fn register_download_files(paths: Vec<PathBuf>) -> Result<()> {
    let mut entries = Vec::new();
    for path in &paths {
        let metadata = tokio::fs::metadata(path).await?;
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let sha256 = {
            let path = path.clone();
            tokio::task::spawn_blocking(move || -> Option<String> {
                use sha2::Digest;
                let mut file = std::fs::File::open(&path).ok()?;
                let mut hasher = sha2::Sha256::new();
                let mut buf = [0u8; 8192];
                loop {
                    use std::io::Read;
                    let n = file.read(&mut buf).ok()?;
                    if n == 0 { break; }
                    hasher.update(&buf[..n]);
                }
                Some(format!("{:x}", hasher.finalize()))
            }).await.unwrap_or(None)
        };
        entries.push(DownloadFileEntry {
            id: Uuid::new_v4().to_string(),
            name,
            path: path.clone(),
            size: metadata.len(),
            file_type: mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string(),
            sha256,
        });
    }
    set_download_files(entries).await;
    Ok(())
}

impl Drop for FastSwapManager {
    fn drop(&mut self) {
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
        }
    }
}
