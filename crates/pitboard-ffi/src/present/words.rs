//! What both apps say and the command line does not, each sentence a function of typed values,
//! as `pitboard_core::words` holds what all three say. English, for now: a sentence is never
//! a format string a view fills in, so each can read a translation later by its name.
//!
//! What the command line says too is not here but there, and called from here: a limit's
//! names, a reset, a pace, a parked login's life.

use crate::{Level, Warning};
use pitboard_core::host::Os;
use pitboard_core::pace::{Pace, Standing};
use pitboard_core::usage::whole;
use unicode_segmentation::UnicodeSegmentation;

const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

/// "5-hour" stays "5-hour" and "weekly" becomes "Weekly": a phrase that starts a line starts
/// with a capital, and nothing else about it changes. Swift's `Character.uppercased`, which
/// the app's `capitalizedFirst` used, maps a letter as Rust's `char::to_uppercase` does, "ß"
/// to "SS" among them, and leaves the marks after it where they were.
pub(crate) fn capitalised(phrase: &str) -> String {
    let mut chars = phrase.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
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

/// A figure as a bar or a line says it, as the command line draws it and the automatic
/// switch judges it.
pub(crate) fn figure(percent: f64) -> String {
    format!("{}%", whole(percent))
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
    if in_seconds < MINUTE {
        "about to run out".into()
    } else {
        format!("runs out in {}", pitboard_core::words::span(in_seconds))
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
        Standing::Over { runs_out_in } if runs_out_in < MINUTE => {
            format!("{even} At this pace it is about to run out.")
        }
        Standing::Over { runs_out_in } => format!(
            "{even} At this pace it runs out in {}.",
            pitboard_core::words::span(runs_out_in)
        ),
        Standing::Under | Standing::Even => even,
    }
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

/// A warning's heading, from its code, so the menu can name it in a line. The message under
/// it is the core's own, which says what to do.
pub(crate) fn warning_heading(warning: &Warning) -> &'static str {
    match warning.code.as_str() {
        "sessions_still_running" => "Open sessions still use the previous account",
        "sessions_keep_old_login" => "Open sessions still use the old login",
        "sessions_unknown" => "Couldn’t tell which sessions are open",
        "auth_overridden" => "An environment variable overrides the login",
        "fallback_login" => "A login file is left behind the keychain",
        "parked_login_refused" => "A parked login was refused",
        "lock_compromised" => "The login may have been written twice",
        "parks_pending_removal" => "Old parked logins are still there",
        "written_on_the_command_line" => "A login was passed on the command line",
        "sign_in_parked_not_in_use" => "The new login was parked, not put in use",
        "interrupted_switch_finished" => "An interrupted switch was finished",
        "interrupted_switch_undone" => "An interrupted switch was undone",
        "recovery_undetermined" => "An interrupted switch is waiting",
        "login_replaced" => "A login was replaced outside Pitboard",
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

/// What giving up on an interrupted switch kept.
pub(crate) fn gave_up(from: &str, to: &str, kept: u32) -> String {
    format!(
        "The switch from {from} to {to} was given up. {} kept, and nothing was deleted.",
        logins(kept)
    )
}

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

/// Why a tool is missing from the sheet for a new account, rather than leaving it out without
/// a word.
pub(crate) fn not_offered(os: Os, names: &[&str], programs: &[&str]) -> String {
    let verb = if names.len() == 1 { "is" } else { "are" };
    format!(
        "{} {verb} not offered: Pitboard did not find {} on {}.",
        names.join(" and "),
        programs.join(" or "),
        this_machine(os)
    )
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

/// A change as the activity list names it, by the verb the log keeps: "Switch", "Enrol", and
/// one it does not know yet by the verb itself made readable rather than not at all. The
/// core logs a switch as `use`, after the command that makes one.
pub(crate) fn change_verb(verb: &str) -> String {
    match verb {
        "use" | "switch" => "Switch".into(),
        "auto-switch" => "Automatic switch".into(),
        "enroll" => "Enrol".into(),
        "forget" => "Forget".into(),
        "rename" => "Rename".into(),
        "renew" => "Renew".into(),
        "abandon" => "Give up on a switch".into(),
        "repair" => "Repair".into(),
        "adopt" => "Adopt".into(),
        "uninstall" => "Uninstall".into(),
        "in-use" => "Login in use changed".into(),
        _ => readable(verb),
    }
}

/// How a change ended: "Done", or what stopped it, from the code the log keeps.
pub(crate) fn change_outcome(outcome: &str) -> String {
    if outcome == "ok" {
        "Done".into()
    } else {
        readable(outcome)
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

/// What stands in for the activity list while it has nothing in it: its title, and why.
pub(crate) const NO_ACTIVITY: (&str, &str) =
    ("No Activity", "Pitboard lists every change it makes here.");

/// What Renew Now is beside before it has been pressed: what it does.
pub(crate) const RENEWS_WHAT_IS_DUE: &str = "Renew every parked login that is due.";

/// How often the schedule runs, from how many seconds apart its runs are: "Every day", and
/// otherwise in whole hours, as the Swift settings said it.
pub(crate) fn runs_every(seconds: u32) -> String {
    if seconds == 86_400 {
        "Every day".into()
    } else {
        format!("Every {} hours", seconds / 3600)
    }
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

/// What stands in for doctor's checks before there are any: "Checking this Mac…".
pub(crate) fn checking(os: Os) -> String {
    format!("Checking {}…", this_machine(os))
}

/// When the checks shown were made, at a clock time the person's clock says: "Checked at
/// 14:05".
pub(crate) fn checked_at(clock: &str) -> String {
    format!("Checked at {clock}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A phrase that starts a line starts with a capital, and nothing else about it changes:
    /// a figure first stays as it is, and so does a phrase already capitalised.
    ///
    /// PresentationTests.swift's onlyTheFirstLetterOfAPhraseIsCapitalised.
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

    /// VoiceOver reads the column's "30m" as thirty meters and "5h" as letters, so a limit
    /// is said in words: its name as a sentence says it, what it has used, and when it
    /// resets.
    ///
    /// WordingTests.swift's aLimitIsSpokenInWordsAndNotInItsColumnsShorthand.
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
        use pitboard_core::pace::{Pace, Standing};
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
        use pitboard_core::pace::{Pace, Standing};
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
    ///
    /// WordingTests.swift's aSpokenSpanOfTimeReadsTheSameInEveryRegion.
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

    /// The figure is a whole percentage, rounded the way a person rounds: a half goes up.
    #[test]
    fn a_figure_is_rounded_to_the_nearest_whole_percent() {
        assert_eq!(figure(64.4), "64%");
        assert_eq!(figure(64.5), "65%");
        assert_eq!(figure(0.4), "0%");
        assert_eq!(figure(100.0), "100%");
        assert_eq!(figure(130.2), "130%");
    }

    /// A warning is named in the menu by a heading from its code, each its own, and a code
    /// the app does not know yet still gets one rather than an empty line.
    ///
    /// PresentationTests.swift's everyWarningCodeHasItsOwnHeadingAndAnUnknownOneAGeneralOne.
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
        ];
        let warning = |code: &str| Warning {
            code: code.into(),
            message: String::new(),
            account: None,
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

    /// A tool not offered for a new account is named with where Pitboard looked, and the
    /// machine is named as each system names itself.
    #[test]
    fn a_tool_not_offered_says_where_pitboard_looked() {
        assert_eq!(
            not_offered(Os::MacOs, &["Codex"], &["codex"]),
            "Codex is not offered: Pitboard did not find codex on this Mac."
        );
        assert_eq!(
            not_offered(Os::Linux, &["Claude Code", "Codex"], &["claude", "codex"]),
            "Claude Code and Codex are not offered: Pitboard did not find claude or codex on \
             this computer."
        );
        assert_eq!(
            not_offered(Os::Windows, &["Codex"], &["codex"]),
            "Codex is not offered: Pitboard did not find codex on this PC."
        );
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
    ///
    /// WordingTests.swift's aCommandLineInstalledApartSaysHowToUpdateIt.
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
    ///
    /// WordingTests.swift's everyLevelLooksAndSoundsLikeItself, but for the symbols.
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
    ///
    /// PresentationTests.swift's aChangeIsNamedByItsVerbAndAnUnknownOneReadably.
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

    /// What Pitboard found changed outside it in whose login a tool has stored, which the core
    /// logs as `in-use`, is named for what happened, since its subject is the account signed
    /// in on one line and the account whose login went on another, and said as it ended.
    #[test]
    fn a_change_found_outside_pitboard_is_named_for_what_happened() {
        assert_eq!(change_verb("in-use"), "Login in use changed");
        assert_eq!(change_outcome("signed_in_outside"), "Signed in outside");
        assert_eq!(change_outcome("login_replaced"), "Login replaced");
    }

    /// A change that worked says so in a word, and one that did not says what stopped it,
    /// from the code the log keeps.
    ///
    /// PresentationTests.swift's aChangeSaysHowItEnded.
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
    ///
    /// PresentationTests.swift's aChangeSaysWhoAskedForIt.
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

    /// How often the schedule runs, as the settings said it: daily, and otherwise in hours;
    /// and when the checks shown were made.
    #[test]
    fn the_schedule_says_how_often_it_runs() {
        assert_eq!(runs_every(86_400), "Every day");
        assert_eq!(runs_every(43_200), "Every 12 hours");
        assert_eq!(checked_at("14:05"), "Checked at 14:05");
    }
}
