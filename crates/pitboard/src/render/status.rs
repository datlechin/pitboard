//! `pitboard status` for a person and for a program.

use crate::ui::{self, BAD, BOLD, DIM, GOOD, WARN, pad, paint};
use pitboard_core::doctor::RENEW_WITHIN;
use pitboard_core::provider::ProviderId;
use pitboard_core::state::Key;
use pitboard_core::status::{Report, Row, Stale};
use pitboard_core::time;
use pitboard_core::usage::{Snapshot, Source, Window};
use serde_json::{Value, json};

/// A window by how long it runs, where that is known, and otherwise by its kind.
///
/// The length is what makes two tools' windows comparable: OpenAI times every window and
/// names none, Anthropic names every window and times none, and pitboard knows the length
/// of each either way. A kind is only the fallback, for a reading remembered from before
/// the length was kept.
fn window_name(w: &Window) -> String {
    let base = w.length_seconds.and_then(length_name).unwrap_or_else(|| {
        match w.kind.as_str() {
            "session" | "five_hour" => "5h",
            "weekly_all" | "seven_day" | "weekly_scoped" => "week",
            other => other,
        }
        .to_string()
    });
    match &w.scope {
        Some(scope) => format!("{base} · {scope}"),
        None => base,
    }
}

/// `5h`, `day`, `week`, `3d`: a length the way somebody says it, or `None` for one that
/// is not a whole number of hours.
fn length_name(seconds: i64) -> Option<String> {
    const HOUR: i64 = 3600;
    const DAY: i64 = 24 * HOUR;
    match seconds {
        s if s <= 0 => None,
        s if s == 7 * DAY => Some("week".into()),
        s if s == DAY => Some("day".into()),
        s if s % DAY == 0 => Some(format!("{}d", s / DAY)),
        s if s % HOUR == 0 => Some(format!("{}h", s / HOUR)),
        _ => None,
    }
}

/// How somebody would type this row's account, or a placeholder in its tool's own shape
/// for a row nothing has enrolled: `<label>` for Claude Code, `codex/<label>` for Codex.
/// A bare `<label>` on a Codex row would have them enroll a Claude Code account.
fn typed(row: &Row) -> String {
    row.key()
        .unwrap_or_else(|| Key::new(row.provider, "<label>"))
        .typed()
}

/// The tool, as a heading over its accounts.
fn tool_name(which: ProviderId) -> &'static str {
    match which {
        ProviderId::Claude => "Claude Code",
        ProviderId::Codex => "Codex",
        other => other.code(),
    }
}

