use super::{Check, Facts, account_code, fail, ok, warn, words};
use crate::context::Context;
use crate::error::Error;
use crate::provider::{LoginKind, ProviderId};
use crate::state::{Park, State, renewal_due};
use crate::words as said;
use crate::{park, store};

pub(crate) struct ParkFact {
    pub(crate) provider: ProviderId,
    // As a command here takes it: qualified where another tool has an account of the name.
    pub(crate) name: String,
    // Whether its login is the one its tool has stored, as far as files say.
    pub(crate) active: bool,
    pub(crate) last_used_at: Option<i64>,
    pub(crate) park: Option<Park>,
    pub(crate) unreadable: Option<Unreadable>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unreadable {
    // Behind a locked keychain, which says nothing about the login.
    Locked,
    // A lost key, with `CryptUnprotectData`'s code: not a missing or damaged file.
    Sealed(Option<i32>),
    Broken(String),
}

impl ParkFact {
    pub(super) fn typed(&self) -> String {
        self.name.clone()
    }
}

pub(super) fn park_facts(ctx: &Context, state: &State) -> Vec<ParkFact> {
    // From files alone, never Claude Code's config, which another session can rewrite.
    let in_use: std::collections::BTreeMap<ProviderId, Option<crate::provider::Owner>> =
        ProviderId::ALL
            .iter()
            .filter(|&&which| state.accounts.iter().any(|a| a.provider() == which))
            .map(|&which| {
                let known = crate::in_use::known(ctx, state, which);
                (which, known.owner().cloned())
            })
            .collect();
    state
        .accounts
        .iter()
        .map(|a| ParkFact {
            provider: a.provider(),
            name: state.typed(&a.key()),
            last_used_at: a.last_used_at,
            active: in_use
                .get(&a.provider())
                .and_then(Option::as_ref)
                .is_some_and(|owner| a.owned_by(owner)),
            park: a.parked.clone(),
            unreadable: a.parked.as_ref().and_then(|p| {
                let tool = match crate::provider::of(a.provider()) {
                    LoginKind::Document(tool) => tool,
                    // Its own check reads the index and the folder: `desktop_parked_login`.
                    LoginKind::Folder(_) => return None,
                };
                park::load(ctx, tool, &a.key(), p).err().map(|e| match e {
                    Error::Store(store::Error::Locked) => Unreadable::Locked,
                    Error::Store(store::Error::Sealed { code, .. }) => Unreadable::Sealed(code),
                    Error::ParkedCredentialMissing { .. } => {
                        Unreadable::Broken(words::MISSING_FROM_THE_VAULT.into())
                    }
                    Error::ParkedCredentialCorrupt { detail, .. } => Unreadable::Broken(detail),
                    other => Unreadable::Broken(other.to_string()),
                })
            }),
        })
        .collect()
}

// A refresh token's own life, from a real renewal answer.
pub(super) const A_TOKEN_LIFETIME: i64 = 30 * 86_400;

// Pitboard keeps a parked login alive for as long as its account is enrolled, so it says so.
pub(super) fn judge_dormant(park: &ParkFact, now: i64) -> Option<Check> {
    // The sentence below says Pitboard renews the login, which it does only for a document.
    match crate::provider::of(park.provider) {
        LoginKind::Document(_) => {}
        LoginKind::Folder(_) => return None,
    }
    if park.active || park.park.is_none() {
        return None;
    }
    let dormant_for = now - park.last_used_at?;
    if dormant_for < A_TOKEN_LIFETIME {
        return None;
    }
    Some(warn(
        account_code(
            park.provider,
            "dormant_account",
            "codex_dormant_account",
            "desktop_dormant_account",
        ),
        format!("account {}", park.typed()),
        words::not_switched_to_for(dormant_for),
        words::forget_if_not_coming_back(&park.typed()),
    ))
}

// `CryptUnprotectData`'s code after an administrator resets the password
// (docs/facts/windows-runners.md).
pub(super) const DPAPI_KEY_RESET: i32 = 0x8009_000B_u32 as i32;

// Every sealed park lost at once is one finding, worded for a reset only where the code says so.
pub(super) fn judge_vault(facts: &Facts, lost: &[&ParkFact]) -> Option<Check> {
    if facts.vault != store::Backend::SealedFile {
        return None;
    }
    if lost.is_empty() {
        return Some(ok("vault", "parked logins", words::SEALED_TO_THIS_ACCOUNT));
    }
    let again: Vec<String> = lost
        .iter()
        .map(|p| words::enroll_signing_in(&p.typed()))
        .collect();
    let a_reset = lost.iter().all(|p| {
        matches!(
            p.unreadable,
            Some(Unreadable::Sealed(Some(DPAPI_KEY_RESET)))
        )
    });
    let way_out = if a_reset {
        words::sealed_by_a_lost_key(&again)
    } else {
        words::not_open_in_this_logon(&again)
    };
    Some(fail(
        "vault",
        "parked logins",
        words::none_opens(lost.len()),
        way_out,
    ))
}

// Two or more, each sealed and failing to open: a lost key, not a stray unreadable park.
pub(super) fn lost_parks(facts: &Facts) -> Vec<&ParkFact> {
    let parked: Vec<&ParkFact> = facts
        .parks
        .iter()
        .filter(|p| {
            p.park
                .as_ref()
                .is_some_and(|park| park.kind == crate::state::ParkKind::Vault)
        })
        .collect();
    let all = parked.len() >= 2
        && parked
            .iter()
            .all(|p| matches!(p.unreadable, Some(Unreadable::Sealed(_))));
    if facts.vault == store::Backend::SealedFile && all {
        parked
    } else {
        Vec::new()
    }
}

pub(super) fn judge_park(fact: &ParkFact, now: i64, moment: &dyn Fn(i64, i64) -> String) -> Check {
    let code = account_code(
        fact.provider,
        "parked_login",
        "codex_parked_login",
        "desktop_parked_login",
    );
    let name = format!("account {}", fact.typed());
    let renew = words::sign_in_again(&fact.typed());
    let Some(park) = &fact.park else {
        return if fact.active {
            ok(code, name, words::SIGNED_IN_PARKED_ON_A_SWITCH)
        } else {
            warn(code, name, words::NOTHING_PARKED, renew)
        };
    };
    match &fact.unreadable {
        Some(Unreadable::Locked) => {
            return warn(
                code,
                name,
                words::NOT_READ_THE_KEYCHAIN_IS_LOCKED,
                words::UNLOCK_IT_THEN_DOCTOR,
            );
        }
        Some(Unreadable::Sealed(_)) => {
            return fail(code, name, words::PARK_DOES_NOT_OPEN, renew);
        }
        Some(Unreadable::Broken(why)) => {
            return fail(code, name, words::park_unusable(why), renew);
        }
        None => {}
    }
    let Some(at) = park.refresh_expires_at else {
        return ok(code, name, words::PARKED);
    };
    // The column `pitboard status` shows, after this check's own words.
    let life = said::parked_life_column(at, now);
    if !park.restorable_at(now) {
        let when = moment(at, now);
        warn(code, name, words::park_life_since(&life, &when), renew)
    } else if renewal_due(at, now) {
        warn(code, name, words::park_life(&life), renew)
    } else {
        ok(code, name, words::parked_with(&life))
    }
}
