//! What both apps say and the command line does not, each sentence a function of typed values,
//! as `pitboard_core::words` holds what all three say. English, for now: a sentence is never
//! a format string a view fills in, so each can read a translation later by its name.
//!
//! A catalogue, a file for each surface that says the words: the menu bar's item, the accounts
//! and their limits, notices and notifications, sheets and questions, setting up, Settings and
//! Activity, Diagnostics, the account windows, failures, and the core's refusals as the apps say
//! them. Here are the tools every surface words with, and the words several share.
//!
//! What the command line says too is not here but there, and called from here: a limit's
//! names, a reset, a parked login's life. Each of its sentences an app shows passes
//! `typeset` first.

mod accounts;
mod checks;
mod failures;
mod machine;
mod notices;
mod refusals;
mod setup;
mod sheets;
mod status_item;
mod windows;

pub(crate) use accounts::*;
pub(crate) use checks::*;
pub(crate) use failures::*;
pub(crate) use machine::*;
pub(crate) use notices::*;
pub(crate) use refusals::*;
pub(crate) use setup::*;
pub(crate) use sheets::*;
pub(crate) use status_item::*;
pub(crate) use windows::*;

use crate::Front;
use pitboard_core::host::SecretStore;
use pitboard_core::time::{DAY, HOUR, MINUTE};
use pitboard_core::words::capitalised;
use std::ops::Range;

// macOS title-cases a control or a title, as Apple's guidelines do; Windows sentence-cases all.
pub(crate) fn titled(front: Front, title_case: &str, sentence_case: &str) -> String {
    debug_assert!(
        cased_alike(title_case, sentence_case),
        "{title_case:?} and {sentence_case:?} differ in more than case"
    );
    match front {
        Front::WindowsNotificationArea => sentence_case,
        Front::MacMenuBar | Front::Gnome => title_case,
    }
    .to_owned()
}

fn cased_alike(title_case: &str, sentence_case: &str) -> bool {
    let sentence = sentence_case.to_lowercase();
    let title = title_case.to_lowercase();
    title == sentence || title.strip_suffix('…') == Some(&sentence)
}

// "1 sign-in", "2 sign-ins".
pub(crate) fn sign_ins(count: u32) -> String {
    if count == 1 {
        "1 sign-in".into()
    } else {
        format!("{count} sign-ins")
    }
}

// Its first letter lower case, as a word after the start of a sentence: "resets 3:45 PM".
fn uncapitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(chars).collect()
    })
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

// A day where the person's calendar could not say it, as `utc_clock` says its time: "Nov 20 UTC".
pub(crate) fn utc_date(epoch: i64) -> String {
    jiff::Timestamp::from_second(epoch)
        .map(|at| {
            at.to_zoned(jiff::tz::TimeZone::UTC)
                .strftime("%b %-d UTC")
                .to_string()
        })
        .unwrap_or_default()
}

// The store a sign-in file is left behind. Claude Code leaves none on Linux, so there only a
// fixture's machine, whose keychain is in memory, shows these words.
fn store_in_front(front: Front) -> &'static str {
    pitboard_core::words::store_in_front(match front {
        Front::MacMenuBar | Front::Gnome => SecretStore::Keychain,
        Front::WindowsNotificationArea => SecretStore::CredentialManager,
    })
}

/// How a sentence names the machine the app runs on, as the account windows' alerts do.
pub(crate) fn this_machine(front: Front) -> &'static str {
    crate::account_windows::this_machine(front)
}

// The core's words less each sentence, or clause, naming a pitboard command a terminal runs.
fn without_commands(core: &str) -> String {
    // A bare `pitboard` may be a path, but after "run" or "with".
    let names_one = |part: &str| {
        part.contains("`pitboard ")
            || part.match_indices("`pitboard`").any(|(at, _)| {
                let before = part[..at]
                    .split_whitespace()
                    .next_back()
                    .unwrap_or_default();
                matches!(before.to_lowercase().as_str(), "run" | "running" | "with")
            })
    };
    let ends_sentence = |c, next: Option<char>| {
        matches!(c, '.' | '!' | '?') && next.is_none_or(char::is_whitespace)
    };
    let mut kept = String::with_capacity(core.len());
    for said in pieces(core, ends_sentence) {
        if !names_one(said) {
            kept.push_str(said);
            continue;
        }
        let before: String = pieces(said, |c, next| c == ';' && next == Some(' '))
            .into_iter()
            .take_while(|clause| !names_one(clause))
            .collect();
        if !before.is_empty() {
            kept.push_str(before.trim_end().trim_end_matches(';'));
            kept.push_str(". ");
        }
    }
    kept.trim_end().to_owned()
}

// `text` cut after each character `ends` says ends a piece, outside backticks.
fn pieces(text: &str, ends: impl Fn(char, Option<char>) -> bool) -> Vec<&str> {
    let mut found = Vec::new();
    let (mut start, mut quoted) = (0, false);
    let mut chars = text.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        quoted ^= c == '`';
        if quoted || !ends(c, chars.peek().map(|&(_, next)| next)) {
            continue;
        }
        while chars.next_if(|&(_, next)| next.is_whitespace()).is_some() {}
        let end = chars.peek().map_or(text.len(), |&(next, _)| next);
        found.push(&text[start..end]);
        start = end;
    }
    if start < text.len() {
        found.push(&text[start..]);
    }
    found
}

// The apps' typography for the core's words, where what the core quotes keeps every letter.
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

// What the core quotes without backticks: a path or link, an address, a system's own message.
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

// The core's words as an app shows them: without a terminal's backticks or a command to run.
pub(crate) fn sentence(text: &str) -> String {
    let text = typeset(&prose_capitalised(&without_commands(text)));
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

pub(crate) const SWITCH: &str = "Switch";

pub(crate) const PITBOARD: &str = "Pitboard";

pub(crate) const CONTINUE: &str = "Continue";

pub(crate) fn try_again(front: Front) -> String {
    titled(front, "Try Again", "Try again")
}

pub(crate) const REFRESH: &str = "Refresh";

pub(crate) fn learn_more(front: Front) -> String {
    titled(front, "Learn More", "Learn more")
}

pub(crate) const CANCEL: &str = "Cancel";

#[cfg(test)]
mod tests {
    use super::*;

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
}
