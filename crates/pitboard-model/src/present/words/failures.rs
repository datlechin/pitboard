//! What a failure's alert says: what couldn't be done, and why where an app it asked to quit
//! stopped it. And a switch that didn't finish: what it waits for, cancelling it, and what
//! cancelling kept.

use super::{sign_ins, titled, typeset};
use pitboard_core::host::Os;
use pitboard_core::words::listed;

pub(crate) const NOT_KNOWN_HOW_IT_WENT: &str =
    "Pitboard stopped before it could say how this went. Refresh to see where things stand.";

pub(crate) fn could_not_switch(label: &str) -> String {
    format!("Couldn’t switch to {label}")
}

pub(crate) const COULD_NOT_CANCEL: &str = "Couldn’t cancel the switch";

// `again`: the account signed in to again, or none for a new one.
pub(crate) fn could_not_sign_in(again: Option<&str>) -> String {
    match again {
        Some(label) => format!("Couldn’t sign in to {label}"),
        None => "Couldn’t add the account".into(),
    }
}

pub(crate) fn could_not_remove(label: &str) -> String {
    format!("Couldn’t remove {label}")
}

pub(crate) fn could_not_rename(label: &str) -> String {
    format!("Couldn’t rename {label}")
}

pub(crate) const COULD_NOT_UPDATE_CONFIG: &str = "Couldn’t update Claude Code’s config";

pub(crate) const COULD_NOT_TELL_WHOSE: &str = "Couldn’t tell whose sign-in it is";

pub(crate) const COULD_NOT_CLEAN_UP: &str = "Couldn’t clean up the sign-in file";

pub(crate) const COULD_NOT_CHANGE_WARM: &str = "Couldn’t change which accounts are warmed up";

pub(crate) const COULD_NOT_UNINSTALL: &str = "Couldn’t uninstall Pitboard";

pub(crate) fn could_not_warm(label: &str) -> String {
    format!("Couldn’t warm up {label}")
}

pub(crate) const COULD_NOT_READ_USAGE: &str = "Couldn’t read usage";

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

    // What a failure is titled, by what failed, pinned once.
    #[test]
    fn a_failure_is_titled_in_these_words() {
        assert_eq!(
            NOT_KNOWN_HOW_IT_WENT,
            "Pitboard stopped before it could say how this went. Refresh to see where things \
             stand."
        );
        assert_eq!(could_not_switch("work"), "Couldn’t switch to work");
        assert_eq!(COULD_NOT_CANCEL, "Couldn’t cancel the switch");
        assert_eq!(could_not_sign_in(Some("work")), "Couldn’t sign in to work");
        assert_eq!(could_not_sign_in(None), "Couldn’t add the account");
        assert_eq!(could_not_remove("work"), "Couldn’t remove work");
        assert_eq!(could_not_rename("work"), "Couldn’t rename work");
        assert_eq!(
            COULD_NOT_UPDATE_CONFIG,
            "Couldn’t update Claude Code’s config"
        );
        assert_eq!(COULD_NOT_TELL_WHOSE, "Couldn’t tell whose sign-in it is");
        assert_eq!(COULD_NOT_CLEAN_UP, "Couldn’t clean up the sign-in file");
        assert_eq!(
            COULD_NOT_CHANGE_WARM,
            "Couldn’t change which accounts are warmed up"
        );
        assert_eq!(could_not_warm("work"), "Couldn’t warm up work");
        assert_eq!(COULD_NOT_READ_USAGE, "Couldn’t read usage");
        assert_eq!(
            LAST_MEASURED,
            "The numbers shown are the last ones measured."
        );
        assert_eq!(
            still_open("ChatGPT", "switch again"),
            "ChatGPT is still open, so nothing has changed. Quit it, then switch again."
        );
    }
}
