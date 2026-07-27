//! System tray icon with IDLE / HYPER state colors and a context menu.
//! Icons are drawn programmatically (rounded square with a space-bar mark)
//! so no asset files are needed.

use std::mem::{size_of, zeroed};

use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, GetDC, ReleaseDC, BITMAPINFO, BI_RGB,
    DIB_RGB_COLORS,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, DestroyMenu, GetCursorPos,
    SetForegroundWindow, TrackPopupMenu, HICON, ICONINFO, MF_CHECKED, MF_SEPARATOR, MF_STRING,
    TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP,
};

pub const WM_TRAY: u32 = WM_APP + 1;
const TRAY_ID: u32 = 1;

pub const MENU_ABOUT: usize = 1;
pub const MENU_AUTOSTART: usize = 2;
pub const MENU_EXIT: usize = 3;

const IDLE_COLOR: u32 = 0x00_55_55_55; // gray (0x00RRGGBB)
const HYPER_COLOR: u32 = 0x00_FF_7A_00; // orange

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 32x32 ARGB icon: rounded square filled with `rgb`, white bar near the
/// bottom suggesting a space bar.
fn create_state_icon(rgb: u32) -> HICON {
    const S: i32 = 32;
    const MARGIN: i32 = 2;
    const RADIUS: i32 = 8;

    unsafe {
        let mut bmi: BITMAPINFO = zeroed();
        bmi.bmiHeader.biSize = size_of::<windows_sys::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = S;
        bmi.bmiHeader.biHeight = -S; // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB as u32;

        let hdc = GetDC(std::ptr::null_mut());
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let color_bmp = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        ReleaseDC(std::ptr::null_mut(), hdc);
        if color_bmp.is_null() {
            return std::ptr::null_mut();
        }

        let px = bits as *mut u32;
        for y in 0..S {
            for x in 0..S {
                let inside = inside_rounded_rect(x, y, S, MARGIN, RADIUS);
                let bar = (9..=22).contains(&x) && (21..=25).contains(&y);
                let v: u32 = if inside && bar {
                    0xFF_FF_FF_FF
                } else if inside {
                    0xFF_00_00_00 | rgb
                } else {
                    0
                };
                *px.add((y * S + x) as usize) = v;
            }
        }

        let mask_bmp = CreateBitmap(S, S, 1, 1, std::ptr::null());
        let ii = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask_bmp,
            hbmColor: color_bmp,
        };
        let icon = CreateIconIndirect(&ii);
        DeleteObject(color_bmp);
        DeleteObject(mask_bmp);
        icon
    }
}

fn inside_rounded_rect(x: i32, y: i32, size: i32, margin: i32, r: i32) -> bool {
    let lo = margin;
    let hi = size - 1 - margin;
    if x < lo || x > hi || y < lo || y > hi {
        return false;
    }
    let corners = [
        (lo + r, lo + r),
        (hi - r, lo + r),
        (lo + r, hi - r),
        (hi - r, hi - r),
    ];
    for (cx, cy) in corners {
        let in_corner_box = (x < lo + r || x > hi - r) && (y < lo + r || y > hi - r);
        if in_corner_box {
            // Only test against the nearest corner center.
            let near = (x - cx).abs() <= r && (y - cy).abs() <= r;
            if near {
                let dx = x - cx;
                let dy = y - cy;
                return dx * dx + dy * dy <= r * r;
            }
        }
    }
    true
}

pub struct Tray {
    hwnd: HWND,
    icon_idle: HICON,
    icon_hyper: HICON,
    tip: Vec<u16>,
}

impl Tray {
    pub fn new(hwnd: HWND, tooltip: &str) -> Tray {
        let tray = Tray {
            hwnd,
            icon_idle: create_state_icon(IDLE_COLOR),
            icon_hyper: create_state_icon(HYPER_COLOR),
            tip: wide(tooltip),
        };
        let nid = tray.notify_data(tray.icon_idle);
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &nid);
        }
        tray
    }

    fn notify_data(&self, icon: HICON) -> NOTIFYICONDATAW {
        let mut nid: NOTIFYICONDATAW = unsafe { zeroed() };
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.hwnd;
        nid.uID = TRAY_ID;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = icon;
        let n = self.tip.len().min(nid.szTip.len() - 1);
        nid.szTip[..n].copy_from_slice(&self.tip[..n]);
        nid
    }

    pub fn set_hyper(&self) {
        let nid = self.notify_data(self.icon_hyper);
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    pub fn set_idle(&self) {
        let nid = self.notify_data(self.icon_idle);
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    pub fn remove(&self) {
        let nid = self.notify_data(self.icon_idle);
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }
}

/// Show the context menu at the cursor; returns the chosen MENU_* id (0 = none).
pub fn show_menu(hwnd: HWND, autostart_enabled: bool) -> usize {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return 0;
        }
        let about = wide("关于 Space++ (Win)");
        let autostart = wide("开机自启");
        let exit = wide("退出");
        AppendMenuW(menu, MF_STRING, MENU_ABOUT, about.as_ptr());
        AppendMenuW(
            menu,
            MF_STRING | if autostart_enabled { MF_CHECKED } else { 0 },
            MENU_AUTOSTART,
            autostart.as_ptr(),
        );
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, MENU_EXIT, exit.as_ptr());

        let mut pt: POINT = zeroed();
        GetCursorPos(&mut pt);
        // Required so the menu closes when clicking elsewhere.
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
            pt.x,
            pt.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        DestroyMenu(menu);
        cmd as usize
    }
}
