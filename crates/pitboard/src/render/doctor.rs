//! `pitboard doctor` for a person and for a program.

use crate::ui::{self, BAD, BOLD, DIM, GOOD, WARN, pad, paint};
use pitboard_core::doctor::{self, Check, Diagnosis, Level};
use pitboard_core::words;
use serde_json::{Value, json};

pub(crate) fn human(checks: &[Check]) -> String {
    // Each section lines up on its own, so what one tool's checks are called can never move
    // a column of another's.
    let width_of = |section| {
        checks
            .iter()
            .filter(|c| doctor::section(c.code) == section)
            .map(|c| ui::columns(&c.name))
            .max()
            .unwrap_or(0)
    };
    let mut out = String::new();
    // Each other tool's checks come after Claude Code's, and get a heading where they start.
    // A machine with none of them reads as it always did.
    let mut section = None;
    let mut width = width_of(None);
    for c in checks {
        let of = doctor::section(c.code);
        if of != section {
            width = width_of(of);
            if let Some(tool) = of {
                out.push_str(&format!("\n{}\n", paint(BOLD, tool.name())));
            }
            section = of;
        }
        let mark = match c.level {
            Level::Ok => paint(GOOD, "✓"),
            Level::Warn => paint(WARN, "!"),
            Level::Fail => paint(BAD, "✗"),
        };
        out.push_str(&format!("{mark} {}  {}\n", pad(&c.name, width), c.detail));
        if !c.advice.is_empty() {
            out.push_str(&format!(
                "  {}  {}\n",
                pad("", width),
                paint(DIM, &c.advice)
            ));
        }
    }
    // The sentence the app heads its checks with, in the colour of the worst of them.
    let any = |level| checks.iter().any(|c| c.level == level);
    let style = if any(Level::Fail) {
        BAD
    } else if any(Level::Warn) {
        WARN
    } else {
        GOOD
    };
    let summary = words::doctor_summary(checks.iter().map(|c| c.level));
    out.push_str(&format!("\n{}\n", paint(style, summary)));
    out
}

