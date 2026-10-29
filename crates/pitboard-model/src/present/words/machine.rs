//! What Settings and Activity say of this machine: keeping saved sign-ins fresh, what happens
//! at a limit, warm-ups, the command line, updates, uninstalling, and each change listed.

use super::{sign_in_standing, store_in_front, this_machine, titled, typeset};
use crate::Front;
use crate::model::AtLimit;
use pitboard_core::status::Standing;
use pitboard_core::words::capitalised;

// What keeping saved sign-ins fresh does, under its switch, and that Claude Desktop's never is.
pub(crate) fn renewal_about(front: Front, in_its_app: bool) -> String {
    let machine = match front {
        Front::MacMenuBar => "Your Mac",
        Front::Gnome => "This computer",
        Front::WindowsNotificationArea => "Your PC",
    };
    if in_its_app {
        format!(
            "{machine} refreshes saved sign-ins, except Claude Desktop’s, once a day so they \
             don’t expire."
        )
    } else {
        format!(
            "{machine} refreshes your saved sign-ins once a day, so one you leave alone doesn’t \
             expire."
        )
    }
}

/// Said under the switch where the machine has no scheduler Pitboard writes to: "This Mac has
/// no scheduler Pitboard knows how to write to."
pub(crate) fn no_scheduler(front: Front) -> String {
    format!(
        "{} has no scheduler Pitboard knows how to write to.",
        capitalised(this_machine(front))
    )
}

/// Why daily renewal cannot be turned on from this copy of the app. The schedule runs the
/// command line inside the app long after the app has quit, so it needs one that will still
/// be there: a copy run from a temporary place is gone by then, and without one inside the
/// app there is only the app itself to schedule, which renews nothing.
pub(crate) fn cannot_schedule(front: Front, temporary: bool) -> String {
    if !temporary {
        return "This copy of Pitboard has no command line inside it to run on a schedule.".into();
    }
    match front {
        Front::MacMenuBar => {
            "Move Pitboard to your Applications folder first. Until then macOS runs it \
             from a temporary copy, which is gone once Pitboard quits."
                .into()
        }
        // No app runs on Linux.
        Front::Gnome => "First move Pitboard to a folder it will stay in. Until then it runs \
                         from a temporary copy, which is gone once Pitboard quits."
            .into(),
        // The Windows app is installed by its installer, and a copy run from anywhere else,
        // such as from inside a zip, is the one a person is told to install.
        Front::WindowsNotificationArea => {
            "Install Pitboard first. Until then Windows runs it from a temporary \
             copy, which is gone once Pitboard quits."
                .into()
        }
    }
}

pub(crate) fn share_range(lowest: u8, highest: u8) -> String {
    format!("From {lowest}% to {highest}%")
}

pub(crate) const BEFORE_RUNNING_OUT: &str = "Before an account runs out";

pub(crate) const AT_LIMIT: &str = "When a limit reaches";

// "When a limit reaches 95%", the choice's name for VoiceOver and Narrator.
pub(crate) fn at_limit_label(at: u8) -> String {
    format!("{AT_LIMIT} {at}%")
}

// The share's field, beside "When a limit reaches".
pub(crate) const AT: &str = "At";

pub(crate) fn at_limit_choice(front: Front, choice: AtLimit) -> String {
    match choice {
        AtLimit::Nothing => titled(front, "Do Nothing", "Do nothing"),
        AtLimit::Notify => titled(front, "Notify Me", "Notify me"),
        AtLimit::Switch => titled(front, "Switch Automatically", "Switch automatically"),
    }
}

// Under the choice. Running sessions follow within `seconds` only while the last read found no
// login file behind the store.
pub(crate) fn at_limit_note(
    front: Front,
    choice: AtLimit,
    at: u8,
    file_left: bool,
    seconds: u32,
) -> Option<String> {
    match choice {
        AtLimit::Nothing => None,
        AtLimit::Notify => Some(format!(
            "Pitboard notifies you once a limit of the account in use reaches {at}%, while the app \
             is open."
        )),
        AtLimit::Switch if file_left => Some(format!(
            "Pitboard switches Claude Code at {at}%, but its running sessions don’t follow while a \
             sign-in file is left behind {}.",
            store_in_front(front)
        )),
        AtLimit::Switch => Some(format!(
            "Pitboard switches Claude Code at {at}%, and its running sessions follow within about \
             {seconds} seconds."
        )),
    }
}

