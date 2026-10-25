//! F1 and F2: the Job Object. F1 starts a chain (the probe through the probe to a leaf, or a
//! `.cmd` through `node.exe` to the leaf, as an npm shim does), assigns its head to a job
//! right after starting it, and lets the leaf say whether it ended up in that job; over many
//! rounds this counts the grandchildren that escaped. F2 opens a local stand-in page from a
//! process in a job, then ends the job one way, and records whether the browser was in the
//! job and whether it outlived it. No real site is loaded and only browser images are named.

use super::ffi::{self, Owned};
use super::processes::{open_query, snapshot};
use super::{io_code, logon_now};
use crate::cli::{BrowserOpen, Chain, JobEnd, JobStart, Outcome};
use crate::report::Report;
use crate::{guard, images};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
    JOBOBJECT_BASIC_PROCESS_ID_LIST, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation, OpenJobObjectW,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::SystemServices::JOB_OBJECT_QUERY;
use windows_sys::Win32::System::Threading::{
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NO_WINDOW, CREATE_SUSPENDED, GetCurrentProcess,
    GetExitCodeProcess, OpenProcess, OpenThread, PROCESS_TERMINATE, ResumeThread,
    THREAD_SUSPEND_RESUME, TerminateProcess,
};
use windows_sys::Win32::UI::Shell::{
    ASSOCF_IS_PROTOCOL, ASSOCSTR_COMMAND, AssocQueryStringW, ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

const IN_JOB: u8 = 10;
const NOT_IN_JOB: u8 = 11;
const NO_JOB: u8 = 12;
const LEAF_UNKNOWN: u8 = 13;
const BREAKAWAY_REFUSED: u8 = 14;
const IN_AN_OUTER_JOB_ONLY: u8 = 15;
const NEITHER_BROWSER_NOR_SHELL: u8 = 4;
const STILL_ACTIVE: u32 = 259;

fn create_job(name: &str) -> Result<Owned, u32> {
    let w = ffi::wide(name);
    // SAFETY: `w` is NUL-terminated; the job is owned by the result.
    Owned::new(unsafe { CreateJobObjectW(std::ptr::null(), w.as_ptr()) })
        .ok_or_else(ffi::last_error)
}

fn allow_breakaway(job: &Owned) -> Result<(), u32> {
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_BREAKAWAY_OK;
    // SAFETY: `job` is a job this process made; `limits` is sized for this class.
    let ok = unsafe {
        SetInformationJobObject(
            job.raw(),
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if ok == 0 {
        Err(ffi::last_error())
    } else {
        Ok(())
    }
}

fn in_job(process: HANDLE, job: HANDLE) -> Option<bool> {
    let mut r = 0;
    // SAFETY: both handles are valid or null (null asks about any job).
    (unsafe { IsProcessInJob(process, job, &mut r) } != 0).then_some(r != 0)
}

fn assign(job: &Owned, child: &std::process::Child) -> Result<(), u32> {
    // SAFETY: `job` and the child's process handle are valid.
    let ok = unsafe { AssignProcessToJobObject(job.raw(), child.as_raw_handle() as HANDLE) };
    if ok == 0 {
        Err(ffi::last_error())
    } else {
        Ok(())
    }
}

pub(super) fn guarded(block: &'static str, scratch: &Path) -> Result<(), Report> {
    if let Some(reason) = ffi::refuse_write(scratch) {
        return Err(Report::refused(block, logon_now(), reason));
    }
    std::fs::create_dir_all(scratch).map_err(|e| {
        Report::refused(
            block,
            logon_now(),
            format!("cannot make the scratch folder: {e}"),
        )
    })
}

// W19's route: the one thread a suspended child has, found by a Toolhelp thread snapshot.
fn resume_only_thread(pid: u32) -> Result<(), String> {
    // SAFETY: a thread snapshot of every process; owned below.
    let snap = Owned::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) })
        .ok_or_else(|| format!("snapshot {}", ffi::last_error()))?;
    // SAFETY: THREADENTRY32 is plain data; all zeroes is valid, and dwSize is set below.
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<THREADENTRY32>() as u32;
    let mut threads = Vec::new();
    // SAFETY: `snap` is valid; `entry` is sized.
    let mut more = unsafe { Thread32First(snap.raw(), &mut entry) };
    while more != 0 {
        if entry.th32OwnerProcessID == pid {
            threads.push(entry.th32ThreadID);
        }
        // SAFETY: `snap` and `entry` remain valid.
        more = unsafe { Thread32Next(snap.raw(), &mut entry) };
    }
    let [thread] = threads[..] else {
        return Err(format!("{} threads", threads.len()));
    };
    // SAFETY: OpenThread for resume; the handle is owned below.
    let h = Owned::new(unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, thread) })
        .ok_or_else(|| format!("open {}", ffi::last_error()))?;
    // SAFETY: an open thread handle with suspend-resume rights.
    if unsafe { ResumeThread(h.raw()) } == u32::MAX {
        return Err(format!("resume {}", ffi::last_error()));
    }
    Ok(())
}

