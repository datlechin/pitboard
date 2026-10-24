//! Which process images the probe may name. A process list holds every application the
//! account runs; the probe names only the tools Pitboard switches, the runtimes they run
//! under, Pitboard and the probe, and (for the Job Object and sign-in blocks) the browsers a
//! sign-in opens and the shell's programs it opens them through. Every other process is
//! counted, never named.

/// Whether `image` (a base name such as `codex.exe`) is a tool, a runtime one runs under,
/// Pitboard or the probe: what `processes --find` may ask for.
pub fn is_tool_image(image: &str) -> bool {
    let lower = image.to_lowercase();
    let Some(stem) = lower.strip_suffix(".exe") else {
        return false;
    };
    ["claude", "codex", "chatgpt", "pitboard"]
        .iter()
        .any(|p| stem.starts_with(p))
        || matches!(stem, "node" | "bun")
}

/// The browsers block F2 and the windowless sign-in look for.
pub const BROWSER_IMAGES: [&str; 3] = ["msedge.exe", "chrome.exe", "firefox.exe"];

pub fn is_browser_image(image: &str) -> bool {
    BROWSER_IMAGES.iter().any(|b| b.eq_ignore_ascii_case(image))
}

/// What Claude Code's sign-in starts to open a page, and what the shell asks with where no
/// browser answers for `http`.
pub const SHELL_IMAGES: [&str; 2] = ["rundll32.exe", "OpenWith.exe"];

pub fn is_shell_image(image: &str) -> bool {
    SHELL_IMAGES.iter().any(|s| s.eq_ignore_ascii_case(image))
}

/// What a report may say a process's image is: its name when it is a tool's, a browser's or
/// one of the shell's, else that it is something else.
pub fn reportable(image: &str) -> &str {
    if is_tool_image(image) || is_browser_image(image) || is_shell_image(image) {
        image
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tools_runtimes_pitboard_and_the_probe_may_be_named() {
        for ok in [
            "claude.exe",
            "Claude.exe",
            "codex.exe",
            "codex-command-runner.exe",
            "ChatGPT.exe",
            "node.exe",
            "bun.exe",
            "pitboard.exe",
            "pitboard-probe-detached.exe",
        ] {
            assert!(is_tool_image(ok), "{ok}");
        }
    }

    #[test]
    fn nothing_else_is_named() {
        for other in [
            "explorer.exe",
            "OneDrive.exe",
            "svchost.exe",
            "claude",
            "nodejs.exe",
        ] {
            assert!(!is_tool_image(other), "{other}");
            assert_eq!(reportable(other), "other");
        }
        assert_eq!(reportable("msedge.exe"), "msedge.exe");
        assert_eq!(reportable("codex.exe"), "codex.exe");
        assert_eq!(reportable("OpenWith.exe"), "OpenWith.exe");
        assert_eq!(reportable("rundll32.exe"), "rundll32.exe");
        assert_eq!(reportable("rundll32"), "other");
    }
}
