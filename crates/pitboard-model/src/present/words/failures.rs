//! What a failure's alert says: what couldn't be done, and why. And a switch that didn't
//! finish: what it waits for, cancelling it, and what cancelling kept.

use super::{missing, sign_ins, titled, typeset};
use crate::model::{Cause, Doing};
use pitboard_core::host::Os;
use pitboard_core::state::Key;
use pitboard_core::words::listed;

pub(crate) const NOT_KNOWN_HOW_IT_WENT: &str =
    "Pitboard stopped before it could say how this went. Refresh to see where things stand.";

// What was being done, as a failure is titled: "Couldn’t switch to personal".
pub(crate) fn could_not(doing: &Doing) -> String {
    match doing {
        Doing::Switch {
            account: Key { label, .. },
        } => format!("Couldn’t switch to {label}"),
        Doing::CancelSwitch => "Couldn’t cancel the switch".into(),
        Doing::SignIn { again: Some(label) } => format!("Couldn’t sign in to {label}"),
        Doing::SignIn { again: None } => "Couldn’t add the account".into(),
        Doing::Remove {
            account: Key { label, .. },
        } => format!("Couldn’t remove {label}"),
        Doing::Rename {
            account: Key { label, .. },
        } => format!("Couldn’t rename {label}"),
        Doing::UpdateConfig => "Couldn’t update Claude Code’s config".into(),
        Doing::TellWhose => "Couldn’t tell whose sign-in it is".into(),
        Doing::CleanUp => "Couldn’t clean up the sign-in file".into(),
        Doing::ChangeWarm => "Couldn’t change which accounts are warmed up".into(),
        Doing::Uninstall => "Couldn’t uninstall Pitboard".into(),
        Doing::Warm {
            account: Key { label, .. },
        } => format!("Couldn’t warm up {label}"),
        Doing::ReadUsage => "Couldn’t read usage".into(),
    }
}

// Why `doing` didn't happen.
pub(crate) fn why_not(os: Os, doing: &Doing, cause: &Cause) -> String {
    // what to do once the app has quit
    let again = if matches!(doing, Doing::Switch { .. }) {
        "switch again"
    } else {
        "try again"
    };
    match cause {
        Cause::Refused { message } => message.clone(),
        Cause::StillOpen { app } => still_open(app, again),
        Cause::PartStillRunning { app } => part_still_running(os, app, again),
        Cause::ProgramMissing { tool } => {
            let missing = missing(os, *tool);
            format!("{} {}", missing.title, missing.detail)
        }
        Cause::NotKnownHowItWent => NOT_KNOWN_HOW_IT_WENT.into(),
    }
}

pub(crate) const LAST_MEASURED: &str = "The numbers shown are the last ones measured.";

// Why the app it asked to quit stopped a change: "… Quit it, then switch again."
pub(crate) fn still_open(app: &str, again: &str) -> String {
    format!("{app} is still open, so nothing has changed. Quit it, then {again}.")
}

pub(crate) fn part_still_running(os: Os, app: &str, again: &str) -> String {
    format!(
        "Part of {app} is still running after it quit, so nothing has changed. {}",
        typeset(&pitboard_core::words::wait_for_still(os, again))
    )
}

// What a switch nothing can finish waits for, where the read did not say.
pub(crate) fn switch_waits_for(services: &str) -> String {
    format!("The switch can’t finish until {services} answers.")
}

pub(crate) fn move_waits_too(tool: &str, program: &str) -> String {
    format!(
        "A {tool} switch didn’t finish either. With {program} quit, Cancel Switch… finishes or \
         cancels it too, and keeps every folder."
    )
}

// `moving`: each tool whose switch in its app waits, with its program.
pub(crate) fn cancel_stuck(moving: &[(&str, &str)]) -> String {
    if moving.is_empty() {
        return "Every sign-in is kept, and nothing is deleted.".into();
    }
    let mut programs: Vec<&str> = Vec::new();
    for &(_, program) in moving {
        if !programs.contains(&program) {
            programs.push(program);
        }
    }
    let quits = if programs.len() == 1 { "quits" } else { "quit" };
    let its = if moving.len() == 1 {
        "its switch is"
    } else {
        "their switches are"
    };
    format!(
        "Every sign-in is kept and nothing is deleted, and once {} {quits}, {its} finished or \
         cancelled too.",
        listed(&programs)
    )
}

// What cancelling a switch that didn't finish kept.
pub(crate) fn cancelled(from: &str, to: &str, kept: u32) -> String {
    let from = if from.is_empty() {
        String::new()
    } else {
        format!(" from {from}")
    };
    format!(
        "The switch{from} to {to} was cancelled. {} kept, and nothing was deleted.",
        sign_ins(kept)
    )
}

pub(crate) const SWITCH_WAITING: &str = "A switch didn’t finish";

