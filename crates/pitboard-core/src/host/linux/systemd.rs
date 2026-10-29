//! Pitboard's jobs on Linux: a systemd user timer for daily renewal, whose service also starts
//! at each graphical sign-in, and a systemd user service for the watch service, which is that
//! system's answer.
//!
//! `systemctl --user` reaches the user's own systemd whatever `HOME` and `XDG_CONFIG_HOME` this
//! process has, and that manager runs a unit with its own `HOME`, where the job then acts. So
//! before anything is written or taken away, the manager is asked where it reads units from and
//! which home it runs them in, and a job is put only where it would run for this home.

use super::super::unix::service::{self, Control};
use super::bus;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::host::Scheduler;
use crate::permit::Permit;
use crate::schedule::{Elsewhere, Job, Location, RESTART_AFTER_SECONDS};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const SYSTEMCTL: &str = "systemctl";
const TIMER: &str = "pitboard-renew.timer";
const SERVICE: &str = "pitboard-renew.service";
const WATCH: &str = "pitboard-watch.service";
const RELOAD: [&str; 2] = ["--user", "daemon-reload"];

/// What the user's systemd says of the units it runs: the folders it reads them from, its
/// `UnitPath`, and the `HOME` its `Environment` gives them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Reads {
    pub(super) unit_path: Vec<PathBuf>,
    pub(super) home: Option<PathBuf>,
}

impl Reads {
    fn reads_from(&self, units: &Path) -> bool {
        self.unit_path.iter().any(|folder| folder == units)
    }
}

/// What the user's systemd answered, or why nothing from this session reached it to ask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Answer {
    Reads(Reads),
    /// No runtime folder or bus names it, as in a shell `su` started, or nothing serves the one
    /// named. `systemctl --user` reaches it the same way.
    Unreached(String),
}

/// Whoever answers for the user's systemd.
pub(super) trait Manager: Send + Sync + std::fmt::Debug {
    fn reads(&self, ctx: &Context) -> Result<Answer>;
}

/// The user's own systemd, asked over D-Bus.
#[derive(Debug)]
pub(super) struct UserManager;

impl Manager for UserManager {
    /// Never from a build for tests, which [`service::System`] runs nothing for either: the
    /// manager a test would ask is the person's own, whatever home the test gives.
    fn reads(&self, ctx: &Context) -> Result<Answer> {
        if cfg!(any(test, feature = "test-support")) {
            return Err(Error::ScheduleRefused {
                detail: "a build for tests asked the user manager itself".into(),
            });
        }
        Ok(asked(ctx).map_or_else(Answer::Unreached, Answer::Reads))
    }
}

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1",
    gen_async = false
)]
trait SystemdManager {
    #[zbus(property)]
    fn unit_path(&self) -> zbus::Result<Vec<String>>;

    #[zbus(property)]
    fn environment(&self) -> zbus::Result<Vec<String>>;
}

// Both are typed arrays of strings in org.freedesktop.systemd1(5) at v262, `Environment` one
// `NAME=value` each. No property is cached: a peer to peer connection gets no signal.
fn asked(ctx: &Context) -> std::result::Result<Reads, String> {
    let connection = bus::user_manager(ctx)?;
    let manager = SystemdManagerProxy::builder(&connection)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .map_err(|e| e.to_string())?;
    let unit_path = manager.unit_path().map_err(|e| e.to_string())?;
    let environment = manager.environment().map_err(|e| e.to_string())?;
    Ok(Reads {
        unit_path: unit_path.into_iter().map(PathBuf::from).collect(),
        home: environment
            .iter()
            .find_map(|given| given.strip_prefix("HOME="))
            .map(PathBuf::from),
    })
}

/// The user's systemd as a machine in memory has it: it reads this context's own units folder
/// and runs units in this context's home.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub(super) struct Pretend;

#[cfg(any(test, feature = "test-support"))]
impl Manager for Pretend {
    fn reads(&self, ctx: &Context) -> Result<Answer> {
        Ok(Answer::Reads(Reads {
            unit_path: vec![Systemd::units(ctx)],
            home: Some(ctx.home().to_path_buf()),
        }))
    }
}

#[derive(Debug)]
pub(super) struct Systemd {
    control: Arc<dyn Control>,
    manager: Arc<dyn Manager>,
}

impl Systemd {
    pub(super) fn new(control: Arc<dyn Control>, manager: Arc<dyn Manager>) -> Systemd {
        Systemd { control, manager }
    }

    /// Where the user's systemd reads a person's own units, as systemd.unit(5) lists its user
    /// search path: `systemd/user` in `XDG_CONFIG_HOME`, else in `~/.config`.
    fn units(ctx: &Context) -> PathBuf {
        ctx.config_home()
            .map_or_else(|| ctx.home().join(".config"), Path::to_path_buf)
            .join("systemd/user")
    }

    // Where Pitboard wrote them before it read `XDG_CONFIG_HOME`, whatever that said.
    fn written_before(ctx: &Context) -> PathBuf {
        ctx.home().join(".config/systemd/user")
    }

    // Each folder a job is looked for in, the one it is written in first.
    fn folders(ctx: &Context) -> Vec<PathBuf> {
        let (units, before) = (Self::units(ctx), Self::written_before(ctx));
        if units == before {
            vec![units]
        } else {
            vec![units, before]
        }
    }

    fn holding(ctx: &Context, job: Job) -> Option<PathBuf> {
        Self::folders(ctx)
            .into_iter()
            .find(|folder| folder.join(Self::installed_by(job)).is_file())
    }

    fn service(job: Job) -> &'static str {
        match job {
            Job::Renew => SERVICE,
            Job::Watch => WATCH,
        }
    }

    // What says the job is installed: renewal's timer, or the watch service itself.
    fn installed_by(job: Job) -> &'static str {
        match job {
            Job::Renew => TIMER,
            Job::Watch => WATCH,
        }
    }

    // The link `systemctl enable` makes for each unit's `WantedBy=`, as systemd.unit(5) says:
    // its folder, and the unit it names.
    fn links(job: Job) -> &'static [(&'static str, &'static str)] {
        match job {
            Job::Renew => &[
                ("timers.target.wants", TIMER),
                ("graphical-session.target.wants", SERVICE),
            ],
            Job::Watch => &[("default.target.wants", WATCH)],
        }
    }

    fn read(ctx: &Context, job: Job) -> Option<String> {
        std::fs::read_to_string(Self::holding(ctx, job)?.join(Self::service(job))).ok()
    }

    // Each `systemctl --user` call that starts the job, in order.
    fn starts(job: Job) -> &'static [&'static [&'static str]] {
        match job {
            Job::Renew => &[
                &["--user", "enable", SERVICE],
                &["--user", "enable", "--now", TIMER],
            ],
            // `restart` starts a stopped service, and one already running with the new unit.
            Job::Watch => &[&["--user", "enable", WATCH], &["--user", "restart", WATCH]],
        }
    }

    // Each call that takes back what a refused start enabled: all of it where the job was not
    // there before. Where it was, renewal's service is enabled again from the unit put back, which
    // an earlier Pitboard wrote with no `[Install]`, and the timer and the watch service are as
    // they were.
    fn takes_back(job: Job, was_there: bool) -> &'static [&'static [&'static str]] {
        match (job, was_there) {
            (Job::Renew, false) => &[
                &["--user", "disable", "--now", TIMER],
                &["--user", "disable", SERVICE],
            ],
            (Job::Renew, true) => &[&["--user", "disable", SERVICE]],
            (Job::Watch, false) => &[&["--user", "disable", WATCH]],
            (Job::Watch, true) => &[],
        }
    }

    fn start(&self, permit: Permit, job: Job) -> Result<()> {
        Self::starts(job)
            .iter()
            .try_for_each(|args| self.control.start(permit, SYSTEMCTL, args))
    }

    /// Stops and disables `job`: renewal's service with its timer, and renewal's record of its
    /// last run cleared, as systemd.timer(5) asks before a timer with `Persistent=` is taken
    /// away, whatever `systemctl` says. A watch service left running writes in the Pitboard
    /// directory `pitboard uninstall` removes next, so its refusal is the answer.
    fn stop(&self, permit: Permit, job: Job) -> Result<()> {
        let stopped = self.control.run(
            permit,
            SYSTEMCTL,
            &["--user", "disable", "--now", Self::installed_by(job)],
        );
        match job {
            Job::Watch => stopped,
            Job::Renew => {
                let _ = self
                    .control
                    .run(permit, SYSTEMCTL, &["--user", "disable", SERVICE]);
                let _ = self.control.run(
                    permit,
                    SYSTEMCTL,
                    &["--user", "clean", "--what=state", TIMER],
                );
                Ok(())
            }
        }
    }

    /// What the user's systemd answered, where it runs units with this context's home or
    /// nothing reached it. Where it runs them with another, a job written here would act on
    /// that home, and one there is not this home's to take away.
    fn manager_of_this_home(&self, ctx: &Context, job: Job) -> Result<Answer> {
        match self.manager.reads(ctx)? {
            Answer::Reads(reads) if reads.home.as_deref() != Some(ctx.home()) => {
                Err(Error::ScheduleManagerElsewhere {
                    elsewhere: Elsewhere::Home {
                        home: ctx.home().to_path_buf(),
                        theirs: reads.home,
                    },
                    job,
                })
            }
            answer => Ok(answer),
        }
    }
}

