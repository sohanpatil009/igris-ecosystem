#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod ringtone;
pub mod toasts;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod linux;

use crate::eco::errors::EcoResult;

// The shared notification seam (poll/reply trait + raw type) is owned by
// igris-protocol. The desktop's platform impls rely on its default impls for
// `read_notifications` (the desktop reads nothing — notifications arrive over
// the wire) and `reply_to_notification` (desktop replies route over the wire).
pub use igris_protocol::platform::ecosystem::notifications::{PlatformNotification, RawNotification};

/// Enterprise telephony seam (Igris native). Default no-ops so non-telephony
/// hosts keep compiling; the Windows implementation drives the desktop's own
/// phone line (Bluetooth-paired phone) when one is registered.
///
/// Desktop-local richer trait: the phone's minimal seam in igris-protocol
/// stays phone-shaped; the two never cross the crate boundary (the shared eco
/// tree never references PlatformCall/PlatformSms).
pub trait PlatformCall: Send + Sync {
    /// True when this device can place/control calls locally.
    fn is_available(&self) -> bool {
        false
    }
    /// Place an outgoing call through the local line.
    fn dial(&self, _number: &str) -> EcoResult<()> {
        Err(crate::eco::errors::EcoError::Call("no local telephony".to_string()))
    }
    fn accept_call(&self, _call_id: &str) -> EcoResult<()> {
        Ok(())
    }
    fn decline_call(&self, _call_id: &str) -> EcoResult<()> {
        Ok(())
    }
    fn mute_call(&self, _call_id: &str) -> EcoResult<()> {
        Ok(())
    }
    fn end_call(&self, _call_id: &str) -> EcoResult<()> {
        Ok(())
    }
    /// Snapshot of calls currently active on the platform-owned line
    /// (default: none).
    fn snapshot_active(&self) -> Vec<crate::platform::ecosystem::calls::LocalCallSnapshot> {
        Vec::new()
    }
}

/// Enterprise SMS seam (Igris native). Default no-ops.
pub trait PlatformSms: Send + Sync {
    fn send_sms(&self, _address: &str, _body: &str) -> EcoResult<()> {
        Ok(())
    }
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
