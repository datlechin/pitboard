//! What a row, the menu and the window say of an account: its name, standing and saved
//! sign-in, each limit and its reset, when the numbers were read, and the answer to a switch
//! asked for from outside the app.

use super::{ADD_ASKING, SWITCH, titled, uncapitalised};
use crate::Front;
use pitboard_core::provider::ProviderId;
use pitboard_core::status::Standing;
use pitboard_core::time::{DAY, MINUTE};
use pitboard_core::usage::whole;
use pitboard_core::words::capitalised;

// A row's note for a saved sign-in due to expire, or a Claude Desktop one that may lapse.
pub(crate) fn sign_in_expires(left: i64, lapses: bool) -> String {
    let days = left / DAY;
    match (lapses, days) {
        (true, _) if left <= 0 => "Sign-in may have expired".into(),
        (_, _) if left <= 0 => "Sign-in expired".into(),
        (true, 0) => "Sign-in may expire today".into(),
        (true, 1) => "Sign-in may expire in 1 day".into(),
        (true, days) => format!("Sign-in may expire in {days} days"),
        (false, 0) => "Sign-in expires today".into(),
        (false, 1) => "Sign-in expires in 1 day".into(),
        (false, days) => format!("Sign-in expires in {days} days"),
    }
}

// The detail's line for a saved sign-in, `until` as the person's calendar says it.
pub(crate) fn saved_until(until: &str, lapses: bool) -> String {
    if lapses {
        format!("Saved sign-in may expire after {until}")
    } else {
        format!("Saved sign-in valid until {until}")
    }
}

// What VoiceOver and Narrator call a switching row's spinner.
pub(crate) const SWITCHING: &str = "Switching";

pub(crate) fn in_use(front: Front) -> String {
    titled(front, "In Use", IN_USE)
}

// The detail's sign-in line, a value among sentences.
pub(crate) const IN_USE: &str = "In use";

/// What a button that switches says: "Switch to spare".
pub(crate) fn switch_to(label: &str) -> String {
    format!("Switch to {label}")
}

/// The menu's one item for everything else to know about, more than one of them.
pub(crate) fn issues(front: Front, count: usize) -> String {
    if count == 1 {
        titled(front, "1 Issue", "1 issue")
    } else {
        titled(
            front,
            &format!("{count} Issues"),
            &format!("{count} issues"),
        )
    }
}

// "work (Codex)", "work (claude.ai)", where nothing around it says which tool or site.
pub(crate) fn named_with_tool(label: &str, tool: &str) -> String {
    format!("{label} ({tool})")
}

// "work" on macOS, whose subtitle says the rest; "work (42%)", "old (sign-in expired)" on Windows
// and in a status item's menu on Linux, whose entries have no subtitle.
pub(crate) fn named_in_menu(front: Front, title: &str, aside: Option<&str>) -> String {
    match (front, aside) {
        (Front::WindowsNotificationArea | Front::Gnome, Some(aside)) => {
            format!("{title} ({})", uncapitalised(aside))
        }
        (Front::WindowsNotificationArea | Front::Gnome, None) | (Front::MacMenuBar, _) => {
            title.to_owned()
        }
    }
}

/// The line in a menu when the numbers were read, as a time rather than an age: a menu can
/// stay open, and "just now" would still say so ten minutes later.
pub(crate) fn updated(clock: &str) -> String {
    format!("Updated {clock}")
}

pub(crate) const READING: &str = "Reading…";

pub(crate) const NOT_READ_YET: &str = "Not read yet";

pub(crate) const SHOWING_LAST_MEASURED: &str = "Showing the last numbers measured";

// A row for a sign-in nobody named: `unusable` where Pitboard cannot use it either.
pub(crate) fn nameless_login(unusable: bool) -> &'static str {
    if unusable {
        "Sign-in Pitboard can’t use"
    } else {
        "Unknown sign-in"
    }
}

// "work, Team 5x, in use", as VoiceOver and Narrator say a row; `sign_in` what stops a switch.
pub(crate) fn row_spoken(
    name: &str,
    plan: Option<&str>,
    in_use: bool,
    sign_in: Option<&str>,
) -> String {
    let mut said = vec![name.to_owned()];
    said.extend(plan.map(str::to_owned));
    if in_use {
        said.push("in use".into());
    }
    said.extend(sign_in.map(uncapitalised));
    said.join(", ")
}

