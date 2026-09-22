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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct State {
    pub schema: u32,
    pub machine: String,
    pub accounts: Vec<Account>,
    pub active: Option<String>,
    /// The credential slot `active` was recorded for. One state file serves every slot a
    /// machine uses, and CLAUDE_CONFIG_DIR changes which keychain item is the live one, so
    /// a record made in one slot says nothing about another.
    #[serde(default)]
    pub slot: Option<String>,
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
            slot: None,
            discarded: Vec::new(),
        }
    }
}

impl State {
    /// Every label enrolled, for a message that would otherwise send someone to another
    /// command to find out.
    pub fn labels(&self) -> crate::error::Enrolled {
        crate::error::Enrolled(self.accounts.iter().map(|a| a.label.clone()).collect())
    }

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
        let enrolled = self.labels();
        let account = self.get_mut(from).ok_or_else(|| Error::AccountUnknown {
            label: from.to_string(),
            enrolled,
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
    let mut document: serde_json::Value =
        serde_json::from_str(&raw).map_err(|source| Error::StateCorrupt {
            path: path.clone(),
            source,
        })?;
    migrate(&mut document, &path)?;
    let state: State = serde_json::from_value(document).map_err(|source| Error::StateCorrupt {
        path: path.clone(),
        source,
    })?;
    if state.machine != machine_id() {
        return Err(Error::StateWrongMachine { path });
    }
    let mut state = state;
    // Which account is in use is a fact about one slot. Read from another, the record says
    // nothing, and pitboard asks Anthropic who is signed in anyway.
    let slot = crate::claude::live_service(ctx);
    if state.slot.is_some() && state.slot.as_deref() != Some(slot.as_str()) {
        state.active = None;
    }
    Ok(state)
}

/// Brings an older file up to the current format in place.
///
/// The command line and the app carry their own copy of this crate and update by different
/// routes, so on one machine an older pitboard will meet a file a newer one wrote. Reading
/// forwards is what this is for; reading backwards is not possible, and says so.
fn migrate(document: &mut serde_json::Value, path: &std::path::Path) -> Result<()> {
    // Each future bump adds an arm that rewrites the document and falls through to the
    // next, so a file two versions behind is brought all the way forward in one read.
    let found = document
        .get("schema")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default() as u32;
    match found {
        SCHEMA => Ok(()),
        // Nothing released wrote 1 or 2: the schema reached 3 before the first release.
        0..SCHEMA => Err(Error::StateVersionUnknown {
            path: path.to_path_buf(),
            found,
        }),
        _ => Err(Error::StateFromNewerVersion {
            path: path.to_path_buf(),
            found,
            expected: SCHEMA,
        }),
    }
}

pub(crate) fn save(ctx: &Context, state: &State) -> Result<()> {
    home::check_location(&home::dir(ctx))?;
    let mut state = state.clone();
    state.slot = Some(crate::claude::live_service(ctx));
    let state = &state;
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
    /// CLAUDE_CONFIG_DIR picks which keychain item is the live one, and one state file
    /// serves every slot on a machine. A record of what was switched to in one slot says
    /// nothing about another, so it is not carried over.
    #[test]
    fn what_was_active_in_another_slot_is_not_claimed_here() {
        let home = std::env::temp_dir().join(format!("pitboard-slots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let here = Context::new(home.clone()).with_pitboard_home(home.clone());
        let elsewhere = here
            .clone()
            .with_claude_config_dir("/somewhere/else".into());

        let mut state = State::default();
        state.accounts.push(Account {
            label: "work".into(),
            account_uuid: "acc".into(),
            email: "a@b.c".into(),
            organization_uuid: "org".into(),
            oauth_account: serde_json::json!({}),
            parked: None,
        });
        state.active = Some("work".into());
        save(&here, &state).expect("saved");

        assert_eq!(load(&here).unwrap().active.as_deref(), Some("work"));
        assert_eq!(
            load(&elsewhere).unwrap().active,
            None,
            "another slot's record of what is in use is not this slot's"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A file written by the version in people's hands today. The command line and the app
    /// update separately, so a file one of them wrote has to keep loading in the other.
    #[test]
    fn the_format_shipped_in_0_1_x_still_loads() {
        let written = serde_json::json!({
            "schema": 3,
            "machine": machine_id(),
            "accounts": [{
                "label": "work",
                "account_uuid": "acc-1",
                "email": "a@b.c",
                "organization_uuid": "org-1",
                "oauth_account": {"emailAddress": "a@b.c"},
                "parked": {
                    "service": "pitboard-park-acc-1-1789935600123",
                    "parked_at": 1_789_935_600,
                    "refresh_fingerprint": "abcd",
                    "access_expires_at": 1_789_999_999,
                    "refresh_expires_at": 1_792_000_000
                }
            }],
            "active": "work",
            "discarded": []
        });
        let mut document = written.clone();
        migrate(&mut document, std::path::Path::new("/tmp/state.json")).expect("still current");
        let state: State = serde_json::from_value(document).expect("still parses");
        assert_eq!(state.get("work").unwrap().email, "a@b.c");
        assert_eq!(state.active.as_deref(), Some("work"));
    }

    /// The other direction cannot work, and the message has to say which half to upgrade.
    #[test]
    fn a_file_from_a_newer_pitboard_says_so() {
        let mut document = serde_json::json!({"schema": SCHEMA + 1});
        let err = migrate(&mut document, std::path::Path::new("/tmp/state.json")).unwrap_err();
        assert_eq!(err.code(), "state_from_newer_version");
        assert!(err.to_string().contains("upgrade whichever"), "{err}");
    }

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
