//! What the presentation's tests share: accounts and limits as the core reports them, made
//! the way the Swift tests' Fixtures.swift made them, and a person's clock that a test can
//! read, in UTC.

use crate::model::{LocalTime, PlatformError};
use crate::{Account, Limit, Parked, Source, Tool, Usage};
use pitboard_core::provider::ProviderId;

pub(crate) fn claude_code() -> Tool {
    crate::tool(ProviderId::Claude)
}

pub(crate) fn codex() -> Tool {
    crate::tool(ProviderId::Codex)
}

pub(crate) fn both_tools() -> Vec<Tool> {
    vec![claude_code(), codex()]
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

/// An account as the core reports one, a Claude Code one not signed in unless a test says
/// otherwise. `None` is a login signed in and not enrolled. Switchable unless it is the one
/// signed in, and its numbers every limit it has where its tool is Claude Code, as a real
/// one is.
pub(crate) fn account(label: Option<&str>) -> AccountMade {
    AccountMade {
        label: label.map(str::to_owned),
        provider: "claude".into(),
        signed_in: false,
        switchable: None,
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
    switchable: Option<bool>,
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

    pub(crate) fn switchable(self, switchable: bool) -> AccountMade {
        AccountMade {
            switchable: Some(switchable),
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

    /// With its numbers read at `at`, which is what a limit's pace is as of.
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
                parked_at: 0,
                access_expires_at: None,
                refresh_expires_at: Some(refresh_expires_at),
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
            switchable: self
                .switchable
                .unwrap_or(!self.signed_in && self.label.is_some()),
            parked: self.parked,
            usage: self.measured.then_some(Usage {
                source: Source::Live,
                observed_at: Some(self.read_at),
                windows: self.windows,
                lists_every_limit: self.provider == "claude",
            }),
            stale: None,
            stale_explanation: self.explanation,
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
        switchable: false,
        parked: None,
        usage: None,
        stale: Some("login_unreadable".into()),
        stale_explanation: Some("Codex's login could not be read; run `pitboard doctor`".into()),
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
