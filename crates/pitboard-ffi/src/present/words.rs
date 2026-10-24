//! What both apps say and the command line does not, each sentence a function of typed values,
//! as `pitboard_core::words` holds what all three say. English, for now: a sentence is never
//! a format string a view fills in, so each can read a translation later by its name.
//!
//! What the command line says too is not here but there, and called from here: a limit's
//! names, a reset, a pace, a parked login's life. Each of its sentences an app shows passes
//! `typeset` first.

use crate::model::Sheet;
use crate::model::state::{Recording, in_its_app};
use crate::{Level, Warning};
use pitboard_core::error::{Cause, Error, LoginDoubt, Then, Unidentified};
use pitboard_core::host::{Os, Private};
use pitboard_core::label::Unusable;
use pitboard_core::provider::ProviderId;
use pitboard_core::service::{Stored, Warning as CoreWarning};
use pitboard_core::state::{Key, SetAside};
use pitboard_core::time::{DAY, HOUR, MINUTE};
use pitboard_core::usage::pace::{Pace, Standing};
use pitboard_core::usage::whole;
use pitboard_core::words::{capitalised, figure, listed};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

pub(crate) const NOT_KNOWN_HOW_IT_WENT: &str =
    "Pitboard stopped before it could say how this went. Refresh to see where things stand.";

pub(crate) fn could_not_switch(qualified: &str) -> String {
    format!("Couldn’t switch to {}", Key::parse(qualified).label)
}

pub(crate) const COULD_NOT_GIVE_UP: &str = "Couldn’t give up on the interrupted switch";

pub(crate) fn could_not_sign_in(name: &str) -> String {
    format!("Couldn’t sign in to {name}")
}

pub(crate) fn could_not_forget(qualified: &str) -> String {
    format!("Couldn’t forget {}", Key::parse(qualified).label)
}

pub(crate) fn could_not_rename(label: &str) -> String {
    format!("Couldn’t rename {label}")
}

pub(crate) const COULD_NOT_UPDATE_CONFIG: &str = "Couldn’t update Claude Code’s config";

pub(crate) const COULD_NOT_TELL_WHOSE: &str = "Couldn’t tell whose login it is";

pub(crate) const COULD_NOT_STOW: &str = "Couldn’t put the login away";

pub(crate) const COULD_NOT_CHANGE_WARM: &str = "Couldn’t change which accounts are warmed up";

pub(crate) fn could_not_warm(qualified: &str) -> String {
    format!("Couldn’t warm up {}", Key::parse(qualified).label)
}

pub(crate) const COULD_NOT_NAME: &str = "Couldn’t name this account";

// A sign-in that put the new login of `name` in use: `again` where it was enrolled already.
pub(crate) fn signed_in_now(name: &str, again: bool, app: Option<&str>) -> String {
    match (again, app) {
        (true, None) => format!("Signed in to {name} again. Its new login is the one in use now."),
        (false, None) => {
            format!("Enrolled {name}, the account signed in now. Its new login is the one in use.")
        }
        (true, Some(app)) => format!("Signed in to {name} again. {app} is signed in to it now."),
        (false, Some(app)) => format!("Added {name}, the account {app} is signed in to now."),
    }
}

// What recording the account an app shows from `from` failed under.
pub(crate) fn could_not_enrol(from: Option<&Sheet>, name: &str) -> String {
    match from {
        Some(Sheet::Add { .. }) => format!("Couldn’t add {name}"),
        Some(Sheet::SignInAgain { .. }) => could_not_sign_in(name),
        _ => COULD_NOT_NAME.into(),
    }
}

// A parked login that switches for now, as `pitboard status` hints a sign-in for it: one due
// to expire, or one whose sign-in may have lapsed already.
pub(crate) fn sign_in_hint(life: &str, lapsed: bool) -> String {
    if lapsed {
        format!("{life}: sign in again")
    } else {
        format!("{life}: sign in again to keep it")
    }
}

/// A button's, a menu item's or a heading's words as each system writes them: title case
/// on macOS, as Apple's guidelines have it, and sentence case on Windows, which Microsoft's
/// writing guidance gives all of an app's text.
pub(crate) fn titled(os: Os, title_case: &str, sentence_case: &str) -> String {
    match os {
        Os::Windows => sentence_case,
        Os::MacOs | Os::Linux => title_case,
    }
    .to_owned()
}

/// How many characters a person counts in `text`: extended grapheme clusters, as Swift counts
/// a `String`'s `Character`s. "é" written as "e" and a combining accent is one, and so is a
/// flag of two regional indicators or a family joined by zero-width joiners.
fn characters(text: &str) -> usize {
    text.graphemes(true).count()
}

/// A label as the menu bar has room for it: whole up to twelve characters, and past that its
/// first eleven and an ellipsis. Counted as a person counts them, so no cut falls inside an
/// accented letter, a flag or an emoji, as the Swift app's `prefix(11)` cut none.
pub(crate) fn bar_name(label: &str) -> String {
    if characters(label) <= 12 {
        return label.to_owned();
    }
    let mut cut: String = label.graphemes(true).take(11).collect();
    cut.push('…');
    cut
}

/// `numerator / denominator` rounded to the nearest whole, a half to the even one, as
/// Foundation rounds a span of time it says.
fn rounded_half_even(numerator: i64, denominator: i64) -> i64 {
    let (whole, rest) = (
        numerator.div_euclid(denominator),
        numerator.rem_euclid(denominator),
    );
    match (2 * rest).cmp(&denominator) {
        std::cmp::Ordering::Less => whole,
        std::cmp::Ordering::Greater => whole + 1,
        std::cmp::Ordering::Equal => whole + (whole & 1),
    }
}

/// `count` of `unit`, in words: "1 hour", "2 hours", "0 hours".
fn units(count: i64, unit: &str) -> String {
    if count == 1 {
        format!("1 {unit}")
    } else {
        format!("{count} {unit}s")
    }
}

/// A span of time as VoiceOver reads it inside a sentence: "1 hour, 30 minutes", "3 days".
///
/// Foundation's `Duration.UnitsFormatStyle` said it in the Swift app: days, hours and
/// minutes, wide, at most two of them, in `en_US_POSIX`, and never under a minute. Measured
/// on macOS 27.0 with Swift 6.4, over 100 spans. It rounds the span to whole minutes, a half
/// to the even one, to see which units it has, and says the two largest that are not
/// nought. The last it says is the rest of the span in that unit, rounded the same way,
/// which can carry into the one before it: 1 day 23 hours 30 minutes is "2 days, 0 hours".
pub(crate) fn spoken_span(seconds: i64) -> String {
    let seconds = seconds.max(MINUTE);
    let minutes = rounded_half_even(seconds, MINUTE);
    let (days, hours, rest) = (minutes / (24 * 60), minutes % (24 * 60) / 60, minutes % 60);
    let named: Vec<(i64, &str)> = [
        (days, DAY, "day"),
        (hours, HOUR, "hour"),
        (rest, MINUTE, "minute"),
    ]
    .into_iter()
    .filter(|(count, ..)| *count > 0)
    .map(|(_, length, unit)| (length, unit))
    .take(2)
    .collect();
    match named[..] {
        [(length, unit)] => units(rounded_half_even(seconds, length), unit),
        [(first, first_unit), (second, second_unit)] => {
            let mut whole = seconds / first;
            let mut part = rounded_half_even(seconds - whole * first, second);
            if part * second == first {
                whole += 1;
                part = 0;
            }
            format!("{}, {}", units(whole, first_unit), units(part, second_unit))
        }
        // Every span is at least a minute, so it has a unit.
        _ => units(1, "minute"),
    }
}

/// A limit as VoiceOver says it: "5-hour limit, 42 percent used, 38 percent under an even
/// pace, resets in 3 hours". The column beside the bar says "5h" and "resets in 30m", which
/// is read letter by letter or as a unit: "m" is read as "meters". Once the reset is due it
/// says so, as the column does. `name` is the limit's sentence name with its scope, "weekly
/// Fable".
pub(crate) fn spoken_limit(
    name: &str,
    percent: f64,
    resetting_in: Option<i64>,
    pace: Option<&Pace>,
) -> String {
    let mut used = format!("{name} limit, {} percent used", whole(percent));
    if let Some(pace) = pace {
        let points = whole(pace.delta.abs());
        used.push_str(&match pace.standing {
            Standing::Over { .. } => format!(", {points} percent over an even pace"),
            Standing::Under => format!(", {points} percent under an even pace"),
            Standing::Even => ", on an even pace".into(),
        });
    }
    match resetting_in {
        None => used,
        Some(seconds) if seconds <= 0 => format!("{used}, resetting now"),
        Some(seconds) => format!("{used}, resets in {}", spoken_span(seconds)),
    }
}

/// When a limit runs out at its pace, beside its figure in the menu: "runs out in 20h
/// 18m", and under a minute "about to run out", as `pitboard status` says it.
pub(crate) fn runs_out_in(in_seconds: i64) -> String {
    match pitboard_core::words::time_to_run_out(in_seconds) {
        Some(left) => format!("runs out in {left}"),
        None => "about to run out".into(),
    }
}

/// What a bar's help says of the mark on it: how far from an even pace the limit is, what an
/// even pace would have used by now, and for a limit over pace, when it runs out.
pub(crate) fn pace_help(pace: &Pace) -> String {
    let even = format!(
        "{}: an even pace would have used {} of it by now.",
        pitboard_core::words::pace_column(pace),
        figure(pace.expected)
    );
    match pace.standing {
        Standing::Over { runs_out_in } => {
            match pitboard_core::words::time_to_run_out(runs_out_in) {
                Some(left) => format!("{even} At this pace it runs out in {left}."),
                None => format!("{even} At this pace it is about to run out."),
            }
        }
        Standing::Under | Standing::Even => even,
    }
}

// The tool is named in the title above it.
pub(crate) fn follows(adoption: &pitboard_core::provider::Adoption) -> String {
    sentence(&pitboard_core::words::after_a_switch(adoption, "Sessions"))
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

pub(crate) fn warning_heading(os: Os, warning: &Warning) -> &'static str {
    match warning.code.as_str() {
        "sessions_still_running" => "Open sessions still use the previous account",
        "sessions_keep_old_login" => "Open sessions still use the old login",
        "sessions_unknown" => "Couldn’t tell which sessions are open",
        "auth_overridden" => "An environment variable overrides the login",
        "fallback_login" => match os {
            Os::MacOs | Os::Linux => "A login file is left behind the keychain",
            Os::Windows => "A login file is left behind Credential Manager",
        },
        "parked_login_refused" => "A parked login was refused",
        "lock_compromised" => "The login may have been written twice",
        "parks_pending_removal" => "Old parked logins are still there",
        "written_on_the_command_line" => "A login was passed on the command line",
        "sign_in_parked_not_in_use" => "The new login was parked, not put in use",
        "interrupted_switch_finished" => "An interrupted switch was finished",
        "interrupted_switch_undone" => "An interrupted switch was undone",
        "recovery_undetermined" => SWITCH_WAITING,
        "login_replaced" => "A login was replaced outside Pitboard",
        "in_use_unconfirmed" => "The account in use is not confirmed",
        // `in_app_warning` words the two alike for an app.
        "config_names_another" if warning.provider.as_deref().is_some_and(in_its_app) => {
            "The account in use is not confirmed"
        }
        "config_names_another" => "Claude Code’s config names another account",
        "login_taken_as_named" => "A login was taken as the account named for it",
        "looks_signed_out" => "The account in use may be signed out",
        "set_aside" => "Pitboard set a folder aside",
        "move_waiting" => "An interrupted move is waiting",
        "move_not_given_up" => "An interrupted move wasn’t given up",
        "interrupted_move_undone" => "An interrupted move was undone",
        "interrupted_move_finished" => "An interrupted move was finished",
        "login_item_gone" => "Part of a login was gone",
        _ => "Pitboard has a warning",
    }
}

/// "1 login", "2 logins".
pub(crate) fn logins(count: u32) -> String {
    if count == 1 {
        "1 login".into()
    } else {
        format!("{count} logins")
    }
}

/// What an interrupted switch nothing can finish waits for, where the read did not say.
pub(crate) fn switch_waits_for(services: &str) -> String {
    format!("An interrupted switch can’t be finished until {services} answers.")
}

pub(crate) fn move_waits_too(tool: &str, program: &str) -> String {
    format!(
        "A {tool} move was interrupted too. With {program} quit, Give Up… settles it or gives it \
         up as well, and keeps every folder."
    )
}

// `moving`: each tool whose move waits, with its program.
pub(crate) fn give_up_stuck(moving: &[(String, &str)]) -> String {
    let stops = "Every login is kept, and nothing is deleted. Pitboard stops trying to finish it";
    if moving.is_empty() {
        return format!("{stops}.");
    }
    let tools: Vec<&str> = moving.iter().map(|(tool, _)| tool.as_str()).collect();
    let mut programs: Vec<&str> = Vec::new();
    for &(_, program) in moving {
        if !programs.contains(&program) {
            programs.push(program);
        }
    }
    format!(
        "{stops} and, with {} quit, settles or gives up the interrupted {} move too.",
        listed(&programs),
        listed(&tools)
    )
}

/// What giving up on an interrupted switch kept.
pub(crate) fn gave_up(from: &str, to: &str, kept: u32) -> String {
    let from = if from.is_empty() {
        String::new()
    } else {
        format!(" from {from}")
    };
    format!(
        "The switch{from} to {to} was given up. {} kept, and nothing was deleted.",
        logins(kept)
    )
}

pub(crate) const DISMISS: &str = "Dismiss";

pub(crate) const SWITCH_WAITING: &str = "An interrupted switch is waiting";

pub(crate) const GIVING_UP_KEEPS_LOGINS: &str =
    "Giving up on it keeps every login. Nothing is deleted.";

pub(crate) const GIVE_UP_ON_SWITCH: &str = "Give up on the interrupted switch?";

pub(crate) const GIVE_UP_ON_MOVE: &str = "Give up on the interrupted move?";

pub(crate) fn give_up_on_move(app: &str) -> String {
    format!(
        "Every folder is kept and nothing is deleted. Whatever is in {app}’s folder stays there."
    )
}

pub(crate) const GAVE_UP_ON_SWITCH: &str = "Gave up on the interrupted switch";

pub(crate) const GAVE_UP_ON_MOVE: &str = "Gave up on the interrupted move";

pub(crate) const STOWED: &str = "Put away the login left in a file";

pub(crate) const COULD_NOT_READ_USAGE: &str = "Couldn’t read usage";

pub(crate) const LAST_MEASURED: &str = "The numbers shown are the last ones measured.";

// A sign-in inside an app left halfway: `again` names the account added already.
pub(crate) fn unfinished(tool: &str, again: Option<&str>) -> String {
    match again {
        Some(label) => format!("Signing in to {label} again isn’t finished"),
        None => format!("Adding a {tool} account isn’t finished"),
    }
}

pub(crate) fn finish(os: Os, again: bool) -> String {
    if again {
        titled(os, "Finish Signing In…", "Finish signing in…")
    } else {
        titled(os, "Finish Adding…", "Finish adding…")
    }
}

pub(crate) fn put_back(os: Os, label: &str) -> String {
    titled(
        os,
        &format!("Put {label} Back"),
        &format!("Put {label} back"),
    )
}

// `asks`: the button opens a sheet or a question, and ends with an ellipsis; the one that
// answers it does not.
pub(crate) fn give_up(os: Os, asks: bool) -> String {
    if asks {
        titled(os, "Give Up…", "Give up…")
    } else {
        titled(os, "Give Up", "Give up")
    }
}

pub(crate) fn put_away(os: Os, asks: bool) -> String {
    if asks {
        titled(os, "Put Away…", "Put away…")
    } else {
        titled(os, "Put Away", "Put away")
    }
}

pub(crate) fn sign_in_again(os: Os, asks: bool) -> String {
    if asks {
        titled(os, "Sign In Again…", "Sign in again…")
    } else {
        titled(os, "Sign In Again", "Sign in again")
    }
}

pub(crate) fn add_account_shown(os: Os, app: &str, asks: bool) -> String {
    let (title, sentence) = (
        format!("Add the Account {app} Shows"),
        format!("Add the account {app} shows"),
    );
    if asks {
        titled(os, &format!("{title}…"), &format!("{sentence}…"))
    } else {
        titled(os, &title, &sentence)
    }
}

pub(crate) fn update_config(os: Os) -> String {
    titled(
        os,
        "Update Claude Code’s Config",
        "Update Claude Code’s config",
    )
}

pub(crate) const SWITCH: &str = "Switch";

// "work has used 96% of its 5-hour limit.", then what came of it.
pub(crate) fn used_then(account: &str, used: &str, then: &str) -> String {
    format!(
        "{}. {then}",
        typeset(&pitboard_core::words::has_used(&quoted(account), used))
    )
}

