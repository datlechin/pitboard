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
mod rename;
mod renew;
mod uninstall;

pub use enroll::{Enrolled, SignIn, WatchedSignIn, enroll, sign_in, sign_in_watched};
pub use forget::forget;
pub use journal::{Abandoned, Recovered, pending as interrupted};
pub use rename::rename;
pub use renew::{Renewal, renew_parked};
pub use uninstall::{Removed, uninstall};

use crate::context::Context;
use crate::error::{Error, Result};
use crate::service::Warning;
use crate::state::{Account, Park, State};
use crate::{api, claude, configfile, home, lock, park, state, store, time};
use journal::{Journal, clear_journal, reconcile, write_journal};
use serde_json::Value;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

/// Claude Code serves the credential from a 30 second cache whose clock restarts on every
/// read or write, so a session picks up a swap within about 30 seconds of its last read
/// rather than of its start. Measured over three runs on one machine: swapping at t+8, t+20
/// and t+28 seconds took effect at t+32.3, t+33.5 and t+32.95 from process start.
pub const ADOPTION_CEILING_SECONDS: u32 = 33;

pub enum Outcome {
    Switched {
        from: String,
        to: String,
        parked: Park,
    },
    /// Not a failure: the state the caller asked for already holds.
    AlreadyActive { label: String },
}

fn oauth_of(document: &Value) -> Result<Value> {
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
    ctx: Context,
}

/// Throws away a record of an interrupted switch that cannot be finished, keeping every
/// copy it names. Takes pitboard's own lock but never Claude Code's: it installs nothing.
pub fn abandon(ctx: &Context) -> Result<Option<Abandoned>> {
    if ctx.custom_oauth {
        return Err(Error::CustomOauthEndpoint);
    }
    let _exclusive = exclusive(ctx)?;
    let mut state = state::load(ctx)?;
    journal::abandon(ctx, &mut state)
}

/// What recovery found is returned apart from the `Settled`, so it can be reported whether
/// or not the command that follows succeeds.
pub fn settle(ctx: &Context) -> Result<(Settled, Option<Recovered>)> {
    // Under a custom OAuth endpoint the live login is in "Claude Code-custom-oauth-
    // credentials", not the item pitboard reads. Acting would park nothing and restore
    // into an item nobody reads, so pitboard does not act at all.
    if ctx.custom_oauth {
        return Err(Error::CustomOauthEndpoint);
    }
    let exclusive = exclusive(ctx)?;
    let mut state = state::load(ctx)?;
    let recovered = reconcile(ctx, &mut state)?;
    purge(ctx, &mut state);
    Ok((
        Settled {
            _exclusive: exclusive,
            state,
            ctx: ctx.clone(),
        },
        recovered,
    ))
}

/// Delete what no account refers to any more. A failed save only leaves deleted names
/// listed, and deleting a missing item succeeds, so a later run clears them.
fn purge(ctx: &Context, state: &mut State) -> usize {
    let listed = state.discarded.len();
    let remaining = park::purge(ctx, state);
    if remaining != listed {
        let _ = state::save(ctx, state);
    }
    remaining
}

/// Makes pitboard runs exclusive of each other. A kernel lock, unlike the directory lock
/// Claude Code's protocol requires around its own writes: the operating system releases it
/// when a process ends, so there is no staleness rule for two runs to both satisfy.
fn exclusive(ctx: &Context) -> Result<std::fs::File> {
    let (file, path) = lock_file(ctx)?;
    file.lock()
        .map_err(|source| Error::HomeUnwritable { path, source })?;
    Ok(file)
}

/// `exclusive` without waiting: `None` while another pitboard run holds it.
fn try_exclusive(ctx: &Context) -> Option<std::fs::File> {
    let (file, _) = lock_file(ctx).ok()?;
    file.try_lock().ok()?;
    Some(file)
}

fn lock_file(ctx: &Context) -> Result<(std::fs::File, PathBuf)> {
    let path = home::dir(ctx).join("state.lock");
    let fail = |source| Error::HomeUnwritable {
        path: path.clone(),
        source,
    };
    home::ensure(ctx).map_err(fail)?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(&path)
        .map_err(fail)?;
    Ok((file, path))
}

