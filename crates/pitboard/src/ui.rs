//! Terminal presentation: styles, and column arithmetic that counts what a terminal shows.
//! Text is written through anstream, which drops the styling when the output is not a
//! terminal, or when `NO_COLOR` asks it to.

use anstyle::{AnsiColor, Style};
use std::fmt::Display;

pub const BOLD: Style = Style::new().bold();
pub const DIM: Style = Style::new().dimmed();
pub const GOOD: Style = AnsiColor::Green.on_default();
pub const WARN: Style = AnsiColor::Yellow.on_default();
pub const BAD: Style = AnsiColor::Red.on_default();

pub fn paint(style: Style, text: impl Display) -> String {
    format!("{style}{text}{style:#}")
}

/// Left-aligned in `width` columns. Pad before painting: escape codes take no columns.
pub fn pad(text: &str, width: usize) -> String {
    format!("{text:<width$}")
}

/// How alarming a share of a limit is.
pub fn level(percent: f64) -> Style {
    if percent >= 90.0 {
        BAD
    } else if percent >= 70.0 {
        WARN
    } else {
        GOOD
    }
}

pub fn bar(percent: f64, width: usize) -> String {
    let filled = ((percent / 100.0) * width as f64)
        .round()
        .clamp(0.0, width as f64) as usize;
    format!(
        "{}{}",
        paint(level(percent), "█".repeat(filled)),
        paint(DIM, "░".repeat(width - filled))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(styled: &str) -> String {
        anstream::adapter::strip_str(styled).to_string()
    }

    #[test]
    fn bars_are_the_width_they_claim() {
        assert_eq!(plain(&bar(0.0, 10)), "░░░░░░░░░░");
        assert_eq!(plain(&bar(62.0, 10)), "██████░░░░");
        assert_eq!(plain(&bar(150.0, 10)), "██████████");
    }

    #[test]
    fn padding_counts_characters_not_bytes() {
        assert_eq!(pad("░", 3), "░  ");
        assert_eq!(pad("longer", 3), "longer");
    }
}
