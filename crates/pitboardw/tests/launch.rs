// pitboardw.exe as a task starts it, with a stand-in for the pitboard.exe beside it.
#![cfg(windows)]

use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{
    CREATE_NEW_CONSOLE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

// A folder of the test's own holding pitboardw.exe, and the stand-in as pitboard.exe where asked.
fn installed(with_program: bool) -> (tempfile::TempDir, PathBuf) {
    let built = PathBuf::from(env!("CARGO_BIN_EXE_pitboardw"));
    let folder = tempfile::tempdir().expect("a scratch folder");
    let launcher = folder.path().join("pitboardw.exe");
    std::fs::copy(&built, &launcher).expect("pitboardw.exe copied");
    if with_program {
        let child = built
            .parent()
            .expect("its folder")
            .join("examples")
            .join("pitboardw-child.exe");
        std::fs::copy(&child, folder.path().join("pitboard.exe"))
            .unwrap_or_else(|e| panic!("{}: {e}", child.display()));
    }
    (folder, launcher)
}

fn ran(program: &Path, args: &[&str]) -> Output {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("ran")
}

fn said(out: &Output, what: &str) -> Vec<String> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix(what).map(str::to_owned))
        .collect()
}

// Without it, a console program gets a console of its own with a window, as a task's does.
#[test]
fn the_program_runs_with_a_console_that_has_no_window() {
    let (folder, launcher) = installed(true);
    let launched = ran(&launcher, &[]);
    assert!(launched.status.success(), "{:?}", launched.status);
    assert_ne!(said(&launched, "console "), ["shown"]);

    let alone = Command::new(folder.path().join("pitboard.exe"))
        .creation_flags(CREATE_NEW_CONSOLE)
        .stdin(Stdio::null())
        .output()
        .expect("ran");
    assert_eq!(said(&alone, "console "), ["shown"]);
}

// Task Scheduler keeps what the launcher ends with as the task's last result.
#[test]
fn the_arguments_go_on_as_given_and_the_programs_code_comes_back() {
    let (_folder, launcher) = installed(true);
    let args = [
        "renew",
        "--scheduled",
        "two words",
        "a \"quote\"",
        "",
        "exit=7",
    ];
    let launched = ran(&launcher, &args);
    assert_eq!(launched.status.code(), Some(7));
    assert_eq!(said(&launched, "arg "), args);
}

// As Task Scheduler ends a task's program, at Stop or the time limit.
#[test]
fn ending_the_launcher_ends_the_program_it_started() {
    let (_folder, launcher) = installed(true);
    let mut launched = Command::new(&launcher)
        .arg("wait")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("started");
    let mut first = String::new();
    BufReader::new(launched.stdout.take().expect("its output"))
        .read_line(&mut first)
        .expect("read");
    let pid: u32 = first
        .trim()
        .strip_prefix("pid ")
        .and_then(|pid| pid.parse().ok())
        .unwrap_or_else(|| panic!("{first:?}"));
    // SAFETY: no pointer; a null handle is checked below.
    let program = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    assert!(!program.is_null(), "{}", std::io::Error::last_os_error());
    launched.kill().expect("ended");
    launched.wait().expect("waited on");
    // SAFETY: the handle just opened, closed once waited on.
    let ended = unsafe {
        let waited = WaitForSingleObject(program, 10_000);
        CloseHandle(program);
        waited
    };
    assert_eq!(ended, WAIT_OBJECT_0, "the program outlived its launcher");
}

#[test]
fn a_launcher_alone_ends_with_the_error_windows_gave() {
    let (_folder, launcher) = installed(false);
    let launched = ran(&launcher, &["renew", "--scheduled"]);
    // HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)
    assert_eq!(launched.status.code(), Some(0x8007_0002_u32 as i32));
}
