//! Keeping parked logins alive without anybody running a command.
//!
//! A refresh token has a finite life, and the only two things that renew one are somebody
//! typing `pitboard` and the menu bar app's poll. So the tool is safe for a macOS user who
//! installed the app and leaves it running, and quietly unsafe for everyone else: the
//! whole of Linux, and any macOS user on the command line alone. Go away for the refresh
//! window, come back, and every parked login is dead and each account needs a browser
//! sign-in, which is precisely the cost Pitboard exists to spare people.
//!
//! This is opt-in, and stays opt-in. A background process that talks to Anthropic on a
//! schedule is the shape most likely to be read as automation, so it is something a person
//! turns on knowing what it is, and what it does is written where they can read it: it
//! renews the owner's own parked logins and nothing else. It never switches, never asks for
//! usage, never warms an account up, and makes no request other than the token exchange. A
//! warm-up, which has Claude Code send a request, runs only where somebody leaves the app or
//! `pitboard watch` running.
//!
//! What it installs is the system's own scheduler, never a homemade daemon: a LaunchAgent
//! on macOS, which runs inside the login session so the keychain is unlocked, a systemd
//! user timer on Linux, which is that system's answer, and a task in Task Scheduler on
//! Windows, which runs only while the person is signed in. How each is asked is
//! [`crate::host::Scheduler`]'s; what is decided here is which Pitboard it runs, how often,
//! and whether it is this home's to change. Where there is no scheduler, it says so rather
//! than inventing one.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::host::{Os, Scheduler};
use crate::permit::Permit;
use std::path::{Path, PathBuf};

/// Once a day. A refresh token's life is measured in weeks and Pitboard starts renewing
/// three days out, so a daily check has three chances to catch each one, and a machine
/// that was asleep for one of them still has two.
pub(crate) const EVERY_SECONDS: u32 = 86_400;
// How long launchd and systemd wait to start a watch service that failed again. systemd's
// default of 100 ms would meet its start limit, five starts in 10 seconds, and give up.
#[cfg_attr(
    windows,
    allow(dead_code, reason = "the watch service's task asks for no restart")
)]
pub(crate) const RESTART_AFTER_SECONDS: u32 = 60;

/// A job Pitboard hands the system's own scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    /// `pitboard renew`, daily.
    Renew,
    /// `pitboard watch`, from sign-in on ([`crate::watch_service`]).
    Watch,
}

impl Job {
    /// What the job runs Pitboard with, marked as its run ([`SCHEDULED_RUN`], [`WATCH_RUN`]).
    pub(crate) fn marked(self) -> [&'static str; 2] {
        match self {
            Job::Renew => SCHEDULED_RUN,
            Job::Watch => WATCH_RUN,
        }
    }
}

/// Whether the schedule is installed, and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    /// Installed, where `location` says.
    Yes {
        location: Location,
        every_seconds: u32,
    },
    No,
    /// This machine has no scheduler Pitboard knows how to ask.
    Unsupported,
}

/// Why the user's systemd on Linux would not run a job for this home
/// ([`Error::ScheduleManagerElsewhere`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Elsewhere {
    /// It reads no units from `units`, the folder the job is written in.
    Folder { units: PathBuf },
    /// It runs units with `theirs`, or with no `HOME`, rather than with `home`, where the job
    /// would then act.
    Home {
        home: PathBuf,
        theirs: Option<PathBuf>,
    },
}

/// Where a person finds the schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    /// The file launchd or systemd reads.
    File(PathBuf),
    /// Task Scheduler's path for the task, such as `\Pitboard\Renew S-1-5-21-…`.
    Task(String),
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Location::File(path) => write!(f, "{}", path.display()),
            Location::Task(task) => f.write_str(task),
        }
    }
}

pub(crate) fn scheduler(ctx: &Context) -> Option<&dyn Scheduler> {
    ctx.host().scheduler()
}

// The file a test's machine keeps the job in: Task Scheduler's own as a pretend keeps it.
#[cfg(test)]
pub(crate) fn path(ctx: &Context) -> Option<PathBuf> {
    job_path(ctx, Job::Renew)
}

#[cfg(test)]
pub(crate) fn job_path(ctx: &Context, job: Job) -> Option<PathBuf> {
    Some(match scheduler(ctx)?.location(ctx, job) {
        Location::File(path) => path,
        Location::Task(task) => crate::host::task::pretend_file(ctx, &task),
    })
}

pub(crate) fn status(ctx: &Context) -> Installed {
    match scheduler(ctx) {
        None => Installed::Unsupported,
        Some(s) if s.installed(ctx, Job::Renew) => Installed::Yes {
            location: s.location(ctx, Job::Renew),
            every_seconds: EVERY_SECONDS,
        },
        Some(_) => Installed::No,
    }
}

/// The arguments the schedule's job runs Pitboard with where what the system's scheduler
/// says of the job it started is not to be relied on: `pitboard renew --scheduled`. systemd
/// passes a service a timer started `$TRIGGER_UNIT` only from version 252, and
/// systemd.exec(5) calls it best effort, lossy and not to be relied on; a service started
/// by hand gets none. So the unit's `ExecStart` carries the marker, and the command line,
/// started with it, says so through [`Context::started_by_the_schedule`]. launchd names its
/// job in `XPC_SERVICE_NAME`, so the LaunchAgent runs plain `renew`.
pub const SCHEDULED_RUN: [&str; 2] = ["renew", "--scheduled"];

/// What the watch service's job runs Pitboard with on every system, before its options: the
/// marker [`Context::started_by_the_watch_service`] follows.
pub const WATCH_RUN: [&str; 2] = ["watch", "--scheduled"];

// Each job serves the default home alone, whatever `PITBOARD_HOME` it is given.
pub(crate) fn serves(ctx: &Context) -> bool {
    ctx.os
        .same_path(&crate::home::dir(ctx), &ctx.default_pitboard_home())
}

/// Whether this process is a run `job` itself started: as the system's scheduler says,
/// where it says, or as the marker its job is started with says ([`Job::marked`]).
pub(crate) fn started_this_run(ctx: &Context, job: Job) -> bool {
    ctx.started_by() == Some(job)
        || scheduler(ctx).is_some_and(|s| s.started_this_process(job, ctx.scheduled_job()))
}

/// The context a run of `job` runs in: where the job started this run, the default home's,
/// whatever `PITBOARD_HOME` says, and otherwise `ctx` as it is.
///
/// The schedule belongs to the default home alone ([`serves`]), whichever home installed
/// it. Its job is never written with `PITBOARD_HOME`, but the system can still hand it one:
/// `launchctl setenv` gives a variable to every job launchd starts, and a systemd user
/// environment, from `systemctl --user set-environment` or `environment.d`, to every unit.
/// A run that followed it renewed another home's parked logins and left the default home's
/// to run out.
///
/// In a build for tests, a run the schedule started whose home is this account's own is
/// stopped here, before it reads anything: it would renew the real `~/.pitboard`, and a
/// test gives a run of the schedule a home of its own.
pub(crate) fn for_its_run(ctx: &Context, job: Job) -> Context {
    if !started_this_run(ctx, job) {
        return ctx.clone();
    }
    #[cfg(any(test, feature = "test-support"))]
    assert!(
        !crate::host::user::is_the_accounts_own_home(ctx.home()),
        "a build for tests ran as the system's job {job:?} with this account's own home, \
         where it would change the real Pitboard directory: give it a home of its own"
    );
    let run = ctx.clone().with_pitboard_home(ctx.default_pitboard_home());
    match scheduler(&run).map(|s| s.read_by_its_runs(&run, job)) {
        Some(given) if !given.is_empty() => run.with_proxy_variables_over(&given),
        _ => run,
    }
}

