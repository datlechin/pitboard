//! A1 and the runner facts: the process token as Pitboard reads it, the owner a file made
//! with it gets, and a Safer normal-user token computed from it (the CI fallback the check
//! asks about: it must read as not elevated, and a program started with it must run).

use super::ffi::{self, Token};
use super::{acl, logon_now, take_child_report};
use crate::cli::DpapiAction;
use crate::elevation::{self, integrity_word};
use crate::report::Report;
use serde_json::{Value, json};
use std::ffi::OsStr;
use std::path::Path;

use windows_sys::Win32::Foundation::{GENERIC_ALL, HANDLE, HLOCAL, LUID, LocalFree};
use windows_sys::Win32::Security::AppLocker::{
    SAFER_LEVEL_OPEN, SAFER_LEVELID_NORMALUSER, SAFER_SCOPEID_USER, SaferCloseLevel,
    SaferComputeTokenFromLevel, SaferCreateLevel,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, ConvertStringSidToSidW,
    EXPLICIT_ACCESS_W, GRANT_ACCESS, SE_WINDOW_OBJECT, SetEntriesInAclW, SetSecurityInfo,
    TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
};
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};
use windows_sys::Win32::Security::{
    ACL, AdjustTokenPrivileges, CreateRestrictedToken, CreateWellKnownSid,
    DACL_SECURITY_INFORMATION, DISABLE_MAX_PRIVILEGE, GetTokenInformation, LUA_TOKEN, PSID,
    SAFER_LEVEL_HANDLE, SE_PRIVILEGE_ENABLED, SECURITY_ATTRIBUTES, SID_AND_ATTRIBUTES,
    SetTokenInformation, TOKEN_ADJUST_DEFAULT, TOKEN_ADJUST_PRIVILEGES, TOKEN_ADJUST_SESSIONID,
    TOKEN_ASSIGN_PRIMARY, TOKEN_DEFAULT_DACL, TOKEN_DUPLICATE, TOKEN_GROUPS, TOKEN_PRIVILEGES,
    TOKEN_QUERY, TokenDefaultDacl, TokenGroups, WRITE_RESTRICTED, WinWorldSid,
};
use windows_sys::Win32::Security::{LookupPrivilegeValueW, PSECURITY_DESCRIPTOR};
use windows_sys::Win32::Storage::FileSystem::{CreateDirectoryW, READ_CONTROL};
use windows_sys::Win32::System::Diagnostics::Debug::{SEM_FAILCRITICALERRORS, SetErrorMode};
use windows_sys::Win32::System::StationsAndDesktops::{
    CloseDesktop, CreateDesktopW, DESKTOP_CREATEMENU, DESKTOP_CREATEWINDOW, DESKTOP_DELETE,
    DESKTOP_ENUMERATE, DESKTOP_HOOKCONTROL, DESKTOP_JOURNALPLAYBACK, DESKTOP_JOURNALRECORD,
    DESKTOP_READ_CONTROL, DESKTOP_READOBJECTS, DESKTOP_SWITCHDESKTOP, DESKTOP_WRITE_DAC,
    DESKTOP_WRITE_OWNER, DESKTOP_WRITEOBJECTS, HDESK,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, GetCurrentProcess, OpenProcessToken,
};

