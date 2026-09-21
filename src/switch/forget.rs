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
    if state.active.as_deref() == Some(label) {
        return Err(Error::CannotForgetActiveAccount {
            label: label.to_string(),
        });
    }
    let account = state.remove(label).ok_or_else(|| Error::AccountUnknown {
        label: label.to_string(),
    })?;
    state::save(&ctx, &state)?;
    let pending = purge(&ctx, &mut state);
    Ok((
        account.email,
        (pending > 0)
            .then_some(Warning::ParksPendingRemoval(pending))
            .into_iter()
            .collect(),
    ))
}
