// src/ui/sentinel_status.rs - Minimal SENTINEL status strip (UI stub).
//
// Covers the SENTINEL Sec34 requirements that can be read from existing UI
// state WITHOUT new backend plumbing:
// - Current task (last command / status from the host pump)
// - Active device (THIS STATION label passed in by the host)
// - Current model + provider (chat model selector state)
// - Online/offline state (`online::is_online_mode()`)
// - Cancellation entry point (Cancel button; see propagation note below)
//
// CANCELLATION PROPAGATION NOTE (master Sec31): the canonical order is
// UI -> Task -> Agent -> Tool -> Device. This strip's Cancel button only
// emits `on_cancel`; the host must fan it out to the live executors
// (FastSwap `notify_cancellation`, reminder/alarm teardown, and -- Phase 12
// -- `igris-core::CancellationNode::cancel()` on the task subtree).
// The UI never owns cancellation itself.
//
// TODO Phase 12 (full SENTINEL dashboard): dedicated tab/panel with live
// Task progress (`EventKind::TaskProgress`), Tool activity, Security events
// (`EventKind::SecurityEvent`), Memory activity (`EventKind::MemoryActivity`),
// Agent activity (`EventKind::AgentActivity`), Errors, and Model/resource
// usage -- all subscribed to the `igris-core` event bus, with enforcement
// staying in the backend.

use dioxus::prelude::*;

/// Minimal SENTINEL status strip. All values are display-only snapshots
/// passed in (task/device) or read from existing crate globals
/// (online flag, selected model/provider).
#[component]
pub fn SentinelStatus(
    current_task: String,
    status_text: String,
    active_device: String,
    on_cancel: EventHandler<()>,
    primary_color: String,
    accent_rgb: String,
) -> Element {
    let online = crate::online::is_online_mode();
    let model = crate::get_selected_model();
    let provider = crate::get_selected_provider();
    let has_task = !current_task.trim().is_empty();

    // Short model id for the strip (e.g. "meta/llama-3.1-70b-instruct" -> tail).
    let model_short = model
        .rsplit('/')
        .next()
        .unwrap_or(model.as_str())
        .to_string();
    let model_label = format!("{provider}/{model_short}");

    rsx! {
        div {
            title: "SENTINEL status: task, device, model, connectivity",
            style: format!(
                "display: flex; align-items: center; gap: 10px; flex-wrap: wrap; padding: 10px 16px; border-radius: 4px; background: rgba(5,10,20,0.85); border: 1px solid rgba({}, 0.12); font-family: 'JetBrains Mono', monospace;",
                accent_rgb
            ),
            // Online / offline badge (never color-only: text label included).
            span {
                title: if online { "Online mode: NVIDIA NIM" } else { "Offline mode: local models" },
                style: if online {
                    "padding: 2px 10px; border-radius: 4px; font-size: 9px; letter-spacing: 1px; font-weight: 700; background: rgba(34,197,94,0.12); color: #22c55e; border: 1px solid rgba(34,197,94,0.25);".to_string()
                } else {
                    "padding: 2px 10px; border-radius: 4px; font-size: 9px; letter-spacing: 1px; font-weight: 700; background: rgba(245,158,11,0.12); color: #f59e0b; border: 1px solid rgba(245,158,11,0.25);".to_string()
                },
                if online { "ONLINE" } else { "OFFLINE" }
            }
            // Current model badge.
            span {
                title: "Current chat model",
                style: format!(
                    "padding: 2px 10px; border-radius: 4px; font-size: 9px; letter-spacing: 1px; background: rgba({}, 0.1); color: {}; border: 1px solid rgba({}, 0.25);",
                    accent_rgb, primary_color, accent_rgb
                ),
                "MODEL :: {model_label}"
            }
            // Active device badge.
            span {
                title: "Active device",
                style: "padding: 2px 10px; border-radius: 4px; font-size: 9px; letter-spacing: 1px; background: rgba(255,255,255,0.05); color: rgba(255,255,255,0.55); border: 1px solid rgba(255,255,255,0.08);",
                "DEVICE :: {active_device}"
            }
            // Current task + status.
            span {
                title: "Current task",
                style: "flex: 1; min-width: 140px; font-size: 10px; color: rgba(255,255,255,0.55); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                if has_task { "TASK :: {current_task}  ||  {status_text}" } else { "TASK :: <IDLE>  ||  {status_text}" }
            }
            // Cancellation entry point (UI -> Task -> Agent -> Tool -> Device).
            button {
                title: "Cancel current task (UI propagates cancel to task, agent, tool, and device)",
                disabled: !has_task,
                style: if has_task {
                    "padding: 4px 12px; border-radius: 4px; border: 1px solid rgba(239,68,68,0.35); background: rgba(239,68,68,0.12); color: #ef4444; cursor: pointer; font-size: 10px; font-weight: 700; letter-spacing: 1px; font-family: 'JetBrains Mono', monospace;".to_string()
                } else {
                    "padding: 4px 12px; border-radius: 4px; border: 1px solid rgba(255,255,255,0.08); background: transparent; color: rgba(255,255,255,0.2); font-size: 10px; font-weight: 700; letter-spacing: 1px; font-family: 'JetBrains Mono', monospace;".to_string()
                },
                onclick: move |_| {
                    on_cancel.call(());
                },
                "CANCEL"
            }
        }
    }
}
