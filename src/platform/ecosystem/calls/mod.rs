//! Platform telephony seam (Igris native).
//!
//! The desktop can host its own phone line when a Bluetooth-paired phone is
//! registered with the OS (`ApplicationModel.Calls` on Windows). This module
//! exposes that line behind the `PlatformCall` trait and owns the local call
//! poller that turns platform call-state changes into `EcoEvent`s. Hosts
//! without telephony keep the no-op stub.

#[cfg(target_os = "windows")]
pub mod windows;

use crate::eco::call::CallData;
use crate::eco::errors::EcoResult;
use crate::eco::events::{EcoEvent, EventBus};
use crate::platform::ecosystem::notifications::PlatformCall;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// Snapshot of one locally active call on the platform-owned line.
#[derive(Clone, Debug)]
pub struct LocalCallSnapshot {
    pub call_id: String,
    /// ringing | dialing | talking | held
    pub event: String,
    pub phone_number: String,
    pub display_name: String,
}

static PLATFORM_CALL: OnceLock<Box<dyn PlatformCall>> = OnceLock::new();

/// Install the host telephony seam once at startup.
pub fn set_platform_call(seam: Box<dyn PlatformCall>) {
    let _ = PLATFORM_CALL.set(seam);
}

pub fn platform_call() -> Option<&'static Box<dyn PlatformCall>> {
    PLATFORM_CALL.get()
}

/// Factory for the platform telephony seam.
pub fn create_platform_call() -> Box<dyn PlatformCall> {
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsPlatformCall)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Box::new(StubCall)
    }
}

/// Fallback for platforms without a local telephony stack.
#[cfg(not(target_os = "windows"))]
struct StubCall;

#[cfg(not(target_os = "windows"))]
impl PlatformCall for StubCall {}

/// Execute a call action on the desktop's own line (dial/accept/decline/
/// mute/end). Used when the action targets this device's call.
pub fn execute_local_action(call_id: &str, action: &str, phone_number: &str) -> EcoResult<()> {
    let seam = platform_call()
        .ok_or_else(|| crate::eco::errors::EcoError::Call("no local telephony".to_string()))?;
    match action {
        "dial" => seam.dial(phone_number),
        "accept" => seam.accept_call(call_id),
        "decline" => seam.decline_call(call_id),
        "mute" => seam.mute_call(call_id),
        "end" => seam.end_call(call_id),
        other => Err(crate::eco::errors::EcoError::Call(format!(
            "unknown call action: {}",
            other
        ))),
    }
}

/// Poll the platform seam for active-call changes and translate transitions
/// into `EcoEvent::CallReceived` carrying this device's identity. Ringing
/// calls also raise a native toast with Accept/Decline. Runs on its own
/// thread; WinRT work inside the seam never blocks the tokio runtime.
pub fn spawn_local_call_poller(event_bus: Arc<EventBus>, interval_ms: u64) {
    std::thread::spawn(move || {
        let mut known: HashMap<String, LocalCallSnapshot> = HashMap::new();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(interval_ms));
            let snapshots = match platform_call() {
                Some(seam) => seam.snapshot_active(),
                None => continue,
            };

            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            for snap in snapshots {
                seen.insert(snap.call_id.clone());
                let changed = known
                    .get(&snap.call_id)
                    .map(|prev| prev.event != snap.event)
                    .unwrap_or(true);
                if !changed {
                    continue;
                }
                known.insert(snap.call_id.clone(), snap.clone());
                emit_local_call(&event_bus, &snap);
            }

            // Calls that vanished from the platform ended.
            let ended: Vec<LocalCallSnapshot> = known
                .iter()
                .filter(|(id, _)| !seen.contains(*id))
                .map(|(_, snap)| snap.clone())
                .collect();
            for snap in ended {
                known.remove(&snap.call_id);
                let mut ended_snap = snap;
                ended_snap.event = "ended".to_string();
                emit_local_call(&event_bus, &ended_snap);
            }
        }
    });
}

/// Phone-originated call events (a linked handset's GSM line) raise and
/// clear the same native toast the local poller uses for the desktop's own
/// line: ringing shows the incoming card, any later state clears it. Lives
/// in the platform layer so the shared eco/ module stays device-neutral.
pub fn spawn_remote_call_watcher(event_bus: Arc<EventBus>) {
    event_bus.subscribe(Arc::new(move |event| {
        #[cfg(target_os = "windows")]
        if let EcoEvent::CallReceived(data) = event {
            let local_id = crate::eco::pairing::get_local_device_id().unwrap_or_default();
            if local_id.is_empty() || data.device_id == local_id {
                return;
            }
            if data.event == "ringing" {
                crate::platform::ecosystem::notifications::toasts::show_call_toast(
                    &data.call_id,
                    &data.contact_name,
                    &data.phone_number,
                    &data.device_id,
                );
            } else {
                crate::platform::ecosystem::notifications::toasts::hide_call_toast(&data.call_id);
            }
        }
    }));
}

fn emit_local_call(event_bus: &EventBus, snap: &LocalCallSnapshot) {
    let device_id = crate::eco::pairing::get_local_device_id().unwrap_or_default();
    let device_name = crate::eco::pairing::get_local_device_name().unwrap_or_default();
    let data = CallData {
        call_id: snap.call_id.clone(),
        event: snap.event.clone(),
        phone_number: snap.phone_number.clone(),
        contact_name: snap.display_name.clone(),
        device_name,
        device_id,
        timestamp: chrono::Utc::now().timestamp_millis(),
        is_cancel: false,
    };
    #[cfg(target_os = "windows")]
    {
        if snap.event == "ringing" {
            // Local line: no owner id → activation executes on this device's
            // own telephony seam.
            crate::platform::ecosystem::notifications::toasts::show_call_toast(
                &snap.call_id,
                &snap.display_name,
                &snap.phone_number,
                "",
            );
        } else {
            crate::platform::ecosystem::notifications::toasts::hide_call_toast(&snap.call_id);
        }
    }
    event_bus.emit(EcoEvent::CallReceived(data));
}