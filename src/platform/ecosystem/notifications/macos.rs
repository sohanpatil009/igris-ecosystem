use crate::eco::errors::EcoResult;
use super::PlatformNotification;
use std::process::Command;

pub struct MacosNotification;

impl PlatformNotification for MacosNotification {
    fn has_permission(&self) -> bool {
        // Check if accessibility permissions are granted
        let script = r#"
        tell application "System Events"
            try
                set frontApp to name of first application process whose frontmost is true
                return "granted"
            on error
                return "denied"
            end try
        end tell
        "#;

        Command::new("osascript")
            .args(["-e", script])
            .output()
            .map(|o| {
                let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                stdout.trim() == "granted"
            })
            .unwrap_or(false)
    }

    fn request_permission(&self) -> EcoResult<()> {
        // Open System Preferences to Accessibility pane
        let _ = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .output();

        Ok(())
    }
}
