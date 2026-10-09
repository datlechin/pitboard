//! Whose login each tool has stored, as its service last said of that exact login.
//!
//! Neither Claude Code's config nor the account Pitboard last switched to is that. On
//! 8 October 2026 each named the wrong account for hours: another Claude Code process wrote
//! its own account into the config, and the login in the keychain changed outside Pitboard,
//! which went on naming the account it had last switched to. So the record keeps what the
//! service said, with the fingerprint of the login it said it of, and every reader that does
//! not read the store takes the account in use from it ([`known`]). What the tool's own
//! record names is only a sign that something signed in since.

use crate::api::Owner;
use crate::context::Context;
use crate::provider::{self, ProviderId};
use crate::state::{Key, State, new_id};
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
    pub(crate) fn agrees(&self, which: ProviderId, other: &InUse) -> bool {
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

/// What is known of whose login one tool has stored, from files alone: no keychain, and no
/// request.
#[derive(Debug, Clone, PartialEq)]
pub struct Known {
    /// Whose login the store held, as last established. `None` where nothing is: nothing was
    /// ever recorded, or the login is there and is no one account's.
    pub last: Option<InUse>,
    /// Why the store may hold another login than `last` says, where it may.
    pub doubt: Option<Doubt>,
    /// The account the tool's own record names now ([`names`]), where `last.named` is what
    /// it named when `last` was checked against the store.
    pub own_record: Option<Owner>,
}

/// Why what was last established of a tool's store may not be what it holds now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doubt {
    /// Nobody has asked: there is no record, or one brought forward from schema 5.
    NeverEstablished,
    /// The tool's own record names another account than when the record was checked
    /// against the store, as a sign-in leaves it, and as a process starting on another
    /// login can.
    NamedMoved,
    /// The login could not be read, so the record stands in for it.
    LoginUnreadable,
}

impl Known {
    /// Whose login the store holds, as last established.
    pub fn owner(&self) -> Option<&Owner> {
        self.last.as_ref()?.owner.as_ref()
    }

    /// The account the tool's own record names in place of the one whose login is stored,
    /// where nothing is in doubt. `/status` in Claude Code shows that one.
    pub(crate) fn named_another(&self, which: ProviderId) -> Option<&Owner> {
        let owner = self.owner()?;
        let named = self.own_record.as_ref()?;
        (self.doubt.is_none() && new_id(which, named) != new_id(which, owner)).then_some(named)
    }
}

/// Whose login `which` has stored, as far as files say. For a tool whose login names its own
/// account, the login's own claims, or the record where the login cannot be read. For
/// another, the record, judged against what the tool's own record names now ([`judged`]).
pub(crate) fn known(ctx: &Context, state: &State, which: ProviderId) -> Known {
    let tool = provider::of(which);
    if !tool.identifies_by_itself() {
        return judged(ctx, which, state.in_use(which).cloned());
    }
    let found = |owner: Option<Owner>, login: String| Known {
        last: Some(InUse {
            owner,
            login,
            known_at: ctx.now(),
            named: None,
        }),
        doubt: None,
        own_record: None,
    };
    match tool.read_live(ctx) {
        Err(_) => Known {
            last: state.in_use(which).cloned(),
            doubt: Some(Doubt::LoginUnreadable),
            own_record: None,
        },
        Ok(None) => found(None, String::new()),
        Ok(Some(credential)) => match tool.identify(ctx, &credential) {
            Ok(owner) => found(Some(Owner::from(owner)), tool.fingerprint(&credential.raw)),
            Err(_) => Known {
                last: None,
                doubt: None,
                own_record: None,
            },
        },
    }
}

/// `last`, what was established of `which`'s store, judged against what the tool's own
/// record names now. Where that is another account than when `last` was checked against the
/// store, something may have signed in since.
pub(crate) fn judged(ctx: &Context, which: ProviderId, last: Option<InUse>) -> Known {
    let own_record = names(ctx, which);
    let now = own_record.as_ref().map(|owner| new_id(which, owner));
    let doubt = match &last {
        None => Some(Doubt::NeverEstablished),
        Some(record) if record.known_at == 0 => Some(Doubt::NeverEstablished),
        Some(record) if record.named != now => Some(Doubt::NamedMoved),
        Some(_) => None,
    };
    Known {
        last,
        doubt,
        own_record,
    }
}

/// The account `which`'s own record names now, as [`provider::Provider::own_record`] reads
/// it.
pub(crate) fn names(ctx: &Context, which: ProviderId) -> Option<Owner> {
    provider::of(which).own_record(ctx).map(Owner::from)
}

/// What `which`'s own record names now, by the id [`new_id`] gives that account's login.
///
/// Read before the store wherever both are read: a sign-in between the two reads then leaves
/// a record naming what the tool's own record named before it, which reads as moved since.
pub(crate) fn named(ctx: &Context, which: ProviderId) -> Option<String> {
    names(ctx, which).map(|owner| new_id(which, &owner))
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
