//! Which accounts Pitboard knows and where each one is parked. No secrets: the logins stay
//! in the keychain or vault.
//!
//! Stamped with the machine that wrote it, because a parked login belongs to exactly one
//! machine: presenting a refresh token another machine has since rotated ends the login on
//! both.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::host::Os;
use crate::in_use::{Identified, InUse, Replaced};
use crate::permit::Permit;
use crate::provider::ProviderId;
use crate::provider::{Identity, Owner};
use crate::{atomic, home};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

const SCHEMA: u32 = 9;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParkKind {
    #[default]
    Vault,
    // a folder under `<pitboard home>/parks`, holding an app's own files
    Folder,
}

impl ParkKind {
    // By reference, as `skip_serializing_if` calls it.
    pub fn is_vault(&self) -> bool {
        matches!(self, ParkKind::Vault)
    }
}

/// How a home keeps its parked logins, where its system offers more than one way
/// ([`crate::host::Host::vault_forms`]): on Linux, as items of the Secret Service or as files
/// in the home.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VaultForm {
    SecretService,
    Files,
}

// Where a home's parks are, recorded by its first save on a system that offers a choice.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Vault {
    pub(crate) form: VaultForm,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Dropped {
    pub service: String,
    #[serde(default, skip_serializing_if = "ParkKind::is_vault")]
    pub kind: ParkKind,
    // Set by `forget_folder` alone, for `State::remove` and `uninstall`: a held login goes then.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub forgotten: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SetAside {
    InTheWay,
    Superseded,
    AnotherMachine,
    GivenUp,
    Orphan,
}

// A folder under `parks` no account holds as its park: only `forget` and `uninstall` delete it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Aside {
    pub name: String,
    pub tool: ProviderId,
    pub at: i64,
    pub why: SetAside,
    // the id it was parked for
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    // the fingerprint of the session its jar holds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Half {
    Leave,
    Arrive,
}

// One half of a folder login's move, written before its first rename and removed by the save
// that records what the half came to.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Moving {
    pub half: Half,
    pub started_at: i64,
    // the account whose login moves
    pub id: String,
    pub label: String,
    // the folder under `parks` it moves to (leave) or from (arrive)
    pub park: String,
    pub login: String,
    // the items the source held when the record was written
    pub items: Vec<String>,
    // SHA-256, hex, of each account key's value as the source held it
    pub keys: BTreeMap<String, String>,
    // each folder this move reserved for what it set aside, and why, saved before it is made
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub asides: BTreeMap<String, SetAside>,
    pub slot: String,
    // the sign-in this leave is for, copied into `awaiting` by its commit
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub awaited: Option<Awaited>,
}

// A sign-in inside the app is waited for, with an account parked for it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Awaited {
    pub since: i64,
    // the name asked
    pub label: String,
    // the id that label has already: a Sign In Again
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    // the id of the account parked for it
    pub put_back: String,
}

/// A login held for an account while another is signed in. There is at most one per
/// account: once installed it is Claude Code's again, and Claude Code rotates it from then
/// on, so a copy kept back would only ever present a token it has moved past.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Park {
    pub service: String,
    #[serde(default, skip_serializing_if = "ParkKind::is_vault")]
    pub kind: ParkKind,
    pub parked_at: i64,
    // a document's refresh token's, a folder login's session cookie's as stored
    #[serde(rename = "refresh_fingerprint")]
    pub login: String,
    /// Until then its usage can be asked without renewing it first.
    pub access_expires_at: Option<i64>,
    /// Until then it can be restored.
    pub refresh_expires_at: Option<i64>,
    // the jar's own date for the session cookie: said, never a refusal
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_expires_at: Option<i64>,
}

/// A login of a tool's own that Pitboard renewed and has not saved where the tool keeps it
/// yet, with the vault item holding a copy of the renewed login meanwhile.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub(crate) struct Renewing {
    pub(crate) tool: ProviderId,
    /// The tool's credential slot it is kept in.
    pub(crate) slot: String,
    /// Which of the tool's stores keeps it, as [`crate::store::Backend::name`] names it.
    pub(crate) store: String,
    /// The fingerprint of the refresh token the renewal sent.
    pub(crate) sent: String,
    /// The vault item holding the renewed login.
    pub(crate) service: String,
}

impl Park {
    pub fn restorable_at(&self, now: i64) -> bool {
        self.refresh_expires_at.is_none_or(|at| at > now)
    }

    pub(crate) fn askable_at(&self, now: i64) -> bool {
        self.access_expires_at.is_none_or(|at| at > now)
    }
}

pub(crate) const RENEW_WITHIN: i64 = 3 * 86_400;

/// Whether a login whose refresh token expires at `refresh_expires_at` is due to be renewed:
/// within three days, and not yet expired.
pub fn renewal_due(refresh_expires_at: i64, now: i64) -> bool {
    (1..RENEW_WITHIN).contains(&(refresh_expires_at - now))
}

/// What one provider keeps about an account that the others have no equivalent of.
///
/// A tagged enum rather than a pile of optional fields, so no code reading a Codex account
/// ever has to decide what an absent Claude organisation means for it. `provider` is the
/// tag, and the variant's own fields sit beside `label` and `email` in the file, which is
/// why a schema 3 account needs nothing moved to become a schema 4 one.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub(crate) enum Detail {
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
    },
    Desktop {},
}

/// Claude Code's own extras, for a caller that has already established it is holding a
/// Claude account.
#[derive(Debug)]
pub(crate) struct ClaudeDetail<'a> {
    pub(crate) organization_uuid: &'a str,
    pub(crate) oauth_account: &'a Value,
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
    /// seconds: its tool's store held that login with nothing parked for it and no other
    /// slot's record naming it, and its service then named another account's login there, or
    /// none. Pitboard keeps no copy of a login in use, so nothing brings it back. Cleared once
    /// the account holds a parked login or is in use again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_at: Option<i64>,
    /// The plan its login last said, as a label: "Max 20x", "Pro".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// Which tool's login this is, and whatever only that tool keeps.
    #[serde(flatten)]
    pub(crate) detail: Detail,
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

    // A label 0.1.x wrote with a slash, such as `team/a`, where `team` names no tool, is whole.
    pub fn parse(typed: &str) -> Key {
        match crate::label::parse(typed) {
            Ok(crate::label::Spec::Qualified(provider, label)) => Key::new(provider, label),
            _ => Key::new(crate::label::DEFAULT, typed),
        }
    }
}

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.typed())
    }
}

impl Account {
    pub(crate) fn key(&self) -> Key {
        Key::new(self.provider(), self.label.clone())
    }

    pub(crate) fn is(&self, key: &Key) -> bool {
        self.label == key.label && self.provider() == key.provider
    }

    pub fn provider(&self) -> ProviderId {
        match self.detail {
            Detail::Claude { .. } => ProviderId::Claude,
            Detail::Codex { .. } => ProviderId::Codex,
            Detail::Desktop {} => ProviderId::Desktop,
        }
    }

    /// Whose login this account's is, as its tool's service names one.
    pub(crate) fn owner(&self) -> Owner {
        Owner {
            account_uuid: self.account_uuid.clone(),
            email: self.email.clone(),
            organization_uuid: match &self.detail {
                Detail::Claude {
                    organization_uuid, ..
                } => organization_uuid.clone(),
                Detail::Codex { workspace_id, .. } => workspace_id.clone().unwrap_or_default(),
                Detail::Desktop {} => String::new(),
            },
        }
    }

