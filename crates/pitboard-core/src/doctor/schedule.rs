use super::network::{NetworkFact, ProxyFact, network_fact};
use super::{Check, Facts, Level, fail, ok, warn};
use crate::context::Context;
use crate::host::Os;
use crate::host::task::{Drift, Task};
use crate::schedule::Fault;
use crate::words;
use std::path::PathBuf;

/// The daily renewal schedule as it is installed, read from what Pitboard wrote: a file, or
/// on Windows the task as Task Scheduler keeps it.
pub(crate) struct ScheduleFact {
    pub(crate) location: crate::schedule::Location,
    // Task Scheduler's word on its runs and its settings; none from launchd or systemd.
    pub(crate) task: Option<Task>,
    /// The Pitboard it runs, where the file names one the way Pitboard writes it.
    pub(crate) program: Option<PathBuf>,
    /// Whether that Pitboard is still there to be run.
    pub(crate) program_found: bool,
    /// How its runs send requests, as the proxy variables its file gives them say: those of
    /// the Pitboard that installed it, read as this run's are ([`Facts::network`]).
    pub(crate) network: NetworkFact,
    /// Whether its runs send requests another way than this run does: through another proxy
    /// or none, with another user name or password, or with other hosts exempt.
    pub(crate) network_differs: bool,
}

pub(crate) struct WatchServiceFact {
    pub(crate) location: crate::schedule::Location,
    pub(crate) program: Option<PathBuf>,
    pub(crate) program_found: bool,
    pub(crate) at: Option<crate::duty::autoswitch::Threshold>,
    // Task Scheduler's word on its runs and its settings; none from launchd or systemd.
    pub(crate) task: Option<Task>,
}

/// The schedule, where this home has one.
pub(super) fn schedule_fact(ctx: &Context) -> Option<ScheduleFact> {
    if !crate::schedule::serves(ctx) {
        return None;
    }
    let crate::schedule::Installed::Yes { location, .. } = crate::schedule::status(ctx) else {
        return None;
    };
    let program = crate::schedule::installed_program(ctx);
    let proxies = crate::schedule::installed_proxies(ctx).unwrap_or_default();
    Some(ScheduleFact {
        program_found: program.as_deref().is_some_and(std::path::Path::is_file),
        program,
        location,
        task: crate::schedule::task(ctx, crate::schedule::Job::Renew),
        network: network_fact(&proxies),
        network_differs: !proxies.same_way(&ctx.proxy),
    })
}

pub(super) fn watch_service_fact(ctx: &Context) -> Option<WatchServiceFact> {
    if !crate::schedule::serves(ctx) {
        return None;
    }
    let crate::watch_service::Installed::Yes { location, at } = crate::watch_service::status(ctx)
    else {
        return None;
    };
    let program = crate::watch_service::installed_program(ctx);
    Some(WatchServiceFact {
        program_found: program.as_deref().is_some_and(std::path::Path::is_file),
        program,
        location,
        at,
        task: crate::watch_service::task(ctx),
    })
}

