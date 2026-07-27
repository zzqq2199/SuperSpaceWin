//! Foreground window process lookup for the blacklist feature.

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// Lowercase exe name (e.g. "gameapp.exe") of the foreground window's
/// process, or None if it cannot be determined.
fn foreground_process_name() -> Option<(isize, String)> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(handle);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let name = path.rsplit(['\\', '/']).next().unwrap_or(&path).to_lowercase();
        Some((hwnd as isize, name))
    }
}

/// Checks whether the foreground process is blacklisted, caching the answer
/// per foreground window handle so the (OpenProcess + query) cost is paid
/// only when the foreground window changes, not on every keystroke.
pub struct BlacklistChecker {
    blacklist: Vec<String>,
    cached: Option<(isize, bool)>,
}

impl BlacklistChecker {
    pub fn new(blacklist: Vec<String>) -> Self {
        BlacklistChecker {
            blacklist: blacklist.into_iter().map(|s| s.to_lowercase()).collect(),
            cached: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.blacklist.is_empty()
    }

    pub fn foreground_blacklisted(&mut self) -> bool {
        if self.blacklist.is_empty() {
            return false;
        }
        let hwnd = unsafe { GetForegroundWindow() } as isize;
        if let Some((cached_hwnd, result)) = self.cached {
            if cached_hwnd == hwnd && hwnd != 0 {
                return result;
            }
        }
        let result = match foreground_process_name() {
            Some((_, name)) => self.blacklist.iter().any(|b| b == &name),
            None => false,
        };
        self.cached = Some((hwnd, result));
        result
    }
}