    /// Whether `owner`'s login is this account's.
    pub(crate) fn owned_by(&self, owner: &Owner) -> bool {
        self.holds(&owner.identity())
    }

    /// Whether `identity` is this account's login.
    pub(crate) fn holds(&self, identity: &Identity) -> bool {
        self.account_uuid == identity.account_uuid
            && match &self.detail {
                // One person in two organisations holds two logins, each on its own plan.
                Detail::Claude {
                    organization_uuid, ..
                } => *organization_uuid == identity.organization_uuid,
                // Codex's account id already joins the workspace with the person in it.
                Detail::Codex { .. } => true,
                // One claude.ai session covers the person's organisations.
                Detail::Desktop {} => true,
            }
    }

    /// Claude Code's extras, or `None` when this account belongs to another tool.
    pub(crate) fn claude(&self) -> Option<ClaudeDetail<'_>> {
        match &self.detail {
            Detail::Claude {
                organization_uuid,
                oauth_account,
            } => Some(ClaudeDetail {
                organization_uuid,
                oauth_account,
            }),
            Detail::Codex { .. } | Detail::Desktop {} => None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct State {
    pub(crate) schema: u32,
    pub(crate) machine: String,
    pub(crate) accounts: Vec<Account>,
    /// Whose login each tool has stored, as its service last said, by the tool's code: the
    /// record of the credential slot `slot` names for that tool.
    #[serde(default)]
    pub(crate) in_use: BTreeMap<String, InUse>,
    /// The credential slot each tool's `in_use` was recorded for. One state file serves
    /// every slot a machine uses, and a tool's own home variable changes which store is the
    /// live one, so a record made in one slot says nothing about another.
    #[serde(default)]
    pub(crate) slot: BTreeMap<String, String>,
    /// The record of each other slot a tool was read in, by the tool's code and then the
    /// slot. [`load`] puts the slot it reads in place of the one `slot` names, so a read or a
    /// change in one slot keeps every other slot's record for when that slot is read again.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) other_slots: BTreeMap<String, BTreeMap<String, InUse>>,
    /// Parked items no account refers to any more. Listed in the same save that drops them
    /// and removed once deleted, so a delete that fails or is interrupted is retried.
    #[serde(default)]
    pub discarded: Vec<Dropped>,
    /// Parked items an account here holds that this home never wrote down: logins
    /// `pitboard repair` found in the store and gave back. On macOS every `PITBOARD_HOME`
    /// shares the login keychain, so each may be another Pitboard's parked login, and
    /// letting one go unused must leave it where it is. Removed once nothing here holds it.
    /// Always empty where the vault is a directory inside this home, which nobody else
    /// parks in.
    #[serde(default)]
    pub(crate) foreign: Vec<String>,
    /// Parked items that copy the login in a file behind a tool's store which is still
    /// there, each written down before the copy is. A put-away that stopped leaves the login
    /// in both, and a session that signs in with the file can renew it there, which spends
    /// the copy. So the next put-away never keeps one of these in place of what the file
    /// holds by then. Emptied once the file has gone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) from_file: Vec<String>,
    /// Renewals of a tool's own login whose answer is not saved where the tool keeps it yet,
    /// each written down before the refresh token is sent. At most one per slot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) renewing: Vec<Renewing>,
    // by tool code
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub moving: BTreeMap<String, Moving>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aside: Vec<Aside>,
    // by tool code
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub awaiting: BTreeMap<String, Awaited>,
    /// How this home keeps its parked logins, where the system offers a choice. Recorded by
    /// the home's first save and never changed by a save after it: the parks are where it
    /// says, and a home that took another form would no longer find them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) vault: Option<Vault>,
}

impl Default for State {
    fn default() -> Self {
        State {
            schema: SCHEMA,
            machine: machine_id(),
            accounts: Vec::new(),
            in_use: BTreeMap::new(),
            slot: BTreeMap::new(),
            other_slots: BTreeMap::new(),
            discarded: Vec::new(),
            foreign: Vec::new(),
            from_file: Vec::new(),
            renewing: Vec::new(),
            moving: BTreeMap::new(),
            aside: Vec::new(),
            awaiting: BTreeMap::new(),
            vault: None,
        }
    }
}

impl State {
    /// Whose login `which` has stored, as its service last said.
    pub(crate) fn in_use(&self, which: ProviderId) -> Option<&InUse> {
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
    /// `at`, and the one before lost the login it had in use. With nothing parked for it and
    /// no other slot's record naming it, that was its only login, gone to a sign-in outside
    /// Pitboard: a switch parks the login it moves out first.
    pub(crate) fn identified(&mut self, which: ProviderId, found: InUse, at: i64) -> Identified {
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
                .filter(|owner| !self.stored_in_another_slot(which, owner))
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

    /// Whether another slot's record says `which` has `owner`'s login stored there, which a
    /// change in this slot leaves where it is.
    fn stored_in_another_slot(&self, which: ProviderId, owner: &Owner) -> bool {
        let account = Some(new_id(which, owner));
        self.other_slots.get(which.code()).is_some_and(|records| {
            records
                .values()
                .any(|record| record.account(which) == account)
        })
    }

    /// Make `which`'s record the one of `slot`. Where it is of another slot, that one waits
    /// with the other slots' records, and the record `slot` was last given, where there is
    /// one, comes back. A record with no slot named is taken as `slot`'s.
    fn in_slot(&mut self, which: ProviderId, slot: String) {
        let code = which.code();
        if let Some(recorded) = self.slot.get(code).filter(|recorded| **recorded != slot) {
            let others = self.other_slots.entry(code.to_string()).or_default();
            if let Some(record) = self.in_use.remove(code) {
                others.insert(recorded.clone(), record);
            }
            if let Some(record) = others.remove(&slot) {
                self.in_use.insert(code.to_string(), record);
            }
            if others.is_empty() {
                self.other_slots.remove(code);
            }
        }
        self.slot.insert(code.to_string(), slot);
    }

    /// Forget whose login every tool has stored, in every slot.
    pub(crate) fn forget_in_use(&mut self) {
        self.in_use.clear();
        self.slot.clear();
        self.other_slots.clear();
    }

    /// The account's name as a command would take it here: bare for the tool a bare name
    /// means, unless another tool has an account of the same name, in which case a bare
    /// name would be refused as ambiguous and the tool is said.
    ///
    /// What every message that tells somebody what to type uses, so the command it names
    /// is one that works on this machine.
    pub(crate) fn typed(&self, key: &Key) -> String {
        let shared = self
            .accounts
            .iter()
            .any(|a| a.label == key.label && a.provider() != key.provider);
        if shared { key.qualified() } else { key.typed() }
    }

    /// Every label this tool has enrolled, for a message that would otherwise send someone
    /// to another command to find out.
    pub(crate) fn labels(&self, provider: ProviderId) -> crate::error::Enrolled {
        crate::error::Enrolled(
            self.accounts
                .iter()
                .filter(|a| a.provider() == provider)
                .map(|a| a.label.clone())
                .collect(),
        )
    }

    // A vault item's name or a folder's: held, waiting to be deleted, renewing, moving or set
    // aside.
    pub fn names(&self, service: &str) -> bool {
        self.accounts
            .iter()
            .filter_map(|a| a.parked.as_ref())
            .any(|p| p.service == service)
            || self.discarded.iter().any(|d| d.service == service)
            || self
                .renewing
                .iter()
                .any(|renewing| renewing.service == service)
            || self.aside.iter().any(|aside| aside.name == service)
            || self.moves(service)
    }

    // Whether a move under way holds the folder `name`, as its park or as one it set aside.
    pub fn moves(&self, name: &str) -> bool {
        self.moving
            .values()
            .any(|moving| moving.park == name || moving.asides.contains_key(name))
    }

    pub fn moving(&self, which: ProviderId) -> Option<&Moving> {
        self.moving.get(which.code())
    }

    pub fn awaiting(&self, which: ProviderId) -> Option<&Awaited> {
        self.awaiting.get(which.code())
    }

    /// The account under this key.
    ///
    /// Callers that took a name from a person should go through [`crate::label::resolve`]
    /// first, which knows what to do when two tools share a label.
    pub(crate) fn get(&self, key: &Key) -> Option<&Account> {
        self.accounts.iter().find(|a| a.is(key))
    }

    /// This tool's account holding `owner`'s login.
    pub(crate) fn account_of(&self, provider: ProviderId, owner: &Owner) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|a| a.provider() == provider && a.owned_by(owner))
    }

    /// Whose `identity` is, where Pitboard knows the account: enrolled, or the one its record
    /// last named in use.
    pub(crate) fn named(&self, provider: ProviderId, identity: &Identity) -> Option<Owner> {
        let enrolled = self
            .accounts
            .iter()
            .find(|a| a.provider() == provider && a.holds(identity))
            .map(|a| Owner::named(identity.clone(), a.email.clone()));
        enrolled.or_else(|| {
            self.in_use(provider)?
                .owner
                .clone()
                .filter(|owner| owner.identity() == *identity)
        })
    }

    /// What `owner`'s login is filed under: its account's id, or the one it would be
    /// enrolled with.
    pub(crate) fn id_of(&self, provider: ProviderId, owner: &Owner) -> String {
        self.id_for(provider, &owner.identity())
    }

    /// [`State::id_of`], for a login known by its identity alone.
    pub(crate) fn id_for(&self, provider: ProviderId, identity: &Identity) -> String {
        self.accounts
            .iter()
            .find(|a| a.provider() == provider && a.holds(identity))
            .map_or_else(|| identity_id(provider, identity), |a| a.id.clone())
    }

    /// This tool's account filed under `id`.
    pub(crate) fn by_id(&self, provider: ProviderId, id: &str) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|a| a.provider() == provider && a.id == id)
    }