fn unreached(why: &str) -> Error {
    Error::ScheduleRefused {
        detail: format!("your user's systemd could not be asked where it reads units: {why}"),
    }
}

// sd_booted(3) asks the same: systemd as init makes this directory as the system starts.
pub(super) fn started_by_systemd(root: &Path) -> bool {
    root.join("run/systemd/system").is_dir()
}

impl Scheduler for Systemd {
    fn location(&self, ctx: &Context, job: Job) -> Location {
        let folder = Self::holding(ctx, job).unwrap_or_else(|| Self::units(ctx));
        Location::File(folder.join(Self::installed_by(job)))
    }

    fn installed(&self, ctx: &Context, job: Job) -> bool {
        Self::holding(ctx, job).is_some()
    }

    /// Read from the service's `ExecStart`, as [`unit_file`] writes it, or as Pitboard wrote
    /// renewal's before its job carried the marker: `<program> renew`.
    fn program(&self, ctx: &Context, job: Job) -> Option<PathBuf> {
        command(&Self::read(ctx, job)?, job).map(|(program, _)| program)
    }

    fn options(&self, ctx: &Context, job: Job) -> Vec<String> {
        Self::read(ctx, job)
            .and_then(|body| command(&body, job))
            .map(|(_, options)| options)
            .unwrap_or_default()
    }

    /// Read from the service's `Environment=` lines, as [`unit_file`] writes them.
    fn environment(&self, ctx: &Context, job: Job) -> Vec<(String, String)> {
        Self::read(ctx, job)
            .map(|body| given(&body))
            .unwrap_or_default()
    }

    fn put(
        &self,
        ctx: &Context,
        permit: Permit,
        job: Job,
        program: &Path,
        options: &[String],
        environment: &[(String, String)],
    ) -> Result<()> {
        let program = spelled(program)?;
        let units = Self::units(ctx);
        match self.manager_of_this_home(ctx, job)? {
            Answer::Reads(reads) if reads.reads_from(&units) => {}
            Answer::Reads(_) => {
                return Err(Error::ScheduleManagerElsewhere {
                    elsewhere: Elsewhere::Folder { units },
                    job,
                });
            }
            Answer::Unreached(why) => return Err(unreached(&why)),
        }
        let unit = units.join(Self::service(job));
        let timer_path = units.join(TIMER);
        let before = (
            std::fs::read_to_string(&unit).ok(),
            std::fs::read_to_string(&timer_path).ok(),
        );
        service::write(
            permit,
            &unit,
            &unit_file(job, &program, options, environment),
        )?;
        if job == Job::Renew {
            service::write(permit, &timer_path, &timer())?;
        }
        let _ = self.control.run(permit, SYSTEMCTL, &RELOAD);
        let Err(refused) = self.start(permit, job) else {
            return Ok(());
        };
        let was_there = match job {
            Job::Renew => before.1.is_some(),
            Job::Watch => before.0.is_some(),
        };
        for args in Self::takes_back(job, was_there) {
            let _ = self.control.run(permit, SYSTEMCTL, args);
        }
        service::restore(permit, &unit, before.0.as_deref());
        if job == Job::Renew {
            service::restore(permit, &timer_path, before.1.as_deref());
        }
        let _ = self.control.run(permit, SYSTEMCTL, &RELOAD);
        if was_there {
            let _ = self.start(permit, job);
        }
        Err(refused)
    }

    /// Where the user's systemd reads the folder a job is in, it is stopped and disabled
    /// through `systemctl` first ([`Systemd::stop`]). Where it does not, it never loaded the job,
    /// so its files and the links `enable` made go and `systemctl` is asked nothing. Where
    /// nothing reached it to ask, as in a shell `su` started, both are done: `systemctl` is
    /// asked as before Pitboard asked the manager, and the links go in case it reached nothing
    /// either.
    fn remove(&self, ctx: &Context, permit: Permit, job: Job) -> Result<bool> {
        let holding: Vec<PathBuf> = Self::folders(ctx)
            .into_iter()
            .filter(|folder| folder.join(Self::installed_by(job)).is_file())
            .collect();
        if holding.is_empty() {
            return Ok(false);
        }
        let answer = self.manager_of_this_home(ctx, job)?;
        let loads = |folder: &Path| match &answer {
            Answer::Reads(reads) => Some(reads.reads_from(folder)),
            Answer::Unreached(_) => None,
        };
        let asks = holding.iter().any(|folder| loads(folder) != Some(false));
        if asks {
            self.stop(permit, job)?;
        }
        for folder in &holding {
            if loads(folder) != Some(true) {
                for (wants, unit) in Self::links(job) {
                    service::remove(permit, &folder.join(wants).join(unit))?;
                }
            }
            service::remove(permit, &folder.join(Self::installed_by(job)))?;
            service::remove(permit, &folder.join(Self::service(job)))?;
        }
        if asks {
            let _ = self.control.run(permit, SYSTEMCTL, &RELOAD);
        }
        Ok(true)
    }

    /// Nothing systemd passes is read here, only what a test says the job is: what it
    /// passes is not to be relied on ([`crate::schedule::SCHEDULED_RUN`]), so the service's
    /// runs are told apart by the marker its `ExecStart` carries, which the command line
    /// started with it passes on. A repair on Linux stops nothing either way: it rewrites the
    /// units and enables the timer again, so a run of the schedule is not in its way.
    fn started_this_process(&self, job: Job, said: Option<&str>) -> bool {
        said == Some(Self::service(job))
    }

    // A timer written before `OnCalendar=` counted only time awake, and a service written
    // before `ConditionFileIsExecutable=` fails at each start once its Pitboard is gone.
    fn outdated(&self, ctx: &Context, job: Job) -> bool {
        let Some(folder) = Self::holding(ctx, job) else {
            return false;
        };
        let lacks = |file: &str, setting: &str| {
            !std::fs::read_to_string(folder.join(file))
                .is_ok_and(|body| body.lines().any(|line| line.starts_with(setting)))
        };
        lacks(Self::service(job), "ConditionFileIsExecutable=")
            || (job == Job::Renew && lacks(TIMER, "OnCalendar="))
    }
}

/// A program's path as a unit spells it, read in systemd.service(5), systemd.syntax(7) and
/// systemd's `src/core/load-fragment.c` at tag `v262`. `ExecStart=` unquotes its first word,
/// then expands specifiers, so the path is in double quotes where it holds a space and each `%`
/// is `%%`; `$` is kept, since no variable is expanded in the program a unit runs.
/// `ConditionFileIsExecutable=` is not unquoted, only expanded, so there it is the path with
/// `%%`.
struct Spelled {
    run: String,
    condition: String,
}

