//! Pitboard's jobs on Linux: a systemd user timer for daily renewal and a systemd user service
//! for the watch service, which is that system's answer.

use super::super::unix::service::{self, Control};
use crate::context::Context;
use crate::error::Result;
use crate::host::Scheduler;
use crate::schedule::{EVERY_SECONDS, Job, Location, RESTART_AFTER_SECONDS};
use crate::service::Permit;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const SYSTEMCTL: &str = "systemctl";
const TIMER: &str = "pitboard-renew.timer";
const SERVICE: &str = "pitboard-renew.service";
const WATCH: &str = "pitboard-watch.service";
const RELOAD: [&str; 2] = ["--user", "daemon-reload"];

#[derive(Debug)]
pub(super) struct Systemd {
    control: Arc<dyn Control>,
}

impl Systemd {
    pub(super) fn new(control: Arc<dyn Control>) -> Systemd {
        Systemd { control }
    }

    fn units(ctx: &Context) -> PathBuf {
        ctx.home().join(".config/systemd/user")
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

    fn read(ctx: &Context, job: Job) -> Option<String> {
        if !Self::units(ctx).join(Self::installed_by(job)).is_file() {
            return None;
        }
        std::fs::read_to_string(Self::units(ctx).join(Self::service(job))).ok()
    }

    // Each `systemctl --user` call that starts the job, in order.
    fn starts(job: Job) -> &'static [&'static [&'static str]] {
        match job {
            Job::Renew => &[&["--user", "enable", "--now", TIMER]],
            // `restart` starts a stopped service, and one already running with the new unit.
            Job::Watch => &[&["--user", "enable", WATCH], &["--user", "restart", WATCH]],
        }
    }

    fn start(&self, permit: Permit, job: Job) -> Result<()> {
        Self::starts(job)
            .iter()
            .try_for_each(|args| self.control.start(permit, SYSTEMCTL, args))
    }
}

// sd_booted(3) asks the same: systemd as init makes this directory as the system starts.
pub(super) fn started_by_systemd(root: &Path) -> bool {
    root.join("run/systemd/system").is_dir()
}

impl Scheduler for Systemd {
    fn location(&self, ctx: &Context, job: Job) -> Location {
        Location::File(Self::units(ctx).join(Self::installed_by(job)))
    }

