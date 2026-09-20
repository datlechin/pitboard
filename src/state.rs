//! pitboard's own on-disk state: which accounts are known and where each one is parked.
//!
//! Secrets are never here. This file holds an index and the identity Claude Code itself
//! recorded; the credentials stay in the keychain. It is stamped with the machine that
//! wrote it because a parked credential belongs to exactly one machine: presenting a
//! refresh token that another machine has since rotated destroys the login for both.

use crate::{hex, time};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

const SCHEMA: u32 = 2;
const CLOUD_MARKERS: [&str; 5] = [
    "Dropbox",
    "Google Drive",
    "OneDrive",
    "com~apple~CloudDocs",
    "Sync",
];

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Generation {
    pub service: String,
    pub parked_at: i64,
    pub refresh_fingerprint: String,
    /// When these bytes were last installed into the live slot.
    ///
    /// Claude Code rotates the refresh token in place from that moment on, so the parked
    /// copy is superseded. Presenting a superseded token returns invalid_grant, and Claude
    /// Code answers that by zeroing the live credential. A consumed generation is therefore
    /// never restored again.
    #[serde(default)]
    pub installed_at: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Account {
    pub label: String,
    pub account_uuid: String,
    pub email: String,
    pub organization_uuid: String,
    /// Claude Code's own `oauthAccount` object, minus `profileFetchedAt` so that
    /// restoring it makes Claude Code refetch the profile rather than trust our copy.
    pub oauth_account: Value,
    pub generations: Vec<Generation>,
}

impl Account {
    pub fn newest(&self) -> Option<&Generation> {
        self.generations.iter().max_by_key(|g| g.parked_at)
    }

    /// The newest generation that has not already been installed and superseded.
    pub fn restorable(&self) -> Option<&Generation> {
        self.generations
            .iter()
            .filter(|g| g.installed_at.is_none())
            .max_by_key(|g| g.parked_at)
    }

    pub fn references(&self, service: &str) -> bool {
        self.generations.iter().any(|g| g.service == service)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct State {
    pub schema: u32,
    pub machine: String,
    pub accounts: Vec<Account>,
    pub active: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        State {
            schema: SCHEMA,
            machine: machine_id(),
            accounts: Vec::new(),
            active: None,
        }
    }
}

impl State {
    pub fn get(&self, label: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.label == label)
    }

    pub fn by_uuid(&self, uuid: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.account_uuid == uuid)
    }

    pub fn attach(&mut self, label: &str, generation: Generation) {
        if let Some(account) = self.accounts.iter_mut().find(|a| a.label == label) {
            account.generations.push(generation);
        }
    }

    pub fn mark_installed(&mut self, label: &str, service: &str, at: i64) {
        if let Some(account) = self.accounts.iter_mut().find(|a| a.label == label) {
            if let Some(g) = account
                .generations
                .iter_mut()
                .find(|g| g.service == service)
            {
                g.installed_at = Some(at);
            }
        }
    }

    pub fn references(&self, service: &str) -> bool {
        self.accounts.iter().any(|a| a.references(service))
    }

    pub fn upsert(&mut self, account: Account) {
        match self.accounts.iter_mut().find(|a| a.label == account.label) {
            Some(existing) => *existing = account,
            None => self.accounts.push(account),
        }
    }
}

/// A stable identifier for this machine, from the platform rather than anything we invent.
pub fn machine_id() -> String {
    let mut uuid = [0u8; 16];
    let wait = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if unsafe { libc::gethostuuid(uuid.as_mut_ptr(), &wait) } == 0 {
        hex::encode(&uuid)
    } else {
        String::from("unknown")
    }
}

pub fn dir() -> PathBuf {
    std::env::var_os("PITBOARD_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".pitboard")
        })
}

fn file() -> PathBuf {
    dir().join("state.json")
}

/// Refuse a state directory that a sync client would copy to another machine.
fn check_location(path: &Path) -> Result<(), String> {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.to_string_lossy();
    if let Some(m) = CLOUD_MARKERS.iter().find(|m| text.contains(**m)) {
        return Err(format!(
            "{} looks like it is inside {m}. Parked credentials belong to one machine; \
             set PITBOARD_HOME to a local path.",
            resolved.display()
        ));
    }
    Ok(())
}