/// `program` spelled for a unit, or refused before anything is written where no unit can run
/// it: `config_parse_exec` refuses a program whose path is not UTF-8 or holds a control
/// character, a backslash or a quote (`string_is_safe`), and the unit file parser strips a space
/// at the end of a line.
fn spelled(program: &Path) -> Result<Spelled> {
    let refused = |what: &str| Error::ScheduleRefused {
        detail: format!(
            "a systemd unit cannot run a program whose path holds {what}, so nothing was \
             written. Install Pitboard where its path holds none, then try again."
        ),
    };
    let path = program
        .to_str()
        .ok_or_else(|| refused("bytes that are not UTF-8"))?;
    if let Some(c) = path.chars().find(char::is_ascii_control) {
        return Err(refused(&format!("the character U+{:04X}", u32::from(c))));
    }
    if path.contains('\\') {
        return Err(refused("a backslash"));
    }
    if path.contains(['"', '\'']) {
        return Err(refused("a quote"));
    }
    if path.ends_with(' ') {
        return Err(refused("a space at its end"));
    }
    let condition = path.replace('%', "%%");
    let run = if condition.contains(' ') {
        format!("\"{condition}\"")
    } else {
        condition.clone()
    };
    Ok(Spelled { run, condition })
}

/// The program and the options a service's `ExecStart` runs, as [`unit_file`] writes it, or
/// as Pitboard wrote it before: the path as it is, and for renewal before its job carried the
/// marker, `<program> renew`.
fn command(body: &str, job: Job) -> Option<(PathBuf, Vec<String>)> {
    let run = body
        .lines()
        .find_map(|line| line.strip_prefix("ExecStart="))?;
    let marked = format!(" {}", job.marked().join(" "));
    let quoted = run
        .strip_prefix('"')
        .and_then(|quoted| quoted.split_once('"'));
    let (program, options) = match quoted {
        Some((program, after)) => (program, after.strip_prefix(marked.as_str())?),
        None => run.rsplit_once(marked.as_str()).or_else(|| match job {
            Job::Renew => run.strip_suffix(" renew").map(|program| (program, "")),
            Job::Watch => None,
        })?,
    };
    let options = options.split_whitespace().map(str::to_owned).collect();
    Some((PathBuf::from(program.replace("%%", "%")), options))
}

/// systemd's own format. `environment` is as for the launchd job: the service runs with
/// systemd's environment, so what it is given of the installing Pitboard's,
/// `PITBOARD_NO_ARGV` and the proxy variables, is written in, one `Environment=` line each.
/// It runs `<program>` with the job's marker, which a run of the job is told apart by rather
/// than by what systemd passes it, and `options`. A Pitboard from before renewal's marker
/// rejects the flag, so after going back to one the timer fails every day until that
/// Pitboard's `schedule install` rewrites the unit, which the CHANGELOG says.
///
/// Each service is skipped, not failed, once its program is gone or no longer runs
/// (`ConditionFileIsExecutable=`, systemd.unit(5)), as after the package that installed it is
/// removed. Renewal's service also starts at each graphical sign-in, once the session is up
/// (`WantedBy=` and `After=graphical-session.target`, systemd.special(7)): a run the login
/// keyring refused at boot runs again after signing in has unlocked it. `After=` orders only
/// units started together (systemd.unit(5)), so a run its timer starts waits for no session.
/// The watch service starts with the person's systemd, at their first sign-in, and again a
/// minute after it fails or crashes: a panic exits 101, and a run an update left behind exits
/// on a state file the new build wrote.
fn unit_file(
    job: Job,
    program: &Spelled,
    options: &[String],
    environment: &[(String, String)],
) -> String {
    let environment = environment
        .iter()
        .map(|(name, value)| assignment(name, value))
        .collect::<String>();
    let run = [job.marked().join(" "), options.join(" ")].join(" ");
    let run = format!("{} {}", program.run, run.trim_end());
    let condition = &program.condition;
    match job {
        Job::Renew => format!(
            "[Unit]\n\
             Description=Renew Pitboard's parked logins\n\
             Documentation=https://docs.usepitboard.com\n\
             After=graphical-session.target\n\
             ConditionFileIsExecutable={condition}\n\
             \n\
             [Service]\n\
             Type=oneshot\n\
             {environment}\
             ExecStart={run}\n\
             \n\
             [Install]\n\
             WantedBy=graphical-session.target\n"
        ),
        Job::Watch => format!(
            "[Unit]\n\
             Description=Switch Claude Code before an account runs out, and warm up accounts\n\
             Documentation=https://docs.usepitboard.com\n\
             ConditionFileIsExecutable={condition}\n\
             \n\
             [Service]\n\
             {environment}\
             ExecStart={run}\n\
             Restart=on-failure\n\
             RestartSec={RESTART_AFTER_SECONDS}\n\
             \n\
             [Install]\n\
             WantedBy=default.target\n"
        ),
    }
}

/// The characters an assignment can hold with no quotes around it: none that systemd reads
/// as anything but itself.
fn plain(text: &str) -> bool {
    text.chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_.:/@,".contains(c))
}

/// One `Environment=` line, giving the service `name` with `value`, as systemd reads it back
/// whole. Read in systemd.exec(5) and systemd.syntax(7), at systemd's main branch on 7 October
/// 2026 (a28ccc9): each line is unquoted by systemd.syntax's rules, so an assignment holding
/// a space or an `=` is put in double quotes, inside which a backslash and a double quote are
/// written with a backslash before them; a `%` starts a specifier, so it is written `%%`; `$`
/// means nothing. A character that does not print, a line break among them, is written as
/// systemd.syntax's `\xNN` or `\uNNNN`, so it never ends the line; systemd then refuses that
/// one assignment, as it refuses every value holding one.
fn assignment(name: &str, value: &str) -> String {
    if plain(name) && plain(value) {
        return format!("Environment={name}={value}\n");
    }
    let mut quoted = String::new();
    for c in format!("{name}={value}").chars() {
        match c {
            '\\' | '"' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '%' => quoted.push_str("%%"),
            c if c.is_control() && c.is_ascii() => quoted.push_str(&format!("\\x{:02x}", c as u32)),
            c if c.is_control() => quoted.push_str(&format!("\\u{:04x}", c as u32)),
            c => quoted.push(c),
        }
    }
    format!("Environment=\"{quoted}\"\n")
}

/// The variables a unit [`unit_file`] wrote gives its service, in the order written: each
/// `Environment=` line read back as [`assignment`] wrote it.
fn given(body: &str) -> Vec<(String, String)> {
    body.lines()
        .filter_map(|line| {
            let written = line.strip_prefix("Environment=")?;
            let assigned = match written
                .strip_prefix('"')
                .and_then(|quoted| quoted.strip_suffix('"'))
            {
                Some(quoted) => unquoted(quoted),
                None => written.to_string(),
            };
            let (name, value) = assigned.split_once('=')?;
            Some((name.to_string(), value.to_string()))
        })
        .collect()
}

/// What [`assignment`] put between double quotes, read back: `\\`, `\"` and `%%` are each
/// the one character they stand for, and `\xNN` and `\uNNNN` the character of that number.
fn unquoted(quoted: &str) -> String {
    let mut text = String::with_capacity(quoted.len());
    let mut chars = quoted.chars();
    while let Some(c) = chars.next() {
        let escaped = match c {
            '\\' | '%' => chars.next().unwrap_or(c),
            c => {
                text.push(c);
                continue;
            }
        };
        let digits = match (c, escaped) {
            ('\\', 'x') => 2,
            ('\\', 'u') => 4,
            _ => {
                text.push(escaped);
                continue;
            }
        };
        let number: String = chars.by_ref().take(digits).collect();
        text.extend(
            u32::from_str_radix(&number, 16)
                .ok()
                .and_then(char::from_u32),
        );
    }
    text
}

