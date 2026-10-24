// Pitboard's jobs on Windows: tasks in Task Scheduler's `\Pitboard\` folder, written and read
// apart from the system so every system's tests read the tasks Pitboard registers.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::host::Scheduler;
use crate::permit::Permit;
use crate::schedule::{EVERY_SECONDS, Job, Location};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) const FOLDER: &str = r"\Pitboard";
const ENVIRONMENT: &str = "schedule.env";
const WATCH_ENVIRONMENT: &str = "watch.env";
// Task Scheduler's own results that are no failure: RF-E1 read `NOT_RUN_YET` off a task whose
// person was not signed in.
const RUNNING: i32 = 0x0004_1301;
const NOT_RUN_YET: i32 = 0x0004_1303;
const QUEUED: i32 = 0x0004_1325;

// Task names are global within a folder, so each person's carries their SID. A second person
// registers in the folder the first made (RF-E1c, run 38922866517).
pub(crate) fn name(job: Job, sid: &str) -> String {
    match job {
        Job::Renew => format!("Renew {sid}"),
        Job::Watch => format!("Watch {sid}"),
    }
}

pub(crate) fn path(job: Job, sid: &str) -> String {
    format!(r"{FOLDER}\{}", name(job, sid))
}

// A task as Task Scheduler keeps it: its definition, and how its last run went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Registered {
    pub(crate) xml: String,
    pub(crate) last_run: Option<i64>,
    pub(crate) last_result: Option<i32>,
}

// Task Scheduler, or a test's machine's stand-in for it, asked for one task in `FOLDER`.
pub(crate) trait Service: Send + Sync + std::fmt::Debug {
    // `None` where there is no such task; an error where Task Scheduler could not say.
    fn read(&self, ctx: &Context, name: &str) -> Result<Option<Registered>>;
    fn register(&self, permit: Permit, ctx: &Context, name: &str, xml: &str) -> Result<()>;
    fn delete(&self, permit: Permit, ctx: &Context, name: &str) -> Result<()>;
    fn run(&self, permit: Permit, ctx: &Context, name: &str) -> Result<()>;
    // End each of its runs under way; a task with none, or none at all, is already stopped.
    fn stop(&self, permit: Permit, ctx: &Context, name: &str) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Drift {
    OnAcPowerOnly,
    StopsOnBattery,
    SkipsMissedStarts,
    Disabled,
}

// What Task Scheduler says of the task beyond what Pitboard wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Task {
    pub(crate) last_run: Option<i64>,
    pub(crate) last_result: Option<i32>,
    pub(crate) drifted: Vec<Drift>,
}

impl Task {
    pub(crate) fn failed(&self) -> Option<i32> {
        self.last_result
            .filter(|result| ![0, RUNNING, NOT_RUN_YET, QUEUED].contains(result))
    }
}

// InteractiveToken runs only while the person is signed in, when DPAPI and Credential Manager
// open (RF-D2), and keeps no password. The proxy is in `schedule.env` or `watch.env`: a
// definition has no environment, and an argument would show a proxy's password.
//
// The watch service starts at the person's own sign-in (`UserId`, documented to take a SID) and
// runs with no time limit (`PT0S`); renewal runs daily at `start`, for 10 minutes at most.
pub(crate) fn definition(
    job: Job,
    program: &Path,
    options: &[String],
    start: &str,
    sid: &str,
) -> String {
    let (description, trigger, limit) = match job {
        Job::Renew => (
            "Renew Pitboard's parked logins",
            format!(
                "<CalendarTrigger>
      <StartBoundary>{}</StartBoundary>
      <ScheduleByDay>
        <DaysInterval>{}</DaysInterval>
      </ScheduleByDay>
    </CalendarTrigger>",
                escaped(start),
                EVERY_SECONDS / 86_400
            ),
            "PT10M",
        ),
        Job::Watch => (
            "Switch Claude Code before an account runs out, and warm up accounts",
            format!(
                "<LogonTrigger>
      <UserId>{}</UserId>
    </LogonTrigger>",
                escaped(sid)
            ),
            "PT0S",
        ),
    };
    let arguments = [job.marked().join(" "), options.join(" ")].join(" ");
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>{description}</Description>
  </RegistrationInfo>
  <Triggers>
    {trigger}
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>true</StartWhenAvailable>
    <Hidden>true</Hidden>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>{limit}</ExecutionTimeLimit>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{command}</Command>
      <Arguments>{arguments}</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        command = escaped(&program.to_string_lossy()),
        arguments = escaped(arguments.trim_end()),
    )
}