/// What the bug template promises: labels, codes, paths and times, and no email address,
/// account identifier or login name. The human-readable report above is not touched; a
/// person looking at their own machine should see their own account.
pub(crate) fn json(diagnosis: &Diagnosis) -> Value {
    let report = json!({
        "environment": diagnosis.environment,
        "checks": diagnosis.checks.iter().map(|c| json!({
            "code": c.code,
            "name": c.name,
            "level": match c.level { Level::Ok => "ok", Level::Warn => "warn", Level::Fail => "fail" },
            "detail": c.detail,
            "advice": c.advice,
        })).collect::<Vec<_>>(),
    });
    diagnosis.redaction.over_json(&report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(level: Level) -> Check {
        Check {
            code: "credential",
            name: "credential".into(),
            level,
            detail: "detail".into(),
            advice: if level == Level::Ok {
                String::new()
            } else {
                "do this".into()
            },
        }
    }

    /// The last line is the sentence the app's This Mac pane heads its checks with, so the
    /// two never disagree about the same machine. A check that fails outweighs every
    /// warning: the line then counts what is broken and says not to switch, in red.
    #[test]
    fn the_summary_says_what_the_app_says() {
        let plain = |checks: &[Check]| anstream::adapter::strip_str(&human(checks)).to_string();
        assert!(
            plain(&[check(Level::Ok)]).ends_with("\nEverything Pitboard checks is in order.\n")
        );
        assert!(
            plain(&[check(Level::Ok), check(Level::Warn)])
                .ends_with("\nOne thing is worth looking at.\n")
        );
        assert!(
            plain(&[check(Level::Warn), check(Level::Warn)])
                .ends_with("\n2 things are worth looking at.\n")
        );
        assert!(
            plain(&[check(Level::Warn), check(Level::Fail)])
                .ends_with("\n1 broken: do not switch accounts until fixed.\n")
        );
        assert!(
            human(&[check(Level::Warn), check(Level::Fail)])
                .contains(&paint(BAD, "1 broken: do not switch accounts until fixed."))
        );
        assert!(
            human(&[check(Level::Ok), check(Level::Warn)])
                .contains(&paint(WARN, "One thing is worth looking at."))
        );
        assert!(
            human(&[check(Level::Ok)])
                .contains(&paint(GOOD, "Everything Pitboard checks is in order."))
        );
    }

    #[test]
    fn a_healthy_mac_with_claude_code_and_codex_reads_as_it_did() {
        let checks = pitboard_core::testing::doctor_on_a_healthy_mac();
        let printed = anstream::adapter::strip_str(&human(&checks)).to_string();
        insta::assert_snapshot!("doctor_on_a_healthy_mac", printed);
    }

    /// Codex's checks sit under a heading of their own, after everything about Claude
    /// Code, and a report with none of them reads as it did before there were any.
    #[test]
    fn codex_checks_get_a_heading_of_their_own() {
        let plain = |checks: &[Check]| anstream::adapter::strip_str(&human(checks)).to_string();
        let claude = [check(Level::Ok), check(Level::Warn)];
        assert!(!plain(&claude).contains("Codex"));
        let before = plain(&[check(Level::Ok)]);

        let mut codex = check(Level::Ok);
        codex.code = "codex_backend";
        codex.name = "Codex login store".into();
        let mut running = check(Level::Ok);
        running.code = "codex_running";
        running.name = "running Codex".into();
        let text = plain(&[check(Level::Ok), codex, running]);
        let lines: Vec<&str> = text.lines().collect();
        let heading = lines.iter().position(|l| *l == "Codex").expect(&text);
        assert_eq!(
            lines[heading - 1],
            "",
            "set apart from Claude Code's: {text}"
        );
        assert!(lines[heading + 1].contains("Codex login store"), "{text}");
        assert_eq!(text.matches("\nCodex\n").count(), 1, "{text}");
        assert!(
            before.starts_with(&text[..text.find("\nCodex").unwrap()]),
            "what comes before it is what a machine without Codex shows: {text}"
        );
    }

    #[test]
    fn claude_desktop_checks_get_a_heading_of_their_own() {
        let plain = |checks: &[Check]| anstream::adapter::strip_str(&human(checks)).to_string();
        let mut version = check(Level::Ok);
        version.code = "claude_version";
        version.name = "Claude Code".into();
        let mut codex = check(Level::Ok);
        codex.code = "codex_backend";
        codex.name = "Codex login store".into();
        let mut folder = check(Level::Ok);
        folder.code = "desktop_data_folder";
        folder.name = "Claude data folder".into();
        let mut account = check(Level::Warn);
        account.code = "desktop_parked_login";
        account.name = "account desktop/a-rather-long-label".into();
        let text = plain(&[check(Level::Ok), version, codex, folder, account]);
        let lines: Vec<&str> = text.lines().collect();
        let at = |line: &str| lines.iter().position(|l| *l == line).expect(&text);
        let (codex_at, desktop_at) = (at("Codex"), at("Claude Desktop"));
        assert!(
            lines[..codex_at].iter().any(|l| l.contains("Claude Code")),
            "claude_version is Claude Code's: {text}"
        );
        assert_eq!(lines[desktop_at - 1], "", "set apart from Codex's: {text}");
        assert_eq!(lines[codex_at + 1], "✓ Codex login store  detail", "{text}");
        let longest = "account desktop/a-rather-long-label";
        assert_eq!(
            lines[desktop_at + 1],
            format!("✓ {}  detail", pad("Claude data folder", longest.len())),
            "its own column: {text}"
        );
        assert_eq!(
            lines[desktop_at + 2],
            format!("! {longest}  detail"),
            "{text}"
        );
        assert_eq!(text.matches("\nClaude Desktop\n").count(), 1, "{text}");
    }

    /// A Codex account's checks are Codex's: under its heading, and in its column, so a
    /// long `codex/...` name cannot push Claude Code's details along.
    #[test]
    fn a_codex_account_is_listed_under_codex() {
        let plain = |checks: &[Check]| anstream::adapter::strip_str(&human(checks)).to_string();
        let mut backend = check(Level::Ok);
        backend.code = "codex_backend";
        backend.name = "Codex login store".into();
        let mut account = check(Level::Ok);
        account.code = "codex_parked_login";
        account.name = "account codex/a-rather-long-label".into();
        let before = plain(&[check(Level::Ok)]);
        let text = plain(&[check(Level::Ok), backend, account]);
        let lines: Vec<&str> = text.lines().collect();
        let heading = lines.iter().position(|l| *l == "Codex").expect(&text);
        let at = lines
            .iter()
            .position(|l| l.contains("codex/a-rather-long-label"))
            .unwrap();
        assert!(at > heading, "{text}");
        assert_eq!(lines[0], before.lines().next().unwrap(), "{text}");
    }
}