/// Once each calendar day, read in systemd.timer(5) at tag `v262`. `OnCalendar=` counts the
/// wall clock, which goes on while the computer sleeps, and a run due then happens once it
/// wakes. `Persistent=` acts only with `OnCalendar=`: it keeps the last run on disk and starts
/// one missed while the computer was off as the timer starts. `RandomizedDelaySec=` spreads
/// each start, a catch-up's too, over the hour after it is due.
fn timer() -> String {
    "[Unit]\n\
     Description=Renew Pitboard's parked logins daily\n\
     \n\
     [Timer]\n\
     OnCalendar=daily\n\
     Persistent=true\n\
     RandomizedDelaySec=1h\n\
     \n\
     [Install]\n\
     WantedBy=timers.target\n"
        .to_string()
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    use crate::host::memory::MemoryHost;
    use crate::schedule::{install, installed_program};
    use std::os::unix::fs::PermissionsExt;

    // As sd_booted(3) tells: a container started without systemd as init has no user manager.
    #[test]
    fn only_a_system_systemd_started_offers_its_scheduler() {
        let root = tempfile::tempdir().expect("a scratch root");
        assert!(!started_by_systemd(root.path()));
        std::fs::create_dir_all(root.path().join("run/systemd")).expect("made");
        std::fs::write(root.path().join("run/systemd/system"), "").expect("a file");
        assert!(
            !started_by_systemd(root.path()),
            "a file is not the directory"
        );
        std::fs::remove_file(root.path().join("run/systemd/system")).expect("gone");
        std::fs::create_dir(root.path().join("run/systemd/system")).expect("made");
        assert!(started_by_systemd(root.path()));
    }

    /// The pairs a service is given, as `install` hands them over.
    fn pairs(given: &[(&str, &str)]) -> Vec<(String, String)> {
        given
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect()
    }

    // The unit for a program at `path`, which a unit can run.
    fn unit(job: Job, path: &str, options: &[String], environment: &[(String, String)]) -> String {
        let program = spelled(Path::new(path)).expect("a path a unit holds");
        unit_file(job, &program, options, environment)
    }

    /// The service runs with systemd's environment, not the shell's, so what it is given of
    /// the installing Pitboard's, `PITBOARD_NO_ARGV` and the proxy variables, is written in,
    /// one `Environment=` line each, quoted where systemd would otherwise split or change it,
    /// and nothing is written where there is nothing to give. What was written reads back as
    /// it was.
    #[test]
    fn the_unit_gives_its_service_the_variables_it_is_handed_and_reads_them_back() {
        let program = "/usr/local/bin/pitboard";
        let handed = pairs(&[
            ("PITBOARD_NO_ARGV", "1"),
            ("ALL_PROXY", "socks5h://alice:p%40ss\"w\\rd@127.0.0.1:1080"),
            ("HTTPS_PROXY", "http://proxy.example.com:3128"),
            ("no_proxy", ""),
            ("NO_PROXY", "*.anthropic.com, localhost"),
        ]);
        let written = unit(Job::Renew, program, &[], &handed);
        assert!(
            written.contains(
                "Type=oneshot\n\
                 Environment=PITBOARD_NO_ARGV=1\n\
                 Environment=\"ALL_PROXY=socks5h://alice:p%%40ss\\\"w\\\\rd@127.0.0.1:1080\"\n\
                 Environment=HTTPS_PROXY=http://proxy.example.com:3128\n\
                 Environment=no_proxy=\n\
                 Environment=\"NO_PROXY=*.anthropic.com, localhost\"\n\
                 ExecStart=/usr/local/bin/pitboard renew --scheduled\n"
            ),
            "{written}"
        );
        assert_eq!(given(&written), handed);
        assert!(!unit(Job::Renew, program, &[], &[]).contains("Environment="));
        assert_eq!(given(&unit(Job::Renew, program, &[], &[])), Vec::new());

        // A line break in a value never ends the line it is on.
        let broken = pairs(&[("NO_PROXY", "a\nExecStart=/bin/false\u{85}")]);
        let written = unit(Job::Renew, program, &[], &broken);
        assert!(
            written.contains("Environment=\"NO_PROXY=a\\x0aExecStart=/bin/false\\u0085\"\n"),
            "{written}"
        );
        assert_eq!(
            written
                .lines()
                .filter(|l| l.starts_with("ExecStart="))
                .count(),
            1,
            "{written}"
        );
        assert_eq!(given(&written), broken);
    }

    // From the person's first sign-in, and again a minute after it fails or crashes.
    #[test]
    fn the_watch_service_starts_with_the_users_systemd_and_reads_back_its_share() {
        let at = ["--at".to_string(), "90".to_string()];
        let written = unit(Job::Watch, "/usr/local/bin/pitboard", &at, &[]);
        assert!(
            written.contains(
                "[Service]\n\
                 ExecStart=/usr/local/bin/pitboard watch --scheduled --at 90\n\
                 Restart=on-failure\n\
                 RestartSec=60\n\
                 \n\
                 [Install]\n\
                 WantedBy=default.target\n"
            ),
            "{written}"
        );
        assert!(!written.contains("Type=oneshot"), "{written}");
        assert_eq!(
            command(&written, Job::Watch),
            Some((PathBuf::from("/usr/local/bin/pitboard"), at.to_vec()))
        );
        assert_eq!(command(&written, Job::Renew), None, "not renewal's");
    }

    /// The service runs one verb, with the marker a run of the schedule is told apart by,
    /// rather than by what systemd passes it. The timer counts calendar days, which go on
    /// while the computer sleeps, and starts a run missed while it was off: its
    /// `Persistent=true` did nothing beside `OnUnitActiveSec=`, which counts only time awake.
    #[test]
    fn the_timer_runs_on_each_calendar_day_and_catches_up_after_sleep() {
        let written = unit(Job::Renew, "/usr/local/bin/pitboard", &[], &[]);
        assert!(
            written.contains("ExecStart=/usr/local/bin/pitboard renew --scheduled\n"),
            "{written}"
        );
        assert!(!written.contains("status"));
        let timer = timer();
        assert!(
            timer.contains(
                "[Timer]\n\
                 OnCalendar=daily\n\
                 Persistent=true\n\
                 RandomizedDelaySec=1h\n\
                 \n\
                 [Install]\n\
                 WantedBy=timers.target\n"
            ),
            "{timer}"
        );
        for monotonic in ["OnUnitActiveSec", "OnStartupSec", "OnBootSec"] {
            assert!(!timer.contains(monotonic), "{timer}");
        }
    }

    /// GNOME's and Plasma's session targets start `graphical-session.target` at each sign-in
    /// (docs/facts/linux.md), so renewal's service, wanted by it, runs then too, once the
    /// session is up: a run the login keyring refused at boot runs again after signing in has
    /// unlocked it. Installing enables the service beside the timer, and taking renewal away
    /// disables both.
    #[test]
    fn the_renewal_service_also_runs_at_each_graphical_sign_in() {
        let written = unit(Job::Renew, "/usr/bin/pitboard", &[], &[]);
        assert!(
            written.contains(
                "Documentation=https://docs.usepitboard.com\n\
                 After=graphical-session.target\n"
            ),
            "{written}"
        );
        assert!(
            written.ends_with(
                "ExecStart=/usr/bin/pitboard renew --scheduled\n\
                 \n\
                 [Install]\n\
                 WantedBy=graphical-session.target\n"
            ),
            "{written}"
        );

        let (_home, ctx) = scratch_home();
        let asked = Arc::new(Asked::default());
        let systemd = Systemd::new(asked.clone(), Arc::new(Pretend));
        let program = Path::new("/usr/bin/pitboard");
        systemd
            .put(&ctx, Permit::for_a_test(), Job::Renew, program, &[], &[])
            .expect("installed");
        assert_eq!(
            asked.taken(),
            [
                "--user daemon-reload".to_string(),
                format!("--user enable {SERVICE}"),
                format!("--user enable --now {TIMER}"),
            ]
        );
        assert!(
            systemd
                .remove(&ctx, Permit::for_a_test(), Job::Renew)
                .expect("taken away")
        );
        assert_eq!(
            asked.taken(),
            [
                format!("--user disable --now {TIMER}"),
                format!("--user disable {SERVICE}"),
                format!("--user clean --what=state {TIMER}"),
                "--user daemon-reload".to_string(),
            ]
        );
    }

    /// A start systemd refused leaves neither unit enabled where none was: a link left in
    /// `timers.target.wants` or `graphical-session.target.wants` names a unit that is gone. Over
    /// a schedule that was there, the service's link is taken back before its units are put back
    /// and started again, since an earlier Pitboard's service has no `[Install]` to enable it by.
    #[test]
    fn a_refused_start_takes_back_both_enables() {
        let (_home, ctx) = scratch_home();
        let asked = Arc::new(Asked::default());
        let systemd = Systemd::new(asked.clone(), Arc::new(Pretend));
        let program = Path::new("/usr/bin/pitboard");
        let units = Systemd::units(&ctx);
        let reload = "--user daemon-reload".to_string();
        let enable = format!("--user enable {SERVICE}");
        let start = format!("--user enable --now {TIMER}");
        let disable = format!("--user disable {SERVICE}");
        let stop = format!("--user disable --now {TIMER}");

        for (refusing, asked_for) in [
            (
                SERVICE,
                vec![
                    reload.clone(),
                    enable.clone(),
                    stop.clone(),
                    disable.clone(),
                    reload.clone(),
                ],
            ),
            (
                TIMER,
                vec![
                    reload.clone(),
                    enable.clone(),
                    start.clone(),
                    stop.clone(),
                    disable.clone(),
                    reload.clone(),
                ],
            ),
        ] {
            asked.refuse(refusing);
            let refused = systemd
                .put(&ctx, Permit::for_a_test(), Job::Renew, program, &[], &[])
                .expect_err("refused");
            assert_eq!(refused.code(), "schedule_refused", "{refusing}");
            assert_eq!(asked.taken(), asked_for, "{refusing}");
            assert!(!systemd.installed(&ctx, Job::Renew), "{refusing}");
            assert!(!units.join(SERVICE).exists(), "{refusing}");
        }

        std::fs::create_dir_all(&units).expect("made");
        let earlier = (
            "[Service]\nType=oneshot\nExecStart=/usr/bin/pitboard renew --scheduled\n",
            "[Timer]\nOnUnitActiveSec=86400\nOnStartupSec=900\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n",
        );
        std::fs::write(units.join(SERVICE), earlier.0).expect("written");
        std::fs::write(units.join(TIMER), earlier.1).expect("written");
        asked.refuse(TIMER);
        systemd
            .put(&ctx, Permit::for_a_test(), Job::Renew, program, &[], &[])
            .expect_err("refused");
        assert_eq!(
            asked.taken(),
            [
                reload.clone(),
                enable.clone(),
                start.clone(),
                disable,
                reload,
                enable,
                start
            ]
        );
        assert_eq!(
            (
                std::fs::read_to_string(units.join(SERVICE)).expect("put back"),
                std::fs::read_to_string(units.join(TIMER)).expect("put back"),
            ),
            (earlier.0.to_string(), earlier.1.to_string())
        );
    }

    /// systemd skips a service whose program is gone rather than failing it each day or at
    /// each sign-in, as once the package that installed Pitboard is removed.
    #[test]
    fn a_service_whose_pitboard_is_gone_is_skipped() {
        for (job, description, ordered) in [
            (
                Job::Renew,
                "Renew Pitboard's parked logins",
                "After=graphical-session.target\n",
            ),
            (
                Job::Watch,
                "Switch Claude Code before an account runs out, and warm up accounts",
                "",
            ),
        ] {
            let written = unit(job, "/usr/bin/pitboard", &[], &[]);
            assert!(
                written.starts_with(&format!(
                    "[Unit]\n\
                     Description={description}\n\
                     Documentation=https://docs.usepitboard.com\n\
                     {ordered}\
                     ConditionFileIsExecutable=/usr/bin/pitboard\n\
                     \n\
                     [Service]\n"
                )),
                "{written}"
            );
        }
    }

    /// `ExecStart=` unquotes the program before it expands specifiers, and the condition only
    /// expands them, so each spells it its own way and both name the same file.
    #[test]
    fn a_program_path_with_a_space_or_a_percent_reads_back_as_written() {
        let path = "/home/x/100% sure/bin/pitboard";
        let at = ["--at".to_string(), "90".to_string()];
        for (job, options, run) in [
            (Job::Renew, &[][..], "renew --scheduled"),
            (Job::Watch, &at[..], "watch --scheduled --at 90"),
        ] {
            let written = unit(job, path, options, &[]);
            assert!(
                written.contains("ConditionFileIsExecutable=/home/x/100%% sure/bin/pitboard\n"),
                "{written}"
            );
            assert!(
                written.contains(&format!(
                    "ExecStart=\"/home/x/100%% sure/bin/pitboard\" {run}\n"
                )),
                "{written}"
            );
            assert_eq!(
                command(&written, job),
                Some((PathBuf::from(path), options.to_vec())),
                "{written}"
            );
        }
        let dollar = unit(Job::Renew, "/opt/$x/pitboard", &[], &[]);
        assert!(dollar.contains("ExecStart=/opt/$x/pitboard renew --scheduled\n"));
        assert_eq!(
            command(&dollar, Job::Renew).map(|(program, _)| program),
            Some(PathBuf::from("/opt/$x/pitboard"))
        );

        // Installed and read back from the disk, as `schedule status` and doctor read it.
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let program = home.join("100% sure").join("pitboard");
        std::fs::create_dir_all(program.parent().expect("its folder")).expect("made");
        std::fs::write(&program, "").expect("a program");
        let ctx = Context::new(home)
            .with_memory_stores(MemoryHost::new())
            .with_schedule_program(program.clone());
        install(&ctx, Permit::for_a_test()).expect("installed");
        assert_eq!(installed_program(&ctx), Some(program));
    }

    /// `config_parse_exec` refuses such a program, and the unit file parser strips a space at the
    /// end of the condition, so nothing is written.
    #[test]
    fn a_program_path_no_unit_can_run_is_refused_before_anything_is_written() {
        let (_home, ctx) = scratch_home();
        let asked = Arc::new(Asked::default());
        let systemd = Systemd::new(asked.clone(), Arc::new(Pretend));
        for (path, named) in [
            ("/opt/a\"b/pitboard", "a quote"),
            ("/opt/it's/pitboard", "a quote"),
            ("/opt/a\\b/pitboard", "a backslash"),
            ("/opt/a\nb/pitboard", "the character U+000A"),
            ("/opt/a\tb/pitboard", "the character U+0009"),
            ("/opt/pitboard ", "a space at its end"),
        ] {
            let refused = systemd
                .put(
                    &ctx,
                    Permit::for_a_test(),
                    Job::Renew,
                    Path::new(path),
                    &[],
                    &[],
                )
                .expect_err("refused");
            assert_eq!(refused.code(), "schedule_refused", "{path:?}");
            assert!(refused.to_string().contains(named), "{refused}");
        }
        assert!(!Systemd::units(&ctx).exists(), "nothing was written");
        assert_eq!(asked.taken(), Vec::<String>::new());
    }

    /// A unit Pitboard wrote before its job carried the marker still names the Pitboard it
    /// runs, and installing again writes the marker in. A test says the job by the unit's
    /// own name; nothing systemd passes is read.
    #[test]
    fn a_unit_written_without_the_marker_still_names_its_program_and_install_adds_it() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let program = home.join("bin/pitboard");
        std::fs::create_dir_all(program.parent().expect("its directory")).expect("made");
        std::fs::write(&program, "").expect("a program");
        let ctx = Context::new(home.clone())
            .with_memory_stores(MemoryHost::new())
            .with_schedule_program(program.clone());
        let units = Systemd::units(&ctx);
        std::fs::create_dir_all(&units).expect("made");
        std::fs::write(units.join(TIMER), timer()).expect("written");
        std::fs::write(
            units.join(SERVICE),
            format!(
                "[Service]\nType=oneshot\nExecStart={} renew\n",
                program.display()
            ),
        )
        .expect("written");
        assert_eq!(installed_program(&ctx), Some(program.clone()), "as before");

        install(&ctx, Permit::for_a_test()).expect("installed again");
        let written = std::fs::read_to_string(units.join(SERVICE)).expect("the service");
        assert!(
            written.contains(&format!(
                "ExecStart={} renew --scheduled\n",
                program.display()
            )),
            "{written}"
        );
        assert_eq!(installed_program(&ctx), Some(program));

        let systemd = Systemd::new(
            Arc::new(service::Pretend {
                refuse_start: Arc::default(),
            }),
            Arc::new(Pretend),
        );
        assert!(systemd.started_this_process(Job::Renew, Some(SERVICE)));
        assert!(!systemd.started_this_process(Job::Renew, Some(TIMER)));
        assert!(
            !systemd.started_this_process(Job::Renew, None),
            "what systemd passes is not read"
        );
    }

    // systemd as a test asks it: each call written down, and the next one carrying the word a
    // test names refused.
    #[derive(Debug, Default)]
    struct Asked {
        refusing: std::sync::Mutex<Option<&'static str>>,
        calls: std::sync::Mutex<Vec<String>>,
    }

    impl Asked {
        fn refuse(&self, word: &'static str) {
            *self
                .refusing
                .lock()
                .expect("a poisoned test is a failed test") = Some(word);
        }

        fn taken(&self) -> Vec<String> {
            std::mem::take(&mut self.calls.lock().expect("a poisoned test is a failed test"))
        }
    }

    impl Control for Asked {
        fn run(&self, _: Permit, _program: &str, args: &[&str]) -> Result<()> {
            let call = args.join(" ");
            self.calls
                .lock()
                .expect("a poisoned test is a failed test")
                .push(call.clone());
            let mut refusing = self
                .refusing
                .lock()
                .expect("a poisoned test is a failed test");
            if refusing.is_some_and(|word| args.contains(&word)) {
                *refusing = None;
                return Err(Error::ScheduleRefused {
                    detail: format!("systemctl refused {call} in a test"),
                });
            }
            Ok(())
        }
    }

    // The user's systemd as a test says it is.
    #[derive(Debug)]
    struct Says(Reads);

    impl Manager for Says {
        fn reads(&self, _ctx: &Context) -> Result<Answer> {
            Ok(Answer::Reads(self.0.clone()))
        }
    }

    fn says(unit_path: &[&Path], home: Option<&Path>) -> Arc<Says> {
        Arc::new(Says(Reads {
            unit_path: unit_path
                .iter()
                .map(|folder| folder.to_path_buf())
                .collect(),
            home: home.map(Path::to_path_buf),
        }))
    }

    fn scratch_home() -> (tempfile::TempDir, Context) {
        let home = tempfile::tempdir().expect("a scratch home");
        let ctx = Context::new(home.path().to_path_buf()).with_memory_stores(MemoryHost::new());
        (home, ctx)
    }

    // A context read as the command line reads one, with this home and `XDG_CONFIG_HOME`.
    fn configured_in(home: &Path, config: &Path) -> Context {
        let named = |path: &Path| path.to_string_lossy().into_owned();
        let env: crate::context::Environment =
            [("HOME", named(home)), ("XDG_CONFIG_HOME", named(config))]
                .into_iter()
                .collect();
        Context::for_command_line(&env).with_memory_stores(MemoryHost::new())
    }

    /// The manager `systemctl --user` reaches is the user's whatever this shell's
    /// `XDG_CONFIG_HOME` says, so a unit in a folder it does not read would never run, or a
    /// branch build's would run beside the real ones. Nothing is written or asked of
    /// `systemctl` then.
    #[test]
    fn a_unit_is_written_only_where_the_user_manager_reads_its_folder() {
        let (home, ctx) = scratch_home();
        let asked = Arc::new(Asked::default());
        let program = Path::new("/usr/local/bin/pitboard");
        let units = Systemd::units(&ctx);
        let elsewhere = home.path().join("elsewhere/systemd/user");
        let systemd = Systemd::new(asked.clone(), says(&[&elsewhere], Some(home.path())));
        for job in [Job::Renew, Job::Watch] {
            let refused = systemd
                .put(&ctx, Permit::for_a_test(), job, program, &[], &[])
                .expect_err("refused");
            assert_eq!(refused.code(), "schedule_manager_elsewhere");
            assert!(
                refused.to_string().contains(&units.display().to_string()),
                "{refused}"
            );
        }
        assert!(!units.exists(), "nothing was written");
        assert_eq!(asked.taken(), Vec::<String>::new(), "nor asked");

        let systemd = Systemd::new(
            asked.clone(),
            says(&[&elsewhere, &units], Some(home.path())),
        );
        systemd
            .put(&ctx, Permit::for_a_test(), Job::Renew, program, &[], &[])
            .expect("installed where it is read");
        assert!(systemd.installed(&ctx, Job::Renew));
    }

    /// The manager runs a unit with its own `HOME`, where renewal and the watch service then
    /// act, so one that runs another home's, or none, is given nothing, and nothing of this
    /// home's is taken away through it.
    #[test]
    fn a_unit_is_written_only_for_the_home_the_user_manager_runs_with() {
        let (home, ctx) = scratch_home();
        let asked = Arc::new(Asked::default());
        let program = Path::new("/usr/local/bin/pitboard");
        let units = Systemd::units(&ctx);
        let other = home.path().join("other");
        for theirs in [Some(other.as_path()), None] {
            let systemd = Systemd::new(asked.clone(), says(&[&units], theirs));
            for job in [Job::Renew, Job::Watch] {
                let refused = systemd
                    .put(&ctx, Permit::for_a_test(), job, program, &[], &[])
                    .expect_err("refused");
                assert_eq!(refused.code(), "schedule_manager_elsewhere");
                let said = refused.to_string();
                assert!(said.contains(&home.path().display().to_string()), "{said}");
                if let Some(theirs) = theirs {
                    assert!(said.contains(&theirs.display().to_string()), "{said}");
                }
            }
        }
        assert!(!units.exists(), "nothing was written");
        assert_eq!(asked.taken(), Vec::<String>::new());

        let ours = Systemd::new(asked.clone(), says(&[&units], Some(home.path())));
        ours.put(&ctx, Permit::for_a_test(), Job::Watch, program, &[], &[])
            .expect("installed for this home");
        asked.taken();
        let theirs = Systemd::new(asked.clone(), says(&[&units], Some(&other)));
        let refused = theirs
            .remove(&ctx, Permit::for_a_test(), Job::Watch)
            .expect_err("another home's manager");
        assert_eq!(refused.code(), "schedule_manager_elsewhere");
        assert!(units.join(WATCH).is_file(), "still there");
        assert_eq!(asked.taken(), Vec::<String>::new());
    }

    /// A manager that does not read the folder never loaded what is in it, and `systemctl`
    /// would reach that manager: the files and the links `enable` made go, and nothing else.
    #[test]
    fn nothing_is_asked_of_systemctl_where_the_manager_does_not_read_the_units() {
        let (home, ctx) = scratch_home();
        let asked = Arc::new(Asked::default());
        let program = Path::new("/usr/local/bin/pitboard");
        let units = Systemd::units(&ctx);
        let reading = Systemd::new(asked.clone(), says(&[&units], Some(home.path())));
        let not_reading = Systemd::new(asked.clone(), says(&[], Some(home.path())));
        for job in [Job::Renew, Job::Watch] {
            reading
                .put(&ctx, Permit::for_a_test(), job, program, &[], &[])
                .expect("installed");
            let links = enabled(&units, job);
            asked.taken();

            assert!(
                not_reading
                    .remove(&ctx, Permit::for_a_test(), job)
                    .expect("taken away")
            );
            assert_eq!(asked.taken(), Vec::<String>::new(), "{job:?}");
            assert!(!reading.installed(&ctx, job));
            for gone in [Systemd::service(job), Systemd::installed_by(job)] {
                assert!(!units.join(gone).exists(), "{gone}");
            }
            for link in &links {
                assert!(
                    std::fs::symlink_metadata(link).is_err(),
                    "{}",
                    link.display()
                );
            }
        }

        // Where it reads them, it is asked to stop each, and to forget when renewal last ran.
        for job in [Job::Renew, Job::Watch] {
            reading
                .put(&ctx, Permit::for_a_test(), job, program, &[], &[])
                .expect("installed");
        }
        asked.taken();
        for job in [Job::Renew, Job::Watch] {
            assert!(
                reading
                    .remove(&ctx, Permit::for_a_test(), job)
                    .expect("taken away")
            );
        }
        assert_eq!(
            asked.taken(),
            [
                format!("--user disable --now {TIMER}"),
                format!("--user disable {SERVICE}"),
                format!("--user clean --what=state {TIMER}"),
                "--user daemon-reload".to_string(),
                format!("--user disable --now {WATCH}"),
                "--user daemon-reload".to_string(),
            ]
        );
    }

    // The links `systemctl enable` makes for `job` in `units`, made as it makes them.
    fn enabled(units: &Path, job: Job) -> Vec<PathBuf> {
        Systemd::links(job)
            .iter()
            .map(|(wants, unit)| {
                let link = units.join(wants).join(unit);
                std::fs::create_dir_all(link.parent().expect("its folder")).expect("made");
                crate::host::fs::testing::link(&units.join(unit), &link)
                    .expect("enabled, as systemctl links it");
                link
            })
            .collect()
    }

    // The user's systemd where nothing from here reaches it.
    #[derive(Debug)]
    struct Unreached;

    impl Manager for Unreached {
        fn reads(&self, _ctx: &Context) -> Result<Answer> {
            Ok(Answer::Unreached(
                "neither XDG_RUNTIME_DIR nor DBUS_SESSION_BUS_ADDRESS is set".into(),
            ))
        }
    }

    /// Where nothing reaches the user's systemd, as in a shell `su` started, `systemctl` is
    /// asked as it was before Pitboard asked the manager: renewal's units go whatever it says,
    /// with the links `enable` made, and the watch service's only once it is stopped, since
    /// one left running writes the directory `pitboard uninstall` removes next. Nothing is
    /// installed there.
    #[test]
    fn a_job_is_taken_away_where_nothing_reaches_the_user_manager() {
        let (home, ctx) = scratch_home();
        let asked = Arc::new(Asked::default());
        let program = Path::new("/usr/local/bin/pitboard");
        let units = Systemd::units(&ctx);
        let reading = Systemd::new(asked.clone(), says(&[&units], Some(home.path())));
        let unreached = Systemd::new(asked.clone(), Arc::new(Unreached));
        let mut links = Vec::new();
        for job in [Job::Renew, Job::Watch] {
            reading
                .put(&ctx, Permit::for_a_test(), job, program, &[], &[])
                .expect("installed");
            links.push(enabled(&units, job));
        }
        asked.taken();
        let gone = |job: Job, links: &[PathBuf]| {
            assert!(!reading.installed(&ctx, job), "{job:?}");
            for gone in [Systemd::service(job), Systemd::installed_by(job)] {
                assert!(!units.join(gone).exists(), "{gone}");
            }
            for link in links {
                assert!(
                    std::fs::symlink_metadata(link).is_err(),
                    "{}",
                    link.display()
                );
            }
        };

        // `systemctl` reaches nothing either.
        asked.refuse("disable");
        assert!(
            unreached
                .remove(&ctx, Permit::for_a_test(), Job::Renew)
                .expect("taken away")
        );
        assert_eq!(
            asked.taken(),
            [
                format!("--user disable --now {TIMER}"),
                format!("--user disable {SERVICE}"),
                format!("--user clean --what=state {TIMER}"),
                "--user daemon-reload".to_string(),
            ]
        );
        gone(Job::Renew, &links[0]);

        asked.refuse("disable");
        let refused = unreached
            .remove(&ctx, Permit::for_a_test(), Job::Watch)
            .expect_err("it may still be running");
        assert_eq!(refused.code(), "schedule_refused");
        assert!(units.join(WATCH).is_file(), "still there");
        asked.taken();

        assert!(
            unreached
                .remove(&ctx, Permit::for_a_test(), Job::Watch)
                .expect("stopped, then taken away")
        );
        assert_eq!(
            asked.taken(),
            [
                format!("--user disable --now {WATCH}"),
                "--user daemon-reload".to_string(),
            ]
        );
        gone(Job::Watch, &links[1]);

        let refused = unreached
            .put(&ctx, Permit::for_a_test(), Job::Renew, program, &[], &[])
            .expect_err("nothing to tell where it would run");
        assert_eq!(refused.code(), "schedule_refused");
        assert!(
            refused.to_string().contains("could not be asked"),
            "{refused}"
        );
        assert!(!units.join(TIMER).exists());
        assert_eq!(asked.taken(), Vec::<String>::new());
    }

    /// Where `XDG_CONFIG_HOME` names a full path, the user's systemd reads units from
    /// `systemd/user` there, not in `~/.config` (systemd.unit(5)).
    #[test]
    fn the_units_go_where_xdg_config_home_says() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let (home, config) = (scratch.path().join("home"), scratch.path().join("config"));
        let program = home.join("bin/pitboard");
        std::fs::create_dir_all(program.parent().expect("its folder")).expect("made");
        std::fs::write(&program, "").expect("a program");
        let ctx = configured_in(&home, &config).with_schedule_program(program.clone());
        let units = config.join("systemd/user");

        let location = install(&ctx, Permit::for_a_test()).expect("installed");
        assert_eq!(location, Location::File(units.join(TIMER)));
        for file in [TIMER, SERVICE] {
            assert!(units.join(file).is_file(), "{file}");
        }
        assert!(!home.join(".config").exists(), "nothing in ~/.config");
        assert_eq!(installed_program(&ctx), Some(program));
    }

    /// A Pitboard before this one wrote its units in `~/.config/systemd/user` whatever
    /// `XDG_CONFIG_HOME` said, so they are looked for there too: `status` and doctor find
    /// them, and taking the job away takes them, through `systemctl` only where the manager
    /// reads that folder.
    #[test]
    fn a_unit_an_earlier_pitboard_wrote_in_dot_config_is_still_found_and_taken_away() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let (home, config) = (scratch.path().join("home"), scratch.path().join("config"));
        let ctx = configured_in(&home, &config);
        let before = home.join(".config/systemd/user");
        let program = "/usr/local/bin/pitboard";
        let earlier = || {
            std::fs::create_dir_all(before.join("timers.target.wants")).expect("made");
            std::fs::write(
                before.join(TIMER),
                "[Timer]\nOnUnitActiveSec=86400\nOnStartupSec=900\nPersistent=true\n",
            )
            .expect("written");
            std::fs::write(
                before.join(SERVICE),
                format!("[Service]\nType=oneshot\nExecStart={program} renew --scheduled\n"),
            )
            .expect("written");
            crate::host::fs::testing::link(
                &before.join(TIMER),
                &before.join("timers.target.wants").join(TIMER),
            )
            .expect("enabled");
        };
        let asked = Arc::new(Asked::default());

        earlier();
        let systemd = Systemd::new(
            asked.clone(),
            says(&[&config.join("systemd/user")], Some(&home)),
        );
        assert!(systemd.installed(&ctx, Job::Renew));
        assert_eq!(
            systemd.location(&ctx, Job::Renew),
            Location::File(before.join(TIMER))
        );
        assert_eq!(
            systemd.program(&ctx, Job::Renew),
            Some(PathBuf::from(program))
        );
        assert!(systemd.outdated(&ctx, Job::Renew));
        assert!(
            systemd
                .remove(&ctx, Permit::for_a_test(), Job::Renew)
                .expect("taken away")
        );
        assert_eq!(asked.taken(), Vec::<String>::new(), "never loaded");
        assert!(!systemd.installed(&ctx, Job::Renew));
        for gone in [TIMER, SERVICE, "timers.target.wants/pitboard-renew.timer"] {
            assert!(
                std::fs::symlink_metadata(before.join(gone)).is_err(),
                "{gone}"
            );
        }

        // A manager without that XDG_CONFIG_HOME reads it, and is asked to stop it.
        earlier();
        let systemd = Systemd::new(asked.clone(), says(&[&before], Some(&home)));
        assert!(
            systemd
                .remove(&ctx, Permit::for_a_test(), Job::Renew)
                .expect("taken away")
        );
        assert_eq!(
            asked.taken(),
            [
                format!("--user disable --now {TIMER}"),
                format!("--user disable {SERVICE}"),
                format!("--user clean --what=state {TIMER}"),
                "--user daemon-reload".to_string(),
            ]
        );
        assert!(!before.join(TIMER).exists());
    }

    // The user's systemd as a test plays it: the two properties, on the object and interface
    // org.freedesktop.systemd1(5) names.
    struct Plays {
        unit_path: Vec<String>,
        environment: Vec<String>,
    }

    #[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
    impl Plays {
        #[zbus(property)]
        fn unit_path(&self) -> Vec<String> {
            self.unit_path.clone()
        }

        #[zbus(property)]
        fn environment(&self) -> Vec<String> {
            self.environment.clone()
        }
    }

    /// The private socket in the runtime folder is reached peer to peer, as systemd serves it,
    /// and `HOME` is read out of the manager's `Environment`. Served here by a stand-in in the
    /// test's own folder: no build for tests asks the user's own systemd.
    #[test]
    fn the_user_manager_is_asked_on_its_private_socket_for_its_folders_and_home() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let runtime = scratch.path().join("runtime");
        std::fs::create_dir_all(runtime.join("systemd")).expect("made");
        let listener = std::os::unix::net::UnixListener::bind(runtime.join("systemd/private"))
            .expect("listening");
        let folders = ["/home/x/.config/systemd/user", "/usr/lib/systemd/user"];
        let plays = Plays {
            unit_path: folders.map(String::from).to_vec(),
            environment: ["LANG=C.UTF-8", "HOME=/home/x", "PATH=/usr/bin"]
                .map(String::from)
                .to_vec(),
        };
        let serving = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("asked");
            let connection = zbus::blocking::connection::Builder::async_io_unix_stream(stream)
                .server(zbus::Guid::generate())
                .and_then(|builder| builder.p2p().serve_at("/org/freedesktop/systemd1", plays))
                .and_then(zbus::blocking::connection::Builder::build)
                .expect("a peer to peer connection");
            connection.closed();
        });
        let runtime = runtime.to_string_lossy().into_owned();
        let env = crate::context::Environment::on(
            crate::host::Os::Linux,
            [("HOME", "/home/x"), ("XDG_RUNTIME_DIR", runtime.as_str())],
        );
        let reads = asked(&Context::read(&env, "cli")).expect("answered");
        assert_eq!(
            reads,
            Reads {
                unit_path: folders.map(PathBuf::from).to_vec(),
                home: Some(PathBuf::from("/home/x")),
            }
        );
        serving
            .join()
            .expect("served until the question was answered");
    }

    /// The system's own manager, asked from a build for tests, refuses before it reaches a bus,
    /// whether or not systemd started this system, and nothing is written.
    #[test]
    fn the_systems_user_manager_is_never_asked_from_a_build_for_tests() {
        let (_home, ctx) = scratch_home();
        let refused = UserManager.reads(&ctx).expect_err("refused");
        assert_eq!(refused.code(), "schedule_refused");
        let systemd = Systemd::new(service::system(), Arc::new(UserManager));
        for job in [Job::Renew, Job::Watch] {
            let refused = systemd
                .put(
                    &ctx,
                    Permit::for_a_test(),
                    job,
                    Path::new("/usr/local/bin/pitboard"),
                    &[],
                    &[],
                )
                .expect_err("refused");
            assert!(refused.to_string().contains("user manager"), "{refused}");
        }
        assert!(!Systemd::units(&ctx).exists());
    }

    // Enabled but not running, a new service would start at the next sign-in all the same; one
    // before is restarted as it was.
    #[test]
    fn a_watch_service_systemd_will_not_restart_is_taken_away_or_put_back() {
        let (_home, ctx) = scratch_home();
        let asked = Arc::new(Asked::default());
        let systemd = Systemd::new(asked.clone(), Arc::new(Pretend));
        let program = Path::new("/usr/local/bin/pitboard");
        let at = |percent: &str| vec!["--at".to_string(), percent.to_string()];
        let reload = "--user daemon-reload".to_string();
        let enable = format!("--user enable {WATCH}");
        let restart = format!("--user restart {WATCH}");

        asked.refuse("restart");
        let refused = systemd
            .put(
                &ctx,
                Permit::for_a_test(),
                Job::Watch,
                program,
                &at("90"),
                &[],
            )
            .expect_err("refused");
        assert_eq!(refused.code(), "schedule_refused");
        assert!(!systemd.installed(&ctx, Job::Watch));
        assert_eq!(
            asked.taken(),
            [
                reload.clone(),
                enable.clone(),
                restart.clone(),
                format!("--user disable {WATCH}"),
                reload.clone()
            ]
        );

        systemd
            .put(
                &ctx,
                Permit::for_a_test(),
                Job::Watch,
                program,
                &at("90"),
                &[],
            )
            .expect("installed");
        asked.taken();
        asked.refuse("restart");
        systemd
            .put(
                &ctx,
                Permit::for_a_test(),
                Job::Watch,
                program,
                &at("70"),
                &[],
            )
            .expect_err("refused");
        assert_eq!(systemd.options(&ctx, Job::Watch), at("90"));
        assert_eq!(
            asked.taken(),
            [
                reload.clone(),
                enable.clone(),
                restart.clone(),
                reload,
                enable,
                restart
            ]
        );
    }

    // A watch service left running would write the Pitboard directory again once `pitboard
    // uninstall` removed it; renewal's units go whatever systemd says.
    #[test]
    fn a_watch_service_systemd_will_not_stop_stays_installed() {
        let (_home, ctx) = scratch_home();
        let asked = Arc::new(Asked::default());
        let systemd = Systemd::new(asked.clone(), Arc::new(Pretend));
        let program = Path::new("/usr/local/bin/pitboard");
        for job in [Job::Renew, Job::Watch] {
            systemd
                .put(&ctx, Permit::for_a_test(), job, program, &[], &[])
                .expect("installed");
        }

        asked.refuse("disable");
        let refused = systemd
            .remove(&ctx, Permit::for_a_test(), Job::Watch)
            .expect_err("still running");
        assert_eq!(refused.code(), "schedule_refused");
        assert!(systemd.installed(&ctx, Job::Watch));
        assert!(Systemd::units(&ctx).join(WATCH).is_file());

        asked.refuse("disable");
        assert!(
            systemd
                .remove(&ctx, Permit::for_a_test(), Job::Renew)
                .expect("taken away")
        );
        assert!(!systemd.installed(&ctx, Job::Renew));
        assert!(!Systemd::units(&ctx).join(SERVICE).exists());
    }

    /// `pitboard schedule install` writes the proxy variables it was started with into the
    /// service, and the service and its timer only the person can read, mode 600, since a
    /// proxy's address can hold a password: over a service that was open to others too,
    /// which kept its mode when Pitboard wrote it before.
    #[test]
    fn an_installed_unit_carries_the_proxy_and_only_its_owner_can_read_it() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let program = home.join("bin/pitboard");
        std::fs::create_dir_all(program.parent().expect("its directory")).expect("made");
        std::fs::write(&program, "").expect("a program");
        let home_text = home.to_string_lossy().into_owned();
        let env: crate::context::Environment = [
            ("HOME", home_text.as_str()),
            ("ALL_PROXY", "socks5h://alice:s3cret@127.0.0.1:1080"),
            ("no_proxy", "localhost"),
        ]
        .into_iter()
        .collect();
        let ctx = Context::for_command_line(&env)
            .with_memory_stores(MemoryHost::new())
            .with_schedule_program(program.clone());
        let units = Systemd::units(&ctx);
        std::fs::create_dir_all(&units).expect("made");
        std::fs::write(units.join(SERVICE), "a service open to others\n").expect("written");
        std::fs::set_permissions(units.join(SERVICE), std::fs::Permissions::from_mode(0o644))
            .expect("opened to others");

        install(&ctx, Permit::for_a_test()).expect("installed");
        let written = std::fs::read_to_string(units.join(SERVICE)).expect("the service");
        assert!(
            written.contains(
                "Environment=ALL_PROXY=socks5h://alice:s3cret@127.0.0.1:1080\n\
                 Environment=no_proxy=localhost\n"
            ),
            "{written}"
        );
        for file in [SERVICE, TIMER] {
            assert_eq!(
                crate::host::fs::access(&units.join(file)).map(|access| access.described),
                Some("mode 600".to_string()),
                "{file}"
            );
        }
        assert_eq!(installed_program(&ctx), Some(program));
    }
}