    /// The account a parked item was written for.
    ///
    /// A park's name carries the account's id and not its tool, because names were fixed
    /// before there was a second tool and every item already on a machine is filed under
    /// one. The ids cannot collide in practice: each is built from its tool's UUIDs.
    pub(crate) fn owner_of_park(&self, id: &str) -> Option<&Account> {
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

    // The park it replaces: a vault item is released, a folder is set aside.
    pub fn park(&mut self, key: &Key, park: Park) {
        let (service, at) = (park.service.clone(), park.parked_at);
        let Some(account) = self.get_mut(key) else {
            return;
        };
        account.replaced_at = None;
        if let Some(previous) = account.parked.replace(park)
            && previous.service != service
        {
            match previous.kind {
                ParkKind::Vault => self.release(&previous.service),
                ParkKind::Folder => self.set_aside(key, previous, SetAside::Superseded, at),
            }
        }
    }

    // Keep `park` of `key`'s account as a folder no account holds, and stop holding it.
    pub fn set_aside(&mut self, key: &Key, park: Park, why: SetAside, at: i64) {
        let account = self.get_mut(key).map(|account| {
            account.parked.take_if(|held| held.service == park.service);
            account.id.clone()
        });
        if self.aside.iter().any(|aside| aside.name == park.service) {
            return;
        }
        self.aside.push(Aside {
            name: park.service,
            tool: key.provider,
            at,
            why,
            account,
            login: Some(park.login),
        });
    }

    // A folder a move leaves that no account holds as its park any more. `aside.login` is
    // the session its jar holds, so none where the jar went back into the app's folder.
    pub(crate) fn keep_aside(&mut self, aside: Aside) {
        self.let_go(&aside.name);
        if !self.aside.iter().any(|kept| kept.name == aside.name) {
            self.aside.push(aside);
        }
    }

    // An aside given back as a park, or deleted.
    pub(crate) fn drop_aside(&mut self, name: &str) -> Option<Aside> {
        let at = self.aside.iter().position(|aside| aside.name == name)?;
        Some(self.aside.remove(at))
    }

    pub(crate) fn aside_of(&self, id: &str) -> impl Iterator<Item = &Aside> {
        self.aside
            .iter()
            .filter(move |aside| aside.account.as_deref() == Some(id))
    }

    /// Hold a park this home did not write, found in the store and given back. It is used
    /// like any other, and deleted only once it has been.
    pub(crate) fn park_foreign(&mut self, key: &Key, park: Park) {
        let service = park.service.clone();
        self.park(key, park);
        if self.references(&service) && !self.is_foreign(&service) {
            self.foreign.push(service);
        }
    }

    /// Whether an account here holds `service` without this home having written it.
    pub(crate) fn is_foreign(&self, service: &str) -> bool {
        self.foreign.iter().any(|listed| listed == service)
    }

    /// Take `service` as this home's own from now on, because this home has used it: a
    /// renewal presented its refresh token. Letting it go afterwards deletes it.
    pub(crate) fn used_here(&mut self, service: &str) {
        self.foreign.retain(|listed| listed != service);
    }

    // Installed, spent by a renewal, or a copy of a login still signed in: nothing can use
    // it again, and a copy must not stay beside its login, so it goes whoever wrote it.
    pub(crate) fn discard(&mut self, service: &str, kind: ParkKind) {
        self.let_go(service);
        if !self
            .discarded
            .iter()
            .any(|listed| listed.service == service)
        {
            self.discarded.push(Dropped {
                service: service.to_string(),
                kind,
                forgotten: false,
            });
        }
    }

    /// Stop holding `service` because nothing here wants it any more: a newer park replaced
    /// it, or its account was dropped. Listed for deletion only when this home wrote it. One
    /// `repair` gave back may be another Pitboard's parked login, which this one never
    /// used, and deleting it would end that account's session for somebody who never ran
    /// the command that did it.
    pub(crate) fn release(&mut self, service: &str) {
        if self.is_foreign(service) {
            self.let_go(service);
        } else {
            self.discard(service, ParkKind::Vault);
        }
    }

    /// No account holds `service` afterwards, and nothing records who wrote it or what it
    /// copied.
    fn let_go(&mut self, service: &str) {
        for account in &mut self.accounts {
            account.parked.take_if(|p| p.service == service);
        }
        self.foreign.retain(|listed| listed != service);
        self.from_file.retain(|listed| listed != service);
    }

    /// `service` is about to hold a copy of the login in a file behind a tool's store.
    pub(crate) fn copying_from_file(&mut self, service: &str) {
        if !self.is_from_file(service) {
            self.from_file.push(service.to_string());
        }
    }

    /// Whether `service` copies the login a file behind a tool's store held, with that file
    /// still there as far as anything recorded.
    pub(crate) fn is_from_file(&self, service: &str) -> bool {
        self.from_file.iter().any(|listed| listed == service)
    }

    /// The file behind a tool's store has gone, so no park copies it any more. Whether any
    /// was written down as one.
    pub(crate) fn file_gone(&mut self) -> bool {
        !std::mem::take(&mut self.from_file).is_empty()
    }

    pub(crate) fn references(&self, service: &str) -> bool {
        self.accounts
            .iter()
            .any(|a| a.parked.as_ref().is_some_and(|p| p.service == service))
    }

    /// Add the account, or replace the one this tool already has under its label.
    pub(crate) fn upsert(&mut self, account: Account) {
        match self.get_mut(&account.key()) {
            Some(existing) => *existing = account,
            None => self.accounts.push(account),
        }
    }

    /// Enroll the account under `from` as `to` instead, inside the same tool. Only the
    /// label changes: parked logins are named by account, not by label.
    pub(crate) fn relabel(&mut self, from: &Key, to: &str) -> Result<&Account> {
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

    // Whose login its tool has stored stays recorded: the record names its owner, not a label.
    pub fn remove(&mut self, key: &Key) -> Option<Account> {
        let index = self.accounts.iter().position(|a| a.is(key))?;
        let account = self.accounts.remove(index);
        if let Some(park) = &account.parked {
            match park.kind {
                ParkKind::Vault => self.release(&park.service),
                ParkKind::Folder => self.forget_folder(&park.service),
            }
        }
        let filed: Vec<String> = self
            .aside_of(&account.id)
            .map(|aside| aside.name.clone())
            .collect();
        for name in filed {
            self.drop_aside(&name);
            self.forget_folder(&name);
        }
        self.awaiting
            .retain(|_, awaited| awaited.put_back != account.id);
        for awaited in self.awaiting.values_mut() {
            awaited.account.take_if(|id| *id == account.id);
        }
        Some(account)
    }

    // Every folder set aside listed for deletion, as `uninstall` asks.
    pub(crate) fn forget_asides(&mut self) {
        for aside in std::mem::take(&mut self.aside) {
            self.forget_folder(&aside.name);
        }
    }

    // Every folder listed for deletion deleted whatever it still holds, as `uninstall` asks.
    pub(crate) fn forget_discarded(&mut self) {
        let folders: Vec<String> = self
            .discarded
            .iter()
            .filter(|dropped| dropped.kind == ParkKind::Folder && !dropped.forgotten)
            .map(|dropped| dropped.service.clone())
            .collect();
        for name in folders {
            self.forget_folder(&name);
        }
    }

    // The one way a folder that holds a login is listed to be deleted. Never one a move holds.
    fn forget_folder(&mut self, name: &str) {
        self.let_go(name);
        if self.moves(name) {
            return;
        }
        self.discarded.retain(|listed| listed.service != name);
        self.discarded.push(Dropped {
            service: name.to_string(),
            kind: ParkKind::Folder,
            forgotten: true,
        });
    }
}

/// The id a login is enrolled with. Claude's joins the organisation, since Anthropic's
/// account uuid is the person and each of their organisations is a login of its own. `_`
/// is in neither uuid, and is safe in a park's name and a readings file.
pub(crate) fn new_id(provider: ProviderId, owner: &Owner) -> String {
    identity_id(provider, &owner.identity())
}

pub(crate) fn identity_id(provider: ProviderId, identity: &Identity) -> String {
    match provider {
        ProviderId::Claude => format!("{}_{}", identity.account_uuid, identity.organization_uuid),
        ProviderId::Codex => identity.account_uuid.clone(),
        ProviderId::Desktop => format!("desktop_{}", identity.account_uuid),
    }
}

// What `machine_id` answers where the platform's id could not be read.
pub(crate) const UNKNOWN_MACHINE: &str = "unknown";

/// Hashed, so the raw platform identifier never lands in a file Pitboard writes.
pub(crate) fn machine_id() -> String {
    use sha2::{Digest, Sha256};
    match machine_uid::get() {
        Ok(raw) => hex::encode(Sha256::digest(raw.as_bytes())),
        Err(_) => String::from(UNKNOWN_MACHINE),
    }
}

// The account index's name in Pitboard's directory.
pub(crate) const FILE: &str = "state.json";

fn file(ctx: &Context) -> PathBuf {
    home::dir(ctx).join(FILE)
}

// One stat of the account index alone: the readings the status line writes after every
// message say nothing of who is signed in, and a front end follows them apart.
pub(crate) fn changed_at(ctx: &Context) -> i64 {
    crate::time::changed_at(&file(ctx))
}

pub(crate) fn load(ctx: &Context) -> Result<State> {
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
    let path = file(ctx);
    let Some(document) = current_document(ctx)? else {
        return Ok((State::default(), true));
    };
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
    // Whose login a tool has stored is a fact about one slot. Per tool, so a changed
    // `CLAUDE_CONFIG_DIR` says nothing about Codex's record, nor `CODEX_HOME` about Claude
    // Code's.
    for &tool in ProviderId::ALL {
        if let Some(slot) = crate::provider::slot(ctx, tool) {
            state.in_slot(tool, slot);
        }
    }
    Ok((state, here))
}

// The state file brought up to this schema, or `None` where there is no file yet.
fn current_document(ctx: &Context) -> Result<Option<Value>> {
    crate::release::check()?;
    let path = file(ctx);
    home::check_absolute(ctx)?;
    home::check_location(ctx, &home::dir(ctx))?;
    let raw = match atomic::read(&path, |path| std::fs::read_to_string(path)) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(Error::StateUnreadable { path, source }),
    };
    let mut document: Value = serde_json::from_str(&raw).map_err(|source| Error::StateCorrupt {
        path: path.clone(),
        source,
    })?;
    migrate(&mut document, &path, ctx.os)?;
    Ok(Some(document))
}

/// How this home keeps its parked logins, as its state file records it: `None` where it
/// records none, as a home with no state file yet does, and every home on a system that
/// keeps its parks one way.
///
/// Read from the file at each look rather than from a state in memory, so the vault every
/// caller opens is the one the home's last save named.
pub(crate) fn vault_form(ctx: &Context) -> Result<Option<VaultForm>> {
    let Some(mut document) = current_document(ctx)? else {
        return Ok(None);
    };
    let Some(vault) = document.get_mut("vault").map(Value::take) else {
        return Ok(None);
    };
    let vault: Vault = serde_json::from_value(vault).map_err(|source| Error::StateCorrupt {
        path: file(ctx),
        source,
    })?;
    Ok(Some(vault.form))
}

/// Where a home that records no form yet writes down the one its parks go in, before the
/// first is written: a save of the state as it stands, which records it. The park, its name
/// and the save that names it then agree, even where the Secret Service stops answering
/// between them. Nothing is written where the system offers no choice, or the home recorded one.
pub(crate) fn record_vault(ctx: &Context, permit: Permit) -> Result<()> {
    if ctx.host().vault_forms().is_empty() || vault_form(ctx)?.is_some() {
        return Ok(());
    }
    let (state, _) = load_any_machine(ctx)?;
    save(ctx, permit, &state)
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
///
/// `os` is the system reading it, which wrote it: a step may bring a file forward as that
/// system's Pitboard kept it.
fn migrate(document: &mut Value, path: &std::path::Path, os: Os) -> Result<()> {
    // Each bump adds a step after the ones before it, so a file two versions behind is
    // brought all the way forward in one read.
    let found = document
        .get("schema")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let found = u32::try_from(found).unwrap_or(u32::MAX);
    match found {
        SCHEMA => Ok(()),
        3..SCHEMA => {
            if found == 3 {
                three_to_four(document);
            }
            if found <= 4 {
                four_to_five(document);
            }
            if found <= 5 {
                five_to_six(document);
            }
            if found <= 6 {
                six_to_seven(document);
            }
            if found <= 7 {
                seven_to_eight(document);
            }
            eight_to_nine(document, os);
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
fn three_to_four(document: &mut Value) {
    let claude = Value::from(ProviderId::Claude.code());
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
fn four_to_five(document: &mut Value) {
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
fn five_to_six(document: &mut Value) {
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
    document["schema"] = serde_json::json!(6);
}

/// Schema 7 adds `renewing`, which a schema 6 file reads as empty. It is a bump so that an
/// earlier Pitboard refuses the file: one would drop `renewing` at its next save, leaving the
/// copy of the renewed login named by nothing, so the next renewal of that login, by this
/// version or by 0.10.0's `stow`, would send the spent refresh token again.
fn six_to_seven(document: &mut Value) {
    document["schema"] = serde_json::json!(7);
}

// Schema 8 holds Claude Desktop's login, kept as files, beside one kept as a document: a park's
// `kind`, `discarded` as typed entries that say `forgotten`, `moving`, `aside`, `awaiting`, and
// Claude Desktop's `in_use` slot, the data folder's path. A bump: every earlier release reads
// `discarded` as names, and would call a file of typed entries corrupt.
fn seven_to_eight(document: &mut Value) {
    if let Some(discarded) = document.get_mut("discarded").and_then(Value::as_array_mut) {
        for entry in discarded {
            if let Value::String(service) = entry {
                *entry = serde_json::json!({ "service": service });
            }
        }
    }
    document["schema"] = serde_json::json!(8);
}

/// Schema 9 records how a home keeps its parks (`vault`), where the system offers a choice. On
/// Linux every earlier Pitboard parked in files, so a schema 8 file there comes forward with
/// its parks in files; macOS and Windows keep theirs one way and record none. A bump: an
/// earlier Pitboard would drop `vault` at its next save and look for this home's parks in
/// files, where a home on the Secret Service has none.
fn eight_to_nine(document: &mut Value, os: Os) {
    match os {
        Os::Linux => document["vault"] = serde_json::json!({ "form": "files" }),
        Os::MacOs | Os::Windows => {}
    }
    document["schema"] = serde_json::json!(9);
}

pub(crate) fn save(ctx: &Context, permit: Permit, state: &State) -> Result<()> {
    home::check_absolute(ctx)?;
    home::check_location(ctx, &home::dir(ctx))?;
    let mut state = state.clone();
    for &tool in ProviderId::ALL {
        if let Some(slot) = crate::provider::slot(ctx, tool) {
            state.in_slot(tool, slot);
        }
    }
    state.vault = recorded_vault(ctx, &state)?;
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

// The form a save writes: none where the system keeps parks one way; otherwise the one this
// state holds, else the one the file records, else the one a new home takes.
fn recorded_vault(ctx: &Context, state: &State) -> Result<Option<Vault>> {
    let host = ctx.host();
    if host.vault_forms().is_empty() {
        return Ok(None);
    }
    let form = match state.vault {
        Some(vault) => Some(vault.form),
        None => vault_form(ctx)?.or_else(|| host.new_home_form(ctx)),
    };
    Ok(form.map(|form| Vault { form }))
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    #[test]
    fn a_key_reads_back_as_it_was_written() {
        for key in [
            Key::new(ProviderId::Claude, "work"),
            Key::new(ProviderId::Codex, "work"),
            Key::new(ProviderId::Desktop, "work"),
            Key::new(ProviderId::Claude, "team/a"),
        ] {
            assert_eq!(Key::parse(&key.typed()), key);
            assert_eq!(Key::parse(&key.qualified()), key);
        }
    }

    /// CLAUDE_CONFIG_DIR picks which keychain item is the live one, and one state file
    /// serves every slot on a machine. A record of whose login one slot holds says nothing
    /// about another, so it is not carried over, and says nothing about Codex's at all. It is
    /// kept for when its slot is read again, whatever another slot records meanwhile.
    #[test]
    fn whose_login_another_slot_holds_is_not_claimed_here() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let here = Context::new(home.clone()).with_pitboard_home(home.clone());
        let elsewhere = here
            .clone()
            .with_claude_config_dir(home.join("somewhere-else").to_string_lossy().into());

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

        let mut moved = moved;
        let there = InUse::of(&moved.accounts[0], "g", 200);
        moved.identified(ProviderId::Claude, there.clone(), 200);
        save(&elsewhere, Permit::for_a_test(), &moved).expect("saved");
        assert_eq!(
            load(&here).unwrap().in_use(ProviderId::Claude),
            Some(&found),
            "this slot's record is kept while another slot records its own"
        );
        save(&here, Permit::for_a_test(), &load(&here).unwrap()).expect("saved");
        assert_eq!(
            load(&elsewhere).unwrap().in_use(ProviderId::Claude),
            Some(&there),
            "and so is the other's"
        );
    }

    /// `CODEX_HOME` picks which `auth.json` is Codex's live login, the way
    /// `CLAUDE_CONFIG_DIR` picks Claude Code's keychain item. A record made under one home
    /// says nothing about another, and says nothing about Claude Code's at all.
    #[test]
    fn another_codex_home_is_another_codex_slot() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
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
        let moved = here
            .clone()
            .with_codex_home(home.join("somewhere-else").to_string_lossy().into());
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
        let mut document = written;
        migrate(
            &mut document,
            std::path::Path::new("/tmp/state.json"),
            crate::host::OS,
        )
        .expect("still current");
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
            "discarded": ["gone"]
        });
        migrate(
            &mut once,
            std::path::Path::new("/tmp/state.json"),
            crate::host::OS,
        )
        .expect("3 to 9");
        assert_eq!(once["discarded"], serde_json::json!([{"service": "gone"}]));
        let mut twice = once.clone();
        migrate(
            &mut twice,
            std::path::Path::new("/tmp/state.json"),
            crate::host::OS,
        )
        .expect("9 is current");
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
        migrate(
            &mut document,
            std::path::Path::new("/tmp/state.json"),
            crate::host::OS,
        )
        .expect("3 to 9");
        let state: State = serde_json::from_value(document).expect("parses");
        assert!(state.in_use.is_empty() && state.slot.is_empty());
    }

    /// A file as 0.9.0 writes it names the account last switched to. It comes forward as
    /// that account's record with nothing established about the login, so nothing reads it
    /// as what the service said until the service is asked.
    #[test]
    fn a_schema_5_file_comes_forward_with_the_account_in_use_unconfirmed() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().to_path_buf();
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
        assert_eq!(written["schema"], SCHEMA);
        assert!(written.get("active").is_none(), "{written}");
        assert_eq!(written["in_use"]["claude"]["known_at"], 0);
    }

    #[test]
    fn a_schema_6_file_is_written_back_at_this_schema_and_otherwise_as_it_was() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().to_path_buf();
        let ctx = Context::new(home.clone()).with_pitboard_home(home.clone());
        let mut state = State::default();
        state.accounts.push(account("work", Some(park("parked"))));
        let found = InUse::of(&state.accounts[0], "f", 100);
        state.identified(ProviderId::Claude, found, 100);
        state.discarded.push(Dropped {
            service: "gone".into(),
            kind: ParkKind::Vault,
            forgotten: false,
        });
        save(&ctx, Permit::for_a_test(), &state).expect("saved");
        let path = home.join("state.json");
        let read = |path: &PathBuf| -> Value {
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
        };
        let mut written = read(&path);
        written["schema"] = 6.into();
        std::fs::write(&path, written.to_string()).unwrap();

        let state = load(&ctx).expect("a schema 6 file is read");
        save(&ctx, Permit::for_a_test(), &state).expect("saved");

        brought_forward(&mut written);
        assert_eq!(read(&path), written);
    }

    #[test]
    fn a_schema_7_file_0_11_0_wrote_is_written_back_at_this_schema_with_its_discarded_parks_typed()
    {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().to_path_buf();
        let ctx = Context::new(home.clone()).with_pitboard_home(home.clone());
        let mut state = State::default();
        state.accounts.push(account("work", Some(park("parked"))));
        let found = InUse::of(&state.accounts[0], "f", 100);
        state.identified(ProviderId::Claude, found, 100);
        save(&ctx, Permit::for_a_test(), &state).expect("saved");
        let path = home.join("state.json");
        let read = |path: &PathBuf| -> Value {
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
        };
        let mut written = read(&path);
        written["schema"] = 7.into();
        written["discarded"] = serde_json::json!(["pitboard-park-a-1", "pitboard-park-b-2"]);
        std::fs::write(&path, written.to_string()).unwrap();

        let state = load(&ctx).expect("a schema 7 file is read");
        save(&ctx, Permit::for_a_test(), &state).expect("saved");

        brought_forward(&mut written);
        written["discarded"] = serde_json::json!([
            {"service": "pitboard-park-a-1"},
            {"service": "pitboard-park-b-2"}
        ]);
        assert_eq!(read(&path), written);
    }

    #[test]
    fn a_schema_7_park_reads_and_writes_as_it_was() {
        let written = serde_json::json!({
            "service": "pitboard-park-a-1", "parked_at": 100, "refresh_fingerprint": "0a1b",
            "access_expires_at": 200, "refresh_expires_at": null
        });
        let park: Park = serde_json::from_value(written.clone()).expect("a schema 7 park");
        assert_eq!(park.kind, ParkKind::Vault);
        assert_eq!(park.login, "0a1b");
        assert_eq!(park.session_expires_at, None);
        assert_eq!(serde_json::to_value(&park).unwrap(), written);

        let folder = Park {
            kind: ParkKind::Folder,
            session_expires_at: Some(900),
            ..park
        };
        let written = serde_json::to_value(&folder).unwrap();
        assert_eq!(written["kind"], "folder");
        assert_eq!(written["refresh_fingerprint"], "0a1b");
        assert_eq!(written["session_expires_at"], 900);
        assert_eq!(serde_json::from_value::<Park>(written).unwrap(), folder);
        let dropped = Dropped {
            service: "pitboard-park-a-1".into(),
            kind: ParkKind::Folder,
            forgotten: false,
        };
        assert_eq!(
            serde_json::to_value(&dropped).unwrap(),
            serde_json::json!({"service": "pitboard-park-a-1", "kind": "folder"})
        );
    }

    fn folder_park(service: &str, login: &str, at: i64) -> Park {
        Park {
            service: service.into(),
            kind: ParkKind::Folder,
            parked_at: at,
            login: login.into(),
            access_expires_at: None,
            refresh_expires_at: None,
            session_expires_at: None,
        }
    }

    #[test]
    fn a_move_and_a_sign_in_waited_for_name_their_folders_and_follow_a_forgotten_account() {
        let mut s = State::default();
        s.accounts.push(account("work", None));
        s.accounts
            .push(account("home", Some(folder_park("park-home", "f1", 100))));
        let (work, home) = (
            Key::new(ProviderId::Claude, "work"),
            Key::new(ProviderId::Claude, "home"),
        );
        let awaited = Awaited {
            since: 5,
            label: "travel".into(),
            account: Some("home-uuid".into()),
            put_back: "work-uuid".into(),
        };
        s.moving.insert(
            "desktop".into(),
            Moving {
                half: Half::Arrive,
                started_at: 5,
                id: "home-uuid".into(),
                label: "home".into(),
                park: "park-home".into(),
                login: "f1".into(),
                items: vec!["Cookies".into()],
                keys: BTreeMap::new(),
                asides: BTreeMap::from([("aside-1".to_string(), SetAside::InTheWay)]),
                slot: "/data".into(),
                awaited: None,
            },
        );
        s.awaiting.insert("desktop".into(), awaited.clone());
        assert!(s.names("park-home") && s.names("aside-1") && !s.names("aside-2"));
        assert!(s.moves("aside-1") && s.moving(ProviderId::Desktop).is_some());

        let written = serde_json::to_value(&s).unwrap();
        assert_eq!(written["moving"]["desktop"]["half"], "arrive");
        assert_eq!(
            written["moving"]["desktop"]["asides"]["aside-1"],
            "in_the_way"
        );
        assert_eq!(written["awaiting"]["desktop"]["put_back"], "work-uuid");
        let read: State = serde_json::from_value(written).unwrap();
        assert_eq!(read.moving, s.moving);
        assert_eq!(read.awaiting, s.awaiting);
        let plain = serde_json::to_value(State::default()).unwrap();
        assert!(plain.get("moving").is_none() && plain.get("awaiting").is_none());

        // A folder a move holds is never listed to be deleted, forgotten or not.
        s.remove(&home);
        assert!(discarded(&s).is_empty(), "{:?}", s.discarded);
        assert_eq!(
            s.awaiting["desktop"].account, None,
            "a Sign In Again of a forgotten account reads as an add"
        );
        s.remove(&work);
        assert!(s.awaiting.is_empty(), "nothing is left to put back");
    }

    #[test]
    fn a_folder_park_replaced_is_set_aside_and_never_listed_for_deletion() {
        let mut s = State::default();
        s.accounts.push(account("work", None));
        s.accounts.push(account("home", Some(park("vault-old"))));
        let (work, home) = (
            Key::new(ProviderId::Claude, "work"),
            Key::new(ProviderId::Claude, "home"),
        );
        s.park(&work, folder_park("folder-old", "f1", 100));
        s.park(&work, folder_park("folder-new", "f2", 200));
        assert_eq!(
            s.get(&work).unwrap().parked.as_ref().unwrap().service,
            "folder-new"
        );
        assert_eq!(
            s.aside,
            [Aside {
                name: "folder-old".into(),
                tool: ProviderId::Claude,
                at: 200,
                why: SetAside::Superseded,
                account: Some("work-uuid".into()),
                login: Some("f1".into()),
            }]
        );
        assert!(discarded(&s).is_empty(), "never listed for deletion");
        assert!(s.names("folder-old") && s.names("folder-new"));
        assert_eq!(s.aside_of("work-uuid").count(), 1);

        // A vault park replaced is let go as it always was.
        s.park(&home, park("vault-new"));
        assert_eq!(discarded(&s), ["vault-old"]);
        assert_eq!(s.aside.len(), 1);

        assert_eq!(
            s.drop_aside("folder-old").map(|a| a.name).as_deref(),
            Some("folder-old")
        );
        assert!(!s.names("folder-old"));
    }

    #[test]
    fn forgetting_an_account_is_the_only_way_its_folder_is_listed_forgotten() {
        let mut s = State::default();
        s.accounts.push(account("work", None));
        s.accounts.push(account("home", None));
        let (work, home) = (
            Key::new(ProviderId::Claude, "work"),
            Key::new(ProviderId::Claude, "home"),
        );
        s.park(&work, folder_park("work-1", "f1", 100));
        s.park(&work, folder_park("work-2", "f2", 200));
        s.park(&home, folder_park("home-1", "f3", 100));
        s.set_aside(
            &home,
            folder_park("home-0", "f0", 50),
            SetAside::GivenUp,
            300,
        );
        s.discard("emptied", ParkKind::Folder);
        assert!(
            s.discarded.iter().all(|d| !d.forgotten),
            "{:?}",
            s.discarded
        );

        s.remove(&work);
        let forgotten: Vec<&str> = s
            .discarded
            .iter()
            .filter(|d| d.forgotten)
            .map(|d| d.service.as_str())
            .collect();
        assert_eq!(
            forgotten,
            ["work-2", "work-1"],
            "its park, and what was set aside for it"
        );
        assert!(s.discarded.iter().all(|d| d.kind == ParkKind::Folder));
        assert_eq!(
            s.aside.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            ["home-0"],
            "another account's stays"
        );
        assert!(s.get(&home).unwrap().parked.is_some());

        s.forget_asides();
        assert!(s.aside.is_empty());
        assert!(
            s.discarded
                .iter()
                .any(|d| d.service == "home-0" && d.forgotten)
        );
        assert!(!s.discarded.iter().any(|d| d.service == "home-1"));
    }

    #[test]
    fn a_discarded_entry_already_typed_is_left_as_it_is() {
        let mut document = serde_json::json!({
            "schema": 7, "machine": machine_id(), "accounts": [],
            "discarded": ["pitboard-park-a-1", {"service": "pitboard-park-b-2"}]
        });
        migrate(
            &mut document,
            std::path::Path::new("/tmp/state.json"),
            crate::host::OS,
        )
        .expect("7 to 9");
        assert_eq!(
            document["discarded"],
            serde_json::json!([{"service": "pitboard-park-a-1"}, {"service": "pitboard-park-b-2"}])
        );
    }

    /// 0.11.0 kept a Codex plan beside the workspace, as its token says it.
    #[test]
    fn a_codex_plan_0_11_0_recorded_is_the_accounts_plan() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().to_path_buf();
        let ctx = Context::new(home.clone()).with_pitboard_home(home.clone());
        let mut state = State::default();
        state.accounts.push(codex_account("work"));
        save(&ctx, Permit::for_a_test(), &state).expect("saved");
        let path = home.join("state.json");
        let read = |path: &PathBuf| -> Value {
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
        };
        let mut written = read(&path);
        written["schema"] = 7.into();
        written["accounts"][0]["plan"] = "pro".into();
        std::fs::write(&path, written.to_string()).unwrap();

        let state = load(&ctx).expect("read");
        assert_eq!(state.accounts[0].plan.as_deref(), Some("pro"));
        save(&ctx, Permit::for_a_test(), &state).expect("saved");

        brought_forward(&mut written);
        assert_eq!(read(&path), written);
        assert_eq!(read(&path)["accounts"][0]["label"], "work");
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
        migrate(
            &mut document,
            std::path::Path::new("/tmp/state.json"),
            crate::host::OS,
        )
        .expect("4 to 9");
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
            plan: None,
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
            plan: None,
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
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().to_path_buf();
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
    }

    /// Two writes of the index within one second are two changes to a front end that follows
    /// it: a switch typed in a terminal in the second of the last write it saw went unseen
    /// until it next looked for another reason.
    #[test]
    fn two_writes_within_one_second_are_two_changes() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().to_path_buf();
        let ctx = Context::new(home.clone()).with_pitboard_home(home.clone());
        std::fs::write(home.join("state.json"), "{}").unwrap();
        let second = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_760_000_000);
        let written_at = |millis| {
            std::fs::File::options()
                .write(true)
                .open(home.join("state.json"))
                .and_then(|index| {
                    index.set_modified(second + std::time::Duration::from_millis(millis))
                })
                .expect("the index's time set");
            changed_at(&ctx)
        };
        assert_ne!(written_at(100), written_at(600));
    }

    // Every Linux Pitboard before schema 9 parked in files (`store/vault.rs`), so a home it kept
    // stays in files, even where a Secret Service answers by now.
    #[test]
    fn a_schema_8_file_linux_wrote_comes_forward_with_its_parks_in_files() {
        let written = serde_json::json!({
            "schema": 8, "machine": machine_id(), "accounts": [], "discarded": []
        });
        let mut document = written.clone();
        migrate(
            &mut document,
            std::path::Path::new("/tmp/state.json"),
            Os::Linux,
        )
        .expect("8 to 9");
        assert_eq!(document["schema"], 9);
        assert_eq!(document["vault"], serde_json::json!({"form": "files"}));
        let state: State = serde_json::from_value(document).expect("parses");
        assert_eq!(
            state.vault,
            Some(Vault {
                form: VaultForm::Files
            })
        );

        let scratch = tempfile::tempdir().expect("a scratch folder");
        let home = scratch.path().join("home");
        let env = crate::context::Environment::on(
            Os::Linux,
            [("HOME", home.to_string_lossy().into_owned())],
        );
        let mem = crate::host::memory::MemoryHost::by_form();
        let ctx = Context::read(&env, "cli")
            .with_pitboard_home(home.join(".pitboard"))
            .with_memory_stores(std::sync::Arc::clone(&mem));
        std::fs::create_dir_all(home.join(".pitboard")).expect("a Pitboard home");
        std::fs::write(home.join(".pitboard/state.json"), written.to_string()).expect("written");
        assert_eq!(vault_form(&ctx).expect("read"), Some(VaultForm::Files));
        save(&ctx, Permit::for_a_test(), &load(&ctx).expect("loaded")).expect("saved");
        let saved: Value = serde_json::from_str(
            &std::fs::read_to_string(home.join(".pitboard/state.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(saved["vault"], serde_json::json!({"form": "files"}));
        assert_eq!(saved["schema"], SCHEMA);
    }

    // macOS keeps every park in the keychain and Windows in files sealed to the account: one way
    // each, so neither records one.
    #[test]
    fn a_schema_8_file_from_macos_or_windows_comes_forward_with_no_vault_form() {
        for os in [Os::MacOs, Os::Windows] {
            let mut document = serde_json::json!({
                "schema": 8, "machine": machine_id(), "accounts": [], "discarded": []
            });
            migrate(&mut document, std::path::Path::new("/tmp/state.json"), os).expect("8 to 9");
            assert_eq!(document["schema"], 9, "{os:?}");
            assert!(document.get("vault").is_none(), "{os:?}: {document}");
            let state: State = serde_json::from_value(document).expect("parses");
            assert_eq!(state.vault, None, "{os:?}");
        }

        let scratch = tempfile::tempdir().expect("a scratch folder");
        let ctx = Context::new(scratch.path().to_path_buf())
            .with_pitboard_home(scratch.path().join(".pitboard"))
            .with_memory_stores(crate::host::memory::MemoryHost::new());
        save(&ctx, Permit::for_a_test(), &State::default()).expect("saved");
        let saved = std::fs::read_to_string(scratch.path().join(".pitboard/state.json")).unwrap();
        assert!(!saved.contains("vault"), "{saved}");
    }

    /// The other direction cannot work, and the message has to say which half to upgrade.
    #[test]
    fn a_file_from_a_newer_pitboard_is_refused() {
        let mut current =
            serde_json::json!({"schema": SCHEMA, "machine": machine_id(), "accounts": []});
        migrate(
            &mut current,
            std::path::Path::new("/tmp/state.json"),
            crate::host::OS,
        )
        .expect("this one's own");
        let mut document = serde_json::json!({"schema": SCHEMA + 1});
        let err = migrate(
            &mut document,
            std::path::Path::new("/tmp/state.json"),
            crate::host::OS,
        )
        .unwrap_err();
        assert_eq!(err.code(), "state_from_newer_version");
        let said = err.to_string();
        assert!(said.contains("Update this Pitboard"), "{said}");
        assert!(
            !said.contains("brew") && !said.contains("cargo"),
            "it names no one way of installing Pitboard: {said}"
        );
    }

    #[test]
    fn a_schema_number_too_large_to_hold_is_refused() {
        let mut document = serde_json::json!({
            "schema": (1_u64 << 32) + u64::from(SCHEMA),
            "machine": machine_id(),
            "accounts": []
        });
        let err = migrate(
            &mut document,
            std::path::Path::new("/tmp/state.json"),
            crate::host::OS,
        )
        .unwrap_err();
        assert_eq!(err.code(), "state_from_newer_version");
    }

    #[test]
    fn each_migration_step_stamps_the_schema_it_brings_a_file_to() {
        for (from, step) in [
            (3, three_to_four as fn(&mut Value)),
            (4, four_to_five),
            (5, five_to_six),
            (6, six_to_seven),
            (7, seven_to_eight),
            (8, |document| eight_to_nine(document, Os::Linux)),
            (8, |document| eight_to_nine(document, Os::MacOs)),
        ] {
            let mut document = serde_json::json!({"schema": from, "accounts": []});
            step(&mut document);
            assert_eq!(document["schema"], from + 1, "the step from {from}");
        }
    }

    use super::*;

    // An older file as this schema writes it back, on this system: on Linux, with its parks in
    // files, where every earlier Pitboard there kept them.
    fn brought_forward(written: &mut Value) {
        written["schema"] = SCHEMA.into();
        match crate::host::OS {
            Os::Linux => written["vault"] = serde_json::json!({"form": "files"}),
            Os::MacOs | Os::Windows => {}
        }
    }

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
            kind: ParkKind::Vault,
            login: "f".into(),
            access_expires_at: Some(200),
            refresh_expires_at: Some(300),
            session_expires_at: None,
        }
    }

    fn discarded(s: &State) -> Vec<&str> {
        s.discarded.iter().map(|d| d.service.as_str()).collect()
    }

    fn account(label: &str, parked: Option<Park>) -> Account {
        Account {
            last_used_at: None,
            replaced_at: None,
            plan: None,
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
        assert_eq!(discarded(&s), ["old"]);
        assert!(!s.references("old"));
    }

    #[test]
    fn discarding_releases_whichever_account_held_it_and_lists_it_once() {
        let mut s = State::default();
        s.upsert(account("work", Some(park("current"))));
        s.discard("something-else", ParkKind::Vault);
        assert!(s.get(&claude("work")).unwrap().parked.is_some());
        s.discard("current", ParkKind::Vault);
        s.discard("current", ParkKind::Vault);
        assert!(s.get(&claude("work")).unwrap().parked.is_none());
        assert_eq!(discarded(&s), ["something-else", "current"]);
    }

    /// A park let go of, installed or replaced, is no longer written down as a copy of a
    /// file's login, and the list is left out of the file while it is empty.
    #[test]
    fn a_park_let_go_of_is_no_longer_a_copy_of_a_files_login() {
        let mut s = State::default();
        s.upsert(account("work", Some(park("copy"))));
        assert!(!serde_json::to_string(&s).unwrap().contains("from_file"));
        s.copying_from_file("copy");
        s.copying_from_file("copy");
        assert_eq!(s.from_file, ["copy"]);
        assert!(s.is_from_file("copy"));

        s.park(&claude("work"), park("renewed"));
        assert!(s.from_file.is_empty());
        assert!(!s.file_gone(), "nothing was written down by then");
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

    /// An account signed in under two `CLAUDE_CONFIG_DIR`s keeps its login in the other when
    /// a sign-in to another account replaces it in this one, so it lost nothing. Once no
    /// slot's record names it, the login replaced was its only one.
    #[test]
    fn a_login_another_slot_still_holds_is_not_the_only_one() {
        let mut s = State::default();
        s.upsert(account("work", None));
        s.upsert(account("personal", None));
        let said = |s: &State, label: &str, login: &str, at: i64| {
            InUse::of(s.get(&claude(label)).unwrap(), login, at)
        };
        let other = |s: &mut State, record: InUse| {
            s.other_slots
                .entry(ProviderId::Claude.code().into())
                .or_default()
                .insert("Claude Code-credentials-other".into(), record);
        };
        let work_there = said(&s, "work", "work-there", 50);
        other(&mut s, work_there);
        let work = said(&s, "work", "work-login", 100);
        s.identified(ProviderId::Claude, work, 100);

        let personal = said(&s, "personal", "personal-login", 200);
        let identified = s.identified(ProviderId::Claude, personal, 200);

        assert_eq!(identified.replaced, None);
        assert_eq!(s.get(&claude("work")).unwrap().replaced_at, None);

        let work = said(&s, "work", "work-login-2", 300);
        s.identified(ProviderId::Claude, work, 300);
        let personal_there = said(&s, "personal", "personal-there", 350);
        other(&mut s, personal_there);
        let personal = said(&s, "personal", "personal-login", 400);
        let identified = s.identified(ProviderId::Claude, personal, 400);

        assert_eq!(
            identified.replaced,
            Some(Replaced {
                key: claude("work")
            })
        );
        assert_eq!(s.get(&claude("work")).unwrap().replaced_at, Some(400));
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
        assert_eq!(discarded(&s), ["p"]);
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

        assert_eq!(discarded(&s), ["ours"], "only the park this home wrote");
        assert!(s.foreign.is_empty(), "and nothing here holds the others");
    }

    /// Installed or renewed, a park has been used up whoever wrote it, and goes the way
    /// every used park goes.
    #[test]
    fn a_park_this_home_did_not_write_is_listed_once_it_is_used() {
        let mut s = State::default();
        s.upsert(account("work", None));
        s.park_foreign(&claude("work"), park("found"));
        s.discard("found", ParkKind::Vault);
        assert!(s.get(&claude("work")).unwrap().parked.is_none());
        assert_eq!(discarded(&s), ["found"]);
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
        let mut document = written;
        migrate(
            &mut document,
            std::path::Path::new("/tmp/state.json"),
            crate::host::OS,
        )
        .expect("current");
        let mut state: State = serde_json::from_value(document).expect("still parses");
        assert!(state.foreign.is_empty());
        state.remove(&Key::new(ProviderId::Codex, "work"));
        assert_eq!(
            discarded(&state),
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
    fn a_parked_login_is_due_from_three_days_before_it_expires() {
        const NOW: i64 = 1_789_935_600;
        assert!(!renewal_due(NOW + RENEW_WITHIN, NOW));
        assert!(renewal_due(NOW + RENEW_WITHIN - 1, NOW));
        assert!(renewal_due(NOW + 1, NOW));
        assert!(!renewal_due(NOW, NOW));
        assert!(!renewal_due(NOW - 60, NOW));
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
            plan: None,
            label: label.into(),
            id: format!("codex-{label}-uuid"),
            account_uuid: format!("codex-{label}-uuid"),
            email: format!("{label}@openai.example"),
            detail: Detail::Codex { workspace_id: None },
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
        assert_eq!(discarded(&s), ["codex-park"]);
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