pub(crate) const CANCELLING_KEEPS_SIGN_INS: &str =
    "Cancelling it keeps every sign-in and deletes nothing.";

pub(crate) const CANCEL_SWITCH_QUESTION: &str = "Cancel the switch that didn’t finish?";

pub(crate) const CANCEL_MOVE_QUESTION: &str = "Cancel the switch in Claude that didn’t finish?";

pub(crate) fn cancel_move(app: &str) -> String {
    format!("Every folder is kept, {app}’s included, and nothing is deleted.")
}

pub(crate) const SWITCH_CANCELLED: &str = "Switch cancelled";

pub(crate) const MOVE_CANCELLED: &str = "Switch in Claude cancelled";

// `asks`: the button opens a sheet or a question, and ends with an ellipsis; the one that
// answers it does not.
pub(crate) fn cancel_switch(os: Os, asks: bool) -> String {
    if asks {
        titled(os, "Cancel Switch…", "Cancel switch…")
    } else {
        titled(os, "Cancel Switch", "Cancel switch")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pitboard_core::provider::ProviderId;

    #[test]
    fn what_is_left_of_an_app_that_quit_is_quit_in_task_manager_on_windows() {
        assert_eq!(
            part_still_running(Os::Windows, "ChatGPT", "switch again"),
            "Part of ChatGPT is still running after it quit, so nothing has changed. Wait until \
             it ends, or quit it in Task Manager, then switch again."
        );
        assert_eq!(
            part_still_running(Os::MacOs, "ChatGPT", "switch again"),
            "Part of ChatGPT is still running after it quit, so nothing has changed. Wait until \
             it ends, or quit it in Activity Monitor, then switch again."
        );
    }

    // What a failure is titled, by what was being done, pinned once.
    #[test]
    fn a_failure_is_titled_in_these_words() {
        let work = || Key::new(ProviderId::Claude, "work");
        for (doing, said) in [
            (Doing::Switch { account: work() }, "Couldn’t switch to work"),
            (Doing::CancelSwitch, "Couldn’t cancel the switch"),
            (
                Doing::SignIn {
                    again: Some("work".into()),
                },
                "Couldn’t sign in to work",
            ),
            (Doing::SignIn { again: None }, "Couldn’t add the account"),
            (Doing::Remove { account: work() }, "Couldn’t remove work"),
            (Doing::Rename { account: work() }, "Couldn’t rename work"),
            (Doing::UpdateConfig, "Couldn’t update Claude Code’s config"),
            (Doing::TellWhose, "Couldn’t tell whose sign-in it is"),
            (Doing::CleanUp, "Couldn’t clean up the sign-in file"),
            (
                Doing::ChangeWarm,
                "Couldn’t change which accounts are warmed up",
            ),
            (Doing::Uninstall, "Couldn’t uninstall Pitboard"),
            (Doing::Warm { account: work() }, "Couldn’t warm up work"),
            (Doing::ReadUsage, "Couldn’t read usage"),
        ] {
            assert_eq!(could_not(&doing), said, "{doing:?}");
        }
    }

    // Why, by its cause: what to do once an app has quit follows what was being done.
    #[test]
    fn a_failure_says_why_in_these_words() {
        let switch = Doing::Switch {
            account: Key::new(ProviderId::Codex, "work"),
        };
        let add = Doing::SignIn { again: None };
        let chatgpt = || "ChatGPT".to_owned();
        for (doing, cause, said) in [
            (
                &add,
                Cause::Refused {
                    message: "The sign-in didn’t finish.".into(),
                },
                "The sign-in didn’t finish.",
            ),
            (
                &switch,
                Cause::StillOpen { app: chatgpt() },
                "ChatGPT is still open, so nothing has changed. Quit it, then switch again.",
            ),
            (
                &add,
                Cause::StillOpen { app: chatgpt() },
                "ChatGPT is still open, so nothing has changed. Quit it, then try again.",
            ),
            (
                &add,
                Cause::PartStillRunning { app: chatgpt() },
                "Part of ChatGPT is still running after it quit, so nothing has changed. Wait \
                 until it ends, or quit it in Activity Monitor, then try again.",
            ),
            (
                &add,
                Cause::ProgramMissing {
                    tool: ProviderId::Claude,
                },
                "Pitboard can’t find Claude Code on this Mac. Adding a Claude Code account runs \
                 Claude Code’s own sign-in. Install it, then come back to this window.",
            ),
            (
                &switch,
                Cause::NotKnownHowItWent,
                "Pitboard stopped before it could say how this went. Refresh to see where \
                 things stand.",
            ),
        ] {
            assert_eq!(why_not(Os::MacOs, doing, &cause), said, "{cause:?}");
        }
        assert_eq!(
            LAST_MEASURED,
            "The numbers shown are the last ones measured."
        );
    }
}
