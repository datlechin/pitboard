//! Task Scheduler through `ITaskService`, on a thread of its own in COM's multithreaded
//! apartment, whatever apartment the caller's thread is in. A build for tests asks it nothing.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::host::task::{FOLDER, Registered, Service, from_ole_date};
use crate::service::Permit;
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::TaskScheduler::{
    IRegisteredTask, ITaskFolder, ITaskService, TASK_CREATE_OR_UPDATE,
    TASK_LOGON_INTERACTIVE_TOKEN, TaskScheduler,
};
use windows::Win32::System::Variant::VARIANT;
use windows::core::BSTR;

#[derive(Debug)]
pub(super) struct System;

// RF-E1 read ERROR_FILE_NOT_FOUND's for a missing folder and a missing task (run
// 38922866517); ERROR_PATH_NOT_FOUND's says the same.
const NOT_FOUND: [u32; 2] = [0x8007_0002, 0x8007_0003];
// ERROR_ALREADY_EXISTS: another process made the folder first (RF-E1, run 38922866517).
const ALREADY_THERE: u32 = 0x8007_00B7;

// SCHED_E_TASK_NOT_RUNNING: nothing to stop.
const NOT_RUNNING: u32 = 0x8004_130B;

const ASKED_IN_A_TEST: &str = "a build for tests asked Task Scheduler itself";

impl Service for System {
    fn read(&self, ctx: &Context, name: &str) -> Result<Option<Registered>> {
        if cfg!(any(test, feature = "test-support")) {
            return Ok(None);
        }
        let (name, zone) = (name.to_owned(), ctx.zone());
        in_the_mta(move |service| {
            let Some(folder) = folder(service)? else {
                return Ok(None);
            };
            // SAFETY: a folder the connected service gave, and the task it gave.
            unsafe {
                let task = match folder.GetTask(&BSTR::from(name.as_str())) {
                    Ok(task) => task,
                    Err(e) if not_found(&e) => return Ok(None),
                    Err(e) => return Err(e),
                };
                Ok(Some(Registered {
                    xml: task.Xml()?.to_string(),
                    last_run: task
                        .LastRunTime()
                        .ok()
                        .and_then(|date| from_ole_date(date, &zone)),
                    last_result: task.LastTaskResult().ok(),
                }))
            }
        })
    }

    // TASK_CREATE_OR_UPDATE with an interactive token and no user: the person registering it.
    fn register(&self, _: Permit, _ctx: &Context, name: &str, xml: &str) -> Result<()> {
        if cfg!(any(test, feature = "test-support")) {
            return Err(Error::ScheduleRefused {
                detail: ASKED_IN_A_TEST.into(),
            });
        }
        let (name, xml) = (name.to_owned(), xml.to_owned());
        in_the_mta(move |service| {
            let folder = made_folder(service)?;
            // SAFETY: a folder the connected service gave.
            unsafe {
                folder.RegisterTask(
                    &BSTR::from(name.as_str()),
                    &BSTR::from(xml.as_str()),
                    TASK_CREATE_OR_UPDATE.0,
                    &VARIANT::default(),
                    &VARIANT::default(),
                    TASK_LOGON_INTERACTIVE_TOKEN,
                    &VARIANT::default(),
                )
            }
            .map(drop)
        })
    }