// Under the setting, where switching by itself stopped on `message`: `until` it tries again.
pub(crate) fn auto_stopped(until: Option<&str>, message: &str) -> String {
    match until {
        Some(until) => {
            format!("Pitboard couldn’t switch Claude Code, and tries again at {until}: {message}")
        }
        None => format!("Pitboard can’t tell whether to switch Claude Code: {message}"),
    }
}

// Why Pitboard can't judge whether to switch Claude Code by itself, in the app's words.
pub(crate) fn not_watching(why: &pitboard_core::duty::autoswitch::Blind) -> String {
    use pitboard_core::duty::autoswitch::Blind;
    match why {
        Blind::SwitchInterrupted => SWITCH_NOT_FINISHED.into(),
        Blind::NothingSignedIn => "Claude Code isn’t signed in".into(),
        Blind::NotEnrolled { email } => {
            format!("Claude Code is signed in to {email}, an account you haven’t added")
        }
        Blind::Unidentified { detail, .. } => {
            format!("Pitboard couldn’t tell which account Claude Code is signed in to ({detail})")
        }
        other => pitboard_core::words::not_watching(other),
    }
}

const SWITCH_NOT_FINISHED: &str =
    "a switch didn’t finish, and the next change you make finishes it";

pub(crate) fn warm_up(front: Front) -> String {
    titled(front, "Warm-Up", "Warm-up")
}

pub(crate) fn warm_up_settings(front: Front) -> String {
    titled(front, "Warm-Up Settings…", "Warm-up settings")
}

pub(crate) fn warm_up_now(front: Front) -> String {
    titled(front, "Warm Up Now", "Warm up now")
}

pub(crate) const WARMING_UP: &str = "Warming up…";

// An account's warm-up, where it is turned off.
pub(crate) const WARM_UP_OFF: &str = "Off";

// A clock says a day's end as the next day's 00:00, which a span ending then does not mean.
pub(crate) fn warm_hours(
    from: u16,
    to: u16,
    time_of_day: impl Fn(u16) -> Option<String>,
) -> Option<String> {
    const DAY: u16 = 24 * 60;
    if from == 0 && to == DAY {
        return Some("All day".into());
    }
    let to = if to == DAY {
        "midnight".to_owned()
    } else {
        time_of_day(to)?
    };
    Some(format!("{} to {to}", time_of_day(from)?))
}

pub(crate) const NONE_TO_WARM: &str = "Add a Claude Code account to warm it up.";

pub(crate) const WARM_UP_NOTE: &str = "Pitboard starts each account’s 5-hour limit again as soon \
                                       as it resets, within these hours, while it’s open.";

pub(crate) fn warm_up_failed(failed: &str) -> String {
    format!("The last warm-up didn’t start: {failed}")
}

// The next warm-up of `account`, `at` a clock time, or now where it is due.
pub(crate) fn next_warm_up(account: &str, at: Option<&str>) -> String {
    match at {
        Some(at) => format!("Next warm-up: {account} at {at}."),
        None => format!("Warming up {account} now."),
    }
}

// What follows an account's name in its warm-up's line, in the app's words: "warms up at 22:50".
pub(crate) fn warm_standing(
    standing: &pitboard_core::duty::warm::Standing,
    moment: &dyn Fn(i64) -> String,
) -> String {
    use pitboard_core::duty::warm::{Hold, Standing};
    match standing {
        Standing::Held(Hold::LoginExpired) => "can’t warm up: its sign-in expired".into(),
        Standing::Held(Hold::NothingParked) => {
            "can’t warm up: it isn’t in use, and has no saved sign-in".into()
        }
        Standing::Waiting { until } => format!(
            "tries again at {}, after a warm-up didn’t start",
            moment(*until)
        ),
        Standing::Held(Hold::GaveUp { until }) => format!(
            "tries again at {}, after warm-ups didn’t start today",
            moment(*until)
        ),
        other => pitboard_core::words::warm_standing(other, moment),
    }
}