// A Claude Desktop login in use, read last while its app was closed.
pub(crate) fn read_ago(seconds: i64, program: &str) -> String {
    format!("Read {} ago, with {program} closed", span(seconds))
}

// "Switch", and aloud "Switch to work".
pub(crate) fn switch_account(spoken: Option<&str>) -> String {
    match spoken {
        Some(name) => switch_to(name),
        None => SWITCH.into(),
    }
}

// "Add…", and aloud "Add dana@example.com…": it adds at once, then asks for a name.
pub(crate) fn add_account_signed_in(spoken: Option<&str>) -> String {
    match spoken {
        Some(name) => format!("Add {name}…"),
        None => ADD_ASKING.into(),
    }
}

pub(crate) fn rename(asks: bool) -> &'static str {
    if asks { "Rename…" } else { "Rename" }
}

pub(crate) fn remove_account(front: Front) -> String {
    titled(front, "Remove Account…", "Remove account…")
}

pub(crate) const REMOVE: &str = "Remove";

pub(crate) fn remove_title(name: &str) -> String {
    format!("Remove “{name}”?")
}

// The menu's line under an account a switch runs for.
pub(crate) const SWITCHING_NOW: &str = "Switching…";

pub(crate) const CANNOT_READ_OR_SWITCH: &str = "Can’t be read or switched";

pub(crate) const NOT_ADDED_YET: &str = "Not added yet";

// What stops a switch to an account until it is signed in to again, by why.
pub(crate) fn sign_in_standing(standing: Standing) -> &'static str {
    match standing {
        Standing::LoginExpired => "Sign-in expired",
        Standing::NothingParked => "Not signed in",
        _ => "Sign-in no longer works",
    }
}

// "5-hour limit, 42 percent used, resets 3:45 PM": `resets` as the row says it, or nothing.
pub(crate) fn spoken_limit(title: &str, percent: f64, resets: &str) -> String {
    let used = format!("{title}, {} percent used", whole(percent));
    if resets.is_empty() {
        used
    } else {
        format!("{used}, {}", uncapitalised(resets))
    }
}

// "5-hour limit", "Weekly limit (Fable)": `name` as a sentence says it, "weekly".
pub(crate) fn limit_title(name: &str, model: Option<&str>) -> String {
    let title = format!("{} limit", capitalised(name));
    match model {
        Some(model) => format!("{title} ({model})"),
        None => title,
    }
}

// Beside a limit's bar: "5-hour", "Weekly (Fable)".
pub(crate) fn limit_short(name: &str, model: Option<&str>) -> String {
    let short = capitalised(name);
    match model {
        Some(model) => format!("{short} ({model})"),
        None => short,
    }
}

// "42% used", as Anthropic says a limit's figure.
pub(crate) fn used(figure: &str) -> String {
    format!("{figure} used")
}

// "Resets 3:45 PM", "Resets Mon 9:00 AM": `clock` as the person's clock says the moment.
pub(crate) fn resets(clock: &str) -> String {
    format!("Resets {clock}")
}

pub(crate) const RESETTING_NOW: &str = "Resetting now";

// Before the countdown to a reset that a widget draws itself.
pub(crate) const RESETS_IN: &str = "Resets in";

// How long until a reset, to the minute it has not yet reached.
pub(crate) fn resets_in(seconds: i64) -> String {
    format!("in {}", span(seconds))
}

// "2 hr 9 min", "1 day 3 hr", "under a minute".
fn span(seconds: i64) -> String {
    let minutes = seconds.max(0) / MINUTE;
    let (days, hours, rest) = (minutes / (24 * 60), minutes % (24 * 60) / 60, minutes % 60);
    match (days, hours, rest) {
        (0, 0, 0) => "under a minute".into(),
        (0, 0, minutes) => format!("{minutes} min"),
        (0, hours, 0) => format!("{hours} hr"),
        (0, hours, minutes) => format!("{hours} hr {minutes} min"),
        (1, 0, _) => "1 day".into(),
        (1, hours, _) => format!("1 day {hours} hr"),
        (days, 0, _) => format!("{days} days"),
        (days, hours, _) => format!("{days} days {hours} hr"),
    }
}

