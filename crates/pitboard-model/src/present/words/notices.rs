//! What notices and notifications say: what a switch or a sign-in did, the headings of
//! warnings by code, their buttons, and what switching by itself tells somebody.

use super::{SWITCH, SWITCH_WAITING, limit_title, quoted, switch_to, titled, typeset};
use crate::Front;
use crate::Warning;
use crate::model::state::in_its_app;
use crate::present::Severity;
use pitboard_core::status::Standing;
use pitboard_core::words::capitalised;

/// The heading of a tool's last switch: "Switched to work", "Switched Codex to work" beside
/// another tool's accounts.
pub(crate) fn switched_to(tool: Option<&str>, label: &str) -> String {
    match tool {
        Some(tool) => format!("Switched {tool} to {label}"),
        None => format!("Switched to {label}"),
    }
}

// "Signed in to work", or "Signed in to work in Codex" beside another tool's accounts.
pub(crate) fn signed_in_to(tool: Option<&str>, label: &str) -> String {
    match tool {
        Some(tool) => format!("Signed in to {label} in {tool}"),
        None => format!("Signed in to {label}"),
    }
}

// A sign-in that put `name` in use: `again` where it was added already.
pub(crate) fn signed_in_now(name: &str, again: bool, app: Option<&str>) -> String {
    match (again, app) {
        (true, None) => format!("Signed in to {name} again. It’s the account in use now."),
        (false, None) => format!("Added {name}, the account signed in now."),
        (true, Some(app)) => format!("Signed in to {name} again. {app} is signed in to it now."),
        (false, Some(app)) => format!("Added {name}, the account {app} is signed in to now."),
    }
}

/// Said before a countdown to when sessions already open follow a switch.
pub(crate) fn sessions_follow_in(tool: Option<&str>) -> String {
    match tool {
        Some(tool) => format!("{tool} sessions already open follow in"),
        None => "Sessions already open follow in".into(),
    }
}

// The tool is named in the title above it, and a file behind its store in Clean Up's notice.
pub(crate) fn follows(adoption: &pitboard_core::provider::Adoption) -> String {
    use pitboard_core::provider::Adoption;
    match adoption {
        Adoption::PollingWithin(seconds) => {
            format!("Sessions already running follow within {seconds} seconds.")
        }
        Adoption::AtRenewal { .. } => {
            "Sessions already running keep their account until they’re started again.".into()
        }
        Adoption::RestartRequired { program, .. } => {
            format!("Restart any running {program} for this to take effect.")
        }
        Adoption::NextOpen { app } => opens_signed_in(app),
    }
}

// What the app a switch closed holds once it is opened again.
pub(crate) fn opens_signed_in(app: &str) -> String {
    format!("{app} opens signed in to it.")
}

/// What a switch means for sessions of a tool that never picks one up by itself. Said of any
/// session and not of running ones, since the core counts those itself when it can, and a
/// notice left in the panel should not claim sessions that may not exist. `from` is empty
/// when nothing was signed in before, and then there is no old account to name.
pub(crate) fn restart_notice(program: &str, from: &str) -> String {
    let old = if from.is_empty() {
        "the account it started with"
    } else {
        from
    };
    format!(
        "Any {program} session started before this switch keeps using {old} until it is \
         quit and started again."
    )
}

