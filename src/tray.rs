//! System tray icon with IDLE / HYPER state colors and a context menu.
//! Icons replicate icons/idle_icon.svg and icons/hyper_icon.svg from the
//! macOS version (circle + crosshair; hyper adds a center dot), rendered
//! programmatically so no image decoding is needed.

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

// Colors from the SVGs (0xRRGGBB).
const BASE_COLOR: u32 = 0x2C_3E_50; // circle fill
const IDLE_ACCENT: u32 = 0x34_98_DB; // blue
const HYPER_ACCENT: u32 = 0xE7_4C_3C; // red

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Sample the SVG design at a point in its 16x16 viewBox coordinates.
/// Returns 0xAARRGGBB. Geometry from idle_icon.svg / hyper_icon.svg:
/// circle cx=8 cy=8 r=6 fill=base stroke=accent stroke-width=1,
/// crosshair lines (5,8)-(11,8) and (8,5)-(8,11) stroke-width=2,
/// hyper adds a filled accent circle r=2.
fn sample_svg(sx: f32, sy: f32, accent: u32, with_dot: bool) -> u32 {
    let dx = sx - 8.0;
    let dy = sy - 8.0;
    let d = (dx * dx + dy * dy).sqrt();
    if d > 6.5 {
        return 0; // transparent
    }
    if d >= 5.5 {
        return 0xFF_00_00_00 | accent; // circle stroke
    }
    if with_dot && d <= 2.0 {
        return 0xFF_00_00_00 | accent; // hyper center dot
    }
    let h_line = (7.0..=9.0).contains(&sy) && (5.0..=11.0).contains(&sx);
    let v_line = (7.0..=9.0).contains(&sx) && (5.0..=11.0).contains(&sy);
    if h_line || v_line {
        return 0xFF_00_00_00 | accent; // crosshair
    }
    0xFF_00_00_00 | BASE_COLOR
}

/// 32x32 ARGB icon rendering the SVG design with 3x3 supersampling.
fn create_state_icon(accent: u32, with_dot: bool) -> HICON {
    const S: i32 = 32;
    const SUB: i32 = 3;

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
                // Average SUB*SUB subsamples (premultiplied-alpha style
                // average is overkill at this size; plain average is fine).
                let (mut a, mut r, mut g, mut b) = (0u32, 0u32, 0u32, 0u32);
                for j in 0..SUB {
                    for i in 0..SUB {
                        // Map pixel to the 16x16 SVG viewBox.
                        let sx = (x as f32 + (i as f32 + 0.5) / SUB as f32) * 16.0 / S as f32;
                        let sy = (y as f32 + (j as f32 + 0.5) / SUB as f32) * 16.0 / S as f32;
                        let c = sample_svg(sx, sy, accent, with_dot);
                        a += c >> 24;
                        r += (c >> 16) & 0xFF;
                        g += (c >> 8) & 0xFF;
                        b += c & 0xFF;
                    }
                }
                let n = (SUB * SUB) as u32;
                let v = ((a / n) << 24) | ((r / n) << 16) | ((g / n) << 8) | (b / n);
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
            icon_idle: create_state_icon(IDLE_ACCENT, false),
            icon_hyper: create_state_icon(HYPER_ACCENT, true),
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
        let about = wide("关于 Super++");
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
