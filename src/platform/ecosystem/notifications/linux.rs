use crate::eco::errors::EcoResult;
use super::PlatformNotification;
use std::process::Command;

pub struct LinuxNotification;

impl PlatformNotification for LinuxNotification {
    fn has_permission(&self) -> bool {
        // Check if D-Bus notifications are available
        Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--dest", "org.freedesktop.Notifications",
                "--object-path", "/org/freedesktop/Notifications",
                "--method", "org.freedesktop.Notifications.GetServerInformation",
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn request_permission(&self) -> EcoResult<()> {
        // Linux typically doesn't require explicit permission
        Ok(())
    }
}
