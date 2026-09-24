//! Taking pitboard off a machine without leaving credentials behind.

use super::{Result, Settled, purge};
use crate::context::Context;
use crate::state::Account;
use crate::{home, state};

/// What was removed, for the report.
pub struct Removed {
    /// Parked logins deleted from the keychain or the vault.
    pub parks: usize,
    /// Parked logins that could not be deleted, which is why the home was kept.
    pub pending: usize,
    /// Parked logins left where they are because this pitboard did not write them:
    /// `repair` gave them back from a store every pitboard on the machine shares, so each
    /// may be another pitboard's. Always none where the vault is inside pitboard's own
    /// directory.
    pub left: usize,
    /// Whether ~/.pitboard itself is gone.
    pub home_removed: bool,
}

/// Deletes every parked login this pitboard wrote, then pitboard's own directory. Claude
/// Code's login is left exactly as it is: whoever is signed in stays signed in.
///
/// The directory is removed last and only when every parked login is gone, because
/// state.json is the only index of those keychain items. Deleting it first would leave live
/// refresh tokens on the machine with no way left to name them. A login `repair` gave back
/// is not this pitboard's to delete, so it is left, and does not keep the directory.
pub fn uninstall(settled: Settled) -> Result<Removed> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
    } = settled;
    let held = state.accounts.iter().filter_map(|a| a.parked.as_ref());
    let left = held
        .clone()
        .filter(|park| state.is_foreign(&park.service))
        .count();
    let parks = held.count() - left;
    for key in state.accounts.iter().map(Account::key).collect::<Vec<_>>() {
        state.remove(&key);
    }
    state.active.clear();
    state::save(&ctx, &state)?;
    let pending = purge(&ctx, &mut state);
    // The sweep in settle has already resolved every outstanding name, so what is left
    // refers to nothing. The home goes next, and an index of names with no home is noise.
    if pending == 0 {
        crate::pending::clear(&ctx);
    }
    let home_removed = pending == 0 && remove_home(&ctx);
    Ok(Removed {
        parks: parks.saturating_sub(pending),
        pending,
        left,
        home_removed,
    })
}

/// The lock file this run holds lives in here too; on Unix an open file goes on existing
/// until the last handle closes, so removing the directory now is safe.
fn remove_home(ctx: &Context) -> bool {
    std::fs::remove_dir_all(home::dir(ctx)).is_ok()
}
