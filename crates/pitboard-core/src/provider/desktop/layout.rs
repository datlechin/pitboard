use crate::provider::folder::Class;
use std::ffi::OsStr;

// Read from a listing of a data folder under 2.7032.0, the items E2 moved under 2.19675.0 and
// the GPU caches 2.42304.0's Electron names.
const ACCOUNT: &[&str] = &[
    "Cookies",
    "Cookies-journal",
    "Local Storage",
    "Session Storage",
    "IndexedDB",
    "WebStorage",
    "File System",
];

const STAYS: &[&str] = &[
    "config.json",
    "config.json.journal",
    "claude-code-sessions",
    "local-agent-mode-sessions",
    "spaces-present",
    "fcache",
    "plan-usage-history.json",
    "cowork-enabled-cli-ops.json",
    "ant-device-registry.json",
    "git-worktrees.json",
    "vm-support-probe.json",
    "extensions-blocklist.json",
    "ca-bundle.pem",
    "declarative_performance_observer.db",
    "ant-did",
    "claude_desktop_config.json",
    "claude-code",
    "window-state.json",
    "Local State",
    "Preferences",
    "Crashpad",
    "sentry",
    "Cache",
    "Code Cache",
    "GPUCache",
    "DawnGraphiteCache",
    "DawnWebGPUCache",
    "ShaderCache",
    "GrShaderCache",
    "GraphiteDawnCache",
    "GPUPersistentCache",
    "blob_storage",
    "Shared Dictionary",
    "DIPS",
    "Network Persistent State",
    "TransportSecurity",
    "Trust Tokens",
    "Claude Extensions",
    "Claude Extensions Settings",
    "extensions-installations.json",
    ".DS_Store",
];

pub(super) fn classify(name: &OsStr) -> Class {
    let Some(name) = name.to_str() else {
        return Class::Unread;
    };
    if ACCOUNT.contains(&name) {
        Class::Account
    } else if STAYS.contains(&name) {
        Class::Stays
    } else if passing(name) {
        Class::Passing
    } else {
        Class::Unread
    }
}

const TEAM_ID: &str = "Q6L2SF6YDW";

fn passing(name: &str) -> bool {
    crate::atomic::temp_owner(name).is_some()
        || name
            .strip_prefix("config.json.corrupt-")
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        || chromium_temporary(name).is_some_and(|(file, made)| {
            STAYS.contains(&file)
                && made.len() == 6
                && made.bytes().all(|b| b.is_ascii_alphanumeric())
        })
}

// Chromium's temporary beside a file it writes, under Claude's team and bundle ids: the
// register's `desktop_chromium_temporary`.
fn chromium_temporary(name: &str) -> Option<(&str, &str)> {
    name.strip_prefix('.')?
        .strip_prefix(TEAM_ID)?
        .strip_prefix('.')?
        .strip_prefix(super::BUNDLE_ID)?
        .strip_prefix('.')?
        .rsplit_once('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_classifies_as_its_row_says() {
        // Written out, not read from the tables `classify` reads: a name dropped there fails.
        let account = [
            "Cookies",
            "Cookies-journal",
            "Local Storage",
            "Session Storage",
            "IndexedDB",
            "WebStorage",
            "File System",
        ];
        let stays = [
            "config.json",
            "config.json.journal",
            "claude-code-sessions",
            "local-agent-mode-sessions",
            "spaces-present",
            "fcache",
            "plan-usage-history.json",
            "cowork-enabled-cli-ops.json",
            "ant-device-registry.json",
            "git-worktrees.json",
            "vm-support-probe.json",
            "extensions-blocklist.json",
            "ca-bundle.pem",
            "declarative_performance_observer.db",
            "ant-did",
            "claude_desktop_config.json",
            "claude-code",
            "window-state.json",
            "Local State",
            "Preferences",
            "Crashpad",
            "sentry",
            "Cache",
            "Code Cache",
            "GPUCache",
            "DawnGraphiteCache",
            "DawnWebGPUCache",
            "ShaderCache",
            "GrShaderCache",
            "GraphiteDawnCache",
            "GPUPersistentCache",
            "blob_storage",
            "Shared Dictionary",
            "DIPS",
            "Network Persistent State",
            "TransportSecurity",
            "Trust Tokens",
            "Claude Extensions",
            "Claude Extensions Settings",
            "extensions-installations.json",
            ".DS_Store",
        ];
        for name in account {
            assert_eq!(classify(OsStr::new(name)), Class::Account, "{name}");
        }
        for name in stays {
            assert_eq!(classify(OsStr::new(name)), Class::Stays, "{name}");
        }
        assert_eq!((account.len(), ACCOUNT.len()), (7, 7));
        assert_eq!((stays.len(), STAYS.len()), (41, 41));
        for name in [
            ".config.json.4242.1.pitboard",
            ".config.json.4242.pitboard",
            "config.json.corrupt-1791000000123",
            ".Q6L2SF6YDW.com.anthropic.claudefordesktop.Preferences.a1B2c3",
            ".Q6L2SF6YDW.com.anthropic.claudefordesktop.Local State.Zz09Ab",
        ] {
            assert_eq!(classify(OsStr::new(name)), Class::Passing, "{name}");
        }
        for name in [
            "Partitions",
            "Service Worker",
            "session-popouts.json",
            "cowork-plugin-oauth.json",
            "cowork-plugin-env.json",
            "ccd-environment-config.json",
            "ccd-session-presets.json",
            "buddy-tokens.json",
            "developer_settings.json",
            "configLibrary",
            "Network",
            "Cookies-wal",
            "Cookies-shm",
            "SingletonLock",
            "SingletonSocket",
            "SingletonCookie",
            "config.json.corrupt-",
            "config.json.corrupt-12a",
            ".org.chromium.Chromium.a1B2c3",
            ".Q6L2SF6YDW.com.anthropic.claudefordesktop.Preferences.abc",
            ".Q6L2SF6YDW.com.anthropic.claudefordesktop.Preferences.a1B2c!",
            ".Q6L2SF6YDW.com.anthropic.claudefordesktop.Cookies.a1B2c3",
            ".Q6L2SF6YDW.com.anthropic.claudefordesktop.something-new.a1B2c3",
            ".com.anthropic.claudefordesktop.Preferences.a1B2c3",
            "something-new",
        ] {
            assert_eq!(classify(OsStr::new(name)), Class::Unread, "{name}");
        }
    }
}
