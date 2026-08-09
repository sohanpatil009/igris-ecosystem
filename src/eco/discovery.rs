use crate::eco::constants::*;
use crate::eco::device::EcoDevice;
use crate::eco::events::{EcoEvent, EventBus};
use crate::eco::protocol::{ClipboardSyncPayload, NotificationSyncPayload, NotificationReplyPayload};
use axum::extract::ConnectInfo;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoveredEcoDevice {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub ip: String,
    pub port: u16,
    pub is_trusted: bool,
    pub is_online: bool,
    pub last_seen_secs: u64,
}

#[derive(Deserialize)]
struct PairRequestPayload {
    sender_id: String,
    sender_name: String,
    #[serde(default = "default_port")]
    sender_port: u16,
}

fn default_port() -> u16 { ECO_TLS_PORT }

#[derive(Deserialize)]
struct UntrustPayload {
    device_id: String,
}

lazy_static::lazy_static! {
    pub static ref ECO_NETWORK_DEVICES: Arc<RwLock<Vec<DiscoveredEcoDevice>>> =
        Arc::new(RwLock::new(Vec::new()));
}

pub(crate) struct DiscoveredDevice {
    pub(crate) device: EcoDevice,
    pub(crate) last_heartbeat: Instant,
}

pub struct DeviceDiscovery {
    known_devices: Arc<RwLock<HashMap<String, DiscoveredDevice>>>,
    event_bus: Arc<EventBus>,
}

