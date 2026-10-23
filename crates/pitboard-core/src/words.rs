//! What Pitboard says in more than one place, each written once as a function of typed
//! values: its sentences, the words of its columns, and the level a limit's usage is at,
//! where the command line's colours and the app's tints change. The command line calls
//! these functions directly, and `pitboard-ffi`'s model calls them as it makes what an app
//! shows, so where both say a thing they say it in the same words. The bindings export none
//! of them: an app shows what the snapshot says. A thing said both in a column and in a
//! sentence, such as a limit's name or a parked login's life, has a function for each form.
//!
//! All of it is English. Clock times are not here: the command line writes them with
//! `time::moment`, and the app in the format its Mac is set to.

use crate::doctor::{Level, renewal_due};
use crate::host::{Os, WINDOWS_FLOOR, token};
use crate::schedule::Job;
use crate::time::{DAY, HOUR, MINUTE, WEEK};
use crate::usage::pace::{Pace, Standing};
use crate::usage::whole;
use std::fmt::Write as _;

/// A length of time to the precision a person reads: "6d 4h", "2h 05m", "47m", "<1m".
///
/// Minutes beside hours take two digits, so a column of them lines up as they tick. Under a
/// minute is "<1m": "0m" reads as though nothing were left when the difference is seconds
/// either way.
pub fn span(seconds: i64) -> String {
    let s = seconds.max(0);
    let (days, hours, minutes) = (s / DAY, s % DAY / HOUR, s % HOUR / MINUTE);
    match (days, hours) {
        (0, 0) if minutes == 0 => "<1m".into(),
        (0, 0) => format!("{minutes}m"),
        (0, h) => format!("{h}h {minutes:02}m"),
        (d, h) => format!("{d}d {h}h"),
    }
}

/// A limit in the column form, beside its bar: "5h", "week", "day", "3d", "30m", "45s", and
/// "week · Fable" for one scoped to a model.
///
/// By how long it runs, where that is known, and otherwise by its kind. The length is what
/// makes two tools' limits comparable: OpenAI times every window and names none, Anthropic
/// names every window and times none, and Pitboard knows the length of each either way. A
/// kind is only the fallback, for a reading remembered from before the length was kept.
pub fn limit_column(kind: &str, length_seconds: Option<i64>, scope: Option<&str>) -> String {
    let base = match length_seconds.filter(|s| *s > 0) {
        Some(WEEK) => "week".into(),
        Some(DAY) => "day".into(),
        Some(s) if s % DAY == 0 => format!("{}d", s / DAY),
        Some(s) if s % HOUR == 0 => format!("{}h", s / HOUR),
        Some(s) if s % MINUTE == 0 => format!("{}m", s / MINUTE),
        Some(s) => format!("{s}s"),
        None => match kind {
            "session" | "five_hour" => "5h".into(),
            "weekly_all" | "seven_day" | "weekly_scoped" => "week".into(),
            other => other.into(),
        },
    };
    match scope {
        Some(scope) => format!("{base} · {scope}"),
        None => base,
    }
}

/// A limit in the sentence form: "5-hour", "weekly", "daily", "3-day", "30-minute".
///
/// From its length where that is known, as [`limit_column`] is: "session" means nothing to
/// somebody reading about a Codex account. Without its scope, which the sentence places
/// itself, as in "weekly Fable 98%".
pub fn limit_name(kind: &str, length_seconds: Option<i64>) -> String {
    match length_seconds.filter(|s| *s > 0) {
        Some(WEEK) => "weekly".into(),
        Some(DAY) => "daily".into(),
        Some(s) if s % DAY == 0 => format!("{}-day", s / DAY),
        Some(s) if s % HOUR == 0 => format!("{}-hour", s / HOUR),
        Some(s) if s % MINUTE == 0 => format!("{}-minute", s / MINUTE),
        Some(s) => format!("{s}-second"),
        None => match kind {
            "session" | "five_hour" => "5-hour".into(),
            "seven_day" => "weekly".into(),
            weekly if weekly.starts_with("weekly") => "weekly".into(),
            other => other.replace('_', " "),
        },
    }
}

/// A share of a limit as every front end draws it: "64%", a half rounded up.
pub fn figure(percent: f64) -> String {
    format!("{}%", whole(percent))
}

/// How much of a limit an account has used, in a sentence about an automatic switch:
/// "96% of its 5-hour limit", or "97% of its weekly Opus limit" for one limit of a model.
pub fn share_of_limit(limit: &crate::usage::Window) -> String {
    let name = scoped_limit_name(&limit.kind, limit.length_seconds, limit.scope.as_deref());
    format!("{} of its {name} limit", figure(limit.percent))
}

/// How much of a limit `account` has used, as every sentence about a limit at the share starts:
/// "work has used 96% of its 5-hour limit". `used` is [`share_of_limit`].
pub fn has_used(account: &str, used: &str) -> String {
    format!("{account} has used {used}")
}

/// An attempt to switch away from `from` came to nothing a moment ago, and the next is made at
/// `until`, written as each front end writes a clock time.
pub fn auto_waiting(from: &str, used: &str, until: &str) -> String {
    format!("{}. Pitboard tries again at {until}.", has_used(from, used))
}

/// Claude Code was switched by itself from `from` to `to`.
pub fn auto_switched(from: &str, to: &str, used: &str) -> String {
    format!(
        "Switched Claude Code from {from} to {to}: {}.",
        has_used(from, used)
    )
}

/// Pitboard does not switch away from a limit of `from` at the share, for `why`, a clause from
/// [`not_switching`], and decides again at `until` where it waits to.
pub fn auto_skipped(from: &str, used: &str, why: &str, until: Option<&str>) -> String {
    let mut said = format!(
        "{}. Pitboard is not switching: {why}.",
        has_used(from, used)
    );
    if let Some(until) = until {
        let _ = write!(said, " It decides again at {until}.");
    }
    said
}

/// Pitboard cannot judge whether to switch Claude Code, for `why`, a clause from
/// [`not_watching`], and asks again at `until` where it waits to.
pub fn auto_not_watching(why: &str, until: Option<&str>) -> String {
    let mut said = format!("Pitboard is not switching Claude Code: {why}.");
    if let Some(until) = until {
        let _ = write!(said, " It asks again at {until}.");
    }
    said
}

/// What the automatic switch watches where it has nothing to do: "Watching work: 62% of its
/// 5-hour limit, as of 16:40." `used` is [`share_of_limit`] of its fullest limit, and the
/// clock times come as each front end writes them: `as_of`, when that was read, and
/// `held_until`, until when Anthropic holds Pitboard off asking about the account.
pub fn watching(
    account: &str,
    used: Option<&str>,
    as_of: Option<&str>,
    held_until: Option<&str>,
) -> String {
    let mut said = format!("Watching {account}");
    if let Some(used) = used {
        let _ = write!(said, ": {used}");
    }
    if let Some(as_of) = as_of {
        let _ = write!(said, ", as of {as_of}");
    }
    said.push('.');
    if let Some(until) = held_until {
        let _ = write!(
            said,
            " Anthropic holds Pitboard off asking again until {until}."
        );
    }
    said
}

/// Why Pitboard does not switch Claude Code away from a limit at `threshold`, as a clause
/// that says what to do about it where anything can be done. It names no command, and no
/// time, which each front end says in its own way: the app says it too.
pub fn not_switching(
    why: &crate::duty::autoswitch::Skip,
    threshold: crate::duty::autoswitch::Threshold,
) -> String {
    use crate::duty::autoswitch::{SETTLING_SECONDS, Skip};
    use crate::duty::cadence::ATTEMPTS;
    match why {
        Skip::NoRoom { unread } => {
            let room = format!(
                "no other Claude Code account has room below {}% in every limit",
                threshold.percent()
            );
            if unread.is_empty() {
                room
            } else {
                format!(
                    "{room}, and there is no reading of {} from Anthropic yet",
                    listed(unread.clone())
                )
            }
        }
        Skip::AlreadyLeft => {
            "this limit was switched away from once already, and is not again before it resets"
                .into()
        }
        Skip::GaveUp => format!(
            "{ATTEMPTS} attempts to switch away from this limit failed, and none is made again \
             before it resets"
        ),
        Skip::Settling { .. } => format!(
            "the account was put in use less than {} minutes ago",
            SETTLING_SECONDS / 60
        ),
        Skip::Overridden(names) => format!(
            "Claude Code signs in another way, set by {}, so a switch would change nothing its \
             sessions use",
            names.join(", ")
        ),
    }
}

const INTERRUPTED: &str = "a switch was interrupted, and the next change you make finishes it";
const CUSTOM_OAUTH: &str =
    "Claude Code uses a custom OAuth endpoint, and Pitboard does not act on Claude Code then";

/// Why Pitboard cannot judge whether to switch Claude Code by itself, as a clause that says
/// what to do about it where anything can be done. It names no command, and no time, which
/// each front end says in its own way.
pub fn not_watching(why: &crate::duty::autoswitch::Blind) -> String {
    use crate::duty::autoswitch::Blind;
    match why {
        Blind::SwitchInterrupted => INTERRUPTED.into(),
        Blind::CustomOauth => CUSTOM_OAUTH.into(),
        Blind::NothingSignedIn => "Claude Code has no login stored".into(),
        Blind::NotEnrolled { email } => {
            format!("Claude Code has {email}'s login stored, and that account is not enrolled")
        }
        Blind::Unidentified { detail, .. } => {
            format!("whose login Claude Code has stored could not be told ({detail})")
        }
        Blind::NoReading { account } => {
            format!("there is no reading of {account} from Anthropic yet")
        }
    }
}

/// Why Pitboard warms up no account on this machine for now, as a clause.
pub fn not_warming(why: &crate::duty::warm::Halt) -> String {
    use crate::duty::warm::Halt;
    match why {
        Halt::SwitchInterrupted => INTERRUPTED.into(),
        Halt::CustomOauth => CUSTOM_OAUTH.into(),
        Halt::Overridden(names) => format!(
            "Claude Code would sign a warm-up in another way, set by {}",
            names.join(", ")
        ),
        Halt::ProgramMissing => "Claude Code is not installed where Pitboard looks".into(),
    }
}

/// That Pitboard warms up no account on this machine for now, and why.
pub fn warming_halted(why: &crate::duty::warm::Halt) -> String {
    format!("Pitboard is not warming up accounts: {}.", not_warming(why))
}

