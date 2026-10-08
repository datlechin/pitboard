//! Which accounts Pitboard knows and where each one is parked. No secrets: the logins stay
//! in the keychain or vault.
//!
//! Stamped with the machine that wrote it, because a parked login belongs to exactly one
//! machine: presenting a refresh token another machine has since rotated ends the login on
//! both.

use crate::api::Owner;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::in_use::{Identified, InUse, Replaced};
use crate::provider::ProviderId;
use crate::service::Permit;
use crate::{atomic, home};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

const SCHEMA: u32 = 6;

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
    /// What Pitboard files the account's parked logins, readings, budget and windows under.
    /// Set at enrolment and never changed, so none of them has to move.
    pub id: String,
    /// The account as its tool names it: Anthropic's account uuid, which is the person and
    /// not the login, or Codex's ChatGPT account joined with the person in it.
    pub account_uuid: String,
    pub email: String,
    pub parked: Option<Park>,
    /// When this account last came to be in use, in epoch seconds: switched to, enrolled
    /// while signed in, or named by its service for the login its tool has stored.
    ///
    /// Pitboard renews a parked login for as long as the account is enrolled, so an account
    /// somebody enrolled once and never came back to keeps a live, continuously rotated
    /// refresh token on the machine indefinitely. Nothing said so, and nothing asked.
    /// Recording this is what lets `doctor` say it.
    ///
    /// `None` on an account enrolled before this was recorded, and on one that has never
    /// been in use.
    #[serde(default)]
    pub last_used_at: Option<i64>,
    /// When the account's only login was replaced by a sign-in outside Pitboard, in epoch
    /// seconds: its tool's store held that login with nothing parked for it, and its service
    /// then named another account's login there, or none. Pitboard keeps no copy of a login
    /// in use, so nothing brings it back. Cleared once the account holds a parked login or is
    /// in use again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_at: Option<i64>,
    /// Which tool's login this is, and whatever only that tool keeps.
    #[serde(flatten)]
    pub detail: Detail,
}

/// One account, the way Pitboard tells accounts apart: which tool, and what it is called
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

    /// Whose login this account's is, as its tool's service names one.
    pub fn owner(&self) -> Owner {
        Owner {
            account_uuid: self.account_uuid.clone(),
            email: self.email.clone(),
            organization_uuid: match &self.detail {
                Detail::Claude {
                    organization_uuid, ..
                } => organization_uuid.clone(),
                Detail::Codex { workspace_id, .. } => workspace_id.clone().unwrap_or_default(),
            },
        }
    }

    /// Whether `owner`'s login is this account's.
    pub fn owned_by(&self, owner: &Owner) -> bool {
        self.account_uuid == owner.account_uuid
            && match &self.detail {
                // One person in two organisations holds two logins, each on its own plan.
                Detail::Claude {
                    organization_uuid, ..
                } => *organization_uuid == owner.organization_uuid,
                // Codex's account id already joins the workspace with the person in it.
                Detail::Codex { .. } => true,
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
    /// Whose login each tool has stored, as its service last said, by the tool's code.
    ///
    /// Until schema 6 the account Pitboard last switched to stood in for this, as `active`.
    /// A sign-in outside Pitboard left that naming an account whose login was gone, and
    /// nothing read the store to notice.
    #[serde(default)]
    pub in_use: BTreeMap<String, InUse>,
    /// The credential slot each tool's `in_use` was recorded for. One state file serves
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
    /// shares the login keychain, so each may be another Pitboard's parked login, and
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
            in_use: BTreeMap::new(),
            slot: BTreeMap::new(),
            discarded: Vec::new(),
            foreign: Vec::new(),
        }
    }
}

impl State {
    /// Whose login `which` has stored, as its service last said.
    pub fn in_use(&self, which: ProviderId) -> Option<&InUse> {
        self.in_use.get(which.code())
    }

