//! Changing the label an account is enrolled under.

use super::{Result, Settled};
use crate::state;

/// Returns the account's email.
pub fn rename(settled: Settled, from: &str, to: &str) -> Result<String> {
    let Settled {
        _exclusive,
        mut state,
    } = settled;
    let email = state.relabel(from, to)?.email.clone();
    state::save(&state)?;
    Ok(email)
}