/// Why a warm-up started nothing, as a clause.
pub(crate) fn warm_failed(why: &crate::duty::warm::Failure) -> String {
    use crate::duty::warm::Failure;
    match why {
        Failure::LimitReached => {
            "a limit of it is used up, and Anthropic refused the request".into()
        }
        Failure::LoginRefused => "Anthropic refused its login; sign in to it again".into(),
        Failure::NotStarted => {
            "the request was answered, and Anthropic still has no period running".into()
        }
        Failure::Unreachable => "Anthropic could not be reached".into(),
        Failure::TimedOut => "Claude Code did not finish in time".into(),
        Failure::Failed(said) => format!("Claude Code failed ({said})"),
    }
}

/// Where an account turned on for warm-ups stands, as what follows its name: "work warms up
/// again at 22:50", "work has its weekly limit used up until Oct 15 03:00". Each moment is
/// written as `moment` writes it, the command line's way or the app's.
pub fn warm_standing(
    standing: &crate::duty::warm::Standing,
    moment: &dyn Fn(i64) -> String,
) -> String {
    use crate::duty::warm::{Hold, Standing};
    match standing {
        Standing::Due => "warms up now".into(),
        Standing::Running {
            until,
            next: Some(next),
        } if next == until => format!("warms up again at {}", moment(*next)),
        Standing::Running {
            until,
            next: Some(next),
        } => format!(
            "has its 5-hour period running until {}, and warms up again at {}",
            moment(*until),
            moment(*next)
        ),
        Standing::Running { until, next: None } => {
            format!("has its 5-hour period running until {}", moment(*until))
        }
        Standing::Resting { at } => format!("warms up at {}", moment(*at)),
        Standing::Waiting { until } => format!(
            "tries again at {}, after a warm-up came to nothing",
            moment(*until)
        ),
        Standing::Held(Hold::NoReading) => "waits for a reading from Anthropic".into(),
        Standing::Held(Hold::NothingToStart) => "has no 5-hour limit to start".into(),
        Standing::Held(Hold::LimitUsedUp { limit }) => {
            let name = scoped_limit_name(&limit.kind, limit.length_seconds, limit.scope.as_deref());
            match limit.resets_at {
                Some(at) => format!("has its {name} limit used up until {}", moment(at)),
                None => format!("has its {name} limit used up"),
            }
        }
        Standing::Held(Hold::GaveUp { until }) => format!(
            "tries again at {}, after {} warm-ups came to nothing today",
            moment(*until),
            crate::duty::cadence::ATTEMPTS
        ),
        Standing::Held(Hold::LoginExpired) => {
            "needs signing in again: its parked login has expired".into()
        }
        Standing::Held(Hold::NothingParked) => {
            "has no login to warm up with: it is not in use, and nothing is parked".into()
        }
    }
}

/// What sessions already running do after a switch while a file sits behind the store they
/// read, as the register's `fallback_file_pins_session_login` reads Claude Code 2.1.294. The
/// rest of the sentence, and its subject, are each place's own.
pub fn kept_until_renewed() -> &'static str {
    "keep the account they are on until their login is next renewed, or until they are \
     started again"
}

/// What becomes of `sessions` already running once a switch is made, as a sentence with no
/// full stop: "Claude Code sessions already running follow within 33 seconds".
pub fn after_a_switch(adoption: &crate::provider::Adoption, sessions: &str) -> String {
    use crate::provider::Adoption;
    match adoption {
        Adoption::PollingWithin(seconds) => {
            format!("{sessions} already running follow within {seconds} seconds")
        }
        Adoption::AtRenewal { file } => format!(
            "{sessions} already running {}: {} is there",
            kept_until_renewed(),
            file.display()
        ),
        Adoption::RestartRequired { program, .. } => {
            format!("Restart any running `{program}` for this to take effect")
        }
        Adoption::NextOpen { app } => format!("{app} opens signed in to it"),
    }
}

/// What is said where nothing is left behind the store Claude Code keeps its login in, as
/// `pitboard stow` and the app's sheet say it. On Linux the file is that store.
pub fn nothing_left() -> &'static str {
    "Nothing is left behind the store Claude Code keeps its login in, so there is nothing to \
     put away."
}

pub fn store_in_front(os: Os) -> &'static str {
    match os {
        Os::MacOs | Os::Linux => "the keychain",
        Os::Windows => "Credential Manager",
    }
}

// What a session cannot do that signs in with the file behind the store a switch writes.
pub(crate) fn out_of_reach(os: Os) -> &'static str {
    match os {
        Os::MacOs | Os::Linux => "cannot read the keychain",
        Os::Windows => "cannot reach Credential Manager",
    }
}

// A network logon, which has no Credential Manager (MC-net).
pub(crate) const SSH_SIGN_IN: &str = "a sign-in over SSH with a key";

// Said where switching by itself and warming up are turned on, in each front end, with the link.
pub const TERMS_MAY_COVER_THIS: &str =
    "Anthropic's terms may cover automatic switching and warm-ups";
pub const IS_THIS_ALLOWED_URL: &str =
    "https://docs.usepitboard.com/concepts/limits#is-this-allowed";

pub(crate) fn past_credential_manager(bytes: usize, limit: usize) -> String {
    format!("{bytes} bytes, past the {limit} Claude Code keeps in Credential Manager")
}

pub(crate) fn reads_only_the_file(os: Os) -> String {
    let such_as = match os {
        Os::MacOs | Os::Linux => "one started over SSH".to_owned(),
        Os::Windows => format!("one in {SSH_SIGN_IN}"),
    };
    format!("a session that {}, such as {such_as}", out_of_reach(os))
}

pub fn left_lines(os: Os, left: &crate::switch::Left) -> Vec<String> {
    use crate::switch::{Foreseen, Kept};
    if let Foreseen::NotEnrolled(_) = left.login {
        return vec![left_login(left)];
    }
    let held_a_login = left.login != Foreseen::Kept(Kept::NoLogin);
    std::iter::once(left_login(left))
        .chain(dropped_keys(os, &left.dropped, false))
        .chain(std::iter::once(once_stowed(os, held_a_login)))
        .collect()
}

pub fn stowed_lines(os: Os, stowed: &crate::switch::Stowed) -> Vec<String> {
    let held_a_login = stowed.kept != crate::switch::Kept::NoLogin;
    std::iter::once(stowed_login(stowed))
        .chain(dropped_keys(os, &stowed.dropped, true))
        .chain(std::iter::once(once_stowed(os, held_a_login)))
        .collect()
}

/// What the file holds, and what putting it away would do with the login in it.
fn left_login(left: &crate::switch::Left) -> String {
    use crate::switch::{Foreseen, Kept};
    let path = left.path.display();
    match &left.login {
        Foreseen::Kept(Kept::NoLogin) => {
            format!("{path} holds no Claude Code login. Putting it away deletes it.")
        }
        Foreseen::Kept(Kept::Refused) => format!(
            "{path} holds a Claude Code login Anthropic no longer accepts. Putting it away \
             deletes it."
        ),
        Foreseen::Kept(Kept::Stored { label }) => format!(
            "{path} holds the login Claude Code has stored{}. Putting it away deletes the file, \
             and that login stays where it is.",
            of_account(label.as_deref())
        ),
        Foreseen::Kept(Kept::AlreadyParked { label }) => format!(
            "{path} holds the login Pitboard keeps parked for `{label}`. Putting it away deletes \
             the file, and the parked login stays."
        ),
        Foreseen::Kept(Kept::SecondSignIn { label }) => format!(
            "{path} holds another login of `{label}`, the account in use, whose own login Claude \
             Code has stored. Putting it away drops this second sign-in with the file."
        ),
        Foreseen::Kept(Kept::ParkedNow { label }) => format!(
            "{path} holds `{label}`'s login, and Pitboard holds no parked login of `{label}` to \
             keep in its place. Putting it away parks this one for `{label}`, then deletes the \
             file."
        ),
        Foreseen::Kept(Kept::ParkKept { label }) => format!(
            "{path} holds another login of `{label}`, which keeps the login Pitboard has parked \
             for it. Putting it away deletes the file."
        ),
        Foreseen::NotEnrolled(owner) => format!(
            "{path} holds {}'s login{}, an account not enrolled here, so Pitboard cannot put it \
             away. Enrol that account first, with `pitboard enroll <label> --sign-in`, or with \
             `pitboard enroll <label>` while Claude Code is signed in to it.",
            owner.email,
            in_organisation(&owner.organization_uuid)
        ),
        Foreseen::Untold => format!(
            "{path} holds a Claude Code login whose access token has expired. Putting it away \
             renews it and writes it back to the file first, then asks Anthropic whose it is, \
             and goes on only for an account enrolled here."
        ),
    }
}

/// What putting the file away did with the login it held.
fn stowed_login(stowed: &crate::switch::Stowed) -> String {
    use crate::switch::Kept;
    let path = stowed.path.display();
    match &stowed.kept {
        Kept::NoLogin => format!("Deleted {path}, which held no Claude Code login."),
        Kept::Refused => format!(
            "Deleted {path}: Anthropic no longer accepts the login it held, so that login was \
             no longer valid."
        ),
        Kept::Stored { label } => format!(
            "Deleted {path}, which held the login Claude Code has stored{}. That login stays \
             where it is.",
            of_account(label.as_deref())
        ),
        Kept::AlreadyParked { label } => {
            format!("Deleted {path}, which held the login Pitboard keeps parked for `{label}`.")
        }
        Kept::SecondSignIn { label } => format!(
            "Deleted {path}, which held a second sign-in of `{label}`, the account in use. That \
             login was dropped; the one Claude Code has stored stays."
        ),
        Kept::ParkedNow { label } => {
            format!("Parked the login {path} held for `{label}`, then deleted the file.")
        }
        Kept::ParkKept { label } => format!(
            "Deleted {path}, which held another login of `{label}`. `{label}` keeps the login \
             Pitboard had parked for it."
        ),
    }
}

fn dropped_keys(os: Os, keys: &[String], gone: bool) -> Option<String> {
    let one = match keys.len() {
        0 => return None,
        n => n == 1,
    };
    let names = listed(keys.iter().map(|key| format!("`{key}`")));
    let (holds, goes) = match (gone, one) {
        (false, true) => ("holds", "goes"),
        (false, false) => ("holds", "go"),
        (true, _) => ("held", "went"),
    };
    let (count, them) = if one {
        ("1 other key".to_string(), "it")
    } else {
        (format!("{} other keys", keys.len()), "them")
    };
    Some(format!(
        "It also {holds} {count}, {names}, which {goes} with the file: Pitboard does not move \
         {them}, and Claude Code's document in {} keeps its own.",
        store_in_front(os)
    ))
}

fn once_stowed(os: Os, held_a_login: bool) -> String {
    let follow = format!(
        "Once the file is gone, Claude Code sessions already running follow a switch within \
         {} seconds again",
        crate::switch::ADOPTION_CEILING_SECONDS
    );
    let over_ssh = match os {
        Os::MacOs | Os::Linux => "one over SSH".to_owned(),
        Os::Windows => format!("one in {SSH_SIGN_IN}"),
    };
    if held_a_login {
        format!(
            "{follow}, and one that signed in with its login, such as {over_ssh}, is signed out."
        )
    } else {
        format!("{follow}.")
    }
}

