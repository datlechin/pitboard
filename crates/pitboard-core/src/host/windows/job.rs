//! A tool Pitboard starts on Windows, in a job of its own from its first instruction on.

use super::proc::image;
use crate::host::Ran;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Console::GetConsoleWindow;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT,
    JOB_OBJECT_LIMIT_BREAKAWAY_OK, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_PROCESS_ID_LIST, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation, QueryInformationJobObject,
    SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenProcess, OpenThread, PROCESS_CREATION_FLAGS,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, ResumeThread,
    THREAD_SUSPEND_RESUME, TerminateProcess, WaitForSingleObject,
};

// npm's shim, the `node` it starts, the tools, and the console they share. Not a browser.
const CHAIN: [&str; 5] = [
    "cmd.exe",
    "conhost.exe",
    "node.exe",
    "claude.exe",
    "codex.exe",
];

// How long one of them is given to end once it was told to.
const ENDING_MS: u32 = 5_000;

pub(crate) struct Tool {
    child: Child,
    job: Job,
    ended: bool,
}

struct Job(OwnedHandle);

pub(crate) fn start(command: Command) -> io::Result<Tool> {
    // SAFETY: no argument; the window is only compared with null.
    let console = !unsafe { GetConsoleWindow() }.is_null();
    start_with(command, console)
}

fn start_with(mut command: Command, console: bool) -> io::Result<Tool> {
    command.creation_flags(flags(console));
    // No kill on close, and Codex starts its daemons breaking away (`codex_windows_app_server`).
    let job = job(JOB_OBJECT_LIMIT_BREAKAWAY_OK)?;
    let child = spawn_into(command, &job)?;
    Ok(Tool {
        child,
        job,
        ended: false,
    })
}

fn spawn_into(mut command: Command, job: &Job) -> io::Result<Child> {
    let mut child = command.spawn()?;
    if let Err(error) = assign(job, &child).and_then(|()| resume(child.id())) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok(child)
}

// All of it ends past the ceiling or with Pitboard.
pub(crate) fn run_within(
    _: crate::permit::Permit,
    mut command: Command,
    handed: Option<&[u8]>,
    ceiling: Duration,
) -> io::Result<Ran> {
    if handed.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "a run on Windows is handed nothing on a descriptor",
        ));
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(flags(false));
    let job = job(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)?;
    let mut child = spawn_into(command, &job)?;
    let printed = crate::host::drained(child.stdout.take());
    let status = crate::host::ended_within(&mut child, ceiling)?;
    end_all(&job);
    let timed_out = status.is_none();
    let status = status.or_else(|| child.wait().ok());
    Ok(Ran::of(status, timed_out, &printed))
}

// A process leaves the job's list before it ends, so each is waited on by a handle taken then.
fn end_all(job: &Job) {
    let asked = Instant::now();
    loop {
        let listed: Vec<OwnedHandle> = members(job)
            .into_iter()
            .filter_map(|pid| opened(pid, PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION))
            .filter(|process| in_job(process, job))
            .collect();
        // SAFETY: the job this process made, with every right on it.
        unsafe { TerminateJobObject(job.0.as_raw_handle(), 1) };
        if listed.is_empty() {
            return;
        }
        for process in listed {
            let spent = u32::try_from(asked.elapsed().as_millis()).unwrap_or(u32::MAX);
            // SAFETY: an open handle with SYNCHRONIZE.
            unsafe {
                WaitForSingleObject(process.as_raw_handle(), ENDING_MS.saturating_sub(spent))
            };
        }
        if asked.elapsed() >= Duration::from_millis(ENDING_MS.into()) {
            return;
        }
    }
}

// In a terminal the tool shares its console; with none, it would get a window of its own.
fn flags(console: bool) -> PROCESS_CREATION_FLAGS {
    if console {
        CREATE_SUSPENDED
    } else {
        CREATE_SUSPENDED | CREATE_NO_WINDOW
    }
}

fn job(flags: JOB_OBJECT_LIMIT) -> io::Result<Job> {
    // SAFETY: no attributes and no name.
    let made = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if made.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a valid job handle this process now owns, closed by `OwnedHandle`.
    let job = unsafe { OwnedHandle::from_raw_handle(made) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = flags;
    // SAFETY: the job just made, and limits of the size given.
    let set = unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if set == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Job(job))
}

