// Over-broad on purpose: what a test and CI's leak check refuse, never what Pitboard looks up.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    ClaudeCode,
    CodexCli,
    CodexSecrets,
    CodexMcp,
}

impl Family {
    pub fn as_str(self) -> &'static str {
        match self {
            Family::ClaudeCode => "claude_code_credentials",
            Family::CodexCli => "codex_cli",
            Family::CodexSecrets => "codex_secrets",
            Family::CodexMcp => "codex_mcp_credentials",
        }
    }
}

pub fn family(name: &str) -> Option<Family> {
    // Credential Manager ignores case, and cmdkey shows a generic target behind this prefix.
    let lower = name.to_lowercase();
    let lower = lower
        .strip_prefix("legacygeneric:target=")
        .unwrap_or(&lower);
    // keyring-rs names a Windows target `<user>.<service>`, so the MCP store's name may come last.
    let mcp =
        lower.starts_with("codex mcp credentials") || lower.ends_with(".codex mcp credentials");
    if lower.starts_with("claude code") && lower.contains("-credentials") {
        Some(Family::ClaudeCode)
    } else if mcp {
        Some(Family::CodexMcp)
    } else if lower.starts_with("cli|") {
        Some(Family::CodexCli)
    } else if lower.starts_with("secrets|") {
        Some(Family::CodexSecrets)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::claude::slot::{LIVE_SERVICE, service_for_dir};

    #[test]
    fn every_claude_code_slot_and_piece_is_claude_codes() {
        for name in [
            LIVE_SERVICE.to_owned(),
            service_for_dir("/Users/someone/.claude"),
            "Claude Code-credentials/claude-code-user".into(),
            "Claude Code-credentials-e80beed8#0".into(),
            "Claude Code-credentials-e80beed8/claude-code-user#m".into(),
            "Claude Code-credentials-e80beed8#p".into(),
            "Claude Code-staging-credentials".into(),
            "claude code-credentials/CLAUDE-CODE-USER".into(),
            "LegacyGeneric:target=Claude Code-credentials".into(),
        ] {
            assert_eq!(family(&name), Some(Family::ClaudeCode), "{name}");
        }
    }

    #[test]
    fn every_codex_store_is_codexs() {
        for (name, expected) in [
            ("cli|abc123", Family::CodexCli),
            ("CLI|ABC123.Codex Auth", Family::CodexCli),
            ("secrets|abc123", Family::CodexSecrets),
            ("Codex MCP Credentials", Family::CodexMcp),
            ("Codex MCP Credentials/linear", Family::CodexMcp),
            ("linear|3f2a.Codex MCP Credentials", Family::CodexMcp),
            (
                "LegacyGeneric:target=x.codex mcp credentials",
                Family::CodexMcp,
            ),
        ] {
            assert_eq!(family(name), Some(expected), "{name}");
        }
    }

    #[test]
    fn everybody_elses_names_are_no_family() {
        for name in [
            "GitHub - https://github.com",
            "MicrosoftAccount:user=someone@example.com",
            "LegacyGeneric:target=some-app",
            "Claude something else",
            "my cli| thing",
            "pitboard-park-0a1b2c3d-1",
        ] {
            assert_eq!(family(name), None, "{name}");
        }
    }
}
