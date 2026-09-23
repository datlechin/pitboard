//! `pitboard doctor` for a person and for a program.

use crate::ui::{self, BAD, BOLD, DIM, GOOD, WARN, pad, paint};
use pitboard_core::doctor::{Check, Diagnosis, Level};
use serde_json::{Value, json};

/// Whether a check belongs to Codex's section.
fn is_codex(check: &Check) -> bool {
    check.code.starts_with("codex_")
}

pub fn human(checks: &[Check]) -> String {
    // Each section lines up on its own, so what Codex's checks are called can never move a
    // column of Claude Code's.
    let width = |codex: bool| {
        checks
            .iter()
            .filter(|c| is_codex(c) == codex)
            .map(|c| ui::columns(&c.name))
            .max()
            .unwrap_or(0)
    };
    let (claude_width, codex_width) = (width(false), width(true));
    let mut out = String::new();
    // Codex's checks come last and are coded `codex_`, so they get a heading of their own
    // where they start. A machine with no Codex has none of them and reads as it always
    // did.
    let mut in_codex = false;
    for c in checks {
        if !in_codex && is_codex(c) {
            in_codex = true;
            out.push_str(&format!("\n{}\n", paint(BOLD, "Codex")));
        }
        let width = if is_codex(c) {
            codex_width
        } else {
            claude_width
        };
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
    let count = |level| checks.iter().filter(|c| c.level == level).count();
    let summary = match (count(Level::Fail), count(Level::Warn)) {
        (0, 0) => paint(GOOD, "Everything pitboard relies on holds."),
        (0, w) => paint(WARN, format!("{w} to look at; nothing is broken.")),
        (f, _) => paint(
            BAD,
            format!("{f} broken: do not switch accounts until fixed."),
        ),
    };
    out.push_str(&format!("\n{summary}\n"));
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

    #[test]
    fn the_summary_says_whether_anything_is_broken() {
        let plain = |checks: &[Check]| anstream::adapter::strip_str(&human(checks)).to_string();
        assert!(plain(&[check(Level::Ok)]).ends_with("Everything pitboard relies on holds.\n"));
        assert!(plain(&[check(Level::Ok), check(Level::Warn)]).contains("1 to look at"));
        assert!(plain(&[check(Level::Fail)]).contains("1 broken"));
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
}