/// The Pitboard `job` should run: the one the context names, or this one, by the path it was
/// started by.
pub(crate) fn program(ctx: &Context, job: Job) -> Result<PathBuf> {
    program_of(ctx.schedule_program(), crate::host::current_program, job)
}

fn program_of(
    named: Option<&Path>,
    running: impl FnOnce() -> std::io::Result<PathBuf>,
    job: Job,
) -> Result<PathBuf> {
    let program = match named {
        Some(named) => named.to_path_buf(),
        None => running().map_err(|source| Error::HomeUnwritable {
            path: PathBuf::from("the running Pitboard"),
            source,
        })?,
    };
    lasting(&program, job)?;
    Ok(program)
}

/// `program`, where it will still be there when the scheduler runs it. A scheduler starts a
/// program that is gone without telling anyone, every day, so a path that leads nowhere is
/// refused rather than written down.
///
/// A path inside the copy macOS makes of an app opened where it was downloaded leads
/// somewhere while that app runs and nowhere once it quits, so it is refused as well.
fn lasting(program: &Path, job: Job) -> Result<&Path> {
    if in_a_temporary_copy(program) {
        return Err(Error::ScheduleProgramTemporary {
            path: program.to_path_buf(),
            job,
        });
    }
    if !program.is_file() {
        return Err(Error::ScheduleProgramMissing {
            path: program.to_path_buf(),
            job,
        });
    }
    Ok(program)
}

/// Whether `program` is inside a temporary copy, which is there while it runs and gone once it
/// quits ([`TemporaryCopy`]). Neither a schedule nor a link can rely on one, so an app asks this
/// of the command line inside it, by the rule a schedule is refused by.
pub fn in_a_temporary_copy(program: &Path) -> bool {
    temporary_copy(program).is_some()
}

/// A copy a program runs from that is gone once it quits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TemporaryCopy {
    /// The one macOS makes of an app opened where it was downloaded, through a directory named
    /// `AppTranslocation`.
    Translocated,
    /// The one Windows' Explorer makes of a program opened inside a zip.
    Zip,
    /// The folder an AppImage's runtime mounts it in, or extracts it into to run it.
    AppImage,
}

pub(crate) fn temporary_copy(program: &Path) -> Option<TemporaryCopy> {
    if program
        .components()
        .any(|part| part.as_os_str() == "AppTranslocation")
    {
        Some(TemporaryCopy::Translocated)
    } else if opened_from_a_zip(program) {
        Some(TemporaryCopy::Zip)
    } else if program
        .components()
        .any(|part| an_appimages_folder(part.as_os_str().as_encoded_bytes()))
    {
        Some(TemporaryCopy::AppImage)
    } else {
        None
    }
}

// Explorer's `%TEMP%\Temp1_<zip>.zip`, named as seen and not measured.
fn opened_from_a_zip(program: &Path) -> bool {
    program.to_string_lossy().split(['/', '\\']).any(|part| {
        let name = part.to_ascii_lowercase();
        let Some(numbered) = name.strip_prefix("temp") else {
            return false;
        };
        let digits = numbered.bytes().take_while(u8::is_ascii_digit).count();
        digits > 0 && numbered[digits..].starts_with('_') && name.ends_with(".zip")
    })
}

// type2-runtime's `.mount_`, up to six bytes of the AppImage's name and six `mkdtemp` fills, and
// `appimage_extracted_` and the file's MD5 in lower-case hex (docs/facts/linux.md).
fn an_appimages_folder(name: &[u8]) -> bool {
    let mounted = name.strip_prefix(b".mount_").is_some_and(|rest| {
        (6..=12).contains(&rest.len())
            && rest[rest.len() - 6..].iter().all(u8::is_ascii_alphanumeric)
    });
    let extracted = name
        .strip_prefix(b"appimage_extracted_")
        .is_some_and(|digest| {
            digest.len() == 32
                && digest
                    .iter()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        });
    mounted || extracted
}

pub(crate) fn task(ctx: &Context, job: Job) -> Option<crate::host::task::Task> {
    scheduler(ctx)?.task(ctx, job)
}

/// Whether `job` is there as an earlier Pitboard wrote it, which installing it again mends.
pub(crate) fn outdated(ctx: &Context, job: Job) -> bool {
    scheduler(ctx).is_some_and(|s| s.outdated(ctx, job))
}

/// The Pitboard the installed schedule runs, read back from what `install` wrote. `None`
/// where nothing is installed, and where what is there does not name one the way `install`
/// writes it.
pub(crate) fn installed_program(ctx: &Context) -> Option<PathBuf> {
    scheduler(ctx)?.program(ctx, Job::Renew)
}

/// What a job is given of the installing Pitboard's environment, since the system starts it
/// with its own and not the person's shell: `PITBOARD_NO_ARGV` where this context refuses
/// the argument line, and `proxy`, the proxy variables, each by the name and with the value
/// it was set with, so that its runs read the proxy the way those that set it did
/// ([`crate::proxy`]).
pub(crate) fn environment(
    ctx: &Context,
    proxy: impl IntoIterator<Item = (String, String)>,
) -> Vec<(String, String)> {
    let refusing = (!ctx.argv_fallback()).then(|| ("PITBOARD_NO_ARGV".into(), "1".into()));
    refusing.into_iter().chain(proxy).collect()
}

/// The proxy variables the installed schedule's job is given, as its file says, in the
/// order written. Empty where nothing is installed.
fn installed_proxy_variables(ctx: &Context) -> Vec<(String, String)> {
    let Some(scheduler) = scheduler(ctx) else {
        return Vec::new();
    };
    scheduler
        .environment(ctx, Job::Renew)
        .into_iter()
        .filter(|(name, _)| {
            crate::proxy::NAMING
                .iter()
                .chain(&crate::proxy::EXEMPTING)
                .any(|proxy| proxy == name)
        })
        .collect()
}

/// What the installed schedule's runs make of the proxy variables its file gives them, read
/// as this run's are. `None` where nothing is installed.
pub(crate) fn installed_proxies(ctx: &Context) -> Option<crate::proxy::Proxies> {
    if !scheduler(ctx)?.installed(ctx, Job::Renew) {
        return None;
    }
    let given: crate::context::Environment = installed_proxy_variables(ctx).into_iter().collect();
    Some(crate::proxy::Proxies::read(&given))
}

