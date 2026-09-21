//! Dropping an account and the credentials parked for it.

use super::{Error, Result, Settled, purge};
use crate::state;

/// Returns the account's email and how many parked items could not be deleted yet.
pub fn forget(settled: Settled, label: &str) -> Result<(String, usize)> {
    let Settled {
        _exclusive,
        mut state,
    } = settled;
    if state.active.as_deref() == Some(label) {
        return Err(Error::CannotForgetActiveAccount {
            label: label.to_string(),
        });
    }
    let account = state.remove(label).ok_or_else(|| Error::AccountUnknown {
        label: label.to_string(),
    })?;
    state::save(&state)?;
    Ok((account.email, purge(&mut state)))
}