pub(super) fn job(
    scratch: &Path,
    chain: Chain,
    rounds: u32,
    start: JobStart,
    breakaway_ok: bool,
) -> Report {
    if let Err(r) = guarded("job", scratch) {
        return r;
    }
    let probe = std::env::current_exe().unwrap_or_default();
    let name = format!("{}job-{}", crate::PROBE_PREFIX, std::process::id());
    let job = match create_job(&name) {
        Ok(j) => j,
        Err(code) => {
            return Report::refused(
                "job",
                logon_now(),
                format!("CreateJobObjectW failed: {code}"),
            );
        }
    };
    if breakaway_ok && let Err(code) = allow_breakaway(&job) {
        return Report::refused(
            "job",
            logon_now(),
            format!("SetInformationJobObject failed: {code}"),
        );
    }
    // SAFETY: a pseudo-handle for this process.
    let parent_in_a_job = in_job(unsafe { GetCurrentProcess() }, std::ptr::null_mut());

    let cmd_path = scratch.join(format!("{}chain.cmd", crate::PROBE_PREFIX));
    let mjs_path = scratch.join(format!("{}chain.mjs", crate::PROBE_PREFIX));
    if chain == Chain::CmdNodeExe {
        if !Command::new("where")
            .arg("node")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        {
            return Report::refused(
                "job",
                logon_now(),
                "node.exe is not on PATH; the cmd-node-exe chain needs it",
            );
        }
        let _ = std::fs::write(
            &cmd_path,
            "@node \"%~dp0pitboard-probe-chain.mjs\" %*\r\n@exit /b %ERRORLEVEL%\r\n",
        );
        let _ = std::fs::write(
            &mjs_path,
            "import { spawnSync } from \"node:child_process\";\n\
             const [probe, jobName] = process.argv.slice(2);\n\
             const r = spawnSync(probe, [\"job-leaf\", \"--job-name\", jobName], { stdio: \"ignore\", windowsHide: true });\n\
             process.exit(r.status ?? 13);\n",
        );
    }

    let mut codes: BTreeMap<String, u32> = BTreeMap::new();
    let (mut assign_failed, mut in_job_n, mut escaped, mut unknown) = (0u32, 0u32, 0u32, 0u32);
    let (mut breakaway_refused, mut resume_failed, mut outer_only) = (0u32, 0u32, 0u32);
    let mut assign_errors: BTreeMap<u32, u32> = BTreeMap::new();
    let mut resume_errors: BTreeMap<String, u32> = BTreeMap::new();
    let flags = match start {
        JobStart::Running => CREATE_NO_WINDOW,
        JobStart::Suspended => CREATE_NO_WINDOW | CREATE_SUSPENDED,
    };
    let started = Instant::now();
    for _ in 0..rounds {
        let mut cmd = match chain {
            Chain::Exe => {
                let mut c = Command::new(&probe);
                c.args(["job-mid", "--job-name", &name]);
                c
            }
            Chain::ExeBreakaway => {
                let mut c = Command::new(&probe);
                c.args(["job-mid", "--job-name", &name, "--breakaway"]);
                c
            }
            Chain::CmdNodeExe => {
                let mut c = Command::new("cmd.exe");
                c.raw_arg(format!(
                    "/d /s /c \"\"{}\" \"{}\" {}\"",
                    cmd_path.display(),
                    probe.display(),
                    name
                ));
                c
            }
        };
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(flags);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                *codes
                    .entry(format!("spawn_error_{}", io_code(&e)))
                    .or_default() += 1;
                continue;
            }
        };
        if let Err(code) = assign(&job, &child) {
            assign_failed += 1;
            *assign_errors.entry(code).or_default() += 1;
        }
        if start == JobStart::Suspended
            && let Err(e) = resume_only_thread(child.id())
        {
            resume_failed += 1;
            *resume_errors.entry(e).or_default() += 1;
            let _ = child.kill();
        }
        let code = child.wait().ok().and_then(|s| s.code());
        *codes.entry(format!("{code:?}")).or_default() += 1;
        match code {
            Some(c) if c == i32::from(IN_JOB) => in_job_n += 1,
            Some(c) if c == i32::from(NOT_IN_JOB) => escaped += 1,
            Some(c) if c == i32::from(IN_AN_OUTER_JOB_ONLY) => {
                escaped += 1;
                outer_only += 1;
            }
            Some(c) if c == i32::from(BREAKAWAY_REFUSED) => breakaway_refused += 1,
            _ => unknown += 1,
        }
    }
    let _ = std::fs::remove_file(&cmd_path);
    let _ = std::fs::remove_file(&mjs_path);
    Report::ok(
        "job",
        logon_now(),
        json!({
            "chain": format!("{chain:?}"),
            "start": format!("{start:?}"),
            "rounds": rounds,
            "breakaway_ok": breakaway_ok,
            "this_process_already_in_a_job": parent_in_a_job,
            "assign_failed": assign_failed,
            "assign_errors_by_code": assign_errors.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<serde_json::Map<_, _>>(),
            "leaf_in_the_job": in_job_n,
            "leaf_escaped": escaped,
            "leaf_escaped_into_an_outer_job": outer_only,
            "leaf_unknown": unknown,
            "breakaway_refused": breakaway_refused,
            "resume_failed": resume_failed,
            "resume_errors": resume_errors,
            "exit_codes": codes,
            "elapsed_ms": started.elapsed().as_millis() as u64,
        }),
    )
}

