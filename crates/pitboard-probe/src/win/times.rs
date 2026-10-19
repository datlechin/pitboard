// T8: a process's creation time, read by pid as `pitboard_core::host::windows::proc::started`
// reads it, of children this probe starts, against the clock around each start, each child's
// own reading of itself, and once it has exited.

use super::ffi::{self, Owned};
use super::{io_code, logon_now};
use crate::cli::Outcome;
use crate::report::Report;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::process::{Command, Stdio};

use windows_sys::Win32::Foundation::{FILETIME, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, WaitForSingleObject,
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

// What opening the pid of an exited child gives once no handle of this probe's is open: as
// `pitboard_core::host::windows::proc::may_be_running` opens it.
fn once_closed(pid: u32, created: u64) -> String {
    match opened(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE) {
        Err(code) => format!("error {code}"),
        Ok(process) => {
            let same = times(process.raw()).is_ok_and(|(at, _)| at == created);
            // SAFETY: an open handle with SYNCHRONIZE, waited on for no time at all.
            let exited = unsafe { WaitForSingleObject(process.raw(), 0) } == WAIT_OBJECT_0;
            format!(
                "opened, {} process, {}",
                if same { "the same" } else { "another" },
                if exited { "exited" } else { "running" }
            )
        }
    }
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
    let mut at_once: BTreeMap<String, u32> = BTreeMap::new();
    let mut after_100_ms: BTreeMap<String, u32> = BTreeMap::new();
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
        let closed = running
            .as_ref()
            .ok()
            .map(|(created, _)| once_closed(pid, *created));
        std::thread::sleep(std::time::Duration::from_millis(100));
        let later = running
            .as_ref()
            .ok()
            .map(|(created, _)| once_closed(pid, *created));
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
        for (seen, counts) in [(closed, &mut at_once), (later, &mut after_100_ms)] {
            if let Some(seen) = seen {
                *counts.entry(seen).or_default() += 1;
            }
        }
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
            "once_every_handle_is_closed": at_once,
            "and_100_ms_later": after_100_ms,
        }),
    )
}
