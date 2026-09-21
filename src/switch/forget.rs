//! Dropping an account and the credentials parked for it.

use super::{Error, Result, exclusive};
use crate::{state, store};

pub fn forget(label: &str) -> Result<(String, Vec<String>)> {
    let _exclusive = exclusive()?;
    let mut state = state::load()?;
    let index = state
        .accounts
        .iter()
        .position(|a| a.label == label)
        .ok_or_else(|| Error::AccountUnknown {
            label: label.to_string(),
        })?;
    if state.active.as_deref() == Some(label) {
        return Err(Error::CannotForgetActiveAccount {
            label: label.to_string(),
        });
    }
    let account = state.accounts.remove(index);
    state::save(&state)?;

    let stuck = account
        .generations
        .iter()
        .filter(|g| store::vault_delete(&g.service).is_err())
        .map(|g| g.service.clone())
        .collect();
    Ok((account.email, stuck))
}