/// Exit code of a hidden helper given a job name or page that is not the probe's.
const NOT_THE_PROBES: u8 = 3;

pub(super) fn job_leaf(job_name: &str) -> Outcome {
    if !guard::is_probe_object_name(job_name) {
        return Outcome::Code(NOT_THE_PROBES);
    }
    let w = ffi::wide(job_name);
    // SAFETY: `w` is NUL-terminated; the handle is owned below.
    let Some(job) = Owned::new(unsafe { OpenJobObjectW(JOB_OBJECT_QUERY, 0, w.as_ptr()) }) else {
        return Outcome::Code(NO_JOB);
    };
    // SAFETY: a pseudo-handle for this process.
    let me = unsafe { GetCurrentProcess() };
    match in_job(me, job.raw()) {
        Some(true) => Outcome::Code(IN_JOB),
        Some(false) if in_job(me, std::ptr::null_mut()) == Some(true) => {
            Outcome::Code(IN_AN_OUTER_JOB_ONLY)
        }
        Some(false) => Outcome::Code(NOT_IN_JOB),
        None => Outcome::Code(NO_JOB),
    }
}

pub(super) fn job_mid(job_name: &str, breakaway: bool) -> Outcome {
    if !guard::is_probe_object_name(job_name) {
        return Outcome::Code(NOT_THE_PROBES);
    }
    let probe = std::env::current_exe().unwrap_or_default();
    let mut leaf = Command::new(probe);
    leaf.args(["job-leaf", "--job-name", job_name])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if breakaway {
        leaf.creation_flags(CREATE_BREAKAWAY_FROM_JOB);
    }
    match leaf.status() {
        Ok(s) => Outcome::Code(s.code().map_or(LEAF_UNKNOWN, |c| c.clamp(0, 255) as u8)),
        Err(e)
            if breakaway
                && e.raw_os_error() == Some(crate::replace::ERROR_ACCESS_DENIED as i32) =>
        {
            Outcome::Code(BREAKAWAY_REFUSED)
        }
        Err(_) => Outcome::Code(LEAF_UNKNOWN),
    }
}

// The HRESULT where nothing answers for `http`.
pub(super) fn http_open_command() -> Result<String, i32> {
    let http = ffi::wide("http");
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `http` is NUL-terminated; `buf` holds `len` units; no extra string.
    let hr = unsafe {
        AssocQueryStringW(
            ASSOCF_IS_PROTOCOL,
            ASSOCSTR_COMMAND,
            http.as_ptr(),
            std::ptr::null(),
            buf.as_mut_ptr(),
            &mut len,
        )
    };
    if hr == 0 {
        Ok(ffi::from_wide_buf(&buf))
    } else {
        Err(hr)
    }
}

