//! Keeping parked logins alive without anybody running a command.
//!
//! A refresh token has a finite life, and the only two things that renew one are somebody
//! typing `pitboard` and the menu bar app's poll. So the tool is safe for a macOS user who
//! installed the app and leaves it running, and quietly unsafe for everyone else: the
//! whole of Linux, and any macOS user on the command line alone. Go away for the refresh
//! window, come back, and every parked login is dead and each account needs a browser
//! sign-in, which is precisely the cost pitboard exists to spare people.
//!
//! This is opt-in, and stays opt-in. A background process that talks to Anthropic on a
//! schedule is the shape most likely to be read as automation, so it is something a person
//! turns on knowing what it is, and what it does is written where they can read it: it
//! renews the owner's own parked logins and nothing else. It never switches, never asks for
//! usage, and makes no request other than the token exchange.
//!
//! What it installs is the platform's own thing, not a homemade daemon: a LaunchAgent on
//! macOS, which runs inside the login session so the keychain is unlocked, and a systemd
//! user timer on Linux, which is that platform's answer. Where there is neither, it says so
//! rather than inventing a third.

use crate::context::Context;
use crate::error::{Error, Result};
use std::path::PathBuf;

/// What launchd calls the job, so a person can find it without pitboard telling them.
/// systemd names its own unit, which is why this is macOS only.
#[cfg(target_os = "macos")]
const LABEL: &str = "com.datlechin.pitboard.renew";

/// Once a day. A refresh token's life is measured in weeks and pitboard starts renewing
/// three days out, so a daily check has three chances to catch each one, and a machine
/// that was asleep for one of them still has two.
const EVERY_SECONDS: u32 = 86_400;

/// Whether the schedule is installed, and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    /// Installed, at this path.
    Yes {
        path: PathBuf,
        every_seconds: u32,
    },
    No,
    /// This platform has no scheduler pitboard knows how to write.
    Unsupported,
}

#[cfg(target_os = "macos")]
fn agent_path(ctx: &Context) -> PathBuf {
    ctx.home()
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

#[cfg(target_os = "linux")]
fn unit_dir(ctx: &Context) -> PathBuf {
    ctx.home().join(".config/systemd/user")
}

/// Where the schedule lives on this platform, or `None` where there is none.
pub fn path(ctx: &Context) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Some(agent_path(ctx))
    }
    #[cfg(target_os = "linux")]
    {
        Some(unit_dir(ctx).join("pitboard-renew.timer"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = ctx;
        None
    }
}

pub fn status(ctx: &Context) -> Installed {
    match path(ctx) {
        None => Installed::Unsupported,
        Some(path) if path.is_file() => Installed::Yes {
            path,
            every_seconds: EVERY_SECONDS,
        },
        Some(_) => Installed::No,
    }
}

/// Whether the schedule is this context's to look after.
///
/// launchd and systemd start `renew` without `PITBOARD_HOME`, so the schedule always renews
/// the default `~/.pitboard`. A pitboard pointed at another home has none of its own: the
/// one there is belongs to the default home.
pub(crate) fn serves(ctx: &Context) -> bool {
    crate::home::dir(ctx) == ctx.home().join(".pitboard")
}

/// The pitboard the schedule should run: the one the context names, or this one.
fn program(ctx: &Context) -> Result<PathBuf> {
    if let Some(program) = ctx.schedule_program() {
        return lasting(program).map(std::path::Path::to_path_buf);
    }
    let running = std::env::current_exe().map_err(|source| Error::HomeUnwritable {
        path: PathBuf::from("the running pitboard"),
        source,
    })?;
    #[cfg(target_os = "linux")]
    let running = started_as(
        std::env::args_os().next().as_deref(),
        &std::env::var_os("PATH").unwrap_or_default(),
        running,
    );
    Ok(running)
}

/// The path this pitboard was started by, `argv0` found on `search` the way the shell found
/// it, where that leads to the `running` program; otherwise `running`.
///
/// Linux says where the running program is with every link resolved, and the link is what
/// lasts: Homebrew starts pitboard through one in its `bin` that leads into a directory
/// named after the version, which the next upgrade deletes. A path that leads to some
/// other file did not start this one. macOS already says what path a program was started
/// by.
#[cfg(any(target_os = "linux", test))]
fn started_as(
    argv0: Option<&std::ffi::OsStr>,
    search: &std::ffi::OsStr,
    running: PathBuf,
) -> PathBuf {
    let resolved = std::fs::canonicalize(&running).ok();
    argv0
        .and_then(|named| crate::provider::find_program(std::path::Path::new(named), search))
        .filter(|found| resolved.is_some() && std::fs::canonicalize(found).ok() == resolved)
        .unwrap_or(running)
}

/// `program`, where it will still be there when the scheduler runs it. launchd and systemd
/// start a program that is gone without telling anyone, every day, so a path that leads
/// nowhere is refused rather than written down.
///
/// A path inside the copy macOS makes of an app opened where it was downloaded leads
/// somewhere while that app runs and nowhere once it quits, so it is refused as well.
fn lasting(program: &std::path::Path) -> Result<&std::path::Path> {
    if program
        .components()
        .any(|part| part.as_os_str() == "AppTranslocation")
    {
        return Err(Error::ScheduleProgramTemporary {
            path: program.to_path_buf(),
        });
    }
    if !program.is_file() {
        return Err(Error::ScheduleProgramMissing {
            path: program.to_path_buf(),
        });
    }
    Ok(program)
}

/// The pitboard the installed schedule runs, read back from what `install` wrote. `None`
/// where nothing is installed, and where the file does not name one the way `install`
/// writes it.
pub fn installed_program(ctx: &Context) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let body = std::fs::read_to_string(agent_path(ctx)).ok()?;
        let (_, after) = body.split_once("<key>ProgramArguments</key>")?;
        let (_, after) = after.split_once("<string>")?;
        let (program, _) = after.split_once("</string>")?;
        Some(PathBuf::from(unescape(program)))
    }
    #[cfg(target_os = "linux")]
    {
        // The timer is what says the schedule is installed; the service is what it runs.
        if !unit_dir(ctx).join("pitboard-renew.timer").is_file() {
            return None;
        }
        let body = std::fs::read_to_string(unit_dir(ctx).join("pitboard-renew.service")).ok()?;
        body.lines()
            .find_map(|line| line.strip_prefix("ExecStart=")?.strip_suffix(" renew"))
            .map(PathBuf::from)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = ctx;
        None
    }
}

