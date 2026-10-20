#![allow(non_snake_case)]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod state;

use dioxus::prelude::*;
use dioxus::desktop::{Config, WindowBuilder};
use igrisecosystem::eco;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;

use igrisecosystem::fastswap;
use igrisecosystem::ui::{
    SettingsPanel, MenuButton, PresentationPanel, FastSwapPanel, IncomingTransferPopup,
    Sidebar, Tab, AlarmReminderPanel, SystemInfoPanel, EcoDevicePanel,
    NotificationPanel, Theme, CallsPanel,
};

// ---------------------------------------------------------------------------
// Simplified mesh status (replaces the old AssistantState)
// ---------------------------------------------------------------------------

pub(crate) static MESH_STATUS: Lazy<Arc<Mutex<String>>> =
    Lazy::new(|| Arc::new(Mutex::new("Mesh starting...".to_string())));

pub(crate) fn update_mesh_status(status: &str) {
    if let Ok(mut s) = MESH_STATUS.lock() {
        *s = status.to_string();
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    // Single-instance guard. A second IGRIS launch can't bind the mesh ports,
    // so it becomes a silent zombie: clips read by one instance never reach
    // peers bound by the other, and sync appears randomly broken. If another
    // instance is alive, focus its window and exit instead.
    let _instance_guard = match std::net::TcpListener::bind("127.0.0.1:53329") {
        Ok(listener) => listener,
        Err(_) => {
            startup_log("[LAUNCH] Another IGRIS instance is already running — focusing it.");
            focus_existing_window();
            std::process::exit(0);
        }
    };

    // Single thread spawn — no nested thread::spawn inside start_eco_only.
    start_eco_only();

    let window = WindowBuilder::new()
        .with_title("IGRIS")
        .with_visible(true)
        .with_inner_size(dioxus::desktop::tao::dpi::LogicalSize::new(1100.0, 700.0))
        .with_window_icon(Some(load_icon()));

    let cfg = Config::new()
        .with_window(window)
        .with_menu(None)
        .with_disable_context_menu(true);

    LaunchBuilder::desktop()
        .with_cfg(cfg)
        .launch(App);
}

fn load_icon() -> dioxus::desktop::tao::window::Icon {
    let icon_data = include_bytes!("../icons/igris_icon.ico");
    let image = image::load_from_memory(icon_data)
        .expect("Failed to load icon")
        .to_rgba8();
    let (width, height) = image.dimensions();
    dioxus::desktop::tao::window::Icon::from_rgba(image.into_raw(), width, height)
        .expect("Failed to create icon")
}

/// Bring the already-running IGRIS window back to the foreground (used by the
/// single-instance guard). Raw user32 calls — no extra crate needed.
#[cfg(windows)]
fn focus_existing_window() {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "user32")]
    extern "system" {
        fn FindWindowW(class: *const u16, title: *const u16) -> *mut core::ffi::c_void;
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
        fn SetForegroundWindow(hwnd: *mut core::ffi::c_void) -> i32;
    }

    let title: Vec<u16> = std::ffi::OsStr::new("IGRIS")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if !hwnd.is_null() {
            ShowWindow(hwnd, 9); // SW_RESTORE
            SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
fn focus_existing_window() {}

// ---------------------------------------------------------------------------
// Mesh boot (single-spawn, proper error handling)
// ---------------------------------------------------------------------------

fn start_eco_only() {
    std::thread::spawn(|| {
        if let Err(e) = run_eco_startup_thread() {
            startup_log(&format!("[ECO] Fatal error in mesh runtime: {}", e));
        }
    });
}

fn run_eco_startup_thread() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        run_eco_startup().await;
        // Keep the runtime (and every server task spawned on it) alive
        // for the life of the process. Returning here would drop the
        // runtime and silently kill discovery, sync and FastSwap.
        futures::future::pending::<()>().await;
    });
    Ok(())
}

fn startup_log(msg: &str) {
    println!("{}", msg);
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("startup.log")
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{}", msg)
        });
}

