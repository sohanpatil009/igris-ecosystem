#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod linux;
pub mod calls;
pub mod audio;
pub mod notifications;

// The platform seam (traits, phonebook stub, contact entry) has a single
// source of truth in igris-protocol — the desktop tree above implements
// those same traits for the three desktop OSes.
pub use igris_protocol::platform::ecosystem::{
    ContactEntry, PlatformClipboard, PlatformContacts, contacts_search,
    set_contacts_provider,
};

pub fn create_platform_clipboard() -> Box<dyn PlatformClipboard> {
    #[cfg(target_os = "macos")]
    { Box::new(macos::MacosClipboard) }
    #[cfg(target_os = "linux")]
    { Box::new(linux::LinuxClipboard) }
    #[cfg(target_os = "windows")]
    { Box::new(windows::WindowsClipboard) }
}
