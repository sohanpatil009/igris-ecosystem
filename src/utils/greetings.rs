// src/utils/greetings.rs - Personality-based greeting messages

use crate::config::CONFIG;

/// Text-only speak shim — the AI/TTS engine lives in the separate igris-ai
/// crate and is not part of the mesh app. Greetings are logged for now.
fn speak(msg: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("[IGRIS] {}", msg);
    Ok(())
}

/// Get the current wake word from config
pub fn get_wake_word() -> String {
    CONFIG.wake_word()
}

/// Get the greeting message based on personality
pub fn get_greeting() -> String {
    CONFIG.greeting()
}

/// Get the invoke response based on personality
pub fn get_invoke_response() -> String {
    CONFIG.invoke_response()
}

/// Get assistant name
pub fn get_assistant_name() -> String {
    CONFIG.assistant_name()
}

/// Speak the invoke greeting (personality-aware)
pub fn speak_invoke_greeting() -> Result<(), Box<dyn std::error::Error>> {
    speak(&get_greeting())
}

/// Speak the wake response (personality-aware)
pub fn speak_wake_response() -> Result<(), Box<dyn std::error::Error>> {
    speak(&get_invoke_response())
}

/// Speak goodbye (personality-aware)
pub fn speak_goodbye() -> Result<(), Box<dyn std::error::Error>> {
    let config = CONFIG.get();
    let msg = match config.personality {
        crate::config::Personality::Igris => "Standing by, master.",
        crate::config::Personality::Alita => "Alright, catch you later! Just call if you need me.",
    };
    speak(msg)
}

/// Speak welcome message (personality-aware)
pub fn speak_welcome() -> Result<(), Box<dyn std::error::Error>> {
    let name = get_assistant_name();
    let wake = get_wake_word();
    let msg = format!("{} initialized. Say {} to summon me.", name, wake);
    speak(&msg)
}

/// Check if text contains the configured wake word
pub fn contains_wake_word(text: &str) -> bool {
    let text_lower = text.to_lowercase();
    let config = CONFIG.get();
    text_lower.contains(config.personality.wake_word())
}

/// Check if text is a dismissal command
pub fn is_dismissal(text: &str) -> bool {
    let text_lower = text.to_lowercase();
    text_lower.contains("goodbye")
        || text_lower.contains("stand by")
        || text_lower.contains("standby")
        || text_lower.contains("dismiss")
        || text_lower.contains("that's all")
        || text_lower.contains("thank you")
        || text_lower.contains("sleep")
}

/// Legacy messages module for backward compatibility
pub mod messages {
    use super::*;

    pub fn invoke_greeting() -> String {
        get_greeting()
    }

    pub fn wake_word() -> String {
        get_wake_word()
    }

    pub fn wake_response() -> String {
        get_invoke_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wake_word_detection() {
        assert!(contains_wake_word("hello"));
        assert!(contains_wake_word("HELLO"));
        assert!(contains_wake_word("hello igris"));
        assert!(!contains_wake_word("goodbye"));
    }

    #[test]
    fn test_dismissal_detection() {
        assert!(is_dismissal("goodbye"));
        assert!(is_dismissal("stand by"));
        assert!(is_dismissal("sleep"));
        assert!(!is_dismissal("hello"));
    }
}