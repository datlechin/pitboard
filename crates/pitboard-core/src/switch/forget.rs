//! Dropping an account and the credentials parked for it.

use super::{Error, Result, Settled, purge};
use crate::service::Warning;
use crate::state;

/// Returns the account's email.
pub fn forget(settled: Settled, label: &str) -> Result<(String, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
    } = settled;
    // Who is signed in is a fact about the machine. pitboard's record of its last switch
    // is stale the moment someone signs in with Claude Code's own `/login`, and forgetting
    // the account that is actually in use throws away the only record of it.
    let live_uuid = crate::claude::load_config(&ctx)
        .ok()
        .as_ref()
        .and_then(crate::claude::identity)
        .map(|id| id.account_uuid);
    let signed_in = match (&live_uuid, state.get(label)) {
        (Some(uuid), Some(account)) => &account.account_uuid == uuid,
        _ => state.active.as_deref() == Some(label),
    };
    if signed_in {
        return Err(Error::CannotForgetActiveAccount {
            label: label.to_string(),
        });
    }
    let enrolled = state.labels();
    let account = state.remove(label).ok_or_else(|| Error::AccountUnknown {
        label: label.to_string(),
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