fn write(path: &std::path::Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::HomeUnwritable {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    crate::atomic::write(path, body.as_bytes(), crate::atomic::Perms::MatchExisting).map_err(
        |source| Error::HomeUnwritable {
            path: path.to_path_buf(),
            source,
        },
    )
}

/// Install it, and ask the platform to start it. Returns where it went.
pub fn install(ctx: &Context) -> Result<PathBuf> {
    let path = put(ctx, &program(ctx)?)?;
    crate::audit::record(ctx, "schedule", "install", "ok");
    Ok(path)
}

/// Point a schedule that runs an app's own program at the command line the context names.
/// `true` when it did.
///
/// An app up to 0.3.0 scheduled itself, and that app renews nothing when started with
/// `renew`: launchd started a second menu bar app every day instead. The app calls this
/// when it starts, so nothing changes unless the schedule is such a one and belongs to this
/// home, and the context names a command line that will still be there when the scheduler
/// runs it.
///
/// Nothing changes from inside the schedule's own job either: launchd stops a job's process
/// when it unloads the job, which a repair does before loading it again, so nothing would be
/// left to load it back.
pub fn repair(ctx: &Context) -> Result<bool> {
    #[cfg(target_os = "macos")]
    if ctx.launchd_job().as_deref() == Some(LABEL) {
        return Ok(false);
    }
    let Some(named) = ctx.schedule_program() else {
        return Ok(false);
    };
    if !serves(ctx)
        || lasting(named).is_err()
        || !installed_program(ctx).is_some_and(|program| an_apps_own_program(&program))
    {
        return Ok(false);
    }
    let repaired = put(ctx, named);
    crate::audit::record(
        ctx,
        "schedule",
        "repair",
        match &repaired {
            Ok(_) => "ok",
            Err(e) => e.code(),
        },
    );
    repaired.map(|_| true)
}

/// Whether `program` is the one an app bundle starts, `Contents/MacOS/<name>`, where no
/// command line is ever kept.
pub(crate) fn an_apps_own_program(program: &std::path::Path) -> bool {
    let mut dirs = program
        .ancestors()
        .skip(1)
        .map(|dir| dir.file_name().and_then(|n| n.to_str()));
    dirs.next() == Some(Some("MacOS")) && dirs.next() == Some(Some("Contents"))
}

/// Write the schedule to run `program`, and ask the platform to start it.
///
/// Where the platform will not start it, the files go back to what they were and whatever
/// was running before is started again. The status, doctor and the app all read the files,
/// so files left naming a schedule nothing is running would say renewal is on while it is
/// not, which is the failure nobody would notice until the parked logins had run out.
fn put(ctx: &Context, program: &std::path::Path) -> Result<PathBuf> {
    let Some(path) = path(ctx) else {
        return Err(Error::ScheduleUnsupported);
    };

    #[cfg(target_os = "macos")]
    {
        let before = std::fs::read_to_string(&path).ok();
        write(&path, &plist(program))?;
        // `bootstrap` is launchd's own word for this, and replaces the deprecated `load`.
        // SAFETY: `getuid` cannot fail and touches no memory of this process.
        let uid = unsafe { libc::getuid() };
        let domain = format!("gui/{uid}");
        let target = path.to_string_lossy();
        let _ = run("/bin/launchctl", &["bootout", &domain, &target]);
        if let Err(refused) = run("/bin/launchctl", &["bootstrap", &domain, &target]) {
            restore(&path, before.as_deref());
            if before.is_some() {
                let _ = run("/bin/launchctl", &["bootstrap", &domain, &target]);
            }
            return Err(refused);
        }
    }

    #[cfg(target_os = "linux")]
    {
        let unit = unit_dir(ctx).join("pitboard-renew.service");
        let before = (
            std::fs::read_to_string(&unit).ok(),
            std::fs::read_to_string(&path).ok(),
        );
        write(&unit, &service(program))?;
        write(&path, &timer())?;
        let _ = run("systemctl", &["--user", "daemon-reload"]);
        let start = ["--user", "enable", "--now", "pitboard-renew.timer"];
        if let Err(refused) = run("systemctl", &start) {
            restore(&unit, before.0.as_deref());
            restore(&path, before.1.as_deref());
            let _ = run("systemctl", &["--user", "daemon-reload"]);
            if before.1.is_some() {
                let _ = run("systemctl", &start);
            }
            return Err(refused);
        }
    }

    Ok(path)
}

/// Put a file back the way it was before this run wrote it: its old contents, or not there.
/// Best effort, because it runs on the way out of a failure that is already being reported.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn restore(path: &std::path::Path, before: Option<&str>) {
    let _ = match before {
        Some(body) => write(path, body),
        None => remove(path),
    };
}