impl DeviceDiscovery {
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            known_devices: Arc::new(RwLock::new(HashMap::new())),
            event_bus,
        }
    }

    pub(crate) fn get_known_devices(&self) -> Arc<RwLock<HashMap<String, DiscoveredDevice>>> {
        self.known_devices.clone()
    }

    pub async fn start_server(&self, addr: &SocketAddr) {
        let event_bus = self.event_bus.clone();
        let local_id = crate::eco::pairing::get_local_device_id();
        let local_name = crate::eco::pairing::get_local_device_name();

        let app = axum::Router::new()
            .route("/api/ecosystem/v1/info", axum::routing::get({
                let dev_id = local_id.clone();
                let dev_name = local_name.clone();
                move || {
                    let did = dev_id.clone();
                    let dnm = dev_name.clone();
                    async move {
                        axum::Json(serde_json::json!({
                            "status": "ok",
                            "ecosystem": true,
                            "clipboard_sync": true,
                            "device_id": did,
                            "device_name": dnm,
                        }))
                    }
                }
            }))
            .route("/api/ecosystem/v1/clipboard/sync", axum::routing::post({
                let bus = event_bus.clone();
                move |body: axum::extract::Json<ClipboardSyncPayload>| {
                    let bus = bus.clone();
                    async move {
                        let payload = body.0;
                        let data = crate::eco::clipboard::ClipboardData {
                            content: payload.content,
                            content_type: payload.content_type,
                            content_hash: payload.content_hash,
                            source_device: payload.source_device,
                            timestamp: payload.timestamp,
                        };
                        bus.emit(EcoEvent::ClipboardReceived(
                            std::sync::Arc::new(data),
                            String::new(),
                        ));
                        axum::Json(serde_json::json!({"status": "ok"}))
                    }
                }
            }))
            .route("/api/ecosystem/v1/pair/request", axum::routing::post({
                let bus = event_bus.clone();
                let dev_id = local_id.clone();
                let dev_name = local_name.clone();
                move |ConnectInfo(remote_addr): ConnectInfo<SocketAddr>, body: axum::extract::Json<PairRequestPayload>| {
                    let bus = bus.clone();
                    let dev_id = dev_id.clone();
                    let dev_name = dev_name.clone();
                    async move {
                        let p = body.0;
                        let sender_id = p.sender_id;
                        let sender_name = p.sender_name;

                        // Direct two-way link: trust the requesting device immediately.
                        let mut eco_network = ECO_NETWORK_DEVICES.write().await;
                        if let Some(dev) = eco_network.iter_mut().find(|d| d.id == sender_id) {
                            dev.is_trusted = true;
                        } else {
                            eco_network.push(DiscoveredEcoDevice {
                                id: sender_id.clone(),
                                name: sender_name.clone(),
                                hostname: String::new(),
                                ip: remote_addr.ip().to_string(),
                                port: p.sender_port,
                                is_trusted: true,
                                is_online: true,
                                last_seen_secs: 0,
                            });
                        }
                        drop(eco_network);

                        bus.emit(EcoEvent::DeviceTrusted(
                            std::sync::Arc::new(crate::eco::device::EcoDevice::new(sender_id))
                        ));

                        axum::Json(serde_json::json!({
                            "status": "ok",
                            "trusted": true,
                            "receiver_id": dev_id,
                            "receiver_name": dev_name,
                        }))
                    }
                }
            }))
            .route("/api/ecosystem/v1/pair/untrust", axum::routing::post({
                move |body: axum::extract::Json<UntrustPayload>| {
                    async move {
                        let p = body.0;
                        let mut eco_network = ECO_NETWORK_DEVICES.write().await;
                        for dev in eco_network.iter_mut() {
                            if dev.id == p.device_id {
                                dev.is_trusted = false;
                            }
                        }
                        axum::Json(serde_json::json!({"status": "ok"}))
                    }
                }
            }))
            .route("/api/ecosystem/v1/notification/sync", axum::routing::post({
                let bus = event_bus.clone();
                move |body: axum::extract::Json<NotificationSyncPayload>| {
                    let bus = bus.clone();
                    async move {
                        let payload = body.0;
                        let notif = crate::eco::notification::NotificationData {
                            id: payload.notification_id,
                            app_name: payload.app_name,
                            title: payload.title,
                            body: payload.body,
                            device_name: payload.source_device_name,
                            device_id: payload.source_device_id,
                            timestamp: payload.timestamp,
                            read: false,
                            replied: false,
                        };
                        bus.emit(EcoEvent::NotificationReceived(
                            notif,
                            "remote".to_string(),
                        ));
                        axum::Json(serde_json::json!({"status": "ok"}))
                    }
                }
            }))
            .route("/api/ecosystem/v1/notification/reply", axum::routing::post({
                let bus = event_bus.clone();
                move |body: axum::extract::Json<NotificationReplyPayload>| {
                    let bus = bus.clone();
                    async move {
                        let payload = body.0;
                        let reply = crate::eco::notification::NotificationReply {
                            notification_id: payload.notification_id,
                            reply_text: payload.reply_text,
                            source_device_id: payload.source_device_id,
                        };
                        bus.emit(EcoEvent::NotificationReplied(reply));
                        axum::Json(serde_json::json!({"status": "ok"}))
                    }
                }
            }));

        let bind_addr = *addr;
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(bind_addr).await;
            if let Ok(listener) = listener {
                axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.ok();
            }
        });
    }

    /// Periodically scan the subnet on eco's dedicated ports (53327 HTTP, 53328 TLS)
    /// to discover peers — mirrors FastSwap's subnet probing but on eco's own ports.
    pub async fn start_discovery(&self) {
        let known_devices = self.known_devices.clone();

        tokio::spawn(async move {
            loop {
                let local_ip = local_ip_address::local_ip()
                    .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 1)));
                let local_ip_str = local_ip.to_string();
                let (peers_http, peers_tls) = tokio::join!(
                    Self::scan_subnet_http(&local_ip),
                    Self::scan_subnet_tls(&local_ip),
                );
                let http_count = peers_http.len();
                let tls_count = peers_tls.len();
                let all_peers = peers_http.into_iter().chain(peers_tls);
                let mut devices = known_devices.write().await;
                let mut network_list = ECO_NETWORK_DEVICES.write().await;
                let mut seen_ids = std::collections::HashSet::new();
                let mut new_count = 0usize;
                for peer in all_peers {
                    let id = peer.id.to_string();
                    let now = Instant::now();
                    seen_ids.insert(id.clone());
                    match devices.get_mut(&id) {
                        Some(existing) => {
                            existing.last_heartbeat = now;
                        }
                        None => {
                            devices.insert(id.clone(), DiscoveredDevice {
                                device: peer.clone(),
                                last_heartbeat: now,
                            });
                            new_count += 1;
                        }
                    }
                    let ip_str = peer.addr.map(|a| a.ip().to_string()).unwrap_or_default();
                    let port = peer.addr.map(|a| a.port()).unwrap_or(ECO_TLS_PORT);
                    match network_list.iter_mut().find(|d| d.id == id) {
                        Some(entry) => {
                            entry.name = peer.name.clone();
                            entry.hostname = peer.hostname.clone();
                            entry.ip = ip_str;
                            entry.port = port;
                            entry.is_online = true;
                            entry.last_seen_secs = 0;
                        }
                        None => {
                            network_list.push(DiscoveredEcoDevice {
                                id: id.clone(),
                                name: peer.name.clone(),
                                hostname: peer.hostname.clone(),
                                ip: ip_str,
                                port,
                                is_trusted: false,
                                is_online: true,
                                last_seen_secs: 0,
                            });
                        }
                    }
                }
                // Mark peers not seen this pass as offline. Remove stale devices
                // unless they are LINKED, so links survive brief outages.
                network_list.retain_mut(|d| {
                    if seen_ids.contains(&d.id) {
                        return true;
                    }
                    d.last_seen_secs = d.last_seen_secs.saturating_add(HEARTBEAT_INTERVAL_SECS);
                    if d.last_seen_secs >= DEVICE_TIMEOUT_SECS && !d.is_trusted {
                        return false;
                    }
                    d.is_online = false;
                    true
                });
                drop(network_list);
                drop(devices);
                if new_count > 0 || http_count > 0 || tls_count > 0 {
                    println!("[ECO] Discovery: {} eco (http) + {} eco (tls) devices found ({} new) from {}", http_count, tls_count, new_count, local_ip_str);
                }

                tokio::time::sleep(std::time::Duration::from_secs(HEARTBEAT_INTERVAL_SECS)).await;
            }
        });
    }

    /// Probe a single IP on the given port/scheme for the eco info endpoint.
    async fn probe_ip(ip: &str, port: u16, use_tls: bool) -> Option<EcoDevice> {
        let scheme = if use_tls { "https" } else { "http" };
        let url = format!("{}://{}:{}/api/ecosystem/v1/info", scheme, ip, port);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .danger_accept_invalid_certs(use_tls)
            .build()
            .ok()?;

        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    let device_id = body.get("device_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let device_name = body.get("device_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&format!("eco-{}", ip))
                        .to_string();
                    if device_id.is_empty() {
                        return None;
                    }
                    let uuid = match uuid::Uuid::parse_str(&device_id) {
                        Ok(u) => u,
                        Err(_) => uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, device_id.as_bytes()),
                    };
                    let mut eco = EcoDevice::new(device_name);
                    eco.id = uuid;
                    if let Ok(ip_addr) = ip.parse::<std::net::IpAddr>() {
                        eco.addr = Some(SocketAddr::new(ip_addr, ECO_TLS_PORT));
                    }
                    eco.hostname = body.get("device_model")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    eco.capabilities.clipboard_sync = true;
                    Some(eco)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Scan the /24 subnet via HTTP on the ecosystem port (53327).
    async fn scan_subnet_http(local_ip: &std::net::IpAddr) -> Vec<EcoDevice> {
        Self::scan_subnet(local_ip, DEFAULT_ECO_PORT, false).await
    }

    /// Scan the /24 subnet via HTTPS on the ecosystem TLS port (53328).
    async fn scan_subnet_tls(local_ip: &std::net::IpAddr) -> Vec<EcoDevice> {
        Self::scan_subnet(local_ip, ECO_TLS_PORT, true).await
    }

    /// Generic subnet scan: probes all 254 IPs in the /24 on the given port/scheme.
    async fn scan_subnet(local_ip: &std::net::IpAddr, port: u16, use_tls: bool) -> Vec<EcoDevice> {
        let subnet = match local_ip {
            std::net::IpAddr::V4(v4) => {
                let octets = v4.octets();
                format!("{}.{}.{}", octets[0], octets[1], octets[2])
            }
            _ => return Vec::new(),
        };

        let mut discovered = Vec::new();
        let mut tasks = Vec::new();

        for i in 1..=254 {
            let ip = format!("{}.{}", subnet, i);
            if let Ok(probe_ip) = ip.parse::<std::net::IpAddr>() {
                if probe_ip == *local_ip {
                    continue;
                }
            }

            let task = tokio::spawn(async move {
                Self::probe_ip(&ip, port, use_tls).await
            });
            tasks.push(task);

            if tasks.len() >= 50 {
                for task in tasks.drain(..) {
                    if let Ok(Some(device)) = task.await {
                        discovered.push(device);
                    }
                }
            }
        }
        for task in tasks {
            if let Ok(Some(device)) = task.await {
                discovered.push(device);
            }
        }
        discovered
    }

    pub async fn run_cleanup(&self) {
        let known_devices = self.known_devices.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                std::time::Duration::from_secs(DEVICE_TIMEOUT_SECS / 2)
            );
            loop {
                interval.tick().await;
                let mut devices = known_devices.write().await;
                let now = Instant::now();
                devices.retain(|_, d| {
                    now.duration_since(d.last_heartbeat).as_secs() < DEVICE_TIMEOUT_SECS
                });
            }
        });
    }
}
