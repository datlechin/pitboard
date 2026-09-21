//! Moving the signed-in identity from one enrolled account to another.
//!
//! Two rules set the order of every step. Which account the outgoing login belongs to is
//! asked of Anthropic, never read from Claude Code's config, which can lag the login by a
//! day: a login filed under the wrong account takes both accounts with it. And additive
//! writes become durable before destructive ones, so a run that dies midway leaves a spare
//! copy, never a missing one.

mod enroll;
mod forget;
mod journal;

pub use enroll::{Enrolled, enroll};
pub use forget::forget;
pub use journal::{Recovered, pending as interrupted};

use crate::error::{Error, Result};
use crate::state::{Account, Park, State};
use crate::{api, claude, configfile, home, lock, park, state, store, time};
use journal::{Journal, clear_journal, reconcile, write_journal};
use serde_json::Value;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

/// Claude Code re-reads the credential store behind a cache anchored at each process's
/// first read. Measured over three runs on one machine: swapping at t+8, t+20 and t+28
/// seconds all took effect at t+32.3, t+33.5 and t+32.95 from process start.
pub const ADOPTION_CEILING_SECONDS: u32 = 33;

pub enum Outcome {
    Switched {
        from: String,
        to: String,
        parked: Park,
        /// The login moved but the config still names the previous account. Claude Code
        /// does not correct that on its own; the next switch rewrites it.
        config_warning: Option<Error>,
        /// Parked items no longer in use that could not be deleted yet.
        parks_pending: usize,
    },
    /// Not a failure: the state the caller asked for already holds.
    AlreadyActive { label: String },
}

pub(super) fn oauth_of(document: &Value) -> Result<Value> {
    document
        .get("claudeAiOauth")
        .cloned()
        .ok_or_else(|| Error::LiveCredentialShapeUnexpected {
            detail: "it has no claudeAiOauth block".into(),
        })
}

/// pitboard's state, held exclusively, with any interrupted switch already finished. Every
/// command that changes state starts from one, so none acts on what a crash left behind.
pub struct Settled {
    _exclusive: std::fs::File,
    state: State,
}

impl Settled {
    pub fn account(&self, label: &str) -> Option<&Account> {
        self.state.get(label)
    }
}

/// What recovery found is returned apart from the `Settled`, so it can be reported whether
/// or not the command that follows succeeds.
pub fn settle() -> Result<(Settled, Option<Recovered>)> {
    let exclusive = exclusive()?;
    let mut state = state::load()?;
    let recovered = reconcile(&mut state)?;
    purge(&mut state);
    Ok((
        Settled {
            _exclusive: exclusive,
            state,
        },
        recovered,
    ))
}

/// Delete what no account refers to any more. A failed save only leaves deleted names
/// listed, and deleting a missing item succeeds, so a later run clears them.
fn purge(state: &mut State) -> usize {
    let listed = state.discarded.len();
    let remaining = park::purge(state);
    if remaining != listed {
        let _ = state::save(state);
    }
    remaining
}

/// Makes pitboard runs exclusive of each other. A kernel lock, unlike the directory lock
/// Claude Code's protocol requires around its own writes: the operating system releases it
/// when a process ends, so there is no staleness rule for two runs to both satisfy.
fn exclusive() -> Result<std::fs::File> {
    let path = home::dir().join("state.lock");
    let fail = |source| Error::HomeUnwritable {
        path: path.clone(),
        source,
    };
    home::ensure().map_err(fail)?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(&path)
        .map_err(fail)?;
    file.lock().map_err(fail)?;
    Ok(file)
}

/// Who a live access token belongs to. When this cannot be answered, nothing moves: a login
/// filed under a guessed account takes two accounts with it.
pub(super) fn identify(access_token: &str) -> Result<api::Owner> {
    api::owner(access_token).map_err(|e| match e {
        api::ApiError::Unauthorized => Error::SessionExpired,
        other => Error::IdentityUnverifiable {
            detail: other.to_string(),
        },
    })
}

pub(super) fn access_token(document: &Value) -> Result<String> {
    document["claudeAiOauth"]["accessToken"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::LiveCredentialShapeUnexpected {
            detail: "it has no access token".into(),
        })
}

