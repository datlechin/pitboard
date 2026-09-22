//! `pitboard status` for a person and for a program.

use crate::ui::{self, BAD, BOLD, DIM, GOOD, WARN, pad, paint};
use pitboard_core::doctor::RENEW_WITHIN;
use pitboard_core::status::{Report, Row, Stale};
use pitboard_core::time;
use pitboard_core::usage::{Snapshot, Source, Window};
use serde_json::{Value, json};

fn window_name(w: &Window) -> String {
    let base = match w.kind.as_str() {
        "session" | "five_hour" => "5h",
        "weekly_all" | "seven_day" | "weekly_scoped" => "week",
        other => other,
    };
    match &w.scope {
        Some(scope) => format!("{base} · {scope}"),
        None => base.to_string(),
    }
}

/// Whether the account can be switched to, and what to do when it cannot.
fn standing(row: &Row, now: i64) -> String {
    if row.signed_in {
        return match row.label {
            Some(_) => paint(GOOD, "signed in"),
            None => format!(
                "{} {}",
                paint(GOOD, "signed in"),
                paint(WARN, "· not enrolled: pitboard enroll <label>")
            ),
        };
    }
    let label = row.label.as_deref().unwrap_or("<label>");
    let sign_in_again = format!("pitboard enroll {label} --sign-in");
    match &row.parked {
        None => paint(WARN, format!("nothing parked · {sign_in_again}")),
        Some(p) if !p.restorable_at(now) => paint(BAD, format!("login expired · {sign_in_again}")),
        Some(p) => match p.refresh_expires_at {
            Some(at) if at - now < RENEW_WITHIN => paint(
                WARN,
                format!(
                    "ready · expires in {} · {sign_in_again}",
                    time::span(at - now)
                ),
            ),
            Some(at) => format!(
                "{} {}",
                paint(GOOD, "ready"),
                paint(DIM, format!("· good for {}", time::span(at - now)))
            ),
            None => paint(GOOD, "ready"),
        },
    }
}

fn provenance(usage: &Snapshot, now: i64) -> Option<String> {
    let at = usage.observed_at?;
    match usage.source {
        Source::Live => None,
        Source::ClaudeCodeCache => Some(format!(
            "from Claude Code, measured {}",
            time::moment(at, now)
        )),
        Source::Remembered => Some(format!(
            "measured {}, {} ago",
            time::moment(at, now),
            time::span(now - at)
        )),
    }
}

pub fn human(report: &Report) -> String {
    let now = report.now;
    if report.rows.is_empty() {
        return "Nothing is signed in and no account is enrolled.\n\
                Run `claude` and sign in, then `pitboard enroll <label>`.\n"
            .into();
    }
    let label_width = report
        .rows
        .iter()
        .filter_map(|r| r.label.as_deref().map(ui::columns))
        .max()
        .unwrap_or(0);
    let email_width = report
        .rows
        .iter()
        .map(|r| ui::columns(&r.email))
        .max()
        .unwrap_or(0);
    let name_width = report
        .rows
        .iter()
        .flat_map(|r| r.usage.iter().flat_map(|u| u.windows.iter()))
        .map(|w| ui::columns(&window_name(w)))
        .max()
        .unwrap_or(0);

    let mut blocks = Vec::new();
    for row in &report.rows {
        let marker = if row.signed_in {
            paint(GOOD, "●")
        } else {
            paint(DIM, "○")
        };
        let label = match label_width {
            0 => String::new(),
            width => format!(
                "{}  ",
                paint(BOLD, pad(row.label.as_deref().unwrap_or_default(), width))
            ),
        };
        let mut block = format!(
            "{marker} {label}{}  {}\n",
            paint(DIM, pad(&row.email, email_width)),
            standing(row, now)
        );

        let windows: Vec<&Window> = row.usage.iter().flat_map(|u| u.windows.iter()).collect();
        let why = row.stale.and_then(Stale::explanation);
        if windows.is_empty() {
            block.push_str(&format!(
                "    {}\n",
                paint(
                    DIM,
                    match why {
                        Some(why) => format!("no usage known · {why}"),
                        None => "no usage known yet".into(),
                    }
                )
            ));
        } else {
            for w in &windows {
                let resets = w.resets_at.map_or_else(String::new, |at| {
                    if at <= now {
                        "resetting now".into()
                    } else {
                        format!("resets in {}", time::span(at - now))
                    }
                });
                block.push_str(&format!(
                    "    {}  {}  {}  {}\n",
                    pad(&window_name(w), name_width),
                    ui::bar(w.percent, 10),
                    paint(ui::level(w.percent), format!("{:>3.0}%", w.percent)),
                    paint(DIM, resets)
                ));
            }
            let note = row.usage.as_ref().and_then(|u| provenance(u, now));
            let line = match (note, why) {
                (Some(note), Some(why)) => {
                    format!("{} {}", paint(DIM, note), paint(WARN, format!("· {why}")))
                }
                (Some(note), None) => paint(DIM, note),
                (None, Some(why)) => paint(WARN, why),
                (None, None) => String::new(),
            };
            if !line.is_empty() {
                block.push_str(&format!("    {}  {line}\n", pad("", name_width)));
            }
        }
        blocks.push(block);
    }
    let mut out = blocks.join("\n");
    // Said only when it is not the default, so the ordinary answer is unchanged. Where it
    // is not, "who is signed in" is a fact about this slot and not about the machine, and
    // a report that does not say which slot it means is answering a question nobody asked.
    if !report.slot.default {
        out.push_str(&format!(
            "\n{}\n",
            paint(
                DIM,
                format!("for the credential slot {}", report.slot.service)
            )
        ));
    }
    out
}