// Why Pitboard isn't warming up accounts, in the app's words.
pub(crate) fn warming_halted(halt: &pitboard_core::duty::warm::Halt) -> String {
    use pitboard_core::duty::warm::Halt;
    let why = match halt {
        Halt::SwitchInterrupted => SWITCH_NOT_FINISHED.into(),
        other => pitboard_core::words::not_warming(other),
    };
    typeset(&format!("Pitboard isn’t warming up accounts: {why}."))
}

pub(crate) const IS_THIS_ALLOWED: &str = "Is this allowed?";

pub(crate) const NOT_INSTALLED: &str = "Not installed";

// The installer puts the app's `pitboard` on the `PATH`, which only terminals opened after see.
pub(crate) const NEW_TERMINALS: &str =
    "Pitboard’s installer puts pitboard on your PATH, which terminals opened after it ran see.";

/// How the `pitboard` a terminal runs is kept up to date: with the app when it is the one
/// inside it, and otherwise the way it was installed. No other way of installing it updates
/// it by itself, and saying it "updates on its own" read as though one did.
pub(crate) fn update_note(bundled: bool) -> &'static str {
    if bundled {
        "The one inside this app, so it updates with the app."
    } else {
        "Installed apart from this app, so update it the way you installed it."
    }
}

/// Why the command line inside a copy run from a temporary place is not offered for linking
/// onto the `PATH`: a link to it would break once the app quits.
pub(crate) fn cannot_link(front: Front) -> String {
    match front {
        Front::MacMenuBar => {
            "Move Pitboard to your Applications folder first. Until then macOS runs it \
             from a temporary copy, and a link to that would break."
                .into()
        }
        // No app runs on Linux.
        Front::Gnome => "First move Pitboard to a folder it will stay in. Until then it runs \
                         from a temporary copy, and a link to that would break."
            .into(),
        Front::WindowsNotificationArea => {
            "Install Pitboard first. Until then Windows runs it from a temporary \
             copy, and a link to that would break."
                .into()
        }
    }
}

// Said in place of the update settings by a copy built from source, on both systems.
pub(crate) const NO_UPDATE_KEY: &str = "This copy of Pitboard can’t update itself: it was built \
                                        from source and carries no update key. A copy from a \
                                        release keeps itself up to date.";

// Windows' button sits beside its card's "Uninstall Pitboard".
pub(crate) fn uninstall_pitboard(front: Front) -> String {
    match front {
        Front::WindowsNotificationArea => "Uninstall…",
        Front::MacMenuBar | Front::Gnome => "Uninstall Pitboard…",
    }
    .into()
}

pub(crate) const UNINSTALL_QUESTION: &str = "Uninstall Pitboard?";

pub(crate) const UNINSTALL_SAYS: &str = "Pitboard deletes its saved sign-ins, settings and \
                                         account windows. Each tool stays signed in to the \
                                         account in use, and you sign in to the others again to \
                                         use them.";

pub(crate) const UNINSTALL: &str = "Uninstall";

pub(crate) const UNINSTALLING: &str = "Uninstalling Pitboard…";

pub(crate) const KEPT_ITS_FOLDER: &str = "Pitboard couldn’t remove everything";

pub(crate) const UNINSTALL_NOT_KNOWN: &str =
    "Pitboard stopped before it could say how uninstalling went. Try again to finish it.";

// Why Pitboard's folder stayed, which the command line says with its paths.
pub(crate) fn folder_kept(stayed: crate::Uninstalled) -> Option<String> {
    Some(match stayed {
        crate::Uninstalled::Removed => return None,
        crate::Uninstalled::Pending(1) => {
            "A saved sign-in couldn’t be deleted yet, so Pitboard kept its folder. Try again \
             in a moment."
                .into()
        }
        crate::Uninstalled::Pending(count) => format!(
            "{count} saved sign-ins couldn’t be deleted yet, so Pitboard kept its folder. Try \
             again in a moment."
        ),
        crate::Uninstalled::WarmingUp => {
            "An account is warming up, so Pitboard kept its folder. Try again once it’s done."
                .into()
        }
        crate::Uninstalled::HeldOpen => {
            "Another program has a file in Pitboard’s folder open, so Pitboard kept it. Quit \
             that program, then try again."
                .into()
        }
    })
}

