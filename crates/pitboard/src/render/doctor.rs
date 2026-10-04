//! `pitboard doctor` for a person and for a program.

use crate::ui::{self, BAD, BOLD, DIM, GOOD, WARN, pad, paint};
use pitboard_core::doctor::{Check, Diagnosis, Level};
use pitboard_core::words;
use serde_json::{Value, json};

/// The section a check is listed under: Claude Code's, which has no heading, Codex's, or
/// Claude Desktop's, told apart by their codes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    ClaudeCode,
    Codex,
    Desktop,
}

fn section(check: &Check) -> Section {
    if check.code.starts_with("codex_") {
        Section::Codex
    } else if check.code.starts_with("desktop_") {
        Section::Desktop
    } else {
        Section::ClaudeCode
    }
}

pub fn human(checks: &[Check]) -> String {
    // Each section lines up on its own, so what another tool's checks are called can never
    // move a column of Claude Code's.
    let width = |of: Section| {
        checks
            .iter()
            .filter(|c| section(c) == of)
            .map(|c| ui::columns(&c.name))
            .max()
            .unwrap_or(0)
    };
    let mut out = String::new();
    // Codex's checks and then Claude Desktop's come after Claude Code's, so each gets a
    // heading of its own where it starts. A machine with neither has none of them and
    // reads as it always did.
    let mut current = Section::ClaudeCode;
    for c in checks {
        let of = section(c);
        if of != current {
            current = of;
            let heading = match of {
                Section::Codex => "Codex",
                Section::Desktop => "Claude Desktop",
                Section::ClaudeCode => "Claude Code",
            };
            out.push_str(&format!("\n{}\n", paint(BOLD, heading)));
        }
        let width = width(of);
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
pub fn json(diagnosis: &Diagnosis) -> Value {
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

    /// Claude Desktop's checks sit under a heading of their own, in a column of their own,
    /// after Codex's, and a machine without the app reads as it did.
    #[test]
    fn claude_desktop_checks_get_a_heading_of_their_own() {
        let plain = |checks: &[Check]| anstream::adapter::strip_str(&human(checks)).to_string();
        let codex = || {
            let mut codex = check(Level::Ok);
            codex.code = "codex_backend";
            codex.name = "Codex login store".into();
            codex
        };
        let mut cookies = check(Level::Ok);
        cookies.code = "desktop_cookies";
        cookies.name = "Claude Desktop's cookie store".into();
        let mut park = check(Level::Warn);
        park.code = "desktop_parked_login";
        park.name = "account desktop/a-rather-long-label".into();
        let before = plain(&[check(Level::Ok), codex()]);
        let text = plain(&[check(Level::Ok), codex(), cookies, park]);
        let lines: Vec<&str> = text.lines().collect();
        let codex_at = lines.iter().position(|l| *l == "Codex").expect(&text);
        let heading = lines
            .iter()
            .position(|l| *l == "Claude Desktop")
            .expect(&text);
        assert!(heading > codex_at, "{text}");
        assert_eq!(lines[heading - 1], "", "set apart: {text}");
        assert!(lines[heading + 1].contains("cookie store"), "{text}");
        assert!(
            before.starts_with(&text[..text.find("\nClaude Desktop").unwrap()]),
            "what comes before it is what a machine without the app shows: {text}"
        );
        assert!(!before.contains("Claude Desktop"), "{before}");
    }
}
