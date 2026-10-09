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
use crate::pace::{Pace, Standing};
use crate::usage::whole;

const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
const WEEK: i64 = 7 * DAY;

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

/// How much of a limit an account has used, in a sentence about an automatic switch:
/// "96% of its 5-hour limit", or "97% of its weekly Opus limit" for one limit of a model.
pub fn share_of_limit(limit: &crate::usage::Window) -> String {
    let name = scoped_limit_name(&limit.kind, limit.length_seconds, limit.scope.as_deref());
    format!("{}% of its {name} limit", whole(limit.percent))
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
        said.push_str(&format!(": {used}"));
    }
    if let Some(as_of) = as_of {
        said.push_str(&format!(", as of {as_of}"));
    }
    said.push('.');
    if let Some(until) = held_until {
        said.push_str(&format!(
            " Anthropic holds Pitboard off asking again until {until}."
        ));
    }
    said
}

/// Why Pitboard does not switch Claude Code away from a limit at `threshold`, as a clause
/// that says what to do about it where anything can be done. It names no command, and no
/// time, which each front end says in its own way: the app says it too.
pub fn not_switching(
    why: &crate::autoswitch::Skip,
    threshold: crate::autoswitch::Threshold,
) -> String {
    use crate::autoswitch::{ATTEMPTS, SETTLING_SECONDS, Skip};
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

/// Why Pitboard cannot judge whether to switch Claude Code by itself, as a clause that says
/// what to do about it where anything can be done. It names no command, and no time, which
/// each front end says in its own way.
pub fn not_watching(why: &crate::autoswitch::Blind) -> String {
    use crate::autoswitch::Blind;
    match why {
        Blind::SwitchInterrupted => {
            "a switch was interrupted, and the next change you make finishes it".into()
        }
        Blind::CustomOauth => "Claude Code uses a custom OAuth endpoint, and Pitboard does not \
                               act on Claude Code then"
            .into(),
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

/// What sessions already running do after a switch while a file sits behind the store they
/// read, as the register's `fallback_file_pins_session_login` reads Claude Code 2.1.294. The
/// rest of the sentence, and its subject, are each place's own.
pub fn kept_until_renewed() -> &'static str {
    "keep the account they are on until their login is next renewed, or until they are \
     started again"
}

/// What is said where nothing is left behind the store Claude Code keeps its login in, as
/// `pitboard stow` and the app's sheet say it. On Linux the file is that store.
pub fn nothing_left() -> &'static str {
    "Nothing is left behind the store Claude Code keeps its login in, so there is nothing to \
     put away."
}

/// What `pitboard stow` and the app's sheet say of the file behind Claude Code's store before
/// it is put away, a paragraph each: what it holds and what putting it away would do with
/// that, the keys that would go with it, and what becomes of Claude Code's sessions once it is
/// gone. Of a login of an account nobody enrolled, whose it is and what to do first.
pub fn left_lines(left: &crate::switch::Left) -> Vec<String> {
    use crate::switch::{Foreseen, Kept};
    if let Foreseen::NotEnrolled(_) = left.login {
        return vec![left_login(left)];
    }
    let held_a_login = left.login != Foreseen::Kept(Kept::NoLogin);
    std::iter::once(left_login(left))
        .chain(dropped_keys(&left.dropped, false))
        .chain(std::iter::once(once_stowed(held_a_login)))
        .collect()
}

/// What they say once it is put away, a paragraph each.
pub fn stowed_lines(stowed: &crate::switch::Stowed) -> Vec<String> {
    let held_a_login = stowed.kept != crate::switch::Kept::NoLogin;
    std::iter::once(stowed_login(stowed))
        .chain(dropped_keys(&stowed.dropped, true))
        .chain(std::iter::once(once_stowed(held_a_login)))
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
            "{path} holds `{label}`'s login, and Pitboard holds no login of `{label}` it can \
             switch to. Putting it away parks this one for `{label}`, then deletes the file."
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

/// The other keys the file holds, which go with it unmoved, by name and how many: `None`
/// where it holds none. `gone` once the file has gone.
fn dropped_keys(keys: &[String], gone: bool) -> Option<String> {
    let one = match keys.len() {
        0 => return None,
        n => n == 1,
    };
    let names = listed(keys.iter().map(|key| format!("`{key}`")).collect());
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
         {them}, and Claude Code's document in the keychain keeps its own."
    ))
}

/// What becomes of Claude Code's sessions once the file is gone: each takes the login stored
/// again, and one that signed in with a login in the file, as one over SSH does, is signed
/// out.
fn once_stowed(held_a_login: bool) -> String {
    let follow = format!(
        "Once the file is gone, Claude Code sessions already running follow a switch within \
         {} seconds again",
        crate::switch::ADOPTION_CEILING_SECONDS
    );
    if held_a_login {
        format!(
            "{follow}, and one that signed in with its login, such as one over SSH, is signed \
             out."
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
pub fn login(tool: crate::provider::ProviderId, email: &str, shares_email: bool) -> String {
    use crate::provider::ProviderId;
    if !shares_email {
        return email.to_owned();
    }
    let group = match tool {
        ProviderId::Claude => "organisation",
        ProviderId::Codex => "workspace",
    };
    format!("{email} in another {group}")
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

// On macOS and Linux root and sudo share one refusal, since the way out is the same.
pub(crate) fn elevated(os: Os, why: Option<&'static str>) -> ChangesNothing {
    let because = |cannot_tell: &str| match why {
        Some(why) => format!("Pitboard runs {why}"),
        None => format!("Pitboard cannot tell whether it runs {cannot_tell}"),
    };
    match os {
        Os::MacOs | Os::Linux => ChangesNothing {
            refusal: match why {
                Some(_) => "Pitboard changes nothing when it runs as root or with sudo. Run it as \
                            yourself."
                    .into(),
                None => "Pitboard changes nothing when it cannot tell whether it runs as root or \
                         with sudo. Run it as yourself."
                    .into(),
            },
            because: because("as root or with sudo"),
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

/// Things one after another, as a sentence lists them: "a", "a and b", "a, b and c".
pub(crate) fn listed(mut items: Vec<String>) -> String {
    match items.len() {
        0 => String::new(),
        1 => items.remove(0),
        _ => {
            let last = items.pop().unwrap_or_default();
            format!("{} and {last}", items.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Why Pitboard did not switch by itself is said in the app as well as in a terminal,
    /// so it names no command: the app gives up on an interrupted switch with a button.
    #[test]
    fn why_it_did_not_switch_names_no_command() {
        use crate::autoswitch::{Blind, Skip, Threshold};
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

    #[test]
    fn root_and_sudo_are_told_to_run_it_as_themselves() {
        for os in [Os::MacOs, Os::Linux] {
            for why in ["as root", "with sudo"] {
                let said = elevated(os, Some(why));
                assert_eq!(
                    said.refusal,
                    "Pitboard changes nothing when it runs as root or with sudo. Run it as \
                     yourself."
                );
                assert_eq!(said.because, format!("Pitboard runs {why}"));
                assert_eq!(said.way_out, "Run it as yourself.");
                assert_eq!(said.to_ask_again, "Run it as yourself to ask again.");
            }
            let unknown = elevated(os, None);
            assert_eq!(
                unknown.refusal,
                "Pitboard changes nothing when it cannot tell whether it runs as root or with \
                 sudo. Run it as yourself."
            );
            assert_eq!(
                unknown.because,
                "Pitboard cannot tell whether it runs as root or with sudo"
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
        let pace = |delta: f64, standing| crate::pace::Pace {
            expected: 40.0,
            delta,
            standing,
        };
        use crate::pace::Standing;
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
}
