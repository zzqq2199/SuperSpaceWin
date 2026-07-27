//! Win+L lock guard.
//!
//! winlogon matches Win+L on raw input, before low-level hook suppression
//! applies, so a physical Win+L cannot be blocked from user mode. Instead,
//! while Space++ runs, the DisableLockWorkstation policy is enabled so the
//! OS ignores Win+L entirely; an intentional bare Win+L (Win held in IDLE)
//! is detected by the hook and Space++ locks the workstation itself by
//! briefly lifting the policy around a LockWorkStation() call. The policy
//! is re-enabled when the session unlocks.

use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE,
};
use windows_sys::Win32::System::Shutdown::LockWorkStation;

use crate::logger;
use crate::tray::wide;

const POLICY_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Policies\System";
const POLICY_VALUE: &str = "DisableLockWorkstation";

fn open_policy_key() -> Option<HKEY> {
    let subkey = wide(POLICY_KEY);
    let mut hkey: HKEY = std::ptr::null_mut();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut hkey,
            std::ptr::null_mut(),
        )
    };
    if status == 0 {
        Some(hkey)
    } else {
        None
    }
}

/// true = OS ignores Win+L; false = native behavior restored.
fn set_lock_disabled(disabled: bool) -> bool {
    let Some(hkey) = open_policy_key() else {
        return false;
    };
    let name = wide(POLICY_VALUE);
    let status = if disabled {
        let data: u32 = 1;
        unsafe {
            RegSetValueExW(
                hkey,
                name.as_ptr(),
                0,
                REG_DWORD,
                &data as *const u32 as *const u8,
                std::mem::size_of::<u32>() as u32,
            )
        }
    } else {
        // Delete rather than write 0: leave no trace of the policy.
        unsafe { RegDeleteValueW(hkey, name.as_ptr()) }
    };
    unsafe { RegCloseKey(hkey) };
    // Deleting an already-absent value returns ERROR_FILE_NOT_FOUND (2).
    status == 0 || (!disabled && status == 2)
}

/// Called on startup: OS-level Win+L off while Space++ runs.
pub fn enable() {
    if set_lock_disabled(true) {
        logger::log("[lock_guard] Win+L policy guard enabled");
    } else {
        logger::log("[lock_guard] failed to enable Win+L policy guard");
    }
}

/// Called on clean exit: restore native Win+L.
pub fn disable() {
    if set_lock_disabled(false) {
        logger::log("[lock_guard] Win+L policy guard removed");
    } else {
        logger::log("[lock_guard] failed to remove Win+L policy guard");
    }
}

/// Intentional bare Win+L detected: lock on the user's behalf.
/// The policy is lifted just for this request and re-enabled from the
/// session-unlock notification (see WM_WTSSESSION_CHANGE handling).
pub fn request_lock() {
    set_lock_disabled(false);
    let ok = unsafe { LockWorkStation() };
    logger::log(&format!("[lock_guard] intentional Win+L, LockWorkStation ok={ok}"));
    if ok == 0 {
        // Lock failed; don't leave the guard down.
        set_lock_disabled(true);
    }
}

/// Session unlocked: put the guard back up.
pub fn on_session_unlock() {
    set_lock_disabled(true);
    logger::log("[lock_guard] session unlocked, guard re-enabled");
}
