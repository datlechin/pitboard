//! E1-E4 and D2: register, run, read, list and delete probe tasks through `ITaskService`.
//! Every task is named `pitboard-probe-<label>` (optionally with the account's SID after it,
//! printed as `<sid>`, and another account's, which a list may see, as `<other_sid_N>`),
//! its action runs one of the probe's own programs, and it is
//! registered with an interactive token at the least privilege. The folder is the probe's
//! own `\pitboard-probe\` unless E1 asks for W25's candidate `\Pitboard\` or the root; a
//! folder the probe made is removed again when its last probe task goes, and a `\Pitboard\`
//! folder the probe did not make is never removed. Listing names only `pitboard-probe-*`
//! tasks. Everything but read and list needs the throwaway marker.

use super::ffi::{self, Token};
use super::{logon_now, sibling_exe};
use crate::PROBE_PREFIX;
use crate::cli::{TaskAction, TaskExec, TaskFolder};
use crate::report::Report;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{VARIANT_BOOL, VARIANT_FALSE, VARIANT_TRUE};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::System::TaskScheduler::{
    IDailyTrigger, IExecAction, ITaskFolder, ITaskService, TASK_ACTION_EXEC, TASK_CREATE_OR_UPDATE,
    TASK_ENUM_HIDDEN, TASK_LOGON_INTERACTIVE_TOKEN, TASK_RUNLEVEL_LUA, TASK_TRIGGER_DAILY,
    TASK_TRIGGER_TIME, TaskScheduler,
};
use windows::Win32::System::Variant::VARIANT;
use windows::core::{BSTR, Interface};

pub(super) struct Request<'a> {
    pub action: TaskAction,
    pub scratch: Option<&'a Path>,
    pub label: &'a str,
    pub folder: TaskFolder,
    pub sid_suffix: bool,
    pub exec: TaskExec,
    pub args: Option<&'a str>,
    pub at_minutes: Option<u32>,
    pub daily_since_minutes: Option<u32>,
    pub daily_in_minutes: Option<u32>,
    pub disabled: bool,
    pub start_when_available: bool,
    pub allow_battery: bool,
}

fn folder_path(f: TaskFolder) -> &'static str {
    match f {
        TaskFolder::Probe => r"\pitboard-probe",
        TaskFolder::Pitboard => r"\Pitboard",
        TaskFolder::Root => r"\",
    }
}

fn folder_leaf(f: TaskFolder) -> Option<&'static str> {
    match f {
        TaskFolder::Probe => Some("pitboard-probe"),
        TaskFolder::Pitboard => Some("Pitboard"),
        TaskFolder::Root => None,
    }
}

/// The record that the probe made a folder, so only such a folder is ever removed.
fn made_record(scratch: &Path, f: TaskFolder) -> PathBuf {
    scratch.join(format!(
        "{PROBE_PREFIX}task-made-folder-{}",
        folder_leaf(f).unwrap_or("root")
    ))
}

fn exec_path(e: TaskExec) -> PathBuf {
    sibling_exe(match e {
        TaskExec::Probe => "pitboard-probe",
        TaskExec::Detached => "pitboard-probe-detached",
        TaskExec::DetachedAsmv1 => "pitboard-probe-detached-asmv1",
    })
}

fn valid_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 32
        && label
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn err(e: windows::core::Error) -> String {
    format!("task scheduler error {}", code(&e))
}

fn code(e: &windows::core::Error) -> String {
    format!("0x{:08x}", e.code().0)
}

/// A connected Task Scheduler service.
fn connect() -> Result<ITaskService, String> {
    // SAFETY: COM is initialised on this thread before the first call; every call below is
    // on a live interface the service returned.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let service: ITaskService =
            CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER).map_err(err)?;
        service
            .Connect(
                &VARIANT::default(),
                &VARIANT::default(),
                &VARIANT::default(),
                &VARIANT::default(),
            )
            .map_err(err)?;
        Ok(service)
    }
}

/// The time `minutes` from now as the UTC ISO 8601 a trigger's start boundary takes.
fn utc_in(minutes: i64) -> String {
    let secs = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64
        + minutes * 60)
        .max(0) as u64;
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