/// Take it away. `false` when there was nothing installed.
pub fn uninstall(ctx: &Context) -> Result<bool> {
    let Some(path) = path(ctx) else {
        return Err(Error::ScheduleUnsupported);
    };
    if !path.is_file() {
        return Ok(false);
    }

    #[cfg(target_os = "macos")]
    {
        // SAFETY: `getuid` cannot fail and touches no memory of this process.
        let uid = unsafe { libc::getuid() };
        let domain = format!("gui/{uid}");
        let _ = run(
            "/bin/launchctl",
            &["bootout", &domain, &path.to_string_lossy()],
        );
        remove(&path)?;
    }

    #[cfg(target_os = "linux")]
    {
        let _ = run(
            "systemctl",
            &["--user", "disable", "--now", "pitboard-renew.timer"],
        );
        remove(&path)?;
        remove(&unit_dir(ctx).join("pitboard-renew.service"))?;
        let _ = run("systemctl", &["--user", "daemon-reload"]);
    }

    crate::audit::record(ctx, "schedule", "uninstall", "ok");
    Ok(true)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn remove(path: &std::path::Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::HomeUnwritable {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Ask the platform's scheduler to start or stop the schedule.
///
/// Never from a test. A test's home is a scratch directory, but the scheduler it would ask is
/// the person's own: launchd finds a job by the label inside the file, so booting out a
/// scratch copy stops their real schedule, and `systemctl --user` reaches the one session
/// there is. A test writes and reads the files and leaves the scheduler alone.
#[cfg(all(any(target_os = "macos", target_os = "linux"), test))]
fn run(program: &str, args: &[&str]) -> Result<()> {
    let refused = REFUSED.with(|refused| {
        let mut refused = refused.borrow_mut();
        refused
            .filter(|verb| args.contains(verb))
            .inspect(|_| *refused = None)
    });
    match refused {
        Some(verb) => Err(Error::ScheduleRefused {
            detail: format!("{program} refused {verb} in a test"),
        }),
        None => Ok(()),
    }
}

#[cfg(all(any(target_os = "macos", target_os = "linux"), test))]
thread_local! {
    /// The scheduler verb a test has asked to be refused, once.
    static REFUSED: std::cell::RefCell<Option<&'static str>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(all(any(target_os = "macos", target_os = "linux"), not(test)))]
fn run(program: &str, args: &[&str]) -> Result<()> {
    let out = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|source| Error::HomeUnwritable {
            path: PathBuf::from(program),
            source,
        })?;
    if out.status.success() {
        return Ok(());
    }
    Err(Error::ScheduleRefused {
        detail: format!(
            "{program} exited {}: {}",
            out.status
                .code()
                .map_or_else(|| "on a signal".into(), |c| c.to_string()),
            String::from_utf8_lossy(&out.stderr).trim()
        ),
    })
}

/// launchd's own format. `RunAtLoad` is off: installing this is not a reason to talk to
/// Anthropic that second, and the first run comes at the first interval.
#[cfg(target_os = "macos")]
fn plist(program: &std::path::Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>renew</string>
  </array>
  <key>StartInterval</key><integer>{EVERY_SECONDS}</integer>
  <key>RunAtLoad</key><false/>
  <key>LowPriorityIO</key><true/>
  <key>ProcessType</key><string>Background</string>
</dict>
</plist>
"#,
        escape(&program.to_string_lossy())
    )
}

/// A path as XML text. An app can be kept in a folder whose name has an ampersand in it,
/// and launchd refuses a plist that is not well formed.
#[cfg(target_os = "macos")]
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// What [`escape`] wrote, read back.
#[cfg(target_os = "macos")]
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(target_os = "linux")]
fn service(program: &std::path::Path) -> String {
    format!(
        "[Unit]\n\
         Description=Renew pitboard's parked logins\n\
         Documentation=https://docs.usepitboard.com\n\
         \n\
         [Service]\n\
         Type=oneshot\n\
         ExecStart={} renew\n",
        program.display()
    )
}

#[cfg(target_os = "linux")]
fn timer() -> String {
    format!(
        "[Unit]\n\
         Description=Renew pitboard's parked logins daily\n\
         \n\
         [Timer]\n\
         OnUnitActiveSec={EVERY_SECONDS}\n\
         OnStartupSec=900\n\
         Persistent=true\n\
         \n\
         [Install]\n\
         WantedBy=timers.target\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A home of the test's own, gone when the test is.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let root = std::env::temp_dir().join(format!(
                "pitboard-schedule-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("a scratch home");
            Scratch(root)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Put a program at `path`, for a schedule to name.
    fn a_program_at(path: &std::path::Path) {
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("its directory");
        std::fs::write(path, "").expect("a program");
    }

    /// The verb that starts a schedule on this platform.
    #[cfg(target_os = "macos")]
    const START: &str = "bootstrap";
    #[cfg(target_os = "linux")]
    const START: &str = "enable";

    /// Run `change` with the scheduler refusing `verb` the first time it is asked.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn refusing<T>(verb: &'static str, change: impl FnOnce() -> T) -> T {
        REFUSED.with(|refused| *refused.borrow_mut() = Some(verb));
        let outcome = change();
        REFUSED.with(|refused| *refused.borrow_mut() = None);
        outcome
    }

    /// A schedule the scheduler will not start leaves no file behind saying renewal is on.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_schedule_the_scheduler_will_not_start_is_not_left_on_disk() {
        let home = Scratch::new("refused");
        let program = home.0.join("bin/pitboard");
        a_program_at(&program);
        let ctx = Context::new(home.0.clone()).with_schedule_program(program);

        let refused = refusing(START, || install(&ctx)).expect_err("refused");

        assert_eq!(refused.code(), "schedule_refused");
        assert_eq!(status(&ctx), Installed::No);
        assert_eq!(installed_program(&ctx), None);
    }

    /// A repair the scheduler will not start leaves the schedule that was there, which is
    /// what the app and doctor then go on reporting.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_repair_the_scheduler_will_not_start_leaves_the_schedule_as_it_was() {
        let home = Scratch::new("repair-refused");
        std::fs::create_dir_all(home.0.join(".pitboard")).expect("a pitboard home");
        let app = home
            .0
            .join("Applications/Pitboard.app/Contents/MacOS/Pitboard");
        let bundled = home
            .0
            .join("Applications/Pitboard.app/Contents/Helpers/pitboard");
        a_program_at(&app);
        a_program_at(&bundled);
        let ctx = Context::new(home.0.clone());
        install(&ctx.clone().with_schedule_program(app.clone())).expect("0.3.0's schedule");

        let refused = refusing(START, || {
            repair(&ctx.clone().with_schedule_program(bundled))
        })
        .expect_err("refused");

        assert_eq!(refused.code(), "schedule_refused");
        assert!(matches!(status(&ctx), Installed::Yes { .. }));
        assert_eq!(installed_program(&ctx), Some(app));
    }

    #[test]
    fn nothing_is_installed_on_a_machine_where_nothing_was_installed() {
        let home = Scratch::new("none");
        let ctx = Context::new(home.0.clone());
        assert!(matches!(
            status(&ctx),
            Installed::No | Installed::Unsupported
        ));
        assert_eq!(installed_program(&ctx), None);
    }

    #[test]
    fn the_schedule_runs_the_pitboard_the_context_names_and_otherwise_this_one() {
        let home = Scratch::new("program");
        let ctx = Context::new(home.0.clone());
        let scheduled = program(&ctx).expect("this program");
        let running = std::env::current_exe().expect("this test's own program");
        assert_eq!(
            std::fs::canonicalize(&scheduled).expect("there"),
            std::fs::canonicalize(&running).expect("there"),
            "the command line schedules itself"
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            scheduled, running,
            "by the path macOS says it was started by"
        );
        let bundled = home
            .0
            .join("Applications/Pitboard.app/Contents/Helpers/pitboard");
        a_program_at(&bundled);
        assert_eq!(
            program(&ctx.with_schedule_program(bundled.clone())).expect("the named one"),
            bundled,
            "an app schedules the command line it comes with"
        );
    }

    /// Homebrew starts pitboard through a link in its `bin` that leads into a directory
    /// named after the version, and the next upgrade deletes that directory. Linux says
    /// where the running program is with every link resolved, so the schedule is given the
    /// path pitboard was started by instead, wherever that leads to this same program.
    #[test]
    fn the_command_line_schedules_itself_by_the_path_it_was_started_by() {
        use std::os::unix::fs::PermissionsExt;
        let home = Scratch::new("started");
        let runnable = |path: &std::path::Path| {
            a_program_at(path);
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
                .expect("runnable");
        };
        let running = home.0.join("Caskroom/pitboard/0.4.0/pitboard");
        runnable(&running);
        let bin = home.0.join("bin");
        std::fs::create_dir_all(&bin).expect("a bin");
        let link = bin.join("pitboard");
        std::os::unix::fs::symlink(&running, &link).expect("a link");
        let another = home.0.join("elsewhere/pitboard");
        runnable(&another);

        let started = |argv0: Option<&std::path::Path>, search: &std::path::Path| {
            started_as(
                argv0.map(std::path::Path::as_os_str),
                search.as_os_str(),
                running.clone(),
            )
        };
        let name = std::path::Path::new("pitboard");
        let nowhere = std::path::Path::new("");
        assert_eq!(started(Some(name), &bin), link, "a name found on PATH");
        assert_eq!(started(Some(&link), nowhere), link, "a path");
        assert_eq!(
            started(Some(name), another.parent().expect("its directory")),
            running,
            "another pitboard on PATH is not the one that is running"
        );
        assert_eq!(
            started(Some(name), nowhere),
            running,
            "a name found nowhere"
        );
        assert_eq!(started(None, &bin), running, "no name at all");
    }

    /// A pitboard that is named is written down only where it will still be there when the
    /// scheduler runs it. macOS runs an app opened where it was downloaded from a temporary
    /// copy, which is there while the app runs and gone once it quits, so being there now
    /// is not enough.
    #[test]
    fn a_named_pitboard_that_will_not_be_there_is_refused() {
        let home = Scratch::new("refused");
        let ctx = Context::new(home.0.clone());

        let missing = home.0.join("Pitboard.app/Contents/Helpers/pitboard");
        let refused = install(&ctx.clone().with_schedule_program(missing.clone()))
            .expect_err("nothing there to run");
        assert_eq!(refused.code(), "schedule_program_missing");
        assert!(
            refused.to_string().contains(&missing.display().to_string()),
            "{refused}"
        );

        let temporary = home
            .0
            .join("AppTranslocation/6A1C/d/Pitboard.app/Contents/Helpers/pitboard");
        a_program_at(&temporary);
        let refused = install(&ctx.clone().with_schedule_program(temporary.clone()))
            .expect_err("a copy that goes away");
        assert_eq!(refused.code(), "schedule_program_temporary");
        assert!(
            refused
                .to_string()
                .contains(&temporary.display().to_string())
                && refused.to_string().contains("Applications folder"),
            "{refused}"
        );

        assert!(
            matches!(status(&ctx), Installed::No | Installed::Unsupported),
            "nothing was written"
        );
    }

    /// What `pitboard doctor` reads back is what was written, including a path launchd's
    /// format has to escape. The file itself is looked at too: a path with an ampersand
    /// reads back the same whether or not it was escaped.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn an_installed_schedule_says_which_pitboard_it_runs() {
        let home = Scratch::new("installed");
        let bundled = home
            .0
            .join("Tools&Apps/Pitboard.app/Contents/Helpers/pitboard");
        a_program_at(&bundled);
        let ctx = Context::new(home.0.clone()).with_schedule_program(bundled.clone());

        install(&ctx).expect("installed");
        assert!(matches!(status(&ctx), Installed::Yes { .. }));
        #[cfg(target_os = "macos")]
        {
            let written = std::fs::read_to_string(agent_path(&ctx)).expect("the agent");
            assert!(written.contains("/Tools&amp;Apps/"), "{written}");
            assert!(!written.contains("/Tools&Apps/"), "{written}");
        }
        assert_eq!(installed_program(&ctx), Some(bundled));

        assert!(uninstall(&ctx).expect("taken away"));
        assert_eq!(status(&ctx), Installed::No);
        assert_eq!(installed_program(&ctx), None);
    }

    /// An app up to 0.3.0 scheduled itself, and launchd has started a second menu bar app
    /// every day since. An app that names the command line it comes with puts that in its
    /// place, and every other schedule is left as it is.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_schedule_that_runs_an_app_is_pointed_at_its_command_line() {
        let home = Scratch::new("repair");
        std::fs::create_dir_all(home.0.join(".pitboard")).expect("a pitboard home");
        let app = home
            .0
            .join("Applications/Pitboard.app/Contents/MacOS/Pitboard");
        let bundled = home
            .0
            .join("Applications/Pitboard.app/Contents/Helpers/pitboard");
        a_program_at(&app);
        a_program_at(&bundled);
        let ctx = Context::new(home.0.clone());
        let the_app = ctx.clone().with_schedule_program(bundled.clone());

        assert!(
            !repair(&the_app).expect("nothing to do"),
            "nothing installed"
        );

        install(&ctx.clone().with_schedule_program(app.clone())).expect("0.3.0's schedule");
        assert!(
            !repair(&ctx).expect("nothing to do"),
            "no command line named"
        );
        let gone = home.0.join("Old.app/Contents/Helpers/pitboard");
        assert!(
            !repair(&ctx.clone().with_schedule_program(gone)).expect("nothing to do"),
            "a command line that is not there"
        );
        assert!(
            !repair(&the_app.clone().with_pitboard_home(home.0.join("elsewhere")))
                .expect("nothing to do"),
            "a schedule another home's pitboard looks after"
        );
        assert_eq!(installed_program(&ctx), Some(app.clone()));

        assert!(repair(&the_app).expect("repaired"));
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
            !repair(&the_app).expect("nothing to do"),
            "a schedule that runs a command line already"
        );
        install(&ctx.clone().with_schedule_program(app.clone())).expect("the app again");
        let temporary = home
            .0
            .join("AppTranslocation/6A1C/d/Pitboard.app/Contents/Helpers/pitboard");
        a_program_at(&temporary);
        assert!(
            !repair(&ctx.clone().with_schedule_program(temporary)).expect("nothing to do"),
            "a command line that is gone once the app quits"
        );
        assert_eq!(installed_program(&ctx), Some(app));
    }

    /// launchd stops a job's own process when it unloads the job, and a repair unloads the
    /// schedule before loading it again. Made from inside the schedule's own job, as by an
    /// app the old schedule started, it would leave nothing loaded, so it is left to the app
    /// once it is opened any other way.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_schedule_is_not_repaired_from_inside_its_own_job() {
        let home = Scratch::new("inside");
        std::fs::create_dir_all(home.0.join(".pitboard")).expect("a pitboard home");
        let app = home
            .0
            .join("Applications/Pitboard.app/Contents/MacOS/Pitboard");
        let bundled = home
            .0
            .join("Applications/Pitboard.app/Contents/Helpers/pitboard");
        a_program_at(&app);
        a_program_at(&bundled);
        let ctx = Context::new(home.0.clone());
        install(&ctx.clone().with_schedule_program(app.clone())).expect("0.3.0's schedule");
        let the_app = ctx.clone().with_schedule_program(bundled.clone());

        assert!(
            !repair(&the_app.clone().with_launchd_job(LABEL.into())).expect("nothing to do"),
            "started by the schedule"
        );
        assert_eq!(installed_program(&ctx), Some(app));
        assert_eq!(
            crate::audit::read(&ctx, 1)
                .iter()
                .map(|e| e.subject.as_str())
                .collect::<Vec<_>>(),
            ["install"],
            "and nothing recorded"
        );

        let opened = the_app.with_launchd_job("application.com.datlechin.pitboard.1.2".into());
        assert!(repair(&opened).expect("repaired"), "opened from Finder");
        assert_eq!(installed_program(&ctx), Some(bundled));
    }

    /// Only the file `install` writes is read, and only the way it writes it.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_schedule_file_pitboard_did_not_write_names_no_program() {
        let home = Scratch::new("foreign");
        let ctx = Context::new(home.0.clone());
        let installed = path(&ctx).expect("a scheduler here");
        write(&installed, "not what install writes\n").expect("written");
        #[cfg(target_os = "linux")]
        write(
            &unit_dir(&ctx).join("pitboard-renew.service"),
            "[Service]\nExecStart=/bin/true\n",
        )
        .expect("written");

        assert!(matches!(status(&ctx), Installed::Yes { .. }));
        assert_eq!(installed_program(&ctx), None);
    }

    /// launchd and systemd start `renew` with the default home, so a pitboard pointed
    /// anywhere else leaves the schedule alone.
    #[test]
    fn the_schedule_belongs_to_the_default_home_alone() {
        let ctx = Context::new(PathBuf::from("/home/x"));
        assert!(serves(&ctx));
        assert!(serves(
            &ctx.clone()
                .with_pitboard_home(PathBuf::from("/home/x/.pitboard/"))
        ));
        assert!(!serves(
            &ctx.with_pitboard_home(PathBuf::from("/tmp/elsewhere"))
        ));
    }

    /// What it runs is one verb with no arguments, and what it does not do is as much the
    /// point as what it does.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_agent_runs_one_verb_and_does_not_run_at_load() {
        let body = plist(std::path::Path::new("/usr/local/bin/pitboard"));
        assert!(body.contains("<string>/usr/local/bin/pitboard</string>"));
        assert!(body.contains("<string>renew</string>"));
        assert!(!body.contains("status"), "it never asks for usage");
        assert!(!body.contains("use"), "and it never switches");
        assert!(
            body.contains("<key>RunAtLoad</key><false/>"),
            "installing it is not a reason to talk to Anthropic that second"
        );
        assert!(body.contains(&format!("<integer>{EVERY_SECONDS}</integer>")));
    }

    /// launchd refuses a plist that is not well formed, and a folder's name can hold any of
    /// the characters XML gives a meaning to.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_agent_writes_a_path_as_xml_text() {
        let body = plist(std::path::Path::new("/Users/x/A&B <old>/Pitboard.app"));
        assert!(
            body.contains("<string>/Users/x/A&amp;B &lt;old&gt;/Pitboard.app</string>"),
            "{body}"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_unit_runs_one_verb_and_the_timer_survives_a_machine_being_off() {
        let unit = service(std::path::Path::new("/usr/local/bin/pitboard"));
        assert!(unit.contains("ExecStart=/usr/local/bin/pitboard renew"));
        assert!(!unit.contains("status"));
        let timer = timer();
        assert!(timer.contains("Persistent=true"));
        assert!(timer.contains("WantedBy=timers.target"));
    }
}
