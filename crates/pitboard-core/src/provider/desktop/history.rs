//! Claude Desktop's usage as the app itself wrote it down, read without the network and
//! without Claude's key.
//!
//! `plan-usage-history.json` is the machine's, not the account's: it stays where it is
//! across a switch and holds samples for every organisation the app has shown usage for.
//! So a sample is taken only for an organisation the account is known to belong to. What
//! its numbers mean was measured by experiment E10, which matched them against claude.ai's
//! own answer, so a read is marked verified while the register says so.

// Read by status, offline and not, which this file is the boundary for.
#![allow(dead_code)]

use super::paths::{history_file, support_dir};
use crate::context::Context;
use crate::provider::ProviderId;
use crate::state::{Account, Detail};
use crate::usage::{self, Snapshot, Source, Window};
use serde::Deserialize;

/// The register's entry for what `fh` and `sd` mean.
pub(crate) const HISTORY_MEANING: &str = "desktop_usage_history_meaning";

/// The only version of the file this was read from.
const VERSION: u32 = 2;

/// The folders the app keeps per account and organisation, each holding one folder per
/// organisation the account has used.
const SESSION_FOLDERS: [&str; 2] = ["claude-code-sessions", "local-agent-mode-sessions"];

#[derive(Debug, Deserialize)]
struct History {
    version: u32,
    #[serde(default)]
    samples: Vec<Sample>,
}

#[derive(Debug, Deserialize)]
struct Sample {
    /// Epoch milliseconds.
    t: i64,
    org: String,
    u: Used,
}

#[derive(Debug, Deserialize)]
struct Used {
    fh: Option<f64>,
    sd: Option<f64>,
}

/// The organisations `account_uuid` has used in the app on this machine, by the folders it
/// keeps for them, sorted.
pub(crate) fn orgs_of(ctx: &Context, account_uuid: &str) -> Vec<String> {
    let Some(root) = support_dir(ctx) else {
        return Vec::new();
    };
    // A uuid that is not one would reach outside the folders asked about.
    if account_uuid.is_empty() || account_uuid.contains(['/', '.']) {
        return Vec::new();
    }
    let mut orgs: Vec<String> = SESSION_FOLDERS
        .iter()
        .filter_map(|folder| std::fs::read_dir(root.join(folder).join(account_uuid)).ok())
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    orgs.sort();
    orgs.dedup();
    orgs
}