/// `label`'s, after a login that is an enrolled account's, where it is one.
fn of_account(label: Option<&str>) -> String {
    label.map_or_else(String::new, |label| format!(", `{label}`'s"))
}

/// The organisation a Claude Code login is of, as a sentence names it after its email: none
/// for an account outside one, which Anthropic reports as an empty id.
pub(crate) fn in_organisation(organization: &str) -> String {
    if organization.is_empty() {
        String::new()
    } else {
        format!(", in organisation {organization}")
    }
}

/// How much of a limit is used, in three steps. The command line's colours and the app's
/// tints change where these do, and the words always say the number itself, because not
/// everybody sees a colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageLevel {
    /// Under 70%.
    Plenty,
    /// From 70%.
    Low,
    /// From 90%, and past 100%.
    Out,
}

/// The step a limit is at, from the share of it used: `percent` as a reading gives it, which
/// passes 100 when a service reports more used than the limit. Taken as its figure is
/// drawn, so a limit drawn at 90% is in the colour of 90%.
pub fn usage_level(percent: f64) -> UsageLevel {
    match whole(percent) {
        90.. => UsageLevel::Out,
        70.. => UsageLevel::Low,
        _ => UsageLevel::Plenty,
    }
}

/// When a limit resets, beside its bar: "resets in 2h 05m", and "resetting now" once that
/// moment has come. Whether it did reset is the next reading's to say, and until then the
/// figure beside it is the last one measured.
pub fn resets(at: i64, now: i64) -> String {
    if at <= now {
        "resetting now".into()
    } else {
        format!("resets in {}", span(at - now))
    }
}

/// How far a limit is from an even pace, beside its bar: "27% over pace", "13% under
/// pace", "on pace". Over and under rather than ahead and behind, since ahead reads as good
/// news and is the warning.
pub fn pace_column(pace: &Pace) -> String {
    let points = whole(pace.delta.abs());
    match pace.standing {
        Standing::Over { .. } => format!("{points}% over pace"),
        Standing::Under => format!("{points}% under pace"),
        Standing::Even => "on pace".into(),
    }
}

/// A limit's name in a sentence with its model, where it is scoped to one: "weekly",
/// "weekly Fable", "5-hour".
pub fn scoped_limit_name(kind: &str, length_seconds: Option<i64>, scope: Option<&str>) -> String {
    let name = limit_name(kind, length_seconds);
    match scope {
        Some(scope) => format!("{name} {scope}"),
        None => name,
    }
}

/// The limit of an account that runs out first at its pace, named as a sentence names it:
/// "weekly limit runs out in 19h 05m at this pace". Under a minute it is "about to run
/// out", which reads more plainly than "<1m".
pub fn runs_out(limit: &str, in_seconds: i64) -> String {
    if in_seconds < MINUTE {
        format!("{limit} limit is about to run out")
    } else {
        format!(
            "{limit} limit runs out in {} at this pace",
            span(in_seconds)
        )
    }
}

/// How long a parked login stays usable, in the sentence form: a note under an account not
/// in use, counted in whole days. Without it nothing says a switch to the account is about
/// to stop working. Nothing when the login says nothing about it.
pub fn parked_life(refresh_expires_at: Option<i64>, now: i64) -> Option<String> {
    let left = refresh_expires_at? - now;
    Some(match left / DAY {
        _ if left <= 0 => "Its parked login has expired".into(),
        0 => "Parked login good for under a day".into(),
        1 => "Parked login good for 1 more day".into(),
        days => format!("Parked login good for {days} more days"),
    })
}

/// How long a parked login stays usable, in the column form `pitboard status` puts after
/// "ready" and doctor's parked login check after words of its own: "good for 20d 0h";
/// "expires in 2d 4h" once it is due to be renewed, as [`renewal_due`] decides; and
/// "expired" once it has, where doctor adds when and `pitboard status` says "login expired"
/// in place of "ready".
pub fn parked_life_column(refresh_expires_at: i64, now: i64) -> String {
    match refresh_expires_at - now {
        left if left <= 0 => "expired".into(),
        left if renewal_due(refresh_expires_at, now) => format!("expires in {}", span(left)),
        left => format!("good for {}", span(left)),
    }
}

/// What a renewal run did: how many parked logins were due, and how many of those were
/// renewed. Everything due was going to expire, so nothing due is the good answer, and it
/// has to read like one rather than like a failure to do anything. The sentence does not say
/// why the rest were not renewed, and they fare differently: one whose service could not be
/// reached or asked for less traffic is tried again on the next run, one the service refused
/// has been dropped and its account needs a sign-in, and one that failed is named by its
/// error code in `pitboard renew --json`.
pub fn renewal_note(due: usize, renewed: usize) -> String {
    match (due, renewed) {
        (0, _) => "No parked login was due.".into(),
        (due, 0) => format!("{due} due; none could be renewed this time."),
        (1, 1) => "Renewed one.".into(),
        (due, done) if due == done => format!("Renewed all {done}."),
        (due, done) => format!("Renewed {done} of {due}; the rest are tried again next time."),
    }
}

/// A login as a message names it: by its email, and as another organisation's, or for
/// Codex another workspace's, where the account it is told apart from has that email too.
pub(crate) fn login(tool: crate::provider::ProviderId, email: &str, shares_email: bool) -> String {
    use crate::provider::ProviderId;
    let group = match tool {
        ProviderId::Claude => "organisation",
        ProviderId::Codex => "workspace",
        // Nobody tells Pitboard a Claude Desktop account's email.
        ProviderId::Desktop if email.is_empty() => return "another Claude Desktop account".into(),
        ProviderId::Desktop => "account",
    };
    if !shares_email {
        return email.to_owned();
    }
    format!("{email} in another {group}")
}

// Windows PowerShell 5.1's default policy refuses the `claude.ps1` npm leaves beside `claude.cmd`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spelling {
    Bare,
    Cmd,
}

impl Spelling {
    pub fn of(self, tool: crate::provider::ProviderId) -> &'static str {
        use crate::provider::ProviderId;
        match (self, tool) {
            (Spelling::Cmd, ProviderId::Claude) => "claude.cmd",
            (Spelling::Cmd, ProviderId::Codex) => "codex.cmd",
            (Spelling::Bare | Spelling::Cmd, tool) => tool.program(),
        }
    }
}

pub(crate) fn spelled(
    ctx: &crate::context::Context,
    tool: crate::provider::ProviderId,
) -> Spelling {
    match ctx.os {
        Os::MacOs | Os::Linux => Spelling::Bare,
        Os::Windows => spelling(
            ctx.os,
            tool,
            crate::provider::program_of(ctx, tool).as_deref(),
        ),
    }
}

pub(crate) fn spelling(
    os: Os,
    tool: crate::provider::ProviderId,
    found: Option<&std::path::Path>,
) -> Spelling {
    use crate::provider::ProviderId;
    let found_as = |file: &str| {
        found
            .and_then(std::path::Path::file_name)
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|name| name.eq_ignore_ascii_case(file))
    };
    match (os, tool) {
        (Os::Windows, ProviderId::Claude | ProviderId::Codex)
            if found_as(Spelling::Cmd.of(tool)) =>
        {
            Spelling::Cmd
        }
        _ => Spelling::Bare,
    }
}

// What signs a tool in, to start a sentence (`starts`) or to go on with one.
pub(crate) fn sign_in_yourself(
    tool: crate::provider::ProviderId,
    program: &str,
    starts: bool,
) -> String {
    use crate::provider::ProviderId;
    let said = match tool {
        ProviderId::Claude => format!("Run `{program}`"),
        ProviderId::Codex => format!("Run `{program} login`"),
        ProviderId::Desktop => "Open Claude".into(),
    };
    starting(&said, starts)
}

// What runs a tool once, as its sessions do.
pub(crate) fn run_it(tool: crate::provider::ProviderId, program: &str, starts: bool) -> String {
    use crate::provider::ProviderId;
    let said = match tool {
        ProviderId::Claude | ProviderId::Codex => format!("Run `{program}`"),
        ProviderId::Desktop => "Open Claude".into(),
    };
    starting(&said, starts)
}

// What reads a tool's login from where it was, where this run reads another place.
pub(crate) fn pointing_where_it_did(tool: crate::provider::ProviderId) -> String {
    use crate::provider::ProviderId;
    match tool {
        ProviderId::Claude => "Run Pitboard with CLAUDE_CONFIG_DIR pointing where it did",
        ProviderId::Codex => "Run Pitboard with CODEX_HOME pointing where it did",
        ProviderId::Desktop => "Run Pitboard as the account whose Claude that is",
    }
    .into()
}

fn starting(said: &str, starts: bool) -> String {
    if starts {
        return said.into();
    }
    let mut chars = said.chars();
    chars
        .next()
        .map(|first| first.to_lowercase().chain(chars).collect())
        .unwrap_or_default()
}

pub(crate) fn nothing_signed_in(
    tool: crate::provider::ProviderId,
    program: &str,
    label: Option<&str>,
) -> String {
    use crate::provider::ProviderId;
    match tool {
        ProviderId::Claude | ProviderId::Codex => format!(
            "nothing is signed in to {} right now. {}, sign in, then try again.",
            tool.name(),
            sign_in_yourself(tool, program, true)
        ),
        ProviderId::Desktop => format!(
            "Claude is not signed in. Run `pitboard enroll {} --sign-in`, then sign in inside \
             Claude and quit it: Pitboard waits for that, and sets aside any account name \
             Claude kept first.",
            label.unwrap_or("desktop/<label>")
        ),
    }
}

