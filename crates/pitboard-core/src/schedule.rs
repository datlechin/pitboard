//! Keeping parked logins alive without anybody running a command.
//!
//! A refresh token has a finite life, and the only two things that renew one are somebody
//! typing `pitboard` and the menu bar app's poll. So the tool is safe for a macOS user who
//! installed the cask and leaves it running, and quietly unsafe for everyone else: the
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
        return Ok(program.to_path_buf());
    }
    std::env::current_exe().map_err(|source| Error::HomeUnwritable {
        path: PathBuf::from("the running pitboard"),
        source,
    })
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
    let program = program(ctx)?;
    let Some(path) = path(ctx) else {
        return Err(Error::ScheduleUnsupported);
    };

    #[cfg(target_os = "macos")]
    {
        write(&path, &plist(&program))?;
        // `bootstrap` is launchd's own word for this, and replaces the deprecated `load`.
        // SAFETY: `getuid` cannot fail and touches no memory of this process.
        let uid = unsafe { libc::getuid() };
        let domain = format!("gui/{uid}");
        let _ = run(
            "/bin/launchctl",
            &["bootout", &domain, &path.to_string_lossy()],
        );
        run(
            "/bin/launchctl",
            &["bootstrap", &domain, &path.to_string_lossy()],
        )?;
    }

    #[cfg(target_os = "linux")]
    {
        let dir = unit_dir(ctx);
        write(&dir.join("pitboard-renew.service"), &service(&program))?;
        write(&path, &timer())?;
        let _ = run("systemctl", &["--user", "daemon-reload"]);
        run(
            "systemctl",
            &["--user", "enable", "--now", "pitboard-renew.timer"],
        )?;
    }

    crate::audit::record(ctx, "schedule", "install", "ok");
    Ok(path)
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
fn run(_program: &str, _args: &[&str]) -> Result<()> {
    Ok(())
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
         Description=Renew pitboard's parked Claude Code logins\n\
         Documentation=https://usepitboard.com\n\
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
         Description=Renew pitboard's parked Claude Code logins daily\n\
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
        let ctx = Context::new(PathBuf::from("/home/x"));
        assert_eq!(
            program(&ctx).expect("this program"),
            std::env::current_exe().expect("this test's own program"),
            "the command line schedules itself"
        );
        let bundled = PathBuf::from("/Applications/Pitboard.app/Contents/Helpers/pitboard");
        assert_eq!(
            program(&ctx.with_schedule_program(bundled.clone())).expect("the named one"),
            bundled,
            "an app schedules the command line it comes with"
        );
    }

    /// What `pitboard doctor` reads back is what was written, including a path launchd's
    /// format has to escape.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn an_installed_schedule_says_which_pitboard_it_runs() {
        let home = Scratch::new("installed");
        let bundled = home
            .0
            .join("Tools&Apps/Pitboard.app/Contents/Helpers/pitboard");
        let ctx = Context::new(home.0.clone()).with_schedule_program(bundled.clone());

        install(&ctx).expect("installed");
        assert!(matches!(status(&ctx), Installed::Yes { .. }));
        assert_eq!(installed_program(&ctx), Some(bundled));

        assert!(uninstall(&ctx).expect("taken away"));
        assert_eq!(status(&ctx), Installed::No);
        assert_eq!(installed_program(&ctx), None);
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
