//! Low-level keyboard hook and synthetic key injection.

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    MapVirtualKeyW, MAPVK_VK_TO_VSC,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

use crate::key_codes::is_extended;
use crate::state_machine::{Action, Keys};

/// Tag stamped on injected events so the hook ignores our own output
/// (the mac version's OUR_EVENT_TAG / kCGEventSourceUserData).
pub const OUR_EXTRA_INFO: usize = 0x5350_5057; // "SPPW"

fn make_input(vk: u16, down: bool) -> INPUT {
    let mut flags = if down { 0 } else { KEYEVENTF_KEYUP };
    if is_extended(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: OUR_EXTRA_INFO,
            },
        },
    }
}

fn send(inputs: &[INPUT]) {
    if !inputs.is_empty() {
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            );
        }
    }
}

/// Full chord press: modifiers down, main down, main up, modifiers up.
/// Physically held modifiers stay held, so the injected key combines
/// with them automatically (mac version merged pressed_modifiers flags).
pub fn press_keys(keys: &Keys) {
    let mut inputs = Vec::with_capacity(keys.modifiers.len() * 2 + 2);
    for &m in &keys.modifiers {
        inputs.push(make_input(m, true));
    }
    inputs.push(make_input(keys.main, true));
    inputs.push(make_input(keys.main, false));
    for &m in keys.modifiers.iter().rev() {
        inputs.push(make_input(m, false));
    }
    send(&inputs);
}

pub fn key_down(vk: u16) {
    send(&[make_input(vk, true)]);
}

/// Execute state-machine actions. Returns true if Exit was requested.
pub fn execute(actions: &[Action]) -> bool {
    let mut exit = false;
    for action in actions {
        match action {
            Action::Press(keys) => press_keys(keys),
            Action::Down(vk) => key_down(*vk),
            Action::Exit => exit = true,
        }
    }
    exit
}

pub struct HookEvent {
    pub vk: u16,
    pub is_down: bool,
    pub injected_by_us: bool,
}

pub fn parse_hook_event(wparam: WPARAM, lparam: LPARAM) -> HookEvent {
    let kb = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
    HookEvent {
        vk: kb.vkCode as u16,
        is_down: wparam == WM_KEYDOWN as usize || wparam == WM_SYSKEYDOWN as usize,
        injected_by_us: kb.dwExtraInfo == OUR_EXTRA_INFO,
    }
}

pub type HookProc = unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT;

pub fn install_hook(proc: HookProc) -> HHOOK {
    unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(proc), std::ptr::null_mut(), 0) }
}

pub fn remove_hook(hook: HHOOK) {
    if !hook.is_null() {
        unsafe {
            UnhookWindowsHookEx(hook);
        }
    }
}

pub fn call_next_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}
