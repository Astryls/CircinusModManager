//! Snap Layouts, for a window that draws its own caption bar.
//!
//! Hovering Windows' maximize button opens the Snap Layouts flyout. It is not a hover effect
//! the webview can imitate: the shell opens it when the window answers `WM_NCHITTEST` with
//! `HTMAXBUTTON`, and a webview never sees that message. So a custom caption bar silently
//! loses the feature, which is the usual reason people switch one back off.
//!
//! What this does is narrow on purpose. It subclasses the window and answers four messages:
//!
//!   `WM_NCHITTEST`    -- `HTMAXBUTTON` when the cursor is over the maximize button, which is
//!                        what opens the flyout. Everything else falls through untouched.
//!   `WM_NCMOUSEMOVE`  -- the shell sends this instead of the ordinary mouse message once the
//!   `WM_NCMOUSELEAVE`    hit test says "caption", so the button would never light up on
//!                        hover. These are forwarded to the window as an event and the
//!                        webview draws the highlight itself.
//!   `WM_NCLBUTTONDOWN`-- swallowed over the button, so the shell does not start its own
//!                        caption drag, and the press is remembered.
//!   `WM_NCLBUTTONUP`  -- the actual click: toggle the window, but only if the press that
//!                        started it was also on the button. Pressing on the button and
//!                        releasing somewhere else must not maximize, the way it does not for
//!                        any other button in any other program.
//!
//! Everything else goes straight to the original procedure. That matters more than any of the
//! above: a subclass that gets clever about messages it does not fully understand is how a
//! window stops responding, and this one is only ever installed while the custom frame is on.
//!
//! **The rectangle comes from the window itself**, through `set_maximise_rect`, because only
//! the webview knows where it put the button -- and it moves with the window's width, the
//! display scale and whether the bar is drawn at all. Zero width means "there is no button",
//! which is the state after the custom frame is switched off, and the hit test then claims
//! nothing.

#![cfg(target_os = "windows")]

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use tauri::{Emitter, Manager};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    HTMAXBUTTON, WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE,
};

/// The maximize button, in physical pixels relative to the window's client area. Four atomics
/// rather than a mutex: this is read on the UI thread inside a window procedure, where
/// blocking on a lock somebody else holds would freeze the window.
static BTN_X: AtomicI32 = AtomicI32::new(0);
static BTN_Y: AtomicI32 = AtomicI32::new(0);
static BTN_W: AtomicI32 = AtomicI32::new(0);
static BTN_H: AtomicI32 = AtomicI32::new(0);
/// Was the press that is in progress started on the button?
static PRESSED: AtomicBool = AtomicBool::new(false);
static INSTALLED: AtomicBool = AtomicBool::new(false);

const SUBCLASS_ID: usize = 0xC13C_0001;

fn over_button(hwnd: HWND, lparam: LPARAM) -> bool {
    let w = BTN_W.load(Ordering::Relaxed);
    let h = BTN_H.load(Ordering::Relaxed);
    if w <= 0 || h <= 0 {
        return false;
    }
    // `WM_NCHITTEST` carries screen coordinates; the rectangle is client-relative, so the
    // window's own position has to come off before they can be compared.
    let lp = lparam.0 as u32;
    let sx = (lp & 0xFFFF) as i16 as i32;
    let sy = ((lp >> 16) & 0xFFFF) as i16 as i32;
    let mut pt = windows::Win32::Foundation::POINT { x: sx, y: sy };
    // SAFETY: `hwnd` is the window being subclassed, which is alive for the duration of the
    // call, and `pt` is a valid local.
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
    }
    let x = BTN_X.load(Ordering::Relaxed);
    let y = BTN_Y.load(Ordering::Relaxed);
    pt.x >= x && pt.x < x + w && pt.y >= y && pt.y < y + h
}

unsafe extern "system" fn proc_(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, id: usize, data: usize) -> LRESULT {
    let app = data as *const tauri::AppHandle;
    let emit = |event: &str| {
        if !app.is_null() {
            // SAFETY: the pointer is a `Box::leak`ed AppHandle, alive for the process.
            let handle = unsafe { &*app };
            let _ = handle.emit(event, ());
        }
    };
    match msg {
        WM_NCHITTEST => {
            if over_button(hwnd, lparam) {
                return LRESULT(HTMAXBUTTON as isize);
            }
        }
        // Once the hit test says the cursor is on a caption button, the webview stops getting
        // ordinary mouse messages for it -- so without these the button never lights up.
        WM_NCMOUSEMOVE if wparam.0 as u32 == HTMAXBUTTON => emit("snap:hover"),
        WM_NCMOUSELEAVE => emit("snap:leave"),
        WM_NCLBUTTONDOWN if wparam.0 as u32 == HTMAXBUTTON => {
            PRESSED.store(true, Ordering::Relaxed);
            // Swallowed: left to the shell this starts a caption drag.
            return LRESULT(0);
        }
        WM_NCLBUTTONUP if wparam.0 as u32 == HTMAXBUTTON => {
            emit("snap:leave");
            if PRESSED.swap(false, Ordering::Relaxed) {
                emit("snap:toggle");
            }
            return LRESULT(0);
        }
        WM_NCLBUTTONUP => {
            // Released somewhere else: the press is over and did not count.
            PRESSED.store(false, Ordering::Relaxed);
        }
        _ => {}
    }
    // SAFETY: forwarding to the next procedure in the chain with the arguments we were given.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Install the subclass once. Safe to call repeatedly.
pub fn install(app: &tauri::AppHandle) {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let Some(window) = app.get_webview_window("main") else {
        INSTALLED.store(false, Ordering::SeqCst);
        return;
    };
    // The raw pointer, not Tauri's `HWND`. Tauri depends on its own version of the `windows`
    // crate, and if it is not the version in this Cargo.toml the two `HWND` newtypes are
    // different types that will not unify -- a build failure with a confusing message, and
    // the kind that only appears on the one platform this code compiles for. Going through
    // the pointer means the two versions never have to agree.
    let Ok(raw) = window.hwnd() else {
        INSTALLED.store(false, Ordering::SeqCst);
        return;
    };
    let hwnd = HWND(raw.0 as _);
    // Leaked on purpose: the procedure may be called at any time until the process ends, and
    // a handle freed while a message is in flight is a crash on the way out.
    let handle: &'static tauri::AppHandle = Box::leak(Box::new(app.clone()));
    // SAFETY: `hwnd` is the live main window and `proc_` has the signature the API requires.
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(proc_), SUBCLASS_ID, handle as *const _ as usize);
    }
}

/// Take it back off, and stop claiming the hit test. Used when the custom frame is switched
/// off: the system caption bar is drawing its own maximize button and must own the whole
/// non-client area again.
pub fn remove(app: &tauri::AppHandle) {
    set_rect(0, 0, 0, 0);
    if !INSTALLED.swap(false, Ordering::SeqCst) {
        return;
    }
    let Some(window) = app.get_webview_window("main") else { return };
    let Ok(raw) = window.hwnd() else { return };
    let hwnd = HWND(raw.0 as _);
    // SAFETY: removing the subclass installed above, by the same id.
    unsafe {
        let _ = RemoveWindowSubclass(hwnd, Some(proc_), SUBCLASS_ID);
    }
}

pub fn set_rect(x: i32, y: i32, w: i32, h: i32) {
    BTN_X.store(x, Ordering::Relaxed);
    BTN_Y.store(y, Ordering::Relaxed);
    BTN_W.store(w, Ordering::Relaxed);
    BTN_H.store(h, Ordering::Relaxed);
}
