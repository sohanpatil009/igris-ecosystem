use crate::eco::errors::{EcoError, EcoResult};
use crate::platform::ecosystem::PlatformClipboard;
use super::{PlatformNotification, RawNotification};
use std::process::Command;

pub struct WindowsNotification;

impl PlatformNotification for WindowsNotification {
    fn read_notifications(&self) -> EcoResult<Vec<RawNotification>> {
        let mut cmd = Command::new("powershell");
        cmd.arg("-NoProfile").arg("-Command").arg(r#"
            [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
            $notifier = [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier("Shell")
            $xml = [Windows.UI.Notifications.ToastNotificationManager]::GetDefault().GetSetting(0)
            Write-Output "Notifications not directly accessible via PowerShell - use WinRT API"
        "#);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let output = cmd.output()
            .map_err(|e| EcoError::Notification(format!("PowerShell failed: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();

        if stdout.trim().is_empty() || stdout.contains("not directly accessible") {
            return Ok(Vec::new());
        }

        let mut notifications = Vec::new();
        let lines: Vec<&str> = stdout.trim().split("\n").collect();

        for (i, line) in lines.iter().enumerate() {
            let parts: Vec<&str> = line.split("|||").collect();
            if parts.len() >= 2 {
                notifications.push(RawNotification {
                    id: format!("win_notif_{}", i),
                    app_name: parts[0].trim().to_string(),
                    title: parts[1].trim().to_string(),
                    body: if parts.len() > 2 { parts[2].trim().to_string() } else { String::new() },
                    timestamp: chrono::Utc::now().timestamp_millis(),
                });
            }
        }

        Ok(notifications)
    }

    fn reply_to_notification(&self, _notification_id: &str, reply: &str) -> EcoResult<()> {
        use crate::platform::ecosystem::windows::WindowsClipboard;
        let _ = WindowsClipboard.set_text(reply);

        let mut cmd = Command::new("powershell");
        cmd.arg("-NoProfile").arg("-Command")
            .arg(r#"Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait("^v")"#);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let _ = cmd.output();

        Ok(())
    }

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