pub(crate) fn warning_heading(warning: &Warning) -> &'static str {
    match warning.code.as_str() {
        "sessions_still_running" => "Open sessions still use the previous account",
        "sessions_keep_old_login" => "Open sessions still use the old sign-in",
        "sessions_unknown" => "Couldn’t tell which sessions are open",
        "auth_overridden" => "An environment variable overrides the sign-in",
        "fallback_login" => "A leftover sign-in file",
        "parked_login_refused" => "A saved sign-in no longer works",
        "lock_compromised" => "A sign-in may have been written twice",
        "parks_pending_removal" => "Old saved sign-ins are still there",
        "written_on_the_command_line" => "A sign-in was passed on the command line",
        "sign_in_parked_not_in_use" => "The new sign-in was saved, not put in use",
        "interrupted_switch_finished" => "A switch that didn’t finish is done",
        "interrupted_switch_undone" => "A switch that didn’t finish was undone",
        "recovery_undetermined" => SWITCH_WAITING,
        "login_replaced" => "A sign-in was replaced outside Pitboard",
        "in_use_unconfirmed" => "The account in use isn’t confirmed",
        // `in_app_warning` words the two alike for an app.
        "config_names_another" if warning.provider.is_some_and(in_its_app) => {
            "The account in use isn’t confirmed"
        }
        "config_names_another" => "Claude Code’s config names another account",
        "login_taken_as_named" => "A sign-in was taken as the account named for it",
        "looks_signed_out" => "The account in use may be signed out",
        "set_aside" => "Pitboard moved a folder out of the way",
        "move_waiting" => "A switch in Claude didn’t finish",
        "move_not_given_up" => "A switch in Claude wasn’t cancelled",
        "interrupted_move_undone" => "A switch in Claude that didn’t finish was undone",
        "interrupted_move_finished" => "A switch in Claude that didn’t finish is done",
        "login_item_gone" => "Part of a sign-in was missing",
        _ => "Pitboard has a warning",
    }
}

// Said where a symbol shows it: two alike would let an error pass for a note.
pub(crate) fn spoken_severity(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "Note",
        Severity::Warning => "Warning",
        Severity::Error => "Problem",
    }
}

pub(crate) const DISMISS: &str = "Dismiss";

pub(crate) const CLEANED_UP: &str = "Cleaned up the leftover sign-in file";

// The notice's one sentence once the file is gone.
pub(crate) fn cleaned_up(stowed: &pitboard_core::switch::Stowed) -> String {
    use pitboard_core::switch::Kept;
    let stays = |kept: &str| format!("Deleted the file, and {kept} stays.");
    match &stowed.kept {
        Kept::NoLogin => "Deleted the file, which held no sign-in.".into(),
        Kept::Refused => "Deleted the file and the sign-in in it that no longer worked.".into(),
        Kept::Stored { label: Some(label) } => {
            format!("Deleted the file, and {label}’s sign-in stays where Claude Code keeps it.")
        }
        Kept::Stored { label: None } => stays("the sign-in Claude Code keeps"),
        Kept::AlreadyParked { label } | Kept::ParkKept { label } => {
            stays(&format!("{label}’s saved sign-in"))
        }
        Kept::SecondSignIn { label } => {
            format!("Deleted the file and the second sign-in of {label} in it.")
        }
        Kept::ParkedNow { label } => {
            format!("Saved {label}’s sign-in from the file, then deleted the file.")
        }
        _ => "Deleted the leftover sign-in file.".into(),
    }
}

// A sign-in inside an app left halfway: `again` names the account added already.
pub(crate) fn unfinished(tool: &str, again: Option<&str>) -> String {
    match again {
        Some(label) => format!("Signing in to {label} again didn’t finish"),
        None => format!("Adding a {tool} account didn’t finish"),
    }
}

// The lines of the notice for a sign-in inside `app` left halfway, `put_back` signed out for it.
pub(crate) fn awaiting_lines(app: &str, label: &str, put_back: &str, again: bool) -> Vec<String> {
    let then = if again {
        format!("Sign in to {label} in {app}, quit {app}, then choose Finish Signing In.")
    } else {
        format!(
            "Sign in to the account you’re adding in {app}, quit {app}, then choose Finish \
             Adding."
        )
    };
    vec![
        format!("{app} is signed out of {put_back} for a sign-in."),
        then,
    ]
}

pub(crate) fn finish(front: Front, again: bool) -> String {
    if again {
        titled(front, "Finish Signing In…", "Finish signing in…")
    } else {
        titled(front, "Finish Adding…", "Finish adding…")
    }
}

