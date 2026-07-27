//! "Start with Windows" via HKCU\...\Run.

use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
};

use crate::tray::wide;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "SpacePP";

fn open_run_key(access: u32) -> Option<HKEY> {
    let subkey = wide(RUN_KEY);
    let mut hkey: HKEY = std::ptr::null_mut();
    let status = unsafe {
        RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, access, &mut hkey)
    };
    if status == 0 {
        Some(hkey)
    } else {
        None
    }
}

pub fn is_enabled() -> bool {
    let Some(hkey) = open_run_key(KEY_QUERY_VALUE) else {
        return false;
    };
    let name = wide(VALUE_NAME);
    let status = unsafe {
        RegQueryValueExW(
            hkey,
            name.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    unsafe { RegCloseKey(hkey) };
    status == 0
}

pub fn set_enabled(enabled: bool) -> bool {
    let Some(hkey) = open_run_key(KEY_SET_VALUE) else {
        return false;
    };
    let name = wide(VALUE_NAME);
    let status = if enabled {
        let exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(_) => {
                unsafe { RegCloseKey(hkey) };
                return false;
            }
        };
        let value = wide(&format!("\"{}\"", exe.display()));
        unsafe {
            RegSetValueExW(
                hkey,
                name.as_ptr(),
                0,
                REG_SZ,
                value.as_ptr() as *const u8,
                (value.len() * 2) as u32,
            )
        }
    } else {
        unsafe { RegDeleteValueW(hkey, name.as_ptr()) }
    };
    unsafe { RegCloseKey(hkey) };
    status == 0
}
