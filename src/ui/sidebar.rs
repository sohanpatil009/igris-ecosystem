use dioxus::prelude::*;
use super::theme::Theme;

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    FastSwap,
    Devices,
    Notifications,
    Calls,
    Alarms,
    Reminders,
    SystemInfo,
}

#[component]
pub fn Sidebar(
    active_tab: Signal<Tab>,
    primary_color: String,
    accent_rgb: String,
) -> Element {
    let tabs = [
        (Tab::Devices, "NODE", "Devices"),
        (Tab::FastSwap, "XFER", "File Share"),
        (Tab::Notifications, "NOTIF", "Notifications"),
        (Tab::Calls, "CALL", "Calls"),
        (Tab::Alarms, "TIME", "Alarms"),
        (Tab::Reminders, "MEMO", "Reminders"),
        (Tab::SystemInfo, "SYS", "System"),
    ];

    rsx! {
        style { r#"
            .sib-item {{
                display: flex; align-items: center; gap: 12px; padding: 14px 16px;
                cursor: pointer; transition: background-color 0.2s ease, color 0.2s ease, border-color 0.2s ease;
                position: relative; border-left: 2px solid transparent; color: rgba(255,255,255,0.4);
                font-family: 'JetBrains Mono', 'Courier New', monospace;
                background: transparent; border-top: none; border-right: none; border-bottom: none;
                width: 100%; text-align: left; touch-action: manipulation;
            }}
            .sib-item:hover {{
                background: rgba(255,255,255,0.03); color: rgba(255,255,255,0.7);
            }}
            .sib-item:focus-visible {{ outline: 2px solid #22D3EE; outline-offset: -2px; }}
            .sib-badge {{
                font-size: 9px; letter-spacing: 2px; font-weight: 700; min-width: 42px;
            }}
            .sib-label {{
                font-size: 13px; font-weight: 400; letter-spacing: 0.5px;
            }}
        "# }
        nav {
            aria_label: "Primary",
            style: format!(
                "width: 220px; height: 100vh; background: linear-gradient(180deg, {}, {}); \
                 border-right: 1px solid {}; display: flex; flex-direction: column; \
                 flex-shrink: 0; z-index: 30; transition: border-color 0.3s ease; \
                 box-shadow: 4px 0 30px rgba(0,0,0,0.5);",
                Theme::BG_RAISED, Theme::BG_BASE, Theme::BORDER,
            ),
            div { style: format!("padding: 28px 20px 18px; border-bottom: 1px solid {}; margin-bottom: 12px;", Theme::BORDER),
                div { style: format!("font-size: 10px; font-weight: 700; color: {}; letter-spacing: 3px; transition: color 0.3s ease; font-family: 'JetBrains Mono', monospace;", primary_color),
                    "// SYSTEM NAV"
                }
                div { style: "font-size: 9px; color: rgba(255,255,255,0.15); letter-spacing: 1px; margin-top: 4px; font-family: monospace;",
                    ">> v1.0.0_eco"
                }
            }
            div { style: "display: flex; flex-direction: column; gap: 2px; padding: 4px 8px; flex: 1; overflow-y: auto;",
                for (tab, badge, label) in tabs {
                    button {
                        class: "sib-item igris-focus",
                        aria_label: label,
                        aria_current: if active_tab() == tab { "page" } else { "false" },
                        style: format!(
                            "background: {}; {}",
                            if active_tab() == tab { format!("rgba({}, 0.08)", accent_rgb) } else { "transparent".to_string() },
                            if active_tab() == tab {
                                format!("border-left: 2px solid {}; color: {};", primary_color, primary_color)
                            } else {
                                "".to_string()
                            }
                        ),
                        onclick: move |_| active_tab.set(tab),
                        span { class: "sib-badge", aria_hidden: "true", "{badge}" }
                        span { class: "sib-label igris-truncate", "{label}" }
                        if active_tab() == tab {
                            div { aria_hidden: "true", style: format!("position: absolute; right: 12px; width: 4px; height: 4px; border-radius: 50%; background: {}; box-shadow: 0 0 8px {}; animation: igris-pulse-dot 1.5s ease-in-out infinite;", primary_color, primary_color) }
                        }
                    }
                }
            }
            div { style: format!("padding: 16px 20px; border-top: 1px solid {};", Theme::BORDER),
                div { style: "display: flex; align-items: center; gap: 10px; font-size: 10px; color: rgba(255,255,255,0.3); font-family: monospace;",
                    div {
                        aria_hidden: "true",
                        style: format!(
                            "width: 6px; height: 6px; border-radius: 50%; background: {}; \
                             box-shadow: 0 0 10px {}; animation: igris-pulse-dot 2s ease-in-out infinite; transition: background-color 0.3s ease;",
                            &primary_color,
                            &primary_color,
                        ),
                    },
                    "STATUS: MESH ACTIVE"
                }
            }
        }
    }
}
