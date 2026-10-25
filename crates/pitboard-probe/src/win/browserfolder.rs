//! F3: whether the browser a sign-in opens keeps the folder the sign-in ran in. The probe
//! serves its own page on loopback and opens it from a fresh folder through a child that
//! starts there, opens the page as `--as` names and exits, as a sign-in ends. It reads each
//! process's working folder from its PEB, and names only browsers and the shell's programs,
//! never a path.

use super::ffi::{self, Owned};
use super::job::{alive, guarded, http_open_command, start_as_codex, start_association};
use super::processes::{image_path, snapshot};
use super::{io_code, logon_now};
use crate::cli::{Opener, Outcome};
use crate::report::Report;
use crate::{guard, images, loopback};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
use windows_sys::Win32::Foundation::{HANDLE, UNICODE_STRING};
use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_TERMINATE, PROCESS_VM_READ, TerminateProcess,
};

// A 64-bit process's PEB holds its parameters at 0x20, and they hold CurrentDirectory.DosPath
// at 0x38, among the fields windows-sys leaves reserved.
const PEB_PROCESS_PARAMETERS: usize = 0x20;
const PARAMETERS_CURRENT_DIRECTORY: usize = 0x38;
// Time for the shell's programs to hand over and exit once the page has been asked for.
const SETTLE: Duration = Duration::from_secs(5);

// The child's exit codes.
const OPENED: u8 = 0;
const RUNDLL32_FAILED: u8 = 2;
const NOT_THE_PROBES: u8 = 3;
const RUNDLL32_NOT_STARTED: u8 = 5;
const RUNDLL32_STILL_RUNNING: u8 = 6;

// Each path asked for, with when.
type Asked = Arc<Mutex<Vec<(String, Instant)>>>;

pub(super) fn browser_folder(
    scratch: &Path,
    opener: Opener,
    wait_seconds: u64,
    browser: Option<&Path>,
) -> Report {
    const BLOCK: &str = "browser-folder";
    if let Err(r) = guarded(BLOCK, scratch) {
        return r;
    }
    if browser.is_some_and(|b| opener != Opener::Codex || !guard::is_absolute_without_parent(b)) {
        return Report::refused(
            BLOCK,
            logon_now(),
            "--browser is an absolute path, given with --as codex",
        );
    }
    let (port, asked) = match serve() {
        Ok(served) => served,
        Err(e) => {
            return Report::refused(
                BLOCK,
                logon_now(),
                format!("cannot serve the page on loopback: {e}"),
            );
        }
    };
    let at_start = pids();
    let browsers_before = snapshot()
        .map(|all| {
            all.iter()
                .filter(|e| images::is_browser_image(&e.image))
                .count()
        })
        .unwrap_or_default();
    let folder = |name: &str| scratch.join(format!("{}f3-{name}", crate::PROBE_PREFIX));
    let (cold, warm) = (folder("cold"), folder("warm"));
    let how = How {
        opener,
        browser,
        port,
        wait_seconds,
    };
    let opened_cold = open_from(&cold, "cold", &how, &asked);
    let opened_warm = open_from(&warm, "warm", &how, &asked);
    let ended = end_started(&at_start);
    std::thread::sleep(Duration::from_secs(3));
    Report::ok(
        BLOCK,
        logon_now(),
        json!({
            "as": format!("{opener:?}"),
            "association_program": association_program(),
            "browser": browser.map(|b| b.file_name().map_or("other".to_string(), |n| {
                images::reportable(&n.to_string_lossy()).to_string()
            })),
            "browsers_running_before": browsers_before,
            "reads_its_own_folder": folders_of(std::process::id())
                .ok()
                .zip(std::env::current_dir().ok())
                .map(|((working, _), here)| guard::same_path(&working, &here)),
            "cold": opened_cold,
            "warm": opened_warm,
            "ended": ended,
            "after_the_browsers_ended": {
                "cold": remove(&cold),
                "warm": remove(&warm),
            },
        }),
    )
}

// How the block opens its page.
struct How<'a> {
    opener: Opener,
    browser: Option<&'a Path>,
    port: u16,
    wait_seconds: u64,
}

