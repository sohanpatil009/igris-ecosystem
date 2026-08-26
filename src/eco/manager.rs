use crate::eco::clipboard::{ClipboardData, ClipboardManager};
use crate::eco::config::EcosystemConfig;
use crate::eco::constants::*;
use crate::eco::crypto::EcoCrypto;
use crate::eco::device::{Capabilities, EcoDevice};
use crate::eco::discovery::DeviceDiscovery;
use crate::eco::errors::{EcoError, EcoResult};
use crate::eco::events::{EcoEvent, EventBus};
use crate::eco::notification::{NotificationManager, NOTIFICATION_HISTORY};
use crate::eco::permissions::EcoPermissions;
use crate::eco::storage::EcoStorage;
use crate::eco::sync::SyncManager;
use crate::eco::transport::EcoTransport;
use crate::platform::ecosystem::create_platform_clipboard;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct EcoManager {
    config: EcosystemConfig,
    event_bus: Arc<EventBus>,
    transport: Arc<EcoTransport>,
    local_device: Arc<RwLock<EcoDevice>>,
    discovery: Option<Arc<DeviceDiscovery>>,
    clipboard: Option<Arc<std::sync::Mutex<ClipboardManager>>>,
    notification: Option<Arc<std::sync::Mutex<NotificationManager>>>,
    storage: Arc<std::sync::Mutex<EcoStorage>>,
    permissions: Option<Arc<EcoPermissions>>,
    crypto: Option<EcoCrypto>,
    sync: Option<Arc<SyncManager>>,
    eco_port: u16,
    initialized: bool,
    running: bool,
}

impl EcoManager {
    pub fn new(pkg_dir: &PathBuf) -> Self {
        let config_path = pkg_dir.join(ECO_STORAGE_DIR).join(ECO_CONFIG_FILE);
        let config = EcosystemConfig::from_file(&config_path).unwrap_or_default();

        let event_bus = Arc::new(EventBus::new());
        let transport = Arc::new(EcoTransport::new());

        let device_name = config.device_name.clone();
        let local_device = Arc::new(RwLock::new(EcoDevice::new(device_name)));

        let storage = Arc::new(std::sync::Mutex::new(EcoStorage::new(pkg_dir)));

        Self {
            config,
            event_bus,
            transport,
            local_device,
            discovery: None,
            clipboard: None,
            notification: None,
            storage,
            permissions: None,
            crypto: None,
            sync: None,
            eco_port: DEFAULT_ECO_PORT,
            initialized: false,
            running: false,
        }
    }

    pub async fn initialize(&mut self, pkg_dir: &PathBuf) -> EcoResult<()> {
        if self.initialized {
            return Ok(());
        }

        let storage = EcoStorage::new(pkg_dir);
        storage.init_dirs()?;
        self.storage = Arc::new(std::sync::Mutex::new(storage));

        let crypto = EcoCrypto::new(pkg_dir)?;
        let public_key_pem = crypto.public_key_pem();

        {
            let mut device = self.local_device.write().await;
            device.public_key = Some(public_key_pem);
            device.capabilities = Capabilities {
                clipboard_sync: self.config.clipboard_sync,
                notification_sync: self.config.notification_sync,
                ..Default::default()
            };
            // Stable identity across restarts. Peers key their trust store by
            // device id; a fresh random UUID per launch silently broke every
            // existing link until someone re-paired manually.
            let id_file = pkg_dir.join("device_id");
            match std::fs::read_to_string(&id_file)
                .ok()
                .and_then(|s| uuid::Uuid::parse_str(s.trim()).ok())
            {
                Some(id) => device.id = id,
                None => {
                    if let Err(e) = std::fs::write(&id_file, device.id.to_string()) {
                        tracing::warn!("[ECO] could not persist device id: {}", e);
                    }
                }
            }
        }

        let permissions = EcoPermissions::new(
            EcoStorage::new(pkg_dir)
        );
        self.permissions = Some(Arc::new(permissions));
        self.crypto = Some(crypto);

        let clipboard_platform = create_platform_clipboard();
        let clipboard = ClipboardManager::new(
            clipboard_platform,
            self.event_bus.clone(),
            self.storage.clone(),
        );
        self.clipboard = Some(Arc::new(std::sync::Mutex::new(clipboard)));

        // Initialize notification manager
        let (device_id, device_name) = {
            let device = self.local_device.read().await;
            (device.id.to_string(), device.name.clone())
        };
        let notification = NotificationManager::new(
            self.event_bus.clone(),
            pkg_dir,
            device_id,
            device_name,
        );
        self.notification = Some(Arc::new(std::sync::Mutex::new(notification)));

        self.initialized = true;
        self.running = false;

        self.event_bus.emit(EcoEvent::Synced("Ecosystem initialized".to_string()));

        Ok(())
    }

