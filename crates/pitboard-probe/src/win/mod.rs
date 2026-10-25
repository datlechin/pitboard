//! The measure half, compiled on Windows alone. Each block runs one measurement and hands
//! back a [`Report`]; the naming, formatting and guards it leans on are the library's
//! cross-platform half.
//!
//! Nothing here reads, writes or deletes a real login. Every block that writes passes
//! [`ffi::refuse_write`] first; the blocks that read what a tool wrote do so only in a
//! scratch folder of a marked account; principals are named by relation and paths pass
//! through the account's [`crate::redact::Redactor`].

pub mod ffi;

mod acl;
mod browserfolder;
mod console;
mod credman;
mod daemon;
mod dpapi;
mod exelookup;
mod files;
mod handover;
mod homes;
mod job;
mod paths;
mod processes;
mod runner;
mod settingchange;
mod sparse;
mod task;
mod times;
mod tokens;
mod windowless;

use crate::cli::{Command, Outcome};
use crate::logon::LogonSession;
use crate::report::Report;
use serde_json::json;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::LUID;
use windows_sys::Win32::Security::Authentication::Identity::{
    LsaFreeReturnBuffer, LsaGetLogonSessionData, SECURITY_LOGON_SESSION_DATA,
};
use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_STATISTICS, TokenStatistics};
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;

/// Run one command.
pub fn run(command: &Command, out: Option<&Path>) -> Outcome {
    use Command as C;
    let report = match command {
        C::Logon => logon_block(),
        C::Tokens { scratch } => tokens::tokens(scratch.as_deref()),
        C::Safer { spawn, scratch } => tokens::safer(*spawn, scratch.as_deref()),
        C::Homes {
            codex_home,
            claude_config_dir,
        } => homes::homes(codex_home.as_deref(), claude_config_dir.as_deref()),
        C::PathVars { scratch } => homes::path_vars(scratch),
        C::EnvChild => homes::env_child(),
        C::Acl {
            scratch,
            create,
            keep,
            open,
            remove,
            describe,
        } => acl::acl(
            scratch.as_deref(),
            *create,
            *keep,
            open.as_deref(),
            remove.as_deref(),
            describe.as_deref(),
        ),
        C::Volume { scratch } => files::volume(scratch),
        C::ReplaceLoop {
            scratch,
            rounds,
            lead_seconds,
            reader,
            route,
            seconds,
        } => files::replace_loop(scratch, *rounds, *lead_seconds, *reader, *route, *seconds),
        C::ReplaceReader {
            scratch,
            seconds,
            share,
        } => files::replace_reader(scratch, *seconds, *share),
        C::FlushDir { scratch } => files::flush_dir(scratch),
        C::Lock {
            scratch,
            mode,
            seconds,
            timing,
        } => files::lock(scratch, *mode, *seconds, *timing),
        C::Lockfileex {
            scratch,
            mode,
            seconds,
            share,
        } => files::lockfileex(scratch, *mode, *seconds, *share),
        C::Dpapi {
            scratch,
            action,
            file,
        } => dpapi::dpapi(scratch, *action, file),
        C::RestrictedToken { scratch } => tokens::restricted(scratch),
        C::FdHandover {
            bun,
            script,
            token,
            kind,
        } => handover::fd_handover(bun, script, token.as_bytes(), *kind),
        C::Task {
            action,
            scratch,
            label,
            folder,
            sid_suffix,
            watch_service,
            exec,
            args,
            at_minutes,
            daily_since_minutes,
            daily_in_minutes,
            disabled,
            start_when_available,
            allow_battery,
            logon,
            time_limit,
        } => task::task(&task::Request {
            action: *action,
            scratch: scratch.as_deref(),
            label,
            folder: *folder,
            sid_suffix: *sid_suffix,
            watch_service: *watch_service,
            exec: *exec,
            args: args.as_deref(),
            at_minutes: *at_minutes,
            daily_since_minutes: *daily_since_minutes,
            daily_in_minutes: *daily_in_minutes,
            disabled: *disabled,
            start_when_available: *start_when_available,
            allow_battery: *allow_battery,
            logon: *logon,
            time_limit,
        }),
        C::Console { scratch, with_task } => console::console(scratch, *with_task),
        C::ConsoleChild => console::console_child(),
        C::ConsoleLaunch {
            exe,
            flags,
            child_out,
        } => return console::console_launch(exe, *flags, child_out),
        C::Job {
            scratch,
            chain,
            rounds,
            start,
            allow_breakaway,
        } => job::job(scratch, *chain, *rounds, *start, *allow_breakaway),
        C::JobBrowser {
            scratch,
            end,
            wait_seconds,
            open,
        } => job::job_browser(scratch, *end, *wait_seconds, *open),
        C::BrowserFolder {
            scratch,
            opener,
            wait_seconds,
            browser,
        } => browserfolder::browser_folder(scratch, *opener, *wait_seconds, browser.as_deref()),
        C::BrowserFolderOpen {
            url,
            opener,
            browser,
        } => return browserfolder::open(url, *opener, browser.as_deref()),
        C::JobLeaf { job_name } => return job::job_leaf(job_name),
        C::JobMid {
            job_name,
            breakaway,
        } => return job::job_mid(job_name, *breakaway),
        C::JobOpenPage {
            page,
            job_name,
            linger_seconds,
            open,
        } => return job::job_open_page(page, job_name, *linger_seconds, *open),
        C::Processes { find } => processes::processes(find),
        C::CredmanNames {
            leak_check,
            config_dir,
        } => credman::names(*leak_check, config_dir),
        C::CredmanItem {
            action,
            name,
            persist,
            blob_bytes,
        } => credman::item(*action, name, *persist, *blob_bytes),
        C::PeImports { scratch, exe } => pe_imports(scratch.as_deref(), exe.as_deref()),
        C::Sparse {
            action,
            tag,
            scratch,
        } => sparse::sparse(*action, tag, scratch.as_deref(), out),
        C::ExeLookup { scratch, name } => exelookup::exe_lookup(scratch, name),
        C::ExeLookupHost { name, child_path } => {
            return exelookup::host(name, child_path.as_deref());
        }
        C::ExeLookupWhoami => return exelookup::whoami(),
        C::Swap {
            scratch,
            target,
            source,
            route,
        } => files::swap(scratch, target, source, *route),
        C::Windowless {
            scratch,
            newline,
            program,
        } => windowless::windowless(scratch, *newline, program),
        C::Daemon {
            config_dir,
            pipe_key,
        } => daemon::daemon(config_dir, pipe_key.as_deref()),
        C::Symlink { scratch } => runner::symlink(scratch),
        C::RunnerFacts { scratch } => runner::runner_facts(scratch),
        C::ExePath => exe_path(),
        C::FileId { scratch } => paths::file_id_block(scratch),
        C::ReservedNames { scratch } => paths::reserved_names(scratch),
        C::LongPath { scratch } => paths::long_path(scratch),
        C::ProcessTimes { children } => times::process_times(*children),
        C::ProcessTimesChild => return times::child(),
        C::SettingChange {
            action: crate::cli::SettingAction::Watch,
            name,
            seconds,
            scratch,
        } => settingchange::watch(name, *seconds, scratch.as_deref()),
        C::SettingChange {
            action: crate::cli::SettingAction::Broadcast,
            name,
            ..
        } => settingchange::broadcast(name),
    };
    Outcome::Report(report)
}