// The page opened from `folder`, then what runs there and whether it goes.
fn open_from(folder: &Path, name: &str, how: &How<'_>, asked: &Asked) -> Value {
    let _ = std::fs::remove_dir_all(folder);
    if let Err(e) = std::fs::create_dir(folder) {
        return json!({ "not_made": io_code(&e) });
    }
    let before = pids();
    let started = Instant::now();
    let mut child = Command::new(std::env::current_exe().unwrap_or_default());
    child
        .args([
            "browser-folder-open",
            "--url",
            &loopback::page_url(how.port, name),
        ])
        .args(["--as", opener_flag(how.opener)]);
    if let Some(browser) = how.browser {
        child.arg("--browser").arg(browser);
    }
    let child = child
        .current_dir(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let opener_exit = match child {
        Ok(mut child) => wait_bounded(&mut child, Duration::from_secs(60)),
        Err(e) => json!({ "not_started": io_code(&e) }),
    };
    let as_the_opener_ended = look(folder, &before);
    let path = format!("/{name}");
    let deadline = started + Duration::from_secs(how.wait_seconds);
    let mut requested_after_ms = None;
    while Instant::now() < deadline {
        if let Some(at) = asked_at(asked, &path) {
            requested_after_ms = Some(at.duration_since(started).as_millis() as u64);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(SETTLE);
    json!({
        "opener_exit": opener_exit,
        "as_the_opener_ended": as_the_opener_ended,
        "requested_after_ms": requested_after_ms,
        "once_settled": look(folder, &before),
        "removed": remove(folder),
    })
}

// The processes started since `before` and those running in `folder`, by image, and where
// each browser process runs.
fn look(folder: &Path, before: &BTreeSet<u32>) -> Value {
    let mut started = BTreeMap::<String, u32>::new();
    let mut in_the_folder = BTreeMap::<String, u32>::new();
    let mut browsers = BTreeMap::<String, u32>::new();
    for e in snapshot().unwrap_or_default() {
        let image = images::reportable(&e.image).to_string();
        if !before.contains(&e.pid) {
            *started.entry(image.clone()).or_default() += 1;
        }
        let folders = folders_of(e.pid);
        if let Ok((working, _)) = &folders
            && guard::is_within(working, folder)
        {
            *in_the_folder.entry(image).or_default() += 1;
        }
        if images::is_browser_image(&e.image) {
            let place = match &folders {
                Ok((working, _)) if guard::is_within(working, folder) => "in_the_folder".into(),
                Ok((working, Some(program)))
                    if program
                        .parent()
                        .is_some_and(|p| guard::is_within(working, p)) =>
                {
                    "in_its_program_folder".into()
                }
                Ok(_) => "elsewhere".into(),
                Err(code) => format!("unreadable_{code}"),
            };
            *browsers.entry(place).or_default() += 1;
        }
    }
    json!({
        "processes_started": started,
        "processes_in_the_folder": in_the_folder,
        "browser_processes": browsers,
    })
}

// What a sign-in's process does to open the page, started in the folder it measures.
pub(super) fn open(url: &str, opener: Opener, browser: Option<&Path>) -> Outcome {
    if !loopback::is_page_url(url) || browser.is_some_and(|b| !guard::is_absolute_without_parent(b))
    {
        return Outcome::Code(NOT_THE_PROBES);
    }
    match opener {
        // Claude Code waits for it (`windows_browser_open`).
        Opener::Claude => {
            let child = Command::new("rundll32")
                .args(["url,OpenURL", url])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            let Ok(mut child) = child else {
                return Outcome::Code(RUNDLL32_NOT_STARTED);
            };
            let deadline = Instant::now() + Duration::from_secs(30);
            while Instant::now() < deadline {
                match child.try_wait() {
                    Ok(Some(status)) if status.success() => return Outcome::Code(OPENED),
                    Ok(Some(_)) | Err(_) => return Outcome::Code(RUNDLL32_FAILED),
                    Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                }
            }
            Outcome::Code(RUNDLL32_STILL_RUNNING)
        }
        Opener::Codex => Outcome::Code(match browser {
            Some(browser) => start_as_codex(browser, &[url.to_string()]),
            None => start_association(url),
        }),
    }
}

fn opener_flag(opener: Opener) -> &'static str {
    match opener {
        Opener::Claude => "claude",
        Opener::Codex => "codex",
    }
}

fn association_program() -> Value {
    match http_open_command() {
        Ok(command) => json!(
            crate::assoc::command_args(&command, "u")
                .first()
                .and_then(|p| Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned()))
                .map(|n| images::reportable(&n).to_string())
        ),
        Err(hr) => json!({ "none": format!("0x{hr:08X}") }),
    }
}

fn pids() -> BTreeSet<u32> {
    snapshot()
        .map(|all| all.into_iter().map(|e| e.pid).collect())
        .unwrap_or_default()
}

fn wait_bounded(child: &mut Child, limit: Duration) -> Value {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(status)) => return json!({ "exit": status.code() }),
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return json!({ "wait_failed": io_code(&e) }),
        }
    }
    let _ = child.kill();
    json!({ "timed_out": true })
}

fn remove(folder: &Path) -> Value {
    if !folder.exists() {
        return json!({ "removed": true, "already": true });
    }
    match std::fs::remove_dir(folder) {
        Ok(()) => json!({ "removed": true }),
        Err(e) => json!({ "removed": false, "error": io_code(&e) }),
    }
}