fn open_by_association(page: &Path) -> u8 {
    start_association(&crate::assoc::file_url(&page.display().to_string()))
}

// Codex's route through webbrowser 1.2.4: the association's program, started by std with no
// folder of its own, so it is a child of this process, inside its job and in its folder. Where
// no browser answers for `http`, that program is the shell's `OpenWith.exe`.
pub(super) fn start_association(url: &str) -> u8 {
    let Ok(cmdline) = http_open_command() else {
        return 1;
    };
    let args = crate::assoc::command_args(&cmdline, url);
    let Some((program, rest)) = args.split_first() else {
        return 1;
    };
    start_as_codex(Path::new(program), rest)
}

// A browser or the shell's program, started as webbrowser 1.2.4 starts the association's.
pub(super) fn start_as_codex(program: &Path, args: &[String]) -> u8 {
    let startable = program
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| images::is_browser_image(n) || images::is_shell_image(n));
    if !startable {
        return NEITHER_BROWSER_NOR_SHELL;
    }
    let spawned = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    u8::from(spawned.is_err())
}

// Only the probe's stand-in page, by an absolute path, opened by the shell or a browser alone.
pub(super) fn job_open_page(
    page: &Path,
    job_name: &str,
    linger_seconds: u64,
    open: BrowserOpen,
) -> Outcome {
    let is_standin = page
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case(crate::STANDIN_PAGE));
    if !guard::is_probe_object_name(job_name)
        || !is_standin
        || !guard::is_absolute_without_parent(page)
        || !page.is_file()
    {
        return Outcome::Code(NOT_THE_PROBES);
    }
    let w = ffi::wide(job_name);
    // SAFETY: `w` is NUL-terminated; the handle is owned below.
    let job = Owned::new(unsafe { OpenJobObjectW(JOB_OBJECT_QUERY, 0, w.as_ptr()) });
    let waited = Instant::now();
    loop {
        // SAFETY: a pseudo-handle for this process.
        let inside = job
            .as_ref()
            .and_then(|j| in_job(unsafe { GetCurrentProcess() }, j.raw()));
        if inside == Some(true) {
            break;
        }
        if waited.elapsed() > Duration::from_secs(3) {
            return Outcome::Code(2);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let code = match open {
        BrowserOpen::Assoc => open_by_association(page),
        BrowserOpen::Shell => {
            let verb = ffi::wide("open");
            let file = ffi::wide(page);
            // SAFETY: both strings are NUL-terminated; the other pointers may be null.
            let r = unsafe {
                ShellExecuteW(
                    std::ptr::null_mut(),
                    verb.as_ptr(),
                    file.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    SW_SHOWNORMAL,
                )
            };
            u8::from(r as usize <= 32)
        }
    };
    std::thread::sleep(Duration::from_secs(linger_seconds));
    Outcome::Code(code)
}

fn open_flag(open: BrowserOpen) -> &'static str {
    match open {
        BrowserOpen::Shell => "shell",
        BrowserOpen::Assoc => "assoc",
    }
}

