//! Windows telephony implementation using `Windows.ApplicationModel.Calls`.
//!
//! Drives the desktop's own phone line (a Bluetooth-paired phone registered
//! with Windows). Requires CallsPhoneContract v5+ (Windows 10 1903+).
//! In-call control (accept/decline/hold/end/mute) requires v6 (build 20348+).
//!
//! Threading: WinRT COM calls run on whichever thread invokes them. The
//! watcher + active-call enumeration happen on a dedicated std thread so they
//! never block the tokio runtime.

use crate::eco::errors::{EcoError, EcoResult};
use crate::platform::ecosystem::notifications::PlatformCall;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use super::LocalCallSnapshot;
use windows::ApplicationModel::Calls::{
    PhoneCall, PhoneCallManager, PhoneCallOperationStatus, PhoneCallStatus, PhoneLine,
    PhoneLineWatcher, PhoneLineWatcherEventArgs,
};
use windows::Foundation::TypedEventHandler;

/// Shared state for the Windows call platform.
struct CallState {
    /// PhoneLine keyed by line GUID string.
    lines: HashMap<String, PhoneLine>,
    /// Active calls keyed by call ID string.
    active_calls: HashMap<String, PhoneCall>,
}

static CALL_STATE: OnceLock<Mutex<CallState>> = OnceLock::new();

fn state() -> &'static Mutex<CallState> {
    CALL_STATE.get_or_init(|| {
        Mutex::new(CallState {
            lines: HashMap::new(),
            active_calls: HashMap::new(),
        })
    })
}

fn win_err(context: &str, e: impl std::fmt::Display) -> EcoError {
    EcoError::Call(format!("{}: {}", context, e))
}

/// True when the CallsPhoneContract is present (Windows 10 1903+).
fn contract_available() -> bool {
    PhoneCallManager::IsCallActive().is_ok()
}

/// True when CallsPhoneContract v6 is present (build 20348+ / Win11).
fn call_control_available() -> bool {
    PhoneCallManager::RequestStoreAsync().is_ok()
}

pub struct WindowsPlatformCall;

impl PlatformCall for WindowsPlatformCall {
    fn is_available(&self) -> bool {
        contract_available()
    }

    fn dial(&self, number: &str) -> EcoResult<()> {
        if number.is_empty() {
            return Err(EcoError::Call("empty number".to_string()));
        }

        // Find the first dialable line.
        let line = {
            let sg = state().lock().map_err(|_| EcoError::Call("lock poisoned".to_string()))?;
            sg.lines.values().find(|l| l.CanDial().unwrap_or(false)).cloned()
        };

        match line {
            Some(l) => {
                let hstring = windows::core::HSTRING::from(number);
                let display = windows::core::HSTRING::from(number);
                if call_control_available() {
                    // v6: DialWithResult + route audio to the local device.
                    let result = l
                        .DialWithResult(&hstring, &display)
                        .map_err(|e| win_err("dial failed", e))?;
                    let status = result
                        .DialCallStatus()
                        .map_err(|e| win_err("dial status", e))?;
                    if status == PhoneCallOperationStatus::Succeeded {
                        if let Ok(call) = result.DialedCall() {
                            let _ = call.ChangeAudioDeviceAsync(
                                windows::ApplicationModel::Calls::PhoneCallAudioDevice::LocalDevice,
                            );
                            if let Ok(call_id) = call.CallId() {
                                let mut sg = state().lock().unwrap();
                                sg.active_calls.insert(call_id.to_string(), call);
                            }
                        }
                    } else {
                        return Err(EcoError::Call(format!("dial returned {:?}", status)));
                    }
                } else {
                    // v5 fallback: simple Dial.
                    l.Dial(&hstring, &display)
                        .map_err(|e| win_err("dial failed", e))?;
                }
                Ok(())
            }
            None => {
                // No registered line — fall back to the system call UI.
                let hstring = windows::core::HSTRING::from(number);
                let display = windows::core::HSTRING::from(number);
                PhoneCallManager::ShowPhoneCallUI(&hstring, &display)
                    .map_err(|e| win_err("ShowPhoneCallUI failed", e))
            }
        }
    }

    fn accept_call(&self, call_id: &str) -> EcoResult<()> {
        let sg = state().lock().map_err(|_| EcoError::Call("lock poisoned".to_string()))?;
        if let Some(call) = sg.active_calls.get(call_id) {
            call.AcceptIncoming()
                .map_err(|e| win_err("accept failed", e))?;
            Ok(())
        } else {
            Err(EcoError::Call(format!("no active call: {}", call_id)))
        }
    }

    fn decline_call(&self, call_id: &str) -> EcoResult<()> {
        let sg = state().lock().map_err(|_| EcoError::Call("lock poisoned".to_string()))?;
        if let Some(call) = sg.active_calls.get(call_id) {
            call.RejectIncoming()
                .map_err(|e| win_err("decline failed", e))?;
            Ok(())
        } else {
            Err(EcoError::Call(format!("no active call: {}", call_id)))
        }
    }

    fn mute_call(&self, call_id: &str) -> EcoResult<()> {
        let sg = state().lock().map_err(|_| EcoError::Call("lock poisoned".to_string()))?;
        if let Some(call) = sg.active_calls.get(call_id) {
            call.Mute().map_err(|e| win_err("mute failed", e))?;
            Ok(())
        } else {
            Err(EcoError::Call(format!("no active call: {}", call_id)))
        }
    }

    fn end_call(&self, call_id: &str) -> EcoResult<()> {
        let sg = state().lock().map_err(|_| EcoError::Call("lock poisoned".to_string()))?;
        if let Some(call) = sg.active_calls.get(call_id) {
            call.End().map_err(|e| win_err("end failed", e))?;
            Ok(())
        } else {
            Err(EcoError::Call(format!("no active call: {}", call_id)))
        }
    }