// "5-hour limit reached, resets 6:46 PM": `title` as a heading names the limit.
pub(crate) fn limit_reached(title: &str, clock: Option<&str>) -> String {
    match clock {
        Some(clock) => format!("{title} reached, resets {clock}"),
        None => format!("{title} reached"),
    }
}

// The menu's line: "5-hour 42% used, Weekly 12%", each limit by its name beside its bar, and
// in sentence case on Windows and GNOME, whose guidelines give it to text that runs on.
pub(crate) fn limits_line(front: Front, limits: &[(String, String)]) -> String {
    let said: Vec<String> = (limits.iter().enumerate())
        .map(|(at, (name, figure))| match (at, front) {
            (0, _) => format!("{name} {}", used(figure)),
            (_, Front::WindowsNotificationArea | Front::Gnome) => {
                format!("{} {figure}", uncapitalised(name))
            }
            (_, Front::MacMenuBar) => format!("{name} {figure}"),
        })
        .collect();
    said.join(", ")
}

pub(crate) const NO_USAGE_READ: &str = "No usage read";

// Why a row's numbers are old or its account can't be used, in the app's words.
pub(crate) fn stale_explanation(row: &pitboard_core::status::Row) -> Option<String> {
    use pitboard_core::status::Stale;
    let tool = row.provider;
    if row.untold()
        && let Some(reason) = &row.reason
    {
        return Some(format!(
            "Pitboard couldn’t tell which account {}’s sign-in is ({reason})",
            tool.name()
        ));
    }
    // Its service's name, for a tool whose service is asked.
    let service = (tool != ProviderId::Desktop).then(|| tool.service());
    let name = tool.name();
    Some(match row.stale? {
        Stale::LoginRefused if row.signed_in && tool == ProviderId::Claude => {
            "Claude Code’s sign-in no longer works".into()
        }
        Stale::NothingSignedIn => "nothing is signed in".into(),
        Stale::LoginUnreadable if service.is_some() => format!("{name}’s sign-in couldn’t be read"),
        Stale::LoginUnusable if service.is_some() => {
            format!("{name}’s sign-in isn’t one Pitboard can switch")
        }
        Stale::SessionExpired if service.is_some() => {
            format!("{name}’s session has expired; using {name} renews it")
        }
        Stale::LoginReplaced if tool == ProviderId::Desktop => {
            "its sign-in was signed out or replaced in Claude".into()
        }
        Stale::LoginReplaced => "its sign-in was replaced by a sign-in outside Pitboard".into(),
        Stale::ParkUnreadable => "its saved sign-in can’t be read".into(),
        Stale::RateLimited => format!("{} is rate limiting usage checks", service?),
        Stale::Unreachable => format!("{} couldn’t be reached", service?),
        Stale::ServerError => format!("{} answered with an error; try again later", service?),
        Stale::Refused => format!("{} refused to answer for this sign-in", service?),
        Stale::OnHold => format!("{} has put this account on hold", service?),
        Stale::AnswerNotUnderstood => format!("{}’s answer wasn’t understood", service?),
        Stale::LoginRefused => "its saved sign-in no longer works".into(),
        Stale::Interrupted => "the check didn’t finish".into(),
        Stale::NotAsked => format!("read without asking {}", service?),
        Stale::NoUsage => "Pitboard reads no usage for Claude Desktop".into(),
        _ => return None,
    })
}

pub(crate) fn already_in_use(label: &str) -> String {
    format!("{label} is already in use.")
}

pub(crate) const NO_SUCH_ACCOUNT: &str = "That account isn’t in Pitboard anymore.";

pub(crate) fn sign_in_first(label: &str) -> String {
    format!("Sign in to {label} again in Pitboard to switch to it.")
}

pub(crate) fn cannot_switch_to(label: &str) -> String {
    format!("Pitboard can’t switch to {label}.")
}

pub(crate) const SWITCH_UNDER_WAY: &str = "Pitboard is busy switching or signing in. Try again in a \
                                           moment.";