fn browsers() -> BTreeSet<u32> {
    snapshot()
        .map(|all| {
            all.into_iter()
                .filter(|e| images::is_browser_image(&e.image))
                .map(|e| e.pid)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn alive(pid: u32) -> bool {
    open_query(pid).is_ok_and(|h| {
        let mut code = 0u32;
        // SAFETY: an open process handle; `code` is a valid out-param.
        (unsafe { GetExitCodeProcess(h.raw(), &mut code) } != 0) && code == STILL_ACTIVE
    })
}

fn job_pids(job: &Owned) -> Vec<u32> {
    // Room for 256 pids after the list's two counts.
    let mut buf = vec![0usize; 2 + 256];
    let size = (buf.len() * size_of::<usize>()) as u32;
    // SAFETY: `buf` holds `size` bytes, aligned for JOBOBJECT_BASIC_PROCESS_ID_LIST.
    let ok = unsafe {
        QueryInformationJobObject(
            job.raw(),
            JobObjectBasicProcessIdList,
            buf.as_mut_ptr().cast(),
            size,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Vec::new();
    }
    // SAFETY: on success `buf` begins with the list, followed by its ids.
    let list = unsafe { &*(buf.as_ptr() as *const JOBOBJECT_BASIC_PROCESS_ID_LIST) };
    let n = list.NumberOfProcessIdsInList as usize;
    let ids = std::ptr::addr_of!(list.ProcessIdList).cast::<usize>();
    // SAFETY: the list holds `n` ids, within `buf`.
    (0..n.min(256))
        .map(|i| unsafe { *ids.add(i) } as u32)
        .collect()
}

pub(super) fn job_browser(
    scratch: &Path,
    end: JobEnd,
    wait_seconds: u64,
    open: BrowserOpen,
) -> Report {
    if let Err(r) = guarded("job-browser", scratch) {
        return r;
    }
    let page = scratch.join(crate::STANDIN_PAGE);
    let _ = std::fs::write(
        &page,
        "<!doctype html><title>pitboard-probe stand-in</title><p>A local stand-in page. No site is loaded.</p>",
    );
    let before = browsers();
    let name = format!("{}jobb-{}", crate::PROBE_PREFIX, std::process::id());
    let job = match create_job(&name) {
        Ok(j) => j,
        Err(code) => {
            return Report::refused(
                "job-browser",
                logon_now(),
                format!("CreateJobObjectW failed: {code}"),
            );
        }
    };
    let linger = if end == JobEnd::ParentExit {
        0
    } else {
        wait_seconds + 30
    };
    let mut opener = match Command::new(std::env::current_exe().unwrap_or_default())
        .arg("job-open-page")
        .arg("--page")
        .arg(&page)
        .args(["--job-name", &name, "--linger-seconds", &linger.to_string()])
        .args(["--open", open_flag(open)])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return Report::refused(
                "job-browser",
                logon_now(),
                format!("cannot start the opener: {e}"),
            );
        }
    };
    let assigned = assign(&job, &opener);
    std::thread::sleep(Duration::from_secs(wait_seconds));
    let new: Vec<u32> = browsers().difference(&before).copied().collect();
    let in_the_job: Vec<u32> = new
        .iter()
        .copied()
        .filter(|pid| {
            open_query(*pid)
                .ok()
                .and_then(|h| in_job(h.raw(), job.raw()))
                == Some(true)
        })
        .collect();

    let ended = match end {
        JobEnd::TerminateJob => {
            // SAFETY: `job` is a job handle this process made.
            json!({ "terminated": unsafe { TerminateJobObject(job.raw(), 1) } != 0 })
        }
        JobEnd::Allowlist => {
            let mut killed = 0u32;
            let images: BTreeMap<u32, String> = snapshot()
                .map(|all| all.into_iter().map(|e| (e.pid, e.image)).collect())
                .unwrap_or_default();
            for pid in job_pids(&job) {
                if images
                    .get(&pid)
                    .is_some_and(|i| i.to_lowercase().starts_with("pitboard-probe"))
                {
                    // SAFETY: OpenProcess for terminate; the handle is owned below.
                    if let Some(h) = Owned::new(unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) }) {
                        // SAFETY: an open process handle with terminate rights.
                        killed += u32::from(unsafe { TerminateProcess(h.raw(), 1) } != 0);
                    }
                }
            }
            json!({ "probe_processes_terminated": killed })
        }
        JobEnd::ParentExit => json!({ "opener_exit": opener.wait().ok().and_then(|s| s.code()) }),
    };
    let opener_exit = opener.wait().ok().and_then(|s| s.code());
    drop(job);
    std::thread::sleep(Duration::from_secs(3));
    let survived = new.iter().filter(|pid| alive(**pid)).count();
    Report::ok(
        "job-browser",
        logon_now(),
        json!({
            "end": format!("{end:?}"),
            "open": format!("{open:?}"),
            "association_program": http_open_command()
                .ok()
                .map(|c| crate::assoc::command_args(&c, "u"))
                .and_then(|a| a.first().cloned())
                .and_then(|p| Path::new(&p).file_name().map(|n| n.to_string_lossy().into_owned()))
                .map(|n| images::reportable(&n).to_string()),
            "opener_assigned": assigned.is_ok(),
            "opener_exit": opener_exit,
            "browsers_running_before": before.len(),
            "browser_processes_started": new.len(),
            "browser_processes_in_the_job": in_the_job.len(),
            "ended": ended,
            "browser_processes_alive_after": survived,
            "note": "close the browser by hand afterwards; the page is a local file",
        }),
    )
}
