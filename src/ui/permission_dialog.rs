// src/ui/permission_dialog.rs - SENTINEL permission approval dialog (UI stub).
//
// SENTINEL mapping (master Sec34 header strip + Sec11 approval semantics):
// surfaces one pending elevated-action request with Action / Location /
// Risk / Reason, plus the four Sec11 choices:
// Allow Once / Allow For Task / Deny / Always Deny.
//
// SECURITY BOUNDARY NOTE: this component is display + intent-capture ONLY.
// Enforcement lives in the backend (`igris-core` types/events: RiskLevel,
// EventKind::PermissionRequest, CapabilityToken, CancellationNode). The GUI
// must never gate, grant, or bypass approvals itself; `on_decision` only
// forwards the user's intent to the backend approval queue, which validates
// task/device/tool bindings deny-by-default.
//
// VOICE APPROVAL RULE: a voice transcript counts as approval for a pending
// request only via `validate_voice_approval` below, which requires BOTH an
// explicit decision phrase AND a reference to the pending action context.
// Bare affirmations ("yes", "okay", "approve it") with no action reference
// return `None` -- arbitrary speech is never approval.
//
// TODO Phase 12 (full SENTINEL dashboard):
// - Populate `PendingPermissionRequest` from
//   `igris-core::EventKind::PermissionRequest` + action/location metadata.
// - Route `PermissionDecision` into the backend approval queue that issues /
//   denies short-lived `CapabilityToken`s; surface token expiry in the UI.
// - Subscribe the dialog queue to the backend event bus instead of props.

use dioxus::prelude::*;
pub use igris_core::RiskLevel;

/// Backwards-compat alias: the UI now uses `igris_core::RiskLevel` directly
/// (master Sec10: SAFE < LOW < MEDIUM < HIGH < CRITICAL). MEDIUM+ needs
/// explicit approval; HIGH/CRITICAL additionally need a bound capability
/// token (backend). Wire labels stay `SAFE`/`LOW`/`MEDIUM`/`HIGH`/`CRITICAL`
/// via `RiskLevel::as_str` / `Display` / serde renames.
pub type RiskLevelDisplay = RiskLevel;

/// UI-only badge colors for [`RiskLevel`] on the dark HUD.
/// Kept out of `igris-core` (display concern, not security semantics).
pub trait RiskBadgeColors {
    /// (text, background, border) badge colors on the dark HUD.
    fn badge_colors(self) -> (&'static str, &'static str, &'static str);
}

impl RiskBadgeColors for RiskLevel {
    fn badge_colors(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Safe => ("#22c55e", "rgba(34,197,94,0.12)", "rgba(34,197,94,0.3)"),
            Self::Low => ("#38bdf8", "rgba(56,189,248,0.12)", "rgba(56,189,248,0.3)"),
            Self::Medium => ("#f59e0b", "rgba(245,158,11,0.12)", "rgba(245,158,11,0.3)"),
            Self::High => ("#f97316", "rgba(249,115,22,0.14)", "rgba(249,115,22,0.35)"),
            Self::Critical => ("#ef4444", "rgba(239,68,68,0.14)", "rgba(239,68,68,0.4)"),
        }
    }
}

/// SENTINEL Sec11 approval choices surfaced in the GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PermissionChoice {
    AllowOnce,
    AllowForTask,
    Deny,
    AlwaysDeny,
}

impl PermissionChoice {
    pub fn label(self) -> &'static str {
        match self {
            Self::AllowOnce => "ALLOW ONCE",
            Self::AllowForTask => "FOR TASK",
            Self::Deny => "DENY",
            Self::AlwaysDeny => "ALWAYS DENY",
        }
    }
}

/// GUI decision forwarded to the backend approval queue (NOT an enforcement).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionDecision {
    pub request_id: String,
    pub choice: PermissionChoice,
}

/// Pending permission request (display model only).
/// `risk` is `igris_core::RiskLevel` (wire labels SAFE/LOW/MEDIUM/HIGH/CRITICAL).
/// TODO Phase 12: populate from `igris-core::EventKind::PermissionRequest`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPermissionRequest {
    pub request_id: String,
    pub task_id: String,
    /// Tool/action, e.g. `clipboard.push`.
    pub action: String,
    /// Where the action would execute (device / path / URL).
    pub location: String,
    pub risk: RiskLevel,
    /// Why the tool requested elevation (Sec11 reason).
    pub reason: String,
    /// True while the backend is blocked waiting for this decision.
    /// Only awaiting requests are actionable, so a decision can never
    /// approve the wrong (stale) session.
    pub awaiting_approval: bool,
}

impl PendingPermissionRequest {
    pub fn is_high_stakes(&self) -> bool {
        self.risk.requires_capability_token()
    }
}

