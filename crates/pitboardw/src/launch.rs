use crate::line;
use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use std::process::Command;
use windows_sys::Win32::Foundation::{E_FAIL, ERROR_FILE_NOT_FOUND};
use windows_sys::Win32::System::Environment::GetCommandLineW;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, GetCurrentProcess};

// Ends with `pitboard.exe`'s exit code, which Task Scheduler keeps as the task's last result,
// or with this program's own failure as an HRESULT, as Task Scheduler gives its own.
pub(crate) fn run() -> ! {
    // Never closed before this process ends: closing it would end this process, which is in it.
    let job = hold_what_it_starts();
    let code = match job.as_ref() {
        Ok(_) => launch().unwrap_or_else(|error| failure(&error)),
        Err(error) => failure(error),
    };
    std::process::exit(code)
}

fn launch() -> io::Result<i32> {
    let program = line::launched(&std::env::current_exe()?)
        .ok_or_else(|| io::Error::from_raw_os_error(ERROR_FILE_NOT_FOUND as i32))?;
    let mut command = Command::new(program);
    let arguments = arguments();
    if !arguments.is_empty() {
        command.raw_arg(arguments);
    }
    let status = command.creation_flags(CREATE_NO_WINDOW).status()?;
    Ok(status.code().unwrap_or(E_FAIL))
}

fn arguments() -> OsString {
    // SAFETY: Windows keeps this process's command line, ending in a NUL, for the process's life.
    let line = unsafe {
        let start = GetCommandLineW();
        let mut length = 0;
        while *start.add(length) != 0 {
            length += 1;
        }
        std::slice::from_raw_parts(start, length)
    };
    OsString::from_wide(line::after_program(line))
}

// This process and all it starts, in a job that ends them once nobody holds it, so ending this
// process at Task Scheduler's `Stop` or the task's time limit ends `pitboard.exe` too. A
// program started to break away from it still does.
fn hold_what_it_starts() -> io::Result<OwnedHandle> {
    // SAFETY: no attributes and no name.
    let made = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if made.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a job handle this process now owns.
    let job = unsafe { OwnedHandle::from_raw_handle(made) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags =
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;
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
    // SAFETY: the job just made, and this process's own pseudo handle.
    if unsafe { AssignProcessToJobObject(job.as_raw_handle(), GetCurrentProcess()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(job)
}

// HRESULT_FROM_WIN32 where Windows gave an error code.
fn failure(error: &io::Error) -> i32 {
    match error.raw_os_error() {
        Some(code) if code > 0 => (0x8007_0000 | (code as u32 & 0xFFFF)) as i32,
        _ => E_FAIL,
    }
}