    pub async fn start(&mut self) -> EcoResult<()> {
        if !self.initialized {
            return Err(EcoError::NotInitialized);
        }
        if self.running {
            return Ok(());
        }

        if !self.config.enabled {
            return Ok(());
        }

        // ---- Bind ecosystem HTTP server port ----
        let mut port = self.config.port;
        let mut bound = false;
        for offset in 0..PORT_RANGE {
            let try_port = port + offset;
            let addr: SocketAddr = match format!("0.0.0.0:{}", try_port).parse() {
                Ok(a) => a,
                Err(_) => continue,
            };
            if tokio::net::TcpListener::bind(addr).await.is_ok() {
                port = try_port;
                bound = true;
                break;
            }
        }
        if !bound {
            return Err(EcoError::Transport("Could not bind to any port".to_string()));
        }
        self.eco_port = port;

        // ---- Start TLS proxy (eco TLS port -> eco HTTP port) ----
        let tls_cfg = tokio::task::spawn_blocking(|| crate::fastswap::tls::get_or_create_tls_config())
            .await
            .map_err(|e| EcoError::Transport(format!("TLS init panicked: {}", e)))?
            .map_err(|e| EcoError::Transport(format!("TLS init error: {}", e)))?;

        let eco_tls_port = if port == DEFAULT_ECO_PORT { ECO_TLS_PORT } else { port + 1 };
        let _ = crate::fastswap::network::start_tls_proxy(eco_tls_port, port, tls_cfg.server_config)
            .await
            .map_err(|e| EcoError::Transport(format!("TLS proxy error: {}", e)))?;

        // ---- Store local device info for pairing ----
        {
            let device = self.local_device.read().await;
            crate::eco::pairing::set_local_device_info(
                device.id.to_string(),
                device.name.clone(),
            );
        }

        // ---- Start ecosystem HTTP server (clipboard endpoint) ----
        let http_addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();
        let transport = self.transport.clone();
        let local_device = self.local_device.clone();

        let discovery = Arc::new(DeviceDiscovery::new(
            self.event_bus.clone(),
        ));
        discovery.start_server(&http_addr).await;
        discovery.start_discovery().await;
        discovery.run_cleanup().await;

        let known_devices = discovery.get_known_devices();

        // ---- Sync manager (sends clipboard changes to peers via HTTPS) ----
        let sync = Arc::new(SyncManager::new(
            self.event_bus.clone(),
            transport.clone(),
            known_devices.clone(),
            self.local_device.clone(),
        ));
        let _ = sync.start().await;
        self.sync = Some(sync);

        // ---- Apply received clipboard data to local clipboard ----
        {
            let event_bus = self.event_bus.clone();
            let manager_ptr = self.clipboard.clone().unwrap();
            event_bus.subscribe(Arc::new(move |event| {
                if let EcoEvent::ClipboardReceived(data, _from) = event {
                    println!("[ECO] Applying received clipboard (hash={})", &data.content_hash[..16]);
                    let manager = manager_ptr.clone();
                    tokio::spawn(async move {
                        if let Ok(mut guard) = manager.lock() {
                            let _ = guard.apply_clipboard(&data);
                        }
                    });
                }
            }));
        }

        self.discovery = Some(discovery);

        println!("[ECO] HTTP server on port {}, TLS proxy on port {}", port, eco_tls_port);
        println!("[ECO] Discovery via subnet scan on ports {} (HTTP) / {} (TLS)", port, eco_tls_port);

        if self.config.clipboard_sync {
            println!("[ECO] Clipboard monitoring ENABLED (polling every 1s)");
            if let Some(clipboard) = &self.clipboard {
                let cb = clipboard.clone();
                tokio::spawn(async move {
                    ClipboardManager::start_monitoring(cb).await;
                });
            }
        }

        // ---- Notification sync ----
        if self.config.notification_sync {
            println!("[ECO] Notification sync ENABLED (mirroring phone notifications)");
            let notification = self.notification.clone();
            let event_bus = self.event_bus.clone();

            // Subscribe to incoming notifications from peers: store them and
            // mirror them as native toasts.
            {
                let notification = notification.clone();
                event_bus.subscribe(Arc::new(move |event| {
                    if let EcoEvent::NotificationReceived(notif, from) = event {
                        if from == "remote" {
                            println!("[ECO] Received notification from {}: {} - {}",
                                notif.device_name, notif.app_name, notif.title);
                            if let Some(ref mgr) = notification {
                                if let Ok(mut guard) = mgr.lock() {
                                    guard.receive_remote(notif.clone());
                                }
                            }
                            #[cfg(target_os = "windows")]
                            crate::platform::ecosystem::notifications::toasts::show_toast(&notif);
                        }
                    }
                }));
            }

            // Subscribe to remote dismissals: remove from local history so
            // the mirror + UI follow the phone's swipe-away, and hide the
            // native toast.
            {
                let notification = notification.clone();
                event_bus.subscribe(Arc::new(move |event| {
                    if let EcoEvent::NotificationDismissed(payload) = event {
                        if let Some(ref mgr) = notification {
                            if let Ok(mut guard) = mgr.lock() {
                                guard.remove_remote(&payload.source_device_id, &payload.notification_key);
                            }
                        }
                        #[cfg(target_os = "windows")]
                        crate::platform::ecosystem::notifications::toasts::hide_toast(&payload.notification_key);
                    }
                }));
            }

            // Update global notification history for UI access
            {
                event_bus.subscribe(Arc::new(move |_event| {
                    if let Some(ref mgr) = notification {
                        if let Ok(guard) = mgr.lock() {
                            crate::eco::notification::set_notifications(guard.get_notifications());
                        }
                    }
                }));
            }
        }

        self.running = true;

        self.event_bus.emit(EcoEvent::Synced(
            format!("Ecosystem started — HTTP:{} TLS:{}", port, eco_tls_port)
        ));

        Ok(())
    }

