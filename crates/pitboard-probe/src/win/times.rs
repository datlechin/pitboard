// T8: a process's creation time, read by pid as `pitboard_core::host::windows::proc::started`
// reads it, of children this probe starts, against the clock around each start, each child's
// own reading of itself, and once it has exited.

use super::ffi::{self, Owned};
use super::{io_code, logon_now};
use crate::cli::Outcome;
use crate::report::Report;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::process::{Command, Stdio};

use windows_sys::Win32::Foundation::{FILETIME, HANDLE};
use windows_sys::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE,
};

fn now() -> u64 {
    let mut at = FILETIME::default();
    // SAFETY: a FILETIME of this frame.
    unsafe { GetSystemTimePreciseAsFileTime(&raw mut at) };
    ffi::filetime_ticks(at)
}

// Created and exited, as ticks; exited is 0 while it runs.
fn times(process: HANDLE) -> Result<(u64, u64), u32> {
    let (mut created, mut exited) = (FILETIME::default(), FILETIME::default());
    let (mut kernel, mut user) = (FILETIME::default(), FILETIME::default());
    // SAFETY: a process handle, and four FILETIMEs of this frame.
    let read = unsafe {
        GetProcessTimes(
            process,
            &raw mut created,
            &raw mut exited,
            &raw mut kernel,
            &raw mut user,
        )
    };
    if read == 0 {
        return Err(ffi::last_error());
    }
    Ok((ffi::filetime_ticks(created), ffi::filetime_ticks(exited)))
}

fn opened(pid: u32, access: u32) -> Result<Owned, u32> {
    // SAFETY: plain values; the handle is owned by the result.
    Owned::new(unsafe { OpenProcess(access, 0, pid) }).ok_or_else(ffi::last_error)
}

fn by_pid(pid: u32) -> Result<(u64, u64), u32> {
    times(opened(pid, PROCESS_QUERY_LIMITED_INFORMATION)?.raw())
}

pub(super) fn child() -> Outcome {
    // SAFETY: the pseudo handle of this process.
    match times(unsafe { GetCurrentProcess() }) {
        Ok((created, _)) => Outcome::Line(created.to_string()),
        Err(code) => Outcome::Line(format!("error {code}")),
    }
}

pub(super) fn process_times(children: u32) -> Report {
    let me = match std::env::current_exe() {
        Ok(me) => me,
        Err(e) => {
            return Report::refused("process-times", logon_now(), format!("no current exe: {e}"));
        }
    };
    let mut started = 0u32;
    let mut failures: Vec<Value> = Vec::new();
    let mut in_window = 0u32;
    let mut early = Vec::new();
    let mut late = Vec::new();
    let mut same_as_the_child = 0u32;
    let mut same_after_exit = 0u32;
    let mut exit_after_creation = 0u32;
    let mut once_closed = BTreeSet::new();
    let mut created_all = BTreeSet::new();
    for _ in 0..children.min(200) {
        let before = now();
        let spawned = Command::new(&me)
            .arg("process-times-child")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let after = now();
        let child = match spawned {
            Ok(child) => child,
            Err(e) => {
                failures.push(json!({ "spawn": io_code(&e) }));
                continue;
            }
        };
        let pid = child.id();
        let running = by_pid(pid);
        let held = opened(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE);
        let said = child
            .wait_with_output()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string());
        let exited = by_pid(pid);
        drop(held);
        let closed = by_pid(pid);
        let (created, after_exit) = match (running, exited) {
            (Ok(r), Ok(e)) => (r.0, e),
            (r, e) => {
                failures.push(json!({ "running": r.err(), "after_exit": e.err() }));
                continue;
            }
        };
        started += 1;
        created_all.insert(created);
        if (before..=after).contains(&created) {
            in_window += 1;
        } else if created < before {
            early.push(before - created);
        } else {
            late.push(created - after);
        }
        if said
            .as_deref()
            .is_ok_and(|text| *text == created.to_string())
        {
            same_as_the_child += 1;
        }
        if after_exit.0 == created {
            same_after_exit += 1;
        }
        if after_exit.1 > created {
            exit_after_creation += 1;
        }
        once_closed.insert(match closed {
            Ok(_) => "opened".to_string(),
            Err(code) => code.to_string(),
        });
    }
    let micros = |ticks: &[u64]| ticks.iter().max().map(|t| t / 10);
    Report::ok(
        "process-times",
        logon_now(),
        json!({
            "children": children.min(200),
            "started": started,
            "failures": failures,
            "created_between_the_clock_readings": in_window,
            "created_before_the_first_reading": early.len(),
            "most_before_us": micros(&early),
            "created_after_the_second_reading": late.len(),
            "most_after_us": micros(&late),
            "distinct": created_all.len(),
            "child_reads_the_same_text": same_as_the_child,
            "same_once_exited_while_a_handle_is_held": same_after_exit,
            "exit_time_after_creation": exit_after_creation,
            "once_every_handle_is_closed": once_closed,
        }),
    )
}
