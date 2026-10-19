use crate::provider::folder::Class;
use std::ffi::OsStr;

// Read from a listing of a data folder under 2.7032.0, the items E2 moved under 2.19675.0, and
// 2.42304.0's code and framework: the register's `desktop_*` entries each group names.
const ACCOUNT: &[&str] = &[
    "Cookies",
    "Cookies-journal",
    "Local Storage",
    "Session Storage",
    "IndexedDB",
    "WebStorage",
    "File System",
    "Service Worker",
    "Platform Notifications",
    "CdmStorage.db",
    "CdmStorage.db-journal",
    "declarative_performance_observer.db",
    "declarative_performance_observer.db-journal",
    "Partitions",
    "chrome-import-sites.json",
    "builtin-mcp-auth",
    "custom3p-builtin-mcp.json",
    "session-popouts.json",
    "local-sessions.json",
];

const STAYS: &[&str] = &[
    "config.json",
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
    "DIPS-wal",
    "Network Persistent State",
    "TransportSecurity",
    "Trust Tokens",
    "Trust Tokens-journal",
    "shared_proto_db",
    "VideoDecodeStats",
    "WebrtcVideoStats",
    "Custom Dictionary.txt",
    "Custom Dictionary.txt.backup",
    "extensions",
    "Local Extension Settings",
    "Sync Extension Settings",
    "Managed Extension Settings",
    "Extensions Update Cache",
    "Claude Extensions",
    "Claude Extensions Settings",
    "extensions-installations.json",
    ".DS_Store",
    // `desktop_machine_entries`
    "ccd-ids.json",
    "claude-code-spare",
    "claude-code-vm",
    "claude-ssh-remote",
    "git-shadow",
    "ssh",
    "ssh-cli-cache",
    "ssh_configs.json",
    "ssh_worktree_locations.json",
    "ssh-launch-preconnect.json",
    "ssh-remote-server-state.json",
    "ssh-remembered-passwords.json",
    "terminal-scrollback",
    "terminal-shell-integration",
    "worktree-dep-seed",
    "buddy-state.json",
    "buddy-tokens.json",
    "cowork-file-preview",
    "cowork-spawn-settings",
    "document-exports",
    "rpm",
    "edr-integration-runs.json",
    "uv-runtime",
    "office-addin",
    "import-bundles",
    "claude-ai-import-downloads",
    "sibling-import-declined.json",
    "claude-plugins",
    "marketplace-plugin-assets",
    "dxt-staging",
    "ext-icon-probe",
    "builtin-mcp-incidents.json",
    "custom3p-mcp-schemas.json",
    "developer_settings.json",
    "sim-recordings",
    "simulator-airplane-mode-avds.json",
    "simulator-builds",
    "simulator-logs",
    "renderer-logs-truncated",
    // `desktop_kept_by_account`
    "ccd-environment-config.json",
    "ccd-session-presets.json",
    "asides",
    "side-session-notes",
    "remote-bash-outputs",
    "scratch-workspaces",
    "preview-last-frames",
    "framebuffer-ledgers",
    "cowork-managed",
    "cowork-deletion-outbox.jsonl",
    "cowork-session-seed",
    "cowork-plugin-oauth.json",
    "cowork-plugin-env.json",
    "scheduled-task-grants",
    "library-submissions",
    "memory-migration-state.json",
    "dispatch-footprint-census.json",
    "document-baselines",
    "pending-uploads",
    "vm_bundles",
    "bridge-state.json",
    "previous-profile-additions.json",
    "sibling-import.json",
    "usage-metrics",
    "space-memory-copy",
    "mcp-user-tool-toggles.json",
    "ant-device-key-generation.json",
    "remote-control-state.json",
];

// Each keeps `<file>.journal` beside it, which goes where the file goes.
const STORES: &[&str] = &[
    "config.json",
    "git-worktrees.json",
    "buddy-state.json",
    "buddy-tokens.json",
    "ssh-launch-preconnect.json",
    "ssh-remote-server-state.json",
    "ssh-remembered-passwords.json",
    "ccd-environment-config.json",
    "ccd-session-presets.json",
    "cowork-enabled-cli-ops.json",
    "cowork-plugin-env.json",
    "cowork-plugin-oauth.json",
    "custom3p-builtin-mcp.json",
    "custom3p-mcp-schemas.json",
];

const PASSING: &[&str] = &[
    "stealth-relaunch",
    "update-attempt",
    "databases",
    "claude-ai-import-staging-remotePull",
    "claude-ai-import-staging-localZip",
];

// `mkdtemp` adds six letters or digits.
const MADE_BY_MKDTEMP: &[&str] = &[
    "dxt-install-",
    "dxt-stage-",
    "dxt-unpacked-",
    ".cowork-managed-migrating-",
    "WebStorage-doomed-",
];

pub(super) fn classify(name: &OsStr) -> Class {
    name.to_str().map_or(Class::Unread, class)
}