// CF8: a probe started with a link as its only argument, as Pitboard.exe is.
pub fn handed(link: &str) -> Outcome {
    Outcome::Report(sparse::handed(link))
}

/// E5: the path this program reports for itself, as std and as `GetModuleFileNameW` give it,
/// beside how it was started (`argv[0]`) and where its links lead, so a run through a
/// WinGet link, a Scoop shim, a junction or a copy shows which path a scheduled task would
/// be given. Every path is redacted.
fn exe_path() -> Report {
    let logon = logon_now();
    let r = ffi::redactor();
    let red = |p: &Path| r.redact(&p.display().to_string());
    let mut buf = vec![0u16; 32_768];
    // SAFETY: a null module is this program; `buf` holds `buf.len()` units.
    let n = unsafe { GetModuleFileNameW(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32) }
        as usize;
    let (module, module_error) = if n > 0 && n < buf.len() {
        (Some(PathBuf::from(ffi::from_wide_buf(&buf))), None)
    } else {
        (None, Some(ffi::last_error()))
    };
    let current = std::env::current_exe().ok();
    let canonical = module
        .as_deref()
        .and_then(|m| std::fs::canonicalize(m).ok());
    let argv0 = std::env::args_os().next().map(PathBuf::from);
    let link = |p: &Path| {
        std::fs::symlink_metadata(p)
            .ok()
            .map(|m| m.file_type().is_symlink())
    };
    Report::ok(
        "exe-path",
        logon,
        json!({
            "module_file_name": module.as_deref().map(red),
            "module_file_name_error": module_error,
            "current_exe": current.as_deref().map(red),
            "argv0": argv0.as_deref().map(red),
            "canonical": canonical.as_deref().map(red),
            "module_is_a_symbolic_link": module.as_deref().and_then(link),
            "current_exe_equals_module": current.is_some() && current == module,
            "module_hard_links": module.as_deref().and_then(hard_links),
        }),
    )
}

