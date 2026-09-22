use crate::eco::config::EcosystemConfig;
use crate::eco::constants::*;
use crate::eco::device::EcoDevice;
use crate::eco::events::{EcoEvent, EventBus};
use crate::eco::protocol::{
    ClipboardSyncPayload, NotificationActionPayload, NotificationDismissPayload,
    NotificationReplyPayload, NotificationSyncPayload,
};
use axum::extract::ConnectInfo;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

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

fn default_port() -> u16 {
    ECO_TLS_PORT
}

#[derive(Deserialize)]
struct UntrustPayload {
    device_id: String,
}

/// Sanitize an attacker-controlled device id for audit logs (allowlist +
/// truncate). Prevents log injection while keeping UUIDs intact.
/// Never logs clipboard/notification bodies — only endpoint + device id.
fn audit_device_id(id: &str) -> String {
    let clean: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if clean.is_empty() {
        "<empty>".to_string()
    } else if clean.len() > 64 {
        clean[..64].to_string()
    } else {
        clean
    }
}

/// Fail-closed deny response for untrusted peers (deny-by-default).
/// Audit-logs the attempt (endpoint + redacted device id + E_FORBIDDEN)
/// and returns HTTP 403 + `E_FORBIDDEN` JSON. Callers must `return` it
/// without emitting any event or touching state.
fn deny_untrusted(endpoint: &'static str, device_id: &str) -> axum::response::Response {
    tracing::warn!(
        endpoint = endpoint,
        source_device = %audit_device_id(device_id),
        error_code = "E_FORBIDDEN",
        "denied untrusted eco request (fail closed)"
    );
    (
        axum::http::StatusCode::FORBIDDEN,
        axum::Json(serde_json::json!({
            "status": "error",
            "error_code": "E_FORBIDDEN",
            "message": "untrusted device",
        })),
    )
        .into_response()
}

lazy_static::lazy_static! {
    pub static ref ECO_NETWORK_DEVICES: Arc<RwLock<Vec<DiscoveredEcoDevice>>> =
        Arc::new(RwLock::new(Vec::new()));

    /// Peer device_id -> address (ip:ECO_TLS_PORT) learned from inbound
    /// requests. Used by the toast activation path to route replies/actions
    /// back to the phone without holding a tokio runtime handle.
    pub static ref ECO_DEVICE_ADDRS: std::sync::Mutex<HashMap<String, SocketAddr>> =
        std::sync::Mutex::new(HashMap::new());
}

/// All IPv4 addresses of usable local interfaces (excludes loopback,
/// link-local and the rmnet 192.0.0/24 internal range). Scanners skip these
/// so a multi-interface host never discovers itself: the legacy single-IP
/// self-skip missed the wlan address whenever `local_ip()` guessed rmnet.
pub fn local_ipv4s() -> Vec<std::net::Ipv4Addr> {
    let mut local_ips: Vec<std::net::Ipv4Addr> = Vec::new();
    if let Ok(ifaces) = local_ip_address::list_afinet_netifas() {
        for (_name, ip) in ifaces {
            if let std::net::IpAddr::V4(v4) = ip {
                if !v4.is_loopback() {
                    local_ips.push(v4);
                }
            }
        }
    }
    local_ips
}

