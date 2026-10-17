//! Igris Obsidian Signal theme — centralized enterprise tokens.
//!
//! Deep navy surfaces, cyan signal primary, violet awake accent,
//! lime trust, amber pending, rose danger. All panels consume these
//! instead of hardcoded hex so the UI stays consistent.

pub struct Theme;

impl Theme {
    pub const BG_BASE: &'static str = "#050A14";
    pub const BG_RAISED: &'static str = "#0A1128";
    pub const BG_CARD: &'static str = "rgba(10,17,40,0.85)";
    pub const BORDER: &'static str = "rgba(255,255,255,0.06)";
    pub const BORDER_STRONG: &'static str = "rgba(255,255,255,0.12)";

    pub const TEXT_PRIMARY: &'static str = "#F1F5F9";
    pub const TEXT_SECONDARY: &'static str = "#94A3B8";
    pub const TEXT_MUTED: &'static str = "#475569";

    pub const SIGNAL: &'static str = "#22D3EE";
    pub const SIGNAL_RGB: &'static str = "34, 211, 238";
    pub const AWAKE: &'static str = "#A855F7";
    pub const AWAKE_RGB: &'static str = "168, 85, 247";

    pub const TRUST: &'static str = "#A3E635";
    pub const TRUST_DIM: &'static str = "rgba(163,230,53,0.12)";
    pub const PENDING: &'static str = "#FBBF24";
    pub const DANGER: &'static str = "#FB7185";

    pub const RADIUS_SM: &'static str = "6px";
    pub const RADIUS_MD: &'static str = "10px";
    pub const RADIUS_LG: &'static str = "16px";

    pub const FONT_UI: &'static str = "'Inter', system-ui, sans-serif";
    pub const FONT_MONO: &'static str = "'JetBrains Mono', ui-monospace, monospace";

    /// Shared focus ring (keyboard). Use on all buttons/inputs.
    pub const FOCUS_RING: &'static str =
        "outline: 2px solid #22D3EE; outline-offset: 2px;";

    /// Base stylesheet injected once from `main.rs`. Compositor-only
    /// animations, explicit transition properties, reduced-motion guard,
    /// dark color-scheme, tabular numbers, balanced headings.
    pub fn base_css() -> String {
        r#"
            :root { color-scheme: dark; }
            * { box-sizing: border-box; }
            .igris-focus:focus-visible { outline: 2px solid #22D3EE; outline-offset: 2px; }
            .igris-btn { transition: background-color 0.2s ease, border-color 0.2s ease, transform 0.15s ease, box-shadow 0.2s ease; touch-action: manipulation; }
            .igris-btn:hover { filter: brightness(1.12); }
            .igris-btn:active { transform: translateY(1px); }
            .igris-card { background: rgba(10,17,40,0.85); border: 1px solid rgba(255,255,255,0.06); border-radius: 10px; }
            .igris-card:hover { border-color: rgba(34,211,238,0.25); }
            .igris-num { font-variant-numeric: tabular-nums; }
            .igris-truncate { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
            .igris-heading { text-wrap: balance; }
            @keyframes igris-pulse-dot { 0%,100% { opacity: 0.4; transform: scale(1); } 50% { opacity: 1; transform: scale(1.25); } }
            @keyframes igris-fade-in { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: translateY(0); } }
            @keyframes igris-scan { 0% { left: -100%; } 100% { left: 200%; } }
            @media (prefers-reduced-motion: reduce) {
                *, *::before, *::after { animation-duration: 0.01ms !important; animation-iteration-count: 1 !important; }
            }
        "#
        .to_string()
    }
}