    pub async fn shutdown(&mut self) {
        if !self.running {
            return;
        }
        self.running = false;
        self.event_bus.emit(EcoEvent::Synced("Ecosystem shut down".to_string()));
    }

    pub fn config(&self) -> &EcosystemConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut EcosystemConfig {
        &mut self.config
    }

    pub fn event_bus(&self) -> &Arc<EventBus> {
        &self.event_bus
    }

    pub fn local_device(&self) -> &Arc<RwLock<EcoDevice>> {
        &self.local_device
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn enable_clipboard_sync(&mut self) {
        self.config.clipboard_sync = true;
    }

    pub fn disable_clipboard_sync(&mut self) {
        self.config.clipboard_sync = false;
    }

    pub fn enable_notification_sync(&mut self) {
        self.config.notification_sync = true;
    }

    pub fn disable_notification_sync(&mut self) {
        self.config.notification_sync = false;
    }

    pub fn notification_manager(&self) -> Option<&Arc<std::sync::Mutex<NotificationManager>>> {
        self.notification.as_ref()
    }

    /// Send a quick reply back to the originating phone (v2 wire path).
    /// The clipboard/Ctrl+V hack is dead: replies route over the HTTPS
    /// transport to the source device, which fires the RemoteInput.
    pub async fn reply_to_notification(&self, notification_id: &str, reply_text: &str) -> EcoResult<()> {
        if reply_text.trim().is_empty() {
            return Err(EcoError::Notification("Empty reply".to_string()));
        }

        // Locate the notification and its originating device.
        let (notification_key, device_id) = {
            let guard = self.notification
                .as_ref()
                .ok_or(EcoError::NotInitialized)?
                .lock()
                .map_err(|_| EcoError::Notification("Lock failed".to_string()))?;
            let target = guard
                .get_notifications()
                .into_iter()
                .find(|n| n.id == notification_id || n.notification_key == notification_id);
            match target {
                Some(n) => (n.notification_key, n.device_id),
                None => return Err(EcoError::Notification(format!(
                    "No such notification: {}", notification_id
                ))),
            }
        };

        let addr = {
            let addrs = crate::eco::discovery::ECO_DEVICE_ADDRS.lock()
                .map_err(|_| EcoError::Notification("Addr lock failed".to_string()))?;
            addrs.get(&device_id).copied().ok_or_else(|| {
                EcoError::Notification(format!("No route to device {}", device_id))
            })?
        };

        let payload = crate::eco::protocol::NotificationReplyPayload {
            notification_id: notification_id.to_string(),
            notification_key,
            reply_text: reply_text.to_string(),
            source_device_id: crate::eco::pairing::get_local_device_id().unwrap_or_default(),
        };
        self.transport.send_notification_reply(&addr, &payload).await?;

        // Reflect the reply locally.
        if let Some(ref mgr) = self.notification {
            if let Ok(mut guard) = mgr.lock() {
                guard.mark_replied(notification_id);
            }
        }
        Ok(())
    }

    /// Ask the originating phone to fire one of the notification's action
    /// buttons (v2 wire path).
    pub async fn fire_notification_action(&self, notification_id: &str, action_index: u32) -> EcoResult<()> {
        let (notification_key, device_id) = {
            let guard = self.notification
                .as_ref()
                .ok_or(EcoError::NotInitialized)?
                .lock()
                .map_err(|_| EcoError::Notification("Lock failed".to_string()))?;
            let target = guard
                .get_notifications()
                .into_iter()
                .find(|n| n.id == notification_id || n.notification_key == notification_id);
            match target {
                Some(n) => (n.notification_key, n.device_id),
                None => return Err(EcoError::Notification(format!(
                    "No such notification: {}", notification_id
                ))),
            }
        };

        let addr = {
            let addrs = crate::eco::discovery::ECO_DEVICE_ADDRS.lock()
                .map_err(|_| EcoError::Notification("Addr lock failed".to_string()))?;
            addrs.get(&device_id).copied().ok_or_else(|| {
                EcoError::Notification(format!("No route to device {}", device_id))
            })?
        };

        let payload = crate::eco::protocol::NotificationActionPayload {
            notification_key,
            action_index,
            source_device_id: crate::eco::pairing::get_local_device_id().unwrap_or_default(),
        };
        self.transport.send_notification_action(&addr, &payload).await
    }

    pub fn get_notifications(&self) -> Vec<crate::eco::notification::NotificationData> {
        if let Some(ref mgr) = self.notification {
            if let Ok(guard) = mgr.lock() {
                return guard.get_notifications();
            }
        }
        Vec::new()
    }

    pub fn mark_notification_read(&self, notification_id: &str) {
        if let Some(ref mgr) = self.notification {
            if let Ok(mut guard) = mgr.lock() {
                guard.mark_read(notification_id);
            }
        }
    }

    pub fn clear_notifications(&self) {
        if let Some(ref mgr) = self.notification {
            if let Ok(mut guard) = mgr.lock() {
                guard.clear_all();
            }
        }
    }

    pub async fn apply_received_clipboard(&self, data: ClipboardData) -> EcoResult<()> {
        if let Some(clipboard) = &self.clipboard {
            let mut guard = clipboard.lock().map_err(|_| {
                EcoError::Clipboard("Lock failed".to_string())
            })?;
            guard.apply_clipboard(&data)
        } else {
            Err(EcoError::NotInitialized)
        }
    }
}

lazy_static::lazy_static! {
    pub static ref ECO_MANAGER: std::sync::Mutex<Option<EcoManager>> = std::sync::Mutex::new(None);
}

pub fn get_eco_manager() -> Option<std::sync::MutexGuard<'static, Option<EcoManager>>> {
    ECO_MANAGER.lock().ok()
}