/// Who a live access token belongs to. When this cannot be answered, nothing moves: a login
/// filed under a guessed account takes two accounts with it.
fn identify(ctx: &Context, access_token: &str) -> Result<api::Owner> {
    api::owner(ctx, access_token).map_err(|e| match e {
        api::ApiError::Unauthorized => Error::SessionExpired,
        other => Error::IdentityUnverifiable {
            detail: other.to_string(),
        },
    })
}

fn access_token(document: &Value) -> Result<String> {
    document["claudeAiOauth"]["accessToken"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::LiveCredentialShapeUnexpected {
            detail: "it has no access token".into(),
        })
}

pub fn switch(settled: Settled, label: &str) -> Result<(Outcome, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
    } = settled;
    let ctx = &ctx;
    let target = state
        .get(label)
        .cloned()
        .ok_or_else(|| Error::AccountUnknown {
            label: label.to_string(),
            enrolled: state.labels(),
        })?;

    // Asked before taking Claude Code's lock so the round trip does not hold up its writes,
    // then confirmed under the lock.
    let service = claude::live_service(ctx);
    let live = store::read(ctx, &service)?.ok_or(Error::LiveCredentialAbsent)?;
    let identified_with = access_token(&live)?;
    let outgoing = identify(ctx, &identified_with)?;

    if outgoing.account_uuid == target.account_uuid {
        if state.active.as_deref() != Some(label) {
            state.active = Some(label.to_string());
            state::save(ctx, &state)?;
        }
        return Ok((
            Outcome::AlreadyActive {
                label: label.to_string(),
            },
            Vec::new(),
        ));
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
    let incoming = park::load(ctx, label, &held)?;

    let storage = PathBuf::from(claude::storage_dir(ctx)).join(".storage-write");
    let guard = lock::acquire(&storage)?;

    let before_raw = store::read_raw(ctx, &service)?.ok_or(Error::LiveCredentialAbsent)?;
    let before: Value =
        serde_json::from_str(&before_raw).map_err(|e| Error::LiveCredentialShapeUnexpected {
            detail: e.to_string(),
        })?;
    // A refresh keeps the account, so an unchanged access token needs no second round trip.
    // A sign-in between the two reads would not keep it.
    let now_token = access_token(&before)?;
    if now_token != identified_with
        && identify(ctx, &now_token)?.account_uuid != outgoing.account_uuid
    {
        return Err(Error::SignedInAccountChanged);
    }

    // Checked before anything is parked, so a switch that could never be written changes
    // nothing.
    let next = splice(&before, &incoming)?;
    // Asked once. The answer is about the backend that would take this write, so a login
    // living in the fallback file is not told it has the keychain's ceiling.
    let price = store::cost(ctx, &service, &next);
    if price.is_some_and(store::Cost::refused) {
        let price = price.expect("refused implies a ceiling");
        return Err(Error::CredentialTooLarge {
            label: label.to_string(),
            bytes: price.needs,
            limit: price.limit,
        });
    }
    // Said once per switch rather than hidden: the same bytes are visible to `ps` for the
    // length of one `security` call, which is the only way to write a login this size.
    let on_the_command_line =
        price
            .filter(|p| p.on_the_second_route())
            .map(|p| Warning::WrittenOnTheCommandLine {
                bytes: p.needs,
                limit: p.limit,
            });

    let park_service = park::reserve(ctx, &outgoing.account_uuid)?;
    write_journal(
        ctx,
        &Journal {
            started_at: time::now(),
            from_label: outgoing_label.clone(),
            from_uuid: outgoing.account_uuid.clone(),
            to_label: label.to_string(),
            to_uuid: target.account_uuid.clone(),
            park_service: park_service.clone(),
            incoming_service: held.service.clone(),
        },
    )?;

    // Until the incoming login is installed there is nothing for a later run to finish, so
    // a failure here takes the record of intent away with it. A copy that was written but
    // could not be recorded is deleted: nothing that survives would name it.
    let parked = match park::store_at(ctx, &park_service, &oauth_of(&before)?) {
        Ok(parked) => parked,
        Err(e) => {
            clear_journal(ctx);
            return Err(e);
        }
    };
    state.park(&outgoing_label, parked.clone());
    if let Err(e) = state::save(ctx, &state) {
        let _ = store::vault_delete(ctx, &parked.service);
        clear_journal(ctx);
        return Err(e);
    }

    if let Err(e) = install(ctx, &service, &next, &before_raw, &outgoing_label, label) {
        if !only_copy_left(&e) {
            state.discard(&parked.service);
        }
        state::save(ctx, &state)?;
        clear_journal(ctx);
        purge(ctx, &mut state);
        return Err(e);
    }
    state.discard(&held.service);
    state.active = Some(label.to_string());
    state::save(ctx, &state)?;
    drop(guard);

    // Claude Code does not correct a stale config on its own; the next switch rewrites it.
    let config_warning = update_config(
        ctx,
        &target,
        &outgoing.account_uuid,
        &outgoing.organization_uuid,
    )
    .err()
    .map(Warning::ConfigNotUpdated);
    let parks_pending = purge(ctx, &mut state);
    clear_journal(ctx);

    let warnings = on_the_command_line
        .into_iter()
        .chain(config_warning)
        .chain((parks_pending > 0).then_some(Warning::ParksPendingRemoval(parks_pending)))
        .collect();
    Ok((
        Outcome::Switched {
            from: outgoing_label,
            to: label.to_string(),
            parked,
        },
        warnings,
    ))
}