/// Validate a voice transcript as approval for a *specific* pending request.
///
/// Returns the mapped choice only when the transcript BOTH references the
/// pending action context AND contains an explicit decision phrase:
/// - "always deny ..." / "never allow ..." -> `AlwaysDeny`
/// - "... for this task" + approval -> `AllowForTask`
/// - approval words ("allow", "approve", "grant", ...) -> `AllowOnce`
/// - denial words ("deny", "reject", "refuse", ...) -> `Deny`
///
/// Anything else -- including bare "yes" / "okay" with no action reference,
/// or transcripts for a different action -- returns `None`.
pub fn validate_voice_approval(
    transcript: &str,
    request: &PendingPermissionRequest,
) -> Option<PermissionChoice> {
    let said = transcript.to_ascii_lowercase();

    // Context gate: at least one significant action token must be named.
    // ("clipboard.push" -> ["clipboard", "push"]; tokens < 3 chars ignored.)
    let context_ok = request
        .action
        .split(['.', '/', '_', '-', ' ', ':'])
        .filter(|t| t.len() >= 3)
        .any(|token| said.contains(&token.to_ascii_lowercase()));
    if !context_ok {
        return None;
    }

    let contains_any = |words: &[&str]| words.iter().any(|w| said.contains(w));

    // Strongest denial first so "always deny X" never falls through to allow.
    if contains_any(&["always deny", "never allow", "deny always", "block forever"]) {
        return Some(PermissionChoice::AlwaysDeny);
    }
    if contains_any(&["for this task", "for the task", "for task"])
        && contains_any(&["allow", "approve", "grant", "permit"])
    {
        return Some(PermissionChoice::AllowForTask);
    }
    if contains_any(&["allow", "approve", "grant", "permit"]) {
        // Guard: a bare "don't allow" / "do not approve" is a denial.
        if contains_any(&[
            "don't allow",
            "do not allow",
            "do not approve",
            "don't approve",
        ]) {
            return Some(PermissionChoice::Deny);
        }
        return Some(PermissionChoice::AllowOnce);
    }
    if contains_any(&["deny", "reject", "refuse", "block it", "don't do"]) {
        return Some(PermissionChoice::Deny);
    }
    None
}

