//! Which accounts pitboard knows and where each one is parked. No secrets: the logins stay
//! in the keychain or vault.
//!
//! Stamped with the machine that wrote it, because a parked login belongs to exactly one
//! machine: presenting a refresh token another machine has since rotated ends the login on
//! both.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::provider::ProviderId;
use crate::{atomic, home};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

const SCHEMA: u32 = 4;

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

/// What one provider keeps about an account that the others have no equivalent of.
///
/// A tagged enum rather than a pile of optional fields, so no code reading a Codex account
/// ever has to decide what an absent Claude organisation means for it. `provider` is the
/// tag, and the variant's own fields sit beside `label` and `email` in the file, which is
/// why a schema 3 account needs nothing moved to become a schema 4 one.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "provider", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Detail {
    Claude {
        organization_uuid: String,
        /// Written into Claude Code's config on switching here. Only what Anthropic
        /// confirmed, so Claude Code fetches the rest of its profile itself.
        oauth_account: Value,
    },
    Codex {
        /// The ChatGPT workspace this account belongs to, where it belongs to one.
        #[serde(default)]
        workspace_id: Option<String>,
        /// `plus`, `pro`, `team` and so on, read out of the login's own ID token. Kept
        /// because it is free to know and explains a limit somebody is surprised by.
        #[serde(default)]
        plan: Option<String>,
    },
}

/// Claude Code's own extras, for a caller that has already established it is holding a
/// Claude account.
pub struct ClaudeDetail<'a> {
    pub organization_uuid: &'a str,
    pub oauth_account: &'a Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Account {
    pub label: String,
    pub account_uuid: String,
    pub email: String,
    pub parked: Option<Park>,
    /// When this account was last switched to, in epoch seconds.
    ///
    /// pitboard renews a parked login for as long as the account is enrolled, so an account
    /// somebody enrolled once and never came back to keeps a live, continuously rotated
    /// refresh token on the machine indefinitely. Nothing said so, and nothing asked.
    /// Recording this is what lets `doctor` say it.
    ///
    /// `None` on an account enrolled before this was recorded, and on one that has never
    /// been switched to.
    #[serde(default)]
    pub last_used_at: Option<i64>,
    /// Which tool's login this is, and whatever only that tool keeps.
    #[serde(flatten)]
    pub detail: Detail,
}

/// One account, the way pitboard tells accounts apart: which tool, and what it is called
/// there.
///
/// A label alone stopped being enough the day a second tool could have a `work` of its own.
/// Everything that finds, changes or drops an account takes one of these, so no lookup can
/// quietly land on the other tool's account of the same name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key {
    pub provider: ProviderId,
    pub label: String,
}

impl Key {
    pub fn new(provider: ProviderId, label: impl Into<String>) -> Key {
        Key {
            provider,
            label: label.into(),
        }
    }

    /// The name as somebody would type it back: bare for the tool a bare name means,
    /// qualified for any other. Every message and the audit log use this, so a Claude Code
    /// account reads exactly as it did before there was a second tool.
    pub fn typed(&self) -> String {
        if self.provider == crate::label::DEFAULT {
            self.label.clone()
        } else {
            self.qualified()
        }
    }

    /// `codex/work`, whichever tool it is.
    pub fn qualified(&self) -> String {
        format!(
            "{}{}{}",
            self.provider.code(),
            crate::label::SEPARATOR,
            self.label
        )
    }
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.typed())
    }
}

impl Account {
    pub fn key(&self) -> Key {
        Key::new(self.provider(), self.label.clone())
    }

    pub fn is(&self, key: &Key) -> bool {
        self.label == key.label && self.provider() == key.provider
    }

    pub fn provider(&self) -> ProviderId {
        match self.detail {
            Detail::Claude { .. } => ProviderId::Claude,
            Detail::Codex { .. } => ProviderId::Codex,
        }
    }

