//! Dropping an account and the credentials parked for it.

use super::identify::{self, Live};
use super::{Error, Result, Settled, enrolled, purge};
use crate::provider::{self, LoginKind};
use crate::service::Warning;
use crate::state::{self, Key};

/// Returns the account's email.
///
/// The account whose login the tool has stored is refused: that login is its only one, and
/// forgetting the account throws away the only record of whose it is. Pitboard asks whose
/// the login is as a switch does, so a sign-in outside Pitboard since the last read is found,
/// and never takes it from Claude Code's config, which a Claude Code process started or
/// signed in on another login can rewrite with its own account. Where nobody can say, the
/// account Pitboard last recorded in use is refused with why, and so is any account with
/// nothing parked, which may be the one whose login is stored; one with a parked login the
/// record does not name is not in use.
pub(crate) fn forget(settled: Settled, key: &Key) -> Result<(String, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    let account = enrolled(&state, key)?;
    let mut lost = None;
    // An app's data folder stays held until the account is gone.
    let mut _closed = None;
    let mut said = Vec::new();
    let asked = match provider::of(key.provider) {
        LoginKind::Document(tool) => match identify::now(&ctx, permit, &mut state, tool) {
            Ok(Live::Login(login)) => {
                lost = login.lock_warning(key.provider);
                Ok(account.owned_by(&login.owner))
            }
            Ok(Live::Nothing) => Ok(false),
            Err(unknown) => Err(unknown),
        },
        LoginKind::Folder(app) => {
            let asked = super::folder::in_use_for_forget(&ctx, permit, &mut state, key, app)?;
            _closed = asked.closed;
            said = asked.warnings;
            asked.in_use
        }
    };
    let in_use = match asked {
        Ok(in_use) => in_use,
        Err(unknown) => {
            let recorded = state
                .account_in_use(key.provider)
                .is_some_and(|in_use| in_use.is(key));
            if recorded || account.parked.is_none() {
                return Err(unknown.after(said));
            }
            false
        }
    };
    if in_use {
        return Err(Error::CannotForgetActiveAccount { label: key.typed() }.after(said));
    }
    state.remove(key);
    if let Err(unsaved) = state::save(&ctx, permit, &state) {
        return Err(unsaved.after(said));
    }
    crate::fault::point("forget.recorded");
    crate::usage::readings::forget(&ctx, permit, &account.id);
    crate::usage::budget::forget(&ctx, permit, &account.id);
    crate::duty::warm::forget(&ctx, permit, &account.id);
    let pending = purge(&ctx, permit, &mut state);
    Ok((
        account.email,
        lost.into_iter()
            .chain(said)
            .chain((pending > 0).then_some(Warning::ParksPendingRemoval(pending)))
            .collect(),
    ))
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::super::harness::{
        NOW, Rig, account, config_names, document, in_organisation, machine,
    };
    use super::*;
    use crate::in_use::InUse;
    use crate::service::Permit;

    /// `here` in a second organisation beside the first, with the login stored, `here`'s,
    /// recorded as Anthropic named it for `recorded`, and Claude Code's config naming `here`
    /// in `org`, or in no organisation at all.
    fn two_organisations(name: &str, recorded: &str, org: Option<&str>) -> Rig {
        let m = machine(name);
        let mut state = state::load(&m.ctx).expect("state");
        state.upsert(in_organisation("team", "here", "org-team", None));
        let login = provider::document(m.which).fingerprint(&document("here-refresh"));
        let named = InUse::of(state.get(&m.key(recorded)).expect("enrolled"), &login, NOW);
        state.identified(m.which, named, NOW);
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        let mut account = serde_json::json!({
            "accountUuid": "here",
            "emailAddress": "here@example.com",
        });
        if let Some(org) = org {
            account["organizationUuid"] = org.into();
        }
        std::fs::write(
            m.ctx_home().join(".claude.json"),
            serde_json::json!({ "oauthAccount": account }).to_string(),
        )
        .expect("a Claude Code config");
        m
    }

    fn forgotten(m: &Rig, label: &str) -> Result<(String, Vec<Warning>)> {
        let settled = super::super::settle(&m.ctx, Permit::for_a_test(), Some(m.which))
            .expect("nothing to recover")
            .0;
        forget(settled, &m.key(label))
    }

    /// The login in use is the organisation Anthropic named, not every account of the
    /// person, whatever organisation Claude Code's config names.
    #[test]
    fn the_account_in_use_is_told_by_its_organisation() {
        let m = two_organisations("forget-by-organisation", "team", Some("org-here"));
        assert!(matches!(
            forgotten(&m, "team"),
            Err(Error::CannotForgetActiveAccount { .. })
        ));
        forgotten(&m, "here").expect("the other organisation's login is not in use");
    }

    /// A config naming no organisation decides nothing, as no config does: the account
    /// Anthropic named for the login stored is the one in use.
    #[test]
    fn a_config_naming_no_organisation_decides_nothing() {
        let m = two_organisations("forget-no-organisation", "here", None);
        assert!(matches!(
            forgotten(&m, "here"),
            Err(Error::CannotForgetActiveAccount { .. })
        ));
        forgotten(&m, "team").expect("not the account in use");
    }

    /// Another Claude Code process that starts or signs in on another login can write that
    /// login's account into Claude Code's config. The account whose login is stored, as
    /// Anthropic named it, is still the one in use, and the one the config names is not.
    #[test]
    fn the_account_in_use_cannot_be_forgotten_whatever_the_config_names() {
        let m = machine("forget-drift");
        config_names(&m, "there");
        assert!(matches!(
            forgotten(&m, "here"),
            Err(Error::CannotForgetActiveAccount { .. })
        ));
        forgotten(&m, "there").expect("not the account in use");
    }

    /// Claude Code renewed its login, and Anthropic cannot be reached to say whose the
    /// renewed one is. Nobody can say which account is in use, so neither the account the
    /// record names nor one with nothing parked is forgotten: either may hold the only login
    /// it has. An account with a parked login the record does not name is not in use.
    #[test]
    fn with_nobody_to_say_whose_the_login_is_an_account_with_nothing_parked_is_kept() {
        let m = machine("forget-unidentified");
        let mut state = state::load(&m.ctx).expect("state");
        state.upsert(account("elsewhere", "elsewhere", None));
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        m.sign_in(&document("here-renewed"));
        m.api.token_trouble(
            "access-here-renewed",
            crate::api::scripted::Trouble::Offline,
        );

        for label in ["here", "elsewhere"] {
            let refused = forgotten(&m, label).expect_err(label);
            assert_eq!(refused.code(), "identity_unverifiable", "{label}");
        }
        forgotten(&m, "there").expect("parked, and not the account the record names");
    }
}
