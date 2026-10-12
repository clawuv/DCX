//! Windows chrome helpers for the undecorated window.
//!
//! winit gives an undecorated window no resize frame on Windows (the whole
//! window is client area), so a window-proc subclass answers WM_NCHITTEST
//! with the standard edge/corner codes for the outer few pixels — which are
//! plain background in every screen layout. Everything else is forwarded to
//! winit's own proc untouched.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, GetWindowLongPtrW, GetWindowRect, IsZoomed, NCCALCSIZE_PARAMS, SetWindowLongPtrW, SetWindowPos,
    GWL_STYLE, GWLP_WNDPROC, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT,
    SM_CXPADDEDBORDER, SM_CXSIZEFRAME, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    WS_THICKFRAME,
};

const WM_NCCALCSIZE: u32 = 0x0083;
const WM_NCHITTEST: u32 = 0x0084;

static OLD_PROC: AtomicIsize = AtomicIsize::new(0);
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Installs the subclass on the app window. Called every frame; runs once.
pub fn install_resize_frame(frame: &eframe::Frame) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if INSTALLED.load(Ordering::Relaxed) {
        return;
    }
    let Ok(handle) = frame.window_handle() else { return };
    let RawWindowHandle::Win32(h) = handle.as_raw() else { return };
    // SAFETY: on the UI thread with a live window; the previous proc is kept
    // in OLD_PROC and every message we do not handle goes back to it.
    unsafe {
        let old = SetWindowLongPtrW(h.hwnd.get() as HWND, GWLP_WNDPROC, wnd_proc as usize as isize);
        if old == 0 {
            return;
        }
        OLD_PROC.store(old, Ordering::Relaxed);
        INSTALLED.store(true, Ordering::Relaxed);
        // the sizing border makes the system resize loop (and Aero snap)
        // work; WM_NCCALCSIZE below keeps it invisible
        let style = GetWindowLongPtrW(h.hwnd.get() as HWND, GWL_STYLE) as u32;
        SetWindowLongPtrW(h.hwnd.get() as HWND, GWL_STYLE, (style | WS_THICKFRAME) as isize);
        SetWindowPos(
            h.hwnd.get() as HWND,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

/// The width of the resize frame, in points, the shell keeps clear of
/// controls (kept in sync with the hit zones below).
pub fn resize_border() -> f32 {
    8.0
}

// SAFETY: forwards to the winit proc saved at install time.
unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // keep the sizing border invisible: the client area is the whole window,
    // and the resize zones are the outer pixels claimed in WM_NCHITTEST.
    // Windows still sizes a maximized borderless window to the work area
    // plus the frame (ptMaxSize is ignored), so the maximized client is
    // clamped to the work area here — otherwise 8px of content sit off
    // screen on every side.
    if msg == WM_NCCALCSIZE && wp != 0 {
        if IsZoomed(hwnd) != 0 {
            if let Some(work) = work_area(hwnd) {
                (*(lp as *mut NCCALCSIZE_PARAMS)).rgrc[0] = work;
            }
        }
        return 0;
    }
    if msg == WM_NCHITTEST {
        if let Some(hit) = resize_hit(hwnd, lp) {
            return hit;
        }
    }
    let old = OLD_PROC.load(Ordering::Relaxed);
    if old == 0 {
        return 0;
    }
    // SAFETY: the stored value is winit's WNDPROC, put there right before
    // ours took over; both are pointer-sized.
    let proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT = std::mem::transmute(old);
    CallWindowProcW(Some(proc), hwnd, msg, wp, lp)
}

/// The work area (without the taskbar) of the monitor the window is on.
// SAFETY: plain Win32 queries on our own window.
unsafe fn work_area(hwnd: HWND) -> Option<RECT> {
    let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if mon.is_null() || GetMonitorInfoW(mon, &mut info) == 0 {
        return None;
    }
    Some(info.rcWork)
}

/// Standard resize hit codes for the window's outer pixels, or None to let
/// winit answer (client area / maximized windows do not resize).
// SAFETY: plain Win32 queries on our own window.
unsafe fn resize_hit(hwnd: HWND, lp: LPARAM) -> Option<LRESULT> {
    if IsZoomed(hwnd) != 0 {
        return None;
    }
    // screen coordinates, both words signed (multi-monitor setups go negative)
    let x = (lp as u32 & 0xFFFF) as u16 as i16 as i32;
    let y = ((lp as u32 >> 16) & 0xFFFF) as u16 as i16 as i32;
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    if GetWindowRect(hwnd, &mut r) == 0 {
        return None;
    }
    let dpi = GetDpiForWindow(hwnd);
    let thin = GetSystemMetricsForDpi(SM_CXSIZEFRAME, dpi).max(2) as i32;
    let fat = thin + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi).max(0);
    // the top edge stays thin so the toolbar buttons keep their hit area
    let on_l = x < r.left + fat;
    let on_r = x >= r.right - fat;
    let on_t = y < r.top + thin;
    let on_b = y >= r.bottom - fat;
    if !on_l && !on_r && !on_t && !on_b {
        return None;
    }
    let hit = if on_t && on_l {
        HTTOPLEFT
    } else if on_t && on_r {
        HTTOPRIGHT
    } else if on_b && on_l {
        HTBOTTOMLEFT
    } else if on_b && on_r {
        HTBOTTOMRIGHT
    } else if on_l {
        HTLEFT
    } else if on_r {
        HTRIGHT
    } else if on_t {
        HTTOP
    } else {
        HTBOTTOM
    };
    Some(hit as LRESULT)
}