/// SENTINEL permission dialog: Action / Location / Risk / Reason (Sec11) +
/// the four approval choices. Forwards intent via `on_decision`; the backend
/// validates and enforces (UI is never the security boundary).
#[component]
pub fn PermissionDialog(
    request: PendingPermissionRequest,
    on_decision: EventHandler<PermissionDecision>,
    primary_color: String,
    accent_rgb: String,
) -> Element {
    let (risk_text, risk_bg, risk_border) = request.risk.badge_colors();
    let risk_label = request.risk.as_str();

    let req_once = request.request_id.clone();
    let req_task = request.request_id.clone();
    let req_deny = request.request_id.clone();
    let req_always = request.request_id.clone();
    let on_once = on_decision.clone();
    let on_task = on_decision.clone();
    let on_deny = on_decision.clone();
    let on_always = on_decision.clone();

    rsx! {
        div {
            style: "position: fixed; top: 0; left: 0; width: 100vw; height: 100vh; z-index: 9998; background: rgba(0,0,0,0.8); backdrop-filter: blur(8px); display: flex; align-items: center; justify-content: center;",
            div {
                role: "alertdialog",
                title: "Permission request awaiting your decision",
                style: format!(
                    "width: 90%; max-width: 520px; background: linear-gradient(135deg, rgba(8,12,28,0.98), rgba(4,8,18,0.99)); border: 1px solid rgba({}, 0.35); border-radius: 12px; padding: 28px; box-shadow: 0 20px 60px rgba(0,0,0,0.6); font-family: 'JetBrains Mono', monospace;",
                    accent_rgb
                ),
                div { style: format!("font-size: 13px; font-weight: 700; letter-spacing: 2px; color: {}; margin-bottom: 4px;", primary_color),
                    "// PERMISSION REQUEST"
                }
                div { style: "font-size: 11px; color: rgba(255,255,255,0.35); margin-bottom: 16px;",
                    "Task {request.task_id}  ::  Req {request.request_id}"
                }

                div { style: "display: flex; flex-direction: column; gap: 8px; margin-bottom: 16px;",
                    div { style: "font-size: 12px; color: rgba(255,255,255,0.55);",
                        span { style: "color: rgba(255,255,255,0.3);", "ACTION :: " }
                        "{request.action}"
                    }
                    div { style: "font-size: 12px; color: rgba(255,255,255,0.55); word-break: break-word;",
                        span { style: "color: rgba(255,255,255,0.3);", "LOCATION :: " }
                        "{request.location}"
                    }
                    div { style: "display: flex; align-items: center; gap: 8px;",
                        span { style: "font-size: 12px; color: rgba(255,255,255,0.3);", "RISK :: " }
                        span { style: format!("padding: 2px 10px; border-radius: 4px; font-size: 10px; letter-spacing: 1px; font-weight: 700; background: {}; color: {}; border: 1px solid {};", risk_bg, risk_text, risk_border),
                            "{risk_label}"
                        }
                        if request.is_high_stakes() {
                            span { style: "font-size: 10px; color: rgba(255,255,255,0.4);",
                                "(capability token required)"
                            }
                        }
                    }
                    div { style: "font-size: 12px; color: rgba(255,255,255,0.55); line-height: 1.5;",
                        span { style: "color: rgba(255,255,255,0.3);", "REASON :: " }
                        "{request.reason}"
                    }
                }

                div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-bottom: 14px;",
                    button {
                        title: "Allow this action one time only",
                        style: "padding: 12px; border-radius: 8px; border: 1px solid rgba(34,197,94,0.4); background: rgba(34,197,94,0.15); color: #22c55e; cursor: pointer; font-size: 12px; font-weight: 700; letter-spacing: 1px; font-family: 'JetBrains Mono', monospace;",
                        onclick: move |_| {
                            on_once.call(PermissionDecision { request_id: req_once.clone(), choice: PermissionChoice::AllowOnce });
                        },
                        "ALLOW ONCE"
                    }
                    button {
                        title: "Allow this action for the whole task",
                        style: "padding: 12px; border-radius: 8px; border: 1px solid rgba(56,189,248,0.4); background: rgba(56,189,248,0.12); color: #38bdf8; cursor: pointer; font-size: 12px; font-weight: 700; letter-spacing: 1px; font-family: 'JetBrains Mono', monospace;",
                        onclick: move |_| {
                            on_task.call(PermissionDecision { request_id: req_task.clone(), choice: PermissionChoice::AllowForTask });
                        },
                        "FOR TASK"
                    }
                    button {
                        title: "Deny this request (cancels the pending tool call)",
                        style: "padding: 12px; border-radius: 8px; border: 1px solid rgba(245,158,11,0.4); background: rgba(245,158,11,0.12); color: #f59e0b; cursor: pointer; font-size: 12px; font-weight: 700; letter-spacing: 1px; font-family: 'JetBrains Mono', monospace;",
                        onclick: move |_| {
                            on_deny.call(PermissionDecision { request_id: req_deny.clone(), choice: PermissionChoice::Deny });
                        },
                        "DENY"
                    }
                    button {
                        title: "Always deny this action in the future",
                        style: "padding: 12px; border-radius: 8px; border: 1px solid rgba(239,68,68,0.4); background: rgba(239,68,68,0.12); color: #ef4444; cursor: pointer; font-size: 12px; font-weight: 700; letter-spacing: 1px; font-family: 'JetBrains Mono', monospace;",
                        onclick: move |_| {
                            on_always.call(PermissionDecision { request_id: req_always.clone(), choice: PermissionChoice::AlwaysDeny });
                        },
                        "ALWAYS DENY"
                    }
                }

                div { style: "font-size: 10px; color: rgba(255,255,255,0.25); line-height: 1.5; text-align: center;",
                    "Voice approval must name the action (e.g. \"allow clipboard.push once\"). Bare \"yes\" is ignored. Enforcement happens in the backend -- this dialog only forwards your intent."
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PendingPermissionRequest {
        PendingPermissionRequest {
            request_id: "req-1".to_string(),
            task_id: "task-1".to_string(),
            action: "clipboard.push".to_string(),
            location: "THIS STATION".to_string(),
            risk: RiskLevel::High,
            reason: "Send clipboard to peer".to_string(),
            awaiting_approval: true,
        }
    }

    #[test]
    fn bare_yes_is_never_approval() {
        let req = fixture();
        for transcript in ["yes", "okay", "approve it", "yeah sure"] {
            assert_eq!(validate_voice_approval(transcript, &req), None);
        }
    }

    #[test]
    fn contextual_approval_maps() {
        let req = fixture();
        assert_eq!(
            validate_voice_approval("allow clipboard push once", &req),
            Some(PermissionChoice::AllowOnce)
        );
        assert_eq!(
            validate_voice_approval("deny the clipboard push", &req),
            Some(PermissionChoice::Deny)
        );
        assert_eq!(
            validate_voice_approval("always deny clipboard push", &req),
            Some(PermissionChoice::AlwaysDeny)
        );
    }

    #[test]
    fn wrong_action_context_rejected() {
        let req = fixture();
        assert_eq!(validate_voice_approval("allow camera capture", &req), None);
    }
}