fn assign(job: &Job, child: &Child) -> io::Result<()> {
    // SAFETY: the job and the child's own process handle, both open.
    if unsafe { AssignProcessToJobObject(job.0.as_raw_handle(), child.as_raw_handle()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

// std's `Child` keeps no handle on its thread; a suspended program has one, a snapshot names it.
fn resume(pid: u32) -> io::Result<()> {
    // SAFETY: a snapshot of every thread.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a valid snapshot handle this process now owns, closed by `OwnedHandle`.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot) };
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        ..THREADENTRY32::default()
    };
    let mut threads = Vec::new();
    // SAFETY: an open snapshot, and an entry whose size it carries.
    let mut more = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) } != 0;
    while more {
        if entry.th32OwnerProcessID == pid {
            threads.push(entry.th32ThreadID);
        }
        // SAFETY: as above.
        more = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) } != 0;
    }
    let [thread] = threads[..] else {
        return Err(io::Error::other(format!(
            "the program started suspended has {} threads",
            threads.len()
        )));
    };
    // SAFETY: plain values.
    let handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, thread) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a valid thread handle this process now owns, closed by `OwnedHandle`.
    let opened = unsafe { OwnedHandle::from_raw_handle(handle) };
    // SAFETY: an open thread, with the right to resume it.
    if unsafe { ResumeThread(opened.as_raw_handle()) } == u32::MAX {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

impl Tool {
    pub(crate) fn child(&mut self) -> &mut Child {
        &mut self.child
    }

    pub(crate) fn wait(&mut self) -> io::Result<ExitStatus> {
        let status = self.child.wait();
        self.ended = status.is_ok();
        status
    }

    // A browser the sign-in opened runs in the job too, and is the person's (RF-F2).
    pub(crate) fn stop(&mut self) {
        end_the_chain(&self.job);
        let _ = self.child.kill();
        let _ = self.child.wait();
        end_the_chain(&self.job);
        self.ended = true;
    }

    #[cfg(test)]
    fn holds(&self, pid: u32) -> bool {
        let Some(process) = opened(pid, PROCESS_QUERY_LIMITED_INFORMATION) else {
            return false;
        };
        in_job(&process, &self.job)
    }
}

// Pitboard letting go of a sign-in unfinished; a Ctrl+C reaches the tool itself, and no drop runs.
impl Drop for Tool {
    fn drop(&mut self) {
        if !self.ended {
            self.stop();
        }
    }
}

fn opened(pid: u32, access: u32) -> Option<OwnedHandle> {
    // SAFETY: plain values.
    let handle = unsafe { OpenProcess(access, 0, pid) };
    // SAFETY: a non-null process handle this process now owns, closed by `OwnedHandle`.
    (!handle.is_null()).then(|| unsafe { OwnedHandle::from_raw_handle(handle) })
}

fn in_job(process: &OwnedHandle, job: &Job) -> bool {
    let mut inside = 0;
    // SAFETY: two open handles, and a BOOL of this frame.
    unsafe {
        IsProcessInJob(process.as_raw_handle(), job.0.as_raw_handle(), &mut inside) != 0
            && inside != 0
    }
}

fn in_the_chain(process: &OwnedHandle) -> bool {
    image(process)
        .as_deref()
        .and_then(std::path::Path::file_name)
        .is_some_and(|name| {
            CHAIN
                .iter()
                .any(|chained| name.eq_ignore_ascii_case(chained))
        })
}

// Ends each until none is left, so one started meanwhile by another is ended too.
fn end_the_chain(job: &Job) {
    for _ in 0..5 {
        let mut ending = Vec::new();
        for pid in members(job) {
            let access =
                PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE;
            let Some(process) = opened(pid, access) else {
                continue;
            };
            // SAFETY: an open handle with SYNCHRONIZE, waited on for no time at all.
            let running =
                unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } != WAIT_OBJECT_0;
            // A pid listed and since ended may be another program's by now.
            if running && in_job(&process, job) && in_the_chain(&process) {
                // SAFETY: an open handle with the right to end it.
                unsafe { TerminateProcess(process.as_raw_handle(), 1) };
                ending.push(process);
            }
        }
        if ending.is_empty() {
            return;
        }
        for process in ending {
            // SAFETY: an open handle with SYNCHRONIZE.
            unsafe { WaitForSingleObject(process.as_raw_handle(), ENDING_MS) };
        }
    }
}