pub(super) fn task(req: &Request) -> Report {
    let logon = logon_now();
    if !valid_label(req.label) {
        return Report::refused("task", logon, "--label takes up to 32 of a-z, 0-9 and -");
    }
    let sid = Token::current().ok().and_then(|t| t.user_sid());
    let name = match (req.sid_suffix, &sid) {
        (true, Some(s)) => format!("{PROBE_PREFIX}{}-{s}", req.label),
        (true, None) => return Report::refused("task", logon, "cannot read the account's SID"),
        (false, _) => format!("{PROBE_PREFIX}{}", req.label),
    };
    // Every SID a printed name holds: this account's as `<sid>`, another's (a task another
    // user registered with --sid-suffix, which list sees) as a numbered `<other_sid_N>`.
    let mut sids = crate::redact::SidRedactor::new(sid.as_deref());
    let mut shown = |n: &str| sids.redact(n);
    let writes = !matches!(req.action, TaskAction::Read | TaskAction::List);
    let scratch = match (writes, req.scratch) {
        (true, None) => return Report::refused("task", logon, "this action needs --scratch"),
        (true, Some(s)) => {
            if let Some(reason) = ffi::refuse_write(s) {
                return Report::refused("task", logon, reason);
            }
            let _ = std::fs::create_dir_all(s);
            Some(s)
        }
        (false, s) => s,
    };
    let service = match connect() {
        Ok(s) => s,
        Err(e) => return Report::refused("task", logon, e),
    };
    let result = match req.action {
        TaskAction::Register => register(&service, req, &name, scratch.expect("checked")),
        TaskAction::Run => run(&service, req.folder, &name),
        TaskAction::Read => read(&service, req.folder, &name, sid.as_deref()),
        TaskAction::List => list(&service),
        TaskAction::Delete => delete(&service, req.folder, &name, scratch.expect("checked")),
        TaskAction::Enable => enable(&service, req.folder, &name),
    };
    match result {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("name".into(), json!(shown(&name)));
                obj.insert("folder".into(), json!(folder_path(req.folder)));
            }
            if let Some(list) = v.get_mut("tasks").and_then(Value::as_array_mut) {
                for t in list {
                    if let Some(n) = t.get("name").and_then(Value::as_str).map(&mut shown) {
                        t["name"] = json!(n);
                    }
                }
            }
            Report::ok("task", logon, v)
        }
        Err(e) => Report::refused("task", logon, e),
    }
}

