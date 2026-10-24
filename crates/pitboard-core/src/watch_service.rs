//! `pitboard watch` run by the system's own service manager; ARCHITECTURE.md has its rules.

use crate::context::Context;
use crate::duty::autoswitch::Threshold;
use crate::error::{Error, Result};
use crate::schedule::{self, Job, Location};
use crate::service::Permit;
use std::path::PathBuf;

/// Whether the watch service is installed, and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    /// Installed, where `location` says. `at` is the share it switches at, where its job says
    /// one the way Pitboard writes it.
    Yes {
        location: Location,
        at: Option<Threshold>,
    },
    No,
    /// This machine has no scheduler Pitboard knows how to ask.
    Unsupported,
}

pub(crate) fn status(ctx: &Context) -> Installed {
    match schedule::scheduler(ctx) {
        None => Installed::Unsupported,
        Some(s) if s.installed(ctx, Job::Watch) => Installed::Yes {
            location: s.location(ctx, Job::Watch),
            at: share(&s.options(ctx, Job::Watch)),
        },
        Some(_) => Installed::No,
    }
}

/// The Pitboard the installed service runs. `None` where nothing is installed, and where what
/// is there does not name one the way `install` writes it.
pub(crate) fn installed_program(ctx: &Context) -> Option<PathBuf> {
    schedule::scheduler(ctx)?.program(ctx, Job::Watch)
}

// Task Scheduler's word on the task; none from launchd or systemd.
pub(crate) fn task(ctx: &Context) -> Option<crate::host::task::Task> {
    schedule::task(ctx, Job::Watch)
}

/// Install it, switching at `at`, and ask the system to start it now. Returns where it went.
/// A reinstall replaces the one there, which the system starts again.
///
/// Refused with [`Error::ScheduleNotDefaultHome`] where this context is not the default home's
/// ([`schedule::serves`]): it would watch `~/.pitboard`'s accounts while `PITBOARD_HOME` named
/// others. `uninstall` is not refused for it, so the service can always be taken away.
pub(crate) fn install(ctx: &Context, permit: Permit, at: Threshold) -> Result<Location> {
    if !schedule::serves(ctx) {
        return Err(Error::ScheduleNotDefaultHome {
            home: crate::home::dir(ctx),
            default: ctx.default_pitboard_home(),
            job: Job::Watch,
        });
    }
    // A run of the service a reinstall ends is then between changes, never mid-renewal.
    let _quiet = crate::switch::exclusive(ctx, permit)?;
    let location = schedule::put(
        ctx,
        permit,
        Job::Watch,
        &schedule::program(ctx, Job::Watch)?,
        &["--at".to_string(), at.percent().to_string()],
        &schedule::environment(ctx, schedule::proxy_variables(ctx)),
    )?;
    crate::audit::record(ctx, permit, VERB, "install", "ok");
    Ok(location)
}

/// Stop it and take it away. `false` when there was nothing installed. Its run is ended only
/// while Pitboard's lock is held, so never in the middle of a change.
pub(crate) fn uninstall(ctx: &Context, permit: Permit) -> Result<bool> {
    let watched = ctx.clone().with_pitboard_home(ctx.default_pitboard_home());
    let _quiet = match schedule::scheduler(ctx) {
        Some(s) if s.installed(ctx, Job::Watch) => {
            Some(crate::switch::exclusive(&watched, permit)?)
        }
        _ => None,
    };
    taken_away(ctx, permit)
}

// `uninstall`, for a change that holds Pitboard's lock already.
pub(crate) fn taken_away(ctx: &Context, permit: Permit) -> Result<bool> {
    if !schedule::remove(ctx, permit, Job::Watch)? {
        return Ok(false);
    }
    crate::audit::record(ctx, permit, VERB, "uninstall", "ok");
    Ok(true)
}

const VERB: &str = "watch-service";