    /// Claude Code's extras, or `None` when this account belongs to another tool.
    pub fn claude(&self) -> Option<ClaudeDetail<'_>> {
        match &self.detail {
            Detail::Claude {
                organization_uuid,
                oauth_account,
            } => Some(ClaudeDetail {
                organization_uuid,
                oauth_account,
            }),
            Detail::Codex { .. } => None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct State {
    pub schema: u32,
    pub machine: String,
    pub accounts: Vec<Account>,
    /// Which account is signed in, per provider.
    ///
    /// One string until schema 4, which stopped being true the moment a machine could have
    /// a Claude Code login and a Codex login at the same time. They are different programs
    /// reading different stores; neither signs the other out.
    #[serde(default)]
    pub active: BTreeMap<String, String>,
    /// The credential slot each provider's `active` was recorded for. One state file serves
    /// every slot a machine uses, and a tool's own home variable changes which store is the
    /// live one, so a record made in one slot says nothing about another.
    #[serde(default)]
    pub slot: BTreeMap<String, String>,
    /// Parked items no account refers to any more. Listed in the same save that drops them
    /// and removed once deleted, so a delete that fails or is interrupted is retried.
    #[serde(default)]
    pub discarded: Vec<String>,
    /// Parked items an account here holds that this home never wrote down: logins
    /// `pitboard repair` found in the store and gave back. On macOS every `PITBOARD_HOME`
    /// shares the login keychain, so each may be another pitboard's parked login, and
    /// letting one go unused must leave it where it is. Removed once nothing here holds it.
    /// Always empty where the vault is a directory inside this home, which nobody else
    /// parks in.
    #[serde(default)]
    pub foreign: Vec<String>,
}

impl Default for State {
    fn default() -> Self {
        State {
            schema: SCHEMA,
            machine: machine_id(),
            accounts: Vec::new(),
            active: BTreeMap::new(),
            slot: BTreeMap::new(),
            discarded: Vec::new(),
            foreign: Vec::new(),
        }
    }
}

impl State {
    /// Which account is signed in for this provider, as pitboard last recorded it.
    pub fn active_for(&self, provider: ProviderId) -> Option<&str> {
        self.active.get(provider.code()).map(String::as_str)
    }

    pub fn set_active(&mut self, provider: ProviderId, label: Option<String>) {
        match label {
            Some(label) => self.active.insert(provider.code().to_string(), label),
            None => self.active.remove(provider.code()),
        };
    }

    /// The credential slot this provider's `active` was recorded for.
    pub fn slot_for(&self, provider: ProviderId) -> Option<&str> {
        self.slot.get(provider.code()).map(String::as_str)
    }

    pub fn set_slot(&mut self, provider: ProviderId, slot: String) {
        self.slot.insert(provider.code().to_string(), slot);
    }

    /// The account's name as a command would take it here: bare for the tool a bare name
    /// means, unless another tool has an account of the same name, in which case a bare
    /// name would be refused as ambiguous and the tool is said.
    ///
    /// What every message that tells somebody what to type uses, so the command it names
    /// is one that works on this machine.
    pub fn typed(&self, key: &Key) -> String {
        let shared = self
            .accounts
            .iter()
            .any(|a| a.label == key.label && a.provider() != key.provider);
        if shared { key.qualified() } else { key.typed() }
    }

    /// Every label this tool has enrolled, for a message that would otherwise send someone
    /// to another command to find out.
    pub fn labels(&self, provider: ProviderId) -> crate::error::Enrolled {
        crate::error::Enrolled(
            self.accounts
                .iter()
                .filter(|a| a.provider() == provider)
                .map(|a| a.label.clone())
                .collect(),
        )
    }

    /// Whether anything in the state refers to this vault item: an account holding it, or
    /// the list of ones waiting to be deleted.
    pub fn names(&self, service: &str) -> bool {
        self.accounts
            .iter()
            .filter_map(|a| a.parked.as_ref())
            .any(|p| p.service == service)
            || self.discarded.iter().any(|s| s == service)
    }

    /// The account under this key.
    ///
    /// Callers that took a name from a person should go through [`crate::label::resolve`]
    /// first, which knows what to do when two tools share a label.
    pub fn get(&self, key: &Key) -> Option<&Account> {
        self.accounts.iter().find(|a| a.is(key))
    }

    /// This tool's account with this identity.
    pub fn by_uuid(&self, provider: ProviderId, uuid: &str) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|a| a.provider() == provider && a.account_uuid == uuid)
    }