// A day after it is put, at that time of day, on the machine's clock.
fn first_start(ctx: &Context) -> String {
    jiff::Timestamp::from_second(ctx.now() + i64::from(EVERY_SECONDS))
        .unwrap_or_default()
        .to_zoned(ctx.zone())
        .strftime("%Y-%m-%dT%H:%M:%S")
        .to_string()
}

// Only what `definition` writes, and only as it writes it: the job's marker, then its options.
pub(crate) fn options(xml: &str, job: Job) -> Option<Vec<String>> {
    let arguments = unescaped(element(element(xml, "Exec")?, "Arguments")?.trim());
    let mut words = arguments.split_whitespace();
    let marked = job.marked();
    if words.by_ref().take(marked.len()).ne(marked) {
        return None;
    }
    Some(words.map(str::to_owned).collect())
}

pub(crate) fn program(xml: &str, job: Job) -> Option<PathBuf> {
    let exec = element(xml, "Exec")?;
    if job == Job::Renew && !options(xml, job)?.is_empty() {
        return None;
    }
    let command = unescaped(element(exec, "Command")?.trim());
    // Task Scheduler's own editor quotes a path with a space in it.
    let command = command
        .strip_prefix('"')
        .and_then(|quoted| quoted.strip_suffix('"'))
        .unwrap_or(&command);
    (!command.is_empty()).then(|| PathBuf::from(command))
}

// Task Scheduler leaves a setting out where it is at its default.
pub(crate) fn drifted(xml: &str) -> Vec<Drift> {
    let settings = element(xml, "Settings").unwrap_or_default();
    let on =
        |tag: &str, default: bool| element(settings, tag).map_or(default, |v| v.trim() == "true");
    [
        (on("DisallowStartIfOnBatteries", true), Drift::OnAcPowerOnly),
        (on("StopIfGoingOnBatteries", true), Drift::StopsOnBattery),
        (!on("StartWhenAvailable", false), Drift::SkipsMissedStarts),
        (!on("Enabled", true), Drift::Disabled),
    ]
    .into_iter()
    .filter_map(|(drifted, drift)| drifted.then_some(drift))
    .collect()
}

// An OLE automation date, days since 30 December 1899 on the machine's clock, as epoch seconds.
pub(crate) fn from_ole_date(date: f64, zone: &jiff::tz::TimeZone) -> Option<i64> {
    if !date.is_finite() || date <= 0.0 {
        return None;
    }
    let seconds = (date * 86_400.0).round() as i64;
    jiff::civil::date(1899, 12, 30)
        .at(0, 0, 0, 0)
        .checked_add(jiff::SignedDuration::from_secs(seconds))
        .ok()?
        .to_zoned(zone.clone())
        .ok()
        .map(|at| at.timestamp().as_second())
}

// The first `<tag>`, as Task Scheduler writes one: with no attributes.
fn element<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let (_, after) = xml.split_once(&format!("<{tag}>"))?;
    let (inside, _) = after.split_once(&format!("</{tag}>"))?;
    Some(inside)
}

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn unescaped(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

// Beside the parked logins the job serves, private as they are.
pub(crate) fn environment_file(ctx: &Context, job: Job) -> PathBuf {
    ctx.default_pitboard_home().join(match job {
        Job::Renew => ENVIRONMENT,
        Job::Watch => WATCH_ENVIRONMENT,
    })
}

fn proxy_variable(name: &str) -> bool {
    crate::proxy::NAMING
        .iter()
        .chain(&crate::proxy::EXEMPTING)
        .any(|proxy| *proxy == name)
}

// One `NAME=value` a line; a backslash, a line feed and a carriage return as `\\`, `\n`, `\r`.
pub(crate) fn environment_text(given: &[(String, String)]) -> String {
    given
        .iter()
        .map(|(name, value)| {
            let value = value
                .replace('\\', r"\\")
                .replace('\n', r"\n")
                .replace('\r', r"\r");
            format!("{name}={value}\n")
        })
        .collect()
}

pub(crate) fn environment_read(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(name, _)| proxy_variable(name))
        .map(|(name, value)| {
            let mut read = String::with_capacity(value.len());
            let mut chars = value.chars();
            while let Some(c) = chars.next() {
                if c != '\\' {
                    read.push(c);
                    continue;
                }
                read.push(match chars.next() {
                    Some('n') => '\n',
                    Some('r') => '\r',
                    Some(other) => other,
                    None => '\\',
                });
            }
            (name.to_string(), read)
        })
        .collect()
}

