//! Which accounts pitboard knows and where each one is parked. No secrets: the logins stay
//! in the keychain or vault.
//!
//! Stamped with the machine that wrote it, because a parked login belongs to exactly one
//! machine: presenting a refresh token another machine has since rotated ends the login on
//! both.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::{atomic, home};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

const SCHEMA: u32 = 3;

/// A login held for an account while another is signed in. There is at most one per
/// account: once installed it is Claude Code's again, and Claude Code rotates it from then
/// on, so a copy kept back would only ever present a token it has moved past.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Park {
    pub service: String,
    pub parked_at: i64,
    pub refresh_fingerprint: String,
    /// Until then its usage can be asked without renewing it first.
    pub access_expires_at: Option<i64>,
    /// Until then it can be restored.
    pub refresh_expires_at: Option<i64>,
}

impl Park {
    pub fn restorable_at(&self, now: i64) -> bool {
        self.refresh_expires_at.is_none_or(|at| at > now)
    }

    pub fn askable_at(&self, now: i64) -> bool {
        self.access_expires_at.is_none_or(|at| at > now)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Account {
    pub label: String,
    pub account_uuid: String,
    pub email: String,
    pub organization_uuid: String,
    /// Written into Claude Code's config on switching here. Only what Anthropic confirmed,
    /// so Claude Code fetches the rest of its profile itself.
    pub oauth_account: Value,
    pub parked: Option<Park>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct State {
    pub schema: u32,
    pub machine: String,
    pub accounts: Vec<Account>,
    pub active: Option<String>,
    /// Parked items no account refers to any more. Listed in the same save that drops them
    /// and removed once deleted, so a delete that fails or is interrupted is retried.
    #[serde(default)]
    pub discarded: Vec<String>,
}

impl Default for State {
    fn default() -> Self {
        State {
            schema: SCHEMA,
            machine: machine_id(),
            accounts: Vec::new(),
            active: None,
            discarded: Vec::new(),
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

    fn get_mut(&mut self, label: &str) -> Option<&mut Account> {
        self.accounts.iter_mut().find(|a| a.label == label)
    }

    /// Hold `park` for the account, discarding whatever it replaces.
    pub fn park(&mut self, label: &str, park: Park) {
        let service = park.service.clone();
        if let Some(previous) = self
            .get_mut(label)
            .and_then(|account| account.parked.replace(park))
            && previous.service != service
        {
            self.discard(&previous.service);
        }
    }

    /// Stop holding `service` and list it for deletion: it has been installed, or it copies a
    /// login that is still signed in.
    pub fn discard(&mut self, service: &str) {
        for account in &mut self.accounts {
            account.parked.take_if(|p| p.service == service);
        }
        if !self.discarded.iter().any(|listed| listed == service) {
            self.discarded.push(service.to_string());
        }
    }

    pub fn references(&self, service: &str) -> bool {
        self.accounts
            .iter()
            .any(|a| a.parked.as_ref().is_some_and(|p| p.service == service))
    }

    pub fn upsert(&mut self, account: Account) {
        match self.get_mut(&account.label) {
            Some(existing) => *existing = account,
            None => self.accounts.push(account),
        }
    }

    /// Enroll the account under `from` as `to` instead. Only the label changes: parked
    /// logins are named by account, not by label.
    pub fn relabel(&mut self, from: &str, to: &str) -> Result<&Account> {
        if from != to
            && let Some(taken) = self.get(to)
        {
            return Err(Error::LabelTaken {
                label: to.to_string(),
                email: taken.email.clone(),
            });
        }
        if self.active.as_deref() == Some(from) {
            self.active = Some(to.to_string());
        }
        let account = self.get_mut(from).ok_or_else(|| Error::AccountUnknown {
            label: from.to_string(),
        })?;
        account.label = to.to_string();
        Ok(account)
    }

    /// Drop the account, listing its park for deletion.
    pub fn remove(&mut self, label: &str) -> Option<Account> {
        let index = self.accounts.iter().position(|a| a.label == label)?;
        let account = self.accounts.remove(index);
        if let Some(park) = &account.parked {
            self.discard(&park.service);
        }
        Some(account)
    }
}

/// Hashed, so the raw platform identifier never lands in a file pitboard writes.
pub fn machine_id() -> String {
    use sha2::{Digest, Sha256};
    match machine_uid::get() {
        Ok(raw) => hex::encode(Sha256::digest(raw.as_bytes())),
        Err(_) => String::from("unknown"),
    }
}

fn file(ctx: &Context) -> PathBuf {
    home::dir(ctx).join("state.json")
}

pub fn load(ctx: &Context) -> Result<State> {
    let path = file(ctx);
    home::check_location(&home::dir(ctx))?;
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
        Err(source) => return Err(Error::StateUnreadable { path, source }),
    };
    let state: State = serde_json::from_str(&raw).map_err(|source| Error::StateCorrupt {
        path: path.clone(),
        source,
    })?;
    if state.schema != SCHEMA {
        return Err(Error::StateVersionMismatch {
            path,
            found: state.schema,
            expected: SCHEMA,
        });
    }
    if state.machine != machine_id() {
        return Err(Error::StateWrongMachine { path });
    }
    Ok(state)
}

pub fn save(ctx: &Context, state: &State) -> Result<()> {
    home::check_location(&home::dir(ctx))?;
    let path = file(ctx);
    let write = |source| Error::StateWriteFailed {
        path: path.clone(),
        source,
    };
    home::ensure(ctx).map_err(write)?;
    let body = serde_json::to_string_pretty(state).expect("State is always serialisable");
    atomic::write(&path, body.as_bytes(), atomic::Perms::Secret).map_err(write)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_id_is_stable_and_real() {
        let a = machine_id();
        assert_eq!(a, machine_id());
        assert_eq!(
            a.len(),
            64,
            "expected a sha256 of the platform id, got {a:?}"
        );
        assert_ne!(
            a, "unknown",
            "this platform should report a stable machine id"
        );
    }

    fn park(service: &str) -> Park {
        Park {
            service: service.into(),
            parked_at: 100,
            refresh_fingerprint: "f".into(),
            access_expires_at: Some(200),
            refresh_expires_at: Some(300),
        }
    }

    fn account(label: &str, parked: Option<Park>) -> Account {
        Account {
            label: label.into(),
            account_uuid: format!("{label}-uuid"),
            email: format!("{label}@example.com"),
            organization_uuid: "o".into(),
            oauth_account: serde_json::json!({}),
            parked,
        }
    }

    #[test]
    fn a_new_park_discards_the_one_it_replaces() {
        let mut s = State::default();
        s.upsert(account("work", Some(park("old"))));
        s.park("work", park("new"));
        assert_eq!(
            s.get("work").unwrap().parked.as_ref().unwrap().service,
            "new"
        );
        assert_eq!(s.discarded, ["old"]);
        assert!(!s.references("old"));
    }

    #[test]
    fn discarding_releases_whichever_account_held_it_and_lists_it_once() {
        let mut s = State::default();
        s.upsert(account("work", Some(park("current"))));
        s.discard("something-else");
        assert!(s.get("work").unwrap().parked.is_some());
        s.discard("current");
        s.discard("current");
        assert!(s.get("work").unwrap().parked.is_none());
        assert_eq!(s.discarded, ["something-else", "current"]);
    }

    #[test]
    fn relabelling_keeps_the_account_its_park_and_whether_it_is_active() {
        let mut s = State::default();
        s.upsert(account("wrong", Some(park("p"))));
        s.upsert(account("other", None));
        s.active = Some("wrong".into());

        assert_eq!(
            s.relabel("wrong", "right").unwrap().email,
            "wrong@example.com"
        );
        assert!(s.get("wrong").is_none());
        let renamed = s.get("right").unwrap();
        assert_eq!(renamed.account_uuid, "wrong-uuid");
        assert_eq!(renamed.parked.as_ref().unwrap().service, "p");
        assert_eq!(s.active.as_deref(), Some("right"));
        assert!(s.discarded.is_empty(), "nothing is deleted by a rename");
    }

    #[test]
    fn relabelling_refuses_a_taken_label_and_an_unknown_one() {
        let mut s = State::default();
        s.upsert(account("a", None));
        s.upsert(account("b", None));
        s.active = Some("a".into());
        assert!(matches!(s.relabel("a", "b"), Err(Error::LabelTaken { .. })));
        assert!(matches!(
            s.relabel("nobody", "c"),
            Err(Error::AccountUnknown { .. })
        ));
        assert_eq!(
            s.active.as_deref(),
            Some("a"),
            "a refused rename changes nothing"
        );
        assert!(s.relabel("a", "a").is_ok());
    }

    #[test]
    fn removing_an_account_lists_its_park_for_deletion() {
        let mut s = State::default();
        s.upsert(account("work", Some(park("p"))));
        assert_eq!(s.remove("work").unwrap().label, "work");
        assert!(s.accounts.is_empty());
        assert_eq!(s.discarded, ["p"]);
    }

    #[test]
    fn a_park_is_restorable_until_its_login_expires() {
        let p = park("p");
        assert!(p.askable_at(199) && !p.askable_at(200));
        assert!(p.restorable_at(299) && !p.restorable_at(300));
        let unknown = Park {
            access_expires_at: None,
            refresh_expires_at: None,
            ..park("p")
        };
        assert!(
            unknown.restorable_at(i64::MAX),
            "no expiry recorded is not expired"
        );
    }

    #[test]
    fn accounts_are_replaced_by_label_not_duplicated() {
        let mut s = State::default();
        s.upsert(account("work", None));
        s.upsert(Account {
            email: "d@e.f".into(),
            ..account("work", None)
        });
        assert_eq!(s.accounts.len(), 1);
        assert_eq!(s.get("work").unwrap().email, "d@e.f");
    }
}
