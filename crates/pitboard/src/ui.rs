//! Terminal presentation: styles, and column arithmetic that counts what a terminal shows.
//! Text is written through anstream, which drops the styling when the output is not a
//! terminal, or when `NO_COLOR` asks it to.

use anstyle::{AnsiColor, Style};
use std::fmt::Display;
use unicode_width::UnicodeWidthStr;

pub const BOLD: Style = Style::new().bold();
pub const DIM: Style = Style::new().dimmed();
pub const GOOD: Style = AnsiColor::Green.on_default();
pub const WARN: Style = AnsiColor::Yellow.on_default();
pub const BAD: Style = AnsiColor::Red.on_default();

pub fn paint(style: Style, text: impl Display) -> String {
    format!("{style}{text}{style:#}")
}

/// Left-aligned in `width` columns of a terminal, which is not the same as characters: an
/// accented letter can be one column and a CJK one is two. Pad before painting: escape
/// codes take no columns at all.
pub fn pad(text: &str, width: usize) -> String {
    let shown = UnicodeWidthStr::width(text);
    format!("{text}{}", " ".repeat(width.saturating_sub(shown)))
}

/// What a terminal gives this text, for working out a column width.
pub fn columns(text: &str) -> usize {
    UnicodeWidthStr::width(text)
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

    /// A terminal lines columns up by what it draws. A CJK character is two columns and an
    /// accented Latin one is still one, so counting characters puts everything after a name
    /// out of line.
    #[test]
    fn padding_counts_what_a_terminal_shows() {
        assert_eq!(pad("░", 3), "░  ");
        assert_eq!(pad("longer", 3), "longer", "never truncates");
        assert_eq!(pad("công", 8), "công    ", "an accent is one column");
        assert_eq!(pad("工作", 8), "工作    ", "and a CJK character is two");
        assert_eq!(columns("工作"), 4);
    }
}