fn keep_environment(permit: Permit, ctx: &Context, job: Job, text: Option<&str>) -> Result<()> {
    let file = environment_file(ctx, job);
    let kept = match text {
        Some(text) => file
            .parent()
            .map_or(Ok(()), |home| {
                crate::host::fs::create_private_dir(permit, home)
            })
            .and_then(|()| {
                crate::atomic::write(permit, &file, text.as_bytes(), crate::atomic::Perms::Secret)
            }),
        None => match crate::host::fs::remove_file(permit, &file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            removed => removed,
        },
    };
    kept.map_err(|source| Error::HomeUnwritable { path: file, source })
}

#[derive(Debug)]
pub(crate) struct Tasks {
    sid: String,
    service: Arc<dyn Service>,
}

impl Tasks {
    pub(crate) fn new(sid: String, service: Arc<dyn Service>) -> Tasks {
        Tasks { sid, service }
    }

    fn registered(&self, ctx: &Context, job: Job) -> Result<Option<Registered>> {
        self.service.read(ctx, &name(job, &self.sid))
    }
}

impl Scheduler for Tasks {
    fn location(&self, _ctx: &Context, job: Job) -> Location {
        Location::Task(path(job, &self.sid))
    }

    fn installed(&self, ctx: &Context, job: Job) -> bool {
        self.registered(ctx, job).is_ok_and(|task| task.is_some())
    }

    fn program(&self, ctx: &Context, job: Job) -> Option<PathBuf> {
        program(&self.registered(ctx, job).ok()??.xml, job)
    }

    fn options(&self, ctx: &Context, job: Job) -> Vec<String> {
        self.registered(ctx, job)
            .ok()
            .flatten()
            .and_then(|task| options(&task.xml, job))
            .unwrap_or_default()
    }

    fn environment(&self, ctx: &Context, job: Job) -> Vec<(String, String)> {
        if !self.installed(ctx, job) {
            return Vec::new();
        }
        self.read_by_its_runs(ctx, job)
    }

    // `PITBOARD_NO_ARGV` means nothing on Windows, so only the proxy is kept. A logon task
    // registered while the person is signed in would wait for the next sign-in, so the watch
    // service is started at once, after its run under the definition before is ended.
    fn put(
        &self,
        ctx: &Context,
        permit: Permit,
        job: Job,
        program: &Path,
        options: &[String],
        environment: &[(String, String)],
    ) -> Result<()> {
        let previous = self.registered(ctx, job)?;
        let before = std::fs::read_to_string(environment_file(ctx, job)).ok();
        let given: Vec<(String, String)> = environment
            .iter()
            .filter(|(name, _)| proxy_variable(name))
            .cloned()
            .collect();
        let text = (!given.is_empty()).then(|| environment_text(&given));
        let named = name(job, &self.sid);
        keep_environment(permit, ctx, job, text.as_deref())?;
        let xml = definition(job, program, options, &first_start(ctx), &self.sid);
        let started = match job {
            Job::Watch if previous.is_some() => self.service.stop(permit, ctx, &named),
            _ => Ok(()),
        }
        .and_then(|()| self.service.register(permit, ctx, &named, &xml))
        .and_then(|()| match job {
            Job::Renew => Ok(()),
            Job::Watch => self.service.run(permit, ctx, &named),
        });
        if let Err(refused) = started {
            let _ = keep_environment(permit, ctx, job, before.as_deref());
            // Whether a refused update leaves the old definition is not measured.
            match previous {
                Some(previous) => {
                    let _ = self.service.register(permit, ctx, &named, &previous.xml);
                    if job == Job::Watch {
                        let _ = self.service.run(permit, ctx, &named);
                    }
                }
                // Registered, it would start at the next sign-in all the same.
                None if job == Job::Watch => {
                    let _ = self.service.delete(permit, ctx, &named);
                }
                None => {}
            }
            return Err(refused);
        }
        Ok(())
    }

    // A task Task Scheduler cannot say is there or not stops an uninstall, which would leave
    // it running every day. The watch service's run is ended first, as no deletion is documented
    // to; renewal's is left to finish, since ending it could lose the login it was renewing.
    fn remove(&self, ctx: &Context, permit: Permit, job: Job) -> Result<bool> {
        let named = name(job, &self.sid);
        let there = self.registered(ctx, job)?.is_some();
        if there {
            if job == Job::Watch {
                self.service.stop(permit, ctx, &named)?;
            }
            self.service.delete(permit, ctx, &named)?;
        }
        keep_environment(permit, ctx, job, None)?;
        Ok(there)
    }

    // Task Scheduler names no task to the program it starts, so a run is told by its marker.
    fn started_this_process(&self, job: Job, said: Option<&str>) -> bool {
        said == Some(path(job, &self.sid).as_str())
    }

    fn read_by_its_runs(&self, ctx: &Context, job: Job) -> Vec<(String, String)> {
        std::fs::read_to_string(environment_file(ctx, job))
            .map(|text| environment_read(&text))
            .unwrap_or_default()
    }