// Made when missing, the making recorded in `scratch`, with GetFolder's code where it was not
// there and CreateFolder's over it once it is, which another process's making meets.
fn folder(
    service: &ITaskService,
    f: TaskFolder,
    scratch: &Path,
) -> Result<(ITaskFolder, Value), String> {
    // SAFETY: `service` is connected; the calls are on live interfaces.
    unsafe {
        let found = service.GetFolder(&BSTR::from(folder_path(f)));
        let missing = found.as_ref().err().map(code);
        let target = match found {
            Ok(existing) => existing,
            Err(_) => {
                let root = service.GetFolder(&BSTR::from(r"\")).map_err(err)?;
                let leaf = folder_leaf(f).ok_or("the root folder is always there")?;
                let made = root
                    .CreateFolder(&BSTR::from(leaf), &VARIANT::default())
                    .map_err(|e| {
                        format!(
                            "{} after GetFolder's {}",
                            err(e),
                            missing.as_deref().unwrap_or("?")
                        )
                    })?;
                let _ = std::fs::write(made_record(scratch, f), b"made by pitboard-probe");
                made
            }
        };
        let again = folder_leaf(f).and_then(|leaf| {
            service
                .GetFolder(&BSTR::from(r"\"))
                .and_then(|root| root.CreateFolder(&BSTR::from(leaf), &VARIANT::default()))
                .err()
                .map(|e| code(&e))
        });
        Ok((
            target,
            json!({ "get_folder_error": missing, "create_again_error": again }),
        ))
    }
}

fn register(
    service: &ITaskService,
    req: &Request,
    name: &str,
    scratch: &Path,
) -> Result<Value, String> {
    let exe = exec_path(req.exec);
    if !exe.is_file() {
        return Err(format!(
            "{} is not built beside the probe",
            exe.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    let (target, codes) = folder(service, req.folder, scratch)?;
    // SAFETY: `service` is connected; every call is on a live interface it returned.
    unsafe {
        let def = service.NewTask(0).map_err(err)?;
        let principal = def.Principal().map_err(err)?;
        principal
            .SetLogonType(TASK_LOGON_INTERACTIVE_TOKEN)
            .map_err(err)?;
        principal.SetRunLevel(TASK_RUNLEVEL_LUA).map_err(err)?;

        let settings = def.Settings().map_err(err)?;
        let flag = |b: bool| if b { VARIANT_TRUE } else { VARIANT_FALSE };
        settings
            .SetStartWhenAvailable(flag(req.start_when_available))
            .map_err(err)?;
        settings
            .SetDisallowStartIfOnBatteries(flag(!req.allow_battery))
            .map_err(err)?;
        settings
            .SetStopIfGoingOnBatteries(flag(!req.allow_battery))
            .map_err(err)?;
        settings
            .SetExecutionTimeLimit(&BSTR::from("PT10M"))
            .map_err(err)?;
        settings.SetEnabled(flag(!req.disabled)).map_err(err)?;

        let mut boundary = None;
        if let Some(minutes) = req.at_minutes {
            let trigger = def
                .Triggers()
                .map_err(err)?
                .Create(TASK_TRIGGER_TIME)
                .map_err(err)?;
            let at = utc_in(i64::from(minutes));
            trigger
                .SetStartBoundary(&BSTR::from(at.as_str()))
                .map_err(err)?;
            boundary = Some(at);
        }
        // As the renewal's trigger, a day at a time, begun before now or after it.
        let daily = req
            .daily_since_minutes
            .map(|minutes| -i64::from(minutes))
            .or(req.daily_in_minutes.map(i64::from));
        if let Some(minutes) = daily {
            let trigger = def
                .Triggers()
                .map_err(err)?
                .Create(TASK_TRIGGER_DAILY)
                .map_err(err)?;
            let at = utc_in(minutes);
            trigger
                .SetStartBoundary(&BSTR::from(at.as_str()))
                .map_err(err)?;
            let daily: IDailyTrigger = trigger.cast().map_err(err)?;
            daily.SetDaysInterval(1).map_err(err)?;
            boundary = Some(at);
        }

        let action = def
            .Actions()
            .map_err(err)?
            .Create(TASK_ACTION_EXEC)
            .map_err(err)?;
        let exec: IExecAction = action.cast().map_err(err)?;
        exec.SetPath(&BSTR::from(exe.to_string_lossy().as_ref()))
            .map_err(err)?;
        if let Some(args) = req.args {
            exec.SetArguments(&BSTR::from(args)).map_err(err)?;
        }
        exec.SetWorkingDirectory(&BSTR::from(scratch.to_string_lossy().as_ref()))
            .map_err(err)?;

        let registered = target
            .RegisterTaskDefinition(
                &BSTR::from(name),
                &def,
                TASK_CREATE_OR_UPDATE.0,
                &VARIANT::default(),
                &VARIANT::default(),
                TASK_LOGON_INTERACTIVE_TOKEN,
                &VARIANT::default(),
            )
            .map_err(err)?;
        Ok(json!({
            "action": "register",
            "registered": true,
            "state": registered.State().map(|s| s.0).ok(),
            "start_boundary_utc": boundary,
            "daily": daily.is_some(),
            "disabled": req.disabled,
            "start_when_available": req.start_when_available,
            "allow_battery": req.allow_battery,
            "exec": exe.file_name().map(|n| n.to_string_lossy().into_owned()),
            "folder_codes": codes,
        }))
    }
}

fn run(service: &ITaskService, f: TaskFolder, name: &str) -> Result<Value, String> {
    // SAFETY: `service` is connected; the calls are on live interfaces.
    unsafe {
        let task = service
            .GetFolder(&BSTR::from(folder_path(f)))
            .and_then(|folder| folder.GetTask(&BSTR::from(name)))
            .map_err(err)?;
        let started = task.Run(&VARIANT::default());
        Ok(json!({
            "action": "run",
            "started": started.is_ok(),
            "error": started.err().map(err),
        }))
    }
}

fn enable(service: &ITaskService, f: TaskFolder, name: &str) -> Result<Value, String> {
    // SAFETY: `service` is connected; the calls are on live interfaces.
    unsafe {
        let task = service
            .GetFolder(&BSTR::from(folder_path(f)))
            .and_then(|folder| folder.GetTask(&BSTR::from(name)))
            .map_err(err)?;
        let enabled = task.SetEnabled(VARIANT_TRUE);
        Ok(json!({
            "action": "enable",
            "enabled": enabled.is_ok(),
            "error": enabled.err().map(err),
        }))
    }
}

fn read(
    service: &ITaskService,
    f: TaskFolder,
    name: &str,
    sid: Option<&str>,
) -> Result<Value, String> {
    // SAFETY: `service` is connected; the calls are on live interfaces.
    unsafe {
        let folder = match service.GetFolder(&BSTR::from(folder_path(f))) {
            Ok(folder) => folder,
            Err(e) => {
                return Ok(
                    json!({ "action": "read", "folder_present": false, "folder_error": code(&e) }),
                );
            }
        };
        let task = match folder.GetTask(&BSTR::from(name)) {
            Ok(task) => task,
            Err(e) => {
                return Ok(
                    json!({ "action": "read", "task_present": false, "task_error": code(&e) }),
                );
            }
        };
        let mut user = BSTR::new();
        let principal_is_self = task
            .Definition()
            .and_then(|d| d.Principal())
            .and_then(|p| p.UserId(&mut user))
            .ok()
            .map(|()| {
                let u = user.to_string();
                sid == Some(u.as_str())
                    || ffi::account_name().is_some_and(|n| {
                        u.eq_ignore_ascii_case(&n)
                            || u.to_lowercase()
                                .ends_with(&format!("\\{}", n.to_lowercase()))
                    })
            });
        Ok(json!({
            "action": "read",
            "task_present": true,
            "state": task.State().map(|s| s.0).ok(),
            "enabled": task.Enabled().map(VARIANT_BOOL::as_bool).ok(),
            "last_task_result": task.LastTaskResult().ok().map(|r| format!("0x{:08x}", r as u32)),
            "last_run_time_ole_date": task.LastRunTime().ok(),
            "next_run_time_ole_date": task.NextRunTime().ok(),
            "number_of_missed_runs": task.NumberOfMissedRuns().ok(),
            "principal_is_this_account": principal_is_self,
        }))
    }
}

fn list(service: &ITaskService) -> Result<Value, String> {
    let mut tasks = Vec::new();
    for f in [TaskFolder::Root, TaskFolder::Probe, TaskFolder::Pitboard] {
        // SAFETY: `service` is connected; the calls are on live interfaces; the collection
        // is indexed from 1 to its count.
        unsafe {
            let Ok(folder) = service.GetFolder(&BSTR::from(folder_path(f))) else {
                continue;
            };
            let Ok(all) = folder.GetTasks(TASK_ENUM_HIDDEN.0) else {
                continue;
            };
            let count = all.Count().unwrap_or(0);
            for i in 1..=count {
                let Ok(t) = all.get_Item(&VARIANT::from(i)) else {
                    continue;
                };
                let Ok(n) = t.Name() else { continue };
                let n = n.to_string();
                if n.to_lowercase().starts_with(PROBE_PREFIX) {
                    tasks.push(json!({
                        "folder": folder_path(f),
                        "name": n,
                        "state": t.State().map(|s| s.0).ok(),
                        "last_task_result": t.LastTaskResult().ok().map(|r| format!("0x{:08x}", r as u32)),
                    }));
                }
            }
        }
    }
    Ok(json!({ "action": "list", "tasks": tasks }))
}

fn delete(
    service: &ITaskService,
    f: TaskFolder,
    name: &str,
    scratch: &Path,
) -> Result<Value, String> {
    // SAFETY: `service` is connected; the calls are on live interfaces.
    unsafe {
        let Ok(folder) = service.GetFolder(&BSTR::from(folder_path(f))) else {
            return Ok(json!({ "action": "delete", "folder_present": false }));
        };
        let deleted = folder.DeleteTask(&BSTR::from(name), 0);
        // A folder goes only when the probe made it (or it is the probe's own) and it is
        // empty: DeleteFolder refuses a folder that still holds a task.
        let record = made_record(scratch, f);
        let (folder_removed, folder_error) = match folder_leaf(f) {
            Some(leaf) if f == TaskFolder::Probe || record.exists() => {
                let removed = service
                    .GetFolder(&BSTR::from(r"\"))
                    .and_then(|root| root.DeleteFolder(&BSTR::from(leaf), 0));
                if removed.is_ok() {
                    let _ = std::fs::remove_file(&record);
                }
                (Some(removed.is_ok()), removed.err().map(|e| code(&e)))
            }
            _ => (None, None),
        };
        Ok(json!({
            "action": "delete",
            "deleted": deleted.is_ok(),
            "error": deleted.err().map(err),
            "folder_removed": folder_removed,
            "folder_error": folder_error,
        }))
    }
}

/// Register a probe task that runs `exec` with `args`, run it, wait for `out` to appear, and
/// delete it: E4's from-task scenario and the runbook's D2 and E3 in one call.
pub(super) fn run_once(
    label: &str,
    exec: TaskExec,
    args: &str,
    scratch: &Path,
    out: &Path,
) -> Value {
    let req = Request {
        action: TaskAction::Register,
        scratch: Some(scratch),
        label,
        folder: TaskFolder::Probe,
        sid_suffix: false,
        exec,
        args: Some(args),
        at_minutes: None,
        daily_since_minutes: None,
        daily_in_minutes: None,
        disabled: false,
        start_when_available: false,
        allow_battery: true,
    };
    let registered = task(&req).to_json();
    if registered["ok"] != json!(true) {
        return json!({ "registered": registered });
    }
    let ran = task(&Request {
        action: TaskAction::Run,
        ..req
    })
    .to_json();
    let waited = std::time::Instant::now();
    while !out.exists() && waited.elapsed() < std::time::Duration::from_secs(30) {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    let read = task(&Request {
        action: TaskAction::Read,
        ..req
    })
    .to_json();
    let deleted = task(&Request {
        action: TaskAction::Delete,
        ..req
    })
    .to_json();
    json!({
        "ran": ran["data"],
        "read": read["data"],
        "deleted": deleted["data"],
        "report_written": out.exists(),
    })
}