/// What Pitboard reads from a token, with every principal named by relation.
pub(super) fn describe(token: &Token) -> Value {
    let facts = token.facts();
    let reading = elevation::reading(facts);
    let user = token.user_sid();
    json!({
        "elevation_type": facts.elevation_type.map(elevation::ElevationType::as_str),
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

pub(super) fn tokens(scratch: Option<&Path>) -> Report {
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

pub(super) fn safer(spawn: bool, scratch: Option<&Path>) -> Report {
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
        data["spawned"] =
            match ffi::spawn(Some(token.raw()), &exe, &args, CREATE_NO_WINDOW, None, None) {
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

const SE_GROUP_LOGON_ID: u32 = 0xC000_0000;

/// A copy of the logon session SID in `token`'s groups, which Codex uses as a restricting SID.
fn logon_sid_bytes(token: &Token) -> Option<Vec<u8>> {
    let mut len = 0u32;
    // SAFETY: a sizing call with a null buffer returns the needed length.
    unsafe {
        GetTokenInformation(token.raw(), TokenGroups, std::ptr::null_mut(), 0, &mut len);
    }
    if len == 0 {
        return None;
    }
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: `buf` holds at least `len` bytes.
    let ok = unsafe {
        GetTokenInformation(
            token.raw(),
            TokenGroups,
            buf.as_mut_ptr().cast(),
            len,
            &mut len,
        )
    };
    if ok == 0 {
        return None;
    }
    // SAFETY: the buffer begins with a TOKEN_GROUPS; its Groups array holds GroupCount entries.
    let groups = unsafe { &*(buf.as_ptr() as *const TOKEN_GROUPS) };
    let count = groups.GroupCount as usize;
    let first = std::ptr::addr_of!(groups.Groups) as *const SID_AND_ATTRIBUTES;
    for i in 0..count {
        // SAFETY: `i` < GroupCount, so this entry lies inside the buffer.
        let entry = unsafe { &*first.add(i) };
        if entry.Attributes & SE_GROUP_LOGON_ID == SE_GROUP_LOGON_ID {
            return copy_sid(entry.Sid);
        }
    }
    None
}

/// A well-known SID's bytes, owned.
fn well_known_sid(kind: i32) -> Option<Vec<u8>> {
    let mut len = 0u32;
    // SAFETY: a sizing call with a null buffer returns the needed length.
    unsafe {
        CreateWellKnownSid(kind, std::ptr::null_mut(), std::ptr::null_mut(), &mut len);
    }
    if len == 0 {
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    // SAFETY: `buf` holds `len` bytes, the size the first call asked for.
    let ok = unsafe {
        CreateWellKnownSid(
            kind,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut len,
        )
    };
    (ok != 0).then_some(buf)
}

/// The bytes of the SID at `sid`, by its own length (8 + 4 per sub-authority), owned.
fn copy_sid(sid: PSID) -> Option<Vec<u8>> {
    if sid.is_null() {
        return None;
    }
    // SAFETY: a SID's second byte is its sub-authority count; the whole SID is that long.
    let count = unsafe { *(sid as *const u8).add(1) } as usize;
    let len = 8 + 4 * count;
    // SAFETY: the SID is `len` bytes, per its own layout.
    Some(unsafe { std::slice::from_raw_parts(sid as *const u8, len) }.to_vec())
}

// Codex's unelevated sandbox token (`codex_windows_sandbox_token`), with a random capability
// SID and SeChangeNotifyPrivilege on so a folder can be traversed.
fn restricted_token() -> Result<(Token, Vec<&'static str>), String> {
    let mut base: HANDLE = std::ptr::null_mut();
    // SAFETY: a pseudo-handle for this process; `base` receives a token handle.
    let ok = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_DUPLICATE
                | TOKEN_QUERY
                | TOKEN_ASSIGN_PRIMARY
                | TOKEN_ADJUST_DEFAULT
                | TOKEN_ADJUST_SESSIONID
                | TOKEN_ADJUST_PRIVILEGES,
            &mut base,
        )
    };
    if ok == 0 {
        return Err(format!("OpenProcessToken failed: {}", ffi::last_error()));
    }
    let base = Token::owned(base);
    let cap = LocalSid::random_capability().ok_or("could not make a random capability SID")?;
    let mut logon = logon_sid_bytes(&base).ok_or("no logon SID on this token")?;
    let mut everyone =
        well_known_sid(WinWorldSid).ok_or("CreateWellKnownSid for Everyone failed")?;
    let entries = [
        cap.raw(),
        logon.as_mut_ptr().cast(),
        everyone.as_mut_ptr().cast(),
    ]
    .map(|p| SID_AND_ATTRIBUTES {
        Sid: p,
        Attributes: 0,
    });
    let mut out: HANDLE = std::ptr::null_mut();
    // SAFETY: `base` is open for duplication; `entries` point at live SID buffers; `out`
    // receives a new primary token.
    let ok = unsafe {
        CreateRestrictedToken(
            base.raw(),
            DISABLE_MAX_PRIVILEGE | LUA_TOKEN | WRITE_RESTRICTED,
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            entries.len() as u32,
            entries.as_ptr(),
            &mut out,
        )
    };
    if ok == 0 {
        return Err(format!(
            "CreateRestrictedToken failed: {}",
            ffi::last_error()
        ));
    }
    let token = Token::owned(out);
    set_default_dacl(&token, logon.as_mut_ptr().cast())?;
    enable_change_notify(&token);
    Ok((token, vec!["capability", "logon", "everyone"]))
}

/// A SID allocated by `ConvertStringSidToSidW`, freed with `LocalFree` on drop.
struct LocalSid(PSID);

impl LocalSid {
    /// A random `S-1-5-21-a-b-c-d` capability SID, as Codex's `cap.rs` makes one.
    fn random_capability() -> Option<LocalSid> {
        let mut parts = [0u32; 4];
        // SAFETY: `parts` is 16 bytes; the system-preferred RNG needs no algorithm handle.
        let status = unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                parts.as_mut_ptr().cast(),
                size_of_val(&parts) as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        };
        if status != 0 {
            return None;
        }
        let text = ffi::wide(format!(
            "S-1-5-21-{}-{}-{}-{}",
            parts[0], parts[1], parts[2], parts[3]
        ));
        let mut sid: PSID = std::ptr::null_mut();
        // SAFETY: `text` is NUL-terminated; `sid` receives a LocalAlloc SID freed on drop.
        let ok = unsafe { ConvertStringSidToSidW(text.as_ptr(), &mut sid) };
        (ok != 0 && !sid.is_null()).then_some(LocalSid(sid))
    }

    fn raw(&self) -> PSID {
        self.0
    }
}

impl Drop for LocalSid {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a LocalAlloc SID from ConvertStringSidToSidW.
        unsafe {
            LocalFree(self.0.cast::<core::ffi::c_void>() as HLOCAL);
        }
    }
}

/// Set `token`'s default DACL to Codex's: the logon SID GENERIC_ALL, OWNER RIGHTS (`S-1-3-4`)
/// READ_CONTROL, so a write-restricted child's own process, thread and IPC objects grant the
/// restricting logon SID and a different logon cannot rewrite the DACL.
fn set_default_dacl(token: &Token, logon_sid: PSID) -> Result<(), String> {
    let owner_rights = ffi::wide("S-1-3-4");
    let mut owner_sid: PSID = std::ptr::null_mut();
    // SAFETY: `owner_rights` is NUL-terminated; `owner_sid` receives a LocalAlloc SID freed below.
    let ok = unsafe { ConvertStringSidToSidW(owner_rights.as_ptr(), &mut owner_sid) };
    if ok == 0 || owner_sid.is_null() {
        return Err(format!(
            "ConvertStringSidToSidW(S-1-3-4) failed: {}",
            ffi::last_error()
        ));
    }
    let entries = [(logon_sid, GENERIC_ALL), (owner_sid, READ_CONTROL)].map(|(sid, access)| {
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: access,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: 0,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: std::ptr::null_mut(),
                MultipleTrusteeOperation: 0,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_UNKNOWN,
                ptstrName: sid.cast(),
            },
        }
    });
    let mut dacl: *mut ACL = std::ptr::null_mut();
    // SAFETY: `entries` name two live SIDs; `dacl` receives a LocalAlloc ACL freed below.
    let res = unsafe {
        SetEntriesInAclW(
            entries.len() as u32,
            entries.as_ptr(),
            std::ptr::null(),
            &mut dacl,
        )
    };
    // SAFETY: `owner_sid` is the LocalAlloc SID from the conversion.
    unsafe {
        LocalFree(owner_sid.cast::<core::ffi::c_void>() as HLOCAL);
    }
    if res != 0 {
        return Err(format!("SetEntriesInAclW failed: {res}"));
    }
    let info = TOKEN_DEFAULT_DACL { DefaultDacl: dacl };
    // SAFETY: `token` is open for TOKEN_ADJUST_DEFAULT; `info` names a live DACL.
    let ok = unsafe {
        SetTokenInformation(
            token.raw(),
            TokenDefaultDacl,
            std::ptr::addr_of!(info).cast(),
            size_of::<TOKEN_DEFAULT_DACL>() as u32,
        )
    };
    let err = ffi::last_error();
    // SAFETY: `dacl` is the LocalAlloc ACL from SetEntriesInAclW.
    unsafe {
        LocalFree(dacl.cast::<core::ffi::c_void>() as HLOCAL);
    }
    if ok == 0 {
        return Err(format!(
            "SetTokenInformation(TokenDefaultDacl) failed: {err}"
        ));
    }
    Ok(())
}

/// Enable SeChangeNotifyPrivilege on `token`, so a run under it may traverse a directory.
fn enable_change_notify(token: &Token) {
    let mut luid = LUID {
        LowPart: 0,
        HighPart: 0,
    };
    let name = ffi::wide("SeChangeNotifyPrivilege");
    // SAFETY: `name` is NUL-terminated; `luid` is a valid out-param.
    if unsafe { LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) } == 0 {
        return;
    }
    let tp = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [windows_sys::Win32::Security::LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    // SAFETY: `token` is open for adjusting privileges; `tp` names one privilege.
    unsafe {
        AdjustTokenPrivileges(
            token.raw(),
            0,
            &tp,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
    }
}

/// A directory the restricted token may write: Everyone is granted, and Everyone is one of
/// the token's restricting SIDs, so a write-restricted child can leave its report there.
fn make_shared_dir(path: &Path) -> Result<(), String> {
    let sddl = ffi::wide("D:(A;OICI;FA;;;WD)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)");
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: `sddl` is NUL-terminated; `sd` receives a LocalAlloc descriptor freed below.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut sd,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 || sd.is_null() {
        return Err(format!("SDDL conversion failed: {}", ffi::last_error()));
    }
    let attrs = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd,
        bInheritHandle: 0,
    };
    let w = ffi::wide(path);
    // SAFETY: `w` is NUL-terminated; `attrs` holds a valid descriptor.
    let made = unsafe { CreateDirectoryW(w.as_ptr(), &attrs) };
    let err = ffi::last_error();
    // SAFETY: `sd` is the LocalAlloc descriptor from the conversion.
    unsafe {
        LocalFree(sd);
    }
    if made == 0 {
        Err(format!("cannot make the shared out folder: {err}"))
    } else {
        Ok(())
    }
}

/// A private desktop the restricted child can reach, granted to the logon SID like Codex's
/// `desktop.rs`, named `Winsta0\<name>` for `STARTUPINFOW.lpDesktop`. Closed on drop.
struct Desktop {
    handle: HDESK,
    startup: Vec<u16>,
}

const DESKTOP_ALL: u32 = DESKTOP_READOBJECTS
    | DESKTOP_CREATEWINDOW
    | DESKTOP_CREATEMENU
    | DESKTOP_HOOKCONTROL
    | DESKTOP_JOURNALRECORD
    | DESKTOP_JOURNALPLAYBACK
    | DESKTOP_ENUMERATE
    | DESKTOP_WRITEOBJECTS
    | DESKTOP_SWITCHDESKTOP
    | DESKTOP_DELETE
    | DESKTOP_READ_CONTROL
    | DESKTOP_WRITE_DAC
    | DESKTOP_WRITE_OWNER;

impl Desktop {
    fn create(logon_sid: PSID) -> Result<Desktop, String> {
        let mut nonce = [0u8; 16];
        // SAFETY: `nonce` is 16 bytes; the system-preferred RNG needs no algorithm handle.
        let _ = unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                nonce.as_mut_ptr(),
                nonce.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        };
        let name = format!("pitboard-probe-d2b-{}", hex::encode(nonce));
        let name_wide = ffi::wide(&name);
        // SAFETY: `name_wide` is NUL-terminated; a null device, DEVMODE and SA are allowed.
        let handle = unsafe {
            CreateDesktopW(
                name_wide.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                DESKTOP_ALL,
                std::ptr::null(),
            )
        };
        if handle.is_null() {
            return Err(format!("CreateDesktopW failed: {}", ffi::last_error()));
        }
        let desktop = Desktop {
            handle,
            startup: ffi::wide(format!(r"Winsta0\{name}")),
        };
        desktop.grant(logon_sid)?;
        Ok(desktop)
    }

    /// Grant the logon SID DESKTOP_ALL, so a child whose restricting SIDs include it may use
    /// the desktop under a write-restricted token.
    fn grant(&self, logon_sid: PSID) -> Result<(), String> {
        let entry = EXPLICIT_ACCESS_W {
            grfAccessPermissions: DESKTOP_ALL,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: 0,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: std::ptr::null_mut(),
                MultipleTrusteeOperation: 0,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_UNKNOWN,
                ptstrName: logon_sid.cast(),
            },
        };
        let mut dacl: *mut ACL = std::ptr::null_mut();
        // SAFETY: `entry` names a live SID; `dacl` receives a LocalAlloc ACL freed below.
        let res = unsafe { SetEntriesInAclW(1, &entry, std::ptr::null(), &mut dacl) };
        if res != 0 {
            return Err(format!("SetEntriesInAclW for the desktop failed: {res}"));
        }
        // SAFETY: `self.handle` is an open desktop; `dacl` is a live ACL.
        let set = unsafe {
            SetSecurityInfo(
                self.handle,
                SE_WINDOW_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                dacl,
                std::ptr::null(),
            )
        };
        // SAFETY: `dacl` is the LocalAlloc ACL from SetEntriesInAclW.
        unsafe {
            LocalFree(dacl.cast::<core::ffi::c_void>() as HLOCAL);
        }
        if set != 0 {
            return Err(format!("SetSecurityInfo for the desktop failed: {set}"));
        }
        Ok(())
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        // SAFETY: `self.handle` is an open desktop this value owns.
        unsafe {
            CloseDesktop(self.handle);
        }
    }
}

