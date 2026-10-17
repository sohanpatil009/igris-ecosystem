//! Native Windows toast mirror for remote notifications (Phase 3).
//!
//! Remote notifications arriving over the wire are displayed as native
//! Windows toasts with quick-reply and action buttons. Activation routes
//! back to the originating phone via the eco transport
//! (`/api/ecosystem/v1/notification/{reply,action}`).
//!
//! Toast tags are derived from the `notification_key` (Windows tags are
//! limited to 16 alphanumeric characters, phone keys are arbitrary), so
//! re-sending an update replaces the toast instead of stacking copies.

use crate::eco::notification::NotificationData;
use crate::eco::protocol::{NotificationActionPayload, NotificationReplyPayload};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Mutex, OnceLock};
use sha2::Digest;
use winrt_toast_reborn::content::input::InputType;
use winrt_toast_reborn::{Action, ActivatedAction, Input, Toast, ToastDuration, ToastManager};

const AUM_ID: &str = "IGRIS.Ecosystem.Desktop";
const INPUT_ID: &str = "replybox";

/// Routing info captured at show time so the activation callback (which
/// runs on a COM thread with no tokio context) knows where to deliver
/// replies/actions.
#[derive(Clone)]
struct ToastRoute {
    notification_id: String,
    notification_key: String,
    addr: SocketAddr,
    /// `call-*` routes only: the call's phone number for the action relay.
    phone_number: String,
}

static ROUTES: Mutex<Option<HashMap<String, ToastRoute>>> = Mutex::new(None);

// ToastManager is !Send/!Sync per the crate (HSTRING + COM event handlers),
// but in practice it is plain data used from whichever thread calls
// show()/remove(); the WinRT statics are called per-invocation.
struct ManagerCell(ToastManager);
unsafe impl Send for ManagerCell {}
unsafe impl Sync for ManagerCell {}

static MANAGER: OnceLock<ManagerCell> = OnceLock::new();

fn manager() -> &'static ToastManager {
    let cell = MANAGER.get_or_init(|| {
        if let Err(e) = winrt_toast_reborn::register(AUM_ID, "IGRIS Ecosystem", None) {
            eprintln!("[ECO] toast AUMID registration failed: {:?}", e);
        }
        ManagerCell(
            ToastManager::new(AUM_ID)
                .on_activated(Some(INPUT_ID), |action| {
                    if let Some(a) = action {
                        handle_activation(&a);
                    }
                })
                .on_dismissed(|_r| {})
                .on_failed(|f| {
                    eprintln!("[ECO] toast failed to display: {:?}", f.error);
                }),
        )
    });
    &cell.0
}