pub fn json(report: &Report) -> Value {
    json!({
        // Which slot this answer is about. Accounts and their parked logins belong to the
        // machine; who is signed in belongs to one credential slot, and CLAUDE_CONFIG_DIR
        // selects a different one.
        "slot": {
            "service": report.slot.service,
            "default": report.slot.default,
        },
        "signed_in": report.signed_in.as_ref().ok().map(|o| json!({
            "account_uuid": o.account_uuid,
            "email": o.email,
            "organization_uuid": o.organization_uuid,
        })),
        "signed_in_error": report.signed_in.as_ref().err(),
        "accounts": report.rows.iter().map(|r| json!({
            "label": r.label,
            "email": r.email,
            "account_uuid": r.account_uuid,
            "signed_in": r.signed_in,
            "switchable": r.switchable(report.now),
            "parked": r.parked.as_ref().map(|p| json!({
                "parked_at": p.parked_at,
                "access_expires_at": p.access_expires_at,
                "refresh_expires_at": p.refresh_expires_at,
            })),
            "usage": r.usage.as_ref().map(|u| json!({
                "source": u.source,
                "observed_at": u.observed_at,
                "windows": u.windows,
            })),
            "stale": r.stale,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pitboard_core::api::Owner;
    use pitboard_core::state::Park;

    const NOW: i64 = 1_789_935_000;

    fn reading(percent: f64, source: Source) -> Snapshot {
        Snapshot {
            windows: vec![Window {
                kind: "session".into(),
                scope: None,
                percent,
                resets_at: Some(NOW + 3_600),
                is_active: true,
                severity: None,
            }],
            observed_at: Some(NOW - 7_200),
            account_uuid: None,
            source,
        }
    }

    fn parked(refresh_expires_at: i64) -> Park {
        Park {
            service: "pitboard-park-x-1".into(),
            parked_at: NOW - 86_400,
            refresh_fingerprint: "f".into(),
            access_expires_at: Some(NOW - 3_600),
            refresh_expires_at: Some(refresh_expires_at),
        }
    }

    fn row(label: Option<&str>, signed_in: bool) -> Row {
        let name = label.unwrap_or("someone");
        Row {
            label: label.map(str::to_owned),
            email: format!("{name}@example.com"),
            account_uuid: format!("{name}-uuid"),
            signed_in,
            parked: (!signed_in).then(|| parked(NOW + 20 * 86_400)),
            usage: Some(reading(30.0, Source::Live)),
            stale: None,
        }
    }

    fn report(rows: Vec<Row>) -> Report {
        Report {
            slot: pitboard_core::status::Slot {
                service: "Claude Code-credentials".into(),
                default: true,
            },
            now: NOW,
            rows,
            signed_in: Ok(Owner {
                account_uuid: "work-uuid".into(),
                email: "work@example.com".into(),
                organization_uuid: "org".into(),
            }),
        }
    }

    fn plain(styled: &str) -> String {
        anstream::adapter::strip_str(styled).to_string()
    }

    #[test]
    fn a_remembered_reading_says_when_it_was_measured() {
        let mut personal = row(Some("personal"), false);
        personal.usage = Some(reading(44.0, Source::Remembered));
        personal.stale = Some(Stale::ParkedAccessExpired);
        let text = plain(&human(&report(vec![row(Some("work"), true), personal])));
        assert!(
            text.contains("measured") && text.contains("2h 00m ago"),
            "{text}"
        );
    }

    #[test]
    fn a_signed_in_account_that_is_not_enrolled_says_how_to_enroll_it() {
        let text = plain(&human(&report(vec![row(None, true)])));
        assert!(text.contains("pitboard enroll <label>"), "{text}");
    }

    #[test]
    fn an_account_that_cannot_be_switched_to_says_how_to_fix_it() {
        let mut expired = row(Some("expired"), false);
        expired.parked = Some(parked(NOW - 1));
        let mut empty = row(Some("empty"), false);
        empty.parked = None;
        let mut soon = row(Some("soon"), false);
        soon.parked = Some(parked(NOW + 86_400));
        let text = plain(&human(&report(vec![
            row(Some("work"), true),
            expired,
            empty,
            soon,
        ])));
        for (label, says) in [
            (
                "expired",
                "login expired · pitboard enroll expired --sign-in",
            ),
            ("empty", "nothing parked · pitboard enroll empty --sign-in"),
            ("soon", "expires in 1d 0h · pitboard enroll soon --sign-in"),
        ] {
            let line = text
                .lines()
                .find(|l| l.contains(&format!(" {label} ")))
                .unwrap();
            assert!(line.contains(says), "{line}");
        }
    }

    #[test]
    fn columns_line_up_whatever_the_label_and_email_lengths() {
        let text = plain(&human(&report(vec![
            row(Some("a"), true),
            row(Some("much-longer-label"), false),
        ])));
        let column = |email: &str| {
            let line = text.lines().find(|l| l.contains(email)).unwrap();
            line[..line.find(email).unwrap()].chars().count()
        };
        assert_eq!(
            column("a@example.com"),
            column("much-longer-label@example.com"),
            "{text}"
        );
    }

    /// An unused model limit is still a limit: at 0% it says there is room.
    #[test]
    fn every_limit_is_shown_including_one_not_yet_used() {
        let mut work = row(Some("work"), true);
        work.usage.as_mut().unwrap().windows.push(Window {
            kind: "weekly_scoped".into(),
            scope: Some("Fable".into()),
            percent: 0.0,
            resets_at: Some(NOW + 86_400),
            is_active: false,
            severity: None,
        });
        let text = plain(&human(&report(vec![work])));
        let fable = text
            .lines()
            .find(|l| l.contains("week · Fable"))
            .expect(&text);
        assert!(fable.contains("0%"), "{fable}");
    }

    #[test]
    fn the_human_view_never_prints_a_keychain_item_name() {
        let text = human(&report(vec![
            row(Some("work"), true),
            row(Some("personal"), false),
        ]));
        assert!(!text.contains("pitboard-park-"), "{text}");
    }

    #[test]
    fn the_json_says_where_every_number_came_from_and_what_can_be_switched_to() {
        let mut personal = row(Some("personal"), false);
        personal.stale = Some(Stale::ParkedAccessExpired);
        let value = json(&report(vec![row(Some("work"), true), personal]));
        assert_eq!(value["accounts"][0]["usage"]["source"], "live");
        assert_eq!(value["accounts"][0]["switchable"], false);
        assert_eq!(value["accounts"][1]["switchable"], true);
        assert_eq!(value["accounts"][1]["stale"], "parked_access_expired");
        assert!(value["accounts"][1]["parked"]["refresh_expires_at"].is_i64());
        assert!(value["accounts"][1]["parked"].get("service").is_none());
        assert_eq!(value["signed_in"]["account_uuid"], "work-uuid");
    }
}
