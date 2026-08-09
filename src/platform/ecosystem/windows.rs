use crate::eco::errors::{EcoError, EcoResult};
use super::PlatformClipboard;

pub struct WindowsClipboard;

// Win32 clipboard FFI
extern "system" {
    fn OpenClipboard(hWndNewOwner: *mut core::ffi::c_void) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn GetClipboardData(uFormat: u32) -> *mut core::ffi::c_void;
    fn SetClipboardData(uFormat: u32, hMem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    fn GlobalAlloc(uFlags: u32, dwBytes: usize) -> *mut core::ffi::c_void;
    fn GlobalLock(hMem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    fn GlobalUnlock(hMem: *mut core::ffi::c_void) -> i32;
    fn GlobalSize(hMem: *mut core::ffi::c_void) -> usize;
}

const CF_UNICODETEXT: u32 = 13;
const GMEM_MOVEABLE: u32 = 0x0002;

impl WindowsClipboard {
    fn set_text_inner(text: &str) -> EcoResult<()> {
        // Convert text to UTF-16 (null-terminated)
        let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let byte_size = utf16.len() * std::mem::size_of::<u16>();

        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return Err(EcoError::Clipboard("OpenClipboard failed".to_string()));
            }

            // Allocate global memory for the text
            let h_mem = GlobalAlloc(GMEM_MOVEABLE, byte_size);
            if h_mem.is_null() {
                CloseClipboard();
                return Err(EcoError::Clipboard("GlobalAlloc failed".to_string()));
            }

            // Lock and copy data
            let ptr = GlobalLock(h_mem) as *mut u16;
            if ptr.is_null() {
                CloseClipboard();
                return Err(EcoError::Clipboard("GlobalLock failed".to_string()));
            }
            std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr, utf16.len());
            GlobalUnlock(h_mem);

            // Clear clipboard and set new data
            EmptyClipboard();
            if SetClipboardData(CF_UNICODETEXT, h_mem).is_null() {
                CloseClipboard();
                return Err(EcoError::Clipboard("SetClipboardData failed".to_string()));
            }

            CloseClipboard();
        }

        Ok(())
    }

    fn get_text_inner() -> EcoResult<String> {
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return Err(EcoError::Clipboard("OpenClipboard failed".to_string()));
            }

            let h_data = GetClipboardData(CF_UNICODETEXT);
            if h_data.is_null() {
                CloseClipboard();
                return Ok(String::new());
            }

            let ptr = GlobalLock(h_data) as *const u16;
            if ptr.is_null() {
                CloseClipboard();
                return Ok(String::new());
            }

            // Calculate length by finding null terminator
            let mut len = 0;
            while *ptr.add(len) != 0 {
                len += 1;
            }

            let slice = std::slice::from_raw_parts(ptr, len);
            let text = String::from_utf16_lossy(slice);

            GlobalUnlock(h_data);
            CloseClipboard();

            Ok(text)
        }
    }
}

impl PlatformClipboard for WindowsClipboard {
    fn get_text(&self) -> EcoResult<String> {
        Self::get_text_inner()
    }

    fn set_text(&self, text: &str) -> EcoResult<()> {
        Self::set_text_inner(text)
    }
}