pub(crate) const NOT_SWITCHED: &str = "Claude Code wasn’t switched";

pub(crate) const NOT_SWITCHING: &str = "Pitboard isn’t switching Claude Code";

pub(crate) const COULD_NOT_SWITCH_ITSELF: &str = "Pitboard couldn’t switch Claude Code";

pub(crate) const COULD_NOT_WARM_ITSELF: &str = "Pitboard couldn’t warm up an account";

// Under the setting, where switching by itself stopped on `message`: `until` it tries again.
pub(crate) fn auto_stopped(until: Option<&str>, message: &str) -> String {
    match until {
        Some(until) => {
            format!("Pitboard couldn’t switch Claude Code, and tries again at {until}: {message}")
        }
        None => format!("Pitboard can’t tell whether to switch Claude Code: {message}"),
    }
}

pub(crate) fn needs_signing_in(label: &str) -> String {
    format!("{label} needs signing in again")
}

pub(crate) const UNUSABLE_LOGIN: &str =
    "Its parked login can no longer be used, so Pitboard can’t switch to it.";

pub(crate) const SHOW_IN_PITBOARD: &str = "Show in Pitboard";

// What VoiceOver and Narrator call a switching row's spinner.
pub(crate) const SWITCHING: &str = "Switching";

pub(crate) fn in_use(os: Os) -> String {
    titled(os, "In Use", "In use")
}

pub(crate) const CLAUDE_CODE_MISSING: &str = "Claude Code isn’t installed";

pub(crate) const LEARN_TO_INSTALL: &str = "Learn how to install it";

/// The heading of a tool's last switch: "Switched to work", "Switched Codex to work" beside
/// another tool's accounts.
pub(crate) fn switched_to(tool: Option<&str>, label: &str) -> String {
    match tool {
        Some(tool) => format!("Switched {tool} to {label}"),
        None => format!("Switched to {label}"),
    }
}

/// The heading of a sign-in that put a new login in use: "work has a new login", "work in
/// Codex has a new login" beside another tool's accounts.
pub(crate) fn has_a_new_login(tool: Option<&str>, label: &str) -> String {
    match tool {
        Some(tool) => format!("{label} in {tool} has a new login"),
        None => format!("{label} has a new login"),
    }
}

/// Said before a countdown to when sessions already open follow a switch.
pub(crate) fn sessions_follow_in(tool: Option<&str>) -> String {
    match tool {
        Some(tool) => format!("{tool} sessions already open follow in"),
        None => "Sessions already open follow in".into(),
    }
}

/// "spare has 80% of its own left.", of the account advice offers, or "seat has no such
/// limit." of one whose plan does not limit it that way.
pub(crate) fn room_left(account: &str, left: Option<i64>) -> String {
    match left {
        Some(left) => format!("{account} has {left}% of its own left."),
        None => format!("{account} has no such limit."),
    }
}

/// "work has no 5-hour limit left", of the account in use that ran out.
pub(crate) fn ran_out(account: &str, limit: &str) -> String {
    format!("{account} has no {limit} limit left")
}

/// A sentence about one tool's account, said beside another tool's: "Claude Code: work has
/// no 5-hour limit left".
pub(crate) fn of_tool(tool: Option<&str>, sentence: &str) -> String {
    match tool {
        Some(tool) => format!("{tool}: {sentence}"),
        None => sentence.to_owned(),
    }
}

/// What a button that switches says: "Switch to spare".
pub(crate) fn switch_to(label: &str) -> String {
    format!("Switch to {label}")
}

/// The menu's one item for everything else to know about, more than one of them.
pub(crate) fn things_to_look_at(count: usize) -> String {
    format!("{count} things to look at")
}

/// An account named where nothing around it says which tool it is for, once more than one
/// tool is shown: "work (Codex)".
pub(crate) fn named_with_tool(label: &str, tool: &str) -> String {
    format!("{label} ({tool})")
}

/// An account as the menu names it: "work · Team 5x", and on Windows "work · Team 5x · 64%".
pub(crate) fn named_in_menu(
    os: Os,
    title: &str,
    plan: Option<&str>,
    figure: Option<&str>,
) -> String {
    let mut said = vec![title];
    said.extend(plan);
    // A Windows menu item has one line, where macOS puts the account's limits on a second.
    if matches!(os, Os::Windows) {
        said.extend(figure);
    }
    said.join(" · ")
}

/// The line in a menu when the numbers were read, as a time rather than an age: a menu can
/// stay open, and "just now" would still say so ten minutes later.
pub(crate) fn updated(clock: &str) -> String {
    format!("Updated {clock}")
}

/// A clock time where the person's own clock could not say it: in UTC, and named so, since
/// it is nobody's own time. "14:05 UTC", and "Wed 14:05 UTC" on another day of UTC than
/// `now`. Worked out from the two moments alone, so a snapshot asks nothing of the machine's
/// time zone, which only the app's `LocalTime` reads.
pub(crate) fn utc_clock(epoch: i64, now: i64) -> String {
    let (day, of_day) = (epoch.div_euclid(DAY), epoch.rem_euclid(DAY));
    let time = format!("{:02}:{:02} UTC", of_day / HOUR, of_day % HOUR / MINUTE);
    if day == now.div_euclid(DAY) {
        return time;
    }
    // 1 January 1970 was a Thursday.
    let weekday = match day.rem_euclid(7) {
        0 => "Thu",
        1 => "Fri",
        2 => "Sat",
        3 => "Sun",
        4 => "Mon",
        5 => "Tue",
        _ => "Wed",
    };
    format!("{weekday} {time}")
}

/// How a sentence names the machine the app runs on, as the account windows' alerts do.
pub(crate) fn this_machine(os: Os) -> &'static str {
    crate::account_windows::this_machine(os)
}

// What Pitboard needs of `tool` to sign in to it, where it did not find that.
pub(crate) fn missing(os: Os, tool: &str) -> Option<super::Missing> {
    let here = this_machine(os);
    let (title, detail, link_title, link) = match tool {
        "claude" => (
            format!("Pitboard can’t find Claude Code on {here}."),
            "Adding a Claude Code account runs Claude Code’s own sign-in. Install it, then come \
             back to this window.",
            titled(
                os,
                "How to Install Claude Code",
                "How to install Claude Code",
            ),
            super::notices::INSTALL_CLAUDE_CODE,
        ),
        "codex" => (
            format!("Pitboard can’t find the Codex CLI on {here}."),
            "Adding a Codex account runs the Codex CLI’s own sign-in. Pitboard doesn’t use \
             the copy inside the ChatGPT app. Install the CLI, then come back to this window.",
            titled(
                os,
                "How to Install the Codex CLI",
                "How to install the Codex CLI",
            ),
            INSTALL_CODEX_CLI,
        ),
        "desktop" => (
            format!("Pitboard can’t find Claude Desktop on {here}."),
            "Pitboard finds Claude Desktop by the data it keeps once it has been opened. Open \
             Claude, sign in, then come back to this window.",
            "Download Claude".to_owned(),
            DOWNLOAD_CLAUDE,
        ),
        _ => return None,
    };
    Some(super::Missing {
        title,
        detail: detail.into(),
        link_title,
        link: link.into(),
    })
}

// OpenAI's page for the Codex CLI, whose install section names each way to install it.
const INSTALL_CODEX_CLI: &str = "https://developers.openai.com/codex/cli";

const DOWNLOAD_CLAUDE: &str = "https://claude.ai/download";

// Why what was typed cannot be a new name, where something was. `picks_tool`: the sheet has a
// picker for the tool.
pub(crate) fn unusable_name(unusable: Unusable, picks_tool: bool) -> Option<String> {
    let said = match unusable {
        Unusable::Empty => return None,
        Unusable::NotOneWord => "A name is one word, with no spaces.",
        Unusable::Separator if picks_tool => "A name can’t contain “/”. Choose the tool above.",
        Unusable::Separator => "A name can’t contain “/”.",
    };
    Some(said.into())
}

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

/// A check's level, said in a word where a symbol shows it: a check that reads "state: fine"
/// without saying whether it passed is the same as not running it, and if two levels ever
/// sounded the same a broken check would pass for one that passed.
pub(crate) fn spoken_level(level: Level) -> &'static str {
    match level {
        Level::Ok => "Passed",
        Level::Warn => "Worth looking at",
        Level::Fail => "Failed",
    }
}

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

// The check the app adds after doctor's for a notification its system refused.
pub(crate) const NOTIFICATIONS: &str = "Notifications";

// The command line keeps doctor's own names, which maintenance-commands.mdx documents.
pub(crate) fn check_name(code: &str, name: &str) -> String {
    match code {
        "storage_v5" => "Claude Code’s new login storage".into(),
        "machine_id" => "Machine ID".into(),
        _ => typeset(&capitalised(name)),
    }
}

pub(crate) fn check_text(text: &str) -> String {
    typeset(&prose_capitalised(text))
}

// The apps' typography for the core's words. What the core quotes, in backticks or not, keeps
// every letter: a name, a command, a path, an address or a system's own message.
pub(crate) fn typeset(core: &str) -> String {
    let mut said = String::with_capacity(core.len());
    for (at, part) in core.split('`').enumerate() {
        if at % 2 == 1 {
            said.push_str(part);
            continue;
        }
        let mut done = 0;
        for kept in quoted_in(part) {
            let before = said.chars().next_back();
            said.push_str(&contracted(&curled(&part[done..kept.start], before)));
            said.push_str(&part[kept.clone()]);
            done = kept.end;
        }
        let before = said.chars().next_back();
        said.push_str(&contracted(&curled(&part[done..], before)));
    }
    said
}

// A name an app puts in the core's sentence, which `typeset` then leaves as it is.
pub(crate) fn quoted(name: &str) -> String {
    format!("`{name}`")
}

// Where the core quotes without backticks, in order: a word with a path's or a link's slash in
// it, an address up to its domain's end, and a system's message as Rust and Windows end one.
fn quoted_in(text: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut start = 0;
    for piece in text.split_inclusive(char::is_whitespace) {
        let word = piece.trim_end();
        if word.contains(['/', '\\']) {
            spans.push(start..start + word.len());
        } else if let Some(at) = word.find('@') {
            let domain = word[at + 1..]
                .find(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '-')))
                .map_or(word.len(), |end| at + 1 + end);
            spans.push(start..start + domain);
        }
        start += piece.len();
    }
    let number = |text: &str, radix| !text.is_empty() && text.chars().all(|c| c.is_digit(radix));
    for (at, _) in text.match_indices(" (") {
        let Some(code) = text[at + 2..].split_once(')').map(|(code, _)| code) else {
            continue;
        };
        let os = code
            .strip_prefix("os error ")
            .is_some_and(|n| number(n, 10));
        let windows = (code.strip_prefix("0x")).is_some_and(|n| n.len() == 8 && number(n, 16));
        if os || windows {
            let from = (text[..at].rfind(": ").map(|colon| colon + 2))
                .max(text[..at].rfind('(').map(|open| open + 1))
                .unwrap_or(0);
            spans.push(from..at + 2 + code.len() + 1);
        }
    }
    spans.sort_by_key(|span| span.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for span in spans {
        match merged.last_mut() {
            Some(last) if span.start <= last.end => last.end = last.end.max(span.end),
            _ => merged.push(span),
        }
    }
    merged
}

// Only an apostrophe that ends a word, or starts its "s" or a contraction's end: not one inside
// a name, such as o'brien@example.com's first.
fn curled(text: &str, before: Option<char>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut curled = String::with_capacity(text.len());
    for (at, &c) in chars.iter().enumerate() {
        if c != '\'' {
            curled.push(c);
            continue;
        }
        let previous = at.checked_sub(1).map(|at| chars[at]).or(before);
        let after: String = (chars[at + 1..].iter())
            .take_while(|c| c.is_alphanumeric())
            .collect();
        let apostrophe = match after.as_str() {
            "s" => previous.is_some_and(|c| !c.is_whitespace()),
            "" => previous == Some('s'),
            "t" => previous == Some('n'),
            "re" | "ve" | "ll" | "d" | "m" => previous.is_some_and(char::is_alphabetic),
            _ => false,
        };
        curled.push(if apostrophe { '’' } else { c });
    }
    curled
}

// The ones the apps' own words use.
const CONTRACTIONS: [(&str, &str); 10] = [
    ("cannot", "can’t"),
    ("could not", "couldn’t"),
    ("is not", "isn’t"),
    ("does not", "doesn’t"),
    ("did not", "didn’t"),
    ("do not", "don’t"),
    ("has not", "hasn’t"),
    ("have not", "haven’t"),
    ("had not", "hadn’t"),
    ("was not", "wasn’t"),
];

fn contracted(text: &str) -> String {
    let mut contracted = String::with_capacity(text.len());
    let mut rest = text;
    let mut starts_word = true;
    while let Some(c) = rest.chars().next() {
        let whole = |long: &str| {
            rest.get(..long.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(long))
                && !rest[long.len()..].starts_with(char::is_alphanumeric)
        };
        if starts_word
            && let Some((long, short)) = CONTRACTIONS.iter().find(|(long, _)| whole(long))
        {
            contracted.push_str(&if c.is_uppercase() {
                capitalised(short)
            } else {
                (*short).to_owned()
            });
            rest = &rest[long.len()..];
            starts_word = false;
            continue;
        }
        contracted.push(c);
        starts_word = !c.is_alphanumeric() && !matches!(c, '\'' | '’');
        rest = &rest[c.len_utf8()..];
    }
    contracted
}

// The command line prints the core's words after "error:"; an app shows them on their own,
// without a terminal's backticks.
pub(crate) fn sentence(text: &str) -> String {
    let text = typeset(&prose_capitalised(text));
    if text.is_empty() || text.ends_with(['.', '!', '?', '…']) {
        text
    } else {
        format!("{text}.")
    }
}

// Only a first word of lowercase letters, "warm-ups" included, is prose: a path, an email, a
// file name, a figure or a backticked name keeps its case.
fn prose_capitalised(text: &str) -> String {
    let first = text
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':'))
        .next()
        .unwrap_or_default();
    if first
        .split('-')
        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_lowercase()))
    {
        capitalised(text)
    } else {
        text.to_owned()
    }
}

