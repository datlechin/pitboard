//! What the presentation's tests share: accounts and limits as the core reports them, and a
//! person's clock that a test can read, in UTC.

use super::{AccountItem, RowEnd};
use crate::model::state::{Cadence, State};
use crate::model::{LocalTime, PlatformError};
use crate::{Account, Awaiting, Limit, Parked, Status, Tool, Usage, Warning};
use pitboard_core::provider::ProviderId;
use pitboard_core::status::{Stale, Standing};

impl AccountItem {
    // A switch to it is under way, as its row's end shows.
    pub(crate) fn switching(&self) -> bool {
        matches!(self.end, Some(RowEnd::Switching { .. }))
    }
}

pub(crate) fn claude_code() -> Tool {
    crate::tool(ProviderId::Claude)
}

pub(crate) fn codex() -> Tool {
    crate::tool(ProviderId::Codex)
}

pub(crate) fn both_tools() -> Vec<Tool> {
    vec![claude_code(), codex()]
}

pub(crate) fn claude_desktop() -> Tool {
    crate::tool(ProviderId::Desktop)
}

// Every tool a Mac lists.
pub(crate) fn desktop() -> Vec<Tool> {
    vec![claude_code(), codex(), claude_desktop()]
}

// A Claude Desktop account as the core reports one: no email and no usage, ever.
pub(crate) fn desktop_account(label: &str, signed_in: bool) -> Account {
    Account {
        email: String::new(),
        stale: Some(Stale::NoUsage),
        stale_explanation: Some("Pitboard reads no usage for Claude Desktop".into()),
        ..account(Some(label))
            .of("desktop")
            .unmeasured()
            .signed_in_if(signed_in)
            .build()
    }
}

// A warning about Claude Desktop, as the core says one.
pub(crate) fn desktop_warning(code: &str, message: &str) -> Warning {
    Warning {
        code: code.into(),
        message: message.into(),
        account: None,
        held: None,
        provider: Some("desktop".into()),
    }
}

// An add or a Sign In Again inside Claude left halfway, `put_back` parked for it.
pub(crate) fn awaiting(label: &str, put_back: &str, again: bool) -> Awaiting {
    Awaiting {
        provider: "desktop".into(),
        label: label.into(),
        put_back: format!("desktop/{put_back}"),
        again,
    }
}

// What a read of `status` leaves the model with, `found` installed.
pub(crate) fn read(status: Status, found: Vec<Tool>) -> State {
    let mut state = State::new(Cadence::APP);
    state.warnings = status.warnings.clone();
    state.status = Some(status);
    state.installed = Some(found);
    state
}

pub(crate) fn status(accounts: Vec<Account>, warnings: Vec<Warning>) -> Status {
    Status {
        now: 0,
        accounts,
        warnings,
        awaiting: Vec::new(),
    }
}

/// A limit as the core reports one, resetting at 100 unless a test says otherwise.
pub(crate) fn window(kind: &str, percent: f64) -> Limit {
    Limit {
        kind: kind.into(),
        length_seconds: None,
        scope: None,
        percent,
        resets_at: Some(100),
        severity: None,
        is_active: true,
    }
}

/// What a test changes of a limit.
pub(crate) trait LimitExt {
    fn scope(self, scope: &str) -> Limit;
    fn active(self, active: bool) -> Limit;
    fn length(self, seconds: i64) -> Limit;
    fn resets(self, at: Option<i64>) -> Limit;
}

impl LimitExt for Limit {
    fn scope(self, scope: &str) -> Limit {
        Limit {
            scope: Some(scope.into()),
            ..self
        }
    }

    fn active(self, active: bool) -> Limit {
        Limit {
            is_active: active,
            ..self
        }
    }

    fn length(self, seconds: i64) -> Limit {
        Limit {
            length_seconds: Some(seconds),
            ..self
        }
    }

    fn resets(self, at: Option<i64>) -> Limit {
        Limit {
            resets_at: at,
            ..self
        }
    }
}

// Where a test's account stands unless the test says: ready to switch to, where it is enrolled.
pub(crate) fn standing_of(signed_in: bool, named: bool) -> Standing {
    match (signed_in, named) {
        (true, true) => Standing::InUse,
        (true, false) => Standing::NotEnrolled,
        (false, true) => Standing::Ready,
        (false, false) => Standing::NothingParked,
    }
}

/// An account as the core reports one, a Claude Code one not signed in unless a test says
/// otherwise. `None` is a login signed in and not enrolled. Ready to switch to unless it is the
/// one signed in, and its numbers every limit it has where its tool is Claude Code, as a real
/// one is.
pub(crate) fn account(label: Option<&str>) -> AccountMade {
    AccountMade {
        label: label.map(str::to_owned),
        provider: "claude".into(),
        signed_in: false,
        standing: None,
        uuid: None,
        windows: Vec::new(),
        measured: true,
        read_at: 0,
        parked: None,
        explanation: None,
    }
}