/// IPv4 /24 subnets of all usable local interfaces, private-range subnets
/// first, capped at MAX_SCAN_SUBNETS.
///
/// Discovery used to guess one interface via `local_ip()`; on multi-homed
/// hosts (Android rmnet enumerated before wlan0, desktop VPN/vEthernet
/// adapters) it picked the wrong subnet, making that side blind to peers
/// while staying fully discoverable itself — the one-way-visibility bug.
pub fn local_subnets() -> Vec<String> {
    const MAX_SCAN_SUBNETS: usize = 4;

    let mut local_ips = local_ipv4s();
    if local_ips.is_empty() {
        // Legacy fallback: single-interface guess.
        if let Ok(std::net::IpAddr::V4(v4)) = local_ip_address::local_ip() {
            local_ips.push(v4);
        }
    }

    let mut subnets: Vec<(bool, String)> = Vec::new(); // (is_private, "a.b.c")
    for v4 in local_ips {
        let octets = v4.octets();
        // Skip loopback, link-local (169.254/16) and 192.0.0/24 (Qualcomm
        // rmnet internal addressing — never carries LAN peers, and probing
        // its 254 dead addresses wastes ~10s per scan pass).
        if v4.is_loopback()
            || (octets[0] == 169 && octets[1] == 254)
            || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        {
            continue;
        }
        let subnet = format!("{}.{}.{}", octets[0], octets[1], octets[2]);
        if subnets.iter().any(|(_, s)| *s == subnet) {
            continue;
        }
        let is_private = octets[0] == 192 && octets[1] == 168
            || octets[0] == 10
            || octets[0] == 172 && (16..=31).contains(&octets[1]);
        subnets.push((is_private, subnet));
    }
    subnets.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    subnets
        .into_iter()
        .take(MAX_SCAN_SUBNETS)
        .map(|(_, s)| s)
        .collect()
}

pub(crate) struct DiscoveredDevice {
    pub(crate) device: EcoDevice,
    pub(crate) last_heartbeat: Instant,
}

pub struct DeviceDiscovery {
    known_devices: Arc<RwLock<HashMap<String, DiscoveredDevice>>>,
    event_bus: Arc<EventBus>,
    runtime_config: Arc<std::sync::RwLock<EcosystemConfig>>,
}