    /// The account whose login `which` has stored, as its service last said. `None` where
    /// it stored none, where nothing is recorded, and where that login's account is not
    /// enrolled.
    pub fn account_in_use(&self, which: ProviderId) -> Option<&Account> {
        let owner = self.in_use(which)?.owner.as_ref()?;
        self.account_of(which, owner)
    }

    /// Record whose login `which` has stored, as its service said it at `at`.
    ///
    /// A record that agrees with `found` is kept as it is, with when it was first said.
    /// Where the login is another account's than before, that account came to be in use at
    /// `at`, and the one before lost the login it had in use. With nothing parked for it,
    /// that was its only login, gone to a sign-in outside Pitboard: a switch parks the login
    /// it moves out first.
    pub fn identified(&mut self, which: ProviderId, found: InUse, at: i64) -> Identified {
        let before = self.in_use.get(which.code());
        if before.is_some_and(|known| known.agrees(which, &found)) {
            return Identified {
                changed: false,
                replaced: None,
            };
        }
        let was = before.and_then(|known| known.owner.clone());
        let mut replaced = None;
        if before.and_then(|known| known.account(which)) != found.account(which) {
            if let Some(previous) = was
                .as_ref()
                .and_then(|owner| self.account_of_mut(which, owner))
                && previous.parked.is_none()
            {
                previous.replaced_at = Some(at);
                replaced = Some(Replaced {
                    key: previous.key(),
                });
            }
            if let Some(now) = found
                .owner
                .as_ref()
                .and_then(|owner| self.account_of_mut(which, owner))
            {
                now.replaced_at = None;
                now.last_used_at = Some(at);
            }
        }
        self.in_use.insert(which.code().to_string(), found);
        Identified {
            changed: true,
            replaced,
        }
    }

    /// The credential slot this tool's `in_use` was recorded for.
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

    /// This tool's account holding `owner`'s login.
    pub fn account_of(&self, provider: ProviderId, owner: &Owner) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|a| a.provider() == provider && a.owned_by(owner))
    }

    /// What `owner`'s login is filed under: its account's id, or the one it would be
    /// enrolled with.
    pub fn id_of(&self, provider: ProviderId, owner: &Owner) -> String {
        self.account_of(provider, owner)
            .map_or_else(|| new_id(provider, owner), |a| a.id.clone())
    }

    /// This tool's account filed under `id`.
    pub fn by_id(&self, provider: ProviderId, id: &str) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|a| a.provider() == provider && a.id == id)
    }

    /// The account a parked item was written for.
    ///
    /// A park's name carries the account's id and not its tool, because names were fixed
    /// before there was a second tool and every item already on a machine is filed under
    /// one. The ids cannot collide in practice: each is built from its tool's UUIDs.
    pub fn owner_of_park(&self, id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    fn get_mut(&mut self, key: &Key) -> Option<&mut Account> {
        self.accounts.iter_mut().find(|a| a.is(key))
    }

    fn account_of_mut(&mut self, provider: ProviderId, owner: &Owner) -> Option<&mut Account> {
        self.accounts
            .iter_mut()
            .find(|a| a.provider() == provider && a.owned_by(owner))
    }

    /// Hold `park` for the account, releasing whatever it replaces. A newer park does not
    /// use the one before it, so that one is let go rather than consumed.
    pub fn park(&mut self, key: &Key, park: Park) {
        let service = park.service.clone();
        let Some(account) = self.get_mut(key) else {
            return;
        };
        account.replaced_at = None;
        if let Some(previous) = account.parked.replace(park)
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
    /// `repair` gave back may be another Pitboard's parked login, which this one never
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
            let renamed = self.get(from).map(|a| a.email.as_str());
            return Err(Error::LabelTaken {
                label: target.typed(),
                who: crate::words::login(
                    from.provider,
                    &taken.email,
                    renamed == Some(taken.email.as_str()),
                ),
            });
        }
        let enrolled = self.labels(from.provider);
        let account = self.get_mut(from).ok_or_else(|| Error::AccountUnknown {
            label: from.typed(),
            enrolled,
        })?;
        account.label = to.to_string();
        Ok(account)
    }

    /// Drop the account, releasing its park. Whose login its tool has stored stays recorded:
    /// the login is where it was, and the record names its owner, not a label.
    pub fn remove(&mut self, key: &Key) -> Option<Account> {
        let index = self.accounts.iter().position(|a| a.is(key))?;
        let account = self.accounts.remove(index);
        if let Some(park) = &account.parked {
            self.release(&park.service);
        }
        Some(account)
    }
}