/// Mesh boot: runs at app launch. No downloads, no microphone — the Devices
/// tab is usable within seconds.
async fn run_eco_startup() {
    startup_log("\n═══════════════════════════════════════════════════════");
    startup_log("[LAUNCH] IGRIS - Device mesh starting");
    startup_log("═══════════════════════════════════════════════════════\n");

    startup_log("\n[ECO] Initializing device mesh...");
    let pkg_dir = PathBuf::from("./pkg");

    // Register the desktop platform seams with the shared protocol crate:
    // the LAN audio engine (cpal/rodio link) and the local Bluetooth
    // telephony line executor. Unregistered hooks would read "no session".
    igris_protocol::platform::ecosystem::hooks::set_audio_fns(
        igrisecosystem::platform::ecosystem::audio::link_up,
        igrisecosystem::platform::ecosystem::audio::attach,
        igrisecosystem::platform::ecosystem::audio::detach,
        igrisecosystem::platform::ecosystem::audio::test_tone_to_phone,
    );
    #[cfg(target_os = "windows")]
    igris_protocol::platform::ecosystem::hooks::set_local_call_action(
        igrisecosystem::platform::ecosystem::calls::execute_local_action,
    );

    // FastSwap directories: keep the TLS certs in pkg/certs and received
    // files landing in the OS Downloads folder (desktop UX), while the
    // phone injects its app-owned paths on its side.
    {
        let downloads = dirs::download_dir()
            .unwrap_or_else(|| pkg_dir.join("downloads"));
        igris_protocol::fastswap::set_fastswap_paths(pkg_dir.join("certs"), downloads);
    }

    match eco::init_eco_manager_async(
        &pkg_dir,
        igrisecosystem::platform::ecosystem::create_platform_clipboard(),
        igrisecosystem::platform::ecosystem::notifications::create_platform_notification(),
    )
    .await
    {
        Ok(_) => {
            let config_path = pkg_dir.join("ecosystem/ecosystem_config.json");
            if let Some(mut guard) = eco::get_eco_manager() {
                if let Some(ref mut manager) = *guard {
                    manager.enable_clipboard_sync();
                    manager.enable_notification_sync();
                    manager.config_mut().call_sync = true;
                    manager.config_mut().sms_sync = true;
                    manager.config_mut().clipboard_media = true;
                    manager.config_mut().enabled = true;
                    manager.config_mut().save(&config_path);
                }
            }
            match eco::start_eco_manager_async().await {
                Ok(_) => {
                    startup_log("[ECO] Mesh live on ports 53327/53328 (clipboard + notifications + calls/SMS)");

                    // ---- Desktop-only seam wiring (moved out of the shared
                    // EcoManager::start, which must stay platform-neutral) ----
                    let event_bus = {
                        let mut guard = eco::get_eco_manager();
                        match guard.as_mut().and_then(|m| m.as_mut()) {
                            Some(manager) => Some(manager.event_bus().clone()),
                            None => None,
                        }
                    };
                    if let Some(event_bus) = event_bus {
                        #[cfg(target_os = "windows")]
                        {
                            use igrisecosystem::platform::ecosystem::calls;

                            // Local telephony seam: phone-originated rings raise
                            // the native toast; the desktop's own line uses the
                            // watcher + poller.
                            calls::spawn_remote_call_watcher(event_bus.clone());
                            let seam = calls::create_platform_call();
                            if seam.is_available() {
                                calls::set_platform_call(seam);
                                calls::windows::start_call_watcher();
                                calls::spawn_local_call_poller(event_bus.clone(), 1500);
                                startup_log("[ECO] Local telephony seam active (call watcher + poller)");
                            }

                            // Windows toast + reflection for notifications
                            // arriving from peers: show native toasts, and
                            // follow the phone's swipe-away on dismissals.
                            use igrisecosystem::eco::events::EcoEvent;
                            let toasts_bus = event_bus.clone();
                            let toasts_handler = std::sync::Arc::new(move |event: EcoEvent| {
                                match event {
                                    EcoEvent::NotificationReceived(notif, from) => {
                                        if from == "remote" {
                                            igrisecosystem::platform::ecosystem::notifications::toasts::show_toast(&notif);
                                        }
                                    }
                                    EcoEvent::NotificationDismissed(payload) => {
                                        igrisecosystem::platform::ecosystem::notifications::toasts::hide_toast(&payload.notification_key);
                                    }
                                    _ => {}
                                }
                            });
                            toasts_bus.subscribe(toasts_handler);
                        }
                    }

                    // Files must be receivable without opening the File Share
                    // tab first — a phone sending to this desktop otherwise
                    // hits refused ports.
                    crate::fastswap::start_on_demand().await;
                    startup_log("[ECO] FastSwap live on ports 53317/53318");
                    update_mesh_status("Mesh online");
                }
                Err(e) => {
                    startup_log(&format!("[ECO] Start failed: {}", e));
                    update_mesh_status(&format!("Mesh start failed: {}", e));
                }
            }
        }
        Err(e) => {
            startup_log(&format!("[ECO] Init failed: {}", e));
            update_mesh_status(&format!("Mesh init failed: {}", e));
        }
    }
}