pub fn load() -> Result<State, String> {
    let path = file();
    check_location(&dir())?;
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let state: State =
        serde_json::from_str(&raw).map_err(|e| format!("{} is corrupt: {e}", path.display()))?;
    if state.schema != SCHEMA {
        return Err(format!(
            "{} was written by a different version of pitboard (schema {}, expected {SCHEMA})",
            path.display(),
            state.schema
        ));
    }
    let here = machine_id();
    if state.machine != here {
        return Err(format!(
            "{} was written on another machine. Parked credentials cannot be moved between \
             machines: sign in again on this one instead.",
            path.display()
        ));
    }
    Ok(state)
}

pub fn save(state: &State) -> Result<(), String> {
    let d = dir();
    check_location(&d)?;
    std::fs::create_dir_all(&d).map_err(|e| format!("cannot create {}: {e}", d.display()))?;
    let tmp = d.join(format!("state.{}.tmp", std::process::id()));
    let body = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, file()).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

/// Generations worth keeping: the five newest, plus anything from the last 45 days.
pub fn retained(generations: &[Generation]) -> Vec<&Generation> {
    let cutoff = time::now() - 45 * 86_400;
    let mut sorted: Vec<&Generation> = generations.iter().collect();
    sorted.sort_by_key(|g| std::cmp::Reverse(g.parked_at));
    sorted
        .iter()
        .enumerate()
        .filter(|(i, g)| *i < 5 || g.parked_at >= cutoff)
        .map(|(_, g)| *g)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generation(at: i64) -> Generation {
        Generation {
            service: format!("pitboard-park-x-{at}"),
            parked_at: at,
            refresh_fingerprint: "f".into(),
            installed_at: None,
        }
    }

    #[test]
    fn machine_id_is_stable_and_real() {
        let a = machine_id();
        assert_eq!(a, machine_id());
        assert_eq!(a.len(), 32, "expected a 16-byte host uuid, got {a:?}");
    }

    #[test]
    fn retention_keeps_five_however_old_they_are() {
        let ancient: Vec<Generation> = (0..8).map(|i| generation(1_000 + i)).collect();
        let kept = retained(&ancient);
        assert_eq!(kept.len(), 5);
        assert_eq!(kept[0].parked_at, 1_007, "newest first");
    }

    #[test]
    fn retention_also_keeps_anything_recent() {
        let now = time::now();
        let recent: Vec<Generation> = (0..9).map(|i| generation(now - i * 86_400)).collect();
        assert_eq!(retained(&recent).len(), 9);
    }

    #[test]
    fn a_cloud_synced_state_directory_is_refused() {
        let p = Path::new("/Users/x/Library/Mobile Documents/com~apple~CloudDocs/pitboard");
        assert!(check_location(p).is_err());
        assert!(check_location(Path::new("/Users/x/.pitboard")).is_ok());
    }

    #[test]
    fn a_generation_that_was_installed_is_never_offered_again() {
        let mut account = Account {
            label: "a".into(),
            account_uuid: "u".into(),
            email: "a@b.c".into(),
            organization_uuid: "o".into(),
            oauth_account: serde_json::json!({}),
            generations: vec![generation(100), generation(200)],
        };
        assert_eq!(account.restorable().unwrap().parked_at, 200);
        account.generations[1].installed_at = Some(250);
        assert_eq!(
            account.restorable().unwrap().parked_at,
            100,
            "a superseded copy must not be offered"
        );
        account.generations[0].installed_at = Some(260);
        assert!(
            account.restorable().is_none(),
            "refuse rather than restore a dead token"
        );
    }

    #[test]
    fn accounts_are_replaced_by_label_not_duplicated() {
        let mut s = State::default();
        let mk = |email: &str| Account {
            label: "work".into(),
            account_uuid: "u".into(),
            email: email.into(),
            organization_uuid: "o".into(),
            oauth_account: serde_json::json!({}),
            generations: vec![],
        };
        s.upsert(mk("a@b.c"));
        s.upsert(mk("d@e.f"));
        assert_eq!(s.accounts.len(), 1);
        assert_eq!(s.get("work").unwrap().email, "d@e.f");
    }
}