/// An account being made.
pub(crate) struct AccountMade {
    label: Option<String>,
    provider: String,
    signed_in: bool,
    standing: Option<Standing>,
    uuid: Option<String>,
    windows: Vec<Limit>,
    measured: bool,
    /// When its numbers were read.
    read_at: i64,
    parked: Option<Parked>,
    explanation: Option<String>,
}

impl AccountMade {
    pub(crate) fn of(self, provider: &str) -> AccountMade {
        AccountMade {
            provider: provider.into(),
            ..self
        }
    }

    pub(crate) fn signed_in(self) -> AccountMade {
        AccountMade {
            signed_in: true,
            ..self
        }
    }

    fn signed_in_if(self, signed_in: bool) -> AccountMade {
        AccountMade { signed_in, ..self }
    }

    pub(crate) fn standing(self, standing: Standing) -> AccountMade {
        AccountMade {
            standing: Some(standing),
            ..self
        }
    }

    pub(crate) fn uuid(self, uuid: &str) -> AccountMade {
        AccountMade {
            uuid: Some(uuid.into()),
            ..self
        }
    }

    pub(crate) fn limits(self, windows: Vec<Limit>) -> AccountMade {
        AccountMade { windows, ..self }
    }

    /// With its numbers read at `at`.
    pub(crate) fn read_at(self, at: i64) -> AccountMade {
        AccountMade {
            read_at: at,
            ..self
        }
    }

    /// With no numbers at all.
    pub(crate) fn unmeasured(self) -> AccountMade {
        AccountMade {
            measured: false,
            ..self
        }
    }

    /// With its parked login good until `refresh_expires_at`.
    pub(crate) fn parked_until(self, refresh_expires_at: i64) -> AccountMade {
        AccountMade {
            parked: Some(Parked {
                refresh_expires_at: Some(refresh_expires_at),
                session_expires_at: None,
            }),
            ..self
        }
    }

    /// With what to tell a person about its numbers, or why it cannot be used.
    pub(crate) fn explained(self, explanation: &str) -> AccountMade {
        AccountMade {
            explanation: Some(explanation.into()),
            ..self
        }
    }

    pub(crate) fn build(self) -> Account {
        let uuid = self
            .uuid
            .or_else(|| self.label.clone())
            .unwrap_or_else(|| "someone".into());
        Account {
            id: format!("{}:{uuid}", self.provider),
            qualified: self
                .label
                .as_ref()
                .map(|label| format!("{}/{label}", self.provider)),
            unplaced: false,
            email: format!("{}@example.com", self.label.as_deref().unwrap_or(&uuid)),
            account_id: uuid,
            signed_in: self.signed_in,
            standing: self
                .standing
                .unwrap_or_else(|| standing_of(self.signed_in, self.label.is_some())),
            parked: self.parked,
            usage: self.measured.then_some(Usage {
                observed_at: Some(self.read_at),
                windows: self.windows,
                lists_every_limit: self.provider == "claude",
            }),
            stale: None,
            stale_explanation: self.explanation,
            plan: None,
            read_at: None,
            label: self.label,
            provider: self.provider,
        }
    }
}

/// A tool's login that belongs to no account Pitboard can name, as the core reports one: no
/// label, no email, no account id, and what is wrong with it.
pub(crate) fn unplaced(provider: &str, signed_in: bool) -> Account {
    Account {
        id: format!("{provider}:login"),
        provider: provider.into(),
        label: None,
        qualified: None,
        unplaced: true,
        email: String::new(),
        account_id: String::new(),
        signed_in,
        standing: Standing::Unplaced,
        parked: None,
        usage: None,
        stale: Some(Stale::LoginUnreadable),
        stale_explanation: Some("Codex's login could not be read; run `pitboard doctor`".into()),
        plan: None,
        read_at: None,
    }
}

/// A person's clock in UTC, in 24 hours: "14:05", and "Wed 14:05" with its weekday. A day is
/// a day of UTC.
pub(crate) struct Utc;

const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];

impl LocalTime for Utc {
    fn clock(&self, epoch: i64, with_weekday: bool) -> Result<String, PlatformError> {
        let of_day = epoch.rem_euclid(86_400);
        let time = format!("{:02}:{:02}", of_day / 3600, of_day % 3600 / 60);
        if with_weekday {
            let weekday = WEEKDAYS[usize::try_from(epoch.div_euclid(86_400).rem_euclid(7))
                .expect("a day of the week")];
            Ok(format!("{weekday} {time}"))
        } else {
            Ok(time)
        }
    }

    fn same_day(&self, first: i64, second: i64) -> Result<bool, PlatformError> {
        Ok(first.div_euclid(86_400) == second.div_euclid(86_400))
    }