    fn installed(&self, ctx: &Context, job: Job) -> bool {
        Self::units(ctx).join(Self::installed_by(job)).is_file()
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
        let unit = Self::units(ctx).join(Self::service(job));
        let timer_path = Self::units(ctx).join(TIMER);
        let before = (
            std::fs::read_to_string(&unit).ok(),
            std::fs::read_to_string(&timer_path).ok(),
        );
        service::write(
            permit,
            &unit,
            &unit_file(job, program, options, environment),
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
        if job == Job::Watch && !was_there {
            let _ = self
                .control
                .run(permit, SYSTEMCTL, &["--user", "disable", WATCH]);
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

    fn remove(&self, ctx: &Context, permit: Permit, job: Job) -> Result<bool> {
        let installed_by = Self::units(ctx).join(Self::installed_by(job));
        if !installed_by.is_file() {
            return Ok(false);
        }
        let stopped = self.control.run(
            permit,
            SYSTEMCTL,
            &["--user", "disable", "--now", Self::installed_by(job)],
        );
        // A watch service left running writes in the Pitboard directory `pitboard uninstall`
        // removes next.
        if job == Job::Watch {
            stopped?;
        }
        service::remove(permit, &installed_by)?;
        service::remove(permit, &Self::units(ctx).join(Self::service(job)))?;
        let _ = self.control.run(permit, SYSTEMCTL, &RELOAD);
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
}

/// The program and the options a service's `ExecStart` runs, as [`unit_file`] writes it, or
/// as Pitboard wrote renewal's before its job carried the marker.
fn command(body: &str, job: Job) -> Option<(PathBuf, Vec<String>)> {
    let run = body
        .lines()
        .find_map(|line| line.strip_prefix("ExecStart="))?;
    let marked = format!(" {}", job.marked().join(" "));
    let (program, options) = run.rsplit_once(marked.as_str()).or_else(|| match job {
        Job::Renew => run.strip_suffix(" renew").map(|program| (program, "")),
        Job::Watch => None,
    })?;
    let options = options.split_whitespace().map(str::to_owned).collect();
    Some((PathBuf::from(program), options))
}

/// systemd's own format. `environment` is as for the launchd job: the service runs with
/// systemd's environment, so what it is given of the installing Pitboard's,
/// `PITBOARD_NO_ARGV` and the proxy variables, is written in, one `Environment=` line each.
/// It runs `<program>` with the job's marker, which a run of the job is told apart by rather
/// than by what systemd passes it, and `options`. A Pitboard from before renewal's marker
/// rejects the flag, so after going back to one the timer fails every day until that
/// Pitboard's `schedule install` rewrites the unit, which the CHANGELOG says.
///
/// The watch service starts with the person's systemd, at their first sign-in, and again a
/// minute after it fails or crashes: a panic exits 101, and a run an update left behind exits
/// on a state file the new build wrote.
fn unit_file(
    job: Job,
    program: &Path,
    options: &[String],
    environment: &[(String, String)],
) -> String {
    let environment = environment
        .iter()
        .map(|(name, value)| assignment(name, value))
        .collect::<String>();
    let run = [job.marked().join(" "), options.join(" ")].join(" ");
    let run = run.trim_end();
    match job {
        Job::Renew => format!(
            "[Unit]\n\
             Description=Renew Pitboard's parked logins\n\
             Documentation=https://docs.usepitboard.com\n\
             \n\
             [Service]\n\
             Type=oneshot\n\
             {environment}\
             ExecStart={} {run}\n",
            program.display(),
        ),
        Job::Watch => format!(
            "[Unit]\n\
             Description=Switch Claude Code before an account runs out, and warm up accounts\n\
             Documentation=https://docs.usepitboard.com\n\
             \n\
             [Service]\n\
             {environment}\
             ExecStart={} {run}\n\
             Restart=on-failure\n\
             RestartSec={RESTART_AFTER_SECONDS}\n\
             \n\
             [Install]\n\
             WantedBy=default.target\n",
            program.display(),
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

fn timer() -> String {
    format!(
        "[Unit]\n\
         Description=Renew Pitboard's parked logins daily\n\
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

    /// The service runs with systemd's environment, not the shell's, so what it is given of
    /// the installing Pitboard's, `PITBOARD_NO_ARGV` and the proxy variables, is written in,
    /// one `Environment=` line each, quoted where systemd would otherwise split or change it,
    /// and nothing is written where there is nothing to give. What was written reads back as
    /// it was.
    #[test]
    fn the_unit_gives_its_service_the_variables_it_is_handed_and_reads_them_back() {
        let program = Path::new("/usr/local/bin/pitboard");
        let handed = pairs(&[
            ("PITBOARD_NO_ARGV", "1"),
            ("ALL_PROXY", "socks5h://alice:p%40ss\"w\\rd@127.0.0.1:1080"),
            ("HTTPS_PROXY", "http://proxy.example.com:3128"),
            ("no_proxy", ""),
            ("NO_PROXY", "*.anthropic.com, localhost"),
        ]);
        let unit = unit_file(Job::Renew, program, &[], &handed);
        assert!(
            unit.contains(
                "Type=oneshot\n\
                 Environment=PITBOARD_NO_ARGV=1\n\
                 Environment=\"ALL_PROXY=socks5h://alice:p%%40ss\\\"w\\\\rd@127.0.0.1:1080\"\n\
                 Environment=HTTPS_PROXY=http://proxy.example.com:3128\n\
                 Environment=no_proxy=\n\
                 Environment=\"NO_PROXY=*.anthropic.com, localhost\"\n\
                 ExecStart=/usr/local/bin/pitboard renew --scheduled\n"
            ),
            "{unit}"
        );
        assert_eq!(given(&unit), handed);
        assert!(!unit_file(Job::Renew, program, &[], &[]).contains("Environment="));
        assert_eq!(given(&unit_file(Job::Renew, program, &[], &[])), Vec::new());

        // A line break in a value never ends the line it is on.
        let broken = pairs(&[("NO_PROXY", "a\nExecStart=/bin/false\u{85}")]);
        let unit = unit_file(Job::Renew, program, &[], &broken);
        assert!(
            unit.contains("Environment=\"NO_PROXY=a\\x0aExecStart=/bin/false\\u0085\"\n"),
            "{unit}"
        );
        assert_eq!(
            unit.lines().filter(|l| l.starts_with("ExecStart=")).count(),
            1,
            "{unit}"
        );
        assert_eq!(given(&unit), broken);
    }

    // From the person's first sign-in, and again a minute after it fails or crashes.
    #[test]
    fn the_watch_service_starts_with_the_users_systemd_and_reads_back_its_share() {
        let at = ["--at".to_string(), "90".to_string()];
        let unit = unit_file(Job::Watch, Path::new("/usr/local/bin/pitboard"), &at, &[]);
        assert!(
            unit.contains(
                "[Service]\n\
                 ExecStart=/usr/local/bin/pitboard watch --scheduled --at 90\n\
                 Restart=on-failure\n\
                 RestartSec=60\n\
                 \n\
                 [Install]\n\
                 WantedBy=default.target\n"
            ),
            "{unit}"
        );
        assert!(!unit.contains("Type=oneshot"), "{unit}");
        assert_eq!(
            command(&unit, Job::Watch),
            Some((PathBuf::from("/usr/local/bin/pitboard"), at.to_vec()))
        );
        assert_eq!(command(&unit, Job::Renew), None, "not renewal's");
    }

    /// The service runs one verb, with the marker a run of the schedule is told apart by,
    /// rather than by what systemd passes it.
    #[test]
    fn the_unit_runs_one_verb_and_the_timer_survives_a_machine_being_off() {
        let unit = unit_file(Job::Renew, Path::new("/usr/local/bin/pitboard"), &[], &[]);
        assert!(
            unit.contains("ExecStart=/usr/local/bin/pitboard renew --scheduled\n"),
            "{unit}"
        );
        assert!(!unit.contains("status"));
        let timer = timer();
        assert!(timer.contains("Persistent=true"));
        assert!(timer.contains("WantedBy=timers.target"));
    }

    /// A unit Pitboard wrote before its job carried the marker still names the Pitboard it
    /// runs, and installing again writes the marker in. A test says the job by the unit's
    /// own name; nothing systemd passes is read.
    #[test]
    fn a_unit_written_without_the_marker_still_names_its_program_and_install_adds_it() {
        let home = std::env::temp_dir().join(format!(
            "pitboard-systemd-marker-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
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

        let systemd = Systemd::new(Arc::new(service::Pretend {
            refuse_start: Arc::default(),
        }));
        assert!(systemd.started_this_process(Job::Renew, Some(SERVICE)));
        assert!(!systemd.started_this_process(Job::Renew, Some(TIMER)));
        assert!(
            !systemd.started_this_process(Job::Renew, None),
            "what systemd passes is not read"
        );
        let _ = std::fs::remove_dir_all(&home);
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
                return Err(crate::error::Error::ScheduleRefused {
                    detail: format!("systemctl refused {call} in a test"),
                });
            }
            Ok(())
        }
    }

    fn scratch_home(name: &str) -> (PathBuf, Context) {
        let home = std::env::temp_dir().join(format!(
            "pitboard-systemd-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("a scratch home");
        let ctx = Context::new(home.clone()).with_memory_stores(MemoryHost::new());
        (home, ctx)
    }

    // Enabled but not running, a new service would start at the next sign-in all the same; one
    // before is restarted as it was.
    #[test]
    fn a_watch_service_systemd_will_not_restart_is_taken_away_or_put_back() {
        let (home, ctx) = scratch_home("restart");
        let asked = Arc::new(Asked::default());
        let systemd = Systemd::new(asked.clone());
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
        let _ = std::fs::remove_dir_all(&home);
    }

    // A watch service left running would write the Pitboard directory again once `pitboard
    // uninstall` removed it; renewal's units go whatever systemd says.
    #[test]
    fn a_watch_service_systemd_will_not_stop_stays_installed() {
        let (home, ctx) = scratch_home("stop");
        let asked = Arc::new(Asked::default());
        let systemd = Systemd::new(asked.clone());
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
        let _ = std::fs::remove_dir_all(&home);
    }

    /// `pitboard schedule install` writes the proxy variables it was started with into the
    /// service, and the service and its timer only the person can read, mode 600, since a
    /// proxy's address can hold a password: over a service that was open to others too,
    /// which kept its mode when Pitboard wrote it before.
    #[test]
    fn an_installed_unit_carries_the_proxy_and_only_its_owner_can_read_it() {
        let home = std::env::temp_dir().join(format!(
            "pitboard-systemd-proxy-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
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
        let _ = std::fs::remove_dir_all(&home);
    }
}