/// A change as the activity list names it, by the verb the log keeps: "Switch", "Add", and
/// one it does not know yet by the verb itself made readable rather than not at all. The
/// core logs a switch as `use`, after the command that makes one.
pub(crate) fn change_verb(verb: &str) -> String {
    match verb {
        "use" | "switch" => "Switch".into(),
        "auto-switch" => "Automatic switch".into(),
        "auto-stay" => "Automatic switch not made".into(),
        "enroll" => "Add".into(),
        "forget" => "Remove".into(),
        "rename" => "Rename".into(),
        "renew" => "Sign-in kept fresh".into(),
        "abandon" => "Cancel a switch".into(),
        "repair" => "Repair".into(),
        "adopt" => "Adopt".into(),
        "uninstall" => "Uninstall".into(),
        "in-use" => "Sign-in in use changed".into(),
        "stow" => "Clean up a leftover sign-in file".into(),
        "park" => "Sign out for a sign-in".into(),
        "warm-up" => "Warm-up".into(),
        "warm-on" => "Warm-up turned on".into(),
        "warm-off" => "Warm-up turned off".into(),
        "warm-range" => "Warm-up hours changed".into(),
        _ => readable(verb),
    }
}

/// How a change ended: "Done", or what stopped it, from the code the log keeps.
pub(crate) fn change_outcome(outcome: &str) -> String {
    match outcome {
        "ok" => "Done".into(),
        "second_sign_in" => "Second sign-in".into(),
        "warm_up_not_started" => "Warm-up didn’t start".into(),
        "warm_up_limit_reached" => "Limit reached".into(),
        "warm_up_login_refused" => "Sign-in refused".into(),
        "parked_login_expired" => sign_in_standing(Standing::LoginExpired).into(),
        "parked_login_refused" => sign_in_standing(Standing::LoginRefused).into(),
        "nothing_parked" => sign_in_standing(Standing::NothingParked).into(),
        "already_enrolled" => "Already added".into(),
        "live_account_not_enrolled" => "Account in use not added".into(),
        "login_replaced" => "Sign-in replaced".into(),
        "no_login" => "No sign-in".into(),
        "already_parked" => "Already saved".into(),
        "parked_now" => "Saved".into(),
        "park_kept" => "Saved sign-in kept".into(),
        "warm_up_unreachable" => "Anthropic out of reach".into(),
        "warm_up_timed_out" => "Took too long".into(),
        "warm_up_failed" => "Claude Code failed".into(),
        _ => readable(outcome),
    }
}

/// Who asked for a change: this app, a terminal, a line written before that was recorded,
/// and a caller the app does not know yet by its own name.
pub(crate) fn change_caller(caller: &str) -> String {
    match caller {
        "app" => "Pitboard app".into(),
        "cli" => "Command line".into(),
        "unknown" => "Unknown".into(),
        _ => capitalised(caller),
    }
}