impl DeviceDiscovery {
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            known_devices: Arc::new(RwLock::new(HashMap::new())),
            event_bus,
            runtime_config: Arc::new(std::sync::RwLock::new(EcosystemConfig::default())),
        }
    }

    pub fn set_runtime_config(&self, config: EcosystemConfig) {
        if let Ok(mut cfg) = self.runtime_config.write() {
            *cfg = config;
        }
    }

    pub(crate) fn get_known_devices(&self) -> Arc<RwLock<HashMap<String, DiscoveredDevice>>> {
        self.known_devices.clone()
    }

    pub async fn start_server(&self, addr: &SocketAddr) {
        let event_bus = self.event_bus.clone();
        let local_id = crate::eco::pairing::get_local_device_id();
        let local_name = crate::eco::pairing::get_local_device_name();
        let runtime_config = self.runtime_config.clone();

        let app = axum::Router::new()
            .route(
                "/api/ecosystem/v1/info",
                axum::routing::get({
                    let dev_id = local_id.clone();
                    let dev_name = local_name.clone();
                    let cfg_ref = runtime_config.clone();
                    move || {
                        let did = dev_id.clone();
                        let dnm = dev_name.clone();
                        let cfg_ref = cfg_ref.clone();
                        async move {
                            let fastswap_tls_port = crate::fastswap::get_fastswap_tls_port();
                            let (cfg_clipboard_sync, cfg_notification_sync) = {
                                let cfg_guard = cfg_ref.read().unwrap();
                                (cfg_guard.clipboard_sync, cfg_guard.notification_sync)
                            };

                            let trusted_peers: Vec<String> = {
                                if let Some(guard) = crate::eco::pairing::get_pairing_manager() {
                                    if let Some(manager) = guard.as_ref() {
                                        manager.get_trusted_ids()
                                    } else {
                                        Vec::new()
                                    }
                                } else {
                                    Vec::new()
                                }
                            };

                            axum::Json(serde_json::json!({
                                "status": "ok",
                                "ecosystem": true,
                                "clipboard_sync": cfg_clipboard_sync,
                                "notification_sync": cfg_notification_sync,
                                "device_id": did,
                                "device_name": dnm,
                                "device_type": "desktop",
                                "fastswap_port": fastswap_tls_port,
                                "trusted_peers": trusted_peers,
                            }))
                        }
                    }
                }),
            )
            .route(
                "/api/ecosystem/v1/clipboard/sync",
                axum::routing::post({
                    let bus = event_bus.clone();
                    move |body: axum::extract::Json<ClipboardSyncPayload>| {
                        let bus = bus.clone();
                        async move {
                            let payload = body.0;
                            // Trust gate (deny-by-default, fail closed): matches the
                            // /notification/sync gate. Untrusted or missing ids are
                            // denied with E_FORBIDDEN + audit log; never emit.
                            if !crate::eco::pairing::is_trusted_sync(&payload.source_device) {
                                return deny_untrusted("/clipboard/sync", &payload.source_device);
                            }
                            // Schema validation (least-privilege): reject empty ids
                            // and oversized bodies. Bodies are never logged (secret
                            // redaction) — only lengths + redacted device id.
                            if payload.source_device.is_empty() || payload.content_hash.is_empty() {
                                tracing::warn!(
                                    endpoint = "/clipboard/sync",
                                    source_device = %audit_device_id(&payload.source_device),
                                    error_code = "E_VALIDATION",
                                    "rejected malformed clipboard sync"
                                );
                                return (
                                    axum::http::StatusCode::BAD_REQUEST,
                                    axum::Json(serde_json::json!({
                                        "status": "error",
                                        "error_code": "E_VALIDATION",
                                        "message": "invalid clipboard payload",
                                    })),
                                )
                                    .into_response();
                            }
                            if payload.content.len() > MAX_CLIPBOARD_TEXT_BYTES as usize {
                                tracing::warn!(
                                    endpoint = "/clipboard/sync",
                                    source_device = %audit_device_id(&payload.source_device),
                                    content_len = payload.content.len(),
                                    error_code = "E_VALIDATION",
                                    "rejected oversized clipboard sync"
                                );
                                return (
                                    axum::http::StatusCode::PAYLOAD_TOO_LARGE,
                                    axum::Json(serde_json::json!({
                                        "status": "error",
                                        "error_code": "E_VALIDATION",
                                        "message": "clipboard payload too large",
                                    })),
                                )
                                    .into_response();
                            }
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
                            (
                                axum::http::StatusCode::OK,
                                axum::Json(serde_json::json!({"status": "ok"})),
                            )
                                .into_response()
                        }
                    }
                }),
            )
            .route(
                "/api/ecosystem/v1/pair/request",
                axum::routing::post({
                    let bus = event_bus.clone();
                    let dev_id = local_id.clone();
                    let dev_name = local_name.clone();
                    move |ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
                          body: axum::extract::Json<PairRequestPayload>| {
                        let bus = bus.clone();
                        let dev_id = dev_id.clone();
                        let dev_name = dev_name.clone();
                        async move {
                            let p = body.0;
                            let sender_id = p.sender_id;
                            let sender_name = p.sender_name;

                            // Remember how to reach this peer (toast routing).
                            if let Ok(mut addrs) = ECO_DEVICE_ADDRS.lock() {
                                addrs.insert(
                                    sender_id.clone(),
                                    SocketAddr::new(remote_addr.ip(), p.sender_port),
                                );
                            }

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

                            // Persist trust so it survives restarts.
                            crate::eco::pairing::persist_trust(&sender_id);

                            // Emit a DeviceTrusted event carrying the REAL peer
                            // identity (mirrors the igris-protocol fix). The old
                            // `EcoDevice::new(sender_id)` produced a fresh random
                            // UUID with the sender's UUID as its *name*, so any
                            // consumer of the event saw a bogus device until the
                            // next discovery scan replaced it.
                            let mut trusted = crate::eco::device::EcoDevice::new(sender_name);
                            trusted.id = uuid::Uuid::parse_str(&sender_id).unwrap_or_else(|_| {
                                uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, sender_id.as_bytes())
                            });
                            trusted.addr = Some(remote_addr);
                            bus.emit(EcoEvent::DeviceTrusted(std::sync::Arc::new(trusted)));

                            axum::Json(serde_json::json!({
                                "status": "ok",
                                "trusted": true,
                                "receiver_id": dev_id,
                                "receiver_name": dev_name,
                            }))
                        }
                    }
                }),
            )
            .route(
                "/api/ecosystem/v1/pair/untrust",
                axum::routing::post({
                    move |body: axum::extract::Json<UntrustPayload>| {
                        async move {
                            let p = body.0;
                            let mut eco_network = ECO_NETWORK_DEVICES.write().await;
                            for dev in eco_network.iter_mut() {
                                if dev.id == p.device_id {
                                    dev.is_trusted = false;
                                }
                            }
                            drop(eco_network);
                            // Persist the unlink so it survives restarts — matches
                            // the LINK side, which writes through persist_trust().
                            crate::eco::pairing::persist_untrust(&p.device_id);
                            axum::Json(serde_json::json!({"status": "ok"}))
                        }
                    }
                }),
            )
            .route(
                "/api/ecosystem/v1/notification/sync",
                axum::routing::post({
                    let bus = event_bus.clone();
                    move |ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
                          body: axum::extract::Json<NotificationSyncPayload>| {
                        let bus = bus.clone();
                        async move {
                            let payload = body.0;
                            // Trust gate (deny-by-default, fail closed): untrusted or
                            // missing ids are denied with E_FORBIDDEN + audit log.
                            if !crate::eco::pairing::is_trusted_sync(&payload.source_device_id) {
                                return deny_untrusted(
                                    "/notification/sync",
                                    &payload.source_device_id,
                                );
                            }
                            // Schema validation: require sender + notification identity.
                            // Bodies/titles are never logged (secret redaction).
                            if payload.source_device_id.is_empty()
                                || (payload.notification_id.is_empty()
                                    && payload.notification_key.is_empty())
                            {
                                tracing::warn!(
                                    endpoint = "/notification/sync",
                                    source_device = %audit_device_id(&payload.source_device_id),
                                    error_code = "E_VALIDATION",
                                    "rejected malformed notification sync"
                                );
                                return (
                                    axum::http::StatusCode::BAD_REQUEST,
                                    axum::Json(serde_json::json!({
                                        "status": "error",
                                        "error_code": "E_VALIDATION",
                                        "message": "invalid notification payload",
                                    })),
                                )
                                    .into_response();
                            }
                            // Remember how to reach this peer for toast replies/actions.
                            // Prefer the port we actually found the peer on (probe /
                            // discovery result) over the hard-coded TLS guess: a peer
                            // whose eco TLS proxy failed to bind (e.g. a phone that
                            // only answers plain HTTP) would otherwise get replies
                            // routed to a refused port. This was the root cause of
                            // one-way clipboard/notification failures (log evidence:
                            // "Clipboard send to 192.168.1.4:53328 failed ... refused").
                            let known_port = {
                                let net = ECO_NETWORK_DEVICES.read().await;
                                net.iter()
                                    .find(|d| d.id == payload.source_device_id)
                                    .map(|d| d.port)
                            };
                            let reply_port = known_port.unwrap_or(ECO_TLS_PORT);
                            if let Ok(mut addrs) = ECO_DEVICE_ADDRS.lock() {
                                addrs.insert(
                                    payload.source_device_id.clone(),
                                    SocketAddr::new(remote_addr.ip(), reply_port),
                                );
                            }
                            let notif = crate::eco::notification::NotificationData {
                                id: payload.notification_id,
                                notification_key: payload.notification_key,
                                app_package: payload.app_package,
                                app_name: payload.app_name,
                                title: payload.title,
                                body: payload.body,
                                icon: payload.icon,
                                icon_hash: payload.icon_hash,
                                actions: payload.actions,
                                can_reply: payload.can_reply,
                                messages: payload.messages,
                                device_name: payload.source_device_name,
                                device_id: payload.source_device_id,
                                timestamp: payload.timestamp,
                                read: false,
                                replied: false,
                            };
                            let notif_arc = std::sync::Arc::new(notif);
                            std::thread::spawn(move || {
                                let notif = (*notif_arc).clone();
                                bus.emit(EcoEvent::NotificationReceived(
                                    notif,
                                    "remote".to_string(),
                                ));
                            });
                            (
                                axum::http::StatusCode::OK,
                                axum::Json(serde_json::json!({"status": "ok"})),
                            )
                                .into_response()
                        }
                    }
                }),
            )
            .route(
                "/api/ecosystem/v1/notification/reply",
                axum::routing::post({
                    let bus = event_bus.clone();
                    move |body: axum::extract::Json<NotificationReplyPayload>| {
                        let bus = bus.clone();
                        async move {
                            let payload = body.0;
                            // Trust gate (deny-by-default, fail closed): matches
                            // /notification/sync. Never emit on untrusted.
                            if !crate::eco::pairing::is_trusted_sync(&payload.source_device_id) {
                                return deny_untrusted(
                                    "/notification/reply",
                                    &payload.source_device_id,
                                );
                            }
                            // Schema validation: require sender + notification identity;
                            // reply bodies are never logged (secret redaction).
                            if payload.source_device_id.is_empty()
                                || (payload.notification_id.is_empty()
                                    && payload.notification_key.is_empty())
                                || payload.reply_text.is_empty()
                                || payload.reply_text.len() > 8192
                            {
                                tracing::warn!(
                                    endpoint = "/notification/reply",
                                    source_device = %audit_device_id(&payload.source_device_id),
                                    error_code = "E_VALIDATION",
                                    "rejected malformed notification reply"
                                );
                                return (
                                    axum::http::StatusCode::BAD_REQUEST,
                                    axum::Json(serde_json::json!({
                                        "status": "error",
                                        "error_code": "E_VALIDATION",
                                        "message": "invalid reply payload",
                                    })),
                                )
                                    .into_response();
                            }
                            let reply = crate::eco::notification::NotificationReply {
                                notification_id: payload.notification_id,
                                notification_key: payload.notification_key,
                                reply_text: payload.reply_text,
                                source_device_id: payload.source_device_id,
                            };
                            std::thread::spawn(move || {
                                bus.emit(EcoEvent::NotificationReplied(reply));
                            });
                            (
                                axum::http::StatusCode::OK,
                                axum::Json(serde_json::json!({"status": "ok"})),
                            )
                                .into_response()
                        }
                    }
                }),
            )
            .route(
                "/api/ecosystem/v1/notification/dismiss",
                axum::routing::post({
                    let bus = event_bus.clone();
                    move |body: axum::extract::Json<NotificationDismissPayload>| {
                        let bus = bus.clone();
                        async move {
                            let payload = body.0;
                            // Trust gate (deny-by-default, fail closed): matches
                            // /notification/sync. Never emit on untrusted.
                            if !crate::eco::pairing::is_trusted_sync(&payload.source_device_id) {
                                return deny_untrusted(
                                    "/notification/dismiss",
                                    &payload.source_device_id,
                                );
                            }
                            // Schema validation: require sender + notification key.
                            if payload.source_device_id.is_empty()
                                || payload.notification_key.is_empty()
                            {
                                tracing::warn!(
                                    endpoint = "/notification/dismiss",
                                    source_device = %audit_device_id(&payload.source_device_id),
                                    error_code = "E_VALIDATION",
                                    "rejected malformed notification dismiss"
                                );
                                return (
                                    axum::http::StatusCode::BAD_REQUEST,
                                    axum::Json(serde_json::json!({
                                        "status": "error",
                                        "error_code": "E_VALIDATION",
                                        "message": "invalid dismiss payload",
                                    })),
                                )
                                    .into_response();
                            }
                            bus.emit(EcoEvent::NotificationDismissed(payload));
                            (
                                axum::http::StatusCode::OK,
                                axum::Json(serde_json::json!({"status": "ok"})),
                            )
                                .into_response()
                        }
                    }
                }),
            )
            .route(
                "/api/ecosystem/v1/notification/action",
                axum::routing::post({
                    let bus = event_bus.clone();
                    move |body: axum::extract::Json<NotificationActionPayload>| {
                        let bus = bus.clone();
                        async move {
                            let payload = body.0;
                            // Trust gate (deny-by-default, fail closed): matches
                            // /notification/sync. Never emit on untrusted.
                            if !crate::eco::pairing::is_trusted_sync(&payload.source_device_id) {
                                return deny_untrusted(
                                    "/notification/action",
                                    &payload.source_device_id,
                                );
                            }
                            // Schema validation: require sender + notification key.
                            if payload.source_device_id.is_empty()
                                || payload.notification_key.is_empty()
                            {
                                tracing::warn!(
                                    endpoint = "/notification/action",
                                    source_device = %audit_device_id(&payload.source_device_id),
                                    error_code = "E_VALIDATION",
                                    "rejected malformed notification action"
                                );
                                return (
                                    axum::http::StatusCode::BAD_REQUEST,
                                    axum::Json(serde_json::json!({
                                        "status": "error",
                                        "error_code": "E_VALIDATION",
                                        "message": "invalid action payload",
                                    })),
                                )
                                    .into_response();
                            }
                            std::thread::spawn(move || {
                                bus.emit(EcoEvent::NotificationActionRequested(payload));
                            });
                            (
                                axum::http::StatusCode::OK,
                                axum::Json(serde_json::json!({"status": "ok"})),
                            )
                                .into_response()
                        }
                    }
                }),
            )
            .route(
                "/api/ecosystem/v1/notification/request",
                axum::routing::post({
                    let bus = event_bus.clone();
                    move |body: axum::extract::Json<serde_json::Value>| {
                        let bus = bus.clone();
                        async move {
                            // Trust gate (deny-by-default, fail closed): the request
                            // body must carry `source_device_id` of a trusted peer.
                            // Missing or untrusted ids are denied with E_FORBIDDEN +
                            // audit log; legacy senders that omit the field are
                            // therefore denied (fail closed by design).
                            let source_device_id = body
                                .0
                                .get("source_device_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if !crate::eco::pairing::is_trusted_sync(source_device_id) {
                                return deny_untrusted("/notification/request", source_device_id);
                            }
                            std::thread::spawn(move || {
                                bus.emit(EcoEvent::NotificationRequested);
                            });
                            (
                                axum::http::StatusCode::OK,
                                axum::Json(serde_json::json!({"status": "ok"})),
                            )
                                .into_response()
                        }
                    }
                }),
            );

        let bind_addr = *addr;
        tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(bind_addr).await;
            if let Ok(listener) = listener {
                axum::serve(
                    listener,
                    app.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .await
                .ok();
            }
        });
    }

    /// Periodically scan every local interface's /24 on eco's dedicated ports
    /// (53327 HTTP, 53328 TLS) to discover peers — mirrors FastSwap's subnet
    /// probing but on eco's own ports, across all subnets (see `local_subnets`).
    pub async fn start_discovery(&self) {
        let known_devices = self.known_devices.clone();
        let event_bus = self.event_bus.clone();
        let local_id = crate::eco::pairing::get_local_device_id();

        tokio::spawn(async move {
            loop {
                let local_ip = local_ip_address::local_ip().unwrap_or(std::net::IpAddr::V4(
                    std::net::Ipv4Addr::new(192, 168, 1, 1),
                ));
                let local_ip_str = local_ip.to_string();
                let subnets = local_subnets();
                let scans = subnets
                    .iter()
                    .map(|subnet| async move {
                        tokio::join!(
                            Self::scan_subnet(subnet, false),
                            Self::scan_subnet(subnet, true),
                        )
                    })
                    .collect::<Vec<_>>();
                let mut http_count = 0usize;
                let mut tls_count = 0usize;
                let mut all_peers: Vec<EcoDevice> = Vec::new();
                for (peers_http, peers_tls) in futures::future::join_all(scans).await {
                    http_count += peers_http.len();
                    tls_count += peers_tls.len();
                    all_peers.extend(peers_http.into_iter().chain(peers_tls));
                }
                // A peer reachable on both ports shows up twice — dedup by id.
                all_peers.sort_by_key(|p| p.id);
                all_peers.dedup_by(|a, b| a.id == b.id);
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
                            // Refresh mutable fields so capability/config
                            // changes on the peer are picked up promptly
                            // (e.g. clipboard_sync toggled in runtime config).
                            existing.device.addr = peer.addr;
                            existing.device.capabilities = peer.capabilities.clone();
                            existing.device.platform = peer.platform.clone();
                            existing.device.trusted_peers = peer.trusted_peers.clone();
                        }
                        None => {
                            devices.insert(
                                id.clone(),
                                DiscoveredDevice {
                                    device: peer.clone(),
                                    last_heartbeat: now,
                                },
                            );
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
                            let mut trusted = crate::eco::pairing::is_trusted_sync(&id);
                            // Stale trust detection: if we think this peer is
                            // trusted but the peer's /info says it does NOT
                            // trust us back, the peer likely restarted and lost
                            // its trust state. Demote it locally so the UI
                            // reflects the real state.
                            if trusted {
                                if let Some(ref local) = local_id {
                                    match &peer.trusted_peers {
                                        Some(ids) if !ids.contains(local) => {
                                            trusted = false;
                                            crate::eco::pairing::persist_untrust(&id);
                                            event_bus.emit(EcoEvent::DeviceUntrusted(
                                                std::sync::Arc::new(peer.clone()),
                                            ));
                                        }
                                        None => {} // old binary, skip check
                                        _ => {}
                                    }
                                }
                            }
                            entry.is_trusted = trusted;
                        }
                        None => {
                            let mut trusted = crate::eco::pairing::is_trusted_sync(&id);
                            if trusted {
                                if let Some(ref local) = local_id {
                                    match &peer.trusted_peers {
                                        Some(ids) if !ids.contains(local) => {
                                            trusted = false;
                                            crate::eco::pairing::persist_untrust(&id);
                                            event_bus.emit(EcoEvent::DeviceUntrusted(
                                                std::sync::Arc::new(peer.clone()),
                                            ));
                                        }
                                        None => {}
                                        _ => {}
                                    }
                                }
                            }
                            network_list.push(DiscoveredEcoDevice {
                                id: id.clone(),
                                name: peer.name.clone(),
                                hostname: peer.hostname.clone(),
                                ip: ip_str,
                                port,
                                is_trusted: trusted,
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
                    let device_id = body
                        .get("device_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let device_name = body
                        .get("device_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&format!("eco-{}", ip))
                        .to_string();
                    if device_id.is_empty() {
                        return None;
                    }
                    let uuid = match uuid::Uuid::parse_str(&device_id) {
                        Ok(u) => u,
                        Err(_) => {
                            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, device_id.as_bytes())
                        }
                    };
                    let mut eco = EcoDevice::new(device_name);
                    eco.id = uuid;
                    if let Ok(ip_addr) = ip.parse::<std::net::IpAddr>() {
                        eco.addr = Some(SocketAddr::new(ip_addr, port));
                    }
                    eco.hostname = body
                        .get("device_model")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    eco.capabilities.clipboard_sync = true;
                    eco.capabilities.notification_sync = body
                        .get("notification_sync")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    // Carry the peer's self-reported type ("mobile" for phones)
                    // so the keep-alive tier can gate on mobile peers. Overwrites
                    // the EcoDevice::new default (which is the *local* OS) — a
                    // desktop peer omits both keys and stays non-mobile ("").
                    eco.platform = body
                        .get("device_type")
                        .or_else(|| body.get("platform"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    // Peer's self-reported trusted device list — used to detect
                    // stale trust (e.g. desktop restarted and lost trust).
                    eco.trusted_peers =
                        body.get("trusted_peers")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            });
                    Some(eco)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Scan one /24 subnet (`"a.b.c"`) on the ecosystem HTTP (53327) or TLS
    /// (53328) port. Skips every local interface address so the probe host
    /// never discovers itself (see `local_ipv4s`).
    async fn scan_subnet(subnet: &str, use_tls: bool) -> Vec<EcoDevice> {
        let port = if use_tls {
            ECO_TLS_PORT
        } else {
            DEFAULT_ECO_PORT
        };
        let self_ips: std::collections::HashSet<std::net::Ipv4Addr> =
            local_ipv4s().into_iter().collect();

        let mut discovered = Vec::new();
        let mut tasks = Vec::new();

        for i in 1..=254 {
            let ip = format!("{}.{}", subnet, i);
            if let Ok(std::net::IpAddr::V4(v4)) = ip.parse::<std::net::IpAddr>() {
                if self_ips.contains(&v4) {
                    continue;
                }
            }

            let task = tokio::spawn(async move { Self::probe_ip(&ip, port, use_tls).await });
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
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(DEVICE_TIMEOUT_SECS / 2));
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