pub fn init_eco_manager(pkg_dir: &PathBuf) -> EcoResult<()> {
    let mut manager = EcoManager::new(pkg_dir);
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| EcoError::Transport(e.to_string()))?;
    runtime.block_on(manager.initialize(pkg_dir))?;

    let mut guard = ECO_MANAGER.lock()
        .map_err(|_| EcoError::Transport("Lock failed".to_string()))?;
    *guard = Some(manager);
    Ok(())
}

pub fn start_eco_manager() -> EcoResult<()> {
    let mut guard = ECO_MANAGER.lock()
        .map_err(|_| EcoError::Transport("Lock failed".to_string()))?;
    if let Some(ref mut manager) = *guard {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| EcoError::Transport(e.to_string()))?;
        runtime.block_on(manager.start())?;
        Ok(())
    } else {
        Err(EcoError::NotInitialized)
    }
}

pub async fn init_eco_manager_async(pkg_dir: &PathBuf) -> EcoResult<()> {
    let mut manager = EcoManager::new(pkg_dir);
    manager.initialize(pkg_dir).await?;
    let mut guard = ECO_MANAGER.lock()
        .map_err(|_| EcoError::Transport("Lock failed".to_string()))?;
    *guard = Some(manager);
    Ok(())
}

pub async fn start_eco_manager_async() -> EcoResult<()> {
    let mut guard = ECO_MANAGER.lock()
        .map_err(|_| EcoError::Transport("Lock failed".to_string()))?;
    if let Some(ref mut manager) = *guard {
        manager.start().await?;
        Ok(())
    } else {
        Err(EcoError::NotInitialized)
    }
}

pub fn shutdown_eco_manager() {
    if let Ok(mut guard) = ECO_MANAGER.lock() {
        if let Some(ref mut manager) = *guard {
            let runtime = tokio::runtime::Runtime::new().ok();
            if let Some(rt) = runtime {
                rt.block_on(manager.shutdown());
            }
        }
    }
}