/// Windows toast tags: max 16 alphanumeric chars. Derive a stable short tag
/// from the full notification key via SHA-256.
fn tag_for(notification_key: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(notification_key.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
    format!("n{}", &hex[..15])
}

/// Display (or replace) a native toast mirroring a remote notification.
pub fn show_toast(notif: &NotificationData) {
    if notif.notification_key.is_empty() {
        // No stable identity: nothing to mirror or route back to.
        return;
    }
    let tag = tag_for(&notif.notification_key);

    // Route registration must stay synchronous (used by activation callbacks),
    // but the actual WinRT show() call runs on its own thread: the event bus
    // emits synchronously inside the axum handler, and blocking a tokio worker
    // on COM/ToastManager would stall every sync request.
    //
    // Address resolution: learned inbound routes first, live discovery as
    // fallback (a restart wipes the route table until the next inbound
    // request — without fallback the toast shows a dead reply box).
    let route_addr = route_addr_for(&notif.device_id);
    if let Some(addr) = route_addr {
        if let Ok(mut routes) = ROUTES.lock() {
            routes.get_or_insert_with(HashMap::new).insert(
                tag.clone(),
                ToastRoute {
                    notification_id: notif.id.clone(),
                    notification_key: notif.notification_key.clone(),
                    addr,
                    phone_number: String::new(),
                },
            );
        }
    } else {
        toast_log(format!(
            "show_toast: no route for device {} — reply will be dead",
            notif.device_id
        ));
    }

    let tag_clone = tag.clone();
    let app_name = notif.app_name.clone();
    let device_name = notif.device_name.clone();
    let title = notif.title.clone();
    let body = notif.body.clone();
    let can_reply = notif.can_reply;
    let actions = notif.actions.clone();
    std::thread::spawn(move || {
        let mut toast = Toast::new();
        toast
            .tag(&tag_clone)
            .group("igris-notifications")
            .text1(format!("{} · {}", app_name, device_name))
            .text2(&title)
            .duration(ToastDuration::Long);
        if !body.is_empty() {
            toast.text3(&body);
        }
        if can_reply {
            toast
                .input(Input::new(INPUT_ID, InputType::Text).with_placeholder("Type a reply..."))
                .action(Action::new("Reply", "reply", "").with_input_id(INPUT_ID));
        }
        for (i, action) in actions.iter().take(2).enumerate() {
            toast.action(Action::new(action, format!("action:{}", i), ""));
        }
        if let Err(e) = manager().show(&toast) {
            eprintln!("[ECO] toast show failed: {:?}", e);
        }
    });
}

/// Remove a mirrored toast when the phone dismisses the notification.
pub fn hide_toast(notification_key: &str) {
    let tag = tag_for(notification_key);
    std::thread::spawn(move || {
        let _ = manager().remove(&tag);
    });
}

/// Show a toast for an incoming call. `owner_device_id` identifies the
/// device whose line is ringing: empty/our own id → the desktop's own
/// telephony seam (no peer round-trip); a linked phone's id → the
/// Accept/Decline buttons relay `/call/action` to that handset.
pub fn show_call_toast(
    call_id: &str,
    display_name: &str,
    phone_number: &str,
    owner_device_id: &str,
) {
    let key = format!("call-{}", call_id);
    let tag = tag_for(&key);
    let route_addr = if owner_device_id.is_empty() {
        "127.0.0.1:0".parse().unwrap()
    } else {
        route_addr_for(owner_device_id)
            .unwrap_or_else(|| "127.0.0.1:0".parse().unwrap())
    };
    if let Ok(mut routes) = ROUTES.lock() {
        routes.get_or_insert_with(HashMap::new).insert(
            tag.clone(),
            ToastRoute {
                notification_id: call_id.to_string(),
                notification_key: key,
                addr: route_addr,
                phone_number: phone_number.to_string(),
            },
        );
    }
    let name = if display_name.is_empty() {
        phone_number.to_string()
    } else {
        display_name.to_string()
    };
    let number = phone_number.to_string();
    let accept_arg = format!("callaccept:{}", call_id);
    let decline_arg = format!("calldecline:{}", call_id);
    std::thread::spawn(move || {
        let mut toast = Toast::new();
        toast
            .tag(&tag)
            .group("igris-calls")
            .text1("Incoming call")
            .text2(&name)
            .duration(ToastDuration::Long);
        if !number.is_empty() && name != number {
            toast.text3(&number);
        }
        toast
            .action(Action::new("Accept", accept_arg, ""))
            .action(Action::new("Decline", decline_arg, ""));
        if let Err(e) = manager().show(&toast) {
            eprintln!("[ECO] call toast show failed: {:?}", e);
        }
        // Mac-style ring: audible chirp cycle while the card is up.
        crate::platform::ecosystem::notifications::ringtone::start();
    });
}

/// Dismiss the incoming-call toast once the call moves past ringing.
pub fn hide_call_toast(call_id: &str) {
    let tag = tag_for(&format!("call-{}", call_id));
    crate::platform::ecosystem::notifications::ringtone::stop();
    std::thread::spawn(move || {
        let _ = manager().remove(&tag);
    });
}

/// Resolve a peer address for toast routing: live discovery snapshot first
/// (the route table refreshes on every /info heartbeat), registry fallback.
/// Non-blocking (try_read) so the sync event path never stalls.
fn route_addr_for(device_id: &str) -> Option<SocketAddr> {
    if let Ok(net) = crate::eco::discovery::ECO_NETWORK_DEVICES.try_read() {
        if let Some(d) = net.iter().find(|d| d.id == device_id) {
            if let Ok(ip) = d.ip.parse::<std::net::IpAddr>() {
                return Some(SocketAddr::new(ip, d.port));
            }
        }
    }
    None
}

/// Desktop stdout is detached (windows_subsystem) — activation diagnostics
/// go to a file next to the other eco stores so toast replies are debuggable.
fn toast_log(msg: String) {
    try_toast_log(&msg);
}

fn try_toast_log(msg: &str) {
    use std::io::Write;
    let path = std::path::PathBuf::from("./pkg/ecosystem/toast.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(
            f,
            "[{}] {}",
            chrono::Utc::now().format("%H:%M:%S"),
            msg
        );
    }
}
/// Route a toast button press back to the phone.
fn handle_activation(a: &ActivatedAction) {
    let Some(tag) = &a.tag else {
        toast_log("activation without tag".to_string());
        return;
    };

    // Call controls: relay to the handset that owns the ringing line, or
    // fall back to this device's own telephony seam.
    if let Some(call_id) = a.arg.strip_prefix("callaccept:") {
        handle_call_action(call_id, "accept");
        hide_call_toast(call_id);
        return;
    }
    if let Some(call_id) = a.arg.strip_prefix("calldecline:") {
        handle_call_action(call_id, "decline");
        hide_call_toast(call_id);
        return;
    }

    let route = match ROUTES.lock() {
        Ok(m) => m.as_ref().and_then(|map| map.get(tag).cloned()),
        Err(_) => return,
    };
    let Some(route) = route else {
        toast_log(format!("activation for unknown tag {}", tag));
        return;
    };

    let source_device_id = crate::eco::pairing::get_local_device_id().unwrap_or_default();

    if a.arg.starts_with("reply") {
        let text = a.values.get(INPUT_ID).cloned().unwrap_or_default();
        if text.trim().is_empty() {
            toast_log(format!("empty reply for tag {}", tag));
            return;
        }
        toast_log(format!("reply via toast for tag {} ({} chars)", tag, text.len()));
        let payload = NotificationReplyPayload {
            notification_id: route.notification_id,
            notification_key: route.notification_key,
            reply_text: text,
            source_device_id,
        };
        spawn_post(route.addr, "/api/ecosystem/v1/notification/reply", &payload);
    } else if let Some(idx) = a.arg.strip_prefix("action:") {
        if let Ok(action_index) = idx.parse::<u32>() {
            let payload = NotificationActionPayload {
                notification_key: route.notification_key,
                action_index,
                source_device_id,
            };
            spawn_post(route.addr, "/api/ecosystem/v1/notification/action", &payload);
        }
    }
}