    /// The account a parked item was written for.
    ///
    /// A park's name carries the account's identity and not its tool, because names were
    /// fixed before there was a second tool and every item already on a machine is filed
    /// under one. The identities cannot collide in practice: Claude Code's and Codex's are
    /// UUIDs, and Gemini's is Google's numeric subject.
    pub fn owner_of_park(&self, uuid: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.account_uuid == uuid)
    }

    fn get_mut(&mut self, key: &Key) -> Option<&mut Account> {
        self.accounts.iter_mut().find(|a| a.is(key))
    }

    /// Hold `park` for the account, releasing whatever it replaces. A newer park does not
    /// use the one before it, so that one is let go rather than consumed.
    pub fn park(&mut self, key: &Key, park: Park) {
        let service = park.service.clone();
        if let Some(previous) = self
            .get_mut(key)
            .and_then(|account| account.parked.replace(park))
            && previous.service != service
        {
            self.release(&previous.service);
        }
    }

    /// Hold a park this home did not write, found in the store and given back. It is used
    /// like any other, and deleted only once it has been.
    pub fn park_foreign(&mut self, key: &Key, park: Park) {
        let service = park.service.clone();
        self.park(key, park);
        if self.references(&service) && !self.is_foreign(&service) {
            self.foreign.push(service);
        }
    }

    /// Whether an account here holds `service` without this home having written it.
    pub fn is_foreign(&self, service: &str) -> bool {
        self.foreign.iter().any(|listed| listed == service)
    }

    /// Take `service` as this home's own from now on, because this home has used it: a
    /// renewal presented its refresh token. Letting it go afterwards deletes it.
    pub fn used_here(&mut self, service: &str) {
        self.foreign.retain(|listed| listed != service);
    }

    /// Stop holding `service` because it has been used up, and list it for deletion
    /// whoever wrote it: it has been installed, a renewal has spent it, or it copies a login
    /// that is still signed in. Nothing can use it again, and for a tool whose park may
    /// never be a copy it must not stay beside the login it copies.
    pub fn discard(&mut self, service: &str) {
        self.let_go(service);
        if !self.discarded.iter().any(|listed| listed == service) {
            self.discarded.push(service.to_string());
        }
    }

    /// Stop holding `service` because nothing here wants it any more: a newer park replaced
    /// it, or its account was dropped. Listed for deletion only when this home wrote it. One
    /// `repair` gave back may be another pitboard's parked login, which this one never
    /// used, and deleting it would end that account's session for somebody who never ran
    /// the command that did it.
    pub fn release(&mut self, service: &str) {
        if self.is_foreign(service) {
            self.let_go(service);
        } else {
            self.discard(service);
        }
    }

    /// No account holds `service` afterwards, and nothing records who wrote it.
    fn let_go(&mut self, service: &str) {
        for account in &mut self.accounts {
            account.parked.take_if(|p| p.service == service);
        }
        self.foreign.retain(|listed| listed != service);
    }

    pub fn references(&self, service: &str) -> bool {
        self.accounts
            .iter()
            .any(|a| a.parked.as_ref().is_some_and(|p| p.service == service))
    }

    /// Record that the account under `key` was just put to use.
    pub fn used(&mut self, key: &Key, at: i64) {
        if let Some(account) = self.get_mut(key) {
            account.last_used_at = Some(at);
        }
    }

    /// Add the account, or replace the one this tool already has under its label.
    pub fn upsert(&mut self, account: Account) {
        match self.get_mut(&account.key()) {
            Some(existing) => *existing = account,
            None => self.accounts.push(account),
        }
    }

    /// Enroll the account under `from` as `to` instead, inside the same tool. Only the
    /// label changes: parked logins are named by account, not by label.
    pub fn relabel(&mut self, from: &Key, to: &str) -> Result<&Account> {
        let target = Key::new(from.provider, to);
        if from.label != to
            && let Some(taken) = self.get(&target)
        {
            return Err(Error::LabelTaken {
                label: target.typed(),
                email: taken.email.clone(),
            });
        }
        if self.active_for(from.provider) == Some(from.label.as_str()) {
            self.set_active(from.provider, Some(to.to_string()));
        }
        let enrolled = self.labels(from.provider);
        let account = self.get_mut(from).ok_or_else(|| Error::AccountUnknown {
            label: from.typed(),
            enrolled,
        })?;
        account.label = to.to_string();
        Ok(account)
    }

    /// Drop the account, releasing its park.
    pub fn remove(&mut self, key: &Key) -> Option<Account> {
        let index = self.accounts.iter().position(|a| a.is(key))?;
        let account = self.accounts.remove(index);
        if let Some(park) = &account.parked {
            self.release(&park.service);
        }
        if self.active_for(key.provider) == Some(key.label.as_str()) {
            self.set_active(key.provider, None);
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

/// When pitboard's account index last changed, in epoch seconds, or 0 when there is none.
///
/// Three front ends run on one machine and none of them could tell when another had
/// changed anything. A switch typed in a terminal left the menu bar naming the account the
/// person had just stopped using, for as long as five minutes, with a button offering a
/// switch that had already happened.
///
/// This is the cheapest true answer there is: one stat of one file. It is deliberately the
/// account index alone and not the whole directory. The status line writes usage readings
/// after a message in any open session, and those say nothing about who is signed in: a
/// front end follows them with `readings::changed_at`, and takes only the numbers.
pub fn changed_at(ctx: &Context) -> i64 {
    std::fs::metadata(file(ctx))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

pub fn load(ctx: &Context) -> Result<State> {
    let (state, here) = load_any_machine(ctx)?;
    if !here {
        return Err(Error::StateWrongMachine { path: file(ctx) });
    }
    Ok(state)
}

/// The state whatever machine wrote it, and whether that machine is this one.
///
/// Only `adopt` reads it this way. Everything else goes through [`load`], which refuses a
/// file from elsewhere: a parked login is a refresh token, and two machines taking turns
/// presenting one ends the login for both.
pub(crate) fn load_any_machine(ctx: &Context) -> Result<(State, bool)> {
    let path = file(ctx);
    home::check_location(&home::dir(ctx))?;
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((State::default(), true)),
        Err(source) => return Err(Error::StateUnreadable { path, source }),
    };
    let mut document: serde_json::Value =
        serde_json::from_str(&raw).map_err(|source| Error::StateCorrupt {
            path: path.clone(),
            source,
        })?;
    migrate(&mut document, &path)?;
    // An account of a tool this build does not know was written by a newer pitboard, not
    // damaged. Said as such, because the advice for a corrupt file is to delete it, and
    // following that here would orphan every parked login in the vault.
    if let Some(unknown) = unknown_tool(&document) {
        return Err(Error::StateNamesUnknownTool {
            path: path.clone(),
            tool: unknown,
        });
    }
    let state: State = serde_json::from_value(document).map_err(|source| Error::StateCorrupt {
        path: path.clone(),
        source,
    })?;
    let here = state.machine == machine_id();
    let mut state = state;
    // Which account is in use is a fact about one slot. Read from another, the record says
    // nothing, and pitboard asks the tool who is signed in anyway. Per tool, so a changed
    // `CLAUDE_CONFIG_DIR` says nothing about Codex's record, nor `CODEX_HOME` about Claude
    // Code's.
    for &tool in ProviderId::ALL {
        let slot = crate::provider::of(tool).slot(ctx);
        if state
            .slot_for(tool)
            .is_some_and(|recorded| recorded != slot)
        {
            state.set_active(tool, None);
        }
    }
    Ok((state, here))
}

/// The first tool an account names that this build does not know, if any.
fn unknown_tool(document: &Value) -> Option<String> {
    document
        .get("accounts")?
        .as_array()?
        .iter()
        .filter_map(|account| account.get("provider")?.as_str())
        .find(|code| ProviderId::parse(code).is_none())
        .map(str::to_owned)
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
        3 => {
            three_to_four(document);
            Ok(())
        }
        // Nothing released wrote 1 or 2: the schema reached 3 before the first release.
        0..3 => Err(Error::StateVersionUnknown {
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

/// Schema 3 was Claude Code and nothing else, so every account in one is a Claude account
/// and the two singular records are Claude's.
///
/// Deliberately the smallest transform there could be. Nothing is nested and nothing is
/// renamed, because `Detail` is flattened and tagged: a schema 3 account already has
/// `organization_uuid` and `oauth_account` as siblings of `label`, which is exactly where
/// schema 4 reads them. All that is missing is the tag. No keychain item and no vault file
/// is touched, so a bug here is recoverable by fixing the code and reading again, never by
/// somebody signing in from scratch.
fn three_to_four(document: &mut serde_json::Value) {
    let claude = serde_json::Value::from(ProviderId::Claude.code());
    if let Some(accounts) = document.get_mut("accounts").and_then(Value::as_array_mut) {
        for account in accounts {
            if let Some(fields) = account.as_object_mut() {
                fields.insert("provider".into(), claude.clone());
            }
        }
    }
    for singular in ["active", "slot"] {
        let was = document.get(singular).cloned().unwrap_or(Value::Null);
        document[singular] = match was {
            Value::String(label) => serde_json::json!({ ProviderId::Claude.code(): label }),
            _ => serde_json::json!({}),
        };
    }
    document["schema"] = serde_json::json!(SCHEMA);
}

pub(crate) fn save(ctx: &Context, state: &State) -> Result<()> {
    home::check_location(&home::dir(ctx))?;
    let mut state = state.clone();
    for &tool in ProviderId::ALL {
        state.set_slot(tool, crate::provider::of(tool).slot(ctx));
    }
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
            last_used_at: None,
            label: "work".into(),
            account_uuid: "acc".into(),
            email: "a@b.c".into(),
            detail: Detail::Claude {
                organization_uuid: "org".into(),
                oauth_account: serde_json::json!({}),
            },
            parked: None,
        });
        state.set_active(ProviderId::Claude, Some("work".into()));
        save(&here, &state).expect("saved");

        assert_eq!(
            load(&here).unwrap().active_for(ProviderId::Claude),
            Some("work")
        );
        assert_eq!(
            load(&elsewhere).unwrap().active_for(ProviderId::Claude),
            None,
            "another slot's record of what is in use is not this slot's"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// `CODEX_HOME` picks which `auth.json` is Codex's live login, the way
    /// `CLAUDE_CONFIG_DIR` picks Claude Code's keychain item. A record made under one home
    /// says nothing about another, and says nothing about Claude Code's at all.
    #[test]
    fn another_codex_home_is_another_codex_slot() {
        let home = std::env::temp_dir().join(format!(
            "pitboard-codex-slots-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        let here = Context::new(home.clone()).with_pitboard_home(home.clone());
        let mut state = State::default();
        state.set_active(ProviderId::Claude, Some("work".into()));
        state.set_active(ProviderId::Codex, Some("work".into()));
        save(&here, &state).expect("saved");

        let moved = here.clone().with_codex_home("/somewhere/else".into());
        let loaded = load(&moved).unwrap();
        assert_eq!(loaded.active_for(ProviderId::Codex), None);
        assert_eq!(
            loaded.active_for(ProviderId::Claude),
            Some("work"),
            "Claude Code's slot did not move"
        );
        assert_eq!(
            load(&here).unwrap().active_for(ProviderId::Codex),
            Some("work")
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
        assert_eq!(state.get(&claude("work")).unwrap().email, "a@b.c");
        assert_eq!(state.active_for(ProviderId::Claude), Some("work"));
    }

    /// Migrating a file that is already current must change nothing.
    ///
    /// `three_to_four` rewrites `active` and `slot` in place, and a version check that
    /// slipped would wrap an already-wrapped map into `{"claude": {"claude": "work"}}` and
    /// lose which account is in use, silently, on every load after that.
    #[test]
    fn migrating_a_current_file_is_a_no_op() {
        let mut once = serde_json::json!({
            "schema": 3,
            "machine": machine_id(),
            "accounts": [{
                "label": "work", "account_uuid": "acc-1", "email": "a@b.c",
                "organization_uuid": "org-1", "oauth_account": {}, "parked": null
            }],
            "active": "work",
            "slot": "Claude Code-credentials",
            "discarded": []
        });
        migrate(&mut once, std::path::Path::new("/tmp/state.json")).expect("3 to 4");
        let mut twice = once.clone();
        migrate(&mut twice, std::path::Path::new("/tmp/state.json")).expect("4 is current");
        assert_eq!(once, twice, "a second migration must change nothing");
        assert_eq!(once["active"], serde_json::json!({"claude": "work"}));
        assert_eq!(
            once["slot"],
            serde_json::json!({"claude": "Claude Code-credentials"})
        );
        assert_eq!(once["accounts"][0]["provider"], "claude");
    }

    /// Schema 3 had no `active` at all when nothing had been switched to, and a migration
    /// that turned that into a one-entry map naming nothing would claim a switch happened.
    #[test]
    fn a_file_that_never_switched_migrates_to_no_active_account() {
        let mut document = serde_json::json!({
            "schema": 3, "machine": machine_id(), "accounts": [], "discarded": []
        });
        migrate(&mut document, std::path::Path::new("/tmp/state.json")).expect("3 to 4");
        let state: State = serde_json::from_value(document).expect("parses");
        assert_eq!(state.active_for(ProviderId::Claude), None);
        assert!(state.active.is_empty() && state.slot.is_empty());
    }

    /// `Detail` is flattened and tagged, which is the whole reason the migration has
    /// nothing to move. If it ever stopped sitting beside `label` in the file, every
    /// account already written would stop loading.
    #[test]
    fn a_providers_own_fields_sit_beside_the_shared_ones() {
        let account = Account {
            label: "work".into(),
            account_uuid: "acc".into(),
            email: "a@b.c".into(),
            parked: None,
            last_used_at: None,
            detail: Detail::Claude {
                organization_uuid: "org".into(),
                oauth_account: serde_json::json!({"emailAddress": "a@b.c"}),
            },
        };
        let written = serde_json::to_value(&account).expect("writes");
        assert_eq!(written["provider"], "claude");
        assert_eq!(written["organization_uuid"], "org");
        assert_eq!(written["label"], "work");
        assert!(
            written.get("detail").is_none(),
            "flattened, so there is no nested object: {written}"
        );
        let back: Account = serde_json::from_value(written).expect("reads back");
        assert_eq!(back.provider(), ProviderId::Claude);
        assert_eq!(back.claude().unwrap().organization_uuid, "org");
    }

    /// Two tools are two programs reading two stores. A `CLAUDE_CONFIG_DIR` that changed
    /// says nothing about which Codex account is signed in, and clearing both would tell
    /// somebody their other switch never happened.
    #[test]
    fn a_changed_slot_clears_only_that_providers_record() {
        let mut state = State::default();
        state.set_active(ProviderId::Claude, Some("work".into()));
        state.set_slot(ProviderId::Claude, "some-other-slot".into());
        state
            .active
            .insert("pretend-other-provider".into(), "personal".into());

        // What `load_any_machine` does when the slot it reads is not the one recorded.
        if state
            .slot_for(ProviderId::Claude)
            .is_some_and(|recorded| recorded != "Claude Code-credentials")
        {
            state.set_active(ProviderId::Claude, None);
        }

        assert_eq!(state.active_for(ProviderId::Claude), None);
        assert_eq!(
            state
                .active
                .get("pretend-other-provider")
                .map(String::as_str),
            Some("personal"),
            "another provider's record is not this provider's to clear"
        );
    }

    /// A file naming a tool this build does not know came from a newer pitboard. Called
    /// corrupt, its advice would be to delete it, which orphans every parked login.
    #[test]
    fn an_account_of_an_unknown_tool_asks_for_an_upgrade_not_a_deletion() {
        let home = std::env::temp_dir().join(format!(
            "pitboard-unknown-tool-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let ctx = Context::new(home.clone()).with_pitboard_home(home.clone());
        std::fs::write(
            home.join("state.json"),
            serde_json::json!({
                "schema": SCHEMA,
                "machine": machine_id(),
                "accounts": [{
                    "label": "work", "account_uuid": "u", "email": "a@b.c",
                    "parked": null, "provider": "somethingnew"
                }],
                "active": {}, "slot": {}, "discarded": []
            })
            .to_string(),
        )
        .unwrap();
        let err = load(&ctx).unwrap_err();
        assert_eq!(err.code(), "state_names_unknown_tool");
        assert!(err.to_string().contains("somethingnew"), "{err}");
        assert!(!err.to_string().contains("Delete"), "{err}");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The other direction cannot work, and the message has to say which half to upgrade.
    #[test]
    fn a_file_from_a_newer_pitboard_says_so() {
        let mut document = serde_json::json!({"schema": SCHEMA + 1});
        let err = migrate(&mut document, std::path::Path::new("/tmp/state.json")).unwrap_err();
        assert_eq!(err.code(), "state_from_newer_version");
        let said = err.to_string();
        assert!(said.contains("Update this pitboard"), "{said}");
        assert!(
            !said.contains("brew") && !said.contains("cargo"),
            "it names no one way of installing pitboard: {said}"
        );
    }

    use super::*;

    fn claude(label: &str) -> Key {
        Key::new(ProviderId::Claude, label)
    }

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
            last_used_at: None,
            label: label.into(),
            account_uuid: format!("{label}-uuid"),
            email: format!("{label}@example.com"),
            detail: Detail::Claude {
                organization_uuid: "o".into(),
                oauth_account: serde_json::json!({}),
            },
            parked,
        }
    }

    #[test]
    fn a_new_park_discards_the_one_it_replaces() {
        let mut s = State::default();
        s.upsert(account("work", Some(park("old"))));
        s.park(&claude("work"), park("new"));
        assert_eq!(
            s.get(&claude("work"))
                .unwrap()
                .parked
                .as_ref()
                .unwrap()
                .service,
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
        assert!(s.get(&claude("work")).unwrap().parked.is_some());
        s.discard("current");
        s.discard("current");
        assert!(s.get(&claude("work")).unwrap().parked.is_none());
        assert_eq!(s.discarded, ["something-else", "current"]);
    }

    #[test]
    fn relabelling_keeps_the_account_its_park_and_whether_it_is_active() {
        let mut s = State::default();
        s.upsert(account("wrong", Some(park("p"))));
        s.upsert(account("other", None));
        s.set_active(ProviderId::Claude, Some("wrong".into()));

        assert_eq!(
            s.relabel(&claude("wrong"), "right").unwrap().email,
            "wrong@example.com"
        );
        assert!(s.get(&claude("wrong")).is_none());
        let renamed = s.get(&claude("right")).unwrap();
        assert_eq!(renamed.account_uuid, "wrong-uuid");
        assert_eq!(renamed.parked.as_ref().unwrap().service, "p");
        assert_eq!(s.active_for(ProviderId::Claude), Some("right"));
        assert!(s.discarded.is_empty(), "nothing is deleted by a rename");
    }

    #[test]
    fn relabelling_refuses_a_taken_label_and_an_unknown_one() {
        let mut s = State::default();
        s.upsert(account("a", None));
        s.upsert(account("b", None));
        s.set_active(ProviderId::Claude, Some("a".into()));
        assert!(matches!(
            s.relabel(&claude("a"), "b"),
            Err(Error::LabelTaken { .. })
        ));
        assert!(matches!(
            s.relabel(&claude("nobody"), "c"),
            Err(Error::AccountUnknown { .. })
        ));
        assert_eq!(
            s.active_for(ProviderId::Claude),
            Some("a"),
            "a refused rename changes nothing"
        );
        assert!(s.relabel(&claude("a"), "a").is_ok());
    }

    #[test]
    fn removing_an_account_lists_its_park_for_deletion() {
        let mut s = State::default();
        s.upsert(account("work", Some(park("p"))));
        assert_eq!(s.remove(&claude("work")).unwrap().label, "work");
        assert!(s.accounts.is_empty());
        assert_eq!(s.discarded, ["p"]);
    }

    /// A park `repair` gave back may be another pitboard's. Replaced by a newer one, or
    /// dropped with its account, it was never used here, so it is let go and not deleted.
    #[test]
    fn a_park_this_home_did_not_write_is_let_go_and_never_listed_for_deletion() {
        let mut s = State::default();
        s.upsert(account("work", None));
        s.upsert(account("home", None));
        s.park_foreign(&claude("work"), park("found"));
        s.park_foreign(&claude("home"), park("also-found"));
        assert_eq!(s.foreign, ["found", "also-found"]);

        s.park(&claude("work"), park("ours"));
        assert_eq!(
            s.get(&claude("work"))
                .unwrap()
                .parked
                .as_ref()
                .unwrap()
                .service,
            "ours"
        );
        s.remove(&claude("home"));
        s.release("ours");

        assert_eq!(s.discarded, ["ours"], "only the park this home wrote");
        assert!(s.foreign.is_empty(), "and nothing here holds the others");
    }

    /// Installed or renewed, a park has been used up whoever wrote it, and goes the way
    /// every used park goes.
    #[test]
    fn a_park_this_home_did_not_write_is_listed_once_it_is_used() {
        let mut s = State::default();
        s.upsert(account("work", None));
        s.park_foreign(&claude("work"), park("found"));
        s.discard("found");
        assert!(s.get(&claude("work")).unwrap().parked.is_none());
        assert_eq!(s.discarded, ["found"]);
        assert!(s.foreign.is_empty());
    }

    /// Schema 4 is new on this branch and gained the record of parks written elsewhere
    /// after it was first written, so a file without it has to load as one holding none.
    #[test]
    fn a_state_file_from_before_foreign_parks_were_recorded_still_loads() {
        let written = serde_json::json!({
            "schema": SCHEMA,
            "machine": machine_id(),
            "accounts": [{
                "label": "work", "account_uuid": "acc-1", "email": "a@b.c",
                "provider": "codex", "workspace_id": null, "plan": "pro",
                "parked": {
                    "service": "pitboard-park-acc-1-1789935600123",
                    "parked_at": 1_789_935_600,
                    "refresh_fingerprint": "abcd",
                    "access_expires_at": null,
                    "refresh_expires_at": null
                }
            }],
            "active": {}, "slot": {}, "discarded": []
        });
        let mut document = written.clone();
        migrate(&mut document, std::path::Path::new("/tmp/state.json")).expect("current");
        let mut state: State = serde_json::from_value(document).expect("still parses");
        assert!(state.foreign.is_empty());
        state.remove(&Key::new(ProviderId::Codex, "work"));
        assert_eq!(
            state.discarded,
            ["pitboard-park-acc-1-1789935600123"],
            "a park recorded before is this home's own, and goes as it always did"
        );
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
        assert_eq!(s.get(&claude("work")).unwrap().email, "d@e.f");
    }

    fn codex_account(label: &str) -> Account {
        Account {
            last_used_at: None,
            label: label.into(),
            account_uuid: format!("codex-{label}-uuid"),
            email: format!("{label}@openai.example"),
            detail: Detail::Codex {
                workspace_id: None,
                plan: None,
            },
            parked: None,
        }
    }

    /// Two tools, one label. Every lookup used to take the label alone and return the
    /// first match, so `pitboard use codex/work` could park and install Claude Code's
    /// `work` instead, and enrolling Codex's `work` replaced Claude Code's outright.
    #[test]
    fn two_tools_can_each_have_an_account_of_the_same_name() {
        let mut s = State::default();
        s.upsert(account("work", Some(park("claude-park"))));
        s.upsert(codex_account("work"));
        assert_eq!(
            s.accounts.len(),
            2,
            "the second is added, not a replacement"
        );

        let codex = Key::new(ProviderId::Codex, "work");
        assert_eq!(s.get(&codex).unwrap().provider(), ProviderId::Codex);
        assert_eq!(
            s.get(&claude("work")).unwrap().provider(),
            ProviderId::Claude
        );

        s.park(&codex, park("codex-park"));
        assert_eq!(
            s.get(&claude("work"))
                .unwrap()
                .parked
                .as_ref()
                .unwrap()
                .service,
            "claude-park",
            "parking one tool's account leaves the other's alone"
        );

        s.remove(&codex);
        assert!(s.get(&claude("work")).is_some(), "and so does dropping it");
        assert_eq!(s.discarded, ["codex-park"]);
    }

    /// A label is unique within a tool, so renaming into a name only another tool uses is
    /// not a clash.
    #[test]
    fn a_rename_clashes_only_within_its_own_tool() {
        let mut s = State::default();
        s.upsert(account("personal", None));
        s.upsert(codex_account("work"));
        assert!(s.relabel(&claude("personal"), "work").is_ok());
        assert_eq!(
            s.get(&claude("work")).unwrap().email,
            "personal@example.com"
        );
    }

    /// Forgetting the account a tool last switched to must not leave that tool's record
    /// naming it: a later account enrolled under the same label would read as in use.
    #[test]
    fn removing_the_account_in_use_clears_that_tools_record_only() {
        let mut s = State::default();
        s.upsert(account("work", None));
        s.upsert(codex_account("work"));
        s.set_active(ProviderId::Claude, Some("work".into()));
        s.set_active(ProviderId::Codex, Some("work".into()));
        s.remove(&Key::new(ProviderId::Codex, "work"));
        assert_eq!(s.active_for(ProviderId::Codex), None);
        assert_eq!(s.active_for(ProviderId::Claude), Some("work"));
    }

    /// A bare name for an account whose label another tool shares would be refused as
    /// ambiguous, so the name every message suggests is qualified exactly then.
    #[test]
    fn a_name_is_qualified_where_a_bare_one_would_be_ambiguous() {
        let mut s = State::default();
        s.upsert(account("work", None));
        s.upsert(account("personal", None));
        s.upsert(codex_account("work"));
        assert_eq!(s.typed(&claude("work")), "claude/work");
        assert_eq!(s.typed(&Key::new(ProviderId::Codex, "work")), "codex/work");
        assert_eq!(s.typed(&claude("personal")), "personal");
    }

    #[test]
    fn a_key_reads_the_way_it_would_be_typed() {
        assert_eq!(claude("work").to_string(), "work");
        assert_eq!(
            Key::new(ProviderId::Codex, "work").to_string(),
            "codex/work"
        );
        assert_eq!(claude("work").qualified(), "claude/work");
    }
}