    fn snapshot_active(&self) -> Vec<LocalCallSnapshot> {
        // Refresh every registered line's active-call list first.
        let lines: Vec<PhoneLine> = {
            let sg = state().lock().unwrap();
            sg.lines.values().cloned().collect()
        };
        for line in &lines {
            refresh_active_calls(line);
        }

        let sg = state().lock().unwrap();
        sg.active_calls
            .values()
            .filter_map(|call| {
                let call_id = call.CallId().ok()?.to_string();
                let status = call.Status().ok()?;
                let event = match status {
                    PhoneCallStatus::Incoming => "ringing",
                    PhoneCallStatus::Dialing => "dialing",
                    PhoneCallStatus::Talking => "talking",
                    PhoneCallStatus::Held => "held",
                    _ => return None,
                };
                let info = call.GetPhoneCallInfo().ok();
                let phone_number = info
                    .as_ref()
                    .and_then(|i| i.PhoneNumber().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                let display_name = info
                    .as_ref()
                    .and_then(|i| i.DisplayName().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                Some(LocalCallSnapshot {
                    call_id,
                    event: event.to_string(),
                    phone_number,
                    display_name,
                })
            })
            .collect()
    }
}

/// Start the phone-line watcher on a background thread. Called once from
/// `EcoManager::start` when the platform call seam is available.
pub fn start_call_watcher() {
    if !contract_available() {
        eprintln!("[ECO] CallsPhoneContract not available — call watcher skipped");
        return;
    }

    std::thread::spawn(|| {
        // Request the PhoneCallStore to get the line watcher.
        let op = match PhoneCallManager::RequestStoreAsync() {
            Ok(op) => op,
            Err(e) => {
                eprintln!("[ECO] RequestStoreAsync failed: {}", e);
                return;
            }
        };
        let store = match op.get() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[ECO] PhoneCallStore is null — call watcher cannot start: {}", e);
                return;
            }
        };

        let watcher = match store.RequestLineWatcher() {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[ECO] RequestLineWatcher failed: {}", e);
                return;
            }
        };

        // Build the event handlers. They are bound by reference into the
        // watcher (windows-core `Param` takes `&T` for delegate types), and
        // the watcher keeps its own COM reference for the subscription.
        let on_added: TypedEventHandler<PhoneLineWatcher, PhoneLineWatcherEventArgs> =
            TypedEventHandler::new(
                |_sender: windows::core::Ref<'_, PhoneLineWatcher>,
                 args: windows::core::Ref<'_, PhoneLineWatcherEventArgs>| {
                    if let Ok(args) = args.ok() {
                        if let Ok(line_id) = args.LineId() {
                            resolve_line(line_id);
                        }
                    }
                    Ok(())
                },
            );

        let on_removed: TypedEventHandler<PhoneLineWatcher, PhoneLineWatcherEventArgs> =
            TypedEventHandler::new(
                |_sender: windows::core::Ref<'_, PhoneLineWatcher>,
                 args: windows::core::Ref<'_, PhoneLineWatcherEventArgs>| {
                    if let Ok(args) = args.ok() {
                        if let Ok(line_id) = args.LineId() {
                            let line_id_str = format!("{:?}", line_id);
                            let mut sg = state().lock().unwrap();
                            sg.lines.remove(&line_id_str);
                        }
                    }
                    Ok(())
                },
            );

        let on_updated: TypedEventHandler<PhoneLineWatcher, PhoneLineWatcherEventArgs> =
            TypedEventHandler::new(
                |_sender: windows::core::Ref<'_, PhoneLineWatcher>,
                 args: windows::core::Ref<'_, PhoneLineWatcherEventArgs>| {
                    if let Ok(args) = args.ok() {
                        if let Ok(line_id) = args.LineId() {
                            resolve_line(line_id);
                        }
                    }
                    Ok(())
                },
            );

        let _ = watcher.LineAdded(&on_added);
        let _ = watcher.LineRemoved(&on_removed);
        let _ = watcher.LineUpdated(&on_updated);

        if let Err(e) = watcher.Start() {
            eprintln!("[ECO] PhoneLineWatcher Start failed: {}", e);
            return;
        }

        // Park the thread — the watcher callbacks keep it alive via COM refs.
        std::thread::park();
    });
}

/// Resolve a PhoneLine from its GUID, register it if dialable, and refresh
/// the active-call snapshot for it.
fn resolve_line(line_id: windows::core::GUID) {
    let op = match PhoneLine::FromIdAsync(line_id) {
        Ok(op) => op,
        Err(_) => return,
    };
    let line = match op.get() {
        Ok(l) => l,
        Err(_) => return,
    };

    if !line.CanDial().unwrap_or(false) {
        return;
    }

    let line_id_str = format!("{:?}", line_id);
    {
        let mut sg = state().lock().unwrap();
        sg.lines.insert(line_id_str, line.clone());
    }
    refresh_active_calls(&line);
}

/// Enumerate active calls on a line and update the shared state.
fn refresh_active_calls(line: &PhoneLine) {
    let op = match line.GetAllActivePhoneCallsAsync() {
        Ok(op) => op,
        Err(_) => return,
    };
    let result = match op.get() {
        Ok(r) => r,
        Err(_) => return,
    };
    let calls = match result.AllActivePhoneCalls() {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut sg = state().lock().unwrap();
    sg.active_calls.clear();
    for i in 0..calls.Size().unwrap_or(0) {
        if let Ok(call) = calls.GetAt(i) {
            if let Ok(call_id) = call.CallId() {
                sg.active_calls.insert(call_id.to_string(), call);
            }
        }
    }
}