    fn delete(&self, _: Permit, _ctx: &Context, name: &str) -> Result<()> {
        if cfg!(any(test, feature = "test-support")) {
            return Err(Error::ScheduleRefused {
                detail: ASKED_IN_A_TEST.into(),
            });
        }
        let name = name.to_owned();
        in_the_mta(move |service| {
            let Some(folder) = folder(service)? else {
                return Ok(());
            };
            // SAFETY: a folder the connected service gave, and its root.
            unsafe {
                match folder.DeleteTask(&BSTR::from(name.as_str()), 0) {
                    Err(e) if !not_found(&e) => return Err(e),
                    _ => {}
                }
                // Refused, ERROR_DIR_NOT_EMPTY, while another person's task is in it (RF-E1c).
                let _ = service
                    .GetFolder(&BSTR::from(r"\"))
                    .and_then(|root| root.DeleteFolder(&BSTR::from(&FOLDER[1..]), 0));
            }
            Ok(())
        })
    }

    fn run(&self, _: Permit, _ctx: &Context, name: &str) -> Result<()> {
        System::on_task(name, |task| {
            // SAFETY: a task the connected service gave; an empty VARIANT passes no parameters.
            unsafe { task.Run(&VARIANT::default()) }.map(drop)
        })
    }

    fn stop(&self, _: Permit, _ctx: &Context, name: &str) -> Result<()> {
        System::on_task(name, |task| {
            // SAFETY: a task the connected service gave; 0 is the only flag Stop takes.
            let stopped = unsafe { task.Stop(0) };
            match stopped {
                Err(e) if e.code().0 as u32 == NOT_RUNNING => Ok(()),
                stopped => stopped,
            }
        })
    }
}

impl System {
    fn on_task(
        name: &str,
        then: impl FnOnce(&IRegisteredTask) -> windows::core::Result<()> + Send,
    ) -> Result<()> {
        if cfg!(any(test, feature = "test-support")) {
            return Err(Error::ScheduleRefused {
                detail: ASKED_IN_A_TEST.into(),
            });
        }
        let name = name.to_owned();
        in_the_mta(move |service| {
            let Some(folder) = folder(service)? else {
                return Ok(());
            };
            // SAFETY: a folder the connected service gave, and the task it gave.
            match unsafe { folder.GetTask(&BSTR::from(name.as_str())) } {
                Ok(task) => then(&task),
                Err(e) if not_found(&e) => Ok(()),
                Err(e) => Err(e),
            }
        })
    }
}

fn refused(detail: String) -> Error {
    Error::ScheduleRefused { detail }
}

fn not_found(e: &windows::core::Error) -> bool {
    NOT_FOUND.contains(&(e.code().0 as u32))
}

// An error is made text here: it holds the IErrorInfo Task Scheduler set, a COM object of this
// thread's that must not outlive CoUninitialize.
fn in_the_mta<T: Send>(
    work: impl FnOnce(&ITaskService) -> windows::core::Result<T> + Send,
) -> Result<T> {
    let said = |e: windows::core::Error| refused(format!("Task Scheduler said {e}"));
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                // SAFETY: this thread's own apartment, left below where this call joined it.
                let joined = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
                if joined.is_err() && joined != RPC_E_CHANGED_MODE {
                    return Err(said(joined.into()));
                }
                let done = connected().and_then(|service| work(&service)).map_err(said);
                if joined.is_ok() {
                    // SAFETY: after every interface this thread made is released.
                    unsafe { CoUninitialize() };
                }
                done
            })
            .join()
            .unwrap_or_else(|_| Err(refused("Task Scheduler's thread panicked".into())))
    })
}

fn connected() -> windows::core::Result<ITaskService> {
    // SAFETY: COM is initialised on this thread, which alone uses the service.
    unsafe {
        let service: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)?;
        service.Connect(
            &VARIANT::default(),
            &VARIANT::default(),
            &VARIANT::default(),
            &VARIANT::default(),
        )?;
        Ok(service)
    }
}

fn folder(service: &ITaskService) -> windows::core::Result<Option<ITaskFolder>> {
    // SAFETY: a connected service.
    match unsafe { service.GetFolder(&BSTR::from(FOLDER)) } {
        Ok(folder) => Ok(Some(folder)),
        Err(e) if not_found(&e) => Ok(None),
        Err(e) => Err(e),
    }
}

// RF-E1: a standard user may make `\Pitboard\`, and its probe made it after any error of
// GetFolder's, as this does.
fn made_folder(service: &ITaskService) -> windows::core::Result<ITaskFolder> {
    // SAFETY: a connected service, and the root folder it gave.
    unsafe {
        if let Ok(folder) = service.GetFolder(&BSTR::from(FOLDER)) {
            return Ok(folder);
        }
        let made = service
            .GetFolder(&BSTR::from(r"\"))?
            .CreateFolder(&BSTR::from(&FOLDER[1..]), &VARIANT::default());
        match made {
            Err(e) if e.code().0 as u32 == ALREADY_THERE => service.GetFolder(&BSTR::from(FOLDER)),
            made => made,
        }
    }
}
