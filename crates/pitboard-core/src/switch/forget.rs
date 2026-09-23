//! Dropping an account and the credentials parked for it.

use super::{Error, Result, Settled, purge};
use crate::provider::claude::paths as claude;
use crate::service::Warning;
use crate::state::{self, Key};

/// Returns the account's email.
pub fn forget(settled: Settled, key: &Key) -> Result<(String, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
    } = settled;
    // Who is signed in is a fact about the machine. pitboard's record of its last switch
    // is stale the moment someone signs in with Claude Code's own `/login`, and forgetting
    // the account that is actually in use throws away the only record of it.
    let live_uuid = claude::load_config(&ctx)
        .ok()
        .as_ref()
        .and_then(claude::identity)
        .map(|id| id.account_uuid);
    let signed_in = match (&live_uuid, state.get(key)) {
        (Some(uuid), Some(account)) => &account.account_uuid == uuid,
        // No live identity to compare against, so pitboard's own record of the last switch
        // is all there is. Scoped to the provider the label belongs to: a Claude account
        // being signed in says nothing about a Codex one.
        _ => state.active_for(key.provider) == Some(key.label.as_str()),
    };
    if signed_in {
        return Err(Error::CannotForgetActiveAccount { label: key.typed() });
    }
    let enrolled = state.labels(key.provider);
    let account = state.remove(key).ok_or_else(|| Error::AccountUnknown {
        label: key.typed(),
        enrolled,
    })?;
    state::save(&ctx, &state)?;
    crate::fault::point("forget.recorded");
    crate::readings::forget(&ctx, &account.account_uuid);
    crate::budget::forget(&ctx, &account.account_uuid);
    crate::history::forget(&ctx, &account.account_uuid);
    let pending = purge(&ctx, &mut state);
    Ok((
        account.email,
        (pending > 0)
            .then_some(Warning::ParksPendingRemoval(pending))
            .into_iter()
            .collect(),
    ))
}
