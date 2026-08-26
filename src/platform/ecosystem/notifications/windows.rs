use crate::eco::errors::EcoResult;
use super::PlatformNotification;
use std::process::Command;

pub struct WindowsNotification;

impl PlatformNotification for WindowsNotification {
    fn has_permission(&self) -> bool {
        true
    }

    fn request_permission(&self) -> EcoResult<()> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            let _ = Command::new("start")
                .arg("ms-settings:notifications")
                .creation_flags(0x08000000)
                .output();
        }
        Ok(())
    }
}
