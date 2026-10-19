// T8: what a running program sees of a variable written to HKCU\Environment, and which of its
// windows WM_SETTINGCHANGE reaches: a top-level one never shown, and a message-only one. Or
// the broadcast for "Environment" that setx and the Settings app send.

use super::ffi;
use super::logon_now;
use crate::report::Report;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, RRF_NOEXPAND, RRF_RT_ANY, RegGetValueW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    HWND_BROADCAST, HWND_MESSAGE, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW,
    QS_ALLINPUT, RegisterClassW, SMTO_ABORTIFHUNG, SendMessageTimeoutW, TranslateMessage,
    WM_SETTINGCHANGE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

static TOP_LEVEL: AtomicUsize = AtomicUsize::new(0);
static MESSAGE_ONLY: AtomicUsize = AtomicUsize::new(0);
static HEARD: Mutex<Vec<(String, Option<String>)>> = Mutex::new(Vec::new());

fn valid(name: &str) -> bool {
    name.strip_prefix("PITBOARD_PROBE_").is_some_and(|rest| {
        !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
    })
}

unsafe extern "system" fn heard(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_SETTINGCHANGE {
        let which = if window as usize == TOP_LEVEL.load(Ordering::SeqCst) {
            "top_level"
        } else if window as usize == MESSAGE_ONLY.load(Ordering::SeqCst) {
            "message_only"
        } else {
            "other"
        };
        // SAFETY: WM_SETTINGCHANGE's lParam is null or a NUL-terminated string.
        let area = (lparam != 0).then(|| unsafe { ffi::from_wide_ptr(lparam as *const u16) });
        if let Ok(mut heard) = HEARD.lock() {
            heard.push((which.to_string(), area));
        }
        return 0;
    }
    // SAFETY: what this procedure does not handle goes to the default one.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn in_the_registry(name: &str) -> bool {
    let key = ffi::wide("Environment");
    let value = ffi::wide(name);
    let mut size = 0u32;
    // SAFETY: NUL-terminated names; no buffer, so only the size is asked.
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_ANY | RRF_NOEXPAND,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut size,
        )
    };
    read == ERROR_SUCCESS
}

fn window(class: &[u16], parent: HWND) -> HWND {
    // SAFETY: a registered class, a NUL-terminated title and no creation data.
    unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            parent,
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        )
    }
}

pub(super) fn watch(name: &str, seconds: u64, scratch: Option<&Path>) -> Report {
    if !valid(name) {
        return Report::refused(
            "setting-change",
            logon_now(),
            "--name takes a PITBOARD_PROBE_ name of capitals and underscores",
        );
    }
    let ready = match scratch.map(|s| ffi::scratch_file(s, "pitboard-probe-sc-ready")) {
        Some(Ok(ready)) => ready,
        Some(Err(reason)) => return Report::refused("setting-change", logon_now(), reason),
        None => return Report::refused("setting-change", logon_now(), "watch needs --scratch"),
    };
    if let Some(folder) = ready.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let class = ffi::wide("pitboard-probe-sc");
    let registered = WNDCLASSW {
        lpfnWndProc: Some(heard),
        // SAFETY: this program's own module.
        hInstance: unsafe { GetModuleHandleW(std::ptr::null()) },
        lpszClassName: class.as_ptr(),
        ..Default::default()
    };
    // SAFETY: a filled class whose name outlives every window of it.
    if unsafe { RegisterClassW(&raw const registered) } == 0 {
        return Report::refused(
            "setting-change",
            logon_now(),
            format!("RegisterClassW failed {}", ffi::last_error()),
        );
    }
    let top = window(&class, std::ptr::null_mut());
    let message_only = window(&class, HWND_MESSAGE);
    TOP_LEVEL.store(top as usize, Ordering::SeqCst);
    MESSAGE_ONLY.store(message_only as usize, Ordering::SeqCst);
    let start = Instant::now();
    let _ = std::fs::write(&ready, b"up");
    let mut timeline: Vec<Value> = Vec::new();
    let mut last = (std::env::var_os(name).is_some(), in_the_registry(name));
    timeline.push(json!({ "at_ms": 0, "environment": last.0, "registry": last.1 }));
    let mut heard_seen = 0;
    while start.elapsed() < Duration::from_secs(seconds.min(600)) {
        // SAFETY: no handles, a 100 ms bound, waking for any message.
        unsafe { MsgWaitForMultipleObjects(0, std::ptr::null(), 0, 100, QS_ALLINPUT) };
        let mut message = MSG::default();
        // SAFETY: a MSG of this frame, any window of this thread.
        while unsafe { PeekMessageW(&raw mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0
        {
            // SAFETY: a message PeekMessageW just filled.
            unsafe {
                TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
        }
        let now = (std::env::var_os(name).is_some(), in_the_registry(name));
        let heard_now = HEARD.lock().map(|h| h.len()).unwrap_or(0);
        if now != last || heard_now != heard_seen {
            timeline.push(json!({
                "at_ms": start.elapsed().as_millis(),
                "environment": now.0,
                "registry": now.1,
                "messages": heard_now,
            }));
            last = now;
            heard_seen = heard_now;
        }
    }
    // SAFETY: windows this thread made.
    unsafe {
        DestroyWindow(top);
        DestroyWindow(message_only);
    }
    let _ = std::fs::remove_file(&ready);
    let messages: Vec<Value> = HEARD
        .lock()
        .map(|h| {
            h.iter()
                .map(|(window, area)| json!({ "window": window, "area": area }))
                .collect()
        })
        .unwrap_or_default();
    Report::ok(
        "setting-change",
        logon_now(),
        json!({
            "action": "watch",
            "top_level_made": !top.is_null(),
            "message_only_made": !message_only.is_null(),
            "messages": messages,
            "timeline": timeline,
        }),
    )
}

pub(super) fn broadcast(name: &str) -> Report {
    if !valid(name) || !ffi::marker_present() {
        return Report::refused(
            "setting-change",
            logon_now(),
            "a broadcast goes only from a marked account, for a PITBOARD_PROBE_ name",
        );
    }
    let area = ffi::wide("Environment");
    let mut answer = 0usize;
    let start = Instant::now();
    // SAFETY: a NUL-terminated area that outlives the call, which waits at most 5 s a window.
    let sent = unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            area.as_ptr() as LPARAM,
            SMTO_ABORTIFHUNG,
            5000,
            &raw mut answer,
        )
    };
    Report::ok(
        "setting-change",
        logon_now(),
        json!({
            "action": "broadcast",
            "for": name,
            "sent": sent != 0,
            "error": (sent == 0).then(ffi::last_error),
            "ms": start.elapsed().as_millis(),
        }),
    )
}