/// Whether the account can be switched to, and what to do when it cannot.
fn standing(row: &Row, now: i64) -> String {
    // A login pitboard could not pin on any account: nothing to type at it but `doctor`,
    // which the line under it says.
    if row.unplaced() {
        return paint(
            WARN,
            match row.stale {
                Some(Stale::LoginUnusable) => "login cannot be switched",
                _ => "login could not be read",
            },
        );
    }
    if row.signed_in {
        return match row.label {
            Some(_) => paint(GOOD, "signed in"),
            None => format!(
                "{} {}",
                paint(GOOD, "signed in"),
                paint(
                    WARN,
                    format!("· not enrolled: pitboard enroll {}", typed(row))
                )
            ),
        };
    }
    let sign_in_again = format!("pitboard enroll {} --sign-in", typed(row));
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
        .map(|r| ui::columns(&email(r)))
        .max()
        .unwrap_or(0);
    // Said only when there is more than one tool to tell apart, so a machine with one reads
    // exactly as it always did.
    let tools = {
        let mut seen: Vec<ProviderId> = report.rows.iter().map(|r| r.provider).collect();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    };
    let name_width = report
        .rows
        .iter()
        .flat_map(|r| r.usage.iter().flat_map(|u| u.windows.iter()))
        .map(|w| ui::columns(&window_name(w)))
        .max()
        .unwrap_or(0);

    let mut blocks = Vec::new();
    let mut heading: Option<ProviderId> = None;
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
        // Rows come grouped by tool, so a heading goes wherever the tool changes.
        let mut block = String::new();
        if tools > 1 && heading != Some(row.provider) {
            heading = Some(row.provider);
            block.push_str(&format!("{}\n", paint(BOLD, tool_name(row.provider))));
        }
        block.push_str(&format!(
            "{marker} {label}{}  {}\n",
            paint(DIM, pad(&email(row), email_width)),
            standing(row, now)
        ));

        let windows: Vec<&Window> = row.usage.iter().flat_map(|u| u.windows.iter()).collect();
        let why = row.explanation();
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
            // The answer to the question the whole tool exists for, where there is one.
            if let Some(seconds) = row.runway.seconds() {
                block.push_str(&format!(
                    "    {}  {}\n",
                    pad("", name_width),
                    paint(
                        DIM,
                        match row.runway {
                            pitboard_core::history::Runway::Burning(_) =>
                                format!("about {} left at this rate", time::span(seconds)),
                            _ => format!("resets in {}", time::span(seconds)),
                        }
                    )
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
    // Said only when it is not the default, so the ordinary answer is unchanged. Where it
    // is not, "who is signed in" is a fact about this slot and not about the machine, and
    // a report that does not say which slot it means is answering a question nobody asked.
    //
    // The slot is Claude Code's keychain item, so it goes where Claude Code's accounts end
    // and says nothing under another tool's. Printed after everything, it read as though it
    // qualified the Codex accounts above it. With Claude Code alone, where its accounts end
    // is the end, which is where it always was.
    let claude_ends = report
        .rows
        .iter()
        .rposition(|r| r.provider == ProviderId::Claude);
    if !report.slot.default
        && let Some(last) = claude_ends
    {
        blocks.insert(
            last + 1,
            format!(
                "{}\n",
                paint(
                    DIM,
                    format!("for the credential slot {}", report.slot.service)
                )
            ),
        );
    }
    blocks.join("\n")
}

/// The email to show, or what stands in for one on a login pitboard could not pin on any
/// account.
fn email(row: &Row) -> String {
    if row.unplaced() {
        "account unknown".into()
    } else {
        row.email.clone()
    }
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
            // How long this account lasts, from what its limits have been doing. Null when
            // there is not enough to go on: a wrong runway tells somebody to switch when
            // they need not, which is worse than no runway.
            "lasts": r.runway.seconds().map(|seconds| json!({
                "seconds": seconds,
                "why": match r.runway {
                    pitboard_core::history::Runway::Burning(_) => "filling",
                    pitboard_core::history::Runway::Resting(_) => "resets",
                    pitboard_core::history::Runway::Unknown => "unknown",
                },
            })),
            "usage": r.usage.as_ref().map(|u| json!({
                "source": u.source,
                "observed_at": u.observed_at,
                "windows": u.windows,
            })),
            "stale": r.stale,
            // Which tool the account is for, and its name as it would be typed with the
            // tool spelled out. Added beside what was there, so nothing a program already
            // reads moves.
            "provider": r.provider.code(),
            "qualified": r.key().map(|k| k.qualified()),
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
                length_seconds: None,
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
            provider: ProviderId::Claude,
            runway: pitboard_core::history::Runway::Unknown,
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
            length_seconds: None,
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

    fn window(kind: &str, length_seconds: Option<i64>) -> Window {
        Window {
            kind: kind.into(),
            scope: None,
            percent: 10.0,
            resets_at: Some(NOW + 3_600),
            is_active: true,
            severity: None,
            length_seconds,
        }
    }

    /// Named by how long it runs, which is what makes one tool's window read like
    /// another's; by its kind only where the length is not known.
    #[test]
    fn a_window_is_named_by_how_long_it_runs() {
        for (kind, length, named) in [
            ("five_hour", Some(18_000), "5h"),
            ("seven_day", Some(604_800), "week"),
            ("1_day", Some(86_400), "day"),
            ("2_day", Some(172_800), "2d"),
            ("3_hour", Some(10_800), "3h"),
            ("90_minute", Some(5_400), "90_minute"),
            ("primary", None, "primary"),
            ("session", None, "5h"),
            ("weekly_all", None, "week"),
        ] {
            assert_eq!(window_name(&window(kind, length)), named, "{kind}");
        }
    }

    fn codex(label: Option<&str>, signed_in: bool) -> Row {
        Row {
            provider: ProviderId::Codex,
            ..row(label, signed_in)
        }
    }

    /// A hint is something to type, and typed bare it would enroll a Claude Code account.
    #[test]
    fn a_codex_row_says_what_to_type_for_codex() {
        let mut empty = codex(Some("work"), false);
        empty.parked = None;
        let mut expired = codex(Some("home"), false);
        expired.parked = Some(parked(NOW - 1));
        let text = plain(&human(&report(vec![codex(None, true), empty, expired])));
        assert!(
            text.contains("not enrolled: pitboard enroll codex/<label>"),
            "{text}"
        );
        assert!(
            text.contains("nothing parked · pitboard enroll codex/work --sign-in"),
            "{text}"
        );
        assert!(
            text.contains("login expired · pitboard enroll codex/home --sign-in"),
            "{text}"
        );
        assert!(!text.contains("pitboard enroll work"), "{text}");
    }

    /// Two tools' accounts can share a label, so with both on screen the tool is said.
    /// With one, nothing is added: see [`a_claude_code_machine_reads_exactly_as_it_did`].
    #[test]
    fn with_two_tools_each_gets_a_heading() {
        let mut work = codex(Some("work"), true);
        work.usage = Some(Snapshot {
            windows: vec![window("five_hour", Some(18_000))],
            ..reading(10.0, Source::Live)
        });
        let text = plain(&human(&report(vec![
            row(Some("work"), true),
            row(Some("personal"), false),
            work,
            codex(Some("home"), false),
        ])));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Claude Code", "{text}");
        let codex_at = lines
            .iter()
            .position(|l| *l == "Codex")
            .expect("a Codex heading");
        let personal_at = lines
            .iter()
            .position(|l| l.contains("personal@example.com"))
            .unwrap();
        assert!(personal_at < codex_at, "{text}");
        assert!(
            lines[codex_at + 1].contains("work@example.com") && lines[codex_at + 1].contains("●"),
            "{text}"
        );
        assert_eq!(
            text.matches("Claude Code").count(),
            1,
            "one heading per tool: {text}"
        );
    }

    /// A login nobody could read says so, in its tool's words, and offers nothing to type
    /// that would act on an account it cannot name.
    #[test]
    fn a_login_that_could_not_be_read_says_so() {
        let mut unreadable = codex(None, false);
        unreadable.email = String::new();
        unreadable.account_uuid = String::new();
        unreadable.usage = None;
        unreadable.stale = Some(Stale::LoginUnreadable);
        let text = plain(&human(&report(vec![
            codex(Some("work"), false),
            unreadable,
        ])));
        let line = text
            .lines()
            .find(|l| l.contains("account unknown"))
            .expect(&text);
        assert!(line.contains("login could not be read"), "{line}");
        assert!(!line.contains("pitboard enroll"), "{line}");
        assert!(
            text.contains("Codex's login could not be read; run `pitboard doctor`"),
            "{text}"
        );
    }

    /// A login that was read and is not one account's, such as an API key, says it cannot
    /// be switched rather than that it could not be read, and offers nothing to type.
    #[test]
    fn a_login_that_cannot_be_used_says_so() {
        let mut unusable = codex(None, false);
        unusable.email = String::new();
        unusable.account_uuid = String::new();
        unusable.usage = None;
        unusable.stale = Some(Stale::LoginUnusable);
        let text = plain(&human(&report(vec![codex(Some("work"), false), unusable])));
        let line = text
            .lines()
            .find(|l| l.contains("account unknown"))
            .expect(&text);
        assert!(line.contains("login cannot be switched"), "{line}");
        assert!(!line.contains("pitboard enroll"), "{line}");
        assert!(
            text.contains(
                "Codex's login is not one pitboard can park or switch; run `pitboard doctor`"
            ),
            "{text}"
        );
    }

    /// The slot is Claude Code's keychain item. Printed after everything, it sat under the
    /// last Codex account and read as though it were about it.
    #[test]
    fn the_credential_slot_is_said_under_claude_codes_accounts() {
        let mut both = report(vec![
            row(Some("work"), true),
            row(Some("personal"), false),
            codex(Some("work"), true),
        ]);
        both.slot.default = false;
        let text = plain(&human(&both));
        let lines: Vec<&str> = text.lines().collect();
        let slot = lines
            .iter()
            .position(|l| l.starts_with("for the credential slot"))
            .expect(&text);
        let heading = lines.iter().position(|l| *l == "Codex").expect(&text);
        let personal = lines
            .iter()
            .position(|l| l.contains("personal@example.com"))
            .unwrap();
        assert!(personal < slot && slot < heading, "{text}");
        assert!(
            !lines[heading..]
                .iter()
                .any(|l| l.contains("credential slot")),
            "{text}"
        );

        // With no Claude Code account on screen there is nothing for it to be about.
        let mut codex_only = report(vec![codex(Some("work"), true)]);
        codex_only.slot.default = false;
        let text = plain(&human(&codex_only));
        assert!(!text.contains("credential slot"), "{text}");
    }

    #[test]
    fn the_json_says_which_tool_each_account_is_for() {
        let value = json(&report(vec![
            row(Some("work"), true),
            codex(Some("work"), false),
            codex(None, true),
        ]));
        let accounts = value["accounts"].as_array().unwrap();
        assert_eq!(accounts[0]["provider"], "claude");
        assert_eq!(accounts[0]["qualified"], "claude/work");
        assert_eq!(
            accounts[0]["label"], "work",
            "the label itself is untouched"
        );
        assert_eq!(accounts[1]["provider"], "codex");
        assert_eq!(accounts[1]["qualified"], "codex/work");
        assert_eq!(accounts[2]["provider"], "codex");
        assert!(accounts[2]["qualified"].is_null(), "nothing enrolled it");
    }

    /// Every kind of row a Claude Code machine shows, for pinning what it reads as.
    fn every_claude_row() -> Report {
        let mut work = row(Some("work"), true);
        work.usage.as_mut().unwrap().windows.extend([
            Window {
                kind: "weekly_all".into(),
                scope: None,
                percent: 40.0,
                resets_at: Some(NOW + 3 * 86_400),
                is_active: false,
                severity: None,
                length_seconds: Some(604_800),
            },
            Window {
                kind: "weekly_scoped".into(),
                scope: Some("Fable".into()),
                percent: 0.0,
                resets_at: Some(NOW + 86_400),
                is_active: false,
                severity: None,
                length_seconds: Some(604_800),
            },
        ]);
        let mut remembered = row(Some("personal"), false);
        remembered.usage = Some(reading(44.0, Source::Remembered));
        // When it was measured is local time, which is not this test's to pin.
        remembered.usage.as_mut().unwrap().observed_at = None;
        remembered.stale = Some(Stale::Unreachable);
        let mut empty = row(Some("empty"), false);
        empty.parked = None;
        empty.usage = None;
        empty.stale = Some(Stale::NothingParked);
        let mut expired = row(Some("expired"), false);
        expired.parked = Some(parked(NOW - 1));
        expired.usage = None;
        expired.stale = Some(Stale::ParkedAccessExpired);
        let mut stranger = row(None, true);
        stranger.usage = None;
        stranger.stale = Some(Stale::SessionExpired);
        let mut report = report(vec![work, remembered, empty, expired, stranger]);
        report.slot.default = false;
        report
    }

    /// A machine with only Claude Code on it reads exactly as it did before a row knew its
    /// tool: every character, and every colour. Pinned from the renderer as it was, because
    /// "nothing changed for one tool" is a promise and a promise needs a test.
    #[test]
    fn a_claude_code_machine_reads_exactly_as_it_did() {
        let expected = "\
● work      work@example.com      signed in
    5h            ███░░░░░░░   30%  resets in 1h 00m
    week          ████░░░░░░   40%  resets in 3d 0h
    week · Fable  ░░░░░░░░░░    0%  resets in 1d 0h

○ personal  personal@example.com  ready · good for 20d 0h
    5h            ████░░░░░░   44%  resets in 1h 00m
                  Anthropic could not be reached

○ empty     empty@example.com     nothing parked · pitboard enroll empty --sign-in
    no usage known yet

○ expired   expired@example.com   login expired · pitboard enroll expired --sign-in
    no usage known yet

●           someone@example.com   signed in · not enrolled: pitboard enroll <label>
    no usage known · Claude Code's session has expired; `claude` renews it

for the credential slot Claude Code-credentials
";
        assert_eq!(plain(&human(&every_claude_row())), expected);
        assert_eq!(
            human(&every_claude_row()),
            STYLED_BEFORE,
            "the colours moved"
        );
    }

    /// What [`a_claude_code_machine_reads_exactly_as_it_did`] renders, escape codes and
    /// all, as the renderer wrote it before there was a second tool.
    const STYLED_BEFORE: &str = "\u{1b}[32m●\u{1b}[0m \u{1b}[1mwork    \u{1b}[0m  \u{1b}[2mwork@example.com    \u{1b}\
        [0m  \u{1b}[32msigned in\u{1b}[0m\n    5h            \u{1b}[32m███\u{1b}[0m\u{1b}\
        [2m░░░░░░░\u{1b}[0m  \u{1b}[32m 30%\u{1b}[0m  \u{1b}[2mresets in 1h 00m\u{1b}[0m\
        \n    week          \u{1b}[32m████\u{1b}[0m\u{1b}[2m░░░░░░\u{1b}[0m  \u{1b}[32m \
        40%\u{1b}[0m  \u{1b}[2mresets in 3d 0h\u{1b}[0m\n    week · Fable  \u{1b}[32m\u{1b}\
        [0m\u{1b}[2m░░░░░░░░░░\u{1b}[0m  \u{1b}[32m  0%\u{1b}[0m  \u{1b}[2mresets in 1d \
        0h\u{1b}[0m\n\n\u{1b}[2m○\u{1b}[0m \u{1b}[1mpersonal\u{1b}[0m  \u{1b}[2mpersonal\
        @example.com\u{1b}[0m  \u{1b}[32mready\u{1b}[0m \u{1b}[2m· good for 20d 0h\u{1b}\
        [0m\n    5h            \u{1b}[32m████\u{1b}[0m\u{1b}[2m░░░░░░\u{1b}[0m  \u{1b}[3\
        2m 44%\u{1b}[0m  \u{1b}[2mresets in 1h 00m\u{1b}[0m\n                  \u{1b}[33\
        mAnthropic could not be reached\u{1b}[0m\n\n\u{1b}[2m○\u{1b}[0m \u{1b}[1mempty   \
        \u{1b}[0m  \u{1b}[2mempty@example.com   \u{1b}[0m  \u{1b}[33mnothing parked · pi\
        tboard enroll empty --sign-in\u{1b}[0m\n    \u{1b}[2mno usage known yet\u{1b}[0m\
        \n\n\u{1b}[2m○\u{1b}[0m \u{1b}[1mexpired \u{1b}[0m  \u{1b}[2mexpired@example.com \
        \u{1b}[0m  \u{1b}[31mlogin expired · pitboard enroll expired --sign-in\u{1b}[0m\n    \
        \u{1b}[2mno usage known yet\u{1b}[0m\n\n\u{1b}[32m●\u{1b}[0m \u{1b}[1m        \u{1b}\
        [0m  \u{1b}[2msomeone@example.com \u{1b}[0m  \u{1b}[32msigned in\u{1b}[0m \u{1b}\
        [33m· not enrolled: pitboard enroll <label>\u{1b}[0m\n    \u{1b}[2mno usage know\
        n · Claude Code's session has expired; `claude` renews it\u{1b}[0m\n\n\u{1b}[2mf\
        or the credential slot Claude Code-credentials\u{1b}[0m\n";
}