// The browsers and the shell's programs that started since the block began, ended by image.
fn end_started(at_start: &BTreeSet<u32>) -> Value {
    let mut ended = BTreeMap::<String, u32>::new();
    let mut pids = Vec::new();
    for e in snapshot().unwrap_or_default() {
        if at_start.contains(&e.pid)
            || !(images::is_browser_image(&e.image) || images::is_shell_image(&e.image))
        {
            continue;
        }
        // SAFETY: OpenProcess for terminate; the handle is owned below.
        let Some(h) = Owned::new(unsafe { OpenProcess(PROCESS_TERMINATE, 0, e.pid) }) else {
            continue;
        };
        // SAFETY: an open process handle with terminate rights.
        if unsafe { TerminateProcess(h.raw(), 1) } != 0 {
            *ended
                .entry(images::reportable(&e.image).to_string())
                .or_default() += 1;
            pids.push(e.pid);
        }
    }
    std::thread::sleep(Duration::from_secs(2));
    json!({
        "by_image": ended,
        "still_running": pids.iter().filter(|pid| alive(**pid)).count(),
    })
}

fn serve() -> std::io::Result<(u16, Asked)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    let asked = Asked::default();
    let log = Arc::clone(&asked);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let log = Arc::clone(&log);
            std::thread::spawn(move || answer(stream, &log));
        }
    });
    Ok((port, asked))
}

// A connection a browser opens ahead of time may send nothing, so a read gives up.
fn answer(mut stream: TcpStream, log: &Mutex<Vec<(String, Instant)>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut head = Vec::new();
    let mut buf = [0u8; 2048];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") && head.len() < 16 * 1024 {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => head.extend_from_slice(&buf[..n]),
        }
    }
    let Some(path) = loopback::request_path(&head) else {
        return;
    };
    if let Ok(mut log) = log.lock() {
        log.push((path.clone(), Instant::now()));
    }
    let _ = stream.write_all(&loopback::response(&path));
    let _ = stream.flush();
}

fn asked_at(asked: &Asked, path: &str) -> Option<Instant> {
    asked
        .lock()
        .ok()?
        .iter()
        .find(|(p, _)| p == path)
        .map(|(_, at)| *at)
}

fn read_at<T: Copy>(process: HANDLE, at: usize) -> Result<T, u32> {
    let mut value = std::mem::MaybeUninit::<T>::uninit();
    let mut got = 0usize;
    // SAFETY: `value` has room for a T, and at most that many bytes are written.
    let ok = unsafe {
        ReadProcessMemory(
            process,
            at as *const core::ffi::c_void,
            value.as_mut_ptr().cast(),
            size_of::<T>(),
            &mut got,
        )
    };
    if ok == 0 || got != size_of::<T>() {
        return Err(ffi::last_error());
    }
    // SAFETY: every byte of the T was written, and T is plain data.
    Ok(unsafe { value.assume_init() })
}

// The working folder `pid` runs in and its program, or the code that kept the folder from
// being read.
fn folders_of(pid: u32) -> Result<(PathBuf, Option<PathBuf>), u32> {
    // SAFETY: OpenProcess for queries and reads; the handle is owned below.
    let process =
        Owned::new(unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid) })
            .ok_or_else(ffi::last_error)?;
    // PROCESS_BASIC_INFORMATION is six pointer-sized fields, the PEB's address second.
    let mut info = [0usize; 6];
    // SAFETY: `info` is as large as PROCESS_BASIC_INFORMATION and aligned for it.
    let status = unsafe {
        NtQueryInformationProcess(
            process.raw(),
            ProcessBasicInformation,
            info.as_mut_ptr().cast(),
            size_of_val(&info) as u32,
            std::ptr::null_mut(),
        )
    };
    if status < 0 {
        return Err(status as u32);
    }
    let parameters: usize = read_at(process.raw(), info[1] + PEB_PROCESS_PARAMETERS)?;
    let dos_path: UNICODE_STRING =
        read_at(process.raw(), parameters + PARAMETERS_CURRENT_DIRECTORY)?;
    let mut units = vec![0u16; usize::from(dos_path.Length) / 2];
    let mut got = 0usize;
    // SAFETY: `units` has room for the string's Length bytes.
    let ok = unsafe {
        ReadProcessMemory(
            process.raw(),
            dos_path.Buffer.cast_const().cast(),
            units.as_mut_ptr().cast(),
            units.len() * 2,
            &mut got,
        )
    };
    if ok == 0 {
        return Err(ffi::last_error());
    }
    Ok((
        PathBuf::from(std::ffi::OsString::from_wide(&units)),
        image_path(process.raw()).map(PathBuf::from),
    ))
}