/// Accept/decline an incoming call from the toast: if the route carries a
/// real peer address the action is relayed to that handset via
/// `/api/ecosystem/v1/call/action`; otherwise it executes on the desktop's
/// own platform telephony seam.
fn handle_call_action(call_id: &str, action: &str) {
    toast_log(format!("call {} from toast: {}", action, call_id));
    let tag = tag_for(&format!("call-{}", call_id));
    let route = ROUTES
        .lock()
        .ok()
        .and_then(|m| m.as_ref().and_then(|map| map.get(&tag).cloned()));
    if let Some(route) = route {
        if route.addr.port() != 0 {
            let payload = crate::eco::protocol::CallActionPayload {
                call_id: call_id.to_string(),
                action: action.to_string(),
                source_device_id: crate::eco::pairing::get_local_device_id().unwrap_or_default(),
                phone_number: route.phone_number.clone(),
                contact_name: String::new(),
            };
            spawn_post(route.addr, "/api/ecosystem/v1/call/action", &payload);
            if action == "accept" {
                // The handset parks the audio-routing request in its eco
                // core; it applies the moment the line goes offhook. A short
                // delay keeps it ordered after the accept on the same link.
                let audio = crate::eco::protocol::CallActionPayload {
                    call_id: call_id.to_string(),
                    action: "audio".to_string(),
                    source_device_id: crate::eco::pairing::get_local_device_id().unwrap_or_default(),
                    phone_number: route.phone_number.clone(),
                    contact_name: String::new(),
                };
                let relay_addr = route.addr;
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    spawn_post(relay_addr, "/api/ecosystem/v1/call/action", &audio);
                });
            }
            return;
        }
    }
    if let Some(seam) = crate::platform::ecosystem::calls::platform_call() {
        let _ = if action == "accept" {
            seam.accept_call(call_id)
        } else {
            seam.decline_call(call_id)
        };
    }
}

/// Fire-and-forget POST from a detached thread (the COM activation
/// callback has no tokio runtime on its thread). Uses http/https based on
/// the peer's port so it works with both TLS and HTTP-only peers.
/// Outcomes are logged to toast.log — desktop stdout is detached.
fn spawn_post<T: serde::Serialize>(addr: SocketAddr, path: &str, payload: &T) {
    let scheme = if addr.port() == crate::eco::constants::DEFAULT_ECO_PORT { "http" } else { "https" };
    let url = format!("{}://{}{}", scheme, addr, path);
    let body = serde_json::to_vec(payload).unwrap_or_default();
    std::thread::spawn(move || {
        let outcome = (|| {
            let client = reqwest::blocking::Client::builder()
                .danger_accept_invalid_certs(true)
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .map_err(|e| e.to_string())?;
            let resp = client
                .post(&url)
                .header("Content-Type", "application/json")
                .body(body)
                .send()
                .map_err(|e| e.to_string())?;
            Ok::<_, String>(resp.status().to_string())
        })();
        match outcome {
            Ok(status) => toast_log(format!("POST {} -> {}", url, status)),
            Err(e) => toast_log(format!("POST {} failed: {}", url, e)),
        }
    });
}