/// A code as words: "label_taken" is "Label taken".
fn readable(code: &str) -> String {
    capitalised(&code.replace('_', " "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::present::words::{
        ACCOUNT, CANCELLED, CLEAR, DOWNLOADED, OPEN, add_a_second, cannot_open_link, checking,
        could_not_read_accounts, no_link, no_site_account, open_in_browser, open_link_as,
        quit_with_downloads, switch_button, try_again, window_open,
    };

    /// A `pitboard` installed apart from the app is updated the way it was installed, and none
    /// of those ways does it by itself. Saying it "updates on its own" read as though nothing
    /// needed doing, until the app moved on and the command line refused its newer files.
    #[test]
    fn a_command_line_installed_apart_says_how_to_update_it() {
        assert_eq!(
            update_note(true),
            "The one inside this app, so it updates with the app."
        );
        assert_eq!(
            update_note(false),
            "Installed apart from this app, so update it the way you installed it."
        );
    }

    /// The activity list names a change by the verb the log keeps, in words, and one it does
    /// not know yet by the verb itself made readable rather than not at all.
    #[test]
    fn a_change_is_named_by_its_verb_and_an_unknown_one_readably() {
        for (verb, name) in [
            ("switch", "Switch"),
            ("enroll", "Add"),
            ("forget", "Remove"),
            ("rename", "Rename"),
            ("renew", "Sign-in kept fresh"),
            ("abandon", "Cancel a switch"),
            ("repair", "Repair"),
            ("adopt", "Adopt"),
            ("uninstall", "Uninstall"),
        ] {
            assert_eq!(change_verb(verb), name, "{verb}");
        }
        assert_eq!(change_verb("sign_in"), "Sign in");
        assert_eq!(change_verb(""), "");
    }

    /// A switch is named a switch by the verb the core logs it under, `use`, after the
    /// command that makes one: `service::Pitboard::switch_to` has logged it so since the
    /// workspace began. The Swift app named only `switch`, which its fixture logged and the
    /// core never has, so a real switch read "Use" there.
    #[test]
    fn a_switch_is_named_by_the_verb_the_core_logs_it_under() {
        assert_eq!(change_verb("use"), "Switch");
    }

    /// A switch Pitboard made by itself is told apart from one somebody asked for, whichever
    /// front end made it: the core logs it as `auto-switch`, with the caller it always has.
    #[test]
    fn a_switch_made_by_itself_is_named_as_one() {
        assert_eq!(change_verb("auto-switch"), "Automatic switch");
    }

    /// Where Pitboard could not judge whether to switch by itself, the core logs `auto-stay`
    /// with the account it last knew in use and why, and the list says so in words.
    #[test]
    fn a_switch_not_made_by_itself_is_named_as_one() {
        assert_eq!(change_verb("auto-stay"), "Automatic switch not made");
        assert_eq!(change_outcome("not_identified"), "Not identified");
    }

    /// What Pitboard found changed outside it in whose login a tool has stored, which the core
    /// logs as `in-use`, is named for what happened, since its subject is the account signed
    /// in on one line and the account whose login went on another, and said as it ended.
    #[test]
    fn a_change_found_outside_pitboard_is_named_for_what_happened() {
        assert_eq!(change_verb("in-use"), "Sign-in in use changed");
        assert_eq!(change_outcome("signed_in_outside"), "Signed in outside");
        assert_eq!(change_outcome("login_replaced"), "Sign-in replaced");
    }

    /// Putting away a login left in a file, which the core logs as `stow`, is named for what
    /// it does, and said as it ended: what became of the login.
    #[test]
    fn cleaning_up_a_leftover_sign_in_file_is_named_for_what_it_does() {
        assert_eq!(change_verb("stow"), "Clean up a leftover sign-in file");
        assert_eq!(change_outcome("park_kept"), "Saved sign-in kept");
        assert_eq!(change_outcome("second_sign_in"), "Second sign-in");
    }

    #[test]
    fn the_settings_and_the_open_link_window_say_these() {
        assert_eq!(share_range(50, 99), "From 50% to 99%");
        assert_eq!(
            open_link_as("chatgpt.com"),
            "Open this chatgpt.com link as:"
        );
        assert_eq!(
            (
                cannot_open_link(Front::MacMenuBar),
                cannot_open_link(Front::WindowsNotificationArea)
            ),
            (
                pitboard_sites::LinkRefusal::TITLE.into(),
                "Can’t open this link".into()
            )
        );
        assert_eq!(
            (
                could_not_read_accounts(Front::MacMenuBar),
                could_not_read_accounts(Front::WindowsNotificationArea)
            ),
            (
                "Couldn’t Read Accounts".into(),
                "Couldn’t read accounts".into()
            )
        );
        assert_eq!(
            (
                try_again(Front::MacMenuBar),
                try_again(Front::WindowsNotificationArea)
            ),
            ("Try Again".into(), "Try again".into())
        );
        assert_eq!(
            (
                warm_up_now(Front::MacMenuBar),
                warm_up_now(Front::WindowsNotificationArea)
            ),
            ("Warm Up Now".into(), "Warm up now".into())
        );
        assert_eq!(add_a_second(None), "Add a second account");
        assert_eq!(add_a_second(Some("Codex")), "Add a second Codex account");
        assert_eq!(
            no_link(Front::MacMenuBar),
            (
                "No Link to Open".into(),
                "Share a claude.ai or chatgpt.com page with Pitboard from your browser’s Share \
                 menu to open it as one of your accounts."
                    .into()
            )
        );
        assert_eq!(no_link(Front::WindowsNotificationArea).0, "No link to open");
        assert_eq!(window_open("a@example.com"), "a@example.com, window open");
        assert_eq!(OPEN, "Open");
        assert_eq!(
            no_site_account(Front::MacMenuBar, "chatgpt.com"),
            (
                "No chatgpt.com Account".into(),
                "None of the accounts Pitboard has opens chatgpt.com. Add one, and this link \
                 waits here until you choose it."
                    .into()
            )
        );
        assert_eq!(
            no_site_account(Front::WindowsNotificationArea, "chatgpt.com").0,
            "No chatgpt.com account"
        );
        assert_eq!(
            (
                open_in_browser(Front::MacMenuBar),
                open_in_browser(Front::WindowsNotificationArea)
            ),
            ("Open in Browser".into(), "Open in browser".into())
        );
        assert_eq!(ACCOUNT, "Account");
        assert_eq!(
            (CLEAR, DOWNLOADED, CANCELLED),
            ("Clear", "Downloaded", "Cancelled")
        );
        assert_eq!(
            quit_with_downloads(Front::MacMenuBar, 1),
            (
                "A download is in progress. Quit anyway?".into(),
                "Quitting Pitboard stops them, and they won’t resume.",
                "Quit"
            )
        );
        assert_eq!(
            quit_with_downloads(Front::WindowsNotificationArea, 2),
            (
                "2 downloads are in progress. Exit anyway?".into(),
                "Exiting Pitboard stops them, and they won’t resume.",
                "Exit"
            ),
            "as the icon's menu says Exit"
        );
        assert_eq!(
            (
                switch_button(Front::MacMenuBar, "personal"),
                switch_button(Front::WindowsNotificationArea, "personal")
            ),
            ("Switch".into(), "Switch to personal".into()),
            "the Mac's category registered Switch"
        );
        assert_eq!(BEFORE_RUNNING_OUT, "Before an account runs out");
        assert_eq!(AT_LIMIT, "When a limit reaches");
        assert_eq!(at_limit_label(95), "When a limit reaches 95%");
        assert_eq!(AT, "At");
        let both = |choice| {
            (
                at_limit_choice(Front::MacMenuBar, choice),
                at_limit_choice(Front::WindowsNotificationArea, choice),
            )
        };
        assert_eq!(
            both(AtLimit::Nothing),
            ("Do Nothing".into(), "Do nothing".into())
        );
        assert_eq!(
            both(AtLimit::Notify),
            ("Notify Me".into(), "Notify me".into())
        );
        assert_eq!(
            both(AtLimit::Switch),
            ("Switch Automatically".into(), "Switch automatically".into())
        );
        assert_eq!(
            at_limit_note(Front::MacMenuBar, AtLimit::Nothing, 95, false, 33),
            None
        );
        assert_eq!(
            at_limit_note(Front::MacMenuBar, AtLimit::Notify, 95, true, 33).as_deref(),
            Some(
                "Pitboard notifies you once a limit of the account in use reaches 95%, while the \
                 app is open."
            )
        );
        assert_eq!(
            at_limit_note(Front::MacMenuBar, AtLimit::Switch, 95, false, 33).as_deref(),
            Some(
                "Pitboard switches Claude Code at 95%, and its running sessions follow within \
                 about 33 seconds."
            )
        );
        assert_eq!(
            at_limit_note(Front::MacMenuBar, AtLimit::Switch, 95, true, 33).as_deref(),
            Some(
                "Pitboard switches Claude Code at 95%, but its running sessions don’t follow while \
                 a sign-in file is left behind the keychain."
            )
        );
        // On Linux the file is Claude Code's store, with no keychain in front of it.
        assert_eq!(
            at_limit_note(Front::Gnome, AtLimit::Switch, 95, true, 33).as_deref(),
            Some(
                "Pitboard switches Claude Code at 95%, but its running sessions don’t follow while \
                 a sign-in file is left behind the store it keeps its sign-in in."
            )
        );
        assert_eq!(
            (
                warm_up(Front::MacMenuBar),
                warm_up(Front::WindowsNotificationArea)
            ),
            ("Warm-Up".into(), "Warm-up".into())
        );
        assert_eq!(NONE_TO_WARM, "Add a Claude Code account to warm it up.");
        assert_eq!(
            WARM_UP_NOTE,
            "Pitboard starts each account’s 5-hour limit again as soon as it resets, within \
             these hours, while it’s open."
        );
        assert_eq!(
            warm_up_failed("Claude Code failed."),
            "The last warm-up didn’t start: Claude Code failed."
        );
        assert_eq!(
            next_warm_up("work", Some("22:50")),
            "Next warm-up: work at 22:50."
        );
        assert_eq!(next_warm_up("work", None), "Warming up work now.");
        assert_eq!(IS_THIS_ALLOWED, "Is this allowed?");
        assert_eq!(NOT_INSTALLED, "Not installed");
    }

    #[test]
    fn uninstalling_is_said_in_these_words() {
        assert_eq!(
            (
                uninstall_pitboard(Front::MacMenuBar),
                uninstall_pitboard(Front::WindowsNotificationArea)
            ),
            ("Uninstall Pitboard…".into(), "Uninstall…".into()),
            "it asks before it acts"
        );
        assert_eq!(UNINSTALL_QUESTION, "Uninstall Pitboard?");
        assert_eq!(
            UNINSTALL_SAYS,
            "Pitboard deletes its saved sign-ins, settings and account windows. Each tool stays \
             signed in to the account in use, and you sign in to the others again to use them."
        );
        assert_eq!(UNINSTALL, "Uninstall");
        assert_eq!(UNINSTALLING, "Uninstalling Pitboard…");
        assert_eq!(KEPT_ITS_FOLDER, "Pitboard couldn’t remove everything");
        assert_eq!(folder_kept(crate::Uninstalled::Removed), None);
        assert_eq!(
            folder_kept(crate::Uninstalled::Pending(1)).as_deref(),
            Some(
                "A saved sign-in couldn’t be deleted yet, so Pitboard kept its folder. Try again \
                 in a moment."
            )
        );
        assert_eq!(
            folder_kept(crate::Uninstalled::Pending(2)).as_deref(),
            Some(
                "2 saved sign-ins couldn’t be deleted yet, so Pitboard kept its folder. Try again \
                 in a moment."
            )
        );
        assert_eq!(
            folder_kept(crate::Uninstalled::WarmingUp).as_deref(),
            Some(
                "An account is warming up, so Pitboard kept its folder. Try again once it’s done."
            )
        );
        assert_eq!(
            folder_kept(crate::Uninstalled::HeldOpen).as_deref(),
            Some(
                "Another program has a file in Pitboard’s folder open, so Pitboard kept it. Quit \
                 that program, then try again."
            )
        );
    }

    /// A change that worked says so in a word, and one that did not says what stopped it,
    /// from the code the log keeps.
    #[test]
    fn a_change_says_how_it_ended() {
        assert_eq!(change_outcome("ok"), "Done");
        assert_eq!(change_outcome("parked_login_expired"), "Sign-in expired");
        assert_eq!(change_outcome("label_taken"), "Label taken");
        assert_eq!(change_outcome("refused"), "Refused");
    }

    /// Who asked for a change: this app, a terminal, or a line written before that was
    /// recorded, and a caller the app does not know yet by its own name.
    #[test]
    fn a_change_says_who_asked_for_it() {
        assert_eq!(change_caller("app"), "Pitboard app");
        assert_eq!(change_caller("cli"), "Command line");
        assert_eq!(change_caller("unknown"), "Unknown");
        assert_eq!(change_caller("schedule"), "Schedule");
    }

    /// What the settings and the machine pane say of this machine name it as each system
    /// names itself, and say what a copy run from a temporary place is to do in that system's
    /// words. No app runs on Linux, so its sentences name no system's own places.
    #[test]
    fn what_is_said_of_this_machine_names_it_as_its_system_does() {
        assert_eq!(
            no_scheduler(Front::MacMenuBar),
            "This Mac has no scheduler Pitboard knows how to write to."
        );
        assert_eq!(
            no_scheduler(Front::Gnome),
            "This computer has no scheduler Pitboard knows how to write to."
        );
        assert_eq!(
            no_scheduler(Front::WindowsNotificationArea),
            "This PC has no scheduler Pitboard knows how to write to."
        );
        assert_eq!(checking(Front::MacMenuBar), "Checking this Mac…");
        assert_eq!(checking(Front::Gnome), "Checking this computer…");
        assert_eq!(
            checking(Front::WindowsNotificationArea),
            "Checking this PC…"
        );
        for front in [
            Front::MacMenuBar,
            Front::Gnome,
            Front::WindowsNotificationArea,
        ] {
            assert_eq!(
                cannot_schedule(front, false),
                "This copy of Pitboard has no command line inside it to run on a schedule."
            );
        }
        assert_eq!(
            cannot_schedule(Front::MacMenuBar, true),
            "Move Pitboard to your Applications folder first. Until then macOS runs it from a \
             temporary copy, which is gone once Pitboard quits."
        );
        assert_eq!(
            cannot_link(Front::MacMenuBar),
            "Move Pitboard to your Applications folder first. Until then macOS runs it from a \
             temporary copy, and a link to that would break."
        );
        assert_eq!(
            cannot_schedule(Front::Gnome, true),
            "First move Pitboard to a folder it will stay in. Until then it runs from a \
             temporary copy, which is gone once Pitboard quits."
        );
        assert_eq!(
            cannot_link(Front::Gnome),
            "First move Pitboard to a folder it will stay in. Until then it runs from a \
             temporary copy, and a link to that would break."
        );
        assert_eq!(
            cannot_schedule(Front::WindowsNotificationArea, true),
            "Install Pitboard first. Until then Windows runs it from a temporary copy, which is \
             gone once Pitboard quits."
        );
        assert_eq!(
            cannot_link(Front::WindowsNotificationArea),
            "Install Pitboard first. Until then Windows runs it from a temporary copy, and a \
             link to that would break."
        );
        for said in [
            cannot_schedule(Front::Gnome, true),
            cannot_link(Front::Gnome),
            cannot_schedule(Front::WindowsNotificationArea, true),
            cannot_link(Front::WindowsNotificationArea),
        ] {
            assert!(
                !said.contains("macOS") && !said.contains("Applications"),
                "{said}"
            );
        }
    }

    // A twelve-hour clock, as an American's says the warm-up's hours.
    #[test]
    fn the_warm_up_hours_are_said_on_the_persons_clock() {
        let twelve = |minutes: u16| {
            let (hours, minutes) = (minutes / 60, minutes % 60);
            let half = if hours < 12 { "AM" } else { "PM" };
            Some(format!("{}:{minutes:02} {half}", (hours + 11) % 12 + 1))
        };
        assert_eq!(
            warm_hours(6 * 60, 22 * 60, twelve).as_deref(),
            Some("6:00 AM to 10:00 PM")
        );
        assert_eq!(
            warm_hours(22 * 60 + 30, 6 * 60, twelve).as_deref(),
            Some("10:30 PM to 6:00 AM")
        );
        assert_eq!(
            warm_hours(0, 12 * 60, twelve).as_deref(),
            Some("12:00 AM to 12:00 PM")
        );
        assert_eq!(
            warm_hours(6 * 60, 24 * 60, twelve).as_deref(),
            Some("6:00 AM to midnight")
        );
        assert_eq!(warm_hours(0, 24 * 60, twelve).as_deref(), Some("All day"));
        assert_eq!(
            warm_hours(0, 24 * 60, |_| None).as_deref(),
            Some("All day"),
            "the whole day needs no clock"
        );
        assert_eq!(warm_hours(6 * 60, 22 * 60, |_| None), None);
    }

    #[test]
    fn renewal_and_the_command_line_are_said_in_these_words() {
        assert_eq!(
            renewal_about(Front::MacMenuBar, false),
            "Your Mac refreshes your saved sign-ins once a day, so one you leave alone doesn’t \
             expire."
        );
        assert_eq!(
            renewal_about(Front::MacMenuBar, true),
            "Your Mac refreshes saved sign-ins, except Claude Desktop’s, once a day so they don’t \
             expire."
        );
        assert_eq!(
            renewal_about(Front::WindowsNotificationArea, false),
            "Your PC refreshes your saved sign-ins once a day, so one you leave alone doesn’t \
             expire."
        );
        assert_eq!(
            NEW_TERMINALS,
            "Pitboard’s installer puts pitboard on your PATH, which terminals opened after it ran \
             see."
        );
        assert_eq!(
            NO_UPDATE_KEY,
            "This copy of Pitboard can’t update itself: it was built from source and carries no \
             update key. A copy from a release keeps itself up to date."
        );
    }

    #[test]
    fn warm_ups_held_back_by_a_switch_say_it_as_automatic_switching_does() {
        use pitboard_core::duty::warm::Halt;
        assert_eq!(
            warming_halted(&Halt::SwitchInterrupted),
            "Pitboard isn’t warming up accounts: a switch didn’t finish, and the next change you \
             make finishes it."
        );
    }
}