/// The schedule runs a Pitboard by its path, and the path can stop leading anywhere after
/// it was written: an upgrade that deletes the version it named, an app moved or thrown
/// away. The scheduler then fails once a day where nobody looks, and the parked logins it
/// was keeping alive run out. Nothing is said where there is no schedule.
///
/// An app up to 0.3.0 scheduled itself rather than a command line, and that app renews
/// nothing when it is started with `renew`, so that is said as well.
pub(super) fn judge_schedule(facts: &Facts) -> Option<Check> {
    let again = again(facts.os);
    let schedule = facts.schedule.as_ref()?;
    let Some(program) = &schedule.program else {
        return Some(warn(
            "schedule",
            "renewal schedule",
            format!("{} does not say which Pitboard it runs", schedule.location),
            again,
        ));
    };
    match crate::schedule::fault(program, schedule.program_found) {
        Some(Fault::Gone) => {
            return Some(fail(
                "schedule",
                "renewal schedule",
                format!("runs {}, which is not there any more", program.display()),
                again,
            ));
        }
        Some(Fault::TheApp) => {
            return Some(fail(
                "schedule",
                "renewal schedule",
                format!(
                    "runs {}, which is the app itself and not a command line",
                    program.display()
                ),
                again,
            ));
        }
        None => {}
    }
    let runs = format!("daily  ·  runs {}", program.display());
    let Some(task) = &schedule.task else {
        return Some(ok("schedule", "renewal schedule", runs));
    };
    if !task.drifted.is_empty() {
        let drifted = task.drifted.iter().map(drift);
        return Some(warn(
            "schedule",
            "renewal schedule",
            format!(
                "{runs}, but in Task Scheduler it {}",
                words::listed(drifted)
            ),
            again,
        ));
    }
    let ago = |at: i64| words::span(facts.now - at);
    Some(match (task.failed(), task.last_run) {
        (Some(result), at) => warn(
            "schedule",
            "renewal schedule",
            format!(
                "{runs}, and its last run{} ended {}",
                at.map(|at| format!(", {} ago,", ago(at)))
                    .unwrap_or_default(),
                ended(result)
            ),
            "Run `pitboard renew` to see what it says.",
        ),
        (None, Some(at)) => ok(
            "schedule",
            "renewal schedule",
            format!("{runs}  ·  last ran {} ago", ago(at)),
        ),
        (None, None) => ok(
            "schedule",
            "renewal schedule",
            format!("{runs}  ·  not run yet"),
        ),
    })
}

// Nothing is said where there is no service. A Pitboard gone from its path or a last run that
// failed stops every switch it would make, where nobody looks.
pub(super) fn judge_watch_service(facts: &Facts) -> Option<Check> {
    let service = facts.watch_service.as_ref()?;
    let again = "Install it again with `pitboard watch install`.";
    let Some(program) = &service.program else {
        return Some(warn(
            "watch_service",
            "watch service",
            format!("{} does not say which Pitboard it runs", service.location),
            again,
        ));
    };
    if !service.program_found {
        return Some(fail(
            "watch_service",
            "watch service",
            format!("runs {}, which is not there any more", program.display()),
            again,
        ));
    }
    let runs = match service.at {
        Some(at) => format!(
            "from sign-in  ·  switches at {}%  ·  runs {}",
            at.percent(),
            program.display()
        ),
        None => format!("from sign-in  ·  runs {}", program.display()),
    };
    Some(match service.task.as_ref().and_then(Task::failed) {
        Some(result) => warn(
            "watch_service",
            "watch service",
            format!("{runs}, and its last run ended {}", ended(result)),
            "Run `pitboard watch` to see what it says.",
        ),
        None => ok("watch_service", "watch service", runs),
    })
}

// As Task Scheduler's own settings word each.
pub(super) fn drift(drift: &Drift) -> String {
    match drift {
        Drift::OnAcPowerOnly => "starts only if the computer is on AC power",
        Drift::StopsOnBattery => "stops if the computer switches to battery power",
        Drift::SkipsMissedStarts => {
            "does not run as soon as possible after a scheduled start is missed"
        }
        Drift::Disabled => "is disabled",
    }
    .to_string()
}

// A program's exit code, or one of Task Scheduler's own results.
pub(super) fn ended(result: i32) -> String {
    if (0..=255).contains(&result) {
        format!("with exit code {result}")
    } else {
        format!("with 0x{:08X}", result as u32)
    }
}

