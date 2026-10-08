//! Dropping an account and the credentials parked for it.

use super::{Error, Result, Settled, purge};
use crate::provider;
use crate::service::Warning;
use crate::state::{self, Key};

/// Returns the account's email.
pub fn forget(settled: Settled, key: &Key) -> Result<(String, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    // Who is signed in is a fact about the machine. Pitboard's own record of it is stale
    // the moment someone signs in with the tool's own login command, and forgetting the
    // account that is actually in use throws away the only record of it.
    // Asked of the tool's own files, so it answers offline: Claude Code's config, or a
    // Codex login's own claims.
    let live = provider::of(key.provider)
        .recorded_identity(&ctx)
        .map(crate::api::Owner::from);
    let signed_in = match (&live, state.get(key)) {
        (Some(owner), Some(account)) => account.owned_by(owner),
        // No live identity to compare against, so whose login Pitboard last recorded the
        // store holding is all there is.
        _ => state
            .account_in_use(key.provider)
            .is_some_and(|account| account.is(key)),
    };
    if signed_in {
        return Err(Error::CannotForgetActiveAccount { label: key.typed() });
    }
    let enrolled = state.labels(key.provider);
    let account = state.remove(key).ok_or_else(|| Error::AccountUnknown {
        label: key.typed(),
        enrolled,
    })?;
    state::save(&ctx, permit, &state)?;
    crate::fault::point("forget.recorded");
    crate::readings::forget(&ctx, permit, &account.id);
    crate::budget::forget(&ctx, permit, &account.id);
    let pending = purge(&ctx, permit, &mut state);
    Ok((
        account.email,
        (pending > 0)
            .then_some(Warning::ParksPendingRemoval(pending))
            .into_iter()
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::super::harness::{Machine, in_organisation, machine};
    use super::*;
    use crate::service::Permit;

    /// `here` in a second organisation beside the first, with Claude Code's config naming
    /// `here` in `org`, or in no organisation at all.
    fn two_organisations(name: &str, org: Option<&str>) -> Machine {
        let m = machine(name);
        let mut state = state::load(&m.ctx).expect("state");
        state.upsert(in_organisation("team", "here", "org-team", None));
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

    fn forgotten(m: &Machine, label: &str) -> Result<(String, Vec<Warning>)> {
        let settled = super::super::settle(&m.ctx, Permit::for_a_test(), Some(m.which))
            .expect("nothing to recover")
            .0;
        forget(settled, &m.key(label))
    }

    /// The login in use is the organisation Claude Code's config names, not every account
    /// of the person it names.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn the_account_in_use_is_told_by_its_organisation() {
        let m = two_organisations("forget-by-organisation", Some("org-team"));
        assert!(matches!(
            forgotten(&m, "team"),
            Err(Error::CannotForgetActiveAccount { .. })
        ));
        forgotten(&m, "here").expect("the other organisation's login is not in use");
    }

    /// A config naming no organisation does not say which of the person's logins is in
    /// use, so Pitboard's record of whose login is stored decides, as it does with no config.
    #[test]
    #[cfg_attr(
        windows,
        ignore = "W16: Pitboard writing, replacing and removing files on Windows"
    )]
    fn a_config_naming_no_organisation_leaves_it_to_the_record() {
        let m = two_organisations("forget-no-organisation", None);
        assert!(matches!(
            forgotten(&m, "here"),
            Err(Error::CannotForgetActiveAccount { .. })
        ));
        forgotten(&m, "team").expect("not the account recorded in use");
    }
}