fn members(job: &Job) -> Vec<u32> {
    let header =
        std::mem::offset_of!(JOBOBJECT_BASIC_PROCESS_ID_LIST, ProcessIdList) / size_of::<usize>();
    let mut room = 32;
    loop {
        let mut list = vec![0usize; header + room];
        // SAFETY: an open job, and a buffer of the length given, aligned for the list.
        let read = unsafe {
            QueryInformationJobObject(
                job.0.as_raw_handle(),
                JobObjectBasicProcessIdList,
                list.as_mut_ptr().cast(),
                (list.len() * size_of::<usize>()) as u32,
                std::ptr::null_mut(),
            )
        };
        // SAFETY: the buffer begins with the list's two counts, filled or zero.
        let (assigned, listed) = unsafe {
            let counts = list.as_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>();
            (
                (*counts).NumberOfAssignedProcesses as usize,
                (*counts).NumberOfProcessIdsInList as usize,
            )
        };
        if read != 0 {
            return list[header..header + listed.min(room)]
                .iter()
                .map(|&pid| pid as u32)
                .collect();
        }
        let short = io::Error::last_os_error().raw_os_error() == Some(ERROR_MORE_DATA as i32);
        if !short || assigned < room {
            return Vec::new();
        }
        room = assigned + 32;
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test copies its own program where a tool would be"
)]
mod tests {
    use super::*;
    use crate::helper::{self, Job};
    use std::path::{Path, PathBuf};

    // This test's program under a tool's name, which the helper runs as.
    fn copy_as(dir: &Path, name: &str) -> PathBuf {
        let program = dir.join(name);
        std::fs::create_dir_all(dir).expect("a folder");
        std::fs::copy(std::env::current_exe().expect("this test"), &program).expect("a copy");
        program
    }

    fn quietly(mut command: Command) -> Command {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }

    fn pid_in(file: &Path) -> u32 {
        let said = helper::written(file);
        said.trim()
            .parse()
            .unwrap_or_else(|_| panic!("a process id: {said:?}"))
    }

    fn runs(pid: u32) -> bool {
        super::super::proc::may_be_running(pid)
    }

