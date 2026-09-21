//! `pitboard doctor` for a person and for a program.

use crate::ui::{self, BAD, DIM, GOOD, WARN, pad, paint};
use pitboard_core::doctor::{Check, Diagnosis, Level};
use serde_json::{Value, json};

pub fn human(checks: &[Check]) -> String {
    let width = checks
        .iter()
        .map(|c| ui::columns(&c.name))
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for c in checks {
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

pub fn json(diagnosis: &Diagnosis) -> Value {
    json!({
        "environment": diagnosis.environment,
        "checks": diagnosis.checks.iter().map(|c| json!({
            "code": c.code,
            "name": c.name,
            "level": match c.level { Level::Ok => "ok", Level::Warn => "warn", Level::Fail => "fail" },
            "detail": c.detail,
            "advice": c.advice,
        })).collect::<Vec<_>>(),
    })
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
}