/// How many names the file at `path` has, by its handle's link count.
fn hard_links(path: &Path) -> Option<u32> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let file = std::fs::File::open(path).ok()?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: an open file handle; `info` is a valid out-param.
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) };
    (ok != 0).then_some(info.nNumberOfLinks)
}

/// Write a report to `out`: an absolute path whose file is a `pitboard-probe-*` name, so it
/// can never be a file the probe did not make, in a folder the write guard allows, in a
/// marked account; the file itself passes the guard too, so a link there to a login file is
/// refused.
pub fn write_out(out: &Path, text: &str) -> Result<(), String> {
    let name = out
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("--out names no file")?;
    let folder = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or("--out must be an absolute path")?;
    let path = ffi::scratch_file(folder, name)?;
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

/// The logon this process runs in.
pub fn logon_now() -> LogonSession {
    let Some(luid) = auth_luid() else {
        return LogonSession::unknown();
    };
    let mut data: *mut SECURITY_LOGON_SESSION_DATA = std::ptr::null_mut();
    // SAFETY: `luid` is the token's authentication id; `data` receives an LSA buffer freed
    // below.
    let status = unsafe { LsaGetLogonSessionData(&luid, &mut data) };
    if status != 0 || data.is_null() {
        return LogonSession::unknown();
    }
    // SAFETY: on success `data` points at a valid SECURITY_LOGON_SESSION_DATA.
    let ty = unsafe { (*data).LogonType };
    // SAFETY: `data` came from LsaGetLogonSessionData.
    unsafe {
        LsaFreeReturnBuffer(data as *const core::ffi::c_void);
    }
    LogonSession::from_security_logon_type(ty)
}

/// The token's authentication LUID, which names its logon session.
fn auth_luid() -> Option<LUID> {
    let token = ffi::Token::current().ok()?;
    let mut stats = TOKEN_STATISTICS::default();
    let mut len = 0u32;
    // SAFETY: `stats` is sized for TOKEN_STATISTICS.
    let ok = unsafe {
        GetTokenInformation(
            token.raw(),
            TokenStatistics,
            (&mut stats as *mut TOKEN_STATISTICS).cast(),
            size_of::<TOKEN_STATISTICS>() as u32,
            &mut len,
        )
    };
    (ok != 0).then_some(stats.AuthenticationId)
}

fn logon_block() -> Report {
    let logon = logon_now();
    Report::ok(
        "logon",
        logon,
        json!({
            "note": "logon_expects_user_keys is an expectation: the dpapi and credman blocks \
                     make their calls in any logon and record what happened",
        }),
    )
}

/// R1: a program's imported DLLs. The probe itself, or a program the owner copied under a
/// scratch folder; never a file elsewhere.
fn pe_imports(scratch: Option<&Path>, exe: Option<&Path>) -> Report {
    let logon = logon_now();
    let path = match (exe, scratch) {
        (None, _) => match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => return Report::refused("pe-imports", logon, format!("no current exe: {e}")),
        },
        (Some(exe), Some(scratch)) => {
            if let Err(reason) = ffi::scratch_check(scratch) {
                return Report::refused("pe-imports", logon, reason);
            }
            if !crate::guard::lies_in(exe, scratch) {
                return Report::refused(
                    "pe-imports",
                    logon,
                    "--exe must be a program copied under --scratch",
                );
            }
            exe.to_path_buf()
        }
        (Some(_), None) => {
            return Report::refused("pe-imports", logon, "--exe needs --scratch");
        }
    };
    match std::fs::read(&path) {
        Ok(bytes) => {
            let dlls = crate::pe::import_dlls(&bytes);
            Report::ok(
                "pe-imports",
                logon,
                json!({
                    "exe": path.file_name().map(|n| n.to_string_lossy().into_owned()),
                    "import_dlls": dlls,
                    "takes_c_runtime_from_dll": crate::pe::takes_c_runtime_from_dll(&dlls),
                }),
            )
        }
        Err(e) => Report::refused("pe-imports", logon, format!("cannot read the exe: {e}")),
    }
}

/// A program beside this one, by name (with `.exe`).
pub fn sibling_exe(name: &str) -> PathBuf {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    dir.join(format!("{name}.exe"))
}

/// The report of a child that wrote one with `--out`, read back and removed. `None` when it
/// wrote none.
pub fn take_child_report(path: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    let _ = std::fs::remove_file(path);
    serde_json::from_str(&text).ok()
}

/// An `io::Error` as its raw Windows code, or its kind when it has none.
pub fn io_code(e: &std::io::Error) -> serde_json::Value {
    match e.raw_os_error() {
        Some(code) => json!(code),
        None => json!(format!("{:?}", e.kind())),
    }
}