/// Install it, and ask the system to start it. Returns where it went.
///
/// Its job is given this context's proxy variables, which a reinstall takes again, and
/// `PITBOARD_NO_ARGV` where this context has it ([`environment`]).
///
/// Refused with [`Error::ScheduleNotDefaultHome`] where this context is not the default
/// home's ([`serves`]): the schedule renews `~/.pitboard` alone, so installing it while
/// `PITBOARD_HOME` names another directory would leave that directory's parked logins to
/// run out while seeming to keep them alive. `uninstall` and `repair` are not refused for
/// it, so a schedule can always be taken away.
pub(crate) fn install(ctx: &Context, permit: Permit) -> Result<Location> {
    if !serves(ctx) {
        return Err(Error::ScheduleNotDefaultHome {
            home: crate::home::dir(ctx),
            default: ctx.default_pitboard_home(),
            job: Job::Renew,
        });
    }
    let location = put(
        ctx,
        permit,
        Job::Renew,
        &program(ctx, Job::Renew)?,
        &[],
        &environment(ctx, proxy_variables(ctx)),
    )?;
    crate::audit::record(ctx, permit, "schedule", "install", "ok");
    Ok(location)
}

/// This context's proxy variables, as [`environment`] takes them.
pub(crate) fn proxy_variables(ctx: &Context) -> impl Iterator<Item = (String, String)> + '_ {
    ctx.proxy
        .variables()
        .iter()
        .map(|(name, value)| ((*name).to_string(), value.clone()))
}

// The app calls this as it starts. An app up to 0.3.0 scheduled itself, which launchd started
// as a second menu bar app every day, and on Windows a task outlives the Pitboard it runs. Never
// from the schedule's own run, which launchd stops as a repair unloads its job, and never with
// the app's proxy, which names none a shell set.
pub(crate) fn repair(ctx: &Context, permit: Permit) -> Result<bool> {
    if scheduler(ctx).is_none() || started_this_run(ctx, Job::Renew) {
        return Ok(false);
    }
    let Some(named) = ctx.schedule_program() else {
        return Ok(false);
    };
    if !serves(ctx)
        || lasting(named, Job::Renew).is_err()
        || !installed_program(ctx).is_some_and(|program| broken(ctx, &program))
    {
        return Ok(false);
    }
    let kept = installed_proxy_variables(ctx);
    let repaired = put(ctx, permit, Job::Renew, named, &[], &environment(ctx, kept));
    crate::audit::record(
        ctx,
        permit,
        "schedule",
        "repair",
        match &repaired {
            Ok(_) => "ok",
            Err(e) => e.code(),
        },
    );
    repaired.map(|_| true)
}

