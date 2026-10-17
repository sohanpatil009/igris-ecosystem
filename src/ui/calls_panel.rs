use dioxus::prelude::*;
use crate::eco::call::{get_calls, CallData};
use crate::eco::discovery::ECO_NETWORK_DEVICES;

/// Desktop calls panel: watch-style remote control. Dial from the desk,
/// accept/decline/mute ringing calls, browse the call log. Audio stays on
/// the phone — the desk sends control actions, the phone drives telecom.
#[component]
pub fn CallsPanel(primary_color: String, accent_rgb: String) -> Element {
    let mut calls = use_signal(|| Vec::<CallData>::new());
    let mut peers = use_signal(|| Vec::<(String, String)>::new());
    let mut number = use_signal(String::new);
    let mut status = use_signal(|| String::new());
    let mut sending = use_signal(|| false);
    let mut contact_query = use_signal(String::new);
    let mut contact_hits =
        use_signal(|| Vec::<crate::platform::ecosystem::ContactEntry>::new());
    let mut sms_to = use_signal(String::new);
    let mut sms_body = use_signal(String::new);

    use_effect(move || {
        spawn(async move {
            loop {
                calls.set(get_calls());
                let net = ECO_NETWORK_DEVICES.read().await;
                peers.set(
                    net.iter()
                        .filter(|d| d.is_trusted && d.is_online)
                        .map(|d| (d.id.clone(), d.name.clone()))
                        .collect(),
                );
                drop(net);
                async_std::task::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    });

    // Debounced phonebook search against the first linked peer (2+ chars).
    use_effect(move || {
        let q = contact_query();
        spawn(async move {
            if q.trim().len() < 2 {
                contact_hits.set(Vec::new());
                return;
            }
            async_std::task::sleep(std::time::Duration::from_millis(400)).await;
            if contact_query() != q {
                return;
            }
            let peer_id = peers().first().map(|(id, _)| id.clone());
            let Some(peer_id) = peer_id else { return };
            let hits = if let Ok(guard) = crate::eco::manager::ECO_MANAGER.lock() {
                if let Some(ref manager) = *guard {
                    manager.search_peer_contacts(&peer_id, q.trim()).await.unwrap_or_default()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };
            if contact_query() == q {
                contact_hits.set(hits);
            }
        });
    });

    let do_action = move |(device_id, call_id, action, num): (String, String, String, String)| {
        sending.set(true);
        status.set(String::new());
        spawn(async move {
            let result = if let Ok(guard) = crate::eco::manager::ECO_MANAGER.lock() {
                if let Some(ref manager) = *guard {
                    manager
                        .send_call_action(&device_id, &call_id, &action, &num)
                        .await
                        .map(|_| "Sent ✓".to_string())
                        .map_err(|e| e.to_string())
                } else {
                    Err("Mesh not initialized".to_string())
                }
            } else {
                Err("Mesh lock failed".to_string())
            };
            match result {
                Ok(msg) => status.set(msg),
                Err(e) => status.set(format!("Failed: {}", e)),
            }
            sending.set(false);
        });
    };

    let do_sms = move |(device_id, to, body): (String, String, String)| {
        sending.set(true);
        status.set(String::new());
        spawn(async move {
            let result = if let Ok(guard) = crate::eco::manager::ECO_MANAGER.lock() {
                if let Some(ref manager) = *guard {
                    manager
                        .send_sms_message(&device_id, &to, &body)
                        .await
                        .map(|_| "SMS sent ✓".to_string())
                        .map_err(|e| e.to_string())
                } else {
                    Err("Mesh not initialized".to_string())
                }
            } else {
                Err("Mesh lock failed".to_string())
            };
            match result {
                Ok(msg) => {
                    sms_body.set(String::new());
                    status.set(msg);
                }
                Err(e) => status.set(format!("Failed: {}", e)),
            }
            sending.set(false);
        });
    };

    let ringing: Vec<CallData> = calls()
        .into_iter()
        .filter(|c| c.event == "ringing" && !c.is_cancel)
        .collect();
    let log: Vec<CallData> = {
        let mut all = get_calls();
        all.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        all.into_iter().take(30).collect()
    };
    let peer_count = peers().len();
    let local_id = crate::eco::pairing::get_local_device_id().unwrap_or_default();
    // Dial through the first linked peer; fall back to the desktop's own
    // phone line when no peer is linked.
    let (dial_device, dial_via) = if peer_count > 0 {
        (peers()[0].0.clone(), peers()[0].1.clone())
    } else if !local_id.is_empty() {
        (local_id, "LOCAL LINE".to_string())
    } else {
        (String::new(), "NO LINKED PEER".to_string())
    };

    rsx! {
        section {
            aria_label: "Calls",
            style: "padding: 24px 32px; height: 100%; overflow-y: auto;",
            h2 { class: "igris-heading", style: format!("font-size: 14px; font-weight: 700; color: {}; letter-spacing: 2px; font-family: 'JetBrains Mono', monospace; margin-bottom: 8px;", primary_color),
                "// CALLS"
            }
            div { style: "font-size: 11px; color: rgba(255,255,255,0.35); font-family: monospace; margin-bottom: 16px;",
                "Dial and control from the desk — audio stays on the phone."
            }

            // Ringing banner
            for call in ringing.iter() {
                div {
                    key: "{call.call_id}",
                    role: "alert",
                    style: format!("padding: 16px 20px; border-radius: 10px; margin-bottom: 12px; background: rgba({}, 0.08); border: 1px solid rgba({}, 0.3); display: flex; align-items: center; gap: 16px;", accent_rgb, accent_rgb),
                    div { style: "flex: 1; min-width: 0;",
                        div { class: "igris-truncate", style: "font-size: 15px; font-weight: 700; color: #F1F5F9;",
                            "Incoming: {call.contact_name}"
                        }
                        div { class: "igris-truncate igris-num", style: "font-size: 12px; color: rgba(255,255,255,0.4); font-family: monospace;",
                            "{call.phone_number} · via {call.device_name}"
                        }
                    }
                    button {
                        class: "igris-btn igris-focus",
                        aria_label: "Accept call",
                        style: "padding: 8px 18px; border-radius: 6px; background: rgba(163,230,53,0.15); color: #A3E635; border: 1px solid rgba(163,230,53,0.3); cursor: pointer; font-family: monospace;",
                        onclick: {
                            let d = (call.device_id.clone(), call.call_id.clone(), "accept".to_string(), String::new());
                            let mut do_action = do_action.clone();
                            move |_| do_action(d.clone())
                        },
                        "ACCEPT"
                    }
                    button {
                        class: "igris-btn igris-focus",
                        aria_label: "Decline call",
                        style: "padding: 8px 18px; border-radius: 6px; background: rgba(251,113,133,0.15); color: #FB7185; border: 1px solid rgba(251,113,133,0.3); cursor: pointer; font-family: monospace;",
                        onclick: {
                            let d = (call.device_id.clone(), call.call_id.clone(), "decline".to_string(), String::new());
                            let mut do_action = do_action.clone();
                            move |_| do_action(d.clone())
                        },
                        "DECLINE"
                    }
                }
            }

            // Dial pad
            div { style: format!("padding: 16px 20px; border-radius: 10px; margin-bottom: 12px; background: rgba(10,17,40,0.85); border: 1px solid rgba(255,255,255,0.06);"),
                div { style: "font-size: 11px; letter-spacing: 2px; color: rgba(255,255,255,0.4); font-family: monospace; margin-bottom: 10px;",
                    "DIAL VIA {dial_via}"
                }
                div { style: "display: flex; gap: 8px;",
                    input {
                        class: "igris-focus",
                        style: "flex: 1; min-width: 0; padding: 10px 14px; border-radius: 6px; background: rgba(0,0,0,0.3); border: 1px solid rgba(255,255,255,0.12); color: #F1F5F9; font-family: monospace; font-size: 14px;",
                        value: number(),
                        oninput: move |e: Event<FormData>| number.set(e.value()),
                        onkeydown: {
                            let num_str = number();
                            let dial_dev = dial_device.clone();
                            let mut do_action = do_action.clone();
                            move |e: Event<KeyboardData>| {
                                if e.key() == Key::Enter && !num_str.trim().is_empty() && !dial_dev.is_empty() {
                                    let d = (dial_dev.clone(), uuid::Uuid::new_v4().to_string(), "dial".to_string(), num_str.trim().to_string());
                                    do_action(d)
                                }
                            }
                        },
                        placeholder: "+91…",
                        aria_label: "Phone number",
                    }
                    button {
                        class: "igris-btn igris-focus",
                        aria_label: "Dial",
                        disabled: number().trim().is_empty() || dial_device.is_empty() || sending(),
                        style: format!("padding: 10px 22px; border-radius: 6px; background: rgba({}, 0.15); color: rgba({}, 0.95); border: 1px solid rgba({}, 0.3); cursor: pointer; font-family: monospace; font-weight: 700;", accent_rgb, accent_rgb, accent_rgb),
                        onclick: {
                            let num_str = number();
                            let dial_dev = dial_device.clone();
                            let mut do_action = do_action.clone();
                            move |_| {
                                if num_str.trim().is_empty() || dial_dev.is_empty() { return; }
                                let d = (dial_dev.clone(), uuid::Uuid::new_v4().to_string(), "dial".to_string(), num_str.trim().to_string());
                                do_action(d)
                            }
                        },
                        if sending() { "…" } else { "DIAL" }
                    }
                }
                if !status().is_empty() {
                    div { role: "status", style: "font-size: 11px; font-family: monospace; margin-top: 8px; color: rgba(255,255,255,0.6);",
                        "{status}"
                    }
                }
            }

            // Phonebook search (name or number, like the native dialer).
            div { style: format!("padding: 16px 20px; border-radius: 10px; margin-bottom: 12px; background: rgba(10,17,40,0.85); border: 1px solid rgba(255,255,255,0.06);"),
                div { style: "font-size: 11px; letter-spacing: 2px; color: rgba(255,255,255,0.4); font-family: monospace; margin-bottom: 10px;",
                    "SEARCH PHONEBOOK"
                }
                input {
                    class: "igris-focus",
                    style: "width: 100%; min-width: 0; padding: 10px 14px; border-radius: 6px; background: rgba(0,0,0,0.3); border: 1px solid rgba(255,255,255,0.12); color: #F1F5F9; font-family: monospace; font-size: 13px; box-sizing: border-box;",
                    value: contact_query(),
                    oninput: move |e: Event<FormData>| contact_query.set(e.value()),
                    placeholder: "Name or number…",
                    aria_label: "Search phonebook",
                }
                for hit in contact_hits().iter() {
                    button {
                        key: "hit-{hit.number}",
                        class: "igris-btn igris-focus",
                        aria_label: "Dial {hit.name}",
                        style: "display: flex; width: 100%; box-sizing: border-box; gap: 12px; align-items: baseline; margin-top: 8px; padding: 10px 14px; border-radius: 6px; background: rgba(0,0,0,0.25); border: 1px solid rgba(255,255,255,0.08); cursor: pointer; text-align: left;",
                        onclick: {
                            let num = hit.number.clone();
                            move |_| number.set(num.clone())
                        },
                        span { class: "igris-truncate", style: "flex: 1; font-size: 13px; color: #e5e7eb;",
                            "{hit.name}"
                        }
                        span { class: "igris-num", style: "font-size: 12px; color: rgba(255,255,255,0.45); font-family: monospace;",
                            "{hit.number}"
                        }
                    }
                }
            }

            // SMS compose ( Continuity-style: type on desk, phone transmits ).
            div { style: format!("padding: 16px 20px; border-radius: 10px; margin-bottom: 12px; background: rgba(10,17,40,0.85); border: 1px solid rgba(255,255,255,0.06);"),
                div { style: "font-size: 11px; letter-spacing: 2px; color: rgba(255,255,255,0.4); font-family: monospace; margin-bottom: 10px;",
                    "SEND SMS VIA LINKED PHONE"
                }
                input {
                    class: "igris-focus",
                    style: "width: 100%; min-width: 0; padding: 10px 14px; border-radius: 6px; background: rgba(0,0,0,0.3); border: 1px solid rgba(255,255,255,0.12); color: #F1F5F9; font-family: monospace; font-size: 13px; box-sizing: border-box; margin-bottom: 8px;",
                    value: sms_to(),
                    oninput: move |e: Event<FormData>| sms_to.set(e.value()),
                    placeholder: "To: number…",
                    aria_label: "SMS recipient",
                }
                input {
                    class: "igris-focus",
                    style: "width: 100%; min-width: 0; padding: 10px 14px; border-radius: 6px; background: rgba(0,0,0,0.3); border: 1px solid rgba(255,255,255,0.12); color: #F1F5F9; font-family: monospace; font-size: 13px; box-sizing: border-box; margin-bottom: 8px;",
                    value: sms_body(),
                    oninput: move |e: Event<FormData>| sms_body.set(e.value()),
                    onkeydown: {
                        let mut do_sms = do_sms.clone();
                        move |e: Event<KeyboardData>| {
                            if e.key() == Key::Enter && !sms_to().trim().is_empty() && !sms_body().trim().is_empty() && !peers().is_empty() {
                                let d = (peers()[0].0.clone(), sms_to().trim().to_string(), sms_body().trim().to_string());
                                do_sms(d)
                            }
                        }
                    },
                    placeholder: "Message…",
                    aria_label: "SMS body",
                }
                button {
                    class: "igris-btn igris-focus",
                    aria_label: "Send SMS",
                    disabled: sms_to().trim().is_empty() || sms_body().trim().is_empty() || peer_count == 0 || sending(),
                    style: format!("padding: 10px 22px; border-radius: 6px; background: rgba({}, 0.15); color: rgba({}, 0.95); border: 1px solid rgba({}, 0.3); cursor: pointer; font-family: monospace; font-weight: 700;", accent_rgb, accent_rgb, accent_rgb),
                    onclick: move |_| {
                        if sms_to().trim().is_empty() || sms_body().trim().is_empty() || peers().is_empty() { return; }
                        let mut do_sms = do_sms.clone();
                        let d = (peers()[0].0.clone(), sms_to().trim().to_string(), sms_body().trim().to_string());
                        do_sms(d)
                    },
                    if sending() { "…" } else { "SEND SMS" }
                }
            }

            // Call log
            div { style: "font-size: 11px; letter-spacing: 2px; color: rgba(255,255,255,0.4); font-family: monospace; margin: 16px 0 8px;",
                "LOG"
            }
            if log.is_empty() {
                div { style: "font-size: 12px; color: rgba(255,255,255,0.25); font-family: monospace; padding: 24px 0; text-align: center;",
                    "No calls yet"
                }
            }
            for call in log.iter() {
                div { key: "{call.call_id}-{call.event}", style: "display: flex; gap: 12px; align-items: baseline; padding: 8px 0; border-bottom: 1px solid rgba(255,255,255,0.04); min-width: 0;",
                    span { style: format!("font-size: 10px; font-family: monospace; letter-spacing: 1px; color: {};",
                        match call.event.as_str() {
                            "missed" => "#FB7185",
                            "ringing" => "#FBBF24",
                            _ => "#A3E635",
                        }),
                        "{call.event}"
                    }
                    span { class: "igris-truncate", style: "flex: 1; font-size: 13px; color: #e5e7eb;",
                        "{call.contact_name} {call.phone_number}"
                    }
                    if call.event == "talking" {
                        button {
                            class: "igris-focus",
                            aria_label: "End call",
                            style: "font-size: 10px; font-family: monospace; background: transparent; border: 1px solid rgba(251,113,133,0.3); color: #FB7185; border-radius: 4px; padding: 2px 10px; cursor: pointer;",
                            onclick: {
                                let d = (call.device_id.clone(), call.call_id.clone(), "end".to_string(), String::new());
                                let mut do_action = do_action.clone();
                                move |_| do_action(d.clone())
                            },
                            "END"
                        }
                    }
                }
            }
        }
    }
}
