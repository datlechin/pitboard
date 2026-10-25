//! H1 and I1, and W18's question: the named tools' processes, listed two ways.
//! `WTSEnumerateProcessesExW` is asked for every session (`WTS_ANY_SESSION`); Microsoft
//! documents that an unelevated caller sees only its own session's processes that way, which
//! is what the comparison with Toolhelp, which lists every session, measures. For each
//! process the probe reads its image path, session, package family, parent and whether it
//! runs as this account (by comparing token users, printed only as a yes or no). Only the
//! images [`crate::images`] allows are named; every other process is counted.

use super::ffi::{self, Owned, Token};
use super::logon_now;
use crate::images;
use crate::report::Report;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Security::TOKEN_QUERY;
use windows_sys::Win32::Storage::Packaging::Appx::GetPackageFamilyName;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::RemoteDesktop::{
    ProcessIdToSessionId, WTS_CURRENT_SERVER_HANDLE, WTS_PROCESS_INFO_EXW, WTS_TYPE_CLASS,
    WTSEnumerateProcessesExW, WTSFreeMemoryExW, WTSTypeProcessInfoLevel1,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetProcessInformation, IsWow64Process2, OpenProcess,
    OpenProcessToken, PROCESS_MACHINE_INFORMATION, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, ProcessMachineTypeInfo, QueryFullProcessImageNameW,
};

/// `WTS_ANY_SESSION` (wtsapi32.h). windows-sys does not carry it; `0xFFFF_FFFF` is
/// `WTS_CURRENT_SESSION`, which lists only this session.
const WTS_ANY_SESSION: u32 = 0xFFFF_FFFE;
const APPMODEL_ERROR_NO_PACKAGE: u32 = 15700;

/// One process from a Toolhelp snapshot.
pub(super) struct Entry {
    pub pid: u32,
    pub parent: u32,
    pub image: String,
}

/// Every process, from a Toolhelp snapshot.
pub(super) fn snapshot() -> Result<Vec<Entry>, u32> {
    // SAFETY: a process snapshot; th32ProcessID 0 means all processes.
    let snap = Owned::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) })
        .ok_or_else(ffi::last_error)?;
    // SAFETY: PROCESSENTRY32W is plain data; all zeroes is valid, and dwSize is set below.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut out = Vec::new();
    // SAFETY: `snap` is valid; `entry` is sized.
    let mut more = unsafe { Process32FirstW(snap.raw(), &mut entry) };
    while more != 0 {
        out.push(Entry {
            pid: entry.th32ProcessID,
            parent: entry.th32ParentProcessID,
            image: ffi::from_wide_buf(&entry.szExeFile),
        });
        // SAFETY: `snap` and `entry` remain valid.
        more = unsafe { Process32NextW(snap.raw(), &mut entry) };
    }
    Ok(out)
}

/// A process opened for limited queries.
pub(super) fn open_query(pid: u32) -> Result<Owned, u32> {
    // SAFETY: OpenProcess with a limited-query right; the handle is owned by the result.
    Owned::new(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) })
        .ok_or_else(ffi::last_error)
}

pub(super) fn image_path(h: HANDLE) -> Option<String> {
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `h` is open for limited queries; `buf` holds `len` units.
    let ok =
        unsafe { QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) };
    (ok != 0).then(|| ffi::from_wide_buf(&buf))
}

fn package_family(h: HANDLE) -> Value {
    let mut buf = vec![0u16; 256];
    let mut len = buf.len() as u32;
    // SAFETY: `h` is open for limited queries; `buf` holds `len` units.
    let rc = unsafe { GetPackageFamilyName(h, &mut len, buf.as_mut_ptr()) };
    match rc {
        0 => json!(ffi::from_wide_buf(&buf)),
        APPMODEL_ERROR_NO_PACKAGE => json!("none"),
        code => json!({ "error": code }),
    }
}

fn token_user(h: HANDLE) -> Option<String> {
    let mut t: HANDLE = std::ptr::null_mut();
    // SAFETY: `h` is an open process handle; `t` receives a token handle owned below.
    if unsafe { OpenProcessToken(h, TOKEN_QUERY, &mut t) } == 0 {
        return None;
    }
    Token::owned(t).user_sid()
}

// The machine the process's image is built for, which an x64 program emulated on ARM64 keeps.
fn machine_of(h: HANDLE) -> Option<u16> {
    let mut info = PROCESS_MACHINE_INFORMATION::default();
    // SAFETY: `h` is open for limited queries; `info` is sized for this class.
    let ok = unsafe {
        GetProcessInformation(
            h,
            ProcessMachineTypeInfo,
            (&mut info as *mut PROCESS_MACHINE_INFORMATION).cast(),
            size_of::<PROCESS_MACHINE_INFORMATION>() as u32,
        )
    };
    (ok != 0).then_some(info.ProcessMachine)
}

fn native_machine() -> Option<u16> {
    let (mut process, mut native) = (0u16, 0u16);
    // SAFETY: a pseudo-handle for this process; both out-params are valid.
    let ok = unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, &mut native) };
    (ok != 0).then_some(native)
}

fn machine_value(m: Option<u16>) -> Value {
    m.map_or(Value::Null, |m| json!(crate::pe::machine_name(m)))
}

fn session_of(pid: u32) -> Option<u32> {
    let mut s = 0u32;
    // SAFETY: a valid out-param.
    (unsafe { ProcessIdToSessionId(pid, &mut s) } != 0).then_some(s)
}