pub(crate) fn switch_back_to(front: Front, label: &str) -> String {
    titled(
        front,
        &format!("Switch Back to {label}"),
        &format!("Switch back to {label}"),
    )
}

pub(crate) fn sign_in_again(front: Front, asks: bool) -> String {
    if asks {
        titled(front, "Sign In Again…", "Sign in again…")
    } else {
        titled(front, "Sign In Again", "Sign in again")
    }
}

// A notification's button, which opens the sign-in: Windows and GNOME end a command that asks
// for more with an ellipsis, a notification's too, where a macOS action is a short term.
pub(crate) fn sign_in_again_button(front: Front) -> String {
    let asks = match front {
        Front::WindowsNotificationArea | Front::Gnome => true,
        Front::MacMenuBar => false,
    };
    sign_in_again(front, asks)
}

pub(crate) fn add_account_shown(front: Front, app: &str, asks: bool) -> String {
    let (title, sentence) = (
        format!("Add the Account {app} Shows"),
        format!("Add the account {app} shows"),
    );
    if asks {
        titled(front, &format!("{title}…"), &format!("{sentence}…"))
    } else {
        titled(front, &title, &sentence)
    }
}

pub(crate) fn update_config(front: Front) -> String {
    titled(
        front,
        "Update Claude Code’s Config",
        "Update Claude Code’s config",
    )
}

// A notification's button: macOS draws the one its category registered, and a toast or a GNOME
// notification takes any.
pub(crate) fn switch_button(front: Front, label: &str) -> String {
    match front {
        Front::WindowsNotificationArea | Front::Gnome => switch_to(label),
        Front::MacMenuBar => SWITCH.into(),
    }
}

pub(crate) fn switch_back(front: Front) -> String {
    titled(front, "Switch Back", "Switch back")
}

// "work has used 96% of its 5-hour limit".
pub(crate) fn has_used(account: &str, used: &str) -> String {
    typeset(&pitboard_core::words::has_used(&quoted(account), used))
}

// "work has used 96% of its 5-hour limit.", then what came of it.
pub(crate) fn used_then(account: &str, used: &str, then: &str) -> String {
    format!("{}. {then}", has_used(account, used))
}

// "spare has used 20%.", with `left` of the same limit, or "seat has no such limit."
pub(crate) fn other_used(account: &str, left: Option<i64>) -> String {
    match left {
        Some(left) => format!("{account} has used {}%.", (100 - left).max(0)),
        None => format!("{account} has no such limit."),
    }
}

// "work reached its 5-hour limit", of the account in use that ran out.
pub(crate) fn ran_out(account: &str, limit: &str) -> String {
    format!("{account} reached its {limit} limit")
}

/// A sentence about one tool's account, said beside another tool's: "Claude Code: work has
/// no 5-hour limit left".
pub(crate) fn of_tool(tool: Option<&str>, sentence: &str) -> String {
    match tool {
        Some(tool) => format!("{tool}: {sentence}"),
        None => sentence.to_owned(),
    }
}

// "work has used 95%.", of the account in use at a limit.
pub(crate) fn has_used_share(account: &str, figure: &str) -> String {
    format!("{account} has used {figure}.")
}

// "5-Hour Limit Almost Reached", "Weekly Limit (Fable) Reached": `name` as a sentence says it.
pub(crate) fn limit_notification(
    front: Front,
    name: &str,
    model: Option<&str>,
    reached: bool,
) -> String {
    let headed: Vec<String> = name.split('-').map(capitalised).collect();
    let (title, sentence) = (
        limit_title(&headed.join("-"), model).replacen(" limit", " Limit", 1),
        limit_title(name, model),
    );
    if reached {
        titled(
            front,
            &format!("{title} Reached"),
            &format!("{sentence} reached"),
        )
    } else {
        titled(
            front,
            &format!("{title} Almost Reached"),
            &format!("{sentence} almost reached"),
        )
    }
}