/// After a failed install, whether the copy just parked is the outgoing account's only login.
/// Once its old login is back in place, the copy is a second holder that Claude Code will
/// rotate past; if it could not be put back, the copy is all that is left of it.
fn only_copy_left(failure: &Error) -> bool {
    matches!(failure, Error::SwitchCorrupted { .. })
}

/// Keys that belong to the account rather than to the machine. Claude Code deletes all of
/// them along with the login on logout, so leaving one behind would hand the incoming
/// account the outgoing account's device token or its second OAuth block. Measured in
/// 2.1.278: `delete i.claudeAiOauth, delete i.organizationUuid, delete i.trustedDeviceToken,
/// delete i.enterpriseGateway, delete i.designOauth`.
const ACCOUNT_SCOPED: [&str; 4] = [
    "organizationUuid",
    "trustedDeviceToken",
    "enterpriseGateway",
    "designOauth",
];

/// The live document with the incoming login in place of the outgoing one, and nothing of
/// the outgoing account left behind. Claude Code makes these keys again as it needs them,
/// which is the state a logout and a fresh login would leave.
fn splice(before: &Value, incoming: &Value) -> Result<String> {
    let mut next = before.clone();
    let document = next
        .as_object_mut()
        .ok_or_else(|| Error::LiveCredentialShapeUnexpected {
            detail: "it is not a JSON object".into(),
        })?;
    document.insert("claudeAiOauth".into(), incoming.clone());
    for key in ACCOUNT_SCOPED {
        document.remove(key);
    }
    Ok(serde_json::to_string(&next).expect("a credential document stays serialisable"))
}

fn install(
    ctx: &Context,
    service: &str,
    next: &str,
    before_raw: &str,
    from: &str,
    to: &str,
) -> Result<()> {
    install_with(
        |body| store::write_raw(ctx, service, body),
        || store::read_raw(ctx, service),
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
fn update_config(
    ctx: &Context,
    target: &Account,
    outgoing_account: &str,
    outgoing_org: &str,
) -> Result<()> {
    let path = claude::config_file(ctx);
    configfile::backup(ctx, &path)?;
    let mut config = claude::load_config(ctx)?;
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

    /// Claude Code deletes these along with the login on logout, so they belong to the
    /// account. Left behind, the incoming account would present the outgoing account's
    /// device token, and hold its second OAuth block.
    #[test]
    fn a_switch_leaves_nothing_of_the_outgoing_account() {
        let before = serde_json::json!({
            "claudeAiOauth": {"refreshToken": "old"},
            "organizationUuid": "org-a",
            "trustedDeviceToken": "device-of-a",
            "enterpriseGateway": {"url": "https://gateway.example"},
            "designOauth": {"refreshToken": "design-of-a"},
            "somethingOfThisMachine": true,
        });
        let after: Value = serde_json::from_str(
            &splice(&before, &serde_json::json!({"refreshToken": "new"})).expect("spliced"),
        )
        .expect("valid JSON");
        assert_eq!(after["claudeAiOauth"]["refreshToken"], "new");
        assert_eq!(after["somethingOfThisMachine"], true);
        for key in ACCOUNT_SCOPED {
            assert!(after.get(key).is_none(), "{key} was left behind");
        }
    }

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