fn class(name: &str) -> Class {
    if ACCOUNT.contains(&name) {
        Class::Account
    } else if STAYS.contains(&name) {
        Class::Stays
    } else if let Some(store) = name
        .strip_suffix(".journal")
        .filter(|store| STORES.contains(store))
    {
        class(store)
    } else if passing(name) {
        Class::Passing
    } else {
        Class::Unread
    }
}

const TEAM_ID: &str = "Q6L2SF6YDW";

fn passing(name: &str) -> bool {
    PASSING.contains(&name)
        || crate::atomic::temp_owner(name).is_some()
        || MADE_BY_MKDTEMP
            .iter()
            .any(|prefix| name.strip_prefix(prefix).is_some_and(six_letters_or_digits))
        || set_aside(name)
        || store_temporary(name)
        || written_beside(name).is_some_and(|file| class(file) != Class::Unread)
        || chromium_temporary(name)
            .is_some_and(|(file, made)| STAYS.contains(&file) && six_letters_or_digits(made))
}

fn six_letters_or_digits(made: &str) -> bool {
    made.len() == 6 && made.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

// A file the app could not read, renamed with the milliseconds it gave up.
fn set_aside(name: &str) -> bool {
    [
        "config.json.corrupt-",
        "git-worktrees.json.corrupt-",
        "cowork-deletion-outbox.jsonl.unreadable-",
    ]
    .iter()
    .any(|prefix| name.strip_prefix(prefix).is_some_and(digits))
        || name
            .strip_prefix("memory-migration-state.json.corrupt-")
            .and_then(|rest| rest.split_once('-'))
            .is_some_and(|(made, hash)| {
                digits(made)
                    && hash.len() == 8
                    && hash
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            })
}

// A store's write: `<file>.tmp-`, the last ten digits of the milliseconds and six hex digits.
fn store_temporary(name: &str) -> bool {
    STORES.iter().any(|store| {
        name.strip_prefix(store)
            .and_then(|rest| rest.strip_prefix(".tmp-"))
            .map(str::as_bytes)
            .is_some_and(|made| {
                made.len() == 16
                    && made[..10].iter().all(u8::is_ascii_digit)
                    && made[10..]
                        .iter()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            })
    })
}

// The app's two writers of a whole file: `<file>.tmp`, and `.<file>.<pid>.` with one to six
// base-36 digits and `.tmp`.
fn written_beside(name: &str) -> Option<&str> {
    if let Some(file) = name
        .strip_suffix(".tmp")
        .filter(|file| !file.starts_with('.'))
    {
        return Some(file);
    }
    let (rest, made) = name
        .strip_prefix('.')?
        .strip_suffix(".tmp")?
        .rsplit_once('.')?;
    let (file, pid) = rest.rsplit_once('.')?;
    let random = (1..=6).contains(&made.len())
        && made
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase());
    (digits(pid) && random).then_some(file)
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
            "Service Worker",
            "Platform Notifications",
            "CdmStorage.db",
            "CdmStorage.db-journal",
            "declarative_performance_observer.db",
            "declarative_performance_observer.db-journal",
            "Partitions",
            "chrome-import-sites.json",
            "builtin-mcp-auth",
            "custom3p-builtin-mcp.json",
            "custom3p-builtin-mcp.json.journal",
            "session-popouts.json",
            "local-sessions.json",
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
            "DIPS-wal",
            "Network Persistent State",
            "TransportSecurity",
            "Trust Tokens",
            "Trust Tokens-journal",
            "shared_proto_db",
            "VideoDecodeStats",
            "WebrtcVideoStats",
            "Custom Dictionary.txt",
            "Custom Dictionary.txt.backup",
            "extensions",
            "Local Extension Settings",
            "Sync Extension Settings",
            "Managed Extension Settings",
            "Extensions Update Cache",
            "Claude Extensions",
            "Claude Extensions Settings",
            "extensions-installations.json",
            ".DS_Store",
            "ccd-ids.json",
            "claude-code-spare",
            "claude-code-vm",
            "claude-ssh-remote",
            "git-shadow",
            "ssh",
            "ssh-cli-cache",
            "ssh_configs.json",
            "ssh_worktree_locations.json",
            "ssh-launch-preconnect.json",
            "ssh-remote-server-state.json",
            "ssh-remembered-passwords.json",
            "terminal-scrollback",
            "terminal-shell-integration",
            "worktree-dep-seed",
            "buddy-state.json",
            "buddy-tokens.json",
            "cowork-file-preview",
            "cowork-spawn-settings",
            "document-exports",
            "rpm",
            "edr-integration-runs.json",
            "uv-runtime",
            "office-addin",
            "import-bundles",
            "claude-ai-import-downloads",
            "sibling-import-declined.json",
            "claude-plugins",
            "marketplace-plugin-assets",
            "dxt-staging",
            "ext-icon-probe",
            "builtin-mcp-incidents.json",
            "custom3p-mcp-schemas.json",
            "developer_settings.json",
            "sim-recordings",
            "simulator-airplane-mode-avds.json",
            "simulator-builds",
            "simulator-logs",
            "renderer-logs-truncated",
            "ccd-environment-config.json",
            "ccd-session-presets.json",
            "asides",
            "side-session-notes",
            "remote-bash-outputs",
            "scratch-workspaces",
            "preview-last-frames",
            "framebuffer-ledgers",
            "cowork-managed",
            "cowork-deletion-outbox.jsonl",
            "cowork-session-seed",
            "cowork-plugin-oauth.json",
            "cowork-plugin-env.json",
            "scheduled-task-grants",
            "library-submissions",
            "memory-migration-state.json",
            "dispatch-footprint-census.json",
            "document-baselines",
            "pending-uploads",
            "vm_bundles",
            "bridge-state.json",
            "previous-profile-additions.json",
            "sibling-import.json",
            "usage-metrics",
            "space-memory-copy",
            "mcp-user-tool-toggles.json",
            "ant-device-key-generation.json",
            "remote-control-state.json",
            "git-worktrees.json.journal",
            "buddy-tokens.json.journal",
            "cowork-plugin-oauth.json.journal",
            "custom3p-mcp-schemas.json.journal",
        ];
        for name in account {
            assert_eq!(classify(OsStr::new(name)), Class::Account, "{name}");
        }
        for name in stays {
            assert_eq!(classify(OsStr::new(name)), Class::Stays, "{name}");
        }
        assert_eq!((account.len(), ACCOUNT.len()), (20, 19));
        assert_eq!((stays.len(), STAYS.len()), (123, 118));
        for name in [
            ".config.json.4242.1.pitboard",
            ".config.json.4242.pitboard",
            "config.json.corrupt-1791000000123",
            "git-worktrees.json.corrupt-1791000000123",
            "cowork-deletion-outbox.jsonl.unreadable-1791000000123",
            "memory-migration-state.json.corrupt-1791000000123-0a1b2c3d",
            "config.json.tmp-1000000123a1b2c3",
            "cowork-plugin-oauth.json.tmp-1000000123a1b2c3",
            "bridge-state.json.tmp",
            "update-attempt.tmp",
            "session-popouts.json.tmp",
            ".ccd-ids.json.4242.k3x9z.tmp",
            ".session-popouts.json.4242.a.tmp",
            ".Q6L2SF6YDW.com.anthropic.claudefordesktop.Preferences.a1B2c3",
            ".Q6L2SF6YDW.com.anthropic.claudefordesktop.Local State.Zz09Ab",
            ".Q6L2SF6YDW.com.anthropic.claudefordesktop.Custom Dictionary.txt.Zz09Ab",
            "dxt-install-a1B2c3",
            "dxt-stage-a1B2c3",
            "dxt-unpacked-a1B2c3",
            ".cowork-managed-migrating-a1B2c3",
            "WebStorage-doomed-a1B2c3",
            "stealth-relaunch",
            "update-attempt",
            "databases",
            "claude-ai-import-staging-remotePull",
            "claude-ai-import-staging-localZip",
        ] {
            assert_eq!(classify(OsStr::new(name)), Class::Passing, "{name}");
        }
        for name in [
            "Network",
            "Cookies-wal",
            "Cookies-shm",
            "SingletonLock",
            "SingletonSocket",
            "SingletonCookie",
            "QuotaManager",
            "MediaDeviceSalts",
            "configLibrary",
            "connectors",
            "ccd-session-secrets",
            "cli-probe",
            "host-creds-0a1b2c3d-4e5f-4a7b-8c9d-0e1f2a3b4c5d.json",
            "gpu-crash-streak",
            "cowork-user-files",
            "config.json.corrupt-",
            "config.json.corrupt-12a",
            "memory-migration-state.json.corrupt-1791000000123-0A1B2C3D",
            "memory-migration-state.json.corrupt-1791000000123",
            "config.json.tmp-1000000123a1b2c",
            "config.json.tmp-1000000123A1B2C3",
            "something-new.json.journal",
            "something-new.json.tmp",
            ".something-new.json.4242.a.tmp",
            ".session-popouts.json.4242.a1b2c3d.tmp",
            ".session-popouts.json.42x.a.tmp",
            ".session-popouts.json.4242.A.tmp",
            "Cookies.journal",
            "dxt-stage-a1B2c",
            "dxt-stage-a1B2c!",
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

    #[test]
    fn every_account_name_is_an_item_the_move_carries() {
        let firsts: Vec<&str> = super::super::ITEMS
            .iter()
            .map(|item| item.split('/').next().expect("a name"))
            .collect();
        for first in &firsts {
            assert_eq!(classify(OsStr::new(first)), Class::Account, "{first}");
        }
        for name in ACCOUNT {
            assert!(firsts.contains(name), "{name}");
        }
        for store in STORES {
            if class(store) == Class::Account {
                let journal = format!("{store}.journal");
                assert!(firsts.contains(&journal.as_str()), "{journal}");
            }
        }
    }
}
