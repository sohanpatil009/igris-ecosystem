use crate::eco::constants::*;
use crate::eco::errors::{EcoError, EcoResult};
use crate::eco::events::{EcoEvent, EventBus};
use crate::eco::storage::{ClipboardEntry, EcoStorage};
use crate::platform::ecosystem::PlatformClipboard;
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ClipboardData {
    pub content: String,
    pub content_type: String,
    pub content_hash: String,
    pub source_device: String,
    pub timestamp: i64,
}

pub struct ClipboardManager {
    platform: Box<dyn PlatformClipboard>,
    event_bus: Arc<EventBus>,
    storage: Arc<std::sync::Mutex<EcoStorage>>,
    last_content_hash: Option<String>,
    last_applied_hash: Option<String>,
    /// Hash of the last local change that was actually delivered to at least
    /// one linked peer. Used to retry a push that failed while peers were
    /// offline instead of deduping it away forever.
    last_delivered_hash: Option<String>,
}

impl ClipboardManager {
    pub fn new(
        platform: Box<dyn PlatformClipboard>,
        event_bus: Arc<EventBus>,
        storage: Arc<std::sync::Mutex<EcoStorage>>,
    ) -> Self {
        Self {
            platform,
            event_bus,
            storage,
            last_content_hash: None,
            last_applied_hash: None,
            last_delivered_hash: None,
        }
    }

    pub async fn start_monitoring(manager: Arc<std::sync::Mutex<Self>>) {
        let mut interval = tokio::time::interval(
            std::time::Duration::from_secs(CLIPBOARD_POLL_INTERVAL_SECS)
        );
        loop {
            interval.tick().await;
            let event = {
                let mut guard = match manager.lock() {
                    Ok(g) => g,
                    Err(_) => continue,
                };
                let text = match guard.platform.get_text() {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                if text.is_empty() { continue; }
                let hash = hash_content(&text);

                let is_own_change = Some(&hash) == guard.last_applied_hash.as_ref();
                let is_known = Some(&hash) == guard.last_content_hash.as_ref();
                let is_delivered = guard.last_delivered_hash.as_ref() == Some(&hash);

                // Dedup: skip if the hash matches what we last saw locally
                // (`last_content_hash`) or what we applied from a peer
                // (`last_applied_hash`).  The old condition also required
                // `is_delivered`, which meant an offline peer caused every
                // poll to re-detect and re-store the same clip.
                if is_own_change || is_known {
                    continue;
                }

                println!(
                    "[ECO] Clipboard changed: hash={} own={} known={} delivered={}",
                    &hash[..16],
                    is_own_change,
                    is_known,
                    is_delivered
                );

                let data = ClipboardData {
                    content: text.clone(),
                    content_type: "text/plain".to_string(),
                    content_hash: hash.clone(),
                    source_device: String::new(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                };

                let entry = ClipboardEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    content: text,
                    content_type: "text/plain".to_string(),
                    source_device: String::new(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    content_hash: hash.clone(),
                };

                if let Ok(mut storage) = guard.storage.lock() {
                    let _ = storage.add_clipboard_entry(entry);
                }

                guard.last_content_hash = Some(hash);
                let event = EcoEvent::ClipboardChanged(Arc::new(data));
                event
            };
            let bus = manager.lock().ok().map(|g| g.event_bus.clone());
            if let Some(bus) = bus {
                bus.emit(event);
            }
        }
    }

    /// Check whether incoming clipboard data should be applied (dedup gate).
    /// Returns `true` if the content is new and should be written to the
    /// local clipboard. Caller must call `mark_applied()` after a successful
    /// write.
    pub fn should_apply(&self, data: &ClipboardData) -> bool {
        let result = self.last_content_hash.as_ref() != Some(&data.content_hash);
        println!("[ECO] should_apply: hash={} last_known={} result={}",
            &data.content_hash[..16],
            self.last_content_hash.as_ref().map(|h| &h[..16]).unwrap_or("none"),
            result);
        result
    }

    /// Write incoming clipboard to the local platform clipboard. This is a
    /// blocking I/O call (FFI into Android/iOS) and must NEVER be called
    /// while holding the Mutex.
    pub fn write_to_platform(&self, data: &ClipboardData) -> EcoResult<()> {
        println!("[ECO] write_to_platform: calling platform.set_text for hash={}", &data.content_hash[..16]);
        let result = self.platform.set_text(&data.content);
        println!("[ECO] write_to_platform: platform.set_text returned {:?}", result);
        result
    }

    /// Mark clipboard as applied (update dedup hashes, emit event). Must be
    /// called while holding the Mutex, but after `write_to_platform()`.
    pub fn mark_applied(&mut self, data: &ClipboardData) {
        self.last_applied_hash = Some(data.content_hash.clone());
        self.last_content_hash = Some(data.content_hash.clone());
        // Arrived via the mesh, so the mesh already has it — treat as delivered
        // so an identical local re-read isn't re-broadcast as "undelivered".
        self.last_delivered_hash = Some(data.content_hash.clone());

        let arc = Arc::new(data.clone());
        self.event_bus.emit(EcoEvent::ClipboardApplied(arc));
    }

    /// Record that a clipboard payload reached at least one linked peer, so an
    /// identical local change is deduped again (no repeat fan-out). Called by
    /// `SyncManager` after a successful fan-out; mirrors `igris-protocol`.
    pub fn mark_delivered(&mut self, hash: &str) {
        self.last_delivered_hash = Some(hash.to_string());
    }

    pub fn get_current_hash(&self) -> Option<String> {
        self.last_content_hash.clone()
    }
}

pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}
