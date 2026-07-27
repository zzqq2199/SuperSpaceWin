//! Space++ for Windows: the space bar becomes a Hyper key.
//! Port of the macOS version (SuperSpace); the state machine is in
//! `state_machine.rs`, this file wires it to the OS (hook, tray, config).

#![cfg_attr(not(test), windows_subsystem = "windows")]

mod autostart;
mod config;
mod foreground;
mod key_codes;
mod keyboard;
mod lock_guard;
mod logger;
mod state_machine;
mod tray;

use std::cell::RefCell;

use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MessageBoxW,
    PostQuitMessage, RegisterClassW, TranslateMessage, MB_ICONERROR, MB_ICONINFORMATION, MB_OK,
    MSG, WM_CONTEXTMENU, WM_DESTROY, WM_RBUTTONUP, WNDCLASSW,
};

use config::VerboseConfig;
use key_codes::is_modifier_vk;
use keyboard::{call_next_hook, parse_hook_event};
use state_machine::{State, StateMachine};
use tray::{show_menu, wide, Tray, MENU_ABOUT, MENU_AUTOSTART, MENU_EXIT, WM_TRAY};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
const WTS_SESSION_UNLOCK: usize = 0x8;
const VK_L: u16 = 0x4C;

struct App {
    sm: StateMachine,
    verbose: VerboseConfig,
    tray: Tray,
    about_text: String,
    blacklist: foreground::BlacklistChecker,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let ev = parse_hook_event(wparam, lparam);
        if !ev.injected_by_us {
            let mut suppress = false;
            let mut actions = Vec::new();
            let mut lock_requested = false;
            APP.with(|cell| {
                // try_borrow_mut: if the hook somehow re-enters, pass through.
                let Ok(mut guard) = cell.try_borrow_mut() else {
                    return;
                };
                let Some(app) = guard.as_mut() else { return };

                let is_mod = is_modifier_vk(ev.vk);

                // Bare Win+L (Win held natively, no hyper flow): the OS-level
                // lock is disabled by our policy guard, so lock on the
                // user's behalf.
                if ev.vk == VK_L
                    && ev.is_down
                    && app.sm.state == State::Idle
                    && app.sm.os_win_held()
                {
                    lock_requested = true;
                    suppress = true;
                    return;
                }

                // Blacklisted foreground app: pass everything through.
                if !app.blacklist.is_empty() && app.blacklist.foreground_blacklisted() {
                    if is_mod {
                        app.sm.track_modifier(ev.vk, ev.is_down);
                    }
                    if app.sm.state != State::Idle {
                        if app.verbose.on_state {
                            logger::log(&format!(
                                "[state] {:?} -> Idle (blacklisted foreground app)",
                                app.sm.state
                            ));
                        }
                        let was_hyper = app.sm.state == State::HyperMode;
                        app.sm.reset();
                        if was_hyper {
                            app.tray.set_idle();
                        }
                    }
                    return;
                }

                if app.verbose.on_event {
                    logger::log(&format!(
                        "[event] vk={:#04x} is_down={} is_modifier={}",
                        ev.vk, ev.is_down, is_mod
                    ));
                }

                let before = app.sm.state;
                let out = app.sm.handle_key_event(ev.vk, ev.is_down, is_mod);
                let after = app.sm.state;

                if before != after {
                    if app.verbose.on_state {
                        logger::log(&format!("[state] {:?} -> {:?}", before, after));
                    }
                    if after == State::HyperMode {
                        app.tray.set_hyper();
                    } else if before == State::HyperMode {
                        app.tray.set_idle();
                    }
                }
                if app.verbose.on_action {
                    for a in &out.actions {
                        logger::log(&format!("[action] {a:?}"));
                    }
                }
                suppress = !out.pass_through;
                actions = out.actions;
            });

            // Act outside the borrow so re-entrant hook calls can't panic.
            if lock_requested {
                lock_guard::request_lock();
            }
            if keyboard::execute(&actions) {
                PostQuitMessage(0);
            }
            if suppress {
                return 1;
            }
        }
    }
    call_next_hook(code, wparam, lparam)
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            let event = lparam as u32;
            if event == WM_RBUTTONUP || event == WM_CONTEXTMENU {
                let enabled = autostart::is_enabled();
                match show_menu(hwnd, enabled) {
                    MENU_ABOUT => {
                        let text = APP.with(|cell| {
                            cell.borrow()
                                .as_ref()
                                .map(|a| a.about_text.clone())
                                .unwrap_or_default()
                        });
                        let title = wide("关于 Space++ (Win)");
                        let body = wide(&text);
                        MessageBoxW(hwnd, body.as_ptr(), title.as_ptr(), MB_OK | MB_ICONINFORMATION);
                    }
                    MENU_AUTOSTART => {
                        autostart::set_enabled(!enabled);
                    }
                    MENU_EXIT => PostQuitMessage(0),
                    _ => {}
                }
            }
            0
        }
        WM_WTSSESSION_CHANGE => {
            if wparam == WTS_SESSION_UNLOCK {
                lock_guard::on_session_unlock();
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn error_box(text: &str) {
    let title = wide("Space++ (Win)");
    let body = wide(text);
    unsafe {
        MessageBoxW(std::ptr::null_mut(), body.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR);
    }
}

fn main() {
    logger::init();

    // Single instance.
    let mutex_name = wide("SpacePP_Win_SingleInstance");
    unsafe {
        CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            error_box("Space++ 已在运行。");
            return;
        }
    }

    let (cfg, warnings) = config::load();
    for w in &warnings {
        logger::log(&format!("[config] {w}"));
    }
    let config_desc = cfg
        .path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(内置默认配置)".to_string());
    logger::log(&format!("[SpacePP] start, config: {config_desc}"));

    // Hidden window that owns the tray icon.
    let hinstance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class_name = wide("SpacePPWnd");
    let mut wc: WNDCLASSW = unsafe { std::mem::zeroed() };
    wc.lpfnWndProc = Some(wnd_proc);
    wc.hInstance = hinstance;
    wc.lpszClassName = class_name.as_ptr();
    if unsafe { RegisterClassW(&wc) } == 0 {
        error_box("窗口类注册失败。");
        return;
    }
    let title = wide("Space++");
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        error_box("窗口创建失败。");
        return;
    }

    let tray = Tray::new(hwnd, &format!("Space++ {VERSION} - 正在运行"));
    let about_text = format!(
        "Space++ (Win) 版本 {VERSION}\n\n将空格键变成 Hyper 键的键盘效率工具。\n\n配置文件：{config_desc}\n日志：%TEMP%\\spacepp.log\n\n© 2026 Quan Zhou"
    );

    APP.with(|cell| {
        *cell.borrow_mut() = Some(App {
            sm: StateMachine::new(cfg.map, cfg.hold_as_hyper),
            verbose: cfg.verbose,
            tray,
            about_text,
            blacklist: foreground::BlacklistChecker::new(cfg.blacklist),
        });
    });

    unsafe {
        WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
    }

    let hook = keyboard::install_hook(hook_proc);
    if hook.is_null() {
        error_box("键盘钩子安装失败。");
        APP.with(|cell| {
            if let Some(app) = cell.borrow().as_ref() {
                app.tray.remove();
            }
        });
        return;
    }
    logger::log("[SpacePP] keyboard hook ready");
    lock_guard::enable();

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    unsafe {
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    lock_guard::disable();
    keyboard::remove_hook(hook);
    unsafe {
        WTSUnRegisterSessionNotification(hwnd);
    }
    APP.with(|cell| {
        if let Some(app) = cell.borrow().as_ref() {
            app.tray.remove();
        }
    });
    logger::log("[SpacePP] exit");
}