// ---------------------------------------------------------------------------
// UI
// ---------------------------------------------------------------------------

#[component]
fn App() -> Element {
    let show_settings = use_signal(|| false);
    let mut show_fastswap = use_signal(|| false);
    let mut pending_transfers = use_signal(|| Vec::<fastswap::PendingTransfer>::new());
    let current_tab = use_signal(|| Tab::Devices);

    // Lightweight polling loop: only refreshes FastSwap transfers.
    // No AI state, no search, no camera, no voice polling.
    use_effect(move || {
        spawn(async move {
            loop {
                async_std::task::sleep(std::time::Duration::from_millis(1000)).await;

                let pending = fastswap::get_pending_transfers().await;
                pending_transfers.set(pending);
            }
        });
    });

    // Mesh colors — always the calm cyan/blue palette (no awake/standby toggle).
    let primary_color = "#06b6d4";
    let _secondary_color = "#3b82f6";
    let glow_color = "rgba(34, 211, 238, 0.8)";
    let accent_rgb = "34, 211, 238";

    rsx! {
        style { "{Theme::base_css()}" }
        style { r#"
            * {{ margin: 0; padding: 0; box-sizing: border-box; }}
            ::-webkit-scrollbar {{ width: 0px; height: 0px; }}
            * {{ scrollbar-width: none; }}
            * {{ -ms-overflow-style: none; }}
            .color-transition {{ transition: color 0.3s ease-in-out, background-color 0.3s ease-in-out, border-color 0.3s ease-in-out; }}

            @keyframes spin {{ from {{ transform: rotate(0deg); }} to {{ transform: rotate(360deg); }} }}
            @keyframes pulse {{ 0%, 100% {{ opacity: 0.4; transform: scale(1); }} 50% {{ opacity: 0.8; transform: scale(1.05); }} }}
            @keyframes scanline {{
                0% {{ transform: translateY(-100%); }}
                100% {{ transform: translateY(100vh); }}
            }}
            @keyframes grid-scroll {{
                0% {{ transform: translateY(0); }}
                100% {{ transform: translateY(32px); }}
            }}
            @keyframes hud-flicker {{
                0%, 100% {{ opacity: 1; }}
                50% {{ opacity: 0.97; }}
            }}
            @keyframes bracket-pulse {{
                0%, 100% {{ opacity: 0.5; }}
                50% {{ opacity: 1; }}
            }}
            @keyframes glow-breath {{
                0%, 100% {{ filter: blur(8px); opacity: 0.3; }}
                50% {{ filter: blur(12px); opacity: 0.5; }}
            }}
        "# }

        SettingsPanel { is_open: show_settings }

        a {
            href: "#igris-main",
            class: "igris-focus",
            style: "position: absolute; left: -9999px; top: 8px; z-index: 200; background: #22D3EE; color: #050A14; padding: 8px 16px; border-radius: 6px; font-family: monospace;",
            "Skip to main content"
        }

        if show_fastswap() && current_tab() != Tab::FastSwap {
            div {
                style: "position: fixed; top: 0; left: 0; width: 100vw; height: 100vh; z-index: 100; background: rgba(0,0,0,0.5); backdrop-filter: blur(4px);",
                onclick: move |_| show_fastswap.set(false),
                div {
                    style: "position: absolute; top: 50%; left: 50%; transform: translate(-50%, -50%); width: 90%; max-width: 800px; max-height: 90vh; overflow: auto; border-radius: 20px; box-shadow: 0 20px 60px rgba(0,0,0,0.5);",
                    onclick: move |e| e.stop_propagation(),
                    FastSwapPanel {}
                }
            }
        }

        PresentationPanel {}

        div { aria_live: "polite",
            IncomingTransferPopup { pending_transfers }
        }

        // Main layout with sidebar
        div { style: "width: 100vw; height: 100vh; display: flex; background: #050a14; color: #fff; font-family: 'Inter', sans-serif; position: relative; overflow: hidden;",

            // Sidebar
            Sidebar {
                active_tab: current_tab,
                primary_color: primary_color,
                accent_rgb: accent_rgb,
            }

            // Main content area
            div { style: "flex: 1; display: flex; flex-direction: column; position: relative; overflow: hidden; background: #050a14;",

                // Grid overlay
                div { style: "position: absolute; inset: 0; pointer-events: none; z-index: 0; opacity: 0.03;",
                    div { style: "width: 100%; height: 100%; background-image: linear-gradient(rgba(255,255,255,0.3) 1px, transparent 1px), linear-gradient(90deg, rgba(255,255,255,0.3) 1px, transparent 1px); background-size: 32px 32px; animation: grid-scroll 4s linear infinite;" }
                }

                // Scan line overlay
                div { style: "position: absolute; inset: 0; pointer-events: none; z-index: 1; overflow: hidden; opacity: 0.04;",
                    div { style: "width: 100%; height: 2px; background: linear-gradient(90deg, transparent, rgba(255,255,255,0.5), transparent); animation: scanline 4s linear infinite;" }
                }

                // Accent gradient glow in top-right
                div { style: format!("position: absolute; top: -200px; right: -200px; width: 500px; height: 500px; border-radius: 50%; background: radial-gradient(circle, rgba({}, 0.06), transparent 70%); pointer-events: none; z-index: 0;", accent_rgb) }
                div { style: format!("position: absolute; bottom: -150px; left: -150px; width: 400px; height: 400px; border-radius: 50%; background: radial-gradient(circle, rgba({}, 0.04), transparent 70%); pointer-events: none; z-index: 0;", accent_rgb) }

                // Top bar
                div { style: format!("position: relative; z-index: 10; display: flex; align-items: center; justify-content: space-between; padding: 14px 24px; border-bottom: 1px solid rgba({}, 0.08); flex-shrink: 0; background: rgba(5,10,20,0.8); backdrop-filter: blur(8px);", accent_rgb),
                    // Corner brackets on top bar
                    div { style: format!("position: absolute; bottom: -1px; left: 0; width: 20px; height: 8px; border-left: 1px solid rgba({}, 0.2); border-bottom: 1px solid rgba({}, 0.2);", accent_rgb, accent_rgb) }
                    div { style: format!("position: absolute; bottom: -1px; right: 0; width: 20px; height: 8px; border-right: 1px solid rgba({}, 0.2); border-bottom: 1px solid rgba({}, 0.2);", accent_rgb, accent_rgb) }

                    div { style: "display: flex; align-items: center; gap: 14px;",
                        div { style: format!("font-size: 11px; font-weight: 700; letter-spacing: 2px; font-family: 'JetBrains Mono', monospace; color: {}; text-shadow: 0 0 20px {};", primary_color, glow_color),
                            "// IGRIS"
                        }
                        span { style: "font-size: 9px; color: rgba(255,255,255,0.12); font-family: monospace; letter-spacing: 0.5px;", "v1.0.0_eco" }
                    }
                    MenuButton {
                        settings_open: show_settings,
                        fastswap_open: show_fastswap,
                    }
                }

                // Content area based on active tab
                div { style: "flex: 1; overflow: hidden; display: flex; position: relative; z-index: 5;",
                    match current_tab() {
                        Tab::FastSwap => rsx! {
                            div { style: "flex: 1; overflow-y: auto; padding: 24px;",
                                FastSwapPanel {}
                            }
                        },
                        Tab::Devices => rsx! {
                            EcoDevicePanel {
                                primary_color,
                                accent_rgb,
                            }
                        },
                        Tab::Notifications => rsx! {
                            NotificationPanel {
                                primary_color,
                                accent_rgb,
                            }
                        },
                        Tab::Calls => rsx! {
                            CallsPanel {
                                primary_color,
                                accent_rgb,
                            }
                        },
                        Tab::Alarms | Tab::Reminders => rsx! {
                            AlarmReminderPanel {
                                primary_color,
                                accent_rgb,
                            }
                        },
                        Tab::SystemInfo => rsx! {
                            SystemInfoPanel {
                                primary_color,
                                accent_rgb,
                            }
                        },
                        // Catch-all for any removed tabs (Dashboard, Chat) that
                        // might still exist in the enum — route to Devices.
                        _ => rsx! {
                            EcoDevicePanel {
                                primary_color,
                                accent_rgb,
                            }
                        },
                    }
                }
            }
        }
    }
}