/// The usage the app last wrote down for `account`, where it can be told which of the
/// file's organisations is the account's. `None` where it cannot, where there is no file,
/// and where the file is of a version this was not read from.
pub(crate) fn local_usage(ctx: &Context, account: &Account) -> Option<Snapshot> {
    let raw = std::fs::read_to_string(history_file(ctx)?).ok()?;
    let history: History = serde_json::from_str(&raw).ok()?;
    if history.version != VERSION {
        return None;
    }
    let newest_of = |org: &str| {
        history
            .samples
            .iter()
            .filter(|s| s.org == org)
            .max_by_key(|s| s.t)
    };
    let known = match &account.detail {
        Detail::Desktop {
            organization_uuid: Some(org),
            ..
        } => Some(org.clone()),
        _ => None,
    };
    let sample = match known {
        Some(org) => newest_of(&org),
        None => orgs_of(ctx, &account.account_uuid)
            .iter()
            .filter_map(|org| newest_of(org))
            .max_by_key(|s| s.t),
    }?;
    let window = |kind: &str, percent: Option<f64>| {
        let percent = percent.filter(|p| p.is_finite() && *p >= 0.0)?;
        Some(Window {
            kind: kind.into(),
            scope: None,
            percent,
            resets_at: None,
            is_active: false,
            severity: None,
            length_seconds: usage::anthropic_window_length(kind),
        })
    };
    Some(Snapshot {
        windows: [
            window("five_hour", sample.u.fh),
            window("seven_day", sample.u.sd),
        ]
        .into_iter()
        .flatten()
        .collect(),
        observed_at: Some(sample.t.div_euclid(1000)),
        account_uuid: Some(account.account_uuid.clone()),
        source: Source::DesktopHistory,
        verified: crate::assumptions::verified(ProviderId::Desktop, HISTORY_MEANING),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!(
            "pitboard-desktop-history-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Claude")).unwrap();
        Scratch(root)
    }

    const ACCOUNT: &str = "11111111-1111-1111-1111-111111111111";
    const ORG: &str = "aaaaaaaa-0000-0000-0000-000000000001";
    const OTHER_ORG: &str = "bbbbbbbb-0000-0000-0000-000000000002";
    const SOMEONE_ELSES: &str = "cccccccc-0000-0000-0000-000000000003";

    fn context(root: &Path) -> Context {
        Context::new(root.to_path_buf())
            .with_pitboard_home(root.join(".pitboard"))
            .with_desktop_dir(root.join("Claude").to_string_lossy().into())
    }

    fn account(org: Option<&str>) -> Account {
        Account {
            label: "desk".into(),
            account_uuid: ACCOUNT.into(),
            email: String::new(),
            parked: None,
            last_used_at: None,
            detail: Detail::Desktop {
                organization_uuid: org.map(str::to_owned),
                session_fingerprint: "f".into(),
                session_expires_at: None,
            },
        }
    }

    fn history(root: &Path, samples: &[(i64, &str, f64, f64)]) {
        let samples: Vec<_> = samples
            .iter()
            .map(|(t, org, fh, sd)| serde_json::json!({"t": t, "org": org, "u": {"fh": fh, "sd": sd}}))
            .collect();
        std::fs::write(
            root.join("Claude/plan-usage-history.json"),
            serde_json::json!({"version": 2, "samples": samples}).to_string(),
        )
        .unwrap();
    }

    fn used_in(root: &Path, folder: &str, org: &str) {
        std::fs::create_dir_all(root.join("Claude").join(folder).join(ACCOUNT).join(org)).unwrap();
    }

    fn percents(snapshot: &Snapshot) -> Vec<(String, f64)> {
        snapshot
            .windows
            .iter()
            .map(|w| (w.kind.clone(), w.percent))
            .collect()
    }

    #[test]
    fn the_newest_sample_of_the_accounts_org_is_used() {
        let s = scratch("newest");
        history(
            &s.0,
            &[
                (1_790_000_000_000, ORG, 10.0, 20.0),
                (1_790_000_600_000, ORG, 12.0, 21.0),
                // Newer, and another account's: never shown as this one's.
                (1_790_001_200_000, SOMEONE_ELSES, 99.0, 99.0),
                (1_789_999_000_000, ORG, 5.0, 19.0),
            ],
        );
        let ctx = context(&s.0);
        let read = local_usage(&ctx, &account(Some(ORG))).expect("a sample");
        assert_eq!(
            percents(&read),
            vec![("five_hour".into(), 12.0), ("seven_day".into(), 21.0)]
        );
        assert_eq!(read.observed_at, Some(1_790_000_600));
        assert_eq!(read.source, Source::DesktopHistory);
        assert_eq!(read.windows[0].length_seconds, Some(5 * 3600));
        assert_eq!(read.windows[0].resets_at, None);

        // Without a known organisation, the one folder the app keeps for the account says.
        assert_eq!(
            local_usage(&ctx, &account(None)),
            None,
            "nothing says which"
        );
        used_in(&s.0, "claude-code-sessions", ORG);
        assert_eq!(orgs_of(&ctx, ACCOUNT), vec![ORG.to_string()]);
        let read = local_usage(&ctx, &account(None)).expect("a sample");
        assert_eq!(read.observed_at, Some(1_790_000_600));
    }

    #[test]
    fn an_account_with_two_orgs_takes_the_newest() {
        let s = scratch("two-orgs");
        history(
            &s.0,
            &[
                (1_790_000_000_000, ORG, 10.0, 20.0),
                (1_790_000_900_000, OTHER_ORG, 30.0, 40.0),
                (1_790_002_000_000, SOMEONE_ELSES, 99.0, 99.0),
            ],
        );
        used_in(&s.0, "claude-code-sessions", ORG);
        used_in(&s.0, "local-agent-mode-sessions", OTHER_ORG);
        let ctx = context(&s.0);
        assert_eq!(
            orgs_of(&ctx, ACCOUNT),
            vec![ORG.to_string(), OTHER_ORG.to_string()]
        );
        let read = local_usage(&ctx, &account(None)).expect("a sample");
        assert_eq!(
            percents(&read),
            vec![("five_hour".into(), 30.0), ("seven_day".into(), 40.0)]
        );
    }

    #[test]
    fn history_is_marked_confirmed() {
        let s = scratch("confirmed");
        history(&s.0, &[(1_790_000_000_000, ORG, 10.0, 20.0)]);
        let ctx = context(&s.0);
        let read = local_usage(&ctx, &account(Some(ORG))).expect("a sample");
        assert!(
            read.verified,
            "E10 matched the history against claude.ai's own answer"
        );
        assert_eq!(
            read.verified,
            crate::assumptions::verified(ProviderId::Desktop, HISTORY_MEANING)
        );
        let json = serde_json::to_value(&read).unwrap();
        assert!(
            json.get("verified").is_none(),
            "a measured reading leaves the field out: {json}"
        );
    }

    #[test]
    fn another_version_of_the_file_is_not_read() {
        let s = scratch("version");
        std::fs::write(
            s.0.join("Claude/plan-usage-history.json"),
            serde_json::json!({"version": 3, "samples": [
                {"t": 1_790_000_000_000_i64, "org": ORG, "u": {"fh": 1, "sd": 2}}
            ]})
            .to_string(),
        )
        .unwrap();
        assert_eq!(local_usage(&context(&s.0), &account(Some(ORG))), None);
    }

    #[test]
    fn a_uuid_that_is_a_path_finds_nothing() {
        let s = scratch("odd-uuid");
        std::fs::create_dir_all(s.0.join("Claude/claude-code-sessions/x/y")).unwrap();
        let ctx = context(&s.0);
        assert!(orgs_of(&ctx, "..").is_empty());
        assert!(orgs_of(&ctx, "x/..").is_empty());
        assert!(orgs_of(&ctx, "").is_empty());
    }
}
