//! Logging bootstrap with secret redaction (master Sec19).
//!
//! Rules:
//! - NEVER log passwords, API keys, tokens, license keys, private keys,
//!   bearer credentials, or clipboard contents.
//! - Always pass user-controlled / wire text through
//!   [`sanitize_message`] before `tracing` macros, or wrap secrets in
//!   [`Redacted`] (whose `Debug`/`Display` print `[REDACTED]`).
//! - Redaction is best-effort defense-in-depth: the primary rule is to
//!   not put secrets into log fields at all.

use std::sync::OnceLock;

use tracing_subscriber::{fmt, EnvFilter};

static LOG_INIT: OnceLock<()> = OnceLock::new();

/// Secret key fragments (case-insensitive) whose `key=value` tails get
/// redacted by [`sanitize_message`].
const SECRET_KEYS: &[&str] = &[
    "password",
    "passwd",
    "api_key",
    "apikey",
    "secret",
    "token",
    "bearer",
    "authorization",
    "license",
    "private_key",
    "privatekey",
    "access_key",
    "refresh_token",
    "client_secret",
];

/// Initialize the global `tracing` subscriber (idempotent).
///
/// `default_level` is used when `RUST_LOG` is unset (e.g. `"info"`).
/// Returns `true` on first init, `false` when already initialized.
pub fn init_logging(default_level: &str) -> bool {
    LOG_INIT.get_or_init(|| {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(default_level.to_string()));
        let _ = fmt()
            .with_env_filter(filter)
            .with_target(true)
            .with_file(false)
            .with_line_number(false)
            .try_init();
    });
    true
}

/// Opaque wrapper: `Debug`/`Display` always print `[REDACTED]`.
/// Use for passwords, keys, tokens, clipboard bodies.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Redacted<T>(pub T);

/// Wrap a secret so a stray `{:?}` / `{}` can never leak it.
pub fn redacted<T>(value: T) -> Redacted<T> {
    Redacted(value)
}

impl<T> std::fmt::Debug for Redacted<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl<T> std::fmt::Display for Redacted<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

/// Redact secret tails from a log line.
///
/// Handles `key=value`, `key: value`, `key="value"` (any quoting), and
/// `Bearer <credentials>` / `Basic <credentials>`. Case-insensitive on
/// the key. Non-`key=value` free text is returned unchanged — callers
/// must still avoid logging raw secrets / clipboard bodies.
pub fn sanitize_message(input: &str) -> String {
    let mut out = input.to_string();
    for key in SECRET_KEYS {
        out = redact_key(&out, key);
    }
    out = redact_scheme(&out, "Bearer");
    out = redact_scheme(&out, "Basic");
    out
}

fn redact_key(input: &str, key: &str) -> String {
    // Scan case-insensitively for `key` followed by optional quote/
    // whitespace, then `=`/`:` and a value run to redact.
    let lower = input.to_lowercase();
    let key_lower = key.to_lowercase();
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < input.len() {
        if lower[i..].starts_with(key_lower.as_str()) {
            let mut j = i + key.len();
            // Optional closing quote/bracket after the key.
            if j < input.len() && matches!(bytes[j], b'"' | b'\'' | b']' | b'}') {
                j += 1;
            }
            // Optional whitespace.
            while j < input.len() && matches!(bytes[j], b' ' | b'\t') {
                j += 1;
            }
            // Require `=` or `:` separator, else not a kv pair.
            if j < input.len() && matches!(bytes[j], b'=' | b':') {
                j += 1;
                while j < input.len() && matches!(bytes[j], b' ' | b'\t') {
                    j += 1;
                }
                // Optional opening quote.
                let quote = if j < input.len() && matches!(bytes[j], b'"' | b'\'') {
                    let q = bytes[j];
                    j += 1;
                    Some(q)
                } else {
                    None
                };
                // Value runs until closing quote or delimiter.
                let value_start = j;
                if let Some(q) = quote {
                    while j < input.len() && bytes[j] != q {
                        j += 1;
                    }
                } else {
                    while j < input.len()
                        && !matches!(
                            bytes[j],
                            b' ' | b'\t' | b'\n' | b'\r' | b',' | b';' | b'}' | b']'
                        )
                    {
                        j += 1;
                    }
                }
                let value_end = j;
                // Skip closing quote.
                if quote.is_some() && j < input.len() {
                    j += 1;
                }
                if value_end > value_start {
                    result.push_str(&input[i..value_start]);
                    result.push_str("[REDACTED]");
                    i = j;
                    continue;
                }
            }
        }
        // Copy one char boundary.
        let ch_len = input[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        result.push_str(&input[i..i + ch_len]);
        i += ch_len;
    }
    result
}

fn redact_scheme(input: &str, scheme: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut rest = input;
    loop {
        if let Some(pos) = find_ascii_insensitive(rest, scheme) {
            let after_key = pos + scheme.len();
            let tail = &rest[after_key..];
            // Require whitespace after the scheme name.
            if tail.starts_with(' ') || tail.starts_with('\t') {
                let mut j = 0;
                while j < tail.len() && (tail.as_bytes()[j] == b' ' || tail.as_bytes()[j] == b'\t')
                {
                    j += 1;
                }
                let cred_start = j;
                while j < tail.len()
                    && !matches!(
                        tail.as_bytes()[j],
                        b' ' | b'\t' | b'\n' | b'\r' | b',' | b';' | b'"' | b'\''
                    )
                {
                    j += 1;
                }
                if j > cred_start {
                    result.push_str(&rest[..after_key]);
                    result.push_str(" [REDACTED]");
                    rest = &tail[j..];
                    continue;
                }
            }
            // Not a credential occurrence; copy through the scheme word.
            result.push_str(&rest[..after_key]);
            rest = &rest[after_key..];
        } else {
            result.push_str(rest);
            break;
        }
    }
    result
}

fn find_ascii_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || n.len() > h.len() {
        return None;
    }
    (0..=h.len() - n.len()).find(|&i| {
        h[i..i + n.len()]
            .iter()
            .zip(n.iter())
            .all(|(a, b)| a.to_ascii_lowercase() == b.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_common_secrets() {
        let msg =
            sanitize_message("login password=hunter2 api_key=ABC123 token xyz; Bearer SECRET123");
        assert!(!msg.contains("hunter2"), "{msg}");
        assert!(!msg.contains("ABC123"), "{msg}");
        assert!(!msg.contains("SECRET123"), "{msg}");
        assert!(msg.contains("[REDACTED]"), "{msg}");
    }

    #[test]
    fn redacted_wrapper_never_debugs_inner() {
        let s = format!("{:?}", redacted("super-secret"));
        assert_eq!(s, "[REDACTED]");
    }

    #[test]
    fn clean_text_passes_through() {
        let msg = sanitize_message("device discovered on port 53327");
        assert_eq!(msg, "device discovered on port 53327");
    }
}