// A task whose settings were changed in Task Scheduler stays as it is: the change may be the
// person's, and doctor names the way back.
fn broken(ctx: &Context, program: &Path) -> bool {
    let drifted = task(ctx, Job::Renew).is_some_and(|task| !task.drifted.is_empty());
    !drifted && fault(ctx.os, program, program.is_file()).is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fault {
    // The system fails to start it every day.
    Gone,
    // An app's own program renews nothing.
    TheApp,
}

// Doctor's judgement and the app's repair alike, for a program on `os`.
pub(crate) fn fault(os: Os, program: &Path, found: bool) -> Option<Fault> {
    if !found {
        Some(Fault::Gone)
    } else if os.an_apps_own_program(program) {
        Some(Fault::TheApp)
    } else {
        None
    }
}

/// Hand `job` to the system's scheduler, running `program` with `options` and given
/// `environment`, and ask the system to start it.
pub(crate) fn put(
    ctx: &Context,
    permit: Permit,
    job: Job,
    program: &Path,
    options: &[String],
    environment: &[(String, String)],
) -> Result<Location> {
    let Some(scheduler) = scheduler(ctx) else {
        return Err(Error::ScheduleUnsupported { job });
    };
    scheduler.put(ctx, permit, job, program, options, environment)?;
    Ok(scheduler.location(ctx, job))
}

/// Take `job` away. `false` when there was nothing installed.
pub(crate) fn remove(ctx: &Context, permit: Permit, job: Job) -> Result<bool> {
    let Some(scheduler) = scheduler(ctx) else {
        return Err(Error::ScheduleUnsupported { job });
    };
    scheduler.remove(ctx, permit, job)
}

/// Take it away. `false` when there was nothing installed.
pub(crate) fn uninstall(ctx: &Context, permit: Permit) -> Result<bool> {
    if !remove(ctx, permit, Job::Renew)? {
        return Ok(false);
    }
    crate::audit::record(ctx, permit, "schedule", "uninstall", "ok");
    Ok(true)
}

// The MSI's full uninstall deletes the app's command line; a task running Scoop's copy stays.
pub(crate) fn uninstall_for_app(ctx: &Context, permit: Permit) -> Result<bool> {
    let runs_the_apps = |job| {
        ctx.schedule_program().is_some_and(|named| {
            scheduler(ctx)
                .and_then(|s| s.program(ctx, job))
                .is_some_and(|program| ctx.os.same_path(&program, named))
        })
    };
    let watch_removed = runs_the_apps(Job::Watch) && crate::watch_service::uninstall(ctx, permit)?;
    let renew_removed = runs_the_apps(Job::Renew) && uninstall(ctx, permit)?;
    Ok(watch_removed || renew_removed)
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use crate::host::memory::MemoryHost;
    use std::sync::Arc;

    /// A home of the test's own, gone when the test is, on a machine whose scheduler asks
    /// nobody: its files are written in that home, and launchd or systemd is never told.
    struct Scratch(tempfile::TempDir, Arc<MemoryHost>);

    impl Scratch {
        fn new() -> Scratch {
            Scratch(
                tempfile::tempdir().expect("a scratch home"),
                MemoryHost::new(),
            )
        }

        fn path(&self) -> &Path {
            self.0.path()
        }

        fn ctx(&self) -> Context {
            Context::new(self.path().to_path_buf()).with_memory_stores(Arc::clone(&self.1))
        }

        /// A context read from an environment whose home is this one and that sets `vars`
        /// besides, as the command line's is read, on the same machine in memory.
        fn read(&self, vars: &[(&str, &str)]) -> Context {
            let home = self.path().to_string_lossy().into_owned();
            let env: crate::context::Environment =
                [("HOME", home.as_str()), ("USERPROFILE", home.as_str())]
                    .iter()
                    .chain(vars)
                    .copied()
                    .collect();
            Context::for_command_line(&env).with_memory_stores(Arc::clone(&self.1))
        }
    }

    /// The variables the installed schedule's job is given, as its file says.
    fn given(ctx: &Context) -> Vec<(String, String)> {
        scheduler(ctx)
            .expect("a scheduler")
            .environment(ctx, Job::Renew)
    }

    fn pairs(given: &[(&str, &str)]) -> Vec<(String, String)> {
        given
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect()
    }

    // This system's app's own program under `home`, as 0.3.0 scheduled it on a Mac, and the
    // command line the app comes with.
    fn the_app(home: &Path) -> (PathBuf, PathBuf) {
        match crate::host::OS {
            Os::MacOs | Os::Windows => (
                home.join("Applications/Pitboard.app/Contents/MacOS/Pitboard"),
                home.join("Applications/Pitboard.app/Contents/Helpers/pitboard"),
            ),
            Os::Linux => (
                home.join("usr/bin/pitboard-app"),
                home.join("usr/bin/pitboard"),
            ),
        }
    }

    /// Put a program at `path`, for a schedule to name.
    fn a_program_at(path: &Path) {
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("its directory");
        std::fs::write(path, "").expect("a program");
    }

    /// A schedule the scheduler will not start leaves nothing behind saying renewal is on.
    #[test]
    fn a_schedule_the_scheduler_will_not_start_is_not_left_on_disk() {
        let home = Scratch::new();
        let program = home.path().join("bin/pitboard");
        a_program_at(&program);
        let ctx = home.ctx().with_schedule_program(program);

        home.1.refuse_next_start();
        let refused = install(&ctx, Permit::for_a_test()).expect_err("refused");

        assert_eq!(refused.code(), "schedule_refused");
        assert_eq!(status(&ctx), Installed::No);
        assert_eq!(installed_program(&ctx), None);
    }

    /// A repair the scheduler will not start leaves the schedule that was there, which is
    /// what the app and doctor then go on reporting.
    #[test]
    fn a_repair_the_scheduler_will_not_start_leaves_the_schedule_as_it_was() {
        let home = Scratch::new();
        std::fs::create_dir_all(home.ctx().default_pitboard_home()).expect("a Pitboard home");
        let (app, bundled) = the_app(home.path());
        a_program_at(&app);
        a_program_at(&bundled);
        let ctx = home.ctx();
        install(
            &ctx.clone().with_schedule_program(app.clone()),
            Permit::for_a_test(),
        )
        .expect("0.3.0's schedule");

        home.1.refuse_next_start();
        let refused = repair(
            &ctx.clone().with_schedule_program(bundled),
            Permit::for_a_test(),
        )
        .expect_err("refused");

        assert_eq!(refused.code(), "schedule_refused");
        assert!(matches!(status(&ctx), Installed::Yes { .. }));
        assert_eq!(installed_program(&ctx), Some(app));
    }

    #[test]
    fn nothing_is_installed_on_a_machine_where_nothing_was_installed() {
        let home = Scratch::new();
        let ctx = home.ctx();
        assert_eq!(status(&ctx), Installed::No);
        assert_eq!(installed_program(&ctx), None);
    }

    /// A machine with no scheduler Pitboard knows says so, and writes nothing.
    #[test]
    fn a_machine_with_no_scheduler_says_so() {
        let home = Scratch::new();
        home.1.without_a_scheduler();
        let program = home.path().join("bin/pitboard");
        a_program_at(&program);
        let ctx = home.ctx().with_schedule_program(program);
        assert_eq!(status(&ctx), Installed::Unsupported);
        assert_eq!(path(&ctx), None);
        assert_eq!(
            install(&ctx, Permit::for_a_test())
                .expect_err("nowhere")
                .code(),
            "schedule_unsupported"
        );
        assert_eq!(
            uninstall(&ctx, Permit::for_a_test())
                .expect_err("nowhere")
                .code(),
            "schedule_unsupported"
        );
        assert!(!repair(&ctx, Permit::for_a_test()).expect("nothing to do"));
        assert!(!uninstall_for_app(&ctx, Permit::for_a_test()).expect("nothing to do"));
    }

    #[test]
    fn the_schedule_runs_the_pitboard_the_context_names_and_otherwise_this_one() {
        let home = Scratch::new();
        let ctx = home.ctx();
        let scheduled = program(&ctx, Job::Renew).expect("this program");
        let running = std::env::current_exe().expect("this test's own program");
        assert_eq!(
            std::fs::canonicalize(&scheduled).expect("there"),
            std::fs::canonicalize(&running).expect("there"),
            "the command line schedules itself"
        );
        let bundled = home
            .path()
            .join("Applications/Pitboard.app/Contents/Helpers/pitboard");
        a_program_at(&bundled);
        assert_eq!(
            program(&ctx.with_schedule_program(bundled.clone()), Job::Renew)
                .expect("the named one"),
            bundled,
            "an app schedules the command line it comes with"
        );
    }

    /// A Pitboard that is named is written down only where it will still be there when the
    /// scheduler runs it. macOS runs an app opened where it was downloaded from a temporary
    /// copy, which is there while the app runs and gone once it quits, so being there now
    /// is not enough.
    #[test]
    fn a_named_pitboard_that_will_not_be_there_is_refused() {
        let home = Scratch::new();
        let ctx = home.ctx();

        let missing = home.path().join("Pitboard.app/Contents/Helpers/pitboard");
        let refused = install(
            &ctx.clone().with_schedule_program(missing.clone()),
            Permit::for_a_test(),
        )
        .expect_err("nothing there to run");
        assert_eq!(refused.code(), "schedule_program_missing");
        assert!(
            refused.to_string().contains(&missing.display().to_string()),
            "{refused}"
        );

        let temporary = home
            .path()
            .join("AppTranslocation/6A1C/d/Pitboard.app/Contents/Helpers/pitboard");
        a_program_at(&temporary);
        let refused = install(
            &ctx.clone().with_schedule_program(temporary.clone()),
            Permit::for_a_test(),
        )
        .expect_err("a copy that goes away");
        assert_eq!(refused.code(), "schedule_program_temporary");
        assert!(
            refused
                .to_string()
                .contains(&temporary.display().to_string())
                && refused.to_string().contains("Applications folder"),
            "{refused}"
        );

        let unzipped = home
            .path()
            .join("Temp1_pitboard-v0.15.0-x86_64-pc-windows-msvc.zip")
            .join("pitboard.exe");
        a_program_at(&unzipped);
        let refused = install(
            &ctx.clone().with_schedule_program(unzipped),
            Permit::for_a_test(),
        )
        .expect_err("a copy Explorer made of a zip");
        assert_eq!(refused.code(), "schedule_program_temporary");
        assert!(
            refused
                .to_string()
                .contains("Install Pitboard, or extract the zip"),
            "{refused}"
        );

        let mounted = home.path().join(".mount_PitboaJgKlbE/usr/bin/pitboard");
        a_program_at(&mounted);
        let refused = install(
            &ctx.clone().with_schedule_program(mounted),
            Permit::for_a_test(),
        )
        .expect_err("a copy an AppImage mounted");
        assert_eq!(refused.code(), "schedule_program_temporary");
        assert!(
            refused
                .to_string()
                .contains("Install Pitboard rather than run it from an AppImage"),
            "{refused}"
        );

        assert_eq!(status(&ctx), Installed::No, "nothing was written");
    }

    // pitboard.exe started from inside a zip in Explorer runs from Temp1_*.zip, which goes.
    #[test]
    fn the_running_pitboard_is_refused_where_a_named_one_would_be() {
        let home = Scratch::new();
        let unzipped = home
            .path()
            .join("Temp1_pitboard-v0.15.0-x86_64-pc-windows-msvc.zip")
            .join("pitboard.exe");
        a_program_at(&unzipped);
        let refused =
            program_of(None, || Ok(unzipped.clone()), Job::Renew).expect_err("a copy that goes");
        assert_eq!(refused.code(), "schedule_program_temporary");
        let gone = home.path().join("gone").join("pitboard");
        let refused = program_of(None, || Ok(gone.clone()), Job::Renew).expect_err("nothing there");
        assert_eq!(refused.code(), "schedule_program_missing");

        let installed = home.path().join("bin").join("pitboard");
        a_program_at(&installed);
        assert_eq!(
            program_of(None, || Ok(installed.clone()), Job::Renew).expect("one that lasts"),
            installed
        );
    }

    /// A copy macOS runs from a temporary place is told by a directory of the path, as the
    /// Swift app told it by "/AppTranslocation/" in its command line's path, and by nothing
    /// that only looks like one.
    #[test]
    fn a_temporary_copy_is_told_by_its_path() {
        for (path, temporary) in [
            (
                "/private/var/folders/xy/abc/T/AppTranslocation/0A1B2C/d/Pitboard.app/Contents/\
                 Helpers/pitboard",
                true,
            ),
            (
                "/Applications/Pitboard.app/Contents/Helpers/pitboard",
                false,
            ),
            (
                "/Users/x/MyAppTranslocation/Pitboard.app/Contents/Helpers/pitboard",
                false,
            ),
            (
                "/Users/x/AppTranslocations/Pitboard.app/Contents/Helpers/pitboard",
                false,
            ),
            (
                r"C:\Users\x\AppData\Local\Temp\Temp1_pitboard.zip\pitboard\pitboard.exe",
                true,
            ),
            (r"C:\Users\x\Temp12_Pitboard.ZIP\bin\pitboard.exe", true),
            (r"C:\Users\x\Temp_pitboard.zip\pitboard.exe", false),
            (r"C:\Users\x\Temp1_pitboard\pitboard.exe", false),
            ("/tmp/.mount_PitboaJgKlbE/usr/bin/pitboard", true),
            ("/run/user/1000/.mount_x1aB2c3/usr/bin/pitboard", true),
            (
                "/tmp/appimage_extracted_0123456789abcdef0123456789abcdef/usr/bin/pitboard",
                true,
            ),
            ("/home/x/.mount_/usr/bin/pitboard", false),
            ("/home/x/.mount_ab/usr/bin/pitboard", false),
            ("/home/x/mount_PitboaJgKlbE/usr/bin/pitboard", false),
            ("/home/x/.mount_PitboardThings/usr/bin/pitboard", false),
            ("/home/x/.mount_Pitboa-+_*.,/usr/bin/pitboard", false),
            ("/tmp/appimage_extracted_cache/usr/bin/pitboard", false),
            (
                "/tmp/appimage_extracted_0123456789ABCDEF0123456789ABCDEF/usr/bin/pitboard",
                false,
            ),
            ("/home/x/Applications/Pitboard.AppImage", false),
        ] {
            assert_eq!(in_a_temporary_copy(Path::new(path)), temporary, "{path}");
        }
    }

    // A schedule that runs an app's own program renews nothing, on each system by its rule.
    #[test]
    fn a_schedule_running_the_linux_app_is_at_fault() {
        let fault_of = |os: Os, program: &str| fault(os, Path::new(program), true);
        assert_eq!(
            fault_of(Os::Linux, "/usr/bin/pitboard-app"),
            Some(Fault::TheApp)
        );
        assert_eq!(fault_of(Os::Linux, "/usr/bin/pitboard"), None);
        assert_eq!(
            fault_of(Os::Linux, "/opt/Pitboard.app/Contents/MacOS/Pitboard"),
            None,
            "no bundle on Linux"
        );
        let bundle = "/Applications/Pitboard.app/Contents/MacOS/Pitboard";
        assert_eq!(fault_of(Os::MacOs, bundle), Some(Fault::TheApp));
        assert_eq!(fault_of(Os::MacOs, "/usr/bin/pitboard-app"), None);
        assert_eq!(fault_of(Os::Windows, bundle), Some(Fault::TheApp));
        assert_eq!(
            fault(Os::Linux, Path::new("/usr/bin/pitboard-app"), false),
            Some(Fault::Gone)
        );
    }

    /// What `pitboard doctor` reads back is what was written, including a path the
    /// scheduler's format has to escape.
    #[test]
    fn an_installed_schedule_says_which_pitboard_it_runs() {
        let home = Scratch::new();
        let bundled = home
            .path()
            .join("Tools&Apps/Pitboard.app/Contents/Helpers/pitboard");
        a_program_at(&bundled);
        let ctx = home.ctx().with_schedule_program(bundled.clone());

        install(&ctx, Permit::for_a_test()).expect("installed");
        assert!(matches!(status(&ctx), Installed::Yes { .. }));
        assert_eq!(installed_program(&ctx), Some(bundled));

        assert!(uninstall(&ctx, Permit::for_a_test()).expect("taken away"));
        assert_eq!(status(&ctx), Installed::No);
        assert_eq!(installed_program(&ctx), None);
        assert!(!uninstall(&ctx, Permit::for_a_test()).expect("nothing there"));
    }

    /// An app up to 0.3.0 scheduled itself, and launchd has started a second menu bar app
    /// every day since. An app that names the command line it comes with puts that in its
    /// place, and every other schedule is left as it is.
    #[test]
    fn a_schedule_that_runs_an_app_is_pointed_at_its_command_line() {
        let home = Scratch::new();
        std::fs::create_dir_all(home.ctx().default_pitboard_home()).expect("a Pitboard home");
        let (app, bundled) = the_app(home.path());
        a_program_at(&app);
        a_program_at(&bundled);
        let ctx = home.ctx();
        let the_app = ctx.clone().with_schedule_program(bundled.clone());

        assert!(
            !repair(&the_app, Permit::for_a_test()).expect("nothing to do"),
            "nothing installed"
        );

        install(
            &ctx.clone().with_schedule_program(app.clone()),
            Permit::for_a_test(),
        )
        .expect("0.3.0's schedule");
        assert!(
            !repair(&ctx, Permit::for_a_test()).expect("nothing to do"),
            "no command line named"
        );
        let gone = home.path().join("Old.app/Contents/Helpers/pitboard");
        assert!(
            !repair(
                &ctx.clone().with_schedule_program(gone),
                Permit::for_a_test()
            )
            .expect("nothing to do"),
            "a command line that is not there"
        );
        assert!(
            !repair(
                &the_app
                    .clone()
                    .with_pitboard_home(home.path().join("elsewhere")),
                Permit::for_a_test()
            )
            .expect("nothing to do"),
            "a schedule another home's Pitboard looks after"
        );
        assert_eq!(installed_program(&ctx), Some(app.clone()));

        assert!(repair(&the_app, Permit::for_a_test()).expect("repaired"));
        assert_eq!(installed_program(&ctx), Some(bundled));
        let logged = crate::audit::read(&ctx, 1);
        assert_eq!(
            logged
                .iter()
                .map(|e| (e.verb.as_str(), e.subject.as_str(), e.outcome.as_str()))
                .collect::<Vec<_>>(),
            [("schedule", "repair", "ok")]
        );

        assert!(
            !repair(&the_app, Permit::for_a_test()).expect("nothing to do"),
            "a schedule that runs a command line already"
        );
        install(
            &ctx.clone().with_schedule_program(app.clone()),
            Permit::for_a_test(),
        )
        .expect("the app again");
        let temporary = home
            .path()
            .join("AppTranslocation/6A1C/d/Pitboard.app/Contents/Helpers/pitboard");
        a_program_at(&temporary);
        assert!(
            !repair(
                &ctx.clone().with_schedule_program(temporary),
                Permit::for_a_test()
            )
            .expect("nothing to do"),
            "a command line that is gone once the app quits"
        );
        assert_eq!(installed_program(&ctx), Some(app));
    }

    #[test]
    fn a_schedule_whose_pitboard_is_gone_is_pointed_at_the_apps_command_line() {
        let home = Scratch::new();
        std::fs::create_dir_all(home.ctx().default_pitboard_home()).expect("a Pitboard home");
        let brew = home.path().join("homebrew/bin/pitboard");
        let bundled = home
            .path()
            .join("Applications/Pitboard.app/Contents/Helpers/pitboard");
        a_program_at(&brew);
        a_program_at(&bundled);
        let ctx = home.ctx();
        install(
            &ctx.clone().with_schedule_program(brew.clone()),
            Permit::for_a_test(),
        )
        .expect("installed from Homebrew's pitboard");
        let the_app = ctx.clone().with_schedule_program(bundled.clone());
        assert!(
            !repair(&the_app, Permit::for_a_test()).expect("nothing to do"),
            "a Pitboard that is there"
        );

        std::fs::remove_file(&brew).expect("Homebrew's pitboard uninstalled");
        assert!(repair(&the_app, Permit::for_a_test()).expect("repaired"));
        assert_eq!(installed_program(&ctx), Some(bundled));
    }

    /// A process some other job started is not the schedule's own run, and repairs the way
    /// one opened by hand does.
    #[test]
    fn a_process_another_job_started_still_repairs() {
        let home = Scratch::new();
        std::fs::create_dir_all(home.ctx().default_pitboard_home()).expect("a Pitboard home");
        let (app, bundled) = the_app(home.path());
        a_program_at(&app);
        a_program_at(&bundled);
        let ctx = home.ctx();
        install(
            &ctx.clone().with_schedule_program(app),
            Permit::for_a_test(),
        )
        .expect("0.3.0's schedule");

        let opened = ctx
            .clone()
            .with_schedule_program(bundled.clone())
            .with_scheduled_job("application.com.datlechin.pitboard.1.2".into());
        assert!(repair(&opened, Permit::for_a_test()).expect("repaired"));
        assert_eq!(installed_program(&ctx), Some(bundled));
    }

    // Settings changed in Task Scheduler may be the person's, so only a gone program is mended.
    #[test]
    fn the_app_points_a_task_whose_pitboard_is_gone_at_its_own_and_nothing_else() {
        let home = Scratch(
            tempfile::tempdir().expect("a scratch home"),
            MemoryHost::scheduling_tasks("S-1-5-21-1"),
        );
        let ctx = home.read(&[("HTTPS_PROXY", "http://proxy.example.com:3128")]);
        let scoop = home
            .path()
            .join(r"scoop\apps\pitboard\current\pitboard.exe");
        let bundled = home.path().join(r"Programs\Pitboard\bin\pitboard.exe");
        a_program_at(&scoop);
        a_program_at(&bundled);
        install(
            &ctx.clone().with_schedule_program(scoop.clone()),
            Permit::for_a_test(),
        )
        .expect("installed from Scoop's pitboard");
        let the_app = home.read(&[]).with_schedule_program(bundled.clone());
        assert!(
            !repair(&the_app, Permit::for_a_test()).expect("nothing to do"),
            "a Pitboard that is there"
        );

        std::fs::remove_file(&scoop).expect("Scoop's pitboard uninstalled");
        let kept = path(&ctx).expect("the task's file");
        let written = std::fs::read_to_string(&kept).expect("the task");
        for (setting, changed) in [
            ("<StartWhenAvailable>true<", "<StartWhenAvailable>false<"),
            (
                "<StopIfGoingOnBatteries>false<",
                "<StopIfGoingOnBatteries>true<",
            ),
            ("<Hidden>", "<Enabled>false</Enabled><Hidden>"),
        ] {
            std::fs::write(&kept, written.replace(setting, changed)).expect("changed by hand");
            assert!(
                !repair(&the_app, Permit::for_a_test()).expect("nothing to do"),
                "{changed}"
            );
            assert_eq!(installed_program(&ctx), Some(scoop.clone()), "{changed}");
        }

        std::fs::write(&kept, &written).expect("as Pitboard wrote it");
        assert!(repair(&the_app, Permit::for_a_test()).expect("repaired"));
        assert_eq!(installed_program(&ctx), Some(bundled));
        assert_eq!(
            given(&ctx),
            pairs(&[("HTTPS_PROXY", "http://proxy.example.com:3128")]),
            "the proxy it was given, not the app's none"
        );
        assert!(
            !repair(&the_app, Permit::for_a_test()).expect("nothing to do"),
            "the app's own, which is there"
        );
    }

    #[test]
    fn removing_the_app_takes_away_only_a_task_that_runs_its_command_line() {
        let home = Scratch(
            tempfile::tempdir().expect("a scratch home"),
            MemoryHost::scheduling_tasks("S-1-5-21-1"),
        );
        let proxied = home.read(&[("HTTPS_PROXY", "http://proxy.example.com:3128")]);
        let scoop = home
            .path()
            .join(r"scoop\apps\pitboard\current\pitboard.exe");
        let bundled = home.path().join(r"Programs\Pitboard\bin\pitboard.exe");
        a_program_at(&scoop);
        a_program_at(&bundled);
        let the_app = home.read(&[]).with_schedule_program(bundled.clone());

        assert!(
            !uninstall_for_app(&the_app, Permit::for_a_test()).expect("nothing to do"),
            "nothing installed"
        );
        install(
            &proxied.clone().with_schedule_program(scoop.clone()),
            Permit::for_a_test(),
        )
        .expect("installed from Scoop's pitboard");
        assert!(
            !uninstall_for_app(&the_app, Permit::for_a_test()).expect("nothing to do"),
            "Scoop's"
        );
        assert_eq!(installed_program(&proxied), Some(scoop));

        install(
            &proxied.clone().with_schedule_program(bundled.clone()),
            Permit::for_a_test(),
        )
        .expect("installed from the app");
        assert!(
            !uninstall_for_app(&home.read(&[]), Permit::for_a_test()).expect("nothing to do"),
            "no command line named"
        );
        let kept = path(&proxied).expect("the task's file");
        let written = std::fs::read_to_string(&kept).expect("the task");
        std::fs::write(
            &kept,
            written.replace("<Hidden>", "<Enabled>false</Enabled><Hidden>"),
        )
        .expect("turned off in Task Scheduler");
        assert!(uninstall_for_app(&the_app, Permit::for_a_test()).expect("taken away"));
        assert_eq!(status(&proxied), Installed::No);
        assert!(
            !crate::host::task::environment_file(&proxied, Job::Renew).exists(),
            "nor its proxy"
        );
        let logged = crate::audit::read(&proxied, 1);
        assert_eq!(
            logged
                .iter()
                .map(|e| (e.verb.as_str(), e.subject.as_str(), e.outcome.as_str()))
                .collect::<Vec<_>>(),
            [("schedule", "uninstall", "ok")]
        );
        assert!(!uninstall_for_app(&the_app, Permit::for_a_test()).expect("nothing there"));
    }

    // Started at each sign-in, it would fail at each once the removal deletes the program.
    #[test]
    fn removing_the_app_takes_away_a_watch_service_that_runs_its_command_line() {
        let home = Scratch(
            tempfile::tempdir().expect("a scratch home"),
            MemoryHost::scheduling_tasks("S-1-5-21-1"),
        );
        let scoop = home
            .path()
            .join(r"scoop\apps\pitboard\current\pitboard.exe");
        let bundled = home.path().join(r"Programs\Pitboard\bin\pitboard.exe");
        a_program_at(&scoop);
        a_program_at(&bundled);
        let the_app = home.read(&[]).with_schedule_program(bundled.clone());
        let at = crate::duty::autoswitch::Threshold::DEFAULT;
        let watching = |ctx: &Context| {
            crate::watch_service::status(ctx) != crate::watch_service::Installed::No
        };

        crate::watch_service::install(
            &home.read(&[]).with_schedule_program(scoop),
            Permit::for_a_test(),
            at,
        )
        .expect("installed from Scoop's pitboard");
        assert!(!uninstall_for_app(&the_app, Permit::for_a_test()).expect("nothing to do"));
        assert!(watching(&the_app), "Scoop's stays");

        crate::watch_service::install(&the_app, Permit::for_a_test(), at)
            .expect("installed from the app's");
        install(&the_app, Permit::for_a_test()).expect("renewal too");
        assert!(uninstall_for_app(&the_app, Permit::for_a_test()).expect("taken away"));
        assert!(!watching(&the_app));
        assert_eq!(status(&the_app), Installed::No);
    }

    /// Only what `install` writes is read, and only the way it writes it.
    #[test]
    fn a_schedule_pitboard_did_not_write_names_no_program() {
        let home = Scratch::new();
        let ctx = home.ctx();
        let installed = path(&ctx).expect("a scheduler here");
        std::fs::create_dir_all(installed.parent().expect("its directory")).expect("made");
        std::fs::write(&installed, "not what install writes\n").expect("written");
        if let Some(beside) = installed.parent() {
            // What systemd's timer runs is its service, beside it.
            std::fs::write(
                beside.join("pitboard-renew.service"),
                "[Service]\nExecStart=/bin/true\n",
            )
            .expect("written");
        }

        assert!(matches!(status(&ctx), Installed::Yes { .. }));
        assert_eq!(installed_program(&ctx), None);
    }

    /// The scheduler starts `renew` with the default home, so a Pitboard pointed anywhere
    /// else leaves the schedule alone.
    #[test]
    fn the_schedule_belongs_to_the_default_home_alone() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let ctx = Context::new(home.clone());
        let default = crate::host::OS.default_pitboard_home(&home);
        assert!(serves(&ctx));
        let trailing = PathBuf::from(format!(
            "{}{}",
            default.display(),
            std::path::MAIN_SEPARATOR
        ));
        assert!(serves(&ctx.clone().with_pitboard_home(trailing)));
        let upper = PathBuf::from(default.to_string_lossy().to_uppercase());
        assert_eq!(
            serves(&ctx.clone().with_pitboard_home(upper)),
            crate::host::OS == Os::Windows,
            "in another case"
        );
        assert!(!serves(&ctx.with_pitboard_home(home.join("elsewhere"))));
    }

    // RF-B1, run 38693947329: on Windows a scheduled run and one by hand both derive the
    // folder from `USERPROFILE`, wherever `HOME` points.
    #[test]
    fn a_windows_schedule_renews_the_folder_in_userprofile_whatever_home_says() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let profile = scratch.path().join("profile");
        let named = profile.to_string_lossy().into_owned();
        let env = crate::context::Environment::on(
            Os::Windows,
            [
                ("USERPROFILE", named.as_str()),
                ("HOME", "/c/Users/elsewhere"),
            ],
        );
        let ctx = Context::read(&env, "cli");
        let folder = profile.join("AppData").join("Local").join("Pitboard");
        assert_eq!(ctx.pitboard_home, folder);
        assert_eq!(env.pitboard_home(), folder, "what an app asks for");
        assert!(serves(&ctx));
        let run = for_its_run(
            &ctx.clone()
                .with_pitboard_home(profile.join("elsewhere"))
                .started_by_the_schedule(),
            Job::Renew,
        );
        assert_eq!(run.pitboard_home, folder);
    }

    // Stopped before it reads anything: the context is all that is made.
    #[test]
    #[should_panic(expected = "with this account's own home")]
    fn a_build_for_tests_never_runs_as_the_schedule_with_the_accounts_own_home() {
        let own = {
            let _real = crate::host::user::testing::reaching_the_real_home();
            crate::host::user::home().expect("this account has a home")
        };
        let _ = for_its_run(&Context::new(own).started_by_the_schedule(), Job::Renew);
    }

    /// The schedule renews the default home alone, so it is installed only from there. With
    /// `PITBOARD_HOME` naming any other directory, `install` is refused with
    /// `schedule_not_default_home`, which says what the schedule renews and how to install
    /// it, and nothing is written or started. `PITBOARD_HOME` naming the default home, however
    /// it spells it, installs as before. Neither `uninstall` nor `repair` is refused for it:
    /// a schedule can always be taken away, from whichever home.
    #[test]
    fn a_schedule_is_installed_only_where_pitboard_home_is_the_default_home() {
        let home = Scratch::new();
        std::fs::create_dir_all(home.ctx().default_pitboard_home()).expect("a Pitboard home");
        let program = home.path().join("bin/pitboard");
        a_program_at(&program);
        let ctx = home.ctx().with_schedule_program(program);
        let default = ctx.default_pitboard_home();
        let elsewhere = home.path().join("elsewhere");
        let another = ctx.clone().with_pitboard_home(elsewhere.clone());

        home.1.refuse_next_start();
        let refused = install(&another, Permit::for_a_test()).expect_err("another home");
        assert_eq!(refused.code(), "schedule_not_default_home");
        assert_eq!(refused.exit_code(), 1);
        assert_eq!(
            refused.to_string(),
            format!(
                "the renewal schedule renews only the parked logins in {}, not those in {}, \
                 which PITBOARD_HOME names. Nothing was scheduled. To schedule it for {}, \
                 unset PITBOARD_HOME and install it again.",
                default.display(),
                elsewhere.display(),
                default.display()
            )
        );
        assert_eq!(status(&ctx), Installed::No, "nothing was written");
        assert!(crate::audit::read(&another, 10).is_empty(), "nor logged");

        let spelled = PathBuf::from(format!("{}/", default.display()));
        let refused = install(
            &ctx.clone().with_pitboard_home(spelled),
            Permit::for_a_test(),
        )
        .expect_err("the default home reaches the scheduler, which refuses this once");
        assert_eq!(refused.code(), "schedule_refused");
        install(&ctx, Permit::for_a_test()).expect("installed from the default home");
        assert!(matches!(status(&ctx), Installed::Yes { .. }));

        assert!(!repair(&another, Permit::for_a_test()).expect("not refused"));
        assert!(uninstall(&another, Permit::for_a_test()).expect("taken away from another home"));
        assert_eq!(status(&ctx), Installed::No);
    }

    /// The system starts the schedule's runs with its own environment, so `install` gives its
    /// job the proxy variables it was started with, each under the name it was set by, even
    /// empty, in the order Pitboard reads them, after `PITBOARD_NO_ARGV` where that is set.
    /// The runs then read the same proxy. A reinstall takes the variables of the Pitboard
    /// that runs it, none included. What is written is only the person's to read.
    #[test]
    fn a_schedule_is_given_the_proxy_variables_of_whoever_installs_it() {
        let home = Scratch::new();
        std::fs::create_dir_all(home.ctx().default_pitboard_home()).expect("a Pitboard home");
        let program = home.path().join("bin/pitboard");
        a_program_at(&program);
        let proxied = home
            .read(&[
                ("PITBOARD_NO_ARGV", "1"),
                ("no_proxy", "localhost"),
                ("HTTPS_PROXY", "http://proxy.example.com:3128"),
                ("ALL_PROXY", "socks5h://alice:s3cret@127.0.0.1:1080"),
                ("NO_PROXY", ""),
                ("PATH", "/usr/bin"),
            ])
            .with_schedule_program(program.clone());

        install(&proxied, Permit::for_a_test()).expect("installed");
        let (all, https) = (
            ("ALL_PROXY", "socks5h://alice:s3cret@127.0.0.1:1080"),
            ("HTTPS_PROXY", "http://proxy.example.com:3128"),
        );
        let (handed, written, private) = match crate::host::OS {
            Os::MacOs | Os::Linux => (
                vec![
                    ("PITBOARD_NO_ARGV", "1"),
                    all,
                    https,
                    ("NO_PROXY", ""),
                    ("no_proxy", "localhost"),
                ],
                path(&proxied).expect("a scheduler here"),
                "mode 600",
            ),
            // One variable in any case, the first spelling given, as Windows hands it on (RF-B4);
            // no `PITBOARD_NO_ARGV`, read nowhere there; and the task's definition holds no proxy.
            Os::Windows => (
                vec![all, https, ("no_proxy", "localhost")],
                crate::host::task::environment_file(&proxied, Job::Renew),
                "open to you, SYSTEM and Administrators",
            ),
        };
        assert_eq!(given(&proxied), pairs(&handed));
        let its = installed_proxies(&proxied).expect("installed");
        assert!(its.same_way(&proxied.proxy), "{its:?}");
        assert_eq!(its.variables(), proxied.proxy.variables());
        assert_eq!(
            crate::host::fs::access(&written).map(|access| access.described),
            Some(private.to_string()),
            "a proxy's password is the person's alone to read"
        );

        let direct = home.read(&[]).with_schedule_program(program);
        install(&direct, Permit::for_a_test()).expect("installed again");
        assert_eq!(given(&direct), Vec::new(), "a reinstall takes this run's");
        assert!(
            installed_proxies(&direct)
                .expect("installed")
                .same_way(&direct.proxy)
        );
    }

    /// A repair, which an app makes when it starts, writes the proxy variables the schedule's
    /// file gives, and never the app's own. The schedules it repairs are those an app of
    /// 0.3.0 or earlier wrote, which gave no proxy, so a repair gives none, even where the
    /// app's environment names one. Where such a file gives a proxy all the same, as one
    /// edited by hand can, the repair keeps it.
    #[test]
    fn a_repair_writes_the_proxy_the_schedules_file_gives_and_never_the_apps() {
        let home = Scratch::new();
        std::fs::create_dir_all(home.ctx().default_pitboard_home()).expect("a Pitboard home");
        let (app, bundled) = the_app(home.path());
        a_program_at(&app);
        a_program_at(&bundled);
        let apps_own = [("HTTPS_PROXY", "http://app.example.com:3128")];

        install(
            &home.read(&[]).with_schedule_program(app.clone()),
            Permit::for_a_test(),
        )
        .expect("0.3.0's schedule, which gives no proxy");
        let the_app = home.read(&apps_own).with_schedule_program(bundled.clone());
        assert!(repair(&the_app, Permit::for_a_test()).expect("repaired"));
        assert_eq!(installed_program(&the_app), Some(bundled.clone()));
        assert_eq!(given(&the_app), Vec::new(), "no proxy, not the app's");

        let edited = pairs(&[
            ("https_proxy", "http://alice:s3cret@proxy.example.com:3128"),
            ("NO_PROXY", ".anthropic.com"),
        ]);
        install(
            &home
                .read(&[
                    ("https_proxy", "http://alice:s3cret@proxy.example.com:3128"),
                    ("NO_PROXY", ".anthropic.com"),
                ])
                .with_schedule_program(app),
            Permit::for_a_test(),
        )
        .expect("an app's schedule that gives a proxy, as one edited by hand");
        assert!(repair(&the_app, Permit::for_a_test()).expect("repaired"));
        assert_eq!(installed_program(&the_app), Some(bundled));
        assert_eq!(given(&the_app), edited, "the proxy its file gave");
    }

    /// A real context's scheduler is never asked from a test: the system's own service
    /// manager would reach the person's real schedule. On Linux the user's systemd is asked
    /// first, before anything is written, and refuses; where systemd did not start the system,
    /// as in a container, there is no scheduler to reach.
    #[test]
    fn a_test_never_reaches_the_systems_own_scheduler() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let root = scratch.path().join("root");
        let program = root.join("bin/pitboard");
        a_program_at(&program);
        let ctx = Context::new(root.clone()).with_schedule_program(program);
        let refused =
            install(&ctx, Permit::for_a_test()).expect_err("refused before it reaches the system");
        match scheduler(&ctx) {
            None => assert_eq!(refused.code(), "schedule_unsupported"),
            Some(_) => assert_eq!(refused.code(), "schedule_refused"),
        }
        if crate::host::OS == Os::Linux && scheduler(&ctx).is_some() {
            assert!(refused.to_string().contains("user manager"), "{refused}");
        }
        assert!(
            !matches!(status(&ctx), Installed::Yes { .. }),
            "and nothing is left written"
        );
    }
}