fn share(options: &[String]) -> Option<Threshold> {
    match options {
        [at, percent] if at == "--at" => percent.parse().ok().and_then(Threshold::new),
        _ => None,
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
    use std::sync::Arc;

    /// A home of the test's own on a machine whose scheduler asks nobody, with a Pitboard in
    /// it for the service to run.
    struct Machine(tempfile::TempDir, Arc<MemoryHost>);

    impl Machine {
        fn new() -> Machine {
            Machine::on(MemoryHost::new())
        }

        // As Windows schedules, through Task Scheduler as this machine has it, on any system.
        fn scheduling_tasks() -> Machine {
            Machine::on(MemoryHost::scheduling_tasks(SID))
        }

        fn on(host: Arc<MemoryHost>) -> Machine {
            let machine = Machine(tempfile::tempdir().expect("a scratch home"), host);
            std::fs::create_dir_all(machine.program().parent().expect("its folder")).expect("made");
            std::fs::write(machine.program(), "").expect("a program");
            std::fs::create_dir_all(machine.ctx(&[]).default_pitboard_home())
                .expect("a Pitboard home");
            machine
        }

        fn program(&self) -> PathBuf {
            self.0.path().join("bin").join("pitboard")
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

        fn written(&self, ctx: &Context) -> String {
            std::fs::read_to_string(schedule::job_path(ctx, Job::Watch).expect("a scheduler"))
                .expect("the job's file")
        }
    }

    const SID: &str = "S-1-5-21-1004-1005-1006-1001";

    fn at(percent: u8) -> Threshold {
        Threshold::new(percent).expect("a share there can be")
    }

    /// Installed, the service runs `pitboard watch` with its marker and the share it was
    /// installed at, says so, and is recorded; a reinstall replaces it, and it is taken away
    /// once, apart from the renewal schedule.
    #[test]
    fn the_service_runs_watch_at_its_share_until_it_is_taken_away() {
        for machine in [Machine::new(), Machine::scheduling_tasks()] {
            let ctx = machine.ctx(&[]);
            assert_eq!(status(&ctx), Installed::No);

            let location = install(&ctx, Permit::for_a_test(), at(90)).expect("installed");
            assert_eq!(
                status(&ctx),
                Installed::Yes {
                    location: location.clone(),
                    at: Some(at(90))
                }
            );
            assert_eq!(installed_program(&ctx), Some(machine.program()));
            let written = machine.written(&ctx);
            assert!(
                written.contains("watch --scheduled --at 90")
                    || written.contains(
                        "<string>watch</string>\n    <string>--scheduled</string>\n    \
                         <string>--at</string>\n    <string>90</string>"
                    ),
                "{written}"
            );
            assert_eq!(
                schedule::status(&ctx),
                schedule::Installed::No,
                "the renewal schedule is a job of its own"
            );

            install(&ctx, Permit::for_a_test(), at(80)).expect("installed again");
            assert_eq!(
                status(&ctx),
                Installed::Yes {
                    location,
                    at: Some(at(80))
                }
            );

            schedule::install(&ctx, Permit::for_a_test()).expect("renewal too");
            assert!(uninstall(&ctx, Permit::for_a_test()).expect("taken away"));
            assert_eq!(status(&ctx), Installed::No);
            assert!(!uninstall(&ctx, Permit::for_a_test()).expect("nothing there"));
            assert!(
                matches!(schedule::status(&ctx), schedule::Installed::Yes { .. }),
                "renewal stays"
            );
            let logged: Vec<(String, String)> = crate::audit::read(&ctx, 10)
                .into_iter()
                .filter(|entry| entry.verb == VERB)
                .map(|entry| (entry.subject, entry.outcome))
                .collect();
            assert_eq!(
                logged,
                [("install", "ok"), ("install", "ok"), ("uninstall", "ok")]
                    .map(|(subject, outcome)| (subject.to_string(), outcome.to_string()))
            );
        }
    }

    /// The service watches the default home's accounts alone, so it is installed only from
    /// there, and taken away from any home.
    #[test]
    fn the_service_is_installed_only_where_pitboard_home_is_the_default_home() {
        let machine = Machine::new();
        let ctx = machine.ctx(&[]);
        let default = ctx.default_pitboard_home();
        let elsewhere = machine.0.path().join("elsewhere");
        let another = ctx.clone().with_pitboard_home(elsewhere.clone());

        let refused = install(&another, Permit::for_a_test(), at(95)).expect_err("refused");
        assert_eq!(refused.code(), "schedule_not_default_home");
        assert_eq!(
            refused.to_string(),
            format!(
                "the watch service watches only the accounts in {}, not those in {}, which \
                 PITBOARD_HOME names. Nothing was installed. To install it for {}, unset \
                 PITBOARD_HOME and install it again.",
                default.display(),
                elsewhere.display(),
                default.display()
            )
        );
        assert_eq!(status(&ctx), Installed::No, "nothing was written");

        install(&ctx, Permit::for_a_test(), at(95)).expect("installed");
        assert!(uninstall(&another, Permit::for_a_test()).expect("taken away from elsewhere"));
        assert_eq!(status(&ctx), Installed::No);
    }

    /// The system starts the service with its own environment, so it is given the installing
    /// Pitboard's proxy variables as the renewal schedule is. A run of it watches the default
    /// home whatever `PITBOARD_HOME` the system hands it, with those variables over its own
    /// where the system hands it none, as Task Scheduler does.
    #[test]
    fn a_run_of_the_service_watches_the_default_home_with_the_proxy_it_was_given() {
        let proxied = [("HTTPS_PROXY", "http://alice:s3cret@proxy.example.com:3128")];
        for machine in [Machine::new(), Machine::scheduling_tasks()] {
            let ctx = machine.ctx(&proxied);
            install(&ctx, Permit::for_a_test(), at(95)).expect("installed");
            let handed = schedule::scheduler(&ctx)
                .expect("a scheduler")
                .environment(&ctx, Job::Watch);
            assert_eq!(
                handed,
                [(
                    "HTTPS_PROXY".to_string(),
                    "http://alice:s3cret@proxy.example.com:3128".to_string()
                )]
            );
            let tasks = matches!(
                schedule::job_path(&ctx, Job::Watch),
                Some(path) if path.starts_with(machine.0.path().join("Tasks"))
            );
            if tasks {
                let definition = machine.written(&ctx);
                assert!(!definition.contains("alice"), "{definition}");
            }

            let started = machine
                .ctx(&[])
                .with_pitboard_home(machine.0.path().join("elsewhere"))
                .started_by_the_watch_service();
            assert_eq!(started.pitboard_home, ctx.default_pitboard_home());
            if tasks {
                assert!(
                    started.proxy.same_way(&ctx.proxy),
                    "the file's, over its own"
                );
            }
            let by_hand = machine.ctx(&[]);
            assert_eq!(
                schedule::for_its_run(&by_hand, Job::Watch).pitboard_home,
                by_hand.pitboard_home,
                "a run by hand watches its own"
            );
        }
    }

    /// A service the system will not start leaves the one before as it was, or nothing.
    #[test]
    fn a_service_the_system_will_not_start_leaves_the_one_before() {
        for machine in [Machine::new(), Machine::scheduling_tasks()] {
            let ctx = machine.ctx(&[]);
            machine.1.refuse_next_start();
            let refused = install(&ctx, Permit::for_a_test(), at(95)).expect_err("refused");
            assert_eq!(refused.code(), "schedule_refused");
            assert_eq!(status(&ctx), Installed::No);

            let location = install(&ctx, Permit::for_a_test(), at(95)).expect("installed");
            machine.1.refuse_next_start();
            install(&ctx, Permit::for_a_test(), at(70)).expect_err("refused");
            assert_eq!(
                status(&ctx),
                Installed::Yes {
                    location,
                    at: Some(at(95))
                }
            );
        }
    }

    // Ended while a change is under way, the service's run could lose the login it was renewing.
    #[test]
    fn the_services_run_is_ended_only_between_changes() {
        for machine in [Machine::new(), Machine::scheduling_tasks()] {
            let ctx = machine.ctx(&[]);
            install(&ctx, Permit::for_a_test(), at(90)).expect("installed");
            for (reinstalling, before) in [(true, 90), (false, 80)] {
                let changing =
                    crate::switch::exclusive(&ctx, Permit::for_a_test()).expect("a change");
                let asked = ctx.clone();
                let waiting = std::thread::spawn(move || {
                    if reinstalling {
                        install(&asked, Permit::for_a_test(), at(80)).map(|_| true)
                    } else {
                        uninstall(&asked, Permit::for_a_test())
                    }
                });
                std::thread::sleep(std::time::Duration::from_millis(200));
                assert!(
                    !waiting.is_finished(),
                    "{reinstalling}: it waits for the change"
                );
                assert!(matches!(
                    status(&ctx),
                    Installed::Yes { at: share, .. } if share == Some(at(before))
                ));
                drop(changing);
                assert!(waiting.join().expect("it ends").expect("done"));
            }
            assert_eq!(status(&ctx), Installed::No);
        }
    }

    /// Where there is no scheduler, installing says what to do instead.
    #[test]
    fn a_machine_with_no_scheduler_says_to_leave_watch_running() {
        let machine = Machine::new();
        machine.1.without_a_scheduler();
        let ctx = machine.ctx(&[]);
        assert_eq!(status(&ctx), Installed::Unsupported);
        let refused = install(&ctx, Permit::for_a_test(), at(95)).expect_err("nowhere");
        assert_eq!(refused.code(), "schedule_unsupported");
        assert!(
            refused
                .to_string()
                .contains("leaving `pitboard watch` running"),
            "{refused}"
        );
        assert_eq!(
            uninstall(&ctx, Permit::for_a_test())
                .expect_err("nowhere")
                .code(),
            "schedule_unsupported"
        );
    }

    /// Only the share Pitboard writes is read back as one.
    #[test]
    fn only_a_share_written_as_pitboard_writes_it_is_read() {
        let options = |given: &[&str]| given.iter().map(|o| (*o).to_string()).collect::<Vec<_>>();
        assert_eq!(share(&options(&["--at", "90"])), Some(at(90)));
        for unread in [
            &["--at", "100"][..],
            &["--at", "ninety"],
            &["--at"],
            &[],
            &["--at", "90", "--once"],
        ] {
            assert_eq!(share(&options(unread)), None, "{unread:?}");
        }
    }
}
