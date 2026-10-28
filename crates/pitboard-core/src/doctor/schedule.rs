use super::network::{NetworkFact, network_fact};
use super::{Check, Facts, Level, fail, ok, warn, words};
use crate::context::Context;
use crate::host::Os;
use crate::host::task::Task;
use crate::schedule::Fault;
use std::path::PathBuf;

pub(crate) struct ScheduleFact {
    pub(crate) location: crate::schedule::Location,
    // Task Scheduler's word on its runs and its settings; none from launchd or systemd.
    pub(crate) task: Option<Task>,
    pub(crate) program: Option<PathBuf>,
    pub(crate) program_found: bool,
    // The proxy variables of the Pitboard that installed it.
    pub(crate) network: NetworkFact,
    // Another proxy, user name, password or exempt host than this run's.
    pub(crate) network_differs: bool,
    // Written by an earlier Pitboard, as `Scheduler::outdated` reads it.
    pub(crate) outdated: bool,
    // What the store refused a run of it, until a run of it opens the store.
    pub(crate) refused: Option<crate::schedule::Refused>,
}

pub(crate) struct WatchServiceFact {
    pub(crate) location: crate::schedule::Location,
    pub(crate) program: Option<PathBuf>,
    pub(crate) program_found: bool,
    pub(crate) at: Option<crate::duty::autoswitch::Threshold>,
    // Task Scheduler's word on its runs and its settings; none from launchd or systemd.
    pub(crate) task: Option<Task>,
    pub(crate) outdated: bool,
}

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
        outdated: crate::schedule::outdated(ctx, crate::schedule::Job::Renew),
        refused: crate::schedule::refused(ctx),
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
        outdated: crate::schedule::outdated(ctx, crate::schedule::Job::Watch),
    })
}

// A path an upgrade or a move took away fails once a day where nobody looks.
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
    match crate::schedule::fault(facts.os, program, schedule.program_found) {
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
    if schedule.outdated {
        return Some(warn(
            "schedule",
            "renewal schedule",
            words::renews_by_time_awake(&runs),
            again,
        ));
    }
    // A run at boot can find the store locked that is open by the time anybody looks.
    if let Some(refused) = &schedule.refused {
        return Some(warn(
            "schedule",
            "renewal schedule",
            words::could_not_open(
                &runs,
                facts.now - refused.at,
                refused.vault,
                refused.locked(),
            ),
            words::runs_again(facts.os),
        ));
    }
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

// A Pitboard gone from its path, or a failed run, stops every switch where nobody looks.
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
    if service.outdated {
        return Some(warn(
            "watch_service",
            "watch service",
            words::fails_once_its_pitboard_is_gone(&runs),
            again,
        ));
    }
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

// The app is started as the schedule's runs are, so a terminal's proxy differs: no warning there.
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
    // A SOCKS proxy is not gone through: the network check says what becomes of the requests.
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
            account: None,
        });
    }
    // Installed again from here, it would be given a SOCKS proxy that fails its requests.
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

pub(super) fn again(os: Os) -> &'static str {
    if has_app(os) {
        words::AGAIN_IN_THE_APP_OR_A_TERMINAL
    } else {
        words::AGAIN_IN_A_TERMINAL
    }
}

pub(super) fn has_app(os: Os) -> bool {
    match os {
        Os::MacOs | Os::Windows => true,
        Os::Linux => false,
    }
}