// What a sign-in inside Claude says while Pitboard waits. Labels are as typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WaitLine<'a> {
    Parked { label: &'a str },
    Add { label: &'a str },
    Again { label: &'a str },
    NoLogOut,
    Waiting { put_back: Option<&'a str> },
    Open,
    QuitSignedOut,
    QuitUnfinished,
}

// One line, ending in a newline.
pub(crate) fn awaited(line: WaitLine<'_>) -> String {
    let said = match line {
        WaitLine::Parked { label } => format!(
            "Parked `{label}`. Claude is signed out, and nothing was signed out at claude.ai."
        ),
        WaitLine::Add { label } => format!(
            "Open Claude and sign in to the account to add as `{label}`. When Claude shows \
             your chats, quit it with Command-Q."
        ),
        WaitLine::Again { label } => format!(
            "Open Claude and sign in to `{label}` again. When Claude shows your chats, quit it \
             with Command-Q."
        ),
        WaitLine::NoLogOut => "Do not choose Log out in Claude: it ends the login Claude holds \
                              at claude.ai, and Pitboard cannot bring it back."
            .into(),
        WaitLine::Waiting {
            put_back: Some(label),
        } => format!(
            "Waiting for Claude to quit. Control-C stops waiting and keeps `{label}` parked; \
             `pitboard use {label}` puts it back."
        ),
        WaitLine::Waiting { put_back: None } => {
            "Waiting for Claude to quit. Control-C stops waiting.".into()
        }
        WaitLine::Open => "Claude is open.".into(),
        WaitLine::QuitSignedOut => "Claude quit with nobody signed in. Still waiting.".into(),
        WaitLine::QuitUnfinished => "Claude quit before it finished signing in. Open it, wait \
                                    until it shows your chats, then quit it. Still waiting."
            .into(),
    };
    format!("{said}\n")
}

// `label`, `put_back` as typed.
pub(crate) fn sign_in_awaited(label: &str, put_back: &str, again: bool) -> String {
    let whom = if again {
        format!("`{label}` again")
    } else {
        "another account".to_string()
    };
    format!(
        "Claude is signed out so you can sign in to {whom}, and `{put_back}` is parked. Sign in \
         inside Claude, quit it, then run `pitboard enroll {label}`. To go back, run `pitboard \
         use {put_back}`."
    )
}

// Codex's login server ends one already on its port as it starts (`codex_login_is_driveable`).
pub(crate) fn codex_runs_before_a_sign_in(holding: &[crate::holder::Holding]) -> String {
    let (verb, them) = if crate::holder::plural(holding) {
        ("are", "them")
    } else {
        ("is", "it")
    };
    format!(
        "{} {verb} running. A Codex sign-in open in {them} ends as this one starts.\n",
        crate::holder::described(holding)
    )
}

pub(crate) fn move_waiting(label: &str) -> String {
    format!(
        "A move of `{label}`'s login was interrupted. Keep Claude closed: the next change to a \
         Claude Desktop account settles it first."
    )
}

/// Why an interrupted move of a Claude Desktop login was not given up, as a clause that
/// follows "because", where there is one to give. Each front end adds its own way out.
pub fn not_given_up_because(why: &crate::error::Error) -> Option<String> {
    match why {
        crate::error::Error::MoveMixed { places, .. } => Some(format!(
            "giving it up would leave parts of it beside another login in Claude's folder: {}",
            places.join(", ")
        )),
        other => left_because(other),
    }
}

// Why a waiting move was left as it was, for a refusal `abandon` and `repair` both meet.
fn left_because(why: &crate::error::Error) -> Option<String> {
    use crate::error::Error;
    Some(match why {
        Error::AppOpen { app, still } if still.is_empty() => format!("{app} is open"),
        Error::AppOpen { app, still } => {
            format!("{} from {app} is still running", listed(still.clone()))
        }
        Error::AppStateUnknown { app, why } => {
            format!("it could not tell whether {app} is open ({why})")
        }
        Error::LoginFolderBusy { app } => format!("another Pitboard is changing {app}'s login"),
        Error::LoginFolderUnreadable { tool, detail } => format!(
            "it could not read {}'s cookie store ({detail})",
            tool.program()
        ),
        Error::AppOpenedMidway { app, .. } => format!("{app} was opened meanwhile"),
        // A park Pitboard could not look at, as `store::folder::unread` says it.
        Error::Store(crate::store::Error::Unreadable(detail)) => detail.clone(),
        _ => return None,
    })
}

// What to do about a refusal `left_because` says why of. `command` is the one to run again.
fn left_way_out(why: &crate::error::Error, command: &str) -> String {
    use crate::error::Error;
    let again = format!("run `{command}` again");
    match why {
        Error::AppOpen { app, still } if still.is_empty() => {
            format!("Quit {app} with Command-Q, then {again}.")
        }
        Error::AppOpen { .. } => wait_for_still(crate::host::OS, &again),
        Error::LoginFolderBusy { .. } => format!("Run `{command}` again once it has finished."),
        Error::LoginFolderUnreadable { tool, .. } => format!(
            "Open {program}, quit it with Command-Q, then {again}.",
            program = tool.program()
        ),
        Error::AppOpenedMidway { app, .. } => {
            format!("Nothing is lost. Quit {app} with Command-Q, then {again}.")
        }
        Error::Store(_) => {
            format!("Make that path, and each folder it is in, readable to you, then {again}.")
        }
        _ => format!("Run `{command}` again."),
    }
}

// After `abandon` gave up a switch, so never that nothing has changed.
pub(crate) fn move_not_given_up(label: &str, why: &crate::error::Error) -> String {
    use crate::error::Error;
    let because = not_given_up_because(why);
    let then = match (why, &because) {
        (Error::MoveMixed { .. }, _) => mixed_way_out(label),
        (why, Some(_)) => left_way_out(why, "pitboard abandon"),
        (why, None) => format!(
            "It stopped at `{}`: run `pitboard abandon` again to see why.",
            why.code()
        ),
    };
    match because {
        Some(because) => format!(
            "Pitboard did not give up the interrupted move of `{label}`'s login, because \
             {because}. {then}"
        ),
        None => {
            format!("Pitboard did not give up the interrupted move of `{label}`'s login. {then}")
        }
    }
}

// For `repair`, which goes on past the move, so never that nothing has changed.
pub(crate) fn move_not_settled(label: &str, why: &crate::error::Error) -> String {
    use crate::error::Error;
    let (because, then) = match (left_because(why), why) {
        (Some(because), _) => (because, left_way_out(why, "pitboard repair")),
        (None, Error::RecoveryUndetermined { detail, .. }) => (
            format!("it cannot tell where it stopped ({detail})"),
            undetermined_way_out(),
        ),
        (None, Error::RecoveryStopped { detail, .. }) => (
            format!("it stopped partway ({detail})"),
            "Nothing is lost. Keep Claude closed and run `pitboard repair` again. If it stops \
             again, `pitboard abandon` keeps every folder and moves on."
                .into(),
        ),
        (None, Error::RecoveryElsewhere { tool, slot, .. }) => (
            format!(
                "{} kept its data in {slot} when the move was interrupted, and this run finds \
                 it somewhere else",
                tool.program()
            ),
            elsewhere_way_out(*tool),
        ),
        (None, other) => (
            format!("it stopped at `{}`", other.code()),
            "The next change to a Claude Desktop account settles it first, or says why it \
             cannot."
                .into(),
        ),
    };
    format!(
        "Pitboard did not settle the interrupted move of `{label}`'s login, because {because}. \
         {then}"
    )
}

// For a move Pitboard cannot tell where it stopped.
pub(crate) fn undetermined_way_out() -> String {
    "Open Claude to see whether it is signed in and to which account, then quit it with \
     Command-Q. If it is signed in, run `pitboard enroll desktop/<label>` for the account it \
     shows. If it is signed out, `pitboard abandon` keeps every folder and moves on."
        .into()
}

// For a move interrupted while the app kept its data somewhere else.
pub(crate) fn elsewhere_way_out(tool: crate::provider::ProviderId) -> String {
    format!(
        "{}, or `pitboard abandon` to keep every folder and move on.",
        pointing_where_it_did(tool)
    )
}

pub fn wait_for_still(os: Os, then: &str) -> String {
    let monitor = match os {
        // No app that keeps a tool's login runs on Linux.
        Os::MacOs | Os::Linux => "Activity Monitor",
        Os::Windows => "Task Manager",
    };
    format!("Wait until it ends, or quit it in {monitor}, then {then}.")
}

pub(crate) fn mixed_way_out(label: &str) -> String {
    format!(
        "Open Claude, check which account it shows, quit it with Command-Q, then run `pitboard \
         enroll desktop/<label>` under that account's name: Pitboard sets the parts of \
         `{label}` aside first."
    )
}

// Claude Code's only such setting is a managed one.
pub(crate) fn restricted_way_out(tool: crate::provider::ProviderId) -> &'static str {
    use crate::provider::ProviderId;
    match tool {
        ProviderId::Claude => {
            "Switch to an account of an organisation the setting names; only whoever manages \
             this computer can change it."
        }
        ProviderId::Codex | ProviderId::Desktop => {
            "Switch to an account the setting allows, or change the setting."
        }
    }
}

pub(crate) fn move_settled(
    label: &str,
    half: crate::state::Half,
    ended: crate::switch::Ended,
) -> String {
    use crate::state::Half;
    use crate::switch::Ended;
    let came_to = match (half, ended) {
        (Half::Leave, Ended::Undone) => format!("Pitboard undid it, and `{label}` is in use"),
        (Half::Leave, Ended::Finished) => {
            "Pitboard finished parking it, and Claude is signed out".to_string()
        }
        (Half::Arrive, Ended::Undone) => {
            format!("Pitboard took it back out, `{label}` is parked and Claude is signed out")
        }
        (Half::Arrive, Ended::Finished) => {
            "it had in fact finished, and Pitboard has recorded that".to_string()
        }
    };
    format!("an earlier move of `{label}`'s login was interrupted; {came_to}")
}

pub(crate) fn item_gone(label: &str, item: &str) -> String {
    format!(
        "`{item}` of `{label}`'s login was in neither place after the move, so Pitboard went on \
         without it. Claude makes it again."
    )
}

/// What giving up a Claude Desktop move did and what is left to do, as every front end says
/// it: `name` is the account as the front end names one, and `way_on`, a clause, its way to
/// the account Claude shows.
pub fn given_up(given_up: &crate::switch::GivenUp, name: &str, way_on: &str) -> String {
    let app = given_up.tool.program();
    let mut said = format!(
        "Pitboard gave up the move of {name}'s login and kept every folder: {} parked, {} set \
         aside.",
        given_up.parked, given_up.set_aside
    );
    if let Some(slot) = &given_up.slot {
        let _ = write!(
            said,
            " The move was in {slot}, which Pitboard did not read."
        );
    }
    if given_up.left.is_empty() {
        let _ = write!(said, " {}", capitalised(way_on));
    } else {
        let left = listed(given_up.left.iter().map(|item| format!("`{item}`")));
        let stay = if given_up.left.len() == 1 {
            "stays"
        } else {
            "stay"
        };
        let _ = write!(
            said,
            " {left} {stay} in {app}'s folder. Open {app} once and quit it with Command-Q, then \
             {way_on}"
        );
    }
    said
}

// Claude's settings name `named` since Pitboard last read `last` in use: no service is asked.
pub(crate) fn desktop_named(
    named: &crate::service::Stored,
    last: &crate::service::Stored,
) -> String {
    use crate::service::Stored;
    let since = match last {
        Stored::Account(_) | Stored::Unenrolled(_) => format!(
            "since Pitboard last read {} in use, so Claude may hold another login",
            last.said("nobody")
        ),
        Stored::Nothing | Stored::Unknown => {
            "since Pitboard last read Claude signed out, so somebody may have signed in".into()
        }
    };
    format!(
        "Claude's settings have named {} {since}",
        named.said("nobody")
    )
}

// By the date the session cookie carries, hedged: claude.ai may end it sooner or keep it.
pub fn session_life(session_expires_at: Option<i64>, now: i64) -> Option<String> {
    let left = session_expires_at? - now;
    Some(match left / DAY {
        _ if left <= 0 => "Its sign-in may have lapsed".into(),
        0 => "Its sign-in may lapse in under a day".into(),
        1 => "Its sign-in may lapse in 1 day".into(),
        days => format!("Its sign-in may lapse in {days} days"),
    })
}

// Nobody tells Pitboard a Claude Desktop account's email, so its name stands alone.
pub fn with_email(name: &str, email: &str) -> String {
    if email.is_empty() {
        name.into()
    } else {
        format!("{name} ({email})")
    }
}

// Hedged, as `session_life` is: the date is the cookie's, and claude.ai may end it sooner.
pub fn session_life_column(session_expires_at: i64, now: i64) -> String {
    match session_expires_at - now {
        left if left <= 0 => "sign-in may have lapsed".into(),
        left => format!("sign-in may lapse in {}", span(left)),
    }
}

pub fn enrolled_lines(
    tool: crate::provider::ProviderId,
    enrolled: &crate::switch::Enrolled,
    name: &str,
    to_use: &str,
    another: &str,
) -> String {
    use crate::provider::ProviderId;
    use crate::switch::Enrolled;
    let in_its_app = match (tool, enrolled) {
        (ProviderId::Claude | ProviderId::Codex, _) => None,
        (ProviderId::Desktop, Enrolled::Current { .. }) => Some(format!(
            "Enrolled {name}, the account Claude is signed in to now. Check that Claude shows \
             that account: Pitboard cannot ask claude.ai whose it is.\nAdd another: pitboard \
             enroll {another} --sign-in\n"
        )),
        (ProviderId::Desktop, Enrolled::InUse { again: false, .. }) => Some(format!(
            "Enrolled {name}, the account Claude is signed in to now.\n"
        )),
        (ProviderId::Desktop, Enrolled::InUse { again: true, .. }) => Some(format!(
            "Signed in to {name} again. Claude is signed in to it now.\n"
        )),
        // `folder::enroll` makes neither.
        (ProviderId::Desktop, Enrolled::SignedIn { .. } | Enrolled::Renewed { .. }) => None,
    };
    if let Some(said) = in_its_app {
        return said;
    }
    match enrolled {
        Enrolled::Current { email } => format!(
            "Enrolled {}, the account signed in now.\nAdd another without signing out of it: \
             pitboard enroll {another} --sign-in\n",
            with_email(name, email)
        ),
        Enrolled::SignedIn { email } => format!(
            "Enrolled {}. Switch to it with: pitboard use {to_use}\n",
            with_email(name, email)
        ),
        Enrolled::Renewed { email } => format!(
            "Renewed {}: its parked login is a fresh one.\n",
            with_email(name, email)
        ),
        Enrolled::InUse { email, again: true } => format!(
            "Signed in to {} again. Its new login is the one in use now.\n",
            with_email(name, email)
        ),
        Enrolled::InUse {
            email,
            again: false,
        } => format!(
            "Enrolled {}, the account signed in now. Its new login is the one in use.\n",
            with_email(name, email)
        ),
    }
}

// `tool` is None for a label that names no account.
pub fn forget_question(tool: Option<crate::provider::ProviderId>, label: &str) -> String {
    use crate::provider::ProviderId;
    match tool {
        None | Some(ProviderId::Claude | ProviderId::Codex) => format!(
            "Forget {label} and delete its parked login? Adding it again needs a browser \
             sign-in. [y/N] "
        ),
        Some(ProviderId::Desktop) => format!(
            "Forget {label} and delete its parked login and the folders set aside for it? \
             Adding it again needs a sign-in in Claude. [y/N] "
        ),
    }
}

// Claude Code's config names `email` as signed in and no store Pitboard reads on `os` holds a
// login: Pitboard is looking in the wrong place.
pub(crate) fn named_but_not_found(os: Os, email: &str) -> String {
    let places = match os {
        Os::MacOs | Os::Linux => "in the keychain or in the file it also reads",
        Os::Windows => "in .credentials.json or in Credential Manager",
    };
    format!(
        "Claude Code's config says {email} is signed in, but Pitboard cannot find that login {places}"
    )
}

/// Why putting another login in place of `tool`'s login in use loses nothing, where its
/// service refuses that login's refresh token for good.
pub fn replacing(tool: crate::provider::ProviderId, detail: &str) -> String {
    format!(
        "{} no longer accepts {}'s login in use ({detail}), so putting another login in its \
         place loses nothing.",
        tool.service(),
        tool.name()
    )
}

/// What `pitboard use` asks before it puts `label`'s login in place of that one.
pub fn replace_question(tool: crate::provider::ProviderId, label: &str, detail: &str) -> String {
    format!(
        "{}\nPut {label}'s login in its place? [y/N] ",
        replacing(tool, detail)
    )
}

// What `adopt` says of the Claude Desktop logins it set aside, each account as typed.
pub fn adopted_folders(labels: &[String]) -> String {
    let named = listed(labels.iter().map(|label| format!("`{label}`")));
    format!(
        "Pitboard set aside the parked Claude Desktop logins of {named}. They were parked on \
         another Mac, and Claude cannot read them here where its key differs. `pitboard \
         forget` of an account deletes its own."
    )
}

/// The line under doctor's checks, or over them in the app. Checks that only warn are
/// counted as worth looking at. One that fails outweighs every warning: the line then
/// counts what is broken and says not to switch accounts, since a check fails only when
/// something Pitboard relies on does not hold.
pub fn doctor_summary(levels: impl IntoIterator<Item = Level>) -> String {
    let (mut broken, mut worth_a_look) = (0, 0);
    for level in levels {
        match level {
            Level::Fail => broken += 1,
            Level::Warn => worth_a_look += 1,
            Level::Ok => {}
        }
    }
    match (broken, worth_a_look) {
        (0, 0) => "Everything Pitboard checks is in order.".into(),
        (0, 1) => "One thing is worth looking at.".into(),
        (0, n) => format!("{n} things are worth looking at."),
        (n, _) => format!("{n} broken: do not switch accounts until fixed."),
    }
}

pub(crate) struct ChangesNothing {
    pub(crate) refusal: String,
    // No full stop: a read's warning goes on from it with ", so it changes nothing".
    pub(crate) because: String,
    pub(crate) way_out: &'static str,
    pub(crate) to_ask_again: &'static str,
}

// Doctor's alone, where `EnableLUA` reads 0: the gate reads no setting.
pub(crate) const ADMIN_APPROVAL_OFF: &str = "User Account Control is off on this PC, so every \
     program an administrator starts is elevated: turn it on, or run Pitboard from a standard \
     account.";

// On macOS and Linux each refusal shares one sentence, since the way out is the same. Linux
// lets root act in its own home.
pub(crate) fn elevated(os: Os, why: Option<&'static str>) -> ChangesNothing {
    let because = |cannot_tell: &str| match why {
        Some(why) => format!("Pitboard runs {why}"),
        None => format!("Pitboard cannot tell whether it runs {cannot_tell}"),
    };
    let refused = match os {
        Os::Linux => "with sudo or as root in a folder another user owns",
        Os::MacOs | Os::Windows => "as root or with sudo",
    };
    match os {
        Os::MacOs | Os::Linux => ChangesNothing {
            refusal: match why {
                Some(_) => {
                    format!("Pitboard changes nothing when it runs {refused}. Run it as yourself.")
                }
                None => format!(
                    "Pitboard changes nothing when it cannot tell whether it runs {refused}. Run it \
                     as yourself."
                ),
            },
            because: because(refused),
            way_out: "Run it as yourself.",
            to_ask_again: "Run it as yourself to ask again.",
        },
        Os::Windows => {
            let (way_out, to_ask_again) = match why {
                Some(token::IN_EVERY_PROGRAM) => (
                    "That is so with User Account Control off and in the built-in Administrator \
                     account: run it from a standard account, or turn User Account Control on.",
                    "Run it from a standard account, or with User Account Control on, to ask \
                     again.",
                ),
                _ => (
                    "Run it from a terminal that is not elevated (not Run as administrator).",
                    "Run it from a terminal that is not elevated to ask again.",
                ),
            };
            ChangesNothing {
                refusal: match why {
                    Some(why) => format!("Pitboard changes nothing when it runs {why}. {way_out}"),
                    None => format!(
                        "Pitboard changes nothing when it cannot tell whether it runs elevated. \
                         {way_out}"
                    ),
                },
                because: because("elevated"),
                way_out,
                to_ask_again,
            }
        }
    }
}

/// Why `job` cannot be installed on `os` here, and what to do instead.
pub fn no_scheduler(os: Os, job: Job) -> String {
    let (unit, instead) = match job {
        Job::Renew => (
            "timer",
            "Keeping parked logins alive here means running `pitboard` yourself from time to time.",
        ),
        Job::Watch => (
            "service",
            "Switching by itself here means leaving `pitboard watch` running in a terminal.",
        ),
    };
    match os {
        Os::Linux => format!(
            "systemd did not start this system, as it does not start many containers, so it has \
             no systemd user {unit} for Pitboard to write. {instead}"
        ),
        Os::MacOs | Os::Windows => {
            format!("Pitboard knows no scheduler it can write on this system. {instead}")
        }
    }
}

pub(crate) fn too_old(build: Option<u32>) -> ChangesNothing {
    let way_out = "It changes things on Windows 11 24H2 and later, and on Windows Server 2025: \
                   update Windows to use it here.";
    let to_ask_again = "Update Windows to ask again.";
    match build {
        Some(build) => ChangesNothing {
            refusal: format!("Pitboard changes nothing on Windows build {build}. {way_out}"),
            because: format!(
                "Pitboard runs on Windows build {build}, older than Windows 11 24H2 (build \
                 {WINDOWS_FLOOR})"
            ),
            way_out,
            to_ask_again,
        },
        None => ChangesNothing {
            refusal: format!(
                "Pitboard changes nothing when it cannot tell which build of Windows it runs \
                 on. {way_out}"
            ),
            because: "Pitboard cannot tell which build of Windows it runs on".into(),
            way_out,
            to_ask_again,
        },
    }
}

// A setting's values, each as code, and none as an empty list.
pub(crate) fn listed_values<T: AsRef<str>>(values: &[T]) -> String {
    if values.is_empty() {
        return "an empty list".into();
    }
    listed(values.iter().map(|value| format!("`{}`", value.as_ref())))
}

/// `text` with its first letter a capital, to begin a sentence with: "5-hour" stays "5-hour"
/// and "weekly" becomes "Weekly".
pub fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// Things one after another, as a sentence lists them: "a", "a and b", "a, b and c".
pub fn listed<T: AsRef<str>>(items: impl IntoIterator<Item = T>) -> String {
    let items: Vec<T> = items.into_iter().collect();
    match items.as_slice() {
        [] => String::new(),
        [one] => one.as_ref().to_owned(),
        [rest @ .., last] => {
            let rest: Vec<&str> = rest.iter().map(AsRef::as_ref).collect();
            format!("{} and {}", rest.join(", "), last.as_ref())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_figure_is_rounded_to_the_nearest_whole_percent() {
        assert_eq!(figure(64.4), "64%");
        assert_eq!(figure(64.5), "65%");
        assert_eq!(figure(0.4), "0%");
        assert_eq!(figure(100.0), "100%");
        assert_eq!(figure(130.2), "130%");
    }

    // The file sits behind the keychain on macOS and behind Credential Manager on Windows, which
    // a sign-in over SSH with a key cannot reach.
    #[test]
    fn a_file_left_behind_the_store_is_said_in_the_words_of_its_system() {
        use crate::switch::{Foreseen, Kept, Left, Stowed};
        let path = std::path::PathBuf::from("/x/.credentials.json");
        let left = Left {
            path: path.clone(),
            seen: "0123456789abcdef".into(),
            login: Foreseen::Kept(Kept::ParkKept {
                label: "work".into(),
            }),
            dropped: vec!["mcpOAuth".into()],
        };
        let stowed = Stowed {
            path,
            kept: Kept::ParkKept {
                label: "work".into(),
            },
            dropped: vec!["mcpOAuth".into()],
        };
        for (os, store, over_ssh, reader) in [
            (
                Os::MacOs,
                "the keychain",
                "one over SSH",
                "a session that cannot read the keychain, such as one started over SSH",
            ),
            (
                Os::Windows,
                "Credential Manager",
                "one in a sign-in over SSH with a key",
                "a session that cannot reach Credential Manager, such as one in a sign-in over \
                 SSH with a key",
            ),
        ] {
            for lines in [left_lines(os, &left), stowed_lines(os, &stowed)] {
                assert!(
                    lines[1]
                        .ends_with(&format!("Claude Code's document in {store} keeps its own.")),
                    "{os:?}: {lines:?}"
                );
                assert!(
                    lines[2].ends_with(&format!(
                        "one that signed in with its login, such as {over_ssh}, is signed out."
                    )),
                    "{os:?}: {lines:?}"
                );
            }
            assert_eq!(reads_only_the_file(os), reader);
        }
    }

    #[test]
    fn a_halted_warm_up_says_why() {
        assert_eq!(
            crate::error::Error::WarmUpHalted {
                halt: crate::duty::warm::Halt::ProgramMissing
            }
            .to_string(),
            "Pitboard warms up no account now: Claude Code is not installed where Pitboard looks."
        );
    }

    #[test]
    fn what_is_left_of_an_app_is_quit_where_the_system_quits_it() {
        assert_eq!(
            wait_for_still(Os::Windows, "run this again"),
            "Wait until it ends, or quit it in Task Manager, then run this again."
        );
        for os in [Os::MacOs, Os::Linux] {
            assert_eq!(
                wait_for_still(os, "run this again"),
                "Wait until it ends, or quit it in Activity Monitor, then run this again."
            );
        }
    }

    #[test]
    fn claude_desktops_words_hedge_what_nobody_measured() {
        use crate::provider::ProviderId;
        let now = 1_791_000_000;
        let life = |left: i64| session_life(Some(now + left), now);
        assert_eq!(
            life(25 * DAY + 60).as_deref(),
            Some("Its sign-in may lapse in 25 days")
        );
        assert_eq!(life(DAY).as_deref(), Some("Its sign-in may lapse in 1 day"));
        assert_eq!(
            life(60).as_deref(),
            Some("Its sign-in may lapse in under a day")
        );
        assert_eq!(life(0).as_deref(), Some("Its sign-in may have lapsed"));
        assert_eq!(session_life(None, now), None);
        assert_eq!(
            adopted_folders(&["desktop/work".into(), "desktop/home".into()]),
            "Pitboard set aside the parked Claude Desktop logins of `desktop/work` and \
             `desktop/home`. They were parked on another Mac, and Claude cannot read them here \
             where its key differs. `pitboard forget` of an account deletes its own."
        );
        assert_eq!(
            login(ProviderId::Desktop, "", false),
            "another Claude Desktop account"
        );
        assert_eq!(
            sign_in_yourself(ProviderId::Desktop, "Claude", false),
            "open Claude"
        );
        assert_eq!(
            pointing_where_it_did(ProviderId::Desktop),
            "Run Pitboard as the account whose Claude that is"
        );
    }

    // npm leaves `claude.cmd` and `claude.ps1`, the native installer and WinGet `claude.exe`.
    #[test]
    fn a_hint_names_the_cmd_only_where_pitboard_found_one_on_windows() {
        use crate::provider::ProviderId;
        let npm = "C:/Users/x/AppData/Roaming/npm";
        for (os, tool, found, spelled) in [
            (
                Os::Windows,
                ProviderId::Claude,
                format!("{npm}/claude.cmd"),
                "claude.cmd",
            ),
            (
                Os::Windows,
                ProviderId::Claude,
                format!("{npm}/CLAUDE.CMD"),
                "claude.cmd",
            ),
            (
                Os::Windows,
                ProviderId::Codex,
                format!("{npm}/codex.cmd"),
                "codex.cmd",
            ),
            (
                Os::Windows,
                ProviderId::Claude,
                "C:/Users/x/.local/bin/claude.exe".into(),
                "claude",
            ),
            (
                Os::Windows,
                ProviderId::Codex,
                "C:/Users/x/AppData/Local/Microsoft/WinGet/Links/codex.exe".into(),
                "codex",
            ),
            // Named in PITBOARD_CLAUDE, which a hint never named.
            (
                Os::Windows,
                ProviderId::Claude,
                "C:/tools/cc.cmd".into(),
                "claude",
            ),
            (
                Os::MacOs,
                ProviderId::Claude,
                "/scratch/npm/claude.cmd".into(),
                "claude",
            ),
            (
                Os::Linux,
                ProviderId::Codex,
                "/scratch/npm/codex.cmd".into(),
                "codex",
            ),
        ] {
            let found = std::path::PathBuf::from(found);
            assert_eq!(
                spelling(os, tool, Some(&found)).of(tool),
                spelled,
                "{os:?} {found:?}"
            );
        }
        for tool in ProviderId::ALL.iter().copied() {
            assert_eq!(spelling(Os::Windows, tool, None), Spelling::Bare, "{tool}");
            assert_eq!(Spelling::Bare.of(tool), tool.program(), "{tool}");
        }
        assert_eq!(Spelling::Cmd.of(ProviderId::Desktop), "Claude");
        assert_eq!(
            sign_in_yourself(ProviderId::Claude, "claude.cmd", true),
            "Run `claude.cmd`"
        );
        assert_eq!(
            sign_in_yourself(ProviderId::Codex, "codex.cmd", false),
            "run `codex.cmd login`"
        );
        assert_eq!(
            run_it(ProviderId::Codex, "codex.cmd", true),
            "Run `codex.cmd`"
        );
    }

    #[test]
    fn a_name_with_no_email_has_no_brackets() {
        assert_eq!(with_email("work", "a@example.com"), "work (a@example.com)");
        assert_eq!(with_email("desktop/work", ""), "desktop/work");
    }

    #[test]
    fn enrolled_lines_say_what_main_said_for_claude_code_and_codex() {
        use crate::provider::ProviderId;
        use crate::switch::Enrolled;
        let email = || "a@example.com".to_string();
        for tool in [ProviderId::Claude, ProviderId::Codex] {
            let said = |enrolled: Enrolled| {
                enrolled_lines(tool, &enrolled, "work", "codex/work", "codex/<label>")
            };
            assert_eq!(
                said(Enrolled::Current { email: email() }),
                "Enrolled work (a@example.com), the account signed in now.\n\
                 Add another without signing out of it: pitboard enroll codex/<label> \
                 --sign-in\n"
            );
            assert_eq!(
                said(Enrolled::SignedIn { email: email() }),
                "Enrolled work (a@example.com). Switch to it with: pitboard use codex/work\n"
            );
            assert_eq!(
                said(Enrolled::Renewed { email: email() }),
                "Renewed work (a@example.com): its parked login is a fresh one.\n"
            );
            assert_eq!(
                said(Enrolled::InUse {
                    email: email(),
                    again: true
                }),
                "Signed in to work (a@example.com) again. Its new login is the one in use now.\n"
            );
            assert_eq!(
                said(Enrolled::InUse {
                    email: email(),
                    again: false
                }),
                "Enrolled work (a@example.com), the account signed in now. Its new login is the \
                 one in use.\n"
            );
        }
    }

    #[test]
    fn enrolled_lines_for_claude_desktop_name_the_account_claude_is_signed_in_to() {
        use crate::provider::ProviderId;
        use crate::switch::Enrolled;
        let said = |enrolled: Enrolled| {
            enrolled_lines(
                ProviderId::Desktop,
                &enrolled,
                "desktop/work",
                "desktop/work",
                "desktop/<label>",
            )
        };
        assert_eq!(
            said(Enrolled::Current {
                email: String::new()
            }),
            "Enrolled desktop/work, the account Claude is signed in to now. Check that Claude \
             shows that account: Pitboard cannot ask claude.ai whose it is.\n\
             Add another: pitboard enroll desktop/<label> --sign-in\n"
        );
        assert_eq!(
            said(Enrolled::InUse {
                email: String::new(),
                again: false
            }),
            "Enrolled desktop/work, the account Claude is signed in to now.\n"
        );
        assert_eq!(
            said(Enrolled::InUse {
                email: String::new(),
                again: true
            }),
            "Signed in to desktop/work again. Claude is signed in to it now.\n"
        );
        assert_eq!(
            said(Enrolled::SignedIn {
                email: String::new()
            }),
            "Enrolled desktop/work. Switch to it with: pitboard use desktop/work\n"
        );
        assert!(
            !said(Enrolled::Renewed {
                email: String::new()
            })
            .contains("()")
        );
    }

    #[test]
    fn the_forget_question_says_a_claude_desktop_account_signs_in_again_in_claude() {
        use crate::provider::ProviderId;
        let main = "Forget work and delete its parked login? Adding it again needs a browser \
                    sign-in. [y/N] ";
        for tool in [None, Some(ProviderId::Claude), Some(ProviderId::Codex)] {
            assert_eq!(forget_question(tool, "work"), main, "{tool:?}");
        }
        assert_eq!(
            forget_question(Some(ProviderId::Desktop), "desktop/home"),
            "Forget desktop/home and delete its parked login and the folders set aside for it? \
             Adding it again needs a sign-in in Claude. [y/N] "
        );
    }

    #[test]
    fn a_session_life_is_hedged_and_never_says_expired() {
        const NOW: i64 = 1_789_935_000;
        let column = |left: i64| session_life_column(NOW + left, NOW);
        assert_eq!(column(25 * DAY + 3 * HOUR), "sign-in may lapse in 25d 3h");
        assert_eq!(column(2 * HOUR + 5 * MINUTE), "sign-in may lapse in 2h 05m");
        assert_eq!(column(30), "sign-in may lapse in <1m");
        assert_eq!(column(0), "sign-in may have lapsed");
        assert_eq!(column(-DAY), "sign-in may have lapsed");
        for left in [-DAY, 0, 30, DAY, 40 * DAY] {
            let said = format!(
                "{} {}",
                column(left),
                session_life(Some(NOW + left), NOW).unwrap_or_default()
            );
            assert!(!said.contains("expire"), "{said}");
        }
    }

    /// Why Pitboard did not switch by itself is said in the app as well as in a terminal,
    /// so it names no command: the app gives up on an interrupted switch with a button.
    #[test]
    fn why_it_did_not_switch_names_no_command() {
        use crate::duty::autoswitch::{Blind, Skip, Threshold};
        for why in [
            Skip::NoRoom {
                unread: vec!["spare".into()],
            },
            Skip::AlreadyLeft,
            Skip::GaveUp,
            Skip::Settling { until: 0 },
            Skip::Overridden(vec!["apiKeyHelper".into()]),
        ] {
            let said = not_switching(&why, Threshold::DEFAULT);
            assert!(!said.contains("pitboard "), "{why:?}");
        }
        for why in [
            Blind::SwitchInterrupted,
            Blind::CustomOauth,
            Blind::NothingSignedIn,
            Blind::NotEnrolled {
                email: "me@example.com".into(),
            },
            Blind::Unidentified {
                detail: "could not reach Anthropic: no route to host".into(),
                until: 0,
            },
            Blind::NoReading {
                account: "work".into(),
            },
        ] {
            assert!(!not_watching(&why).contains("pitboard "), "{why:?}");
        }
    }

    // A container whose only user is root seldom runs systemd, so Linux says what is missing.
    #[test]
    fn a_system_with_no_scheduler_says_what_it_lacks() {
        assert_eq!(
            no_scheduler(Os::Linux, Job::Renew),
            "systemd did not start this system, as it does not start many containers, so it has \
             no systemd user timer for Pitboard to write. Keeping parked logins alive here means \
             running `pitboard` yourself from time to time."
        );
        assert_eq!(
            no_scheduler(Os::Linux, Job::Watch),
            "systemd did not start this system, as it does not start many containers, so it has \
             no systemd user service for Pitboard to write. Switching by itself here means \
             leaving `pitboard watch` running in a terminal."
        );
        for os in [Os::MacOs, Os::Windows] {
            assert_eq!(
                no_scheduler(os, Job::Renew),
                "Pitboard knows no scheduler it can write on this system. Keeping parked \
                 logins alive here means running `pitboard` yourself from time to time."
            );
        }
    }

    // On Linux root may act where it owns every folder Pitboard writes, so only sudo and a
    // folder another user owns refuse.
    #[test]
    fn root_and_sudo_are_told_to_run_it_as_themselves() {
        for (os, whys, refused) in [
            (Os::MacOs, ["as root", "with sudo"], "as root or with sudo"),
            (
                Os::Linux,
                ["as root in a folder another user owns", "with sudo"],
                "with sudo or as root in a folder another user owns",
            ),
        ] {
            for why in whys {
                let said = elevated(os, Some(why));
                assert_eq!(
                    said.refusal,
                    format!("Pitboard changes nothing when it runs {refused}. Run it as yourself.")
                );
                assert_eq!(said.because, format!("Pitboard runs {why}"));
                assert_eq!(said.way_out, "Run it as yourself.");
                assert_eq!(said.to_ask_again, "Run it as yourself to ask again.");
            }
            let unknown = elevated(os, None);
            assert_eq!(
                unknown.refusal,
                format!(
                    "Pitboard changes nothing when it cannot tell whether it runs {refused}. Run \
                     it as yourself."
                )
            );
            assert_eq!(
                unknown.because,
                format!("Pitboard cannot tell whether it runs {refused}")
            );
        }
    }

    #[test]
    fn an_elevated_windows_run_is_told_which_terminal_to_use() {
        let terminal = "Run it from a terminal that is not elevated (not Run as administrator).";
        for (why, refusal) in [
            (
                Some(token::AS_ADMINISTRATOR),
                "Pitboard changes nothing when it runs as administrator.",
            ),
            (
                Some(token::AS_A_SERVICE_ACCOUNT),
                "Pitboard changes nothing when it runs as a service account.",
            ),
            (
                None,
                "Pitboard changes nothing when it cannot tell whether it runs elevated.",
            ),
        ] {
            let said = elevated(Os::Windows, why);
            assert_eq!(said.refusal, format!("{refusal} {terminal}"), "{why:?}");
            assert_eq!(said.way_out, terminal, "{why:?}");
            assert_eq!(
                said.to_ask_again,
                "Run it from a terminal that is not elevated to ask again."
            );
        }
        assert_eq!(
            elevated(Os::Windows, Some(token::AS_ADMINISTRATOR)).because,
            "Pitboard runs as administrator"
        );
        assert_eq!(
            elevated(Os::Windows, None).because,
            "Pitboard cannot tell whether it runs elevated"
        );

        let always = elevated(Os::Windows, Some(token::IN_EVERY_PROGRAM));
        assert_eq!(
            always.refusal,
            "Pitboard changes nothing when it runs elevated, as every program this Windows \
             account starts does. That is so with User Account Control off and in the \
             built-in Administrator account: run it from a standard account, or turn User \
             Account Control on."
        );
        assert_eq!(
            always.because,
            "Pitboard runs elevated, as every program this Windows account starts does"
        );
        assert_eq!(
            always.to_ask_again,
            "Run it from a standard account, or with User Account Control on, to ask again."
        );
    }

    #[test]
    fn an_old_windows_is_told_which_it_needs() {
        let old = too_old(Some(22631));
        assert_eq!(
            old.refusal,
            "Pitboard changes nothing on Windows build 22631. It changes things on Windows 11 \
             24H2 and later, and on Windows Server 2025: update Windows to use it here."
        );
        assert_eq!(
            old.because,
            "Pitboard runs on Windows build 22631, older than Windows 11 24H2 (build 26100)"
        );
        assert_eq!(old.to_ask_again, "Update Windows to ask again.");
        let unknown = too_old(None);
        assert_eq!(
            unknown.refusal,
            "Pitboard changes nothing when it cannot tell which build of Windows it runs on. \
             It changes things on Windows 11 24H2 and later, and on Windows Server 2025: update \
             Windows to use it here."
        );
        assert_eq!(
            unknown.because,
            "Pitboard cannot tell which build of Windows it runs on"
        );
    }

    #[test]
    fn spans_read_at_a_glance() {
        assert_eq!(span(-5), "<1m");
        assert_eq!(span(59), "<1m");
        assert_eq!(span(60 * 47), "47m");
        assert_eq!(span(3600 * 2 + 60 * 5), "2h 05m");
        assert_eq!(span(86_400 * 6 + 3600 * 4 + 59), "6d 4h");
    }

    /// A limit is named by how long it runs, which both services agree on, so a Codex limit
    /// reads the way a Claude Code one does, in a sentence and in a column alike.
    #[test]
    fn a_limit_is_named_for_its_length() {
        for (length, column, sentence) in [
            (18_000, "5h", "5-hour"),
            (604_800, "week", "weekly"),
            (86_400, "day", "daily"),
            (10_800, "3h", "3-hour"),
            (172_800, "2d", "2-day"),
            (5_400, "90m", "90-minute"),
            (1_800, "30m", "30-minute"),
            (45, "45s", "45-second"),
        ] {
            assert_eq!(limit_column("any", Some(length), None), column, "{length}");
            assert_eq!(limit_name("any", Some(length)), sentence, "{length}");
        }
    }

    /// A reading taken before the length was kept names its limit the way it always did,
    /// and so does one whose length makes no sense.
    #[test]
    fn a_limit_of_unknown_length_is_named_from_its_kind() {
        assert_eq!(limit_name("session", None), "5-hour");
        assert_eq!(limit_name("five_hour", Some(0)), "5-hour");
        assert_eq!(limit_name("weekly_all", None), "weekly");
        assert_eq!(limit_name("seven_day", None), "weekly");
        assert_eq!(limit_name("primary_window", None), "primary window");
        assert_eq!(limit_column("session", None, None), "5h");
        assert_eq!(limit_column("seven_day", Some(-1), None), "week");
        assert_eq!(limit_column("primary", None, None), "primary");
        assert_eq!(
            limit_column("weekly_scoped", None, Some("Fable")),
            "week · Fable"
        );
        assert_eq!(
            limit_column("weekly_scoped", Some(604_800), Some("Fable")),
            "week · Fable"
        );
    }

    /// A limit changes colour at 70% and at 90% as its figure says them, and stays red past
    /// 100%. The steps are where its colour changes, so each side of each is checked.
    #[test]
    fn a_limits_level_changes_at_seventy_and_at_ninety() {
        assert_eq!(usage_level(0.0), UsageLevel::Plenty);
        assert_eq!(usage_level(69.4), UsageLevel::Plenty);
        assert_eq!(usage_level(69.5), UsageLevel::Low);
        assert_eq!(usage_level(89.4), UsageLevel::Low);
        assert_eq!(usage_level(89.5), UsageLevel::Out);
        assert_eq!(usage_level(100.0), UsageLevel::Out);
        assert_eq!(usage_level(130.0), UsageLevel::Out);
    }

    /// The column beside a bar says how long until a limit resets in the largest units that
    /// stay true, and once that moment has come, that it is resetting.
    #[test]
    fn a_reset_is_said_in_the_largest_units_that_fit() {
        const NOW: i64 = 1_789_935_000;
        let resets = |left: i64| resets(NOW + left, NOW);
        assert_eq!(resets(-5), "resetting now");
        assert_eq!(resets(0), "resetting now");
        assert_eq!(resets(1), "resets in <1m");
        assert_eq!(resets(59), "resets in <1m");
        assert_eq!(resets(125), "resets in 2m");
        assert_eq!(resets(3599), "resets in 59m");
        assert_eq!(resets(3600), "resets in 1h 00m");
        assert_eq!(resets(3600 + 5 * 60), "resets in 1h 05m");
        assert_eq!(resets(3 * 3600 + 25 * 60), "resets in 3h 25m");
        assert_eq!(resets(86_399), "resets in 23h 59m");
        assert_eq!(resets(86_400), "resets in 1d 0h");
        assert_eq!(resets(2 * 86_400 + 4 * 3600 + 59 * 60), "resets in 2d 4h");
    }

    /// A limit in a sentence about an automatic switch has the figure it is drawn with.
    #[test]
    fn a_share_of_a_limit_is_said_as_it_is_drawn() {
        let limit = |percent: f64| crate::usage::Window {
            kind: "session".into(),
            scope: None,
            percent,
            resets_at: None,
            is_active: true,
            severity: None,
            length_seconds: Some(5 * HOUR),
        };
        assert_eq!(share_of_limit(&limit(94.5)), "95% of its 5-hour limit");
        assert_eq!(share_of_limit(&limit(96.4)), "96% of its 5-hour limit");
    }

    /// A pace beside its bar is how far from even it is, said so that over reads as the
    /// warning it is: "ahead" reads as good news.
    #[test]
    fn a_pace_is_said_as_over_under_or_on() {
        let pace = |delta: f64, standing| Pace {
            expected: 40.0,
            delta,
            standing,
        };
        use crate::usage::pace::Standing;
        assert_eq!(
            pace_column(&pace(27.4, Standing::Over { runs_out_in: 9_000 })),
            "27% over pace"
        );
        assert_eq!(pace_column(&pace(-12.6, Standing::Under)), "13% under pace");
        assert_eq!(pace_column(&pace(3.0, Standing::Even)), "on pace");
        assert_eq!(pace_column(&pace(-4.0, Standing::Even)), "on pace");
    }

    /// The limit that runs out first is named in the sentence, with its model where it has
    /// one, and under a minute the sentence says it is about to.
    #[test]
    fn a_limit_running_out_is_named_with_when() {
        assert_eq!(
            runs_out("weekly", 19 * 3600 + 5 * 60),
            "weekly limit runs out in 19h 05m at this pace"
        );
        assert_eq!(
            runs_out(
                &scoped_limit_name("weekly_scoped", Some(WEEK), Some("Fable")),
                2 * DAY
            ),
            "weekly Fable limit runs out in 2d 0h at this pace"
        );
        assert_eq!(runs_out("5-hour", 30), "5-hour limit is about to run out");
        assert_eq!(runs_out("5-hour", 0), "5-hour limit is about to run out");
    }

    /// When a parked login stops working decides whether a switch to it will. The sentence
    /// form says nothing with nothing to go on, says a login past it has expired, and
    /// otherwise counts whole days left, one day in the singular.
    #[test]
    fn a_parked_logins_life_in_a_sentence_is_counted_in_whole_days() {
        const NOW: i64 = 1_789_935_000;
        let life = |left: i64| parked_life(Some(NOW + left), NOW);
        assert_eq!(parked_life(None, NOW), None);
        assert_eq!(life(-60).as_deref(), Some("Its parked login has expired"));
        assert_eq!(life(0).as_deref(), Some("Its parked login has expired"));
        assert_eq!(
            life(1).as_deref(),
            Some("Parked login good for under a day")
        );
        assert_eq!(
            life(86_399).as_deref(),
            Some("Parked login good for under a day")
        );
        assert_eq!(
            life(86_400).as_deref(),
            Some("Parked login good for 1 more day")
        );
        assert_eq!(
            life(2 * 86_400 - 1).as_deref(),
            Some("Parked login good for 1 more day")
        );
        assert_eq!(
            life(2 * 86_400).as_deref(),
            Some("Parked login good for 2 more days")
        );
        assert_eq!(
            life(30 * 86_400).as_deref(),
            Some("Parked login good for 30 more days")
        );
    }

    /// The column form beside "ready" in `pitboard status`: what is left of a parked login
    /// to the minute, and from the moment it is due to be renewed, that it expires.
    #[test]
    fn a_parked_logins_life_in_a_column_says_when_it_expires() {
        use crate::doctor::RENEW_WITHIN;
        const NOW: i64 = 1_789_935_000;
        let life = |left: i64| parked_life_column(NOW + left, NOW);
        assert_eq!(life(20 * 86_400), "good for 20d 0h");
        assert_eq!(life(RENEW_WITHIN), "good for 3d 0h");
        assert_eq!(life(RENEW_WITHIN - 1), "expires in 2d 23h");
        assert_eq!(life(3_900), "expires in 1h 05m");
        assert_eq!(life(30), "expires in <1m");
        assert_eq!(life(0), "expired");
        assert_eq!(life(-60), "expired");
    }

    /// Nothing due is the ordinary case, and it has to read as ordinary rather than as a
    /// failure to do anything. Otherwise the sentence counts what was renewed of what was
    /// due.
    #[test]
    fn a_renewal_run_says_what_it_did() {
        assert_eq!(renewal_note(0, 0), "No parked login was due.");
        assert_eq!(renewal_note(1, 1), "Renewed one.");
        assert_eq!(renewal_note(2, 2), "Renewed all 2.");
        assert_eq!(
            renewal_note(2, 1),
            "Renewed 1 of 2; the rest are tried again next time."
        );
        assert_eq!(
            renewal_note(1, 0),
            "1 due; none could be renewed this time."
        );
    }

    /// Warnings are counted as things worth looking at. A check that fails outweighs every
    /// warning: the sentence then counts only what is broken, and says not to switch.
    #[test]
    fn the_doctor_summary_says_not_to_switch_while_a_check_fails() {
        assert_eq!(
            doctor_summary([Level::Ok, Level::Ok]),
            "Everything Pitboard checks is in order."
        );
        assert_eq!(
            doctor_summary([]),
            "Everything Pitboard checks is in order."
        );
        assert_eq!(
            doctor_summary([Level::Ok, Level::Warn]),
            "One thing is worth looking at."
        );
        assert_eq!(
            doctor_summary([Level::Warn, Level::Ok, Level::Warn]),
            "2 things are worth looking at."
        );
        assert_eq!(
            doctor_summary([Level::Fail]),
            "1 broken: do not switch accounts until fixed."
        );
        assert_eq!(
            doctor_summary([Level::Warn, Level::Ok, Level::Fail]),
            "1 broken: do not switch accounts until fixed."
        );
        assert_eq!(
            doctor_summary([Level::Fail, Level::Warn, Level::Fail]),
            "2 broken: do not switch accounts until fixed."
        );
    }

    // Said once `abandon` has given up a switch, where "nothing has changed" is false.
    #[test]
    fn a_move_not_given_up_says_why_and_never_that_nothing_changed() {
        use crate::error::Error;
        use crate::provider::ProviderId;
        let tool = ProviderId::Desktop;
        let app = "Claude";
        let said = |why| move_not_given_up("desktop/work", &why);
        assert_eq!(
            said(Error::AppOpen {
                app,
                still: Vec::new()
            }),
            "Pitboard did not give up the interrupted move of `desktop/work`'s login, because \
             Claude is open. Quit Claude with Command-Q, then run `pitboard abandon` again."
        );
        assert_eq!(
            said(Error::MachineUnknown),
            "Pitboard did not give up the interrupted move of `desktop/work`'s login. It \
             stopped at `machine_unknown`: run `pitboard abandon` again to see why."
        );
        assert_eq!(
            said(crate::store::folder::unread(
                "desktop/work",
                std::path::Path::new("/Users/dana/.pitboard/parks/p/items/Cookies"),
                "a test held the jar"
            )),
            "Pitboard did not give up the interrupted move of `desktop/work`'s login, because \
             `desktop/work`'s parked login at /Users/dana/.pitboard/parks/p/items/Cookies could \
             not be looked at (a test held the jar). Make that path, and each folder it is in, \
             readable to you, then run `pitboard abandon` again."
        );
        for why in [
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
                label: Some("desktop/work".into()),
            },
            Error::MoveMixed {
                label: "desktop/work".into(),
                places: vec!["`Session Storage` in Claude's folder".into()],
            },
        ] {
            let sentence = said(why);
            assert!(sentence.contains(", because "), "{sentence}");
            assert!(
                !sentence.to_lowercase().contains("nothing has changed"),
                "{sentence}"
            );
        }
    }

    // Said by `repair`, which goes on past the move, so never that nothing has changed.
    #[test]
    fn a_move_not_settled_says_why_and_never_that_nothing_changed() {
        use crate::error::{Error, Then};
        use crate::provider::ProviderId;
        let tool = ProviderId::Desktop;
        let app = "Claude";
        let label = || "desktop/work".to_string();
        let said = |why| move_not_settled("desktop/work", &why);
        assert_eq!(
            said(Error::RecoveryUndetermined {
                tool,
                from: label(),
                to: label(),
                detail: "a settings file that is not valid JSON".into(),
                spelling: Spelling::Bare,
            }),
            "Pitboard did not settle the interrupted move of `desktop/work`'s login, because it \
             cannot tell where it stopped (a settings file that is not valid JSON). Open Claude \
             to see whether it is signed in and to which account, then quit it with Command-Q. \
             If it is signed in, run `pitboard enroll desktop/<label>` for the account it shows. \
             If it is signed out, `pitboard abandon` keeps every folder and moves on."
        );
        assert_eq!(
            said(Error::MachineUnknown),
            "Pitboard did not settle the interrupted move of `desktop/work`'s login, because it \
             stopped at `machine_unknown`. The next change to a Claude Desktop account settles \
             it first, or says why it cannot."
        );
        for why in [
            Error::AppOpen {
                app,
                still: Vec::new(),
            },
            Error::LoginFolderBusy { app },
            Error::RecoveryStopped {
                label: label(),
                detail: "`WebStorage` could not be moved".into(),
            },
            Error::RecoveryElsewhere {
                tool,
                from: label(),
                to: label(),
                slot: "/Volumes/Old/Claude".into(),
            },
            Error::LoginFolderUnknown {
                tool,
                what: "does not exist yet".into(),
                then: Then::OpenAndQuit,
            },
            Error::Store(crate::store::Error::Unreadable("denied".into())),
        ] {
            let sentence = said(why);
            assert!(sentence.contains(", because "), "{sentence}");
            assert!(sentence.ends_with('.'), "{sentence}");
            let lower = sentence.to_lowercase();
            assert!(
                !lower.contains("nothing has changed") && !lower.contains("nothing was changed"),
                "{sentence}"
            );
        }
    }
}