pub fn switch(settled: Settled, label: &str) -> Result<Outcome> {
    let Settled {
        _exclusive,
        mut state,
    } = settled;

    let target = state
        .get(label)
        .cloned()
        .ok_or_else(|| Error::AccountUnknown {
            label: label.to_string(),
        })?;

    // Asked before taking Claude Code's lock so the round trip does not hold up its writes,
    // then confirmed under the lock.
    let service = claude::live_service();
    let live = store::read(&service)?.ok_or(Error::LiveCredentialAbsent)?;
    let identified_with = access_token(&live)?;
    let outgoing = identify(&identified_with)?;

    if outgoing.account_uuid == target.account_uuid {
        if state.active.as_deref() != Some(label) {
            state.active = Some(label.to_string());
            state::save(&state)?;
        }
        return Ok(Outcome::AlreadyActive {
            label: label.to_string(),
        });
    }
    let outgoing_label = state
        .by_uuid(&outgoing.account_uuid)
        .map(|a| a.label.clone())
        .ok_or_else(|| Error::LiveAccountNotEnrolled {
            email: outgoing.email.clone(),
        })?;
    let held = target.parked.clone().ok_or_else(|| Error::NothingParked {
        label: label.to_string(),
    })?;
    if !held.restorable_at(time::now()) {
        return Err(Error::ParkedLoginExpired {
            label: label.to_string(),
        });
    }
    let incoming = park::load(label, &held)?;

    let storage = PathBuf::from(claude::storage_dir()).join(".storage-write");
    let guard = lock::acquire(&storage)?;

    let before_raw = store::read_raw(&service)?.ok_or(Error::LiveCredentialAbsent)?;
    let before: Value =
        serde_json::from_str(&before_raw).map_err(|e| Error::LiveCredentialShapeUnexpected {
            detail: e.to_string(),
        })?;
    // A refresh keeps the account, so an unchanged access token needs no second round trip.
    // A sign-in between the two reads would not keep it.
    let now_token = access_token(&before)?;
    if now_token != identified_with && identify(&now_token)?.account_uuid != outgoing.account_uuid {
        return Err(Error::SignedInAccountChanged);
    }

    // Checked before anything is parked, so a switch that could never be written changes
    // nothing.
    let next = splice(&before, &incoming)?;
    if store::too_large(&service, &next) {
        return Err(Error::LiveCredentialShapeUnexpected {
            detail: "the login to install is past the keychain's size limit".into(),
        });
    }

    let park_service = park::reserve(&outgoing.account_uuid)?;
    write_journal(&Journal {
        started_at: time::now(),
        from_label: outgoing_label.clone(),
        from_uuid: outgoing.account_uuid.clone(),
        to_label: label.to_string(),
        to_uuid: target.account_uuid.clone(),
        park_service: park_service.clone(),
        incoming_service: held.service.clone(),
    })?;

    let parked = park::store_at(&park_service, &oauth_of(&before)?)?;
    state.park(&outgoing_label, parked.clone());
    state::save(&state)?;

    if let Err(e) = install(&service, &next, &before_raw, &outgoing_label, label) {
        if !only_copy_left(&e) {
            state.discard(&parked.service);
        }
        state::save(&state)?;
        clear_journal();
        purge(&mut state);
        return Err(e);
    }
    state.discard(&held.service);
    state.active = Some(label.to_string());
    state::save(&state)?;
    drop(guard);

    let config_warning =
        update_config(&target, &outgoing.account_uuid, &outgoing.organization_uuid).err();
    let parks_pending = purge(&mut state);
    clear_journal();

    Ok(Outcome::Switched {
        from: outgoing_label,
        to: label.to_string(),
        parked,
        config_warning,
        parks_pending,
    })
}

/// After a failed install, whether the copy just parked is the outgoing account's only login.
/// Once its old login is back in place, the copy is a second holder that Claude Code will
/// rotate past; if it could not be put back, the copy is all that is left of it.
fn only_copy_left(failure: &Error) -> bool {
    matches!(failure, Error::SwitchCorrupted { .. })
}

/// The live document with `claudeAiOauth` replaced. Every other key belongs to this machine.
fn splice(before: &Value, incoming: &Value) -> Result<String> {
    let mut next = before.clone();
    next.as_object_mut()
        .ok_or_else(|| Error::LiveCredentialShapeUnexpected {
            detail: "it is not a JSON object".into(),
        })?
        .insert("claudeAiOauth".into(), incoming.clone());
    Ok(serde_json::to_string(&next).expect("a credential document stays serialisable"))
}

fn install(service: &str, next: &str, before_raw: &str, from: &str, to: &str) -> Result<()> {
    install_with(
        |body| store::write_raw(service, body),
        || store::read_raw(service),
        next,
        before_raw,
        from,
        to,
    )
}

