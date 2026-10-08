use crate::PROBE_PREFIX;
pub use pitboard_core::provider::names::Family;

pub const CITEST_PREFIX: &str = "pitboard-citest-";

// One CredEnumerateW each, so no other application's item is ever loaded into the probe.
pub const ENUMERATION_FILTERS: [&str; 5] = [
    "Claude Code*",
    "cli|*",
    "secrets|*",
    "Codex MCP Credentials*",
    "pitboard-*",
];

// Credential Manager filters by prefix alone, so no enumeration can reach these.
pub const UNLISTABLE_FORMS: [&str; 1] = ["<key>.Codex MCP Credentials"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Live(Family),
    CiTest,
    Probe,
    Other,
}

impl Kind {
    pub fn is_leak(self) -> bool {
        matches!(self, Kind::Live(_) | Kind::CiTest)
    }

    pub fn is_reportable(self) -> bool {
        !matches!(self, Kind::Other)
    }
}

pub fn classify(target: &str) -> Kind {
    let lower = target.to_lowercase();
    let lower = lower
        .strip_prefix("legacygeneric:target=")
        .unwrap_or(&lower);
    // Before the families, so a name with one of these prefixes is never taken for a live login.
    if lower.starts_with(PROBE_PREFIX) {
        return Kind::Probe;
    }
    if lower.starts_with(CITEST_PREFIX) {
        return Kind::CiTest;
    }
    pitboard_core::provider::names::family(target).map_or(Kind::Other, Kind::Live)
}

pub fn is_probe_item_name(name: &str) -> bool {
    name.len() > PROBE_PREFIX.len()
        && name.len() <= 256
        && name.starts_with(PROBE_PREFIX)
        && name.bytes().all(|b| b.is_ascii_graphic())
        && classify(name) == Kind::Probe
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_match_folds_case_the_way_credential_manager_does() {
        assert_eq!(
            classify("claude code-credentials"),
            Kind::Live(Family::ClaudeCode)
        );
        assert_eq!(classify("CLI|X"), Kind::Live(Family::CodexCli));
        assert_eq!(classify("PITBOARD-PROBE-x"), Kind::Probe);
    }

    #[test]
    fn the_probe_and_the_tests_name_their_own() {
        assert_eq!(classify("pitboard-probe-vault-1"), Kind::Probe);
        assert_eq!(classify("pitboard-citest-abcd"), Kind::CiTest);
    }

    #[test]
    fn everyone_elses_items_are_other_and_say_nothing() {
        for name in [
            "GitHub - https://github.com",
            "MicrosoftAccount:user=someone@example.com",
            "LegacyGeneric:target=some-app",
            "Claude something else",
            "my cli| thing",
        ] {
            let k = classify(name);
            assert_eq!(k, Kind::Other, "{name}");
            assert!(!k.is_reportable());
            assert!(!k.is_leak());
        }
    }

    #[test]
    fn a_leak_is_a_live_family_or_a_citest_name() {
        assert!(classify("Claude Code-credentials").is_leak());
        assert!(classify("pitboard-citest-x").is_leak());
        assert!(!classify("pitboard-probe-x").is_leak());
        assert!(!classify("GitHub").is_leak());
    }

    #[test]
    fn a_probe_prefix_wins_over_a_family_lookalike() {
        assert_eq!(classify("pitboard-probe-cli|x"), Kind::Probe);
    }

    #[test]
    fn every_enumeration_filter_is_a_prefix_of_a_reportable_name() {
        for filter in ENUMERATION_FILTERS {
            let stem = filter.trim_end_matches('*');
            assert!(filter.ends_with('*') && !stem.contains('*'), "{filter}");
            // A name the filter returns is reportable once it is a family's full form.
        }
        assert!(classify("Claude Code-credentials").is_reportable());
        assert!(classify("cli|x").is_reportable());
        assert!(classify("secrets|x").is_reportable());
        assert!(classify("Codex MCP Credentials").is_reportable());
        assert!(classify("pitboard-probe-x").is_reportable());
    }

    #[test]
    fn the_probe_writes_only_its_own_names() {
        assert!(is_probe_item_name("pitboard-probe-m1"));
        for bad in [
            "pitboard-probe-",
            "Claude Code-credentials",
            "pitboard-citest-x",
            "cli|x",
            "pitboard-probe-a b",
            "x-pitboard-probe-y",
        ] {
            assert!(!is_probe_item_name(bad), "{bad}");
        }
    }
}
