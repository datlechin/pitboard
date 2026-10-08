//! Whose login each tool has stored, as its service last said of that exact login.
//!
//! Neither Claude Code's config nor the account Pitboard last switched to is that. On
//! 8 October 2026 each named the wrong account for hours: another Claude Code process wrote
//! its own account into the config, and the login in the keychain changed outside Pitboard,
//! which went on naming the account it had last switched to. So the record keeps what the
//! service said, with the fingerprint of the login it said it of.

use crate::api::Owner;
use crate::context::Context;
use crate::provider::{self, ProviderId};
use crate::state::{Key, new_id};
use serde::{Deserialize, Serialize};

/// Whose login one tool has stored, and which login that was.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct InUse {
    /// Whose login the store held, as the service said. `None`: it held none.
    pub owner: Option<Owner>,
    /// The fingerprint of that login's refresh token, as [`provider::Provider::fingerprint`]
    /// gives it. Empty where the store held none, or where nothing established which login
    /// it was.
    pub login: String,
    /// When whose `login` is was established, in epoch seconds. 0: never, on a record
    /// brought forward from the account last switched to.
    pub known_at: i64,
    /// The account the tool's own record named when this was checked against the store, by
    /// the id [`new_id`] gives its login. `None` for a tool whose login names its own
    /// account, which keeps no record apart from it, and where the record named nobody.
    pub named: Option<String>,
}

impl InUse {
    /// Whether `other` says what this says of `which`'s store: the same account, the same
    /// login and the same account named. When it was said is not part of it.
    pub fn agrees(&self, which: ProviderId, other: &InUse) -> bool {
        self.account(which) == other.account(which)
            && self.login == other.login
            && self.named == other.named
    }

    /// The owner's account, by the id [`new_id`] gives its login, which is how each tool
    /// tells its logins apart. `None` where the store held none.
    pub fn account(&self, which: ProviderId) -> Option<String> {
        self.owner.as_ref().map(|owner| new_id(which, owner))
    }
}

/// What recording whose login a tool has stored came to.
#[derive(Debug, Clone, PartialEq)]
pub struct Identified {
    /// The record changed, and is saved with the state.
    pub changed: bool,
    /// The account whose only login it replaced, where it replaced one.
    pub replaced: Option<Replaced>,
}

/// An account whose only login is gone: its tool's store held that login with nothing parked
/// for the account and no other slot's record naming it, and its service then named another
/// account's login there, or none.
#[derive(Debug, Clone, PartialEq)]
pub struct Replaced {
    pub key: Key,
}

/// What `which`'s own record names now, by the id [`new_id`] gives that account's login.
///
/// Read before the store wherever both are read: a sign-in between the two reads then leaves
/// a record naming what the tool's own record named before it, which reads as moved since.
pub(crate) fn named(ctx: &Context, which: ProviderId) -> Option<String> {
    let tool = provider::of(which);
    if tool.identifies_by_itself() {
        return None;
    }
    tool.recorded_identity(ctx)
        .map(|found| new_id(which, &Owner::from(found)))
}

/// What `which`'s own record names once a switch has written `owner`'s account there.
pub(crate) fn naming(which: ProviderId, owner: &Owner) -> Option<String> {
    (!provider::of(which).identifies_by_itself()).then(|| new_id(which, owner))
}

#[cfg(test)]
impl InUse {
    /// `account`'s login, fingerprinted `login`, as its service said at `at`.
    pub(crate) fn of(account: &crate::state::Account, login: &str, at: i64) -> InUse {
        InUse {
            owner: Some(account.owner()),
            login: login.into(),
            known_at: at,
            named: None,
        }
    }
}