    fn task(&self, ctx: &Context, job: Job) -> Option<Task> {
        let registered = self.registered(ctx, job).ok()??;
        Some(Task {
            last_run: registered
                .last_run
                .filter(|_| registered.last_result != Some(NOT_RUN_YET)),
            last_result: registered.last_result,
            drifted: drifted(&registered.xml),
        })
    }
}

// Task Scheduler's store as a test's machine keeps it, in the test's own home, where the
// system keeps its own in `System32\Tasks`.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn pretend_file(ctx: &Context, task: &str) -> PathBuf {
    task.split('\\')
        .filter(|part| !part.is_empty())
        .fold(ctx.home().join("Tasks"), |dir, part| dir.join(part))
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub(crate) struct Pretend {
    pub(crate) refuse_start: Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(any(test, feature = "test-support"))]
impl Pretend {
    fn file(ctx: &Context, name: &str) -> PathBuf {
        pretend_file(ctx, &format!(r"{FOLDER}\{name}"))
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Service for Pretend {
    fn read(&self, ctx: &Context, name: &str) -> Result<Option<Registered>> {
        match std::fs::read_to_string(Pretend::file(ctx, name)) {
            Ok(xml) => Ok(Some(Registered {
                xml,
                last_run: None,
                last_result: Some(NOT_RUN_YET),
            })),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::ScheduleRefused {
                detail: format!("Task Scheduler said {e}"),
            }),
        }
    }

    // The task inherits its folder's access, as the system's store gives one.
    #[allow(
        clippy::disallowed_methods,
        reason = "Task Scheduler's own store, which a test's machine keeps as plain files"
    )]
    fn register(&self, _: Permit, ctx: &Context, name: &str, xml: &str) -> Result<()> {
        if self
            .refuse_start
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(Error::ScheduleRefused {
                detail: "Task Scheduler refused it in a test".into(),
            });
        }
        let file = Pretend::file(ctx, name);
        file.parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&file, xml))
            .map_err(|source| Error::HomeUnwritable { path: file, source })
    }

    fn delete(&self, permit: Permit, ctx: &Context, name: &str) -> Result<()> {
        let file = Pretend::file(ctx, name);
        match crate::host::fs::remove_file(permit, &file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(Error::HomeUnwritable {
                path: file,
                source: e,
            }),
            _ => Ok(()),
        }
    }

    // Started as the system would start it: refused once where a test says so.
    fn run(&self, _: Permit, _ctx: &Context, _name: &str) -> Result<()> {
        if self
            .refuse_start
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(Error::ScheduleRefused {
                detail: "Task Scheduler would not start it in a test".into(),
            });
        }
        Ok(())
    }

    fn stop(&self, _: Permit, _ctx: &Context, _name: &str) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use crate::host::memory::MemoryHost;
    use crate::schedule::{self, Installed};

    const SID: &str = "S-1-5-21-1004-1005-1006-1001";

    #[test]
    fn the_task_is_defined_with_every_setting_and_names_no_proxy() {
        let exe = Path::new(r"C:\Users\Dana\Tools & Apps\pitboard.exe");
        let xml = definition(Job::Renew, exe, &[], "2026-10-17T09:30:00", SID);
        for setting in [
            "<StartBoundary>2026-10-17T09:30:00</StartBoundary>",
            "<DaysInterval>1</DaysInterval>",
            "<LogonType>InteractiveToken</LogonType>",
            "<RunLevel>LeastPrivilege</RunLevel>",
            "<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>",
            "<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>",
            "<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>",
            "<StartWhenAvailable>true</StartWhenAvailable>",
            "<Hidden>true</Hidden>",
            "<WakeToRun>false</WakeToRun>",
            "<ExecutionTimeLimit>PT10M</ExecutionTimeLimit>",
            r"<Command>C:\Users\Dana\Tools &amp; Apps\pitboard.exe</Command>",
            "<Arguments>renew --scheduled</Arguments>",
        ] {
            assert_eq!(xml.matches(setting).count(), 1, "{setting}\n{xml}");
        }
        for never in ["Password", "S4U", "Highest", "UserId", "PROXY", "proxy"] {
            assert!(!xml.contains(never), "{never}\n{xml}");
        }
        assert_eq!(program(&xml, Job::Renew).as_deref(), Some(exe));
        assert_eq!(drifted(&xml), []);
        assert_eq!(path(Job::Renew, SID), format!(r"\Pitboard\Renew {SID}"));
    }

    // A start at the sign-in a SID in UserId names is documented, not measured (ARCHITECTURE.md).
    #[test]
    fn the_watch_task_starts_at_the_persons_own_sign_in_and_runs_with_no_limit() {
        let exe = Path::new(r"C:\Users\Dana\Tools & Apps\pitboard.exe");
        let at = ["--at".to_string(), "90".to_string()];
        let xml = definition(Job::Watch, exe, &at, "2026-10-17T09:30:00", SID);
        for setting in [
            &format!("<LogonTrigger>\n      <UserId>{SID}</UserId>\n    </LogonTrigger>")[..],
            "<LogonType>InteractiveToken</LogonType>",
            "<RunLevel>LeastPrivilege</RunLevel>",
            "<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>",
            "<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>",
            "<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>",
            "<Hidden>true</Hidden>",
            "<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>",
            r"<Command>C:\Users\Dana\Tools &amp; Apps\pitboard.exe</Command>",
            "<Arguments>watch --scheduled --at 90</Arguments>",
        ] {
            assert_eq!(xml.matches(setting).count(), 1, "{setting}\n{xml}");
        }
        for never in [
            "CalendarTrigger",
            "StartBoundary",
            "Password",
            "S4U",
            "Highest",
        ] {
            assert!(!xml.contains(never), "{never}\n{xml}");
        }
        assert_eq!(program(&xml, Job::Watch).as_deref(), Some(exe));
        assert_eq!(options(&xml, Job::Watch), Some(at.to_vec()));
        assert_eq!(program(&xml, Job::Renew), None, "not renewal's");
        assert_eq!(drifted(&xml), []);
        assert_eq!(path(Job::Watch, SID), format!(r"\Pitboard\Watch {SID}"));
    }

    #[test]
    fn only_a_task_run_the_way_pitboard_writes_it_names_its_program() {
        let exe = Path::new(r"C:\Program Files\Pitboard\bin\pitboard.exe");
        let xml = definition(Job::Renew, exe, &[], "2026-10-17T09:30:00", SID);
        let quoted = xml
            .replace("<Command>C", "<Command>\"C")
            .replace(".exe</Command>", ".exe\"</Command>");
        assert_eq!(
            program(&quoted, Job::Renew).as_deref(),
            Some(exe),
            "as Task Scheduler's own editor quotes it"
        );
        assert_eq!(
            program(&xml.replace("renew --scheduled", "renew"), Job::Renew),
            None
        );
        assert_eq!(program("not a task", Job::Renew), None);
    }

    #[test]
    fn a_setting_changed_in_task_scheduler_is_read_as_it_leaves_it() {
        let xml = definition(
            Job::Renew,
            Path::new(r"C:\p.exe"),
            &[],
            "2026-10-17T09:30:00",
            SID,
        );
        let changed = xml
            .replace(
                "<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>",
                "",
            )
            .replace(
                "<StartWhenAvailable>true</StartWhenAvailable>",
                "<StartWhenAvailable>false</StartWhenAvailable>",
            )
            .replace("<Hidden>", "<Enabled>false</Enabled>\n    <Hidden>");
        assert_eq!(
            drifted(&changed),
            [
                Drift::OnAcPowerOnly,
                Drift::SkipsMissedStarts,
                Drift::Disabled
            ]
        );
        assert_eq!(
            drifted("<Task><Settings></Settings></Task>"),
            [
                Drift::OnAcPowerOnly,
                Drift::StopsOnBattery,
                Drift::SkipsMissedStarts
            ]
        );
        let trigger_off = xml.replace(
            "<StartBoundary>",
            "<Enabled>false</Enabled>\n      <StartBoundary>",
        );
        assert_eq!(
            drifted(&trigger_off),
            [],
            "a trigger's own is not the task's"
        );
    }

    #[test]
    fn a_last_run_is_read_on_the_machines_clock() {
        let utc = jiff::tz::TimeZone::UTC;
        assert_eq!(from_ole_date(45_000.5, &utc), Some(1_678_881_600));
        let ahead = jiff::tz::TimeZone::fixed(jiff::tz::offset(7));
        assert_eq!(
            from_ole_date(45_000.5, &ahead),
            Some(1_678_881_600 - 7 * 3_600)
        );
        assert_eq!(from_ole_date(0.0, &utc), None);
        assert_eq!(from_ole_date(f64::NAN, &utc), None);
        let not_run = Task {
            last_run: None,
            last_result: Some(NOT_RUN_YET),
            drifted: Vec::new(),
        };
        assert_eq!(not_run.failed(), None);
        for fine in [0, RUNNING, QUEUED] {
            let task = Task {
                last_result: Some(fine),
                ..not_run.clone()
            };
            assert_eq!(task.failed(), None, "{fine:#x}");
        }
        for failed in [1, 0x8007_10E0_u32 as i32] {
            let task = Task {
                last_result: Some(failed),
                ..not_run.clone()
            };
            assert_eq!(task.failed(), Some(failed));
        }
    }

    #[test]
    fn the_environment_file_reads_back_what_it_wrote_and_only_a_proxy() {
        let given = vec![
            (
                "ALL_PROXY".to_string(),
                "socks5h://alice:p\\ss=w\nrd@127.0.0.1:1080\r".to_string(),
            ),
            ("no_proxy".to_string(), String::new()),
        ];
        let text = environment_text(&given);
        assert_eq!(text.lines().count(), 2, "{text}");
        assert_eq!(environment_read(&text), given);
        assert_eq!(
            environment_read("PATH=C:\\Windows\nHTTPS_PROXY=http://proxy.example.com:3128\n"),
            [(
                "HTTPS_PROXY".to_string(),
                "http://proxy.example.com:3128".to_string()
            )]
        );
    }

    // A scratch home on a machine that schedules as Windows does, through a pretend.
    struct TaskHome(tempfile::TempDir, Arc<MemoryHost>);

    impl TaskHome {
        fn new() -> TaskHome {
            let scratch = tempfile::tempdir().expect("a scratch home");
            let program = scratch.path().join("bin").join("pitboard.exe");
            std::fs::create_dir_all(program.parent().expect("its folder")).expect("made");
            std::fs::write(&program, "").expect("a program");
            TaskHome(scratch, MemoryHost::scheduling_tasks(SID))
        }

        fn program(&self) -> PathBuf {
            self.0.path().join("bin").join("pitboard.exe")
        }

        fn ctx(&self, vars: &[(&str, &str)]) -> Context {
            let home = self.0.path().to_string_lossy().into_owned();
            let env: crate::context::Environment =
                [("HOME", home.as_str()), ("USERPROFILE", home.as_str())]
                    .iter()
                    .chain(vars)
                    .copied()
                    .collect();
            Context::for_command_line(&env)
                .with_memory_stores(Arc::clone(&self.1))
                .with_schedule_program(self.program())
        }
    }

    // Task Scheduler as the pretend keeps it, writing down each run and stop it is asked for
    // and refusing the next run where a test says so.
    #[derive(Debug)]
    struct Asked {
        tasks: Pretend,
        refuse_run: std::sync::atomic::AtomicBool,
        asked: std::sync::Mutex<Vec<String>>,
    }

    impl Asked {
        fn new() -> Arc<Asked> {
            Arc::new(Asked {
                tasks: Pretend {
                    refuse_start: Arc::default(),
                },
                refuse_run: std::sync::atomic::AtomicBool::new(false),
                asked: std::sync::Mutex::new(Vec::new()),
            })
        }

        fn said(&self, what: &str, name: &str) {
            self.asked
                .lock()
                .expect("a poisoned test is a failed test")
                .push(format!("{what} {name}"));
        }

        fn taken(&self) -> Vec<String> {
            std::mem::take(&mut self.asked.lock().expect("a poisoned test is a failed test"))
        }
    }

    impl Service for Asked {
        fn read(&self, ctx: &Context, name: &str) -> Result<Option<Registered>> {
            self.tasks.read(ctx, name)
        }

        fn register(&self, permit: Permit, ctx: &Context, name: &str, xml: &str) -> Result<()> {
            self.tasks.register(permit, ctx, name, xml)
        }

        fn delete(&self, permit: Permit, ctx: &Context, name: &str) -> Result<()> {
            self.tasks.delete(permit, ctx, name)
        }

        fn run(&self, _: Permit, _ctx: &Context, name: &str) -> Result<()> {
            self.said("run", name);
            if self
                .refuse_run
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                return Err(Error::ScheduleRefused {
                    detail: "Task Scheduler would not run it in a test".into(),
                });
            }
            Ok(())
        }

        fn stop(&self, _: Permit, _ctx: &Context, name: &str) -> Result<()> {
            self.said("stop", name);
            Ok(())
        }
    }

    fn at(percent: &str) -> Vec<String> {
        vec!["--at".to_string(), percent.to_string()]
    }

    // Ending renewal's run could land between Anthropic's answer and the park it renews.
    #[test]
    fn only_the_watch_services_run_is_ended_when_its_task_is_taken_away() {
        let machine = TaskHome::new();
        let ctx = machine.ctx(&[]);
        let asked = Asked::new();
        let tasks = Tasks::new(SID.into(), asked.clone());
        let program = machine.program();
        tasks
            .put(&ctx, Permit::for_a_test(), Job::Renew, &program, &[], &[])
            .expect("renewal");
        tasks
            .put(
                &ctx,
                Permit::for_a_test(),
                Job::Watch,
                &program,
                &at("90"),
                &[],
            )
            .expect("the watch service");
        asked.taken();

        assert!(
            tasks
                .remove(&ctx, Permit::for_a_test(), Job::Renew)
                .expect("taken away")
        );
        assert_eq!(asked.taken(), Vec::<String>::new());
        assert!(
            tasks
                .remove(&ctx, Permit::for_a_test(), Job::Watch)
                .expect("taken away")
        );
        assert_eq!(asked.taken(), [format!("stop Watch {SID}")]);
    }

    // Registered, a new task would start at the next sign-in all the same; one before is run
    // again as it was.
    #[test]
    fn a_watch_task_that_will_not_run_is_taken_away_or_put_back() {
        let machine = TaskHome::new();
        let ctx = machine.ctx(&[]);
        let asked = Asked::new();
        let tasks = Tasks::new(SID.into(), asked.clone());
        let program = machine.program();
        let watch = format!("Watch {SID}");

        asked
            .refuse_run
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let refused = tasks
            .put(
                &ctx,
                Permit::for_a_test(),
                Job::Watch,
                &program,
                &at("90"),
                &[],
            )
            .expect_err("refused");
        assert_eq!(refused.code(), "schedule_refused");
        assert!(!tasks.installed(&ctx, Job::Watch));
        assert_eq!(asked.taken(), [format!("run {watch}")]);

        tasks
            .put(
                &ctx,
                Permit::for_a_test(),
                Job::Watch,
                &program,
                &at("90"),
                &[],
            )
            .expect("installed");
        asked.taken();
        asked
            .refuse_run
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let proxied = [(
            "HTTPS_PROXY".to_string(),
            "http://proxy.example.com:3128".to_string(),
        )];
        tasks
            .put(
                &ctx,
                Permit::for_a_test(),
                Job::Watch,
                &program,
                &at("70"),
                &proxied,
            )
            .expect_err("refused");
        assert_eq!(tasks.options(&ctx, Job::Watch), at("90"));
        assert!(!environment_file(&ctx, Job::Watch).exists());
        assert_eq!(
            asked.taken(),
            [
                format!("stop {watch}"),
                format!("run {watch}"),
                format!("run {watch}")
            ]
        );
    }

    // The run under way is ended only once nothing is left to fail before the new one starts.
    #[test]
    fn a_watch_environment_that_cannot_be_written_leaves_the_run_under_way() {
        let machine = TaskHome::new();
        let ctx = machine.ctx(&[]);
        let asked = Asked::new();
        let tasks = Tasks::new(SID.into(), asked.clone());
        let program = machine.program();
        tasks
            .put(
                &ctx,
                Permit::for_a_test(),
                Job::Watch,
                &program,
                &at("90"),
                &[],
            )
            .expect("installed");
        asked.taken();
        std::fs::create_dir_all(environment_file(&ctx, Job::Watch).join("in the way"))
            .expect("a folder where the file goes");
        let proxied = [(
            "HTTPS_PROXY".to_string(),
            "http://proxy.example.com:3128".to_string(),
        )];
        tasks
            .put(
                &ctx,
                Permit::for_a_test(),
                Job::Watch,
                &program,
                &at("70"),
                &proxied,
            )
            .expect_err("unwritable");
        assert_eq!(asked.taken(), Vec::<String>::new());
        assert_eq!(tasks.options(&ctx, Job::Watch), at("90"));
    }

    const PROXIED: [(&str, &str); 3] = [
        ("PITBOARD_NO_ARGV", "1"),
        ("HTTPS_PROXY", "http://alice:s3cret@proxy.example.com:3128"),
        ("NO_PROXY", "localhost"),
    ];

    #[test]
    fn the_task_keeps_its_proxy_beside_the_parked_logins_and_takes_it_away() {
        let machine = TaskHome::new();
        let proxied = machine.ctx(&PROXIED);
        let task = Location::Task(format!(r"\Pitboard\Renew {SID}"));

        let installed = schedule::install(&proxied, Permit::for_a_test()).expect("installed");
        assert_eq!(installed, task);
        assert_eq!(
            schedule::status(&proxied),
            Installed::Yes {
                location: task.clone(),
                every_seconds: EVERY_SECONDS
            }
        );
        assert_eq!(
            schedule::installed_program(&proxied),
            Some(machine.program())
        );
        let definition =
            std::fs::read_to_string(schedule::path(&proxied).expect("the task")).expect("written");
        assert!(
            !definition.contains("alice") && !definition.contains("PROXY"),
            "{definition}"
        );
        let file = environment_file(&proxied, Job::Renew);
        assert_eq!(
            std::fs::read_to_string(&file).expect("the proxy"),
            "HTTPS_PROXY=http://alice:s3cret@proxy.example.com:3128\nNO_PROXY=localhost\n",
            "and no PITBOARD_NO_ARGV, which Windows reads nowhere"
        );
        assert!(
            !crate::host::fs::access(&file).expect("its access").shared,
            "a proxy's password is the person's alone to read"
        );
        assert!(
            schedule::installed_proxies(&proxied)
                .expect("installed")
                .same_way(&proxied.proxy)
        );

        let by_hand = machine.ctx(&[("ALL_PROXY", ""), ("HTTPS_PROXY", "http://other:3128")]);
        let run = schedule::for_its_run(&by_hand.clone().started_by_the_schedule(), Job::Renew);
        assert_eq!(
            run.proxy.variables(),
            [
                ("ALL_PROXY", String::new()),
                (
                    "HTTPS_PROXY",
                    "http://alice:s3cret@proxy.example.com:3128".to_string()
                ),
                ("NO_PROXY", "localhost".to_string()),
            ],
            "the file's over its own"
        );
        assert!(run.proxy.same_way(&proxied.proxy));
        assert_eq!(
            schedule::for_its_run(&by_hand, Job::Renew)
                .proxy
                .variables(),
            by_hand.proxy.variables(),
            "a run by hand reads its own"
        );
        let tasks = proxied.host().scheduler().expect("Task Scheduler");
        assert!(tasks.started_this_process(Job::Renew, Some(&format!(r"\Pitboard\Renew {SID}"))));
        assert!(!tasks.started_this_process(Job::Renew, None));

        let direct = machine.ctx(&[]);
        schedule::install(&direct, Permit::for_a_test()).expect("installed again");
        assert!(!file.exists(), "a reinstall takes this run's, none");
        assert_eq!(
            schedule::installed_program(&direct),
            Some(machine.program())
        );

        assert!(schedule::uninstall(&proxied, Permit::for_a_test()).expect("taken away"));
        assert_eq!(schedule::status(&proxied), Installed::No);
        assert!(!schedule::path(&proxied).expect("where").exists());
        assert!(!schedule::uninstall(&proxied, Permit::for_a_test()).expect("nothing there"));
    }

    #[test]
    fn a_task_task_scheduler_cannot_read_stops_its_removal() {
        let machine = TaskHome::new();
        let proxied = machine.ctx(&PROXIED);
        schedule::install(&proxied, Permit::for_a_test()).expect("installed");
        let task = schedule::path(&proxied).expect("the task");
        std::fs::remove_file(&task).expect("taken out");
        std::fs::create_dir(&task).expect("put back as nothing Task Scheduler can read");
        let refused = schedule::uninstall(&proxied, Permit::for_a_test()).expect_err("unknown");
        assert_eq!(refused.code(), "schedule_refused");
        assert!(
            task.exists() && environment_file(&proxied, Job::Renew).exists(),
            "nothing taken away"
        );
        let refused = schedule::install(&proxied, Permit::for_a_test()).expect_err("unknown");
        assert_eq!(refused.code(), "schedule_refused");
        assert!(
            environment_file(&proxied, Job::Renew).exists(),
            "nothing written"
        );
    }

    #[test]
    fn a_task_the_system_refuses_leaves_the_one_before_and_its_proxy() {
        let machine = TaskHome::new();
        let proxied = machine.ctx(&PROXIED);
        machine.1.refuse_next_start();
        let refused = schedule::install(&proxied, Permit::for_a_test()).expect_err("refused");
        assert_eq!(refused.code(), "schedule_refused");
        assert_eq!(schedule::status(&proxied), Installed::No);
        assert!(
            !environment_file(&proxied, Job::Renew).exists(),
            "nothing left written"
        );

        schedule::install(&proxied, Permit::for_a_test()).expect("installed");
        let before =
            std::fs::read_to_string(environment_file(&proxied, Job::Renew)).expect("the proxy");
        let elsewhere = machine.0.path().join("elsewhere").join("pitboard.exe");
        std::fs::create_dir_all(elsewhere.parent().expect("its folder")).expect("made");
        std::fs::write(&elsewhere, "").expect("another program");
        machine.1.refuse_next_start();
        let refused = schedule::install(
            &machine
                .ctx(&[("HTTPS_PROXY", "http://other:3128")])
                .with_schedule_program(elsewhere),
            Permit::for_a_test(),
        )
        .expect_err("refused");
        assert_eq!(refused.code(), "schedule_refused");
        assert_eq!(
            schedule::installed_program(&proxied),
            Some(machine.program())
        );
        assert_eq!(
            std::fs::read_to_string(environment_file(&proxied, Job::Renew)).expect("the proxy"),
            before
        );
    }
}