    fn port_is_free() -> bool {
        std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 1455)).is_ok()
    }

    #[test]
    fn a_cancel_ends_every_program_of_the_tool_and_frees_its_port() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let (tool, server) = (
            copy_as(&dir.join("a"), "codex.exe"),
            copy_as(&dir.join("b"), "codex.exe"),
        );
        let (started, listening) = (dir.join("tool.pid"), dir.join("server.pid"));
        let _held = (helper::hold(&started), helper::hold(&listening));
        assert!(port_is_free(), "nothing else listens on Codex's port");
        let mut running = start(quietly(helper::starting(
            &tool,
            false,
            &started,
            (&server, Job::HoldsPort, &listening),
        )))
        .expect("started");
        let (tool, server) = (pid_in(&started), pid_in(&listening));
        assert!(running.holds(tool) && running.holds(server));
        assert!(!port_is_free());
        running.stop();
        assert!(!runs(tool) && !runs(server));
        assert!(port_is_free(), "let go of as it ended");
    }

    #[test]
    fn a_browser_the_sign_in_opened_outlives_a_cancel_and_pitboard() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let (tool, browser) = (
            copy_as(&dir.join("tool"), "claude.exe"),
            copy_as(&dir.join("browser"), "browser.exe"),
        );
        let mut kept = Vec::new();
        for (round, cancelled) in [(0, true), (1, false)] {
            let (started, opened) = (
                dir.join(format!("tool-{round}.pid")),
                dir.join(format!("browser-{round}.pid")),
            );
            let held = (helper::hold(&started), helper::hold(&opened));
            let mut running = start(quietly(helper::starting(
                &tool,
                false,
                &started,
                (&browser, Job::Waits, &opened),
            )))
            .expect("started");
            let (tool, browser) = (pid_in(&started), pid_in(&opened));
            assert!(running.holds(browser), "the browser runs in the tool's job");
            if cancelled {
                running.stop();
            } else {
                drop(running);
            }
            assert!(!runs(tool), "round {round}");
            assert!(runs(browser), "round {round}: the browser is the person's");
            kept.push((browser, held));
        }
        for (browser, held) in kept {
            drop(held);
            let gone = Instant::now();
            while runs(browser) {
                assert!(gone.elapsed().as_secs() < 20, "the browser stand-in ends");
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    #[test]
    fn a_program_started_breaking_away_starts_and_is_left_alone() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let (tool, daemon) = (
            copy_as(&dir.join("a"), "codex.exe"),
            copy_as(&dir.join("b"), "codex.exe"),
        );
        let (started, apart) = (dir.join("tool.pid"), dir.join("daemon.pid"));
        let _tool_held = helper::hold(&started);
        let daemon_held = helper::hold(&apart);
        let mut running = start(quietly(helper::starting(
            &tool,
            true,
            &started,
            (&daemon, Job::Waits, &apart),
        )))
        .expect("started");
        let said = helper::written(&started);
        assert!(!said.starts_with("refused"), "{said}");
        let daemon = pid_in(&apart);
        assert!(!running.holds(daemon), "out of the job");
        running.stop();
        assert!(runs(daemon), "left alone");
        drop(daemon_held);
        let gone = Instant::now();
        while runs(daemon) {
            assert!(gone.elapsed().as_secs() < 20, "the daemon stand-in ends");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    // As npm's shim for a program starts it.
    #[test]
    fn a_cmd_hands_its_program_each_argument_whole_and_a_line_break_starts_nothing() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        copy_as(dir, "records.exe");
        let front = dir.join("front.cmd");
        std::fs::write(&front, "@ECHO off\r\n\"%~dp0records.exe\"   %*\r\n").expect("a shim");
        let handed = [
            "two words",
            "a \"quoted\" one",
            "50%",
            "%PATH%",
            "a^b",
            "x&y",
        ];
        let recorded = dir.join("recorded.json");
        let mut command = quietly(helper::command(&front, Job::Records, &recorded));
        command.args(handed);
        let mut running = start(command).expect("started");
        assert!(running.wait().expect("ended").success());
        let read: Vec<String> =
            serde_json::from_str(&helper::written(&recorded)).expect("what it was handed");
        assert_eq!(read, handed);

        // std's make_bat_command_line refuses a batch argument with a line break before
        // CreateProcessW, so Pitboard needs no guard of its own.
        let refused = dir.join("refused.json");
        for broken in ["a\nb", "a\rb"] {
            let mut command = quietly(helper::command(&front, Job::Records, &refused));
            command.arg(broken);
            let error = start(command).map(drop).expect_err("refused");
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{broken:?}");
        }
        std::thread::sleep(Duration::from_millis(500));
        assert!(!refused.exists(), "nothing started");
    }

    // What `common::os::found_no_interpreter` takes for npm's shim with no `node` to find.
    #[test]
    fn npms_shim_with_no_interpreter_to_find_exits_1() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let shim = dir.join("codex.cmd");
        std::fs::write(
            &shim,
            "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\n\
             SETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\pitboard-absent.exe\" (\r\n  \
             SET \"_prog=%dp0%\\pitboard-absent.exe\"\r\n) ELSE (\r\n  \
             SET \"_prog=pitboard-absent\"\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || \
             title %COMSPEC% & set PATHEXT=%PATHEXT:;.JS;=;% & \"%_prog%\"  \
             \"%dp0%\\codex.js\" %*\r\n",
        )
        .expect("a shim");
        let mut command = quietly(Command::new(&shim));
        command.arg("login").env("PATH", system32());
        let ended = start(command).expect("started").wait().expect("ended");
        assert_eq!(ended.code(), Some(1), "{ended:?}");
    }

    fn system32() -> PathBuf {
        std::env::var_os("SystemRoot")
            .map(|windows| PathBuf::from(windows).join("System32"))
            .expect("SystemRoot")
    }

    // `cmd.exe` starting another at once, suspended until it is in the job (RF-F1).
    #[test]
    fn a_program_started_at_once_is_in_the_job_every_time() {
        let cmd = system32().join("cmd.exe");
        for round in 0..1000 {
            let mut command = Command::new(&cmd);
            command
                .args(["/d", "/c"])
                .arg(&cmd)
                .args(["/d", "/k"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let mut running = start(command).expect("started");
            let parent = running.child().id();
            let asked = Instant::now();
            let started = loop {
                let found = super::super::proc::snapshot()
                    .expect("the process list")
                    .into_iter()
                    .find(|&(_, its, ref name)| {
                        its == parent && name.eq_ignore_ascii_case("cmd.exe")
                    });
                if let Some((pid, _, _)) = found {
                    break pid;
                }
                assert!(
                    asked.elapsed().as_secs() < 20,
                    "round {round}: nothing started"
                );
                std::thread::yield_now();
            };
            assert!(running.holds(started), "round {round}");
            running.stop();
            assert!(!runs(started), "round {round}");
        }
    }

    fn tool_and_child(dir: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
        (
            copy_as(&dir.join("tool"), "claude.exe"),
            copy_as(&dir.join("child"), "claude.exe"),
            dir.join("tool.pid"),
            dir.join("child.pid"),
        )
    }

    #[test]
    fn a_run_that_ended_says_what_it_printed_and_leaves_nothing_it_started() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let (tool, child, started, opened) = tool_and_child(dir);
        let tool_held = helper::hold(&started);
        let _child_held = helper::hold(&opened);
        let command = helper::starting(&tool, false, &started, (&child, Job::Waits, &opened));
        let watching = {
            let (started, opened) = (started.clone(), opened.clone());
            std::thread::spawn(move || {
                let child = pid_in(&opened);
                pid_in(&started);
                drop(tool_held);
                child
            })
        };
        let ran = run_within(
            crate::permit::Permit::for_a_test(),
            command,
            None,
            Duration::from_secs(60),
        )
        .expect("ran");
        let child = watching.join().expect("both started");
        assert_eq!((ran.exit, ran.timed_out), (Some(0), false));
        assert!(ran.stdout.contains("pitboard-helper: "), "{}", ran.stdout);
        assert!(!runs(child), "what the run started ended with it");
    }

    #[test]
    fn a_run_past_its_ceiling_is_ended_whole() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let (tool, child, started, opened) = tool_and_child(dir);
        let _held = (helper::hold(&started), helper::hold(&opened));
        let command = helper::starting(&tool, false, &started, (&child, Job::Waits, &opened));
        let watching = {
            let (started, opened) = (started.clone(), opened.clone());
            std::thread::spawn(move || (pid_in(&started), pid_in(&opened)))
        };
        let ran = run_within(
            crate::permit::Permit::for_a_test(),
            command,
            None,
            Duration::from_secs(10),
        )
        .expect("ran");
        let (tool, child) = watching.join().expect("both started");
        assert_eq!((ran.exit, ran.timed_out), (None, true));
        assert!(!runs(tool) && !runs(child));
    }

    #[test]
    fn nothing_a_run_starts_breaks_away() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let (tool, child, started, opened) = tool_and_child(dir);
        let _held = (helper::hold(&started), helper::hold(&opened));
        let command = helper::starting(&tool, true, &started, (&child, Job::Waits, &opened));
        let ran = run_within(
            crate::permit::Permit::for_a_test(),
            command,
            None,
            Duration::from_secs(60),
        )
        .expect("ran");
        assert_eq!(ran.exit, Some(0));
        let said = helper::written(&started);
        assert!(said.starts_with("refused"), "{said}");
        assert!(!opened.exists(), "nothing started");
    }

    #[test]
    fn a_run_is_handed_nothing_on_a_descriptor() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let said = dir.join("console");
        let command = quietly(helper::command(
            &std::env::current_exe().expect("this test"),
            Job::SaysItsConsole,
            &said,
        ));
        let refused = run_within(
            crate::permit::Permit::for_a_test(),
            command,
            Some(b"access".as_slice()),
            Duration::from_secs(60),
        )
        .map(drop)
        .expect_err("refused");
        assert_eq!(refused.kind(), io::ErrorKind::Unsupported);
        std::thread::sleep(Duration::from_millis(500));
        assert!(!said.exists(), "nothing started");
    }

    #[test]
    fn with_no_console_to_share_the_tool_gets_no_window() {
        assert_eq!(flags(true), CREATE_SUSPENDED);
        assert_eq!(flags(false), CREATE_SUSPENDED | CREATE_NO_WINDOW);
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path();
        let said = dir.join("console");
        let command = quietly(helper::command(
            &std::env::current_exe().expect("this test"),
            Job::SaysItsConsole,
            &said,
        ));
        let mut running = start_with(command, false).expect("started");
        assert!(running.wait().expect("ended").success());
        assert_eq!(helper::written(&said), "none");
    }
}
