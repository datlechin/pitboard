//! A1 and the runner facts: the process token as Pitboard reads it, the owner a file made
//! with it gets, and a Safer normal-user token computed from it (the CI fallback the check
//! asks about: it must read as not elevated, and a program started with it must run).

use super::ffi::{self, Token};
use super::{acl, logon_now, take_child_report};
use crate::elevation::{self, integrity_word};
use crate::report::Report;
use serde_json::{Value, json};
use std::ffi::OsStr;
use std::path::Path;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Security::AppLocker::{
    SAFER_LEVEL_OPEN, SAFER_LEVELID_NORMALUSER, SAFER_SCOPEID_USER, SaferCloseLevel,
    SaferComputeTokenFromLevel, SaferCreateLevel,
};
use windows_sys::Win32::Security::{
    SAFER_LEVEL_HANDLE, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, GetCurrentProcess, OpenProcessToken,
};

/// What Pitboard reads from a token, with every principal named by relation.
pub fn describe(token: &Token) -> Value {
    let facts = token.facts();
    let reading = elevation::reading(facts);
    let user = token.user_sid();
    json!({
        "elevation_type": facts.elevation_type.map(|e| e.as_str()),
        "token_is_elevated": facts.is_elevated,
        "user_is_service_account": facts.user_is_service,
        "integrity": facts.integrity_rid.map(integrity_word),
        "default_owner": token
            .default_owner_sid()
            .map(|o| elevation::relation(&o, user.as_deref())),
        "reading": reading.as_str(),
        "reading_reason": reading.reason(),
    })
}

pub fn tokens(scratch: Option<&Path>) -> Report {
    let logon = logon_now();
    let token = match Token::current() {
        Ok(t) => t,
        Err(e) => return Report::refused("tokens", logon, format!("OpenProcessToken failed: {e}")),
    };
    let mut data = describe(&token);
    if let Some(scratch) = scratch {
        data["file_made_in_scratch"] = match file_owner(scratch, token.user_sid().as_deref()) {
            Ok(v) => v,
            Err(reason) => return Report::refused("tokens", logon, reason),
        };
    }
    Report::ok("tokens", logon, data)
}

/// Make a file under `scratch`, read its owner's relation to this token, and remove it.
fn file_owner(scratch: &Path, user: Option<&str>) -> Result<Value, String> {
    if let Some(reason) = ffi::refuse_write(scratch) {
        return Err(reason);
    }
    std::fs::create_dir_all(scratch).map_err(|e| format!("cannot make the scratch folder: {e}"))?;
    let file = scratch.join(format!(
        "{}owner-{}.txt",
        crate::PROBE_PREFIX,
        std::process::id()
    ));
    std::fs::write(&file, b"owner").map_err(|e| format!("cannot write a file there: {e}"))?;
    let owner = acl::owner_sid(&file);
    let _ = std::fs::remove_file(&file);
    Ok(match owner {
        Ok(sid) => json!({ "owner": elevation::relation(&sid, user) }),
        Err(code) => json!({ "owner_error": code }),
    })
}

/// A Safer normal-user token computed from this process's token.
fn safer_token() -> Result<Token, String> {
    let mut base: HANDLE = std::ptr::null_mut();
    // SAFETY: a pseudo-handle for this process; `base` receives a token handle.
    let ok = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_DUPLICATE | TOKEN_QUERY | TOKEN_ASSIGN_PRIMARY,
            &mut base,
        )
    };
    if ok == 0 {
        return Err(format!("OpenProcessToken failed: {}", ffi::last_error()));
    }
    let base = Token::owned(base);
    let mut level: SAFER_LEVEL_HANDLE = std::ptr::null_mut();
    // SAFETY: the documented scope and level; `level` receives a handle closed below.
    let ok = unsafe {
        SaferCreateLevel(
            SAFER_SCOPEID_USER,
            SAFER_LEVELID_NORMALUSER,
            SAFER_LEVEL_OPEN,
            &mut level,
            std::ptr::null(),
        )
    };
    if ok == 0 {
        return Err(format!("SaferCreateLevel failed: {}", ffi::last_error()));
    }
    let mut out: HANDLE = std::ptr::null_mut();
    // SAFETY: `level` and `base` are open; `out` receives a new primary token.
    let ok =
        unsafe { SaferComputeTokenFromLevel(level, base.raw(), &mut out, 0, std::ptr::null_mut()) };
    let err = ffi::last_error();
    // SAFETY: `level` came from SaferCreateLevel.
    unsafe {
        SaferCloseLevel(level);
    }
    if ok == 0 {
        return Err(format!("SaferComputeTokenFromLevel failed: {err}"));
    }
    Ok(Token::owned(out))
}

pub fn safer(spawn: bool, scratch: Option<&Path>) -> Report {
    let logon = logon_now();
    let token = match safer_token() {
        Ok(t) => t,
        Err(reason) => return Report::refused("safer", logon, reason),
    };
    let mut data = json!({ "safer_normal_user_token": describe(&token) });
    if spawn {
        let Some(scratch) = scratch else {
            return Report::refused("safer", logon, "--spawn needs --scratch");
        };
        if let Some(reason) = ffi::refuse_write(scratch) {
            return Report::refused("safer", logon, reason);
        }
        let _ = std::fs::create_dir_all(scratch);
        let out = scratch.join(format!(
            "{}safer-{}.json",
            crate::PROBE_PREFIX,
            std::process::id()
        ));
        let exe = std::env::current_exe().unwrap_or_default();
        let args: [&OsStr; 3] = [OsStr::new("--out"), out.as_os_str(), OsStr::new("tokens")];
        data["spawned"] = match ffi::spawn(Some(token.raw()), &exe, &args, CREATE_NO_WINDOW, None) {
            Ok(child) => json!({
                "started": true,
                "exit": child.wait_ms(30_000),
                "report": take_child_report(&out),
            }),
            Err(code) => json!({ "started": false, "error": code }),
        };
    }
    Report::ok("safer", logon, data)
}