/// What can be read about one process, every principal and path hidden.
fn describe(pid: u32, own: Option<&str>, r: &crate::redact::Redactor) -> Value {
    match open_query(pid) {
        Ok(h) => {
            let user = token_user(h.raw());
            json!({
                "image_path": image_path(h.raw()).map(|p| r.redact(&p)),
                "machine": machine_value(machine_of(h.raw())),
                "package_family": package_family(h.raw()),
                "same_user": user.as_deref().zip(own).map(|(u, o)| u == o),
                "token_readable": user.is_some(),
            })
        }
        Err(code) => json!({ "open_error": code }),
    }
}

pub(super) fn processes(find: &[String]) -> Report {
    let logon = logon_now();
    if let Some(bad) = find.iter().find(|f| !images::is_tool_image(f)) {
        return Report::refused(
            "processes",
            logon,
            format!("{bad} is not a tool's, a runtime's, Pitboard's or the probe's image"),
        );
    }
    let wanted = |image: &str| find.iter().any(|f| f.eq_ignore_ascii_case(image));
    let own = Token::current().ok().and_then(|t| t.user_sid());
    let r = ffi::redactor();
    // SAFETY: GetCurrentProcessId has no preconditions.
    let current_session = session_of(unsafe { GetCurrentProcessId() });

    // WTS, every session.
    let wts = {
        let mut level = 1u32;
        let mut info: *mut u16 = std::ptr::null_mut();
        let mut count = 0u32;
        // SAFETY: the out-params receive a WTS-owned array freed below; level 1 selects
        // WTS_PROCESS_INFO_EXW.
        let ok = unsafe {
            WTSEnumerateProcessesExW(
                WTS_CURRENT_SERVER_HANDLE,
                &mut level,
                WTS_ANY_SESSION,
                &mut info,
                &mut count,
            )
        };
        if ok == 0 {
            json!({ "ok": false, "error": ffi::last_error() })
        } else {
            let procs = info as *const WTS_PROCESS_INFO_EXW;
            let mut matched = Vec::new();
            let mut sessions = BTreeMap::<u32, u32>::new();
            for i in 0..count as usize {
                // SAFETY: `procs` is an array of `count` entries.
                let p = unsafe { &*procs.add(i) };
                *sessions.entry(p.SessionId).or_default() += 1;
                // SAFETY: pProcessName is a NUL-terminated image name.
                let name = unsafe { ffi::from_wide_ptr(p.pProcessName) };
                if !wanted(&name) {
                    continue;
                }
                let sid = ffi::sid_to_string(p.pUserSid);
                let mut d = describe(p.ProcessId, own.as_deref(), &r);
                d["image"] = json!(name);
                d["pid"] = json!(p.ProcessId);
                d["session"] = json!(p.SessionId);
                d["same_session"] = json!(current_session == Some(p.SessionId));
                d["wts_user_is_this_account"] =
                    json!(sid.as_deref().zip(own.as_deref()).map(|(a, b)| a == b));
                matched.push(d);
            }
            // SAFETY: `info` is the WTS-owned buffer, freed with its type class and count.
            unsafe {
                WTSFreeMemoryExW(
                    WTSTypeProcessInfoLevel1 as WTS_TYPE_CLASS,
                    info.cast(),
                    count,
                );
            }
            json!({
                "ok": true,
                "total": count,
                "processes_by_session": sessions.iter().map(|(s, n)| (s.to_string(), json!(n))).collect::<serde_json::Map<_, _>>(),
                "matched": matched,
            })
        }
    };

    // SAFETY: a pseudo-handle for this process.
    let own_machine = machine_value(machine_of(unsafe { GetCurrentProcess() }));

    // Toolhelp, every session.
    let toolhelp = match snapshot() {
        Err(code) => json!({ "ok": false, "error": code }),
        Ok(all) => {
            let image_of: BTreeMap<u32, &str> =
                all.iter().map(|e| (e.pid, e.image.as_str())).collect();
            let matched: Vec<Value> = all
                .iter()
                .filter(|e| wanted(&e.image))
                .map(|e| {
                    let mut children = BTreeMap::<String, u32>::new();
                    for c in all.iter().filter(|c| c.parent == e.pid) {
                        *children
                            .entry(images::reportable(&c.image).to_string())
                            .or_default() += 1;
                    }
                    let mut d = describe(e.pid, own.as_deref(), &r);
                    d["image"] = json!(e.image);
                    d["pid"] = json!(e.pid);
                    d["session"] = json!(session_of(e.pid));
                    d["same_session"] = json!(session_of(e.pid) == current_session);
                    d["parent_pid"] = json!(e.parent);
                    d["parent_image"] =
                        json!(image_of.get(&e.parent).map(|i| images::reportable(i)));
                    d["children_by_image"] = json!(children);
                    d
                })
                .collect();
            json!({ "ok": true, "total": all.len(), "matched": matched })
        }
    };

    Report::ok(
        "processes",
        logon,
        json!({
            "current_session": current_session,
            "this_process_machine": own_machine,
            "native_machine": machine_value(native_machine()),
            "wts_any_session": wts,
            "toolhelp": toolhelp,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_reports_its_own_native_machine() {
        let want = if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            "x64"
        };
        // SAFETY: a pseudo-handle for this process.
        let own = machine_of(unsafe { GetCurrentProcess() });
        assert_eq!(own.map(crate::pe::machine_name), Some(want));
        assert_eq!(native_machine().map(crate::pe::machine_name), Some(want));
    }
}