/// D2b/H7: whether a restricted token like Codex's unelevated sandbox unseals a DPAPI park
/// and opens a vault-style file. The unrestricted parent seals a dummy park and makes a
/// protected file; a child under the restricted token, on a private desktop granted to the
/// logon SID, then runs `dpapi --action unseal` and `acl --open` against them, writing its
/// report to a folder Everyone may write. A child that does not write a report within the
/// timeout is terminated and reported `timed_out`, and the block is not `ok` when either
/// report is missing.
pub(super) fn restricted(scratch: &Path) -> Report {
    let logon = logon_now();
    let own = Token::current().ok().and_then(|t| t.user_sid());
    if let Some(reason) = ffi::refuse_write(scratch) {
        return Report::refused("restricted-token", logon, reason);
    }
    if let Err(e) = std::fs::create_dir_all(scratch) {
        return Report::refused(
            "restricted-token",
            logon,
            format!("cannot make the scratch folder: {e}"),
        );
    }
    let sealed_name = "pitboard-probe-d2b-sealed.bin";
    let protected_name = format!("{}protected", crate::PROBE_PREFIX);
    let seal = super::dpapi::dpapi(scratch, DpapiAction::Seal, sealed_name);
    let protected = acl::create_protected(scratch, own.as_deref(), true);
    let out_dir = scratch.join("pitboard-probe-d2b-out");
    let _ = std::fs::remove_dir_all(&out_dir);
    if let Err(reason) = make_shared_dir(&out_dir) {
        return Report::refused("restricted-token", logon, reason);
    }

    let (token, restricting) = match restricted_token() {
        Ok(t) => t,
        Err(reason) => return Report::refused("restricted-token", logon, reason),
    };
    let logon_sid = logon_sid_bytes(&token);
    let desktop = match logon_sid
        .as_ref()
        .ok_or_else(|| "no logon SID on the restricted token".to_string())
        .and_then(|sid| Desktop::create(sid.as_ptr() as PSID))
    {
        Ok(d) => d,
        Err(reason) => return Report::refused("restricted-token", logon, reason),
    };
    // A DLL that fails to initialise under the restricted token exits 0xC0000142 instead of
    // opening a hard-error box that would hang the child, as Codex's process.rs guards against.
    // SAFETY: SetErrorMode only reads and sets this thread's error mode.
    unsafe {
        SetErrorMode(SEM_FAILCRITICALERRORS);
    }
    let exe = std::env::current_exe().unwrap_or_default();
    let scratch_arg = scratch.as_os_str();
    let protected_path = scratch.join(&protected_name);

    let unseal = spawn_under(
        &token,
        &desktop,
        &exe,
        &out_dir,
        "unseal",
        &[
            OsStr::new("dpapi"),
            OsStr::new("--scratch"),
            scratch_arg,
            OsStr::new("--action"),
            OsStr::new("unseal"),
            OsStr::new("--file"),
            OsStr::new(sealed_name),
        ],
    );
    let open = spawn_under(
        &token,
        &desktop,
        &exe,
        &out_dir,
        "open",
        &[
            OsStr::new("acl"),
            OsStr::new("--scratch"),
            scratch_arg,
            OsStr::new("--open"),
            protected_path.as_os_str(),
        ],
    );

    let _ = std::fs::remove_file(scratch.join(sealed_name));
    let _ = std::fs::remove_file(&protected_path);
    let _ = std::fs::remove_dir_all(&out_dir);

    let missing = unseal["report"].is_null() || open["report"].is_null();
    let data = json!({
        "flags": "DISABLE_MAX_PRIVILEGE | LUA_TOKEN | WRITE_RESTRICTED",
        "restricting_sids": restricting,
        "restricted_token": describe(&token),
        "sealed": seal.to_json(),
        "protected": protected.unwrap_or_else(|e| json!({ "error": e })),
        "unseal": unseal,
        "open": open,
    });
    if missing {
        Report::incomplete(
            "restricted-token",
            logon,
            data,
            "a restricted child wrote no report within the timeout",
        )
    } else {
        Report::ok("restricted-token", logon, data)
    }
}

/// Start the probe under `token` on `desktop` with `--out` in `out_dir`, wait, read its report
/// back, and terminate a child that did not write one in time.
fn spawn_under(
    token: &Token,
    desktop: &Desktop,
    exe: &Path,
    out_dir: &Path,
    label: &str,
    args: &[&OsStr],
) -> Value {
    let out = out_dir.join(format!("pitboard-probe-d2b-{label}.json"));
    let mut full: Vec<&OsStr> = vec![OsStr::new("--out"), out.as_os_str()];
    full.extend_from_slice(args);
    match ffi::spawn(
        Some(token.raw()),
        exe,
        &full,
        CREATE_NO_WINDOW,
        None,
        Some(&desktop.startup),
    ) {
        Ok(child) => {
            let exit = child.wait_ms(30_000);
            let timed_out = exit.is_none();
            if timed_out {
                child.kill();
            }
            json!({
                "started": true,
                "exit": exit,
                "timed_out": timed_out,
                "report": take_child_report(&out),
            })
        }
        Err(code) => json!({ "started": false, "error": code }),
    }
}