// Notification titles, under the tool they are about.
pub(crate) fn not_switched(front: Front) -> String {
    titled(
        front,
        "Automatic Switch Skipped",
        "Automatic switch skipped",
    )
}

pub(crate) fn not_switching(front: Front) -> String {
    titled(
        front,
        "Automatic Switching Paused",
        "Automatic switching paused",
    )
}

pub(crate) fn could_not_switch_itself(front: Front) -> String {
    titled(
        front,
        "Couldn’t Switch Automatically",
        "Couldn’t switch automatically",
    )
}

// A Jump List task's switch refused, as a notification's title.
pub(crate) fn could_not_switch_from_outside(front: Front) -> String {
    titled(front, "Couldn’t Switch", "Couldn’t switch")
}

pub(crate) fn could_not_warm_itself(front: Front) -> String {
    titled(front, "Warm-Up Didn’t Start", "Warm-up didn’t start")
}

// A notification's title for an account that needs signing in to again, by why.
pub(crate) fn sign_in_title(front: Front, standing: Standing) -> String {
    match standing {
        Standing::LoginExpired => titled(front, "Sign-In Expired", "Sign-in expired"),
        Standing::NothingParked => titled(front, "Not Signed In", "Not signed in"),
        _ => titled(front, "Sign-In No Longer Works", "Sign-in no longer works"),
    }
}

pub(crate) fn sign_in_replaced(front: Front) -> String {
    titled(front, "Sign-In Replaced", "Sign-in replaced")
}