/// The id a login is enrolled with. Claude's joins the organisation, since Anthropic's
/// account uuid is the person and each of their organisations is a login of its own. `_`
/// is in neither uuid, and is safe in a park's name and a readings file.
pub fn new_id(provider: ProviderId, owner: &Owner) -> String {
    match provider {
        ProviderId::Claude => format!("{}_{}", owner.account_uuid, owner.organization_uuid),
        ProviderId::Codex => owner.account_uuid.clone(),
    }
}

/// Hashed, so the raw platform identifier never lands in a file Pitboard writes.
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

/// When Pitboard's account index last changed, in epoch seconds, or 0 when there is none.
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
///
/// A build that may do nothing here reads nothing either ([`crate::release`]), so an app on
/// a Windows build of a release before Windows is released is refused at its first read.
pub(crate) fn load_any_machine(ctx: &Context) -> Result<(State, bool)> {
    crate::release::check()?;
    let path = file(ctx);
    home::check_absolute(ctx)?;
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
    // An account of a tool this build does not know was written by a newer Pitboard, not
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
    // Whose login a tool has stored is a fact about one slot. Read from another, the record
    // says nothing. Per tool, so a changed `CLAUDE_CONFIG_DIR` says nothing about Codex's
    // record, nor `CODEX_HOME` about Claude Code's.
    for &tool in ProviderId::ALL {
        let slot = crate::provider::of(tool).slot(ctx);
        if state
            .slot_for(tool)
            .is_some_and(|recorded| recorded != slot)
        {
            state.in_use.remove(tool.code());
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
/// routes, so on one machine an older Pitboard will meet a file a newer one wrote. Reading
/// forwards is what this is for; reading backwards is not possible, and says so.
fn migrate(document: &mut serde_json::Value, path: &std::path::Path) -> Result<()> {
    // Each bump adds a step after the ones before it, so a file two versions behind is
    // brought all the way forward in one read.
    let found = document
        .get("schema")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default() as u32;
    match found {
        SCHEMA => Ok(()),
        3..SCHEMA => {
            if found == 3 {
                three_to_four(document);
            }
            if found <= 4 {
                four_to_five(document);
            }
            five_to_six(document);
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
    document["schema"] = serde_json::json!(4);
}

/// Schema 4 filed every account under its tool's account id, and Anthropic's is the person:
/// one person in two organisations was one account. Schema 5 gives each account an id of
/// its own, and an account already enrolled keeps the one everything is filed under, so
/// no park, reading or window has to move.
fn four_to_five(document: &mut serde_json::Value) {
    if let Some(accounts) = document.get_mut("accounts").and_then(Value::as_array_mut) {
        for account in accounts {
            if let Some(fields) = account.as_object_mut()
                && let Some(id) = fields.get("account_uuid").cloned()
            {
                fields.insert("id".into(), id);
            }
        }
    }
    document["schema"] = serde_json::json!(5);
}

/// Schema 5 recorded the account Pitboard last switched to, by label, which a sign-in
/// outside Pitboard left naming an account whose login was gone. Schema 6 records whose login
/// each tool has stored, as its service said. A record brought forward is that account's,
/// with no login and `known_at` 0: nothing established which login the store held. A label
/// naming no account names nobody.
fn five_to_six(document: &mut serde_json::Value) {
    let accounts: Vec<Account> = document
        .get("accounts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|account| serde_json::from_value(account.clone()).ok())
        .collect();
    let in_use: serde_json::Map<String, Value> = document
        .get("active")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(code, label)| {
            let key = Key::new(ProviderId::parse(code)?, label.as_str()?);
            let account = accounts.iter().find(|a| a.is(&key))?;
            let unconfirmed = InUse {
                owner: Some(account.owner()),
                login: String::new(),
                known_at: 0,
                named: None,
            };
            let record = serde_json::to_value(unconfirmed).expect("a record is serialisable");
            Some((code.clone(), record))
        })
        .collect();
    if let Some(fields) = document.as_object_mut() {
        fields.remove("active");
        fields.insert("in_use".into(), Value::Object(in_use));
    }
    document["schema"] = serde_json::json!(SCHEMA);
}

pub(crate) fn save(ctx: &Context, permit: Permit, state: &State) -> Result<()> {
    home::check_absolute(ctx)?;
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
    home::ensure(ctx, permit).map_err(write)?;
    let body = serde_json::to_string_pretty(state).expect("State is always serialisable");
    atomic::write(permit, &path, body.as_bytes(), atomic::Perms::Secret).map_err(write)
}

#[cfg(test)]
mod tests {
    /// CLAUDE_CONFIG_DIR picks which keychain item is the live one, and one state file
    /// serves every slot on a machine. A record of whose login one slot holds says nothing
    /// about another, so it is not carried over, and says nothing about Codex's at all.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn whose_login_another_slot_holds_is_not_claimed_here() {
        let home = std::env::temp_dir().join(format!("pitboard-slots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let here = Context::new(home.clone()).with_pitboard_home(home.clone());
        let elsewhere = here
            .clone()
            .with_claude_config_dir("/somewhere/else".into());

        let mut state = State::default();
        state.accounts.push(account("work", None));
        state.accounts.push(codex_account("personal"));
        let found = InUse::of(&state.accounts[0], "f", 100);
        state.identified(ProviderId::Claude, found.clone(), 100);
        let codex = InUse::of(&state.accounts[1], "f", 100);
        state.identified(ProviderId::Codex, codex.clone(), 100);
        save(&here, Permit::for_a_test(), &state).expect("saved");

        assert_eq!(
            load(&here).unwrap().in_use(ProviderId::Claude),
            Some(&found)
        );
        let moved = load(&elsewhere).unwrap();
        assert_eq!(
            moved.in_use(ProviderId::Claude),
            None,
            "another slot's record of whose login is stored is not this slot's"
        );
        assert_eq!(
            moved.in_use(ProviderId::Codex),
            Some(&codex),
            "Codex's slot did not move"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// `CODEX_HOME` picks which `auth.json` is Codex's live login, the way
    /// `CLAUDE_CONFIG_DIR` picks Claude Code's keychain item. A record made under one home
    /// says nothing about another, and says nothing about Claude Code's at all.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn another_codex_home_is_another_codex_slot() {
        let home = std::env::temp_dir().join(format!(
            "pitboard-codex-slots-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        let here = Context::new(home.clone()).with_pitboard_home(home.clone());
        let mut state = State::default();
        state.accounts.push(account("work", None));
        state.accounts.push(codex_account("work"));
        for (which, at) in [(ProviderId::Claude, 0), (ProviderId::Codex, 1)] {
            let found = InUse::of(&state.accounts[at], "f", 100);
            state.identified(which, found, 100);
        }
        save(&here, Permit::for_a_test(), &state).expect("saved");

        let label_in_use = |state: &State, which| {
            state
                .account_in_use(which)
                .map(|account| account.label.clone())
        };
        let moved = here.clone().with_codex_home("/somewhere/else".into());
        let loaded = load(&moved).unwrap();
        assert_eq!(loaded.in_use(ProviderId::Codex), None);
        assert_eq!(
            label_in_use(&loaded, ProviderId::Claude).as_deref(),
            Some("work"),
            "Claude Code's slot did not move"
        );
        assert_eq!(
            label_in_use(&load(&here).unwrap(), ProviderId::Codex).as_deref(),
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
        assert_eq!(state.get(&claude("work")).unwrap().id, "acc-1");
        assert_eq!(
            state.account_in_use(ProviderId::Claude).map(Account::key),
            Some(claude("work"))
        );
    }

    /// Migrating a file that is already current must change nothing.
    ///
    /// `three_to_four` rewrites `active` and `slot` in place and `five_to_six` drops
    /// `active`, and a version check that slipped would wrap an already-wrapped map into
    /// `{"claude": {"claude": "work"}}`, or find no `active` to bring forward, and lose whose
    /// login is stored, silently, on every load after that.
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
        migrate(&mut once, std::path::Path::new("/tmp/state.json")).expect("3 to 6");
        let mut twice = once.clone();
        migrate(&mut twice, std::path::Path::new("/tmp/state.json")).expect("6 is current");
        assert_eq!(once, twice, "a second migration must change nothing");
        assert!(once.get("active").is_none());
        assert_eq!(
            once["in_use"]["claude"]["owner"]["account_uuid"],
            serde_json::json!("acc-1")
        );
        assert_eq!(
            once["slot"],
            serde_json::json!({"claude": "Claude Code-credentials"})
        );
        assert_eq!(once["accounts"][0]["provider"], "claude");
        assert_eq!(once["accounts"][0]["id"], "acc-1");
        assert_eq!(once["schema"], SCHEMA);
    }

    /// Schema 3 had no `active` at all when nothing had been switched to, and a migration
    /// that turned that into a one-entry map naming nothing would claim a switch happened.
    #[test]
    fn a_file_that_never_switched_migrates_to_no_account_in_use() {
        let mut document = serde_json::json!({
            "schema": 3, "machine": machine_id(), "accounts": [], "discarded": []
        });
        migrate(&mut document, std::path::Path::new("/tmp/state.json")).expect("3 to 6");
        let state: State = serde_json::from_value(document).expect("parses");
        assert!(state.in_use.is_empty() && state.slot.is_empty());
    }

    /// A file as 0.9.0 writes it names the account last switched to. It comes forward as
    /// that account's record with nothing established about the login, so nothing reads it
    /// as what the service said until the service is asked.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_schema_5_file_comes_forward_with_the_account_in_use_unconfirmed() {
        let home = std::env::temp_dir().join(format!(
            "pitboard-schema-5-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let ctx = Context::new(home.clone()).with_pitboard_home(home.clone());
        std::fs::write(
            home.join("state.json"),
            serde_json::json!({
                "schema": 5,
                "machine": machine_id(),
                "accounts": [{
                    "label": "work",
                    "id": "acc-1_org-1",
                    "account_uuid": "acc-1",
                    "email": "a@b.c",
                    "parked": null,
                    "last_used_at": 1_789_935_000,
                    "provider": "claude",
                    "organization_uuid": "org-1",
                    "oauth_account": {"accountUuid": "acc-1", "organizationUuid": "org-1"}
                }],
                "active": {"claude": "work", "codex": "gone"},
                "slot": {},
                "discarded": [],
                "foreign": []
            })
            .to_string(),
        )
        .unwrap();

        let state = load(&ctx).expect("brought forward");
        assert_eq!(
            state.in_use(ProviderId::Claude),
            Some(&InUse {
                owner: Some(Owner {
                    account_uuid: "acc-1".into(),
                    email: "a@b.c".into(),
                    organization_uuid: "org-1".into(),
                }),
                login: String::new(),
                known_at: 0,
                named: None,
            })
        );
        assert_eq!(
            state.in_use(ProviderId::Codex),
            None,
            "a label naming no account names nobody"
        );

        save(&ctx, Permit::for_a_test(), &state).expect("saved");
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(home.join("state.json")).unwrap())
                .unwrap();
        assert_eq!(written["schema"], 6);
        assert!(written.get("active").is_none(), "{written}");
        assert_eq!(written["in_use"]["claude"]["known_at"], 0);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A file as 0.8.0 writes it. Each account keeps the id its parks, readings and windows
    /// are already filed under.
    #[test]
    fn the_format_0_8_0_writes_keeps_every_account_where_it_is_filed() {
        let mut document = serde_json::json!({
            "schema": 4,
            "machine": machine_id(),
            "accounts": [
                {
                    "label": "work",
                    "account_uuid": "acc-1",
                    "email": "a@b.c",
                    "parked": {
                        "service": "pitboard-park-acc-1-1789935600123",
                        "parked_at": 1_789_935_600,
                        "refresh_fingerprint": "abcd",
                        "access_expires_at": 1_789_999_999,
                        "refresh_expires_at": 1_792_000_000
                    },
                    "last_used_at": 1_789_935_000,
                    "provider": "claude",
                    "organization_uuid": "org-1",
                    "oauth_account": {
                        "accountUuid": "acc-1",
                        "emailAddress": "a@b.c",
                        "organizationUuid": "org-1"
                    }
                },
                {
                    "label": "work",
                    "account_uuid": "team_user-1",
                    "email": "a@b.c",
                    "parked": null,
                    "last_used_at": null,
                    "provider": "codex",
                    "workspace_id": "team",
                    "plan": "team"
                }
            ],
            "active": {"claude": "work", "codex": "work"},
            "slot": {"claude": "Claude Code-credentials", "codex": "/home/a/.codex"},
            "discarded": [],
            "foreign": []
        });
        migrate(&mut document, std::path::Path::new("/tmp/state.json")).expect("4 to 6");
        let state: State = serde_json::from_value(document).expect("parses");

        let work = state.get(&claude("work")).unwrap();
        assert_eq!(
            (work.id.as_str(), work.account_uuid.as_str()),
            ("acc-1", "acc-1")
        );
        let codex = state.get(&Key::new(ProviderId::Codex, "work")).unwrap();
        assert_eq!(codex.id, "team_user-1");
        assert_eq!(
            state.owner_of_park("acc-1").map(Account::key),
            Some(work.key())
        );
        let signed_in = Owner {
            account_uuid: "acc-1".into(),
            email: "a@b.c".into(),
            organization_uuid: "org-1".into(),
        };
        assert_eq!(
            state.id_of(ProviderId::Claude, &signed_in),
            "acc-1",
            "the login it was enrolled with is still found by its organisation"
        );
    }

    /// Anthropic's account uuid is the person. Their logins to two organisations are two
    /// accounts, and a third organisation's is neither.
    #[test]
    fn one_person_in_two_organisations_is_two_accounts() {
        let in_org = |label: &str, id: &str, org: &str| Account {
            last_used_at: None,
            replaced_at: None,
            label: label.into(),
            id: id.into(),
            account_uuid: "acc".into(),
            email: "a@b.c".into(),
            detail: Detail::Claude {
                organization_uuid: org.into(),
                oauth_account: serde_json::json!({}),
            },
            parked: None,
        };
        let mut state = State::default();
        state.upsert(in_org("work", "acc", "org-a"));
        state.upsert(in_org("team", "acc_org-b", "org-b"));
        let signed_in = |org: &str| Owner {
            account_uuid: "acc".into(),
            email: "a@b.c".into(),
            organization_uuid: org.into(),
        };
        let label_of = |org: &str| {
            state
                .account_of(ProviderId::Claude, &signed_in(org))
                .map(|a| a.label.as_str())
        };

        assert_eq!(label_of("org-a"), Some("work"));
        assert_eq!(label_of("org-b"), Some("team"));
        assert_eq!(label_of("org-c"), None);
        assert_eq!(
            state.id_of(ProviderId::Claude, &signed_in("org-c")),
            "acc_org-c"
        );
        assert!(
            state
                .account_of(ProviderId::Codex, &signed_in("org-a"))
                .is_none()
        );
    }

    /// `Detail` is flattened and tagged, which is the whole reason the migration has
    /// nothing to move. If it ever stopped sitting beside `label` in the file, every
    /// account already written would stop loading.
    #[test]
    fn a_providers_own_fields_sit_beside_the_shared_ones() {
        let account = Account {
            label: "work".into(),
            id: "acc".into(),
            account_uuid: "acc".into(),
            email: "a@b.c".into(),
            parked: None,
            last_used_at: None,
            replaced_at: None,
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

    /// A file naming a tool this build does not know came from a newer Pitboard. Called
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
                "in_use": {}, "slot": {}, "discarded": []
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
    fn a_file_from_a_newer_pitboard_is_refused() {
        let mut current = serde_json::json!({"schema": 6, "machine": machine_id(), "accounts": []});
        migrate(&mut current, std::path::Path::new("/tmp/state.json")).expect("this one's own");
        let mut document = serde_json::json!({"schema": 7});
        let err = migrate(&mut document, std::path::Path::new("/tmp/state.json")).unwrap_err();
        assert_eq!(err.code(), "state_from_newer_version");
        let said = err.to_string();
        assert!(said.contains("Update this Pitboard"), "{said}");
        assert!(
            !said.contains("brew") && !said.contains("cargo"),
            "it names no one way of installing Pitboard: {said}"
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
            replaced_at: None,
            label: label.into(),
            id: format!("{label}-uuid"),
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
    fn relabelling_keeps_the_account_and_its_park() {
        let mut s = State::default();
        s.upsert(account("wrong", Some(park("p"))));
        s.upsert(account("other", None));

        assert_eq!(
            s.relabel(&claude("wrong"), "right").unwrap().email,
            "wrong@example.com"
        );
        assert!(s.get(&claude("wrong")).is_none());
        let renamed = s.get(&claude("right")).unwrap();
        assert_eq!(renamed.account_uuid, "wrong-uuid");
        assert_eq!(renamed.parked.as_ref().unwrap().service, "p");
        assert!(s.discarded.is_empty(), "nothing is deleted by a rename");
    }

    /// The record names whose login is stored, not a label, so a rename has nothing in it
    /// to change.
    #[test]
    fn renaming_an_account_leaves_the_record_alone() {
        let mut s = State::default();
        s.upsert(account("wrong", None));
        let found = InUse::of(s.get(&claude("wrong")).unwrap(), "f", 100);
        s.identified(ProviderId::Claude, found.clone(), 100);

        s.relabel(&claude("wrong"), "right").unwrap();

        assert_eq!(s.in_use(ProviderId::Claude), Some(&found));
        assert_eq!(
            s.account_in_use(ProviderId::Claude)
                .map(|a| a.label.as_str()),
            Some("right")
        );
    }

    #[test]
    fn relabelling_refuses_a_taken_label_and_an_unknown_one() {
        let mut s = State::default();
        s.upsert(account("a", None));
        s.upsert(account("b", None));
        assert!(matches!(
            s.relabel(&claude("a"), "b"),
            Err(Error::LabelTaken { .. })
        ));
        assert!(matches!(
            s.relabel(&claude("nobody"), "c"),
            Err(Error::AccountUnknown { .. })
        ));
        assert_eq!(
            s.accounts
                .iter()
                .map(|a| a.label.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"],
            "a refused rename changes nothing"
        );
        assert!(s.relabel(&claude("a"), "a").is_ok());
    }

    /// Anthropic naming another account's login, where the account in use had nothing
    /// parked, means a sign-in outside Pitboard wrote over that account's only login.
    #[test]
    fn an_answer_naming_another_account_marks_the_account_whose_only_login_it_replaced() {
        let mut s = State::default();
        s.upsert(account("work", None));
        s.upsert(account("personal", None));
        s.upsert(account("spare", Some(park("p"))));
        let said = |s: &State, label: &str, login: &str, at: i64| {
            InUse::of(s.get(&claude(label)).unwrap(), login, at)
        };
        let work = said(&s, "work", "work-login", 100);
        s.identified(ProviderId::Claude, work, 100);

        let personal = said(&s, "personal", "personal-login", 200);
        let identified = s.identified(ProviderId::Claude, personal.clone(), 200);
        assert!(identified.changed);
        assert_eq!(
            identified.replaced,
            Some(Replaced {
                key: claude("work")
            })
        );
        assert_eq!(s.get(&claude("work")).unwrap().replaced_at, Some(200));
        let now = s.get(&claude("personal")).unwrap();
        assert_eq!((now.last_used_at, now.replaced_at), (Some(200), None));
        assert_eq!(s.in_use(ProviderId::Claude), Some(&personal));

        // The same account on another login: a renewal, or a sign-in to it again.
        let renewed = InUse {
            login: "personal-renewed".into(),
            known_at: 300,
            ..personal
        };
        let identified = s.identified(ProviderId::Claude, renewed.clone(), 300);
        assert!(identified.changed);
        assert_eq!(identified.replaced, None);
        assert_eq!(
            s.get(&claude("personal")).unwrap().last_used_at,
            Some(200),
            "still the account it was"
        );

        let again = InUse {
            known_at: 400,
            ..renewed
        };
        assert!(!s.identified(ProviderId::Claude, again, 400).changed);
        assert_eq!(
            s.in_use(ProviderId::Claude).map(|r| r.known_at),
            Some(300),
            "known since it was first said"
        );

        // An account that holds a parked login lost nothing when its login in use went.
        let spare = said(&s, "spare", "spare-login", 500);
        s.identified(ProviderId::Claude, spare, 500);
        let back = said(&s, "work", "work-login-2", 600);
        let identified = s.identified(ProviderId::Claude, back, 600);
        assert_eq!(identified.replaced, None);
        assert_eq!(s.get(&claude("spare")).unwrap().replaced_at, None);
        assert_eq!(
            s.get(&claude("work")).unwrap().replaced_at,
            None,
            "in use again"
        );
    }

    #[test]
    fn parking_a_replaced_account_clears_it() {
        let mut s = State::default();
        s.upsert(account("work", None));
        s.upsert(account("personal", None));
        for (label, at) in [("work", 100), ("personal", 200)] {
            let found = InUse::of(s.get(&claude(label)).unwrap(), label, at);
            s.identified(ProviderId::Claude, found, at);
        }
        assert_eq!(s.get(&claude("work")).unwrap().replaced_at, Some(200));

        s.park(&claude("work"), park("p"));

        assert_eq!(s.get(&claude("work")).unwrap().replaced_at, None);
    }

    #[test]
    fn removing_an_account_lists_its_park_for_deletion() {
        let mut s = State::default();
        s.upsert(account("work", Some(park("p"))));
        assert_eq!(s.remove(&claude("work")).unwrap().label, "work");
        assert!(s.accounts.is_empty());
        assert_eq!(s.discarded, ["p"]);
    }

    /// A park `repair` gave back may be another Pitboard's. Replaced by a newer one, or
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
            "schema": 4,
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
            replaced_at: None,
            label: label.into(),
            id: format!("codex-{label}-uuid"),
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

    /// Forgetting an account leaves the login its tool has stored where it was, and the
    /// record says whose it is by owner, so another account given the same label is not in
    /// use, and the same account enrolled again is.
    #[test]
    fn removing_the_account_in_use_leaves_whose_login_is_stored() {
        let mut s = State::default();
        s.upsert(account("work", None));
        s.upsert(codex_account("work"));
        let codex = Key::new(ProviderId::Codex, "work");
        let found = InUse::of(s.get(&codex).unwrap(), "f", 100);
        s.identified(ProviderId::Codex, found.clone(), 100);

        let forgotten = s.remove(&codex).expect("enrolled");
        assert_eq!(s.in_use(ProviderId::Codex), Some(&found));
        assert!(s.account_in_use(ProviderId::Codex).is_none());

        s.upsert(Account {
            id: "codex-other-uuid".into(),
            account_uuid: "codex-other-uuid".into(),
            ..codex_account("work")
        });
        assert!(
            s.account_in_use(ProviderId::Codex).is_none(),
            "a label is not who is in use"
        );
        s.upsert(forgotten);
        assert_eq!(
            s.account_in_use(ProviderId::Codex).map(Account::key),
            Some(codex)
        );
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