/// Write the new login, and if that fails, leave the old one in place. A failed write
/// often changes nothing, so the slot is read back before deciding a rollback is needed,
/// and only a rollback that also fails is reported as a lost login.
fn install_with(
    write: impl Fn(&str) -> std::result::Result<(), store::Error>,
    read: impl Fn() -> std::result::Result<Option<String>, store::Error>,
    next: &str,
    before_raw: &str,
    from: &str,
    to: &str,
) -> Result<()> {
    let Err(failure) = write(next) else {
        return Ok(());
    };
    let rolled_back = |detail: String| Error::SwitchRolledBack {
        from: from.to_string(),
        to: to.to_string(),
        detail,
    };
    if matches!(read(), Ok(Some(now)) if now == before_raw) {
        return Err(rolled_back(failure.to_string()));
    }
    match write(before_raw) {
        Ok(()) => Err(rolled_back(failure.to_string())),
        Err(rollback) => Err(Error::SwitchCorrupted {
            from: from.to_string(),
            to: to.to_string(),
            detail: format!("{failure}; {rollback}"),
        }),
    }
}

/// Record the new identity in Claude Code's config. Runs after the login is in place, so
/// the config never names an account before its login is live.
fn update_config(target: &Account, outgoing_account: &str, outgoing_org: &str) -> Result<()> {
    let path = configfile::path();
    configfile::backup(&path)?;
    let mut config = claude::load_config()?;
    configfile::splice_identity(
        &mut config,
        &target.oauth_account,
        &[outgoing_account, outgoing_org],
    );
    configfile::write(&path, &config)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::RefCell;

    fn failing(message: &str) -> store::Error {
        store::Error::Write(message.into())
    }

    #[test]
    fn a_successful_write_needs_no_rollback() {
        let written = RefCell::new(Vec::new());
        let result = install_with(
            |b| {
                written.borrow_mut().push(b.to_string());
                Ok(())
            },
            || unreachable!(),
            "new",
            "old",
            "a",
            "b",
        );
        assert!(result.is_ok());
        assert_eq!(*written.borrow(), vec!["new"]);
    }

    #[test]
    fn a_failed_write_that_changed_nothing_is_not_reported_as_a_lost_login() {
        let result = install_with(
            |_| Err(failing("keychain locked")),
            || Ok(Some("old".into())),
            "new",
            "old",
            "a",
            "b",
        );
        assert!(
            matches!(result, Err(Error::SwitchRolledBack { .. })),
            "the old login never left, so the user must not be told to sign in again"
        );
    }

    #[test]
    fn a_half_write_is_rolled_back() {
        let slot = RefCell::new("old".to_string());
        let result = install_with(
            |b| {
                if b == "new" {
                    *slot.borrow_mut() = "garbled".into();
                    Err(failing("interrupted"))
                } else {
                    *slot.borrow_mut() = b.to_string();
                    Ok(())
                }
            },
            || Ok(Some(slot.borrow().clone())),
            "new",
            "old",
            "a",
            "b",
        );
        assert!(matches!(result, Err(Error::SwitchRolledBack { .. })));
        assert_eq!(
            *slot.borrow(),
            "old",
            "the previous login must be back in place"
        );
    }

    #[test]
    fn only_a_failed_rollback_after_a_change_is_reported_as_corruption() {
        let result = install_with(
            |_| Err(failing("disk full")),
            || Ok(Some("garbled".into())),
            "new",
            "old",
            "a",
            "b",
        );
        assert!(matches!(result, Err(Error::SwitchCorrupted { .. })));
    }

    #[test]
    fn a_copy_is_kept_after_a_failed_install_only_when_it_is_all_that_is_left() {
        let (from, to, detail) = ("a".to_string(), "b".to_string(), String::new());
        assert!(!only_copy_left(&Error::SwitchRolledBack {
            from: from.clone(),
            to: to.clone(),
            detail: detail.clone(),
        }));
        assert!(only_copy_left(&Error::SwitchCorrupted { from, to, detail }));
    }

    #[test]
    fn a_credential_without_claude_ai_oauth_is_refused() {
        assert!(oauth_of(&serde_json::json!({"slackTag": {}})).is_err());
        assert!(oauth_of(&serde_json::json!({"claudeAiOauth": {"accessToken": "a"}})).is_ok());
    }
}
