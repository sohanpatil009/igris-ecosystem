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
    if let Ok(addrs) = crate::eco::discovery::ECO_DEVICE_ADDRS.lock() {
        if let Some(addr) = addrs.get(&notif.device_id).copied() {
            if let Ok(mut routes) = ROUTES.lock() {
                routes.get_or_insert_with(HashMap::new).insert(
                    tag.clone(),
                    ToastRoute {
                        notification_id: notif.id.clone(),
                        notification_key: notif.notification_key.clone(),
                        addr,
                    },
                );
            }
        }
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

/// Route a toast button press back to the phone.
fn handle_activation(a: &ActivatedAction) {
    let Some(tag) = &a.tag else { return };
    let route = match ROUTES.lock() {
        Ok(m) => m.as_ref().and_then(|map| map.get(tag).cloned()),
        Err(_) => return,
    };
    let Some(route) = route else { return };

    let source_device_id = crate::eco::pairing::get_local_device_id().unwrap_or_default();

    if a.arg.starts_with("reply") {
        let text = a.values.get(INPUT_ID).cloned().unwrap_or_default();
        if text.trim().is_empty() {
            return;
        }
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

/// Fire-and-forget HTTPS POST from a detached thread (the COM activation
/// callback has no tokio runtime on its thread).
fn spawn_post<T: serde::Serialize>(addr: SocketAddr, path: &str, payload: &T) {
    let url = format!("https://{}{}", addr, path);
    let body = serde_json::to_vec(payload).unwrap_or_default();
    std::thread::spawn(move || {
        let client = reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_secs(10))
            .build();
        if let Ok(client) = client {
            let _ = client.post(&url).body(body).send();
        }
    });
}
