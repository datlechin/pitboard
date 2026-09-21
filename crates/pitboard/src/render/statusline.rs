//! `pitboard statusline` as the one line Claude Code draws.

use crate::ui::{self, BOLD, DIM, paint};
use pitboard_core::statusline::{Shares, StatusLine};
use pitboard_core::time;

fn shares(shares: Shares) -> String {
    let one = |share: Option<f64>| match share {
        Some(p) => paint(ui::level(p), format!("{p:.0}%")),
        None => paint(DIM, "?"),
    };
    format!(
        "{}{}{}",
        one(shares.five_hour),
        paint(DIM, "·"),
        one(shares.weekly)
    )
}

pub fn human(line: &StatusLine) -> String {
    let mut parts = vec![format!(
        "{} {}",
        paint(BOLD, line.current.as_deref().unwrap_or("unenrolled")),
        shares(line.session)
    )];
    for other in &line.others {
        let age = other
            .age
            .map(|seconds| paint(DIM, format!(" ({})", time::span(seconds))))
            .unwrap_or_default();
        parts.push(format!(
            "{} {}{age}",
            paint(DIM, &other.label),
            shares(other.shares)
        ));
    }
    parts.join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pitboard_core::statusline::Entry;

    fn shares(five_hour: f64, weekly: f64) -> Shares {
        Shares {
            five_hour: Some(five_hour),
            weekly: Some(weekly),
        }
    }

    fn plain(styled: &str) -> String {
        anstream::adapter::strip_str(styled).to_string()
    }

    #[test]
    fn reads_as_the_account_in_use_then_the_others() {
        let line = StatusLine {
            current: Some("work".into()),
            session: shares(46.4, 70.0),
            others: vec![
                Entry {
                    label: "personal".into(),
                    shares: shares(12.0, 40.0),
                    age: None,
                },
                Entry {
                    label: "side".into(),
                    shares: Shares::default(),
                    age: Some(3 * 3_600),
                },
            ],
        };
        assert_eq!(
            plain(&human(&line)),
            "work 46%·70%  personal 12%·40%  side ?·? (3h 00m)"
        );
    }

    #[test]
    fn an_account_not_enrolled_is_named_as_such() {
        let line = StatusLine {
            current: None,
            session: Shares::default(),
            others: Vec::new(),
        };
        assert_eq!(plain(&human(&line)), "unenrolled ?·?");
    }
}