pub(crate) fn sign_in_to_keep(label: &str) -> String {
    format!("Sign in to {label} again to keep switching to it.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::present::words::{
        CANCEL_MOVE_QUESTION, CANCEL_SWITCH_QUESTION, CANCELLING_KEEPS_SIGN_INS, MOVE_CANCELLED,
        SWITCH_CANCELLED, auto_stopped, cancel_move, cancel_stuck, cancel_switch, cancelled,
        clean_up, issues, move_waits_too, switch_waits_for,
    };

    // The contractions the apps give the core's words.
    #[test]
    fn the_apps_own_words_on_switching_by_itself_take_their_contractions() {
        let both = |said: &dyn Fn(Front) -> String| {
            (
                said(Front::MacMenuBar),
                said(Front::WindowsNotificationArea),
            )
        };
        let pair = |mac: &str, windows: &str| (mac.to_owned(), windows.to_owned());
        assert_eq!(
            both(&not_switched),
            pair("Automatic Switch Skipped", "Automatic switch skipped")
        );
        assert_eq!(
            both(&not_switching),
            pair("Automatic Switching Paused", "Automatic switching paused")
        );
        assert_eq!(
            both(&could_not_switch_itself),
            pair(
                "Couldn’t Switch Automatically",
                "Couldn’t switch automatically"
            )
        );
        assert_eq!(
            both(&could_not_warm_itself),
            pair("Warm-Up Didn’t Start", "Warm-up didn’t start")
        );
        assert_eq!(
            auto_stopped(Some("08:01"), "Couldn’t write."),
            "Pitboard couldn’t switch Claude Code, and tries again at 08:01: Couldn’t write."
        );
        assert_eq!(
            auto_stopped(None, "Couldn’t read."),
            "Pitboard can’t tell whether to switch Claude Code: Couldn’t read."
        );
    }

    /// A warning is named in the menu by a heading from its code, each its own, and a code
    /// the app does not know yet still gets one rather than an empty line.
    #[test]
    fn every_warning_code_has_its_own_heading_and_an_unknown_one_a_general_one() {
        let headings = [
            (
                "sessions_still_running",
                "Open sessions still use the previous account",
            ),
            (
                "sessions_keep_old_login",
                "Open sessions still use the old sign-in",
            ),
            ("sessions_unknown", "Couldn’t tell which sessions are open"),
            (
                "auth_overridden",
                "An environment variable overrides the sign-in",
            ),
            ("fallback_login", "A leftover sign-in file"),
            ("parked_login_refused", "A saved sign-in no longer works"),
            ("lock_compromised", "A sign-in may have been written twice"),
            (
                "parks_pending_removal",
                "Old saved sign-ins are still there",
            ),
            (
                "written_on_the_command_line",
                "A sign-in was passed on the command line",
            ),
            (
                "sign_in_parked_not_in_use",
                "The new sign-in was saved, not put in use",
            ),
            (
                "interrupted_switch_finished",
                "A switch that didn’t finish is done",
            ),
            (
                "interrupted_switch_undone",
                "A switch that didn’t finish was undone",
            ),
            ("recovery_undetermined", "A switch didn’t finish"),
            ("login_replaced", "A sign-in was replaced outside Pitboard"),
            ("in_use_unconfirmed", "The account in use isn’t confirmed"),
            (
                "config_names_another",
                "Claude Code’s config names another account",
            ),
        ];
        let warning = |code: &str| Warning {
            code: code.into(),
            message: String::new(),
            account: None,
            held: None,
            provider: None,
        };
        for (code, heading) in headings {
            assert_eq!(warning_heading(&warning(code)), heading, "{code}");
        }
        let distinct: std::collections::BTreeSet<_> = headings.iter().map(|(_, h)| h).collect();
        assert_eq!(distinct.len(), headings.len(), "no two codes read alike");
        assert_eq!(
            warning_heading(&warning("not_a_code_yet")),
            "Pitboard has a warning"
        );
        assert_eq!(warning_heading(&warning("")), "Pitboard has a warning");
    }

    // What a notice says happened, pinned once.
    #[test]
    fn a_notice_says_what_happened_in_these_words() {
        assert_eq!(switched_to(None, "work"), "Switched to work");
        assert_eq!(
            switched_to(Some("Claude Code"), "work"),
            "Switched Claude Code to work"
        );
        assert_eq!(sessions_follow_in(None), "Sessions already open follow in");
        assert_eq!(
            sessions_follow_in(Some("Claude Code")),
            "Claude Code sessions already open follow in"
        );
        assert_eq!(
            restart_notice("codex", "personal"),
            "Any codex session started before this switch keeps using personal until it is quit \
             and started again."
        );
        assert_eq!(signed_in_to(None, "work"), "Signed in to work");
        assert_eq!(
            signed_in_to(Some("Codex"), "work"),
            "Signed in to work in Codex"
        );
        assert_eq!(
            signed_in_now("work", true, None),
            "Signed in to work again. It’s the account in use now."
        );
        assert_eq!(
            signed_in_now("work", false, None),
            "Added work, the account signed in now."
        );
        assert_eq!(
            cancelled("personal", "work", 2),
            "The switch from personal to work was cancelled. 2 sign-ins kept, and nothing was \
             deleted."
        );
        assert_eq!(
            cancelled("personal", "work", 1),
            "The switch from personal to work was cancelled. 1 sign-in kept, and nothing was \
             deleted."
        );
        assert_eq!(ran_out("work", "5-hour"), "work reached its 5-hour limit");
        assert_eq!(other_used("spare", Some(80)), "spare has used 20%.");
        assert_eq!(other_used("seat", None), "seat has no such limit.");
        assert_eq!(
            of_tool(Some("Claude Code"), "work reached its 5-hour limit"),
            "Claude Code: work reached its 5-hour limit"
        );
        assert_eq!(
            (
                issues(Front::MacMenuBar, 2),
                issues(Front::WindowsNotificationArea, 2)
            ),
            ("2 Issues".into(), "2 issues".into())
        );
        assert_eq!(
            switch_waits_for("Anthropic or OpenAI"),
            "The switch can’t finish until Anthropic or OpenAI answers."
        );
        assert_eq!(
            cancel_stuck(&[]),
            "Every sign-in is kept, and nothing is deleted."
        );
        assert_eq!(
            cancel_stuck(&[("Claude Desktop", "Claude")]),
            "Every sign-in is kept and nothing is deleted, and once Claude quits, its switch is \
             finished or cancelled too."
        );
        assert_eq!(
            move_waits_too("Claude Desktop", "Claude"),
            "A Claude Desktop switch didn’t finish either. With Claude quit, Cancel Switch… \
             finishes or cancels it too, and keeps every folder."
        );
        assert_eq!(
            unfinished("Claude Desktop", None),
            "Adding a Claude Desktop account didn’t finish"
        );
        assert_eq!(
            unfinished("Claude Desktop", Some("home")),
            "Signing in to home again didn’t finish"
        );
        assert_eq!(
            cancel_move("Claude"),
            "Every folder is kept, Claude’s included, and nothing is deleted."
        );
        assert_eq!(
            has_used("work", "96% of its 5-hour limit"),
            "work has used 96% of its 5-hour limit"
        );
        assert_eq!(
            used_then("work", "96% of its 5-hour limit", "It ended."),
            "work has used 96% of its 5-hour limit. It ended."
        );
        assert_eq!(has_used_share("work", "95%"), "work has used 95%.");
        assert_eq!(
            sign_in_to_keep("old"),
            "Sign in to old again to keep switching to it."
        );
        use pitboard_core::provider::Adoption;
        assert_eq!(
            follows(&Adoption::PollingWithin(33)),
            "Sessions already running follow within 33 seconds."
        );
        assert_eq!(
            follows(&Adoption::AtRenewal {
                file: "/Users/x/.claude/.credentials.json".into()
            }),
            "Sessions already running keep their account until they’re started again."
        );
        assert_eq!(
            follows(&Adoption::NextOpen { app: "Claude" }),
            "Claude opens signed in to it."
        );
    }

    // A notification's title, in the case each system gives one.
    #[test]
    fn a_notification_is_titled_as_each_system_titles_one() {
        let both = |said: &dyn Fn(Front) -> String| {
            (
                said(Front::MacMenuBar),
                said(Front::WindowsNotificationArea),
            )
        };
        let pair = |mac: &str, windows: &str| (mac.to_owned(), windows.to_owned());
        assert_eq!(
            both(&|front| limit_notification(front, "5-hour", None, false)),
            pair("5-Hour Limit Almost Reached", "5-hour limit almost reached")
        );
        assert_eq!(
            both(&|front| limit_notification(front, "weekly", Some("Fable"), true)),
            pair(
                "Weekly Limit (Fable) Reached",
                "Weekly limit (Fable) reached"
            )
        );
        assert_eq!(
            both(&|front| sign_in_title(front, Standing::LoginExpired)),
            pair("Sign-In Expired", "Sign-in expired")
        );
        assert_eq!(
            both(&|front| sign_in_title(front, Standing::LoginRefused)),
            pair("Sign-In No Longer Works", "Sign-in no longer works")
        );
        assert_eq!(
            both(&|front| sign_in_title(front, Standing::NothingParked)),
            pair("Not Signed In", "Not signed in")
        );
        assert_eq!(
            both(&sign_in_replaced),
            pair("Sign-In Replaced", "Sign-in replaced")
        );
    }

    // The notices' fixed words, pinned once.
    #[test]
    fn a_notice_is_headed_in_these_words() {
        assert_eq!(DISMISS, "Dismiss");
        assert_eq!(SWITCH_WAITING, "A switch didn’t finish");
        assert_eq!(
            CANCELLING_KEEPS_SIGN_INS,
            "Cancelling it keeps every sign-in and deletes nothing."
        );
        assert_eq!(
            CANCEL_SWITCH_QUESTION,
            "Cancel the switch that didn’t finish?"
        );
        assert_eq!(
            CANCEL_MOVE_QUESTION,
            "Cancel the switch in Claude that didn’t finish?"
        );
        assert_eq!(SWITCH_CANCELLED, "Switch cancelled");
        assert_eq!(MOVE_CANCELLED, "Switch in Claude cancelled");
        assert_eq!(CLEANED_UP, "Cleaned up the leftover sign-in file");
        assert_eq!(SWITCH, "Switch");
        assert_eq!(
            awaiting_lines("Claude", "travel", "work", false),
            [
                "Claude is signed out of work for a sign-in.",
                "Sign in to the account you’re adding in Claude, quit Claude, then choose Finish \
                 Adding."
            ]
        );
        assert_eq!(
            awaiting_lines("Claude", "home", "work", true)[1],
            "Sign in to home in Claude, quit Claude, then choose Finish Signing In."
        );
    }

    // A notice's and a notification's buttons, as each system titles a button.
    #[test]
    fn a_notice_offers_in_these_words() {
        let both = |said: &dyn Fn(Front) -> String| {
            (
                said(Front::MacMenuBar),
                said(Front::WindowsNotificationArea),
            )
        };
        let pair = |mac: &str, windows: &str| (mac.to_owned(), windows.to_owned());
        assert_eq!(
            both(&|front| finish(front, false)),
            pair("Finish Adding…", "Finish adding…")
        );
        assert_eq!(
            both(&|front| finish(front, true)),
            pair("Finish Signing In…", "Finish signing in…")
        );
        assert_eq!(
            both(&|front| switch_back_to(front, "work")),
            pair("Switch Back to work", "Switch back to work")
        );
        assert_eq!(
            both(&|front| cancel_switch(front, true)),
            pair("Cancel Switch…", "Cancel switch…")
        );
        assert_eq!(
            both(&|front| cancel_switch(front, false)),
            pair("Cancel Switch", "Cancel switch")
        );
        assert_eq!(
            both(&|front| clean_up(front, true)),
            pair("Clean Up…", "Clean up…")
        );
        assert_eq!(
            both(&|front| clean_up(front, false)),
            pair("Clean Up", "Clean up")
        );
        assert_eq!(
            both(&|front| sign_in_again(front, true)),
            pair("Sign In Again…", "Sign in again…")
        );
        assert_eq!(
            both(&|front| sign_in_again(front, false)),
            pair("Sign In Again", "Sign in again")
        );
        assert_eq!(
            both(&sign_in_again_button),
            pair("Sign In Again", "Sign in again…")
        );
        assert_eq!(
            both(&|front| add_account_shown(front, "Claude", true)),
            pair(
                "Add the Account Claude Shows…",
                "Add the account Claude shows…"
            )
        );
        assert_eq!(
            both(&|front| add_account_shown(front, "Claude", false)),
            pair(
                "Add the Account Claude Shows",
                "Add the account Claude shows"
            )
        );
        assert_eq!(
            both(&update_config),
            pair("Update Claude Code’s Config", "Update Claude Code’s config")
        );
        assert_eq!(both(&switch_back), pair("Switch Back", "Switch back"));
    }

    // GNOME heads a notice and its buttons as a Mac does, and like Windows puts an ellipsis on a
    // notification's button that asks for more and the account's name on its Switch.
    #[test]
    fn gnome_notices_in_these_words() {
        let gnome = Front::Gnome;
        assert_eq!(
            limit_notification(gnome, "5-hour", None, false),
            "5-Hour Limit Almost Reached"
        );
        assert_eq!(
            sign_in_title(gnome, Standing::LoginRefused),
            "Sign-In No Longer Works"
        );
        assert_eq!(sign_in_again_button(gnome), "Sign In Again…");
        assert_eq!(switch_button(gnome, "personal"), "Switch to personal");
        assert_eq!(switch_back_to(gnome, "work"), "Switch Back to work");
        assert_eq!(finish(gnome, false), "Finish Adding…");
        assert_eq!(cancel_switch(gnome, true), "Cancel Switch…");
        assert_eq!(clean_up(gnome, true), "Clean Up…");
        assert_eq!(update_config(gnome), "Update Claude Code’s Config");
    }
}