    fn date_and_time(&self, epoch: i64) -> Result<String, PlatformError> {
        self.clock(epoch, true)
    }

    fn time_of_day(&self, minutes: u16) -> Result<String, PlatformError> {
        Ok(format!("{:02}:{:02}", minutes / 60, minutes % 60))
    }
}

/// A person's clock that the app's own code cannot read, as one whose formatter throws.
pub(crate) struct Unreadable;

impl LocalTime for Unreadable {
    fn clock(&self, _epoch: i64, _with_weekday: bool) -> Result<String, PlatformError> {
        Err(PlatformError::Failed {
            reason: "no formatter".into(),
        })
    }

    fn same_day(&self, _first: i64, _second: i64) -> Result<bool, PlatformError> {
        Err(PlatformError::Failed {
            reason: "no calendar".into(),
        })
    }

    fn date_and_time(&self, _epoch: i64) -> Result<String, PlatformError> {
        Err(PlatformError::Failed {
            reason: "no formatter".into(),
        })
    }

    fn time_of_day(&self, _minutes: u16) -> Result<String, PlatformError> {
        Err(PlatformError::Failed {
            reason: "no formatter".into(),
        })
    }
}

// Every string in `shown`'s `{:?}`, but ids and codes: what a person may read of it.
pub(crate) fn strings_of(shown: &impl std::fmt::Debug) -> Vec<String> {
    strings_in(&format!("{shown:?}"))
}

// Every string quoted in `debugged`, as `{:?}` writes one, but ids and codes.
pub(crate) fn strings_in(debugged: &str) -> Vec<String> {
    let mut strings = Vec::new();
    let mut chars = debugged.chars();
    while chars.by_ref().any(|c| c == '"') {
        let mut string = String::new();
        while let Some(c) = chars.next() {
            match c {
                '\\' => string.extend(chars.next()),
                '"' => break,
                c => string.push(c),
            }
        }
        let code = !string.contains(' ')
            && string.contains(['_', '/', ':'])
            && !string.contains(char::is_uppercase);
        if !code {
            strings.push(string);
        }
    }
    strings
}

// Each capital past a sentence's first word that is none of `names`, which sentence case has not.
pub(crate) fn title_cased(said: &str, names: &[&str]) -> Vec<String> {
    let words: Vec<&str> = said.split_whitespace().collect();
    words
        .windows(2)
        .filter(|pair| !pair[0].ends_with(['.', '?', '!', ':']))
        .map(|pair| pair[1].trim_matches(|c: char| !c.is_alphanumeric()))
        .map(|word| word.strip_suffix("’s").unwrap_or(word))
        .filter(|word| word.starts_with(char::is_uppercase) && !names.contains(word))
        .map(str::to_owned)
        .collect()
}

// The core's words for how it works, which the apps leave to the command line and its docs.
pub(crate) fn mechanism_words(text: &str) -> Vec<String> {
    const WORDS: [&str; 22] = [
        "park",
        "parks",
        "parked",
        "parking",
        "enrol",
        "enrols",
        "enrolled",
        "enrolling",
        "enrolment",
        "enroll",
        "record",
        "records",
        "recorded",
        "recording",
        "settle",
        "settles",
        "settled",
        "settling",
        "forget",
        "forgets",
        "forgotten",
        "forgetting",
    ];
    const PUT: [&str; 3] = ["put", "puts", "putting"];
    const GIVE: [&str; 5] = ["give", "gives", "gave", "given", "giving"];
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !(c.is_alphanumeric() || c == '-'))
        .filter(|word| !word.is_empty())
        .collect();
    let mut found: Vec<String> = (words.iter())
        .filter(|word| WORDS.contains(word))
        .map(|word| (*word).to_owned())
        .collect();
    for pair in words.windows(2) {
        let phrase = match pair {
            [verb, "away"] => PUT.contains(verb),
            [verb, "up"] => GIVE.contains(verb),
            _ => false,
        };
        if phrase {
            found.push(pair.join(" "));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The clock a test reads: 1970 began on a Thursday, and 14 January 2026 at noon UTC is a
    /// Wednesday.
    #[test]
    fn the_tests_clock_reads_utc() {
        assert_eq!(Utc.clock(0, true).ok().as_deref(), Some("Thu 00:00"));
        assert_eq!(
            Utc.clock(1_768_392_000, true).ok().as_deref(),
            Some("Wed 12:00")
        );
        assert_eq!(
            Utc.clock(1_768_392_000 + 3725, false).ok().as_deref(),
            Some("13:02")
        );
        assert_eq!(Utc.same_day(1_768_392_000, 1_768_435_199).ok(), Some(true));
        assert_eq!(Utc.same_day(1_768_392_000, 1_768_435_200).ok(), Some(false));
    }
}