/// The system starts the schedule's runs with its own environment, not the person's shell,
/// so they are given the proxy variables of the Pitboard that installed it. Where those send
/// requests another way than this run does, through another proxy or none, with another user
/// name or password, or with other hosts exempt, that is said, credentials as `***`. Nothing
/// is said where they go the same way, or where there is no schedule.
///
/// From a terminal it is a warning: the terminal's proxy is the one the person works with,
/// and installing the schedule again from there gives it that proxy. Where that proxy is a
/// SOCKS one whose requests fail, the advice is to change the variable naming it first, so
/// the schedule is not given a proxy that fails its requests too. In the app it is no
/// warning.
/// The app is started with the system's environment as the schedule's runs are, which on
/// macOS names no proxy unless `launchctl setenv` set one, so a schedule installed from a terminal with
/// a proxy differs from it for good, and the app's switch, the one way it installs the
/// schedule, would give the schedule the app's proxy variables and take the terminal's away.
/// There the line says what each goes through, and how each way of installing it chooses.
pub(super) fn judge_schedule_proxy(facts: &Facts) -> Option<Check> {
    let schedule = facts.schedule.as_ref()?;
    if !schedule.network_differs {
        return None;
    }
    let here = if facts.in_the_app {
        "the app"
    } else {
        "this run"
    };
    let installed = match &schedule.network.proxy {
        None => "the schedule was installed with no proxy".to_string(),
        Some(proxy) => format!("the schedule was installed with {}", proxy_named(proxy)),
    };
    // A SOCKS proxy is not gone through, so it is said to be given, and the network check
    // says what becomes of the requests.
    let this_run = match &facts.network.proxy {
        None => format!("{here} goes directly"),
        Some(proxy) if proxy.socks => format!("{here} is given {}", proxy_named(proxy)),
        Some(proxy) => format!("{here} goes through {}", proxy_named(proxy)),
    };
    let detail = match (&schedule.network.proxy, &facts.network.proxy) {
        // Everything shown is the same, so what differs is what is never shown.
        (Some(its), Some(mine)) if proxy_named(its) == proxy_named(mine) => format!(
            "the schedule was installed with {here}'s proxy, {}, but with another user name \
             or password",
            proxy_named(its)
        ),
        _ => format!("{installed}; {this_run}"),
    };
    if facts.in_the_app {
        return Some(Check {
            code: "schedule_proxy",
            name: "renewal schedule's proxy".into(),
            level: Level::Ok,
            detail,
            advice: "The app runs with the environment the system started it with, not a \
                     terminal's. Turning daily renewal off and on in Settings gives the \
                     schedule the app's proxy variables; `pitboard schedule install` in a \
                     terminal gives it that terminal's."
                .into(),
        });
    }
    // Installed again from here, the schedule would be given a SOCKS proxy that fails its
    // requests, so the variable is to be changed first.
    let advice = match &facts.network.proxy {
        Some(proxy) if proxy.fails => format!(
            "The schedule's runs are given the proxy variables it was installed with. \
             Installed again from here, they would be given this run's SOCKS proxy, which \
             Pitboard does not use yet, and their requests would fail. Set {} to an HTTP \
             proxy's address, or unset it, then install it again with `pitboard schedule \
             install`, which takes the proxy variables set where it runs.",
            proxy.variable
        ),
        _ => "The schedule's runs are given the proxy variables it was installed with. To give \
              them this run's, install it again from here with `pitboard schedule install`, \
              which takes the proxy variables set where it runs."
            .to_string(),
    };
    Some(warn(
        "schedule_proxy",
        "renewal schedule's proxy",
        detail,
        advice,
    ))
}

/// A proxy as the schedule's line names it: its address, the variable it came from, and the
/// hosts Pitboard calls that go around it, where any do.
pub(super) fn proxy_named(proxy: &ProxyFact) -> String {
    let named = format!("{}, from {}", proxy.address, proxy.variable);
    match proxy.exempting {
        Some(exempting) if !proxy.exempt.is_empty() => format!(
            "{named}, {exempting} exempting {}",
            words::listed(proxy.exempt.clone())
        ),
        _ => named,
    }
}

/// How to write the schedule again.
pub(super) fn again(os: Os) -> &'static str {
    if has_app(os) {
        "Turn daily renewal off and on again: in the app's Settings, or with \
         `pitboard schedule uninstall` and then `pitboard schedule install`."
    } else {
        "Turn daily renewal off and on again with `pitboard schedule uninstall` and then \
         `pitboard schedule install`."
    }
}

/// Whether Pitboard has an app for `os`, with Settings of its own.
pub(super) fn has_app(os: Os) -> bool {
    match os {
        Os::MacOs | Os::Windows => true,
        Os::Linux => false,
    }
}