// A question only the app can ask.
pub(crate) fn open_pitboard_to_switch(app: &str) -> String {
    format!("Open Pitboard to switch while {app} is open.")
}

pub(crate) fn open_pitboard_to_replace(label: &str) -> String {
    format!("Open Pitboard to switch to {label}.")
}

pub(crate) const QUESTION_OPEN: &str = "Answer Pitboard’s question first, then try again.";

pub(crate) const BEING_UNINSTALLED: &str = "Pitboard is being uninstalled.";

pub(crate) const NONE_IN_USE: &str = "No account is in use.";

// A widget's or a control's switch where only the app can switch.
pub(crate) const OPEN_TO_SWITCH: &str = "Open Pitboard to switch accounts.";

// `title` names what failed; the app says why, since the core's words can name an email or a path.
pub(crate) fn failed_outside(title: &str) -> String {
    format!("{title}. Open Pitboard to try again.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::present::Severity;
    use crate::present::words::{
        PITBOARD, UNNAMED, WARM_UP_OFF, WARMING_UP, bar_spoken, bar_tip,
        could_not_switch_from_outside, in_bar, open_site, open_site_as, spoken_severity, utc_date,
    };
    use pitboard_core::time::HOUR;

    #[test]
    fn a_saved_sign_in_says_when_it_expires() {
        assert_eq!(
            sign_in_expires(2 * DAY + 60, false),
            "Sign-in expires in 2 days"
        );
        assert_eq!(sign_in_expires(DAY, false), "Sign-in expires in 1 day");
        assert_eq!(sign_in_expires(HOUR, false), "Sign-in expires today");
        assert_eq!(sign_in_expires(0, false), "Sign-in expired");
        assert_eq!(
            sign_in_expires(3 * DAY, true),
            "Sign-in may expire in 3 days"
        );
        assert_eq!(sign_in_expires(-1, true), "Sign-in may have expired");
        assert_eq!(
            saved_until("Nov 20", false),
            "Saved sign-in valid until Nov 20"
        );
        assert_eq!(
            saved_until("Nov 20", true),
            "Saved sign-in may expire after Nov 20"
        );
        assert_eq!(utc_date(1_795_132_800), "Nov 20 UTC");
    }

    #[test]
    fn a_limit_is_spoken_as_one_sentence() {
        assert_eq!(
            spoken_limit("5-hour limit", 42.0, &resets("3:45 PM")),
            "5-hour limit, 42 percent used, resets 3:45 PM"
        );
        assert_eq!(
            spoken_limit("30-minute limit", 12.0, ""),
            "30-minute limit, 12 percent used"
        );
        assert_eq!(
            spoken_limit("Weekly limit (Fable)", 98.0, RESETTING_NOW),
            "Weekly limit (Fable), 98 percent used, resetting now"
        );
    }

    // Counted down to the minute, so it never says a reset has come before it has.
    #[test]
    fn a_reset_is_counted_down_in_hours_and_minutes() {
        for (seconds, said) in [
            (0, "in under a minute"),
            (59, "in under a minute"),
            (60, "in 1 min"),
            (47 * 60 + 59, "in 47 min"),
            (3_600, "in 1 hr"),
            (2 * 3_600 + 9 * 60 + 59, "in 2 hr 9 min"),
            (19 * 3_600 + 24 * 60, "in 19 hr 24 min"),
            (86_400 + 59 * 60, "in 1 day"),
            (86_400 + 3 * 3_600, "in 1 day 3 hr"),
            (6 * 86_400 + 23 * 3_600 + 59 * 60, "in 6 days 23 hr"),
            (7 * 86_400, "in 7 days"),
        ] {
            assert_eq!(resets_in(seconds), said, "{seconds}");
        }
    }

    // The menu bar's, the menu's and a row's words, pinned once.
    #[test]
    fn an_account_is_said_in_these_words() {
        assert_eq!(PITBOARD, "Pitboard");
        assert_eq!(UNNAMED, "unnamed");
        assert_eq!(in_bar("work", "42%"), "work 42%");
        assert_eq!(bar_spoken(None, None, false), "Pitboard");
        assert_eq!(
            bar_spoken(Some("work 42%"), None, false),
            "Pitboard, work 42%"
        );
        assert_eq!(
            bar_spoken(Some("work 42%"), Some("Codex"), true),
            "Pitboard, work 42%, Codex, needs attention"
        );
        assert_eq!(
            bar_spoken(None, Some("Codex"), true),
            "Pitboard, needs attention"
        );
        assert_eq!(bar_tip(false, []), "Pitboard");
        assert_eq!(
            bar_tip(false, [("work".into(), "5-hour 42% used".into())]),
            "Pitboard\nwork: 5-hour 42% used"
        );
        assert_eq!(
            bar_tip(
                true,
                [
                    ("work".into(), "5-hour 12%".into()),
                    ("main".into(), "Weekly 18% used".into())
                ]
            ),
            "Pitboard\nNeeds attention\nwork: 5-hour 12%\nmain: weekly 18% used"
        );
        assert_eq!(nameless_login(true), "Sign-in Pitboard can’t use");
        assert_eq!(nameless_login(false), "Unknown sign-in");
        assert_eq!(row_spoken("work", None, false, None), "work");
        assert_eq!(row_spoken("work", None, true, None), "work, in use");
        assert_eq!(
            row_spoken("work", Some("Team 5x"), true, None),
            "work, Team 5x, in use"
        );
        assert_eq!(
            row_spoken("stale", None, false, Some("Sign-in expired")),
            "stale, sign-in expired"
        );
        assert_eq!(
            read_ago(2 * 3_600 + 5 * 60, "Claude"),
            "Read 2 hr 5 min ago, with Claude closed"
        );
        assert_eq!(
            read_ago(30, "Claude"),
            "Read under a minute ago, with Claude closed"
        );
        assert_eq!(open_site("claude.ai"), "Open claude.ai");
        assert_eq!(open_site_as("claude.ai", "work"), "Open claude.ai as work");
        assert_eq!(switch_account(None), "Switch");
        assert_eq!(switch_account(Some("spare")), "Switch to spare");
        assert_eq!(add_account_signed_in(None), "Add…");
        assert_eq!(
            add_account_signed_in(Some("u@example.com")),
            "Add u@example.com…"
        );
        assert_eq!((rename(true), rename(false)), ("Rename…", "Rename"));
        assert_eq!(
            (
                remove_account(Front::MacMenuBar),
                remove_account(Front::WindowsNotificationArea)
            ),
            ("Remove Account…".into(), "Remove account…".into())
        );
        assert_eq!(REMOVE, "Remove");
        assert_eq!(remove_title("spare"), "Remove “spare”?");
        assert_eq!(WARMING_UP, "Warming up…");
        assert_eq!(SWITCHING_NOW, "Switching…");
        assert_eq!(CANNOT_READ_OR_SWITCH, "Can’t be read or switched");
        assert_eq!(NOT_ADDED_YET, "Not added yet");
        assert_eq!(sign_in_standing(Standing::LoginExpired), "Sign-in expired");
        assert_eq!(
            sign_in_standing(Standing::LoginRefused),
            "Sign-in no longer works"
        );
        assert_eq!(
            sign_in_standing(Standing::LoginReplaced),
            "Sign-in no longer works"
        );
        assert_eq!(sign_in_standing(Standing::NothingParked), "Not signed in");
        assert_eq!(NO_USAGE_READ, "No usage read");
        assert_eq!(limit_title("5-hour", None), "5-hour limit");
        assert_eq!(limit_title("weekly", Some("Fable")), "Weekly limit (Fable)");
        assert_eq!(limit_short("5-hour", None), "5-hour");
        assert_eq!(limit_short("weekly", Some("Fable")), "Weekly (Fable)");
        assert_eq!(used("42%"), "42% used");
        assert_eq!(resets("3:45 PM"), "Resets 3:45 PM");
        assert_eq!(resets("Mon 9:00 AM"), "Resets Mon 9:00 AM");
        assert_eq!(RESETTING_NOW, "Resetting now");
        assert_eq!(RESETS_IN, "Resets in");
        assert_eq!(NONE_IN_USE, "No account is in use.");
        assert_eq!(OPEN_TO_SWITCH, "Open Pitboard to switch accounts.");
        assert_eq!(WARM_UP_OFF, "Off");
        assert_eq!(
            limit_reached("5-hour limit", Some("6:46 PM")),
            "5-hour limit reached, resets 6:46 PM"
        );
        assert_eq!(limit_reached("Weekly limit", None), "Weekly limit reached");
        let limits = [
            ("5-hour".to_owned(), "42%".to_owned()),
            ("Weekly".to_owned(), "12%".to_owned()),
            ("Weekly (Fable)".to_owned(), "3%".to_owned()),
        ];
        assert_eq!(
            limits_line(Front::MacMenuBar, &limits),
            "5-hour 42% used, Weekly 12%, Weekly (Fable) 3%"
        );
        assert_eq!(
            limits_line(Front::WindowsNotificationArea, &limits),
            "5-hour 42% used, weekly 12%, weekly (Fable) 3%"
        );
        assert_eq!(
            limits_line(Front::WindowsNotificationArea, &limits[1..2]),
            "Weekly 12% used"
        );
        assert_eq!(
            (
                in_use(Front::MacMenuBar),
                in_use(Front::WindowsNotificationArea)
            ),
            ("In Use".into(), "In use".into())
        );
        assert_eq!(SWITCHING, "Switching");
        assert_eq!(named_with_tool("work", "Codex"), "work (Codex)");
        assert_eq!(named_with_tool("work", "claude.ai"), "work (claude.ai)");
        assert_eq!(
            named_in_menu(Front::MacMenuBar, "work", Some("64%")),
            "work"
        );
        assert_eq!(
            named_in_menu(Front::WindowsNotificationArea, "work", Some("64%")),
            "work (64%)"
        );
        assert_eq!(
            named_in_menu(
                Front::WindowsNotificationArea,
                "old",
                Some("sign-in expired")
            ),
            "old (sign-in expired)"
        );
        assert_eq!(
            named_in_menu(Front::WindowsNotificationArea, "spare", None),
            "spare"
        );
        assert_eq!(
            limits_line(Front::Gnome, &limits),
            "5-hour 42% used, weekly 12%, weekly (Fable) 3%",
            "text that runs on, in GNOME's sentence case"
        );
        assert_eq!(
            named_in_menu(Front::Gnome, "work", Some("64%")),
            "work (64%)",
            "a status item's menu entry has no subtitle on Linux"
        );
        assert_eq!(named_in_menu(Front::Gnome, "spare", None), "spare");
        assert_eq!(in_use(Front::Gnome), "In Use");
        assert_eq!(updated("12:00"), "Updated 12:00");
        assert_eq!(READING, "Reading…");
        assert_eq!(NOT_READ_YET, "Not read yet");
        assert_eq!(SHOWING_LAST_MEASURED, "Showing the last numbers measured");
        assert_eq!(switch_to("spare"), "Switch to spare");
        assert_eq!(
            (
                spoken_severity(Severity::Info),
                spoken_severity(Severity::Warning)
            ),
            ("Note", "Warning")
        );
        assert_eq!(spoken_severity(Severity::Error), "Problem");
    }

    #[test]
    fn a_request_from_outside_is_answered_in_these_words() {
        assert_eq!(already_in_use("work"), "work is already in use.");
        assert_eq!(NO_SUCH_ACCOUNT, "That account isn’t in Pitboard anymore.");
        assert_eq!(
            sign_in_first("work"),
            "Sign in to work again in Pitboard to switch to it."
        );
        assert_eq!(cannot_switch_to("work"), "Pitboard can’t switch to work.");
        assert_eq!(
            SWITCH_UNDER_WAY,
            "Pitboard is busy switching or signing in. Try again in a moment."
        );
        assert_eq!(
            open_pitboard_to_switch("ChatGPT"),
            "Open Pitboard to switch while ChatGPT is open."
        );
        assert_eq!(
            open_pitboard_to_replace("work"),
            "Open Pitboard to switch to work."
        );
        assert_eq!(
            (
                could_not_switch_from_outside(Front::MacMenuBar),
                could_not_switch_from_outside(Front::WindowsNotificationArea)
            ),
            ("Couldn’t Switch".into(), "Couldn’t switch".into())
        );
    }
}