/// A change as the activity list names it, by the verb the log keeps: "Switch", "Enrol", and
/// one it does not know yet by the verb itself made readable rather than not at all. The
/// core logs a switch as `use`, after the command that makes one.
pub(crate) fn change_verb(verb: &str) -> String {
    match verb {
        "use" | "switch" => "Switch".into(),
        "auto-switch" => "Automatic switch".into(),
        "auto-stay" => "Automatic switch not made".into(),
        "enroll" => "Enrol".into(),
        "forget" => "Forget".into(),
        "rename" => "Rename".into(),
        "renew" => "Renew".into(),
        "abandon" => "Give up on a switch".into(),
        "repair" => "Repair".into(),
        "adopt" => "Adopt".into(),
        "uninstall" => "Uninstall".into(),
        "in-use" => "Login in use changed".into(),
        "stow" => "Put away a login left in a file".into(),
        "park" => "Park for a sign-in".into(),
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
        "warm_up_not_started" => "No period started".into(),
        "warm_up_limit_reached" => "A limit is used up".into(),
        "warm_up_login_refused" => "Login refused".into(),
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

/// A code as words: "parked_login_expired" is "Parked login expired".
fn readable(code: &str) -> String {
    capitalised(&code.replace('_', " "))
}

pub(crate) const GIVE_IT_A_NAME: &str = "Give this account a name";

// `tool`: the tool named, where the account's is not the only one shown.
pub(crate) fn add_a_second(tool: Option<&str>) -> String {
    match tool {
        Some(tool) => format!("Add a second {tool} account"),
        None => "Add a second account".into(),
    }
}

pub(crate) const NO_ACCOUNTS_TO_SHOW: &str = "No accounts to show";

pub(crate) fn could_not_read_accounts(os: Os) -> String {
    titled(os, "Couldn’t Read Accounts", "Couldn’t read accounts")
}

pub(crate) fn try_again(os: Os) -> String {
    titled(os, "Try Again", "Try again")
}

pub(crate) fn warm_up_now(os: Os) -> String {
    titled(os, "Warm Up Now", "Warm up now")
}

pub(crate) fn open_link_as(site: &str) -> String {
    format!("Open this {site} link as:")
}

pub(crate) fn cannot_open_link(os: Os) -> String {
    titled(os, "Can’t Open This Link", "Can’t open this link")
}

pub(crate) fn no_link(os: Os) -> (String, String) {
    (
        titled(os, "No Link to Open", "No link to open"),
        format!(
            "Share a {} page with Pitboard from your browser’s Share menu to open it as one of \
             your accounts.",
            pitboard_sites::Site::names(pitboard_sites::Conjunction::Or)
        ),
    )
}

// What stands in for the activity list while it is empty: its title, and why.
pub(crate) fn no_activity(os: Os) -> (String, &'static str) {
    (
        titled(os, "No Activity", "No activity"),
        "Pitboard lists every change it makes here.",
    )
}

/// Said under the switch where the machine has no scheduler Pitboard writes to: "This Mac has
/// no scheduler Pitboard knows how to write to."
pub(crate) fn no_scheduler(os: Os) -> String {
    format!(
        "{} has no scheduler Pitboard knows how to write to.",
        capitalised(this_machine(os))
    )
}

/// Why daily renewal cannot be turned on from this copy of the app. The schedule runs the
/// command line inside the app long after the app has quit, so it needs one that will still
/// be there: a copy run from a temporary place is gone by then, and without one inside the
/// app there is only the app itself to schedule, which renews nothing.
pub(crate) fn cannot_schedule(os: Os, temporary: bool) -> String {
    if !temporary {
        return "This copy of Pitboard has no command line inside it to run on a schedule.".into();
    }
    match os {
        Os::MacOs => "Move Pitboard to your Applications folder first. Until then macOS runs it \
                      from a temporary copy, which is gone once Pitboard quits."
            .into(),
        // No app runs on Linux, and nothing there runs one from a temporary copy.
        Os::Linux => "First move Pitboard to a folder it will stay in. Until then it runs \
                      from a temporary copy, which is gone once Pitboard quits."
            .into(),
        // The Windows app is installed by its installer, and a copy run from anywhere else,
        // such as from inside a zip, is the one a person is told to install.
        Os::Windows => "Install Pitboard first. Until then Windows runs it from a temporary \
                        copy, which is gone once Pitboard quits."
            .into(),
    }
}

/// Why the command line inside a copy run from a temporary place is not offered for linking
/// onto the `PATH`: a link to it would break once the app quits.
pub(crate) fn cannot_link(os: Os) -> String {
    match os {
        Os::MacOs => "Move Pitboard to your Applications folder first. Until then macOS runs it \
                      from a temporary copy, and a link to that would break."
            .into(),
        // No app runs on Linux, and nothing there runs one from a temporary copy.
        Os::Linux => "First move Pitboard to a folder it will stay in. Until then it runs \
                      from a temporary copy, and a link to that would break."
            .into(),
        Os::Windows => "Install Pitboard first. Until then Windows runs it from a temporary \
                        copy, and a link to that would break."
            .into(),
    }
}

// Said in place of the update settings by a copy built from source, on both systems.
pub(crate) const NO_UPDATE_KEY: &str = "This copy of Pitboard can’t update itself: it was built \
                                        from source and carries no update key. A copy from a \
                                        release keeps itself up to date.";

/// What stands in for doctor's checks before there are any: "Checking this Mac…".
pub(crate) fn checking(os: Os) -> String {
    format!("Checking {}…", this_machine(os))
}

/// When the checks shown were made, at a clock time the person's clock says: "Checked at
/// 14:05".
pub(crate) fn checked_at(clock: &str) -> String {
    format!("Checked at {clock}")
}

pub(crate) fn not_posted(title: &str, clock: &str, reason: &str) -> String {
    format!("“{title}” not posted at {clock} · {reason}")
}

pub(crate) const NO_USAGE_READ: &str = "No usage read";

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

pub(crate) fn share_range(lowest: u8, highest: u8) -> String {
    format!("From {lowest}% to {highest}%")
}

// The installer puts the app's `pitboard` on the `PATH`, which only terminals opened after see.
pub(crate) const NEW_TERMINALS: &str =
    "Pitboard’s installer puts pitboard on your PATH, which terminals opened after it ran see.";

// What daily renewal does, under its switch, and that a parked app login never is renewed.
pub(crate) fn renewal_about(os: Os, in_its_app: bool) -> String {
    let scheduler = match os {
        Os::MacOs => "Your Mac’s own scheduler",
        Os::Linux => "This computer’s own scheduler",
        Os::Windows => "Your PC’s own scheduler",
    };
    let about = format!(
        "{scheduler} renews your parked logins once a day, so one you leave alone does not expire."
    );
    if in_its_app {
        format!("{about} A parked Claude Desktop login is never renewed.")
    } else {
        about
    }
}

// The lines of the notice for a sign-in inside `app` left halfway, `put_back` parked for it.
pub(crate) fn awaiting_lines(app: &str, label: &str, put_back: &str, again: bool) -> Vec<String> {
    let then = if again {
        format!(
            "Sign in to {label} in {app}, quit {app}, then choose Finish Signing In. Put \
             {put_back} Back undoes it."
        )
    } else {
        format!(
            "Sign in to the account you’re adding in {app}, quit {app}, then choose Finish \
             Adding. Put {put_back} Back undoes it."
        )
    };
    vec![
        format!("{app} is signed out for a sign-in, and {put_back} is parked."),
        then,
    ]
}

// What the sheet says while Pitboard waits for `app` to quit signed in, as the wait last said.
pub(crate) fn in_app_waiting(
    app: &str,
    name: &str,
    again: bool,
    step: Option<pitboard_core::switch::WaitStep>,
    opened_for_it: bool,
) -> String {
    use pitboard_core::switch::WaitStep;
    let (sign_in, adds) = if again {
        (
            format!("sign in to {name} again"),
            format!("Pitboard records it as {name}’s login"),
        )
    } else {
        (
            "sign in to the account you’re adding".to_owned(),
            format!("Pitboard adds it as {name}"),
        )
    };
    let first = match step {
        Some(WaitStep::QuitSignedOut) => format!(
            "{app} quit with nobody signed in. Open {app}, {sign_in}, then quit it with \
             Command-Q."
        ),
        Some(WaitStep::QuitUnfinished) => format!(
            "{app} quit before it finished signing in. Open {app}, wait until it shows your \
             chats, then quit it with Command-Q."
        ),
        Some(WaitStep::Waiting | WaitStep::Open) | None if opened_for_it => format!(
            "In {app}, {sign_in}. When it shows your chats, quit {app} with Command-Q, and \
             {adds}."
        ),
        Some(WaitStep::Waiting | WaitStep::Open) | None => format!(
            "Open {app} and {sign_in}. When it shows your chats, quit {app} with Command-Q, and \
             {adds}."
        ),
    };
    format!(
        "{first}\n\nDon’t choose Log out in {app}: it ends the login {app} holds at claude.ai, \
         and Pitboard can’t bring it back."
    )
}

pub(crate) fn sign_in_message(tool: &str, again: Option<&str>) -> String {
    match again {
        None => format!(
            "Pitboard opens {tool}’s own sign-in in your browser. Sign in as the account \
             you’re adding, and Pitboard parks its login beside the one in use."
        ),
        Some(label) => format!(
            "Pitboard opens {tool}’s own sign-in in your browser. Sign in as {label} to give \
             Pitboard a new login for it."
        ),
    }
}

pub(crate) fn sign_in(os: Os) -> String {
    titled(os, "Sign In", "Sign in")
}

pub(crate) fn add_account(os: Os) -> String {
    titled(os, "Add Account", "Add account")
}

pub(crate) fn sign_in_to_again(os: Os, label: &str) -> String {
    titled(
        os,
        &format!("Sign In to {label} Again"),
        &format!("Sign in to {label} again"),
    )
}

pub(crate) fn start_sign_in(os: Os) -> String {
    titled(os, "Start Sign-In", "Start sign-in")
}

pub(crate) fn park(label: &str) -> String {
    format!("Park {label}")
}

pub(crate) fn record_as(label: &str) -> String {
    format!("Record as {label}")
}

pub(crate) fn record_shown(app: &str, label: &str) -> String {
    format!(
        "Open {app} and check that it shows {label}, then quit it. Pitboard records the account \
         {app} is signed in to as {label}’s login."
    )
}

// `work`, parked for it, is `label` itself where its login is the one in use.
pub(crate) fn park_to_sign_in_again(app: &str, label: &str, work: &str) -> String {
    let parks = if work == label {
        format!("{label}’s login as it is")
    } else {
        work.to_owned()
    };
    format!(
        "Pitboard parks {parks}, which leaves {app} signed out and signs nothing out at \
         claude.ai. Then you sign in to {label} again in {app}, and Pitboard records it once \
         {app} quits."
    )
}

pub(crate) fn wait_to_sign_in_again(app: &str, label: &str) -> String {
    format!(
        "{app} is signed out. Pitboard waits while you sign in to {label} in {app}, and records \
         it once {app} quits."
    )
}

pub(crate) fn record_first(app: &str) -> String {
    format!(
        "Pitboard records the account {app} is signed in to under this name. Check which \
         account {app} shows first: Pitboard can’t ask claude.ai whose it is."
    )
}

pub(crate) fn add_once_signed_in(app: &str, parked: &str) -> String {
    format!(
        "{app} is signed out so you can sign in to the account you’re adding, and {parked} is \
         parked. Sign in to it in {app} and quit {app}, then add it here."
    )
}

pub(crate) fn record_unidentified(app: &str) -> String {
    format!(
        "Pitboard can’t identify the login {app} holds. Open {app}, check which account it \
         shows, and quit it. Pitboard records that account under this name: one you added, or \
         a new one."
    )
}

pub(crate) fn park_to_add(app: &str, work: &str) -> String {
    format!(
        "Pitboard parks {work}, which leaves {app} signed out and signs nothing out at \
         claude.ai. Then you sign in to the other account in {app}, and Pitboard adds it once \
         {app} quits."
    )
}

pub(crate) fn wait_to_add(app: &str) -> String {
    format!(
        "{app} is signed out. Pitboard waits while you sign in to the account you’re adding in \
         {app}, and adds it once {app} quits."
    )
}

// What the sheet says under its fields while it records the account `app` shows.
pub(crate) fn recording(app: &str, step: Recording) -> String {
    match step {
        Recording::Quitting => format!("Quitting {app}…"),
        Recording::Reading => format!("Reading {app}’s account…"),
    }
}

// What the name field shows while it is empty.
pub(crate) const NAME_PROMPT: &str = "work";

pub(crate) fn name_this_account(os: Os) -> String {
    titled(os, "Name This Account", "Name this account")
}

pub(crate) fn naming(email: &str, tool: &str) -> String {
    format!(
        "{email} is signed in to {tool}. Pitboard parks its login under this name whenever you \
         switch to another account."
    )
}

pub(crate) const SAVE: &str = "Save";

pub(crate) fn rename_title(label: &str) -> String {
    format!("Rename “{label}”")
}

pub(crate) const RENAMING: &str = "The account keeps its parked login. Only the name you switch \
                                   to it by changes, here and in the command line.";

pub(crate) const RENAME: &str = "Rename";

pub(crate) const FINDING_OUT_WHOSE: &str = "Finding out whose login it is…";

pub(crate) fn stow_title(os: Os) -> String {
    titled(
        os,
        "Put Away the Login Left in a File",
        "Put away the login left in a file",
    )
}

pub(crate) fn stow_message(os: Os) -> String {
    format!(
        "Claude Code left a login in a file behind {}, which keeps sessions already running from \
         following a switch. Pitboard keeps that login for its account where it holds none it \
         can switch to, then deletes the file.",
        pitboard_core::words::store_in_front(os)
    )
}

pub(crate) const WAITING_FOR_BROWSER: &str = "Waiting for your browser";

pub(crate) const CANCEL: &str = "Cancel";

// Said in place of Cancel where the sign-in goes on, an account parked for it.
pub(crate) const LATER: &str = "Later";

pub(crate) fn signing_in_to(os: Os, tool: &str) -> String {
    titled(
        os,
        &format!("Signing In to {tool}"),
        &format!("Signing in to {tool}"),
    )
}

pub(crate) fn finish_in_browser(name: &str, tool: &str) -> String {
    format!("Finish signing in as {name} in your browser. This closes once {tool} says you’re in.")
}

pub(crate) fn code_note(tool: &str) -> String {
    format!(
        "{tool} takes the code shown after you sign in, whether or not your browser came back \
         to it."
    )
}

// The owner approved this sentence as written on 6 October 2026. The owner decided that a
// code may be pasted again, and the Swift app, which never asked again, had nothing to say here.
pub(crate) fn code_refused(tool: &str) -> String {
    format!(
        "{tool} didn’t take that code. Copy the whole code your browser shows, and paste it \
         again."
    )
}

// A sign-in inside `app` under way, before it waits: its message, then what its spinner says.

pub(crate) fn quitting(app: &str) -> (String, String) {
    (
        format!("Pitboard is quitting {app}. {app} may ask about work in progress first."),
        format!("Quitting {app}"),
    )
}

pub(crate) fn parking(label: &str) -> (String, String) {
    (format!("Parking {label}."), format!("Parking {label}"))
}

pub(crate) const GETTING_READY: (&str, &str) = ("Getting ready.", "Starting");

pub(crate) fn waiting_to_quit(app: &str) -> String {
    format!("Waiting for {app} to quit")
}

pub(crate) fn replace_title(os: Os, tool: &str) -> String {
    titled(
        os,
        &format!("Replace {tool}’s Login?"),
        &format!("Replace {tool}’s login?"),
    )
}

pub(crate) const REPLACE: &str = "Replace";

pub(crate) fn quit_to_switch(name: &str) -> String {
    format!("Quit {name} to switch?")
}

// `again`: the account signed in to again, where it is not a new one.
pub(crate) fn quit_to_sign_in(name: &str, again: Option<&str>) -> String {
    match again {
        Some(label) => format!("Quit {name} to sign in to {label} again?"),
        None => format!("Quit {name} to add an account?"),
    }
}

// `again`: `label` was added already, and gets a new login.
pub(crate) fn quit_to_record(name: &str, label: &str, again: bool) -> String {
    if again {
        format!("Quit {name} to record {label}?")
    } else {
        format!("Quit {name} to add {label}?")
    }
}

// For an app that keeps no login Pitboard moves, which takes a switch once it starts again.
pub(crate) fn quit_and_reopen(name: &str) -> String {
    format!(
        "{name} keeps using the account it started with until it quits. Pitboard quits it, \
         switches, and opens it again."
    )
}

fn only_while_closed(name: &str, does: &str) -> String {
    format!("Pitboard {does} only while {name} is closed.")
}

pub(crate) fn quit_to_move(name: &str, label: &str) -> String {
    format!(
        "{} It quits {name}, switches to {label}, and opens it again.",
        only_while_closed(name, &format!("moves {name}’s login"))
    )
}

pub(crate) fn quit_to_park(name: &str, work: &str) -> String {
    format!(
        "{} It quits {name}, parks {work}, and opens {name} again for you to sign in.",
        only_while_closed(name, &format!("parks {work}"))
    )
}

pub(crate) fn quit_to_wait(name: &str) -> String {
    format!(
        "{} It quits {name} and opens it again for you to sign in.",
        only_while_closed(name, "starts waiting for a sign-in")
    )
}

pub(crate) fn quit_to_read(name: &str, label: &str, again: bool) -> String {
    let records = if again {
        format!("{label}’s login")
    } else {
        label.to_owned()
    };
    format!(
        "{} It quits {name}, records that account as {records}, and opens it again.",
        only_while_closed(name, &format!("reads which account {name} is signed in to"))
    )
}

// What a quit question's button says: "Quit Claude and Switch".
fn quit_and(os: Os, name: &str, then: &str, then_sentence: &str) -> String {
    titled(
        os,
        &format!("Quit {name} and {then}"),
        &format!("Quit {name} and {then_sentence}"),
    )
}

pub(crate) fn quit_and_switch(os: Os, name: &str) -> String {
    quit_and(os, name, "Switch", "switch")
}

pub(crate) fn quit_and_park(os: Os, name: &str, work: &str) -> String {
    quit_and(os, name, &format!("Park {work}"), &format!("park {work}"))
}

pub(crate) fn quit_and_start_sign_in(os: Os, name: &str) -> String {
    quit_and(os, name, "Start Sign-In", "start sign-in")
}

pub(crate) fn quit_and_record(os: Os, name: &str, again: bool) -> String {
    if again {
        quit_and(os, name, "Record", "record")
    } else {
        quit_and(os, name, "Add", "add")
    }
}

// An account the core names as typed, `desktop/work`, as the app names it: `work`.
fn bare(typed: &str) -> String {
    quoted(&Key::parse(typed).label)
}

// Where an account is named: one added by its name, any other as the app can say it.
fn stored(stored: &Stored) -> String {
    match stored {
        Stored::Account(name) => bare(name).to_owned(),
        Stored::Unenrolled(email) if email.is_empty() => "an account you haven’t added".into(),
        Stored::Unenrolled(email) => email.clone(),
        Stored::Nothing | Stored::Unknown => "nobody".into(),
    }
}

fn add_what_it_shows(app: &str) -> String {
    format!("choose Add Account…, then Add the Account {app} Shows, under that account’s name.")
}

// The core's sentence names a command or says to run this again, which the app cannot.
pub(crate) fn in_app_refusal(os: Os, error: &Error) -> Option<String> {
    let desktop = ProviderId::Desktop;
    let app = desktop.program();
    let inside = |tool: &ProviderId| in_its_app(tool.code());
    let inside_label = |label: &str| in_its_app(Key::parse(label).provider.code());
    let said = match error {
        Error::AppOpen { app, still } if still.is_empty() => format!(
            "{app} is open, and Pitboard changes its login only while it is closed. Quit {app}, \
             then try again. Nothing has changed."
        ),
        Error::AppOpen { app, still } => format!(
            "{app} has quit, and {} from it is still running, and Pitboard changes {app}’s login \
             only while all of it is closed. {} Nothing has changed.",
            listed(still),
            pitboard_core::words::wait_for_still(os, "try again")
        ),
        Error::AppStateUnknown { app, why } => format!(
            "Pitboard couldn’t tell whether {app} is open ({why}), so nothing has changed. Try \
             again."
        ),
        Error::LoginFolderBusy { app } => format!(
            "Another Pitboard is changing {app}’s login now, so nothing has changed. Try again \
             once it has finished."
        ),
        Error::MachineUnknown => "Pitboard couldn’t read this Mac’s identity, which it writes \
                                  beside every parked Claude Desktop login so it never puts \
                                  back one parked on another Mac. Nothing has changed."
            .into(),
        Error::LiveCredentialAbsent { tool, .. } if inside(tool) => format!(
            "{} isn’t signed in. Choose Start Sign-In, then sign in to the account in {}.",
            tool.program(),
            tool.program()
        ),
        Error::SignInUnfinished { tool } if inside(tool) => {
            let app = tool.program();
            format!(
                "{app} hasn’t finished signing in. Open {app}, wait until it shows your chats, \
                 quit it, then try again. Nothing has changed."
            )
        }
        Error::LiveAccountNotEnrolled { tool, .. } if inside(tool) => format!(
            "{} is signed in to an account you haven’t added, so it can’t be parked. Choose Add \
             Account…, then Add the Account {} Shows, to add it first.",
            tool.program(),
            tool.program()
        ),
        Error::AlreadyEnrolled {
            tool,
            label,
            waited: true,
            ..
        } if inside(tool) => format!(
            "{} is signed in to {label}, which you’ve added. Choose Record as {label} on its \
             row’s Sign In Again… to record that login, then add another.",
            tool.program(),
            label = bare(label)
        ),
        Error::AlreadyEnrolled { tool, label, .. } if inside(tool) => format!(
            "{} is signed in to {label}, which you’ve added already. Choose Use {label}, or Add \
             Account… to add another.",
            tool.program(),
            label = bare(label)
        ),
        Error::LoginInDoubt { tool, why } if inside(tool) => match why {
            LoginDoubt::NamedAnother { recorded, named } => format!(
                "Pitboard recorded this {app} login as {}’s, and {app} names {} for it. Open \
                 {app}, check which account it shows, quit it, then {} Nothing has changed.",
                bare(recorded),
                named
                    .as_deref()
                    .map_or_else(|| "an account you haven’t added".into(), bare),
                add_what_it_shows(tool.program()),
                app = tool.program()
            ),
            LoginDoubt::SignedInOutside { label } => format!(
                "{app} was signed in to {} outside Pitboard, so Pitboard can’t tell this login \
                 from the one it parked. Open {app}, check which account it shows, quit it, then \
                 {} Nothing has changed.",
                bare(label),
                add_what_it_shows(tool.program()),
                app = tool.program()
            ),
        },
        Error::LoginFolderUnknown { tool, what, then } if inside(tool) => {
            let app = tool.program();
            let then = match then {
                Then::Nothing => String::new(),
                Then::OpenAndQuit => format!("Open {app}, quit it, then try again. "),
                Then::UpdatePitboard => "Update Pitboard, or report it at \
                                         https://github.com/datlechin/pitboard/issues if no \
                                         release reads it yet. "
                    .into(),
                Then::CopyAside => format!(
                    "{app} resets a settings file it can’t read when it opens, so copy it aside, \
                     open {app}, quit it, then try again. "
                ),
                Then::SignInAndEnrol => format!(
                    "Open {app}. If it asks you to sign in, sign in. Quit it, then choose Add \
                     Account…, then Add the Account {app} Shows. "
                ),
            };
            format!(
                "{app}’s data folder {what}, so Pitboard moves no {} login. {then}Nothing has \
                 changed.",
                tool.name()
            )
        }
        Error::LoginFolderUnreadable { tool, detail } if inside(tool) => {
            let app = tool.program();
            format!(
                "Pitboard couldn’t read {app}’s cookie store ({detail}). Open {app}, quit it, then \
                 try again. Nothing has changed."
            )
        }
        Error::ParksNotPrivate { path, why } => {
            let path = path.display();
            let said = match why {
                Private::Shared => format!(
                    "{path} can be reached by others. Run chmod 700 on it in Terminal, then try \
                     again."
                ),
                Private::Link => format!(
                    "{path} is a link, so Pitboard parks no Claude Desktop login there. Point \
                     PITBOARD_HOME at a folder of your own on this volume, then try again."
                ),
                Private::NotOwn => format!(
                    "{path} isn’t yours, so Pitboard parks no Claude Desktop login there. Point \
                     PITBOARD_HOME at a folder of your own on this volume, then try again."
                ),
                Private::NotADirectory | Private::Missing | Private::Yes => format!(
                    "{path} isn’t a folder, so Pitboard parks no Claude Desktop login there. \
                     Point PITBOARD_HOME at a folder of your own on this volume, then try again."
                ),
            };
            format!("{said} Nothing has changed.")
        }
        Error::DifferentVolume { parks, root } => format!(
            "Pitboard parks a Claude Desktop login by renaming it into {}, which is on another \
             volume than {}, and it never copies one. Keep Pitboard’s folder on the volume your \
             home is on. Nothing has changed.",
            parks.display(),
            root.display()
        ),
        Error::MoveUndone { label, detail } => format!(
            "Pitboard couldn’t park {label}’s login ({detail}) and undid what it had done. \
             {label} is in use, and nothing was lost. Try again.",
            label = bare(label)
        ),
        Error::RecoveryUndetermined {
            tool, to, detail, ..
        } if inside(tool) => format!(
            "An earlier move of {}’s login was interrupted, and Pitboard can’t tell where it \
             stopped ({detail}). Nothing has changed. Open {app} to see whether it’s signed in \
             and to which account, and quit it. If it’s signed in, choose Add Account…, then Add \
             the Account {app} Shows. If it’s signed out, choose Give Up… on the interrupted \
             move’s notice.",
            bare(to),
            app = tool.program()
        ),
        Error::RecoveryElsewhere { tool, to, slot, .. } if inside(tool) => format!(
            "An earlier move of {}’s login was interrupted while {} kept its data in {slot}. \
             Nothing has changed. Choose Give Up… on the interrupted move’s notice to keep every \
             folder and move on.",
            bare(to),
            tool.program()
        ),
        Error::AppOpenedMidway {
            app,
            label: Some(label),
        } => format!(
            "{app} was opened while Pitboard moved {}’s login, so Pitboard stopped. Nothing is \
             lost. Quit {app}, then try again: Pitboard settles the move first.",
            bare(label)
        ),
        Error::AppOpenedMidway { app, label: None } => format!(
            "{app} was opened while Pitboard set aside the account name {app} kept, so Pitboard \
             stopped. Nothing is lost. Quit {app}, then try again."
        ),
        Error::MoveStopped { label, detail } => format!(
            "Pitboard stopped while moving {}’s login ({detail}). Keep {app} closed and try \
             again: Pitboard settles the move first.",
            bare(label)
        ),
        Error::RecoveryStopped { label, detail } => format!(
            "Pitboard stopped while settling an interrupted move of {}’s login ({detail}). \
             Nothing is lost. Keep {app} closed and try again: Pitboard settles the move first. \
             If it stops again, choose Give Up… on the interrupted move’s notice.",
            bare(label)
        ),
        Error::SwitchLeftSignedOut {
            from: Some(from),
            to,
            detail,
        } => format!(
            "{from} is parked, and {to} couldn’t be put in use ({detail}). {app} is signed out \
             and both logins are parked. Choose Use {to} to try again, or Use {from} to go back.",
            from = bare(from),
            to = bare(to)
        ),
        Error::SwitchLeftSignedOut {
            from: None,
            to,
            detail,
        } => format!(
            "{to} couldn’t be put in use ({detail}). {app} is signed out and {to} is still \
             parked. Choose Use {to} to try again.",
            to = bare(to)
        ),
        Error::MoveMixed { label, places } => format!(
            "Giving up would leave parts of {} beside another login in {app}’s folder: {}. \
             Nothing has changed. Open {app}, check which account it shows, quit it, then {}",
            bare(label),
            places.join(", "),
            add_what_it_shows(app)
        ),
        Error::SignInSettledElsewhere { .. } => {
            "This sign-in was finished, put back or removed elsewhere, so nothing was added.".into()
        }
        Error::NothingParked { tool, label } if inside(tool) => format!(
            "{} has no parked login to switch to. Choose Sign In Again… on its row.",
            bare(label)
        ),
        Error::AccountUnknown { label, .. } if inside_label(label) => format!(
            "No {} account is added as {}. Choose Add Account… to add it.",
            Key::parse(label).provider.name(),
            bare(label)
        ),
        Error::ParkedCredentialMissing { label } if inside_label(label) => format!(
            "The parked login for {0} is missing. Choose Sign In Again… on {0}’s row.",
            bare(label)
        ),
        Error::LiveLoginRefused {
            tool,
            label,
            detail,
        } => pitboard_core::words::replacing(*tool, &bare(label), detail),
        Error::IdentityUnverifiable {
            tool,
            login: Unidentified::Parked { label },
            cause,
            detail,
            ..
        } if !cause.worth_retrying() && *cause != Cause::AnswerNotUnderstood => format!(
            "Pitboard couldn’t confirm with {} whose login is parked for {1} ({detail}), so \
             nothing has changed. Choose Sign In Again… on {1}’s row.",
            tool.service(),
            bare(label)
        ),
        Error::ParkedCredentialCorrupt { label, detail } if inside_label(label) => format!(
            "The parked login for {0} is not the one Pitboard recorded ({detail}). Choose Sign In \
             Again… on {0}’s row.",
            bare(label)
        ),
        _ => return None,
    };
    Some(typeset(&said))
}

// Its notice offers to sign in again, so the words name no command.
fn replaced(tool: ProviderId, label: &str, now: &Stored) -> String {
    let label = bare(label);
    let name = tool.name();
    let has = match now {
        Stored::Account(now) => format!(": {name} now has {}’s login stored", bare(now)),
        Stored::Unenrolled(email) if email.is_empty() => {
            format!(": {name} now has the login of an account you haven’t added")
        }
        Stored::Unenrolled(email) => format!(": {name} now has {email}’s login stored"),
        Stored::Nothing => format!(": {name} now has no login stored"),
        Stored::Unknown => String::new(),
    };
    format!(
        "{label}’s login was replaced by a sign-in outside Pitboard{has}, and Pitboard holds no \
         login for {label}."
    )
}

pub(crate) fn in_app_warning(os: Os, warning: &CoreWarning) -> Option<String> {
    if let CoreWarning::LoginReplaced {
        tool, label, now, ..
    } = warning
        && !in_its_app(tool.code())
    {
        return Some(typeset(&replaced(*tool, label, now)));
    }
    let tool = warning.tool().filter(|tool| in_its_app(tool.code()))?;
    let app = tool.program();
    let add_what_it_shows = format!(
        "Open {app}, check which account it shows, quit it, then choose Add the Account {app} \
         Shows."
    );
    let named = |named: &Stored, last: &Stored| {
        format!(
            "{}. {add_what_it_shows}",
            pitboard_core::words::desktop_named(named, last, stored)
        )
    };
    let said = match warning {
        CoreWarning::InUseUnconfirmed {
            named: by, last, ..
        } => named(by, last),
        CoreWarning::ConfigNamesAnother { config, in_use, .. } => named(config, in_use),
        CoreWarning::LoginReplaced {
            label,
            now: Stored::Nothing,
            ..
        } => format!(
            "{} was signed out in {app}, which ends its login at claude.ai, and Pitboard holds \
             no login for it.",
            bare(label)
        ),
        CoreWarning::LoginReplaced { label, now, .. } => replaced(tool, label, now),
        CoreWarning::TakenAsNamed {
            label,
            recorded: None,
            ..
        } => format!(
            "Pitboard hadn’t seen the login {app} held, and took it as {label}’s because {app} \
             names that account. If {app} showed another account, choose Use {label}, check in \
             {app}, quit it, then choose Add Account… and Add the Account {app} Shows.",
            label = bare(label)
        ),
        CoreWarning::TakenAsNamed {
            label,
            recorded: Some(recorded),
            ..
        } => format!(
            "Pitboard had recorded the login {app} held as {}’s, and filed it as {}’s because \
             {app} names that account. Open {app} and check which account it shows. If it’s \
             another, quit {app}, then choose Add Account… and Add the Account {app} Shows.",
            bare(recorded),
            bare(label)
        ),
        CoreWarning::LooksSignedOut { label, .. } => format!(
            "{app} looks signed out since Pitboard last read {} in use. Pitboard checks at the \
             next change to a {} account.",
            bare(label),
            tool.name()
        ),
        CoreWarning::SetAside {
            path, label, why, ..
        } => {
            let path = path.display();
            let label = label.as_deref().map(bare);
            match (why, label) {
                (SetAside::Superseded, Some(label)) => format!(
                    "Pitboard kept {label}’s earlier parked login in {path} and deleted nothing. \
                     This Mac lists it."
                ),
                (SetAside::Superseded, None) => format!(
                    "Pitboard kept an earlier parked login in {path} and deleted nothing. This \
                     Mac lists it."
                ),
                (SetAside::GivenUp, Some(label)) => format!(
                    "Pitboard moved the parts of {label}’s login left in {app}’s folder into \
                     {path} and deleted nothing. This Mac lists them."
                ),
                (SetAside::GivenUp, None) => format!(
                    "Pitboard moved the parts of a login left in {app}’s folder into {path} and \
                     deleted nothing. This Mac lists them."
                ),
                (SetAside::InTheWay | SetAside::AnotherMachine | SetAside::Orphan, _) => {
                    format!(
                        "Pitboard moved what it found in the way into {path} and deleted nothing."
                    )
                }
            }
        }
        CoreWarning::KeysSetAside { path, label, .. } => format!(
            "{app} rewrote {}’s keys. Pitboard kept the earlier ones in {} and deleted nothing. \
             This Mac lists them.",
            label.as_deref().map_or_else(|| "a login".into(), bare),
            path.display()
        ),
        CoreWarning::MoveWaiting { label, .. } => format!(
            "A move of {}’s login was interrupted. Quit {app}, then use any {} account: Pitboard \
             settles the move first.",
            bare(label),
            tool.name()
        ),
        CoreWarning::SignInAwaited {
            label,
            put_back,
            again,
            ..
        } => awaiting_lines(app, &bare(label), &bare(put_back), *again).join(" "),
        CoreWarning::MoveNotGivenUp { label, why, .. } => not_given_up(os, tool, &bare(label), why),
        _ => return None,
    };
    Some(typeset(&said))
}

// After Give Up gave up a switch, so never that nothing has changed.
fn not_given_up(os: Os, tool: ProviderId, label: &str, why: &Error) -> String {
    let app = tool.program();
    let settles = format!(
        "use any {} account: Pitboard settles the move first",
        tool.name()
    );
    let because = pitboard_core::words::not_given_up_because(why, label);
    let then = match why {
        Error::AppOpen { still, .. } if still.is_empty() => format!("Quit {app}, then {settles}."),
        Error::AppOpen { .. } => pitboard_core::words::wait_for_still(os, &settles),
        Error::LoginFolderBusy { .. } => format!("Once it has finished, {settles}."),
        Error::LoginFolderUnreadable { .. } => format!("Open {app}, quit it, then {settles}."),
        Error::AppOpenedMidway { .. } => format!("Nothing is lost. Quit {app}, then {settles}."),
        Error::MoveMixed { .. } => format!(
            "Open {app}, check which account it shows, quit it, then {}",
            add_what_it_shows(app)
        ),
        Error::Store(_) if because.is_some() => pitboard_core::words::readable_first(&settles),
        _ if because.is_some() => format!("To try again, {settles}."),
        _ => format!("Quit {app}, then {settles}."),
    };
    match because {
        Some(because) => format!(
            "Pitboard didn’t give up the interrupted move of {label}’s login, because {because}. \
             {then}"
        ),
        None => {
            format!("Pitboard couldn’t give up the interrupted move of {label}’s login. {then}")
        }
    }
}

pub(crate) fn in_app_given_up(given_up: &pitboard_core::switch::GivenUp) -> String {
    let app = given_up.tool.program();
    typeset(&pitboard_core::words::given_up(
        given_up,
        &bare(&given_up.label),
        &format!(
            "choose Add Account… and Add the Account {app} Shows for the account {app} shows, or \
             Use on an account where {app} is signed out."
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parked_login_a_sign_in_keeps_says_so_after_its_life() {
        assert_eq!(
            sign_in_hint("Parked login good for 2 more days", false),
            "Parked login good for 2 more days: sign in again to keep it"
        );
        assert_eq!(
            sign_in_hint("Its sign-in may have lapsed", true),
            "Its sign-in may have lapsed: sign in again"
        );
    }

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

    /// A phrase that starts a line starts with a capital, and nothing else about it changes:
    /// a figure first stays as it is, and so does a phrase already capitalised.
    #[test]
    fn only_the_first_letter_of_a_phrase_is_capitalised() {
        assert_eq!(capitalised(""), "");
        assert_eq!(capitalised("w"), "W");
        assert_eq!(capitalised("weekly used up"), "Weekly used up");
        assert_eq!(
            capitalised("Weekly 64%, weekly Fable 98%"),
            "Weekly 64%, weekly Fable 98%"
        );
        assert_eq!(capitalised("5-hour 12%"), "5-hour 12%");
        assert_eq!(capitalised("éclair"), "Éclair");
        assert_eq!(capitalised("e\u{301}clair"), "E\u{301}clair");
    }

    #[test]
    fn the_cores_words_read_as_a_sentence_in_the_apps() {
        assert_eq!(
            sentence("the scheduler refused: access denied"),
            "The scheduler refused: access denied."
        );
        assert_eq!(
            sentence("the sign-in did not finish, so nothing was enrolled."),
            "The sign-in didn’t finish, so nothing was enrolled."
        );
        assert_eq!(
            sentence("/Users/dana/Dropbox looks like it is inside Dropbox"),
            "/Users/dana/Dropbox looks like it is inside Dropbox."
        );
        assert_eq!(
            sentence("`work` is signed in; switch to another account before forgetting it."),
            "work is signed in; switch to another account before forgetting it."
        );
        assert_eq!(
            sentence("`codex/main` already refers to main@example.com in another workspace"),
            "codex/main already refers to main@example.com in another workspace."
        );
        assert_eq!(
            sentence("2 parked login(s) could not be removed yet"),
            "2 parked login(s) couldn’t be removed yet."
        );
        assert_eq!(
            sentence("could not read Pitboard's account list at `/Users/o'neill/.pitboard`"),
            "Couldn’t read Pitboard’s account list at /Users/o'neill/.pitboard."
        );
        assert_eq!(sentence("Is it there?"), "Is it there?");
        assert_eq!(sentence(""), "");
    }

    // Apple's and Microsoft's style both curl an apostrophe and contract a "not" in an app's
    // text; the command line keeps the core's own.
    #[test]
    fn the_cores_words_take_the_apps_typography() {
        assert_eq!(
            typeset("Claude Code's login is no longer accepted; run `claude` and sign in again"),
            "Claude Code’s login is no longer accepted; run claude and sign in again"
        );
        assert_eq!(
            typeset("`desktop/work`'s parked login, the accounts' logins"),
            "desktop/work’s parked login, the accounts’ logins"
        );
        assert_eq!(
            typeset(
                "Cannot read it: it is not there, does not answer, did not finish, has not \
                 started, was not read, had not been, and could not be. Do not run it."
            ),
            "Can’t read it: it isn’t there, doesn’t answer, didn’t finish, hasn’t started, \
             wasn’t read, hadn’t been, and couldn’t be. Don’t run it."
        );
        assert_eq!(
            typeset("o'brien@example.com's login, this nothing, `it is not` here"),
            "o'brien@example.com’s login, this nothing, it is not here"
        );
        assert_eq!(
            typeset("whether or not it will not, would not or must not"),
            "whether or not it will not, would not or must not"
        );
        assert_eq!(typeset(""), "");
    }

    // A path, an address and a system's own message are quoted by the core, not its words.
    #[test]
    fn what_the_core_quotes_keeps_every_letter() {
        assert_eq!(
            sentence(
                "could not write to Pitboard's directory at C:\\Users\\Dan O'Brien\\.pitboard: \
                 The system cannot find the path specified. (os error 3)"
            ),
            "Couldn’t write to Pitboard’s directory at C:\\Users\\Dan O'Brien\\.pitboard: The \
             system cannot find the path specified. (os error 3)."
        );
        assert_eq!(
            typeset("could not read /Users/dan/dan's files/state.json: Permission denied"),
            "couldn’t read /Users/dan/dan's files/state.json: Permission denied"
        );
        assert_eq!(
            typeset(
                "the scheduler refused: Task Scheduler said The system cannot find the file \
                 specified. (0x80070002)"
            ),
            "the scheduler refused: Task Scheduler said The system cannot find the file \
             specified. (0x80070002)"
        );
        assert_eq!(
            typeset("cannot.reply@example.com's login cannot be read"),
            "cannot.reply@example.com’s login can’t be read"
        );
    }

    // The contractions the apps give the core's words.
    #[test]
    fn the_apps_own_words_on_switching_by_itself_take_their_contractions() {
        assert_eq!(NOT_SWITCHED, "Claude Code wasn’t switched");
        assert_eq!(NOT_SWITCHING, "Pitboard isn’t switching Claude Code");
        assert_eq!(
            COULD_NOT_SWITCH_ITSELF,
            "Pitboard couldn’t switch Claude Code"
        );
        assert_eq!(
            COULD_NOT_WARM_ITSELF,
            "Pitboard couldn’t warm up an account"
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

    // As its row, the menu and the rename field write it.
    #[test]
    fn a_label_keeps_every_letter_in_a_sentence() {
        let said = in_app_warning(
            Os::MacOs,
            &CoreWarning::LooksSignedOut {
                tool: ProviderId::Desktop,
                label: "desktop/dana's".into(),
            },
        )
        .expect("said");
        assert!(said.contains(" read dana's in use."), "{said}");
        let said = used_then("cannot", "96% of its 5-hour limit", "");
        assert!(said.starts_with("cannot has used "), "{said}");
    }

    /// VoiceOver reads the column's "30m" as thirty meters and "5h" as letters, so a limit
    /// is said in words: its name as a sentence says it, what it has used, and when it
    /// resets.
    #[test]
    fn a_limit_is_spoken_in_words_and_not_in_its_columns_shorthand() {
        assert_eq!(
            spoken_limit("5-hour", 42.0, Some(3 * 3600), None),
            "5-hour limit, 42 percent used, resets in 3 hours"
        );
        assert_eq!(
            spoken_limit("30-minute", 12.0, None, None),
            "30-minute limit, 12 percent used"
        );
        assert_eq!(
            spoken_limit("weekly Fable", 98.0, Some(0), None),
            "weekly Fable limit, 98 percent used, resetting now",
            "a reset whose time has come is said, as the column says it"
        );
    }

    /// A limit with a pace says it after what it has used, as how far it is from an even
    /// pace, so VoiceOver hears what the mark on its bar shows.
    #[test]
    fn a_limit_with_a_pace_is_spoken_with_it() {
        use pitboard_core::usage::pace::{Pace, Standing};
        let pace = |delta: f64, standing| Pace {
            expected: 6.0,
            delta,
            standing,
        };
        assert_eq!(
            spoken_limit(
                "weekly",
                33.0,
                Some(6 * 86_400),
                Some(&pace(27.0, Standing::Over { runs_out_in: 9_000 }))
            ),
            "weekly limit, 33 percent used, 27 percent over an even pace, resets in 6 days"
        );
        assert_eq!(
            spoken_limit("5-hour", 30.0, None, Some(&pace(-50.0, Standing::Under))),
            "5-hour limit, 30 percent used, 50 percent under an even pace"
        );
        assert_eq!(
            spoken_limit("5-hour", 52.0, None, Some(&pace(2.0, Standing::Even))),
            "5-hour limit, 52 percent used, on an even pace"
        );
    }

    /// What a bar's help says of its mark: what an even pace would have used by now, and
    /// for a limit over pace, when it runs out.
    #[test]
    fn a_bars_help_says_what_its_mark_means() {
        use pitboard_core::usage::pace::{Pace, Standing};
        assert_eq!(
            pace_help(&Pace {
                expected: 5.95,
                delta: 27.05,
                standing: Standing::Over {
                    runs_out_in: 73_090
                },
            }),
            "27% over pace: an even pace would have used 6% of it by now. At this pace it \
             runs out in 20h 18m."
        );
        assert_eq!(
            pace_help(&Pace {
                expected: 80.0,
                delta: -50.0,
                standing: Standing::Under,
            }),
            "50% under pace: an even pace would have used 80% of it by now."
        );
        assert_eq!(
            pace_help(&Pace {
                expected: 5.95,
                delta: 94.0,
                standing: Standing::Over { runs_out_in: 36 },
            }),
            "94% over pace: an even pace would have used 6% of it by now. At this pace it is \
             about to run out."
        );
    }

    /// Pitboard says everything in English, so a span of time VoiceOver reads inside one of
    /// its sentences is English too, whatever the region: the Swift app wrote it in
    /// `en_US_POSIX` for that reason. Here it has no region to follow.
    #[test]
    fn a_spoken_span_of_time_reads_the_same_in_every_region() {
        assert_eq!(
            spoken_limit("5-hour", 42.0, Some(90 * 60), None),
            "5-hour limit, 42 percent used, resets in 1 hour, 30 minutes"
        );
    }

    /// Every span Foundation was asked to say on macOS 27.0 with Swift 6.4, as the Swift app
    /// said it, and what it said: rounding to the minute a half to the even one, the two
    /// largest units that are not nought, and the last rounded up into the one before it.
    #[test]
    fn a_span_is_spoken_as_foundation_spoke_it() {
        for (seconds, said) in [
            (0, "1 minute"),
            (1, "1 minute"),
            (59, "1 minute"),
            (60, "1 minute"),
            (61, "1 minute"),
            (89, "1 minute"),
            (90, "2 minutes"),
            (91, "2 minutes"),
            (119, "2 minutes"),
            (120, "2 minutes"),
            (150, "2 minutes"),
            (179, "3 minutes"),
            (180, "3 minutes"),
            (599, "10 minutes"),
            (3540, "59 minutes"),
            (3569, "59 minutes"),
            (3570, "1 hour"),
            (3599, "1 hour"),
            (3600, "1 hour"),
            (3601, "1 hour"),
            (3629, "1 hour"),
            (3630, "1 hour"),
            (3659, "1 hour, 1 minute"),
            (3660, "1 hour, 1 minute"),
            (3690, "1 hour, 2 minutes"),
            (5400, "1 hour, 30 minutes"),
            (5429, "1 hour, 30 minutes"),
            (5430, "1 hour, 30 minutes"),
            (5459, "1 hour, 31 minutes"),
            (5460, "1 hour, 31 minutes"),
            (7169, "1 hour, 59 minutes"),
            (7170, "2 hours"),
            (7171, "2 hours"),
            (7199, "2 hours"),
            (7200, "2 hours"),
            (9030, "2 hours, 30 minutes"),
            (10_799, "3 hours"),
            (10_800, "3 hours"),
            (18_000, "5 hours"),
            (35_999, "10 hours"),
            (84_599, "23 hours, 30 minutes"),
            (84_600, "23 hours, 30 minutes"),
            (86_340, "23 hours, 59 minutes"),
            (86_369, "23 hours, 59 minutes"),
            (86_370, "1 day"),
            (86_371, "1 day"),
            (86_399, "1 day"),
            (86_400, "1 day"),
            (86_401, "1 day"),
            (86_429, "1 day"),
            (86_430, "1 day"),
            (86_431, "1 day, 1 minute"),
            (86_460, "1 day, 1 minute"),
            (86_490, "1 day, 2 minutes"),
            (86_550, "1 day, 2 minutes"),
            (88_199, "1 day, 30 minutes"),
            (88_200, "1 day, 30 minutes"),
            (88_201, "1 day, 30 minutes"),
            (88_230, "1 day, 30 minutes"),
            (89_969, "1 day, 59 minutes"),
            (89_970, "1 day, 1 hour"),
            (89_971, "1 day, 1 hour"),
            (89_999, "1 day, 1 hour"),
            (90_000, "1 day, 1 hour"),
            (91_799, "1 day, 1 hour"),
            (91_800, "1 day, 2 hours"),
            (117_000, "1 day, 8 hours"),
            (129_600, "1 day, 12 hours"),
            (131_400, "1 day, 12 hours"),
            (135_000, "1 day, 14 hours"),
            (170_940, "1 day, 23 hours"),
            (171_000, "2 days, 0 hours"),
            (171_060, "2 days, 0 hours"),
            (171_900, "2 days, 0 hours"),
            (172_740, "2 days, 0 hours"),
            (172_769, "2 days, 0 hours"),
            (172_770, "2 days"),
            (172_771, "2 days"),
            (172_799, "2 days"),
            (172_800, "2 days"),
            (172_830, "2 days"),
            (174_600, "2 days, 30 minutes"),
            (175_500, "2 days, 45 minutes"),
            (214_200, "2 days, 12 hours"),
            (215_999, "2 days, 12 hours"),
            (259_199, "3 days"),
            (302_400, "3 days, 12 hours"),
            (345_599, "4 days"),
            (345_600, "4 days"),
            (602_940, "6 days, 23 hours"),
            (603_000, "7 days, 0 hours"),
            (603_060, "7 days, 0 hours"),
            (604_799, "7 days"),
            (604_800, "7 days"),
            (605_000, "7 days, 3 minutes"),
            (1_209_600, "14 days"),
            (31_536_000, "365 days"),
            (1_000_000_000, "11574 days, 2 hours"),
            (-5, "1 minute"),
            (30, "1 minute"),
        ] {
            assert_eq!(spoken_span(seconds), said, "{seconds}");
        }
    }

    /// The bar is shared with everything else running, so a name longer than twelve
    /// characters is cut to eleven and an ellipsis, and a name that fits is left whole.
    /// Characters as a person counts them, as Swift counts a `String`: each label here, its
    /// count and its cut are what `String.count` and `prefix(11)` gave on macOS 27.0 with
    /// Swift 6.4, written as their scalars.
    #[test]
    fn a_label_is_cut_where_swift_counts_its_characters() {
        let text = |scalars: &[u32]| -> String {
            scalars
                .iter()
                .map(|&scalar| char::from_u32(scalar).expect("a scalar"))
                .collect()
        };
        let cut_after = |label: &[u32], kept: usize| {
            let mut kept = text(&label[..kept]);
            kept.push('…');
            kept
        };
        let cases: [(&[u32], usize, Option<usize>); 22] = [
            (
                &[
                    0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C,
                ],
                12,
                None,
            ),
            (
                &[
                    0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D,
                ],
                13,
                Some(11),
            ),
            // café-de-la-gare, the accent combining.
            (
                &[
                    0x63, 0x61, 0x66, 0x65, 0x301, 0x2D, 0x64, 0x65, 0x2D, 0x6C, 0x61, 0x2D, 0x67,
                    0x61, 0x72, 0x65,
                ],
                15,
                Some(12),
            ),
            // Eleven flags, each two regional indicators.
            (
                &[
                    0x1F1EF, 0x1F1F5, 0x1F1EB, 0x1F1F7, 0x1F1E9, 0x1F1EA, 0x1F1EC, 0x1F1E7,
                    0x1F1EE, 0x1F1F9, 0x1F1EA, 0x1F1F8, 0x1F1FA, 0x1F1F8, 0x1F1E8, 0x1F1E6,
                    0x1F1E7, 0x1F1F7, 0x1F1F2, 0x1F1FD, 0x1F1E6, 0x1F1FA,
                ],
                11,
                None,
            ),
            // A family joined by zero-width joiners.
            (
                &[
                    0x74, 0x65, 0x61, 0x6D, 0x2D, 0x1F468, 0x200D, 0x1F469, 0x200D, 0x1F467,
                    0x200D, 0x1F466, 0x2D, 0x77, 0x6F, 0x72, 0x6B,
                ],
                11,
                None,
            ),
            // Seven thumbs with a skin tone.
            (
                &[
                    0x1F44D, 0x1F3FD, 0x1F44D, 0x1F3FD, 0x1F44D, 0x1F3FD, 0x1F44D, 0x1F3FD,
                    0x1F44D, 0x1F3FD, 0x1F44D, 0x1F3FD, 0x1F44D, 0x1F3FD,
                ],
                7,
                None,
            ),
            // Seven keycaps.
            (
                &[
                    0x31, 0xFE0F, 0x20E3, 0x32, 0xFE0F, 0x20E3, 0x33, 0xFE0F, 0x20E3, 0x34, 0xFE0F,
                    0x20E3, 0x35, 0xFE0F, 0x20E3, 0x36, 0xFE0F, 0x20E3, 0x37, 0xFE0F, 0x20E3,
                ],
                7,
                None,
            ),
            // Devanagari, whose conjuncts are one character each.
            (
                &[
                    0x915, 0x94D, 0x937, 0x93F, 0x924, 0x93F, 0x91C, 0x2D, 0x915, 0x93E, 0x92E,
                    0x2D, 0x916, 0x93E, 0x924, 0x93E,
                ],
                9,
                None,
            ),
            // Hangul written in jamo, three to a syllable.
            (
                &[
                    0x1100, 0x1161, 0x11A8, 0x1100, 0x1161, 0x11A8, 0x1100, 0x1161, 0x11A8, 0x1100,
                    0x1161, 0x11A8, 0x1100, 0x1161, 0x11A8, 0x2D, 0x61, 0x6E, 0x64, 0x2D, 0x6D,
                    0x6F, 0x72, 0x65,
                ],
                14,
                Some(21),
            ),
            (
                &[
                    0xD55C, 0xAD6D, 0xC5B4, 0x2D, 0xACC4, 0xC815, 0x2D, 0xBCF4, 0xC870, 0x2D,
                    0xC77C, 0xBC18,
                ],
                12,
                None,
            ),
            (
                &[
                    0x65E5, 0x672C, 0x8A9E, 0x306E, 0x30A2, 0x30AB, 0x30A6, 0x30F3, 0x30C8, 0x306E,
                    0x540D, 0x524D, 0x3067, 0x3059,
                ],
                14,
                Some(11),
            ),
            // Thirteen letters, each with two combining marks.
            (
                &[
                    0x61, 0x308, 0x301, 0x62, 0x308, 0x301, 0x63, 0x308, 0x301, 0x64, 0x308, 0x301,
                    0x65, 0x308, 0x301, 0x66, 0x308, 0x301, 0x67, 0x308, 0x301, 0x68, 0x308, 0x301,
                    0x69, 0x308, 0x301, 0x6A, 0x308, 0x301, 0x6B, 0x308, 0x301, 0x6C, 0x308, 0x301,
                    0x6D,
                ],
                13,
                Some(33),
            ),
            // A rainbow flag, joined.
            (
                &[
                    0x1F3F3, 0xFE0F, 0x200D, 0x1F308, 0x2D, 0x70, 0x72, 0x69, 0x64, 0x65, 0x2D,
                    0x74, 0x65, 0x61, 0x6D, 0x2D, 0x78,
                ],
                14,
                Some(14),
            ),
            // Scotland's flag, in tag characters.
            (
                &[
                    0x1F3F4, 0xE0067, 0xE0062, 0xE0073, 0xE0063, 0xE0074, 0xE007F, 0x2D, 0x73,
                    0x63, 0x6F, 0x74, 0x6C, 0x61, 0x6E, 0x64, 0x2D, 0x74, 0x65, 0x61, 0x6D,
                ],
                15,
                Some(17),
            ),
            // A carriage return and a line feed, one character.
            (
                &[
                    0x6C, 0x69, 0x6E, 0x65, 0xD, 0xA, 0x62, 0x72, 0x65, 0x61, 0x6B, 0x2D, 0x61,
                    0x6E, 0x64, 0x2D, 0x6D, 0x6F, 0x72, 0x65,
                ],
                19,
                Some(12),
            ),
            // Thai, whose SARA AM joins the letter before it.
            (
                &[
                    0xE01, 0xE33, 0xE01, 0xE33, 0xE01, 0xE33, 0xE01, 0xE33, 0xE01, 0xE33, 0xE01,
                    0xE33, 0xE01,
                ],
                7,
                None,
            ),
            // Arabic's number sign, which joins what follows it.
            (
                &[
                    0x600, 0x31, 0x600, 0x32, 0x600, 0x33, 0x600, 0x34, 0x600, 0x35, 0x600, 0x36,
                    0x600, 0x37,
                ],
                7,
                None,
            ),
            (
                &[
                    0x646, 0x627, 0x645, 0x2D, 0x62D, 0x633, 0x627, 0x628, 0x2D, 0x637, 0x648,
                    0x64A, 0x644,
                ],
                13,
                Some(11),
            ),
            // A joiner after a letter that is not a pictograph joins nothing after it.
            (
                &[
                    0x78, 0x200D, 0x79, 0x200D, 0x7A, 0x200D, 0x77, 0x200D, 0x76, 0x200D, 0x75,
                    0x200D, 0x74, 0x2D, 0x6D, 0x6F, 0x72, 0x65,
                ],
                12,
                None,
            ),
            (
                &[
                    0x1F9D1, 0x200D, 0x1F4BB, 0x1F9D1, 0x200D, 0x1F4BB, 0x1F9D1, 0x200D, 0x1F4BB,
                    0x1F9D1, 0x200D, 0x1F4BB, 0x1F9D1, 0x200D, 0x1F4BB, 0x1F9D1, 0x200D, 0x1F4BB,
                    0x1F9D1, 0x200D, 0x1F4BB,
                ],
                7,
                None,
            ),
            (
                &[
                    0x915, 0x94D, 0x200D, 0x937, 0x915, 0x94D, 0x937, 0x915, 0x94D, 0x937, 0x915,
                    0x94D, 0x937, 0x915, 0x94D, 0x937,
                ],
                5,
                None,
            ),
            (
                &[
                    0x63, 0x61, 0x66, 0xE9, 0x2D, 0x64, 0x65, 0x2D, 0x6C, 0x61, 0x2D, 0x67, 0x61,
                    0x72, 0x65, 0x2D, 0x78,
                ],
                17,
                Some(11),
            ),
        ];
        for (scalars, count, kept) in cases {
            let label = text(scalars);
            assert_eq!(characters(&label), count, "{label:?}");
            let cut = kept.map_or_else(|| label.clone(), |kept| cut_after(scalars, kept));
            assert_eq!(bar_name(&label), cut, "{label:?}");
        }
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
                "Open sessions still use the old login",
            ),
            ("sessions_unknown", "Couldn’t tell which sessions are open"),
            (
                "auth_overridden",
                "An environment variable overrides the login",
            ),
            ("fallback_login", "A login file is left behind the keychain"),
            ("parked_login_refused", "A parked login was refused"),
            ("lock_compromised", "The login may have been written twice"),
            ("parks_pending_removal", "Old parked logins are still there"),
            (
                "written_on_the_command_line",
                "A login was passed on the command line",
            ),
            (
                "sign_in_parked_not_in_use",
                "The new login was parked, not put in use",
            ),
            (
                "interrupted_switch_finished",
                "An interrupted switch was finished",
            ),
            (
                "interrupted_switch_undone",
                "An interrupted switch was undone",
            ),
            ("recovery_undetermined", "An interrupted switch is waiting"),
            ("login_replaced", "A login was replaced outside Pitboard"),
            ("in_use_unconfirmed", "The account in use is not confirmed"),
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
            assert_eq!(
                warning_heading(Os::MacOs, &warning(code)),
                heading,
                "{code}"
            );
        }
        let distinct: std::collections::BTreeSet<_> = headings.iter().map(|(_, h)| h).collect();
        assert_eq!(distinct.len(), headings.len(), "no two codes read alike");
        assert_eq!(
            warning_heading(Os::MacOs, &warning("not_a_code_yet")),
            "Pitboard has a warning"
        );
        assert_eq!(
            warning_heading(Os::MacOs, &warning("")),
            "Pitboard has a warning"
        );
        assert_eq!(
            warning_heading(Os::Windows, &warning("fallback_login")),
            "A login file is left behind Credential Manager"
        );
    }

    // Each tool not found says what Pitboard needs of it, on each system in its own name.
    #[test]
    fn a_tool_not_found_says_what_to_install() {
        let codex = missing(Os::Windows, "codex").expect("codex");
        assert_eq!(codex.title, "Pitboard can’t find the Codex CLI on this PC.");
        assert_eq!(codex.link_title, "How to install the Codex CLI");
        let claude = missing(Os::MacOs, "claude").expect("claude");
        assert_eq!(claude.title, "Pitboard can’t find Claude Code on this Mac.");
        assert_eq!(
            claude.detail,
            "Adding a Claude Code account runs Claude Code’s own sign-in. Install it, then \
             come back to this window."
        );
        assert_eq!(claude.link_title, "How to Install Claude Code");
        let desktop = missing(Os::MacOs, "desktop").expect("desktop");
        assert_eq!(
            desktop.title,
            "Pitboard can’t find Claude Desktop on this Mac."
        );
        assert_eq!(
            desktop.detail,
            "Pitboard finds Claude Desktop by the data it keeps once it has been opened. Open \
             Claude, sign in, then come back to this window."
        );
        assert_eq!(missing(Os::MacOs, "nobody"), None);
    }

    /// A clock time the person's own clock could not say is said in UTC and named so, from
    /// the moments alone: whatever zone the machine running this is in, the same words. It
    /// names its weekday on another day of UTC, as far back as before 1970.
    #[test]
    fn a_clock_nobody_could_read_is_said_in_utc_and_named_so() {
        // 12:00 UTC on Wednesday 14 January 2026.
        let midday = 1_768_392_000;
        assert_eq!(
            utc_clock(midday + 2 * HOUR + 5 * MINUTE, midday),
            "14:05 UTC"
        );
        assert_eq!(utc_clock(midday - 12 * HOUR, midday), "00:00 UTC");
        assert_eq!(utc_clock(midday + 12 * HOUR, midday), "Thu 00:00 UTC");
        assert_eq!(utc_clock(midday + 59, midday + 6 * DAY), "Wed 12:00 UTC");
        assert_eq!(utc_clock(0, midday), "Thu 00:00 UTC");
        assert_eq!(utc_clock(-1, midday), "Wed 23:59 UTC");
    }

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

    /// A check is shown as a shape and a colour, and said as a word. If two levels ever came
    /// to sound the same, a broken check would read as a passing one. Its shape and its colour
    /// are each app's own.
    #[test]
    fn every_level_sounds_like_itself() {
        let spoken: Vec<&str> = [Level::Ok, Level::Warn, Level::Fail]
            .into_iter()
            .map(spoken_level)
            .collect();
        assert_eq!(spoken, ["Passed", "Worth looking at", "Failed"]);
        let mut distinct = spoken.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), spoken.len());
        assert!(spoken.iter().all(|word| !word.is_empty()));
    }

    /// The activity list names a change by the verb the log keeps, in words, and one it does
    /// not know yet by the verb itself made readable rather than not at all.
    #[test]
    fn a_change_is_named_by_its_verb_and_an_unknown_one_readably() {
        for (verb, name) in [
            ("switch", "Switch"),
            ("enroll", "Enrol"),
            ("forget", "Forget"),
            ("rename", "Rename"),
            ("renew", "Renew"),
            ("abandon", "Give up on a switch"),
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
        assert_eq!(change_verb("in-use"), "Login in use changed");
        assert_eq!(change_outcome("signed_in_outside"), "Signed in outside");
        assert_eq!(change_outcome("login_replaced"), "Login replaced");
    }

    /// Putting away a login left in a file, which the core logs as `stow`, is named for what
    /// it does, and said as it ended: what became of the login.
    #[test]
    fn putting_away_a_login_left_in_a_file_is_named_for_what_it_does() {
        assert_eq!(change_verb("stow"), "Put away a login left in a file");
        assert_eq!(change_outcome("park_kept"), "Park kept");
        assert_eq!(change_outcome("second_sign_in"), "Second sign-in");
    }

    // The notices' sentences, pinned once: the panel's tests compare with these functions.
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
        assert_eq!(has_a_new_login(None, "work"), "work has a new login");
        assert_eq!(
            has_a_new_login(Some("Codex"), "work"),
            "work in Codex has a new login"
        );
        assert_eq!(
            signed_in_now("work", true, None),
            "Signed in to work again. Its new login is the one in use now."
        );
        assert_eq!(
            signed_in_now("work", false, None),
            "Enrolled work, the account signed in now. Its new login is the one in use."
        );
        assert_eq!(
            gave_up("personal", "work", 2),
            "The switch from personal to work was given up. 2 logins kept, and nothing was \
             deleted."
        );
        assert_eq!(
            gave_up("personal", "work", 1),
            "The switch from personal to work was given up. 1 login kept, and nothing was \
             deleted."
        );
        assert_eq!(ran_out("work", "5-hour"), "work has no 5-hour limit left");
        assert_eq!(
            room_left("spare", Some(80)),
            "spare has 80% of its own left."
        );
        assert_eq!(room_left("seat", None), "seat has no such limit.");
        assert_eq!(
            of_tool(Some("Claude Code"), "work has no 5-hour limit left"),
            "Claude Code: work has no 5-hour limit left"
        );
        assert_eq!(things_to_look_at(2), "2 things to look at");
        assert_eq!(
            switch_waits_for("Anthropic or OpenAI"),
            "An interrupted switch can’t be finished until Anthropic or OpenAI answers."
        );
        assert_eq!(
            give_up_stuck(&[]),
            "Every login is kept, and nothing is deleted. Pitboard stops trying to finish it."
        );
        assert_eq!(
            give_up_stuck(&[("Claude Desktop".into(), "Claude")]),
            "Every login is kept, and nothing is deleted. Pitboard stops trying to finish it and, \
             with Claude quit, settles or gives up the interrupted Claude Desktop move too."
        );
        assert_eq!(
            move_waits_too("Claude Desktop", "Claude"),
            "A Claude Desktop move was interrupted too. With Claude quit, Give Up… settles it or \
             gives it up as well, and keeps every folder."
        );
        assert_eq!(
            unfinished("Claude Desktop", None),
            "Adding a Claude Desktop account isn’t finished"
        );
        assert_eq!(
            unfinished("Claude Desktop", Some("home")),
            "Signing in to home again isn’t finished"
        );
        assert_eq!(
            give_up_on_move("Claude"),
            "Every folder is kept and nothing is deleted. Whatever is in Claude’s folder stays \
             there."
        );
        assert_eq!(
            used_then("work", "96% of its 5-hour limit", "It ended."),
            "work has used 96% of its 5-hour limit. It ended."
        );
        assert_eq!(needs_signing_in("work"), "work needs signing in again");
    }

    // A notice's and a notification's buttons, as each system titles a button.
    #[test]
    fn a_notice_offers_in_these_words() {
        let both = |said: &dyn Fn(Os) -> String| (said(Os::MacOs), said(Os::Windows));
        let pair = |mac: &str, windows: &str| (mac.to_owned(), windows.to_owned());
        assert_eq!(
            both(&|os| finish(os, false)),
            pair("Finish Adding…", "Finish adding…")
        );
        assert_eq!(
            both(&|os| finish(os, true)),
            pair("Finish Signing In…", "Finish signing in…")
        );
        assert_eq!(
            both(&|os| put_back(os, "work")),
            pair("Put work Back", "Put work back")
        );
        assert_eq!(both(&|os| give_up(os, true)), pair("Give Up…", "Give up…"));
        assert_eq!(both(&|os| give_up(os, false)), pair("Give Up", "Give up"));
        assert_eq!(
            both(&|os| put_away(os, true)),
            pair("Put Away…", "Put away…")
        );
        assert_eq!(
            both(&|os| put_away(os, false)),
            pair("Put Away", "Put away")
        );
        assert_eq!(
            both(&|os| sign_in_again(os, true)),
            pair("Sign In Again…", "Sign in again…")
        );
        assert_eq!(
            both(&|os| sign_in_again(os, false)),
            pair("Sign In Again", "Sign in again")
        );
        assert_eq!(
            both(&|os| add_account_shown(os, "Claude", true)),
            pair(
                "Add the Account Claude Shows…",
                "Add the account Claude shows…"
            )
        );
        assert_eq!(
            both(&|os| add_account_shown(os, "Claude", false)),
            pair(
                "Add the Account Claude Shows",
                "Add the account Claude shows"
            )
        );
        assert_eq!(
            both(&update_config),
            pair("Update Claude Code’s Config", "Update Claude Code’s config")
        );
    }

    // A sheet's words, by what it does, pinned once: the sheets' and the model's tests compare
    // with these functions.
    #[test]
    fn a_sheet_says_what_it_does_in_these_words() {
        assert_eq!(
            sign_in_message("Codex", None),
            "Pitboard opens Codex’s own sign-in in your browser. Sign in as the account you’re \
             adding, and Pitboard parks its login beside the one in use."
        );
        assert_eq!(
            sign_in_message("Codex", Some("work")),
            "Pitboard opens Codex’s own sign-in in your browser. Sign in as work to give \
             Pitboard a new login for it."
        );
        assert_eq!(
            record_shown("Claude", "home"),
            "Open Claude and check that it shows home, then quit it. Pitboard records the \
             account Claude is signed in to as home’s login."
        );
        assert_eq!(
            park_to_sign_in_again("Claude", "home", "work"),
            "Pitboard parks work, which leaves Claude signed out and signs nothing out at \
             claude.ai. Then you sign in to home again in Claude, and Pitboard records it once \
             Claude quits."
        );
        assert_eq!(
            park_to_sign_in_again("Claude", "work", "work"),
            "Pitboard parks work’s login as it is, which leaves Claude signed out and signs \
             nothing out at claude.ai. Then you sign in to work again in Claude, and Pitboard \
             records it once Claude quits."
        );
        assert_eq!(
            wait_to_sign_in_again("Claude", "home"),
            "Claude is signed out. Pitboard waits while you sign in to home in Claude, and \
             records it once Claude quits."
        );
        assert_eq!(
            record_first("Claude"),
            "Pitboard records the account Claude is signed in to under this name. Check which \
             account Claude shows first: Pitboard can’t ask claude.ai whose it is."
        );
        assert_eq!(
            add_once_signed_in("Claude", "work"),
            "Claude is signed out so you can sign in to the account you’re adding, and work is \
             parked. Sign in to it in Claude and quit Claude, then add it here."
        );
        assert_eq!(
            record_unidentified("Claude"),
            "Pitboard can’t identify the login Claude holds. Open Claude, check which account it \
             shows, and quit it. Pitboard records that account under this name: one you added, \
             or a new one."
        );
        assert_eq!(
            park_to_add("Claude", "work"),
            "Pitboard parks work, which leaves Claude signed out and signs nothing out at \
             claude.ai. Then you sign in to the other account in Claude, and Pitboard adds it \
             once Claude quits."
        );
        assert_eq!(
            wait_to_add("Claude"),
            "Claude is signed out. Pitboard waits while you sign in to the account you’re adding \
             in Claude, and adds it once Claude quits."
        );
        assert_eq!(park("work"), "Park work");
        assert_eq!(record_as("home"), "Record as home");
        assert_eq!(recording("Claude", Recording::Quitting), "Quitting Claude…");
        assert_eq!(
            recording("Claude", Recording::Reading),
            "Reading Claude’s account…"
        );
        assert_eq!(
            naming("a@example.com", "Codex"),
            "a@example.com is signed in to Codex. Pitboard parks its login under this name \
             whenever you switch to another account."
        );
        assert_eq!(rename_title("work"), "Rename “work”");
        assert_eq!(
            RENAMING,
            "The account keeps its parked login. Only the name you switch to it by changes, here \
             and in the command line."
        );
        assert_eq!(
            stow_message(Os::Windows),
            "Claude Code left a login in a file behind Credential Manager, which keeps sessions \
             already running from following a switch. Pitboard keeps that login for its account \
             where it holds none it can switch to, then deletes the file."
        );
        assert_eq!(
            finish_in_browser("work", "Codex"),
            "Finish signing in as work in your browser. This closes once Codex says you’re in."
        );
        assert_eq!(
            code_note("Claude Code"),
            "Claude Code takes the code shown after you sign in, whether or not your browser \
             came back to it."
        );
        assert_eq!(
            code_refused("Claude Code"),
            "Claude Code didn’t take that code. Copy the whole code your browser shows, and \
             paste it again."
        );
        assert_eq!(
            quitting("Claude"),
            (
                "Pitboard is quitting Claude. Claude may ask about work in progress first.".into(),
                "Quitting Claude".into()
            )
        );
        assert_eq!(
            parking("work"),
            ("Parking work.".into(), "Parking work".into())
        );
        assert_eq!(waiting_to_quit("Claude"), "Waiting for Claude to quit");
    }

    // A sheet's titles and buttons, as each system titles them.
    #[test]
    fn a_sheet_is_titled_as_each_system_titles_one() {
        let both = |said: &dyn Fn(Os) -> String| (said(Os::MacOs), said(Os::Windows));
        let pair = |mac: &str, windows: &str| (mac.to_owned(), windows.to_owned());
        assert_eq!(both(&sign_in), pair("Sign In", "Sign in"));
        assert_eq!(both(&add_account), pair("Add Account", "Add account"));
        assert_eq!(
            both(&|os| sign_in_to_again(os, "work")),
            pair("Sign In to work Again", "Sign in to work again")
        );
        assert_eq!(both(&start_sign_in), pair("Start Sign-In", "Start sign-in"));
        assert_eq!(
            both(&name_this_account),
            pair("Name This Account", "Name this account")
        );
        assert_eq!(
            both(&stow_title),
            pair(
                "Put Away the Login Left in a File",
                "Put away the login left in a file"
            )
        );
        assert_eq!(
            both(&|os| signing_in_to(os, "Codex")),
            pair("Signing In to Codex", "Signing in to Codex")
        );
        assert_eq!(
            both(&|os| replace_title(os, "Claude Code")),
            pair(
                "Replace Claude Code’s Login?",
                "Replace Claude Code’s login?"
            )
        );
    }

    #[test]
    fn the_settings_and_the_open_link_window_say_these() {
        assert_eq!(share_range(50, 99), "From 50% to 99%");
        assert_eq!(
            open_link_as("chatgpt.com"),
            "Open this chatgpt.com link as:"
        );
        assert_eq!(
            (cannot_open_link(Os::MacOs), cannot_open_link(Os::Windows)),
            ("Can’t Open This Link".into(), "Can’t open this link".into())
        );
        assert_eq!(
            (
                could_not_read_accounts(Os::MacOs),
                could_not_read_accounts(Os::Windows)
            ),
            (
                "Couldn’t Read Accounts".into(),
                "Couldn’t read accounts".into()
            )
        );
        assert_eq!(
            (try_again(Os::MacOs), try_again(Os::Windows)),
            ("Try Again".into(), "Try again".into())
        );
        assert_eq!(
            (warm_up_now(Os::MacOs), warm_up_now(Os::Windows)),
            ("Warm Up Now".into(), "Warm up now".into())
        );
        assert_eq!(add_a_second(None), "Add a second account");
        assert_eq!(add_a_second(Some("Codex")), "Add a second Codex account");
        assert_eq!(
            no_link(Os::MacOs),
            (
                "No Link to Open".into(),
                "Share a claude.ai or chatgpt.com page with Pitboard from your browser’s Share \
                 menu to open it as one of your accounts."
                    .into()
            )
        );
        assert_eq!(no_link(Os::Windows).0, "No link to open");
    }

    #[test]
    fn a_quit_question_says_what_the_quit_is_for() {
        assert_eq!(quit_to_switch("Claude"), "Quit Claude to switch?");
        assert_eq!(
            quit_to_sign_in("Claude", None),
            "Quit Claude to add an account?"
        );
        assert_eq!(
            quit_to_sign_in("Claude", Some("home")),
            "Quit Claude to sign in to home again?"
        );
        assert_eq!(
            quit_to_record("Claude", "home", true),
            "Quit Claude to record home?"
        );
        assert_eq!(
            quit_to_record("Claude", "travel", false),
            "Quit Claude to add travel?"
        );
        assert_eq!(
            quit_and_reopen("ChatGPT"),
            "ChatGPT keeps using the account it started with until it quits. Pitboard quits it, \
             switches, and opens it again."
        );
        assert_eq!(
            quit_to_move("Claude", "home"),
            "Pitboard moves Claude’s login only while Claude is closed. It quits Claude, switches \
             to home, and opens it again."
        );
        assert_eq!(
            quit_to_park("Claude", "work"),
            "Pitboard parks work only while Claude is closed. It quits Claude, parks work, and \
             opens Claude again for you to sign in."
        );
        assert_eq!(
            quit_to_wait("Claude"),
            "Pitboard starts waiting for a sign-in only while Claude is closed. It quits Claude \
             and opens it again for you to sign in."
        );
        assert_eq!(
            quit_to_read("Claude", "home", true),
            "Pitboard reads which account Claude is signed in to only while Claude is closed. It \
             quits Claude, records that account as home’s login, and opens it again."
        );
        assert_eq!(
            quit_to_read("Claude", "travel", false),
            "Pitboard reads which account Claude is signed in to only while Claude is closed. It \
             quits Claude, records that account as travel, and opens it again."
        );
        let both = |said: &dyn Fn(Os) -> String| (said(Os::MacOs), said(Os::Windows));
        let pair = |mac: &str, windows: &str| (mac.to_owned(), windows.to_owned());
        assert_eq!(
            both(&|os| quit_and_switch(os, "Claude")),
            pair("Quit Claude and Switch", "Quit Claude and switch")
        );
        assert_eq!(
            both(&|os| quit_and_park(os, "Claude", "work")),
            pair("Quit Claude and Park work", "Quit Claude and park work")
        );
        assert_eq!(
            both(&|os| quit_and_start_sign_in(os, "Claude")),
            pair(
                "Quit Claude and Start Sign-In",
                "Quit Claude and start sign-in"
            )
        );
        assert_eq!(
            both(&|os| quit_and_record(os, "Claude", true)),
            pair("Quit Claude and Record", "Quit Claude and record")
        );
        assert_eq!(
            both(&|os| quit_and_record(os, "Claude", false)),
            pair("Quit Claude and Add", "Quit Claude and add")
        );
    }

    #[test]
    fn the_wait_is_said_step_by_step() {
        use pitboard_core::switch::WaitStep;
        let both = |message: &str| {
            format!(
                "{message}\n\nDon’t choose Log out in Claude: it ends the login Claude holds at \
                 claude.ai, and Pitboard can’t bring it back."
            )
        };
        let said = |again, step, opened| in_app_waiting("Claude", "travel", again, step, opened);
        assert_eq!(
            said(false, Some(WaitStep::Waiting), true),
            both(
                "In Claude, sign in to the account you’re adding. When it shows your chats, quit \
                 Claude with Command-Q, and Pitboard adds it as travel."
            )
        );
        assert_eq!(
            said(false, Some(WaitStep::Open), false),
            both(
                "Open Claude and sign in to the account you’re adding. When it shows your chats, \
                 quit Claude with Command-Q, and Pitboard adds it as travel."
            )
        );
        assert_eq!(
            said(false, Some(WaitStep::QuitSignedOut), true),
            both(
                "Claude quit with nobody signed in. Open Claude, sign in to the account you’re \
                 adding, then quit it with Command-Q."
            )
        );
        assert_eq!(
            said(false, Some(WaitStep::QuitUnfinished), true),
            both(
                "Claude quit before it finished signing in. Open Claude, wait until it shows \
                 your chats, then quit it with Command-Q."
            )
        );
        assert_eq!(
            said(true, None, true),
            both(
                "In Claude, sign in to travel again. When it shows your chats, quit Claude with \
                 Command-Q, and Pitboard records it as travel’s login."
            )
        );
    }

    /// A change that worked says so in a word, and one that did not says what stopped it,
    /// from the code the log keeps.
    #[test]
    fn a_change_says_how_it_ended() {
        assert_eq!(change_outcome("ok"), "Done");
        assert_eq!(
            change_outcome("parked_login_expired"),
            "Parked login expired"
        );
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
            no_scheduler(Os::MacOs),
            "This Mac has no scheduler Pitboard knows how to write to."
        );
        assert_eq!(
            no_scheduler(Os::Linux),
            "This computer has no scheduler Pitboard knows how to write to."
        );
        assert_eq!(
            no_scheduler(Os::Windows),
            "This PC has no scheduler Pitboard knows how to write to."
        );
        assert_eq!(checking(Os::MacOs), "Checking this Mac…");
        assert_eq!(checking(Os::Linux), "Checking this computer…");
        assert_eq!(checking(Os::Windows), "Checking this PC…");
        for os in [Os::MacOs, Os::Linux, Os::Windows] {
            assert_eq!(
                cannot_schedule(os, false),
                "This copy of Pitboard has no command line inside it to run on a schedule."
            );
        }
        assert_eq!(
            cannot_schedule(Os::MacOs, true),
            "Move Pitboard to your Applications folder first. Until then macOS runs it from a \
             temporary copy, which is gone once Pitboard quits."
        );
        assert_eq!(
            cannot_link(Os::MacOs),
            "Move Pitboard to your Applications folder first. Until then macOS runs it from a \
             temporary copy, and a link to that would break."
        );
        assert_eq!(
            cannot_schedule(Os::Linux, true),
            "First move Pitboard to a folder it will stay in. Until then it runs from a \
             temporary copy, which is gone once Pitboard quits."
        );
        assert_eq!(
            cannot_link(Os::Linux),
            "First move Pitboard to a folder it will stay in. Until then it runs from a \
             temporary copy, and a link to that would break."
        );
        assert_eq!(
            cannot_schedule(Os::Windows, true),
            "Install Pitboard first. Until then Windows runs it from a temporary copy, which is \
             gone once Pitboard quits."
        );
        assert_eq!(
            cannot_link(Os::Windows),
            "Install Pitboard first. Until then Windows runs it from a temporary copy, and a \
             link to that would break."
        );
        for said in [
            cannot_schedule(Os::Linux, true),
            cannot_link(Os::Linux),
            cannot_schedule(Os::Windows, true),
            cannot_link(Os::Windows),
        ] {
            assert!(
                !said.contains("macOS") && !said.contains("Applications"),
                "{said}"
            );
        }
    }

    #[test]
    fn the_checks_say_when_they_were_made() {
        assert_eq!(checked_at("14:05"), "Checked at 14:05");
    }

    #[test]
    fn a_parked_login_refused_is_named_as_the_app_names_its_account() {
        assert_eq!(
            in_app_refusal(
                Os::MacOs,
                &Error::ParkedCredentialMissing {
                    label: "desktop/home".into(),
                }
            )
            .as_deref(),
            Some("The parked login for home is missing. Choose Sign In Again… on home’s row.")
        );
        assert_eq!(
            in_app_refusal(
                Os::MacOs,
                &Error::ParkedCredentialCorrupt {
                    label: "desktop/home".into(),
                    detail: "it was parked on another Mac".into(),
                }
            )
            .as_deref(),
            Some(
                "The parked login for home isn’t the one Pitboard recorded (it was parked on \
                 another Mac). Choose Sign In Again… on home’s row."
            )
        );
        assert_eq!(
            in_app_refusal(
                Os::MacOs,
                &Error::ParkedCredentialMissing {
                    label: "work".into(),
                }
            ),
            None,
            "Claude Code's says the core's sentence"
        );
    }

    #[test]
    fn a_parked_login_anthropic_will_not_name_is_signed_in_to_again_on_its_row() {
        let untold = |cause| Error::IdentityUnverifiable {
            tool: ProviderId::Claude,
            login: Unidentified::Parked {
                label: "dtg".into(),
            },
            cause,
            detail: "Anthropic answered 400".into(),
            spelling: pitboard_core::words::Spelling::Bare,
        };
        assert_eq!(
            in_app_refusal(Os::MacOs, &untold(Cause::Refused)).as_deref(),
            Some(
                "Pitboard couldn’t confirm with Anthropic whose login is parked for dtg \
                 (Anthropic answered 400), so nothing has changed. Choose Sign In Again… on \
                 dtg’s row."
            )
        );
        assert_eq!(
            in_app_refusal(Os::MacOs, &untold(Cause::Unreachable)),
            None,
            "an outage is the core's sentence: try again"
        );
    }

    #[test]
    fn a_login_refused_for_good_says_replacing_it_loses_nothing() {
        let refused = Error::LiveLoginRefused {
            tool: ProviderId::Claude,
            label: "work".into(),
            detail: "its login has expired and cannot be renewed".into(),
        };
        assert_eq!(
            in_app_refusal(Os::MacOs, &refused).as_deref(),
            Some(
                "Anthropic no longer accepts Claude Code’s login in use (its login has expired \
                 and can’t be renewed), so putting work’s login in its place loses nothing."
            )
        );
    }

    #[test]
    fn claudes_settings_naming_another_account_are_said_as_the_command_line_says_them() {
        let unconfirmed = CoreWarning::InUseUnconfirmed {
            tool: ProviderId::Desktop,
            last: Stored::Account("desktop/work".into()),
            named: Stored::Unenrolled(String::new()),
            asked: false,
        };
        assert_eq!(
            in_app_warning(Os::MacOs, &unconfirmed).as_deref(),
            Some(
                "Claude’s settings have named an account you haven’t added since Pitboard last \
                 read work in use, so Claude may hold another login. Open Claude, check which \
                 account it shows, quit it, then choose Add the Account Claude Shows."
            )
        );
    }

    #[test]
    fn a_list_in_the_apps_words_has_no_serial_comma() {
        let still = Error::AppOpen {
            app: "Claude",
            still: vec![
                "ShipIt (pid 812)".into(),
                "Claude Helper (pid 813)".into(),
                "chrome_crashpad_handler (pid 814)".into(),
            ],
        };
        let said = in_app_refusal(Os::MacOs, &still).expect("in the app's words");
        assert!(
            said.contains(
                "ShipIt (pid 812), Claude Helper (pid 813) and chrome_crashpad_handler (pid 814) \
                 from it"
            ),
            "{said}"
        );
    }

    #[test]
    fn the_app_says_to_quit_what_is_left_of_an_app_in_task_manager_on_windows() {
        let still = |app| Error::AppOpen {
            app,
            still: vec!["Helper (pid 812)".into()],
        };
        for (os, monitor) in [
            (Os::Windows, "Task Manager"),
            (Os::MacOs, "Activity Monitor"),
        ] {
            assert_eq!(
                in_app_refusal(os, &still("ChatGPT")).expect("the app's words"),
                format!(
                    "ChatGPT has quit, and Helper (pid 812) from it is still running, and \
                     Pitboard changes ChatGPT’s login only while all of it is closed. Wait until \
                     it ends, or quit it in {monitor}, then try again. Nothing has changed."
                )
            );
            let not_given_up = CoreWarning::MoveNotGivenUp {
                tool: ProviderId::Desktop,
                label: "desktop/work".into(),
                why: still("Claude"),
            };
            assert_eq!(
                in_app_warning(os, &not_given_up).expect("the app's words"),
                format!(
                    "Pitboard didn’t give up the interrupted move of work’s login, because Helper \
                     (pid 812) from Claude is still running. Wait until it ends, or quit it in \
                     {monitor}, then use any Claude Desktop account: Pitboard settles the move \
                     first."
                )
            );
        }
    }

    #[test]
    fn a_move_not_given_up_over_a_park_it_could_not_look_at_says_why_and_how_out() {
        let why = pitboard_core::testing::park_unread(
            "desktop/work",
            std::path::Path::new("/Users/me/.pitboard/parks/a1"),
            "permission denied",
        );
        assert_eq!(
            in_app_warning(
                Os::MacOs,
                &CoreWarning::MoveNotGivenUp {
                    tool: ProviderId::Desktop,
                    label: "desktop/work".into(),
                    why,
                },
            )
            .expect("the app's words"),
            "Pitboard didn’t give up the interrupted move of work’s login, because work’s \
             parked login at /Users/me/.pitboard/parks/a1 couldn’t be looked at (permission \
             denied). Make that path, and each folder it is in, readable to you, then use any \
             Claude Desktop account: Pitboard settles the move first."
        );
    }

    #[test]
    fn a_move_given_up_is_summed_up_as_the_command_line_sums_it_up() {
        let given_up = pitboard_core::switch::GivenUp {
            tool: ProviderId::Desktop,
            label: "work".into(),
            parked: 1,
            set_aside: 0,
            left: vec!["Session Storage".into()],
            slot: Some("/Volumes/Old/Claude".into()),
        };
        assert_eq!(
            in_app_given_up(&given_up),
            "Pitboard gave up the move of work’s login and kept every folder: 1 parked, 0 set \
             aside. The move was in /Volumes/Old/Claude, which Pitboard didn’t read. Session \
             Storage stays in Claude’s folder. Open Claude once and quit it with Command-Q, then \
             choose Add Account… and Add the Account Claude Shows for the account Claude shows, \
             or Use on an account where Claude is signed out."
        );
        assert!(given_up.to_string().starts_with(
            "Pitboard gave up the move of `desktop/work`'s login and kept every folder: 1 parked, \
             0 set aside. The move was in /Volumes/Old/Claude, which Pitboard did not read. \
             `Session Storage` stays in Claude's folder. Open Claude once and quit it with \
             Command-Q, then run `pitboard enroll desktop/<label>`"
        ));
    }

    #[test]
    fn a_move_not_given_up_never_says_nothing_has_changed() {
        let tool = ProviderId::Desktop;
        let app = "Claude";
        let said = |why| {
            in_app_warning(
                Os::MacOs,
                &CoreWarning::MoveNotGivenUp {
                    tool,
                    label: "desktop/work".into(),
                    why,
                },
            )
            .expect("the app's words")
        };
        assert_eq!(
            said(Error::AppOpen {
                app,
                still: Vec::new()
            }),
            "Pitboard didn’t give up the interrupted move of work’s login, because Claude is \
             open. Quit Claude, then use any Claude Desktop account: Pitboard settles the move \
             first."
        );
        for why in [
            Error::LoginFolderBusy { app },
            Error::MoveMixed {
                label: "desktop/work".into(),
                places: vec!["`Session Storage` in Claude's folder".into()],
            },
            Error::MachineUnknown,
        ] {
            let sentence = said(why);
            assert!(
                !sentence.to_lowercase().contains("nothing has changed"),
                "{sentence}"
            );
        }
    }

    // On the session's PC (2026-10-18) the banner told the app to run `pitboard enroll a
    // --sign-in` beside the row's own Sign in again… button. Its notice offers that sign-in now.
    #[test]
    fn a_login_replaced_outside_names_no_command_and_no_button() {
        use pitboard_core::state::Key;
        let replaced = |tool: ProviderId, now: Stored| CoreWarning::LoginReplaced {
            tool,
            id: "claude_5b1e".into(),
            label: Key::new(tool, "a").typed(),
            now,
        };
        assert_eq!(
            in_app_warning(
                Os::Windows,
                &replaced(
                    ProviderId::Claude,
                    Stored::Account(Key::new(ProviderId::Claude, "b").typed())
                )
            )
            .as_deref(),
            Some(
                "a’s login was replaced by a sign-in outside Pitboard: Claude Code now has b’s \
                 login stored, and Pitboard holds no login for a."
            )
        );
        assert_eq!(
            in_app_warning(Os::MacOs, &replaced(ProviderId::Codex, Stored::Nothing)).as_deref(),
            Some(
                "a’s login was replaced by a sign-in outside Pitboard: Codex now has no login \
                 stored, and Pitboard holds no login for a."
            )
        );
        for tool in [ProviderId::Claude, ProviderId::Codex, ProviderId::Desktop] {
            for now in [
                Stored::Unenrolled(String::new()),
                Stored::Unenrolled("x@example.com".into()),
                Stored::Unknown,
                Stored::Nothing,
            ] {
                let said = in_app_warning(Os::Windows, &replaced(tool, now)).expect("the app's");
                assert!(!said.contains("pitboard "), "{said}");
                assert!(!said.contains("Sign in again"), "{said}");
            }
        }
    }

    // A label 0.1.x wrote can hold a slash, and is typed whole where what comes before it names
    // no tool.
    #[test]
    fn a_label_with_a_slash_from_0_1_is_said_whole() {
        let replaced = CoreWarning::LoginReplaced {
            tool: ProviderId::Claude,
            id: "claude_5b1e".into(),
            label: "team/a".into(),
            now: Stored::Nothing,
        };
        assert_eq!(
            in_app_warning(Os::MacOs, &replaced).as_deref(),
            Some(
                "team/a’s login was replaced by a sign-in outside Pitboard: Claude Code now has \
                 no login stored, and Pitboard holds no login for team/a."
            )
        );
    }

    #[test]
    fn no_claude_desktop_sentence_in_the_app_names_a_command() {
        use pitboard_core::error::Enrolled;
        use pitboard_core::state::Half;
        use pitboard_core::switch::{Ended, GivenUp, MoveSettled};
        use std::path::PathBuf;
        let tool = ProviderId::Desktop;
        let app = "Claude";
        let typed = |label: &str| format!("desktop/{label}");
        let mut errors = vec![
            Error::AppOpen {
                app,
                still: Vec::new(),
            },
            Error::AppOpen {
                app,
                still: vec!["ShipIt (pid 812)".into()],
            },
            Error::AppStateUnknown {
                app,
                why: "`ps` did not answer within 5 seconds".into(),
            },
            Error::LoginFolderBusy { app },
            Error::LoginFolderUnreadable {
                tool,
                detail: "database is locked".into(),
            },
            Error::SignInUnfinished { tool },
            Error::LoginInDoubt {
                tool,
                why: LoginDoubt::NamedAnother {
                    recorded: typed("work"),
                    named: Some(typed("home")),
                },
            },
            Error::LoginInDoubt {
                tool,
                why: LoginDoubt::NamedAnother {
                    recorded: typed("work"),
                    named: None,
                },
            },
            Error::LoginInDoubt {
                tool,
                why: LoginDoubt::SignedInOutside {
                    label: typed("home"),
                },
            },
            Error::DifferentVolume {
                parks: PathBuf::from("/Volumes/Data/.pitboard/parks"),
                root: PathBuf::from("/Users/dana/Library/Application Support/Claude"),
            },
            Error::MachineUnknown,
            Error::AppOpenedMidway {
                app,
                label: Some(typed("work")),
            },
            Error::AppOpenedMidway { app, label: None },
            Error::MoveUndone {
                label: typed("work"),
                detail: "a rename failed".into(),
            },
            Error::MoveStopped {
                label: typed("work"),
                detail: "a rename failed".into(),
            },
            Error::RecoveryStopped {
                label: typed("work"),
                detail: "a rename failed".into(),
            },
            Error::SwitchLeftSignedOut {
                from: Some(typed("work")),
                to: typed("home"),
                detail: "a rename failed".into(),
            },
            Error::SwitchLeftSignedOut {
                from: None,
                to: typed("home"),
                detail: "a rename failed".into(),
            },
            Error::MoveMixed {
                label: typed("work"),
                places: vec!["`Session Storage` in Claude's folder".into()],
            },
            Error::SignInSettledElsewhere {
                label: typed("travel"),
            },
            Error::RecoveryUndetermined {
                tool,
                from: typed("work"),
                to: typed("work"),
                detail: "the jar is gone".into(),
                spelling: pitboard_core::words::Spelling::Bare,
            },
            Error::RecoveryElsewhere {
                tool,
                from: typed("work"),
                to: typed("work"),
                slot: "/Users/dana/Library/Application Support/Claude".into(),
            },
            Error::LiveStoreUnsupported {
                tool,
                reason: "Pitboard switches Claude Desktop accounts on macOS only".into(),
            },
            Error::LiveCredentialAbsent {
                tool,
                label: Some(typed("travel")),
                spelling: pitboard_core::words::Spelling::Bare,
            },
            Error::LiveAccountNotEnrolled {
                tool,
                who: String::new(),
            },
            Error::LabelTaken {
                label: typed("home"),
                who: "another Claude Desktop account".into(),
            },
            Error::NothingParked {
                tool,
                label: typed("home"),
            },
            Error::ParkedCredentialMissing {
                label: typed("home"),
            },
            Error::ParkedCredentialCorrupt {
                label: typed("home"),
                detail: "it was parked on another Mac, and Claude cannot read it here where its \
                         key differs"
                    .into(),
            },
            Error::CannotForgetActiveAccount {
                label: typed("work"),
            },
            Error::AccountUnknown {
                label: typed("home"),
                enrolled: Enrolled(vec![typed("work")]),
            },
            Error::AccountNotInUse {
                tool,
                label: typed("home"),
            },
        ];
        for waited in [false, true] {
            errors.push(Error::AlreadyEnrolled {
                tool,
                email: String::new(),
                label: typed("home"),
                asked: typed("travel"),
                waited,
            });
        }
        for then in [
            Then::Nothing,
            Then::OpenAndQuit,
            Then::UpdatePitboard,
            Then::CopyAside,
            Then::SignInAndEnrol,
        ] {
            errors.push(Error::LoginFolderUnknown {
                tool,
                what:
                    "holds `Cookies-wal`, a part of the account's data this Pitboard has not read"
                        .into(),
                then,
            });
        }
        for why in [
            Private::Shared,
            Private::Link,
            Private::NotOwn,
            Private::NotADirectory,
        ] {
            errors.push(Error::ParksNotPrivate {
                path: PathBuf::from("/Users/dana/.pitboard/parks"),
                why,
            });
        }
        let path = PathBuf::from("/Users/dana/.pitboard/parks/pitboard-aside-1791000400123");
        let mut warnings = vec![
            CoreWarning::TakenAsNamed {
                tool,
                label: typed("work"),
                recorded: None,
            },
            CoreWarning::TakenAsNamed {
                tool,
                label: typed("home"),
                recorded: Some(typed("work")),
            },
            CoreWarning::LooksSignedOut {
                tool,
                label: typed("work"),
            },
            CoreWarning::MoveWaiting {
                tool,
                label: typed("work"),
            },
            CoreWarning::ItemGone {
                tool,
                label: typed("work"),
                item: "WebStorage".into(),
            },
        ];
        for again in [false, true] {
            warnings.push(CoreWarning::SignInAwaited {
                tool,
                label: typed("home"),
                put_back: typed("work"),
                since: 0,
                again,
            });
        }
        for why in [
            Error::AppOpen {
                app,
                still: Vec::new(),
            },
            Error::AppOpen {
                app,
                still: vec!["ShipIt (pid 812)".into()],
            },
            Error::AppStateUnknown {
                app,
                why: "ps failed".into(),
            },
            Error::LoginFolderBusy { app },
            Error::LoginFolderUnreadable {
                tool,
                detail: "a test held the jar".into(),
            },
            Error::AppOpenedMidway {
                app,
                label: Some(typed("work")),
            },
            Error::MoveMixed {
                label: typed("work"),
                places: vec!["`Session Storage` in Claude's folder".into()],
            },
            Error::MachineUnknown,
        ] {
            warnings.push(CoreWarning::MoveNotGivenUp {
                tool,
                label: typed("work"),
                why,
            });
        }
        for (half, ended) in [
            (Half::Leave, Ended::Undone),
            (Half::Leave, Ended::Finished),
            (Half::Arrive, Ended::Undone),
            (Half::Arrive, Ended::Finished),
        ] {
            warnings.push(CoreWarning::MoveSettled(MoveSettled {
                tool,
                label: "work".into(),
                half,
                ended,
            }));
        }
        for why in [
            SetAside::InTheWay,
            SetAside::Superseded,
            SetAside::AnotherMachine,
            SetAside::GivenUp,
            SetAside::Orphan,
        ] {
            for label in [Some(typed("home")), None] {
                warnings.push(CoreWarning::SetAside {
                    tool,
                    path: path.clone(),
                    label,
                    why,
                });
            }
        }
        for label in [Some(typed("home")), None] {
            warnings.push(CoreWarning::KeysSetAside {
                tool,
                path: path.clone(),
                label,
            });
        }
        let stored = [
            Stored::Account(typed("home")),
            Stored::Unenrolled(String::new()),
            Stored::Nothing,
            Stored::Unknown,
        ];
        for now in stored.clone() {
            warnings.push(CoreWarning::LoginReplaced {
                tool,
                id: "desktop_5b1e".into(),
                label: typed("work"),
                now,
            });
        }
        for (named, last) in stored
            .iter()
            .flat_map(|named| stored.iter().map(move |last| (named, last)))
        {
            warnings.push(CoreWarning::InUseUnconfirmed {
                tool,
                last: last.clone(),
                named: named.clone(),
                asked: true,
            });
            warnings.push(CoreWarning::ConfigNamesAnother {
                tool,
                config: named.clone(),
                in_use: last.clone(),
            });
        }
        let given_up = |left: Vec<String>, slot: Option<String>| GivenUp {
            tool,
            label: "work".into(),
            parked: 1,
            set_aside: 1,
            left,
            slot,
        };
        let mut said: Vec<String> = errors
            .iter()
            .map(|error| in_app_refusal(Os::MacOs, error).unwrap_or_else(|| error.to_string()))
            .collect();
        said.extend(
            warnings
                .iter()
                .inspect(|warning| assert_eq!(warning.tool(), Some(tool)))
                .map(|warning| {
                    in_app_warning(Os::MacOs, warning).unwrap_or_else(|| warning.to_string())
                }),
        );
        said.push(in_app_given_up(&given_up(Vec::new(), None)));
        said.push(in_app_given_up(&given_up(
            vec!["Session Storage".into(), "WebStorage".into()],
            Some("/Volumes/Old/Claude".into()),
        )));
        for sentence in said {
            assert!(
                !sentence.contains("pitboard ") && !sentence.contains("run this again"),
                "{sentence}"
            );
        }
    }
}
