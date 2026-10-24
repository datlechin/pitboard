use super::network::{NetworkFact, network_fact};
use super::{Check, Facts, Level, fail, ok, warn, words};
use crate::context::Context;
use crate::host::Os;
use crate::host::task::Task;
use crate::schedule::Fault;
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
            words::does_not_say_which_pitboard(&schedule.location),
            again,
        ));
    };
    match crate::schedule::fault(program, schedule.program_found) {
        Some(Fault::Gone) => {
            return Some(fail(
                "schedule",
                "renewal schedule",
                words::runs_what_is_gone(program.display()),
                again,
            ));
        }
        Some(Fault::TheApp) => {
            return Some(fail(
                "schedule",
                "renewal schedule",
                words::runs_the_app(program.display()),
                again,
            ));
        }
        None => {}
    }
    let runs = words::runs_daily(program.display());
    let Some(task) = &schedule.task else {
        return Some(ok("schedule", "renewal schedule", runs));
    };
    if !task.drifted.is_empty() {
        let drifted: Vec<String> = task.drifted.iter().map(words::drift).collect();
        return Some(warn(
            "schedule",
            "renewal schedule",
            words::drifted_in_task_scheduler(&runs, &drifted),
            again,
        ));
    }
    let ago = |at: i64| facts.now - at;
    Some(match (task.failed(), task.last_run) {
        (Some(result), at) => warn(
            "schedule",
            "renewal schedule",
            words::last_run_ended(&runs, at.map(ago), result),
            words::RENEW_TO_SEE_WHY,
        ),
        (None, Some(at)) => ok(
            "schedule",
            "renewal schedule",
            words::last_ran(&runs, ago(at)),
        ),
        (None, None) => ok("schedule", "renewal schedule", words::not_run_yet(&runs)),
    })
}

// Nothing is said where there is no service. A Pitboard gone from its path or a last run that
// failed stops every switch it would make, where nobody looks.
pub(super) fn judge_watch_service(facts: &Facts) -> Option<Check> {
    let service = facts.watch_service.as_ref()?;
    let again = words::INSTALL_THE_SERVICE_AGAIN;
    let Some(program) = &service.program else {
        return Some(warn(
            "watch_service",
            "watch service",
            words::does_not_say_which_pitboard(&service.location),
            again,
        ));
    };
    if !service.program_found {
        return Some(fail(
            "watch_service",
            "watch service",
            words::runs_what_is_gone(program.display()),
            again,
        ));
    }
    let runs = words::runs_from_sign_in(service.at.map(|at| at.percent()), program.display());
    Some(match service.task.as_ref().and_then(Task::failed) {
        Some(result) => warn(
            "watch_service",
            "watch service",
            words::last_run_ended(&runs, None, result),
            words::WATCH_TO_SEE_WHY,
        ),
        None => ok("watch_service", "watch service", runs),
    })
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
    let here = words::this_side(facts.in_the_app);
    let installed = match &schedule.network.proxy {
        None => words::INSTALLED_WITH_NO_PROXY.to_string(),
        Some(proxy) => words::installed_with(&words::proxy_named(proxy)),
    };
    // A SOCKS proxy is not gone through, so it is said to be given, and the network check
    // says what becomes of the requests.
    let this_run = match &facts.network.proxy {
        None => words::goes_directly(here),
        Some(proxy) if proxy.socks => words::is_given(here, &words::proxy_named(proxy)),
        Some(proxy) => words::goes_through(here, &words::proxy_named(proxy)),
    };
    let detail = match (&schedule.network.proxy, &facts.network.proxy) {
        // Everything shown is the same, so what differs is what is never shown.
        (Some(its), Some(mine)) if words::proxy_named(its) == words::proxy_named(mine) => {
            words::another_user_or_password(here, &words::proxy_named(its))
        }
        _ => words::installed_and_here(&installed, &this_run),
    };
    if facts.in_the_app {
        return Some(Check {
            code: "schedule_proxy",
            name: "renewal schedule's proxy".into(),
            level: Level::Ok,
            detail,
            advice: words::THE_APPS_OWN_ENVIRONMENT.into(),
        });
    }
    // Installed again from here, the schedule would be given a SOCKS proxy that fails its
    // requests, so the variable is to be changed first.
    let advice = match &facts.network.proxy {
        Some(proxy) if proxy.fails => words::socks_would_fail(proxy.variable),
        _ => words::INSTALL_AGAIN_FROM_HERE.to_string(),
    };
    Some(warn(
        "schedule_proxy",
        "renewal schedule's proxy",
        detail,
        advice,
    ))
}

/// How to write the schedule again.
pub(super) fn again(os: Os) -> &'static str {
    if has_app(os) {
        words::AGAIN_IN_THE_APP_OR_A_TERMINAL
    } else {
        words::AGAIN_IN_A_TERMINAL
    }
}

/// Whether Pitboard has an app for `os`, with Settings of its own.
pub(super) fn has_app(os: Os) -> bool {
    match os {
        Os::MacOs | Os::Windows => true,
        Os::Linux => false,
    }
}
