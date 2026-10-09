//! `pitboard statusline` as the one line Claude Code draws.

use crate::ui::{self, BOLD, DIM, paint};
use pitboard_core::pace::Standing;
use pitboard_core::statusline::{Paces, Shares, StatusLine};
use pitboard_core::usage::whole;
use pitboard_core::words;

/// A limit's share, and where its pace is said, a triangle for it: rising over pace,
/// falling under, in the colours `pitboard status` says them in.
fn share(share: Option<f64>, pace: Option<Standing>) -> String {
    let Some(p) = share else {
        return paint(DIM, "?");
    };
    let mark = match pace {
        Some(standing @ Standing::Over { .. }) => paint(ui::pace(standing), "▲"),
        Some(standing @ Standing::Under) => paint(ui::pace(standing), "▼"),
        Some(Standing::Even) | None => String::new(),
    };
    format!("{}{mark}", paint(ui::level(p), format!("{}%", whole(p))))
}

fn shares(shares: Shares, pace: Paces) -> String {
    format!(
        "{}{}{}",
        share(shares.five_hour, pace.five_hour),
        paint(DIM, "·"),
        share(shares.weekly, pace.weekly)
    )
}

/// The session's account: `work?` where the line is not sure of it, `?` alone where it cannot
/// name one, and `unenrolled` where Claude Code has no enrolled account's login stored.
fn label(line: &StatusLine) -> String {
    let unsure = || paint(DIM, "?");
    match (&line.current, line.sure) {
        (Some(label), true) => paint(BOLD, label),
        (Some(label), false) => format!("{}{}", paint(BOLD, label), unsure()),
        (None, true) => paint(BOLD, "unenrolled"),
        (None, false) => unsure(),
    }
}

pub fn human(line: &StatusLine) -> String {
    let mut parts = vec![format!(
        "{} {}",
        label(line),
        shares(line.session, line.pace)
    )];
    for other in &line.others {
        let age = other
            .age
            .map(|seconds| paint(DIM, format!(" ({})", words::span(seconds))))
            .unwrap_or_default();
        parts.push(format!(
            "{} {}{age}",
            paint(DIM, &other.label),
            shares(other.shares, Paces::default())
        ));
    }
    parts.join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{BAD, GOOD};
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
            sure: true,
            session: shares(46.4, 70.0),
            pace: Paces::default(),
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

    /// A half is drawn as the app draws it, and as the automatic switch counts it.
    #[test]
    fn a_half_is_drawn_as_the_app_draws_it() {
        let line = StatusLine {
            current: Some("work".into()),
            sure: true,
            session: shares(94.5, 70.5),
            pace: Paces::default(),
            others: Vec::new(),
        };
        assert_eq!(plain(&human(&line)), "work 95%·71%");
    }

    #[test]
    fn an_account_not_enrolled_is_named_as_such() {
        let line = |session| StatusLine {
            current: None,
            sure: true,
            session,
            pace: Paces::default(),
            others: Vec::new(),
        };
        assert_eq!(plain(&human(&line(Shares::default()))), "unenrolled ?·?");
        assert_eq!(plain(&human(&line(shares(3.0, 31.0)))), "unenrolled 3%·31%");
    }

    /// Where the session's numbers prove no account, or it passed none and Claude Code's
    /// config has moved since Anthropic last named the login stored, the label says so: the
    /// account in use's marked, or a mark alone where it can name none.
    #[test]
    fn an_unsure_label_is_drawn_with_a_question_mark() {
        let line = |current: Option<&str>| StatusLine {
            current: current.map(str::to_owned),
            sure: false,
            session: shares(3.0, 31.0),
            pace: Paces::default(),
            others: Vec::new(),
        };
        assert_eq!(plain(&human(&line(Some("work")))), "work? 3%·31%");
        assert_eq!(plain(&human(&line(None))), "? 3%·31%");
    }

    /// The account in use marks a limit over pace with a red rising triangle and one under
    /// with a green falling one, and nothing at an even pace or where it means nothing.
    #[test]
    fn the_account_in_use_marks_its_pace_on_each_limit() {
        let line = |pace| StatusLine {
            current: Some("work".into()),
            sure: true,
            session: shares(46.4, 70.0),
            pace,
            others: vec![Entry {
                label: "personal".into(),
                shares: shares(12.0, 40.0),
                age: None,
            }],
        };
        let paced = line(Paces {
            five_hour: Some(Standing::Under),
            weekly: Some(Standing::Over { runs_out_in: 3_600 }),
        });
        assert_eq!(plain(&human(&paced)), "work 46%▼·70%▲  personal 12%·40%");
        assert!(human(&paced).contains(&paint(BAD, "▲")));
        assert!(human(&paced).contains(&paint(GOOD, "▼")));
        let even = line(Paces {
            five_hour: Some(Standing::Even),
            weekly: None,
        });
        assert_eq!(plain(&human(&even)), "work 46%·70%  personal 12%·40%");
    }
}
