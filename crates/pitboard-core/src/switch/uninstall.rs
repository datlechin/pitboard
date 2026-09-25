//! Taking pitboard off a machine without leaving credentials behind.

use super::{Result, Settled, purge};
use crate::context::Context;
use crate::error::Error;
use crate::state::Account;
use crate::{home, schedule, state};

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
    /// Whether the daily renewal schedule was taken away. `false` where there was none.
    pub schedule_removed: bool,
}

/// Takes away the daily renewal schedule, deletes every parked login this pitboard wrote,
/// then removes pitboard's own directory. Each tool's login is left exactly as it is:
/// whoever is signed in stays signed in.
///
/// The schedule goes first. It runs `pitboard renew` every day, which would make a new
/// directory once this one is gone, and if it cannot be taken away nothing else has been
/// touched yet.
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
    let schedule_removed = remove_schedule(&ctx)?;
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
        schedule_removed,
    })
}

/// The schedule, where it is this home's. One that renews another home is that home's to
/// take away, and a platform with no scheduler has nothing to take.
fn remove_schedule(ctx: &Context) -> Result<bool> {
    if !schedule::serves(ctx) {
        return Ok(false);
    }
    match schedule::uninstall(ctx) {
        Err(Error::ScheduleUnsupported) => Ok(false),
        removed => removed,
    }
}

/// The lock file this run holds lives in here too; on Unix an open file goes on existing
/// until the last handle closes, so removing the directory now is safe.
fn remove_home(ctx: &Context) -> bool {
    std::fs::remove_dir_all(home::dir(ctx)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::super::harness::{codex_machine, machine};
    use super::super::settle;
    use super::*;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn uninstalling_takes_the_renewal_schedule_with_it() {
        for make in [machine, codex_machine] {
            let m = make("uninstall-schedule");
            schedule::install(&m.ctx).expect("scheduled");

            let removed = uninstall(settle(&m.ctx, None).expect("nothing to recover").0)
                .expect("uninstalled");

            assert!(removed.schedule_removed);
            assert!(removed.home_removed);
            assert_eq!(schedule::status(&m.ctx), schedule::Installed::No);
        }
    }

    #[test]
    fn uninstalling_where_nothing_is_scheduled_is_not_a_failure() {
        let m = machine("uninstall-unscheduled");
        let removed =
            uninstall(settle(&m.ctx, None).expect("nothing to recover").0).expect("uninstalled");
        assert!(!removed.schedule_removed);
        assert!(removed.home_removed);
    }

    /// launchd and systemd renew the default home, so uninstalling any other one leaves
    /// the schedule to the pitboard it serves.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn uninstalling_another_home_leaves_the_schedule_alone() {
        let m = machine("uninstall-elsewhere");
        schedule::install(&m.ctx).expect("scheduled");
        let elsewhere = m
            .ctx
            .clone()
            .with_pitboard_home(m.ctx_home().join("elsewhere"));

        let removed = uninstall(settle(&elsewhere, None).expect("nothing to recover").0)
            .expect("uninstalled");

        assert!(!removed.schedule_removed);
        assert!(matches!(
            schedule::status(&m.ctx),
            schedule::Installed::Yes { .. }
        ));
    }
}
