use super::{Check, Facts, account_code, fail, ok, running, warn};
use crate::context::Context;
use crate::error::Error;
use crate::provider::{LoginKind, ProviderId};
use crate::state::{Park, State, renewal_due};
use crate::{park, store, words};

pub(crate) struct ParkFact {
    /// Which tool the account belongs to, which is what decides how it is named back to a
    /// person: bare for Claude Code, `codex/work` for Codex.
    pub(crate) provider: ProviderId,
    /// The name a command on this machine takes for it: qualified where another tool has an
    /// account of the same name, since a bare one would then be ambiguous.
    pub(crate) name: String,
    /// Whether its login is the one its tool has stored, as far as files say.
    pub(crate) active: bool,
    /// When this account last came to be in use, where that is recorded.
    pub(crate) last_used_at: Option<i64>,
    pub(crate) park: Option<Park>,
    /// Why it cannot be read back, if it cannot.
    pub(crate) unreadable: Option<Unreadable>,
}

/// Why a parked login could not be read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unreadable {
    /// It is behind a keychain that is locked here, which says nothing about the login.
    Locked,
    /// A park sealed to this account that will not open in this logon, with
    /// `CryptUnprotectData`'s code where there was one: a lost key, not a missing or damaged
    /// file.
    Sealed(Option<i32>),
    /// Missing, damaged, or refused by the store, in words.
    Broken(String),
}

impl ParkFact {
    /// The account's name as a command here would take it.
    pub(super) fn typed(&self) -> String {
        self.name.clone()
    }
}

pub(super) fn park_facts(ctx: &Context, state: &State) -> Vec<ParkFact> {
    // Whose login each tool has stored, as its service last said, asked once per tool and
    // only of a tool that has accounts here, and from files alone: Pitboard's record, and a
    // Codex login's own claims. Never Claude Code's config, which a Claude Code process
    // started or signed in on another login can rewrite with its own account.
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
                        Unreadable::Broken("missing from the vault".into())
                    }
                    Error::ParkedCredentialCorrupt { detail, .. } => Unreadable::Broken(detail),
                    other => Unreadable::Broken(other.to_string()),
                })
            }),
        })
        .collect()
}

/// A refresh token's own life, from a real renewal answer: thirty days. An account that has
/// been parked for longer than that without being switched to has had its login kept alive
/// purely by Pitboard, through at least one whole token lifetime, for nobody.
pub(super) const A_TOKEN_LIFETIME: i64 = 30 * 86_400;

/// An account nobody has come back to.
///
/// Pitboard renews a parked login for as long as the account is enrolled, so one enrolled
/// once and never used again keeps a live, continuously rotated refresh token on this
/// machine indefinitely. That is a defensible thing to do and an indefensible thing to do
/// silently. Nothing is dropped on a timer Pitboard chose: the threshold here is the
/// token's own lifetime, and all it does is say so.
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
        format!(
            "not switched to for {}; Pitboard has kept its login alive that whole time",
            words::span(dormant_for)
        ),
        format!(
            "Every `pitboard` renews it, so its refresh token is rotated and kept live on \
             this machine for as long as it stays enrolled. If you are not coming back to \
             it, `pitboard forget {}` deletes the login and the record.",
            park.typed()
        ),
    ))
}

// The code `CryptUnprotectData` returns after an administrator's password reset, as RF-D1d
// measured (run 38797729796), where the account's master key no longer opens the blob.
pub(super) const DPAPI_KEY_RESET: i32 = 0x8009_000B_u32 as i32;

// Every sealed park lost at once, as after an administrator resets the password, is one
// finding. A seal that reads 0x8009000B for all of them points at a reset or a restored
// profile; any other code (a logon without the key, as over SSH) gets neutral wording.
pub(super) fn judge_vault(facts: &Facts, lost: &[&ParkFact]) -> Option<Check> {
    if facts.vault != store::Backend::SealedFile {
        return None;
    }
    if lost.is_empty() {
        return Some(ok(
            "vault",
            "parked logins",
            "sealed to this Windows account, not plain text",
        ));
    }
    let again: Vec<String> = lost
        .iter()
        .map(|p| format!("pitboard enroll {} --sign-in", p.typed()))
        .collect();
    let a_reset = lost.iter().all(|p| {
        matches!(
            p.unreadable,
            Some(Unreadable::Sealed(Some(DPAPI_KEY_RESET)))
        )
    });
    let way_out = if a_reset {
        format!(
            "An administrator's reset of your Windows password, or a profile restored on \
             another PC, leaves them sealed with a key this account no longer has. Sign in to \
             each again{}",
            running(&again)
        )
    } else {
        format!(
            "They do not open in this logon. Run `pitboard doctor` from a desktop session \
             before signing in again{}",
            running(&again)
        )
    };
    Some(fail(
        "vault",
        "parked logins",
        format!(
            "none of the {} parked logins opens for this Windows account",
            lost.len()
        ),
        way_out,
    ))
}

// Two sealed parks or more in a sealed vault, each failing to open: a lost key, not a missing
// or damaged file, so a reset or a restored profile is told from a stray unreadable park.
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
    let renew = format!("Run `pitboard enroll {} --sign-in`.", fact.typed());
    let Some(park) = &fact.park else {
        return if fact.active {
            ok(code, name, "signed in; parked when you switch away")
        } else {
            warn(code, name, "nothing parked to switch to", renew)
        };
    };
    match &fact.unreadable {
        Some(Unreadable::Locked) => {
            return warn(
                code,
                name,
                "not read: the keychain is locked",
                "Unlock it, then run `pitboard doctor` again.",
            );
        }
        Some(Unreadable::Sealed(_)) => {
            return fail(
                code,
                name,
                "its parked login does not open in this logon",
                renew,
            );
        }
        Some(Unreadable::Broken(why)) => {
            return fail(
                code,
                name,
                format!("its parked login is unusable: {why}"),
                renew,
            );
        }
        None => {}
    }
    let Some(at) = park.refresh_expires_at else {
        return ok(code, name, "parked");
    };
    // The column `pitboard status` shows, after this check's own words.
    let life = words::parked_life_column(at, now);
    if !park.restorable_at(now) {
        let when = moment(at, now);
        warn(code, name, format!("its parked login {life} {when}"), renew)
    } else if renewal_due(at, now) {
        warn(code, name, format!("its parked login {life}"), renew)
    } else {
        ok(code, name, format!("parked, {life}"))
    }
}
