#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod toasts;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod linux;

use crate::eco::errors::EcoResult;

/// Platform-specific notification access.
///
/// v2: the desktop reads nothing from the system notification center —
/// notifications only arrive over the wire (mirroring the phone) and are
/// displayed via native toasts (`toasts`). The trait exists purely for
/// permission queries.
pub trait PlatformNotification: Send + Sync {
    /// Check if we have permission to show notifications.
    fn has_permission(&self) -> bool;

    /// Request permission from the user (platform-specific dialog).
    fn request_permission(&self) -> EcoResult<()>;
}

pub fn create_platform_notification() -> Box<dyn PlatformNotification> {
    #[cfg(target_os = "macos")]
    { Box::new(macos::MacosNotification) }
    #[cfg(target_os = "linux")]
    { Box::new(linux::LinuxNotification) }
    #[cfg(target_os = "windows")]
    { Box::new(windows::WindowsNotification) }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    { Box::new(DummyNotification) }
}

/// Fallback for unsupported platforms.
struct DummyNotification;

impl PlatformNotification for DummyNotification {
    fn has_permission(&self) -> bool {
        false
    }
    fn request_permission(&self) -> EcoResult<()> {
        Ok(())
    }
}
