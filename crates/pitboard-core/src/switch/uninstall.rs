//! Taking Pitboard off a machine without leaving credentials behind.

use super::{Result, Settled, purge, tree_journal};
use crate::context::Context;
use crate::error::Error;
use crate::provider::desktop::paths;
use crate::state::Account;
use crate::{home, schedule, state};
use std::path::{Path, PathBuf};

/// What was removed, for the report. It may say more in a later release, so it cannot be
/// built outside this crate.
#[non_exhaustive]
pub struct Removed {
    /// Parked logins deleted from the keychain or the vault.
    pub parks: usize,
    /// Parked logins that could not be deleted, which is why the home was kept.
    pub pending: usize,
    /// Parked logins left where they are because this Pitboard did not write them:
    /// `repair` gave them back from a store every Pitboard on the machine shares, so each
    /// may be another Pitboard's. Always none where the vault is inside Pitboard's own
    /// directory.
    pub left: usize,
    /// Whether ~/.pitboard itself is gone.
    pub home_removed: bool,
    /// Whether the daily renewal schedule was taken away. `false` where there was none.
    pub schedule_removed: bool,
    /// Folders of Claude Desktop data left where they are, which is why the home was kept:
    /// what was set aside from Claude's data folder, and parks nothing names any more.
    /// Pitboard never deletes either.
    pub kept: Vec<PathBuf>,
}

/// Takes away the daily renewal schedule, deletes every parked login this Pitboard wrote,
/// then removes Pitboard's own directory. Each tool's login is left exactly as it is:
/// whoever is signed in stays signed in.
///
/// The schedule goes first. It runs `pitboard renew` every day, which would make a new
/// directory once this one is gone, and if it cannot be taken away nothing else has been
/// touched yet.
///
/// The directory is removed last and only when every parked login is gone, because
/// state.json is the only index of those keychain items. Deleting it first would leave live
/// refresh tokens on the machine with no way left to name them. A login `repair` gave back
/// is not this Pitboard's to delete, so it is left, and does not keep the directory.
///
/// Nothing is touched while a Claude Desktop switch is unfinished: until it is settled, a
/// park it made may be the only copy of the account that was signed in, and nothing but
/// its record names it. What was set aside from Claude's data folder, and a park nothing
/// names, are never deleted, so the folders holding them are left.
pub fn uninstall(settled: Settled) -> Result<Removed> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
    } = settled;
    // Settling for no tool in particular leaves the run for later while Claude is open;
    // here it has to be settled first, or the uninstall goes no further.
    tree_journal::reconcile(&ctx, &mut state, tree_journal::Reconcile::Required)?;
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
    let kept = found_data(&ctx);
    let home_removed = pending == 0 && remove_home(&ctx, &kept);
    Ok(Removed {
        parks: parks.saturating_sub(pending),
        pending,
        left,
        home_removed,
        schedule_removed,
        kept,
    })
}

/// The folders of Claude Desktop data Pitboard never deletes, where there is anything in
/// them. Once every recorded park is deleted, what is left in the parks folder is a park
/// nothing names, such as one an abandoned switch left.
fn found_data(ctx: &Context) -> Vec<PathBuf> {
    [paths::parks_dir(ctx), paths::strays_dir(ctx)]
        .into_iter()
        .filter(|dir| std::fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_some()))
        .collect()
}

/// The schedule, where it is this home's. One that renews another home is that home's to
/// take away, and a machine with no scheduler has nothing to take.
fn remove_schedule(ctx: &Context) -> Result<bool> {
    if !schedule::serves(ctx) {
        return Ok(false);
    }
    match schedule::uninstall(ctx) {
        Err(Error::ScheduleUnsupported) => Ok(false),
        removed => removed,
    }
}

/// The lock file this run holds lives in here too; on macOS and Linux an open file goes on
/// existing until the last handle closes, so removing the directory now is safe.
///
/// Everything but `kept` and the folders above it goes. Whether the home itself is gone.
fn remove_home(ctx: &Context, kept: &[PathBuf]) -> bool {
    remove_all_but(&home::dir(ctx), kept)
}

/// Removes `dir` and everything in it except `kept`, and answers whether `dir` is gone.
/// A link is removed, never followed.
fn remove_all_but(dir: &Path, kept: &[PathBuf]) -> bool {
    if kept.is_empty() {
        return std::fs::remove_dir_all(dir).is_ok();
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    let mut gone = true;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        let removed = if kept.contains(&path) {
            false
        } else if is_dir && kept.iter().any(|k| k.starts_with(&path)) {
            remove_all_but(&path, kept)
        } else if is_dir {
            std::fs::remove_dir_all(&path).is_ok()
        } else {
            std::fs::remove_file(&path).is_ok()
        };
        gone &= removed;
    }
    gone && std::fs::remove_dir(dir).is_ok()
}

#[cfg(test)]
mod tests {
    use super::super::harness::{codex_machine, machine};
    use super::super::settle;
    use super::*;

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

    /// A schedule that cannot be taken away stops the uninstall before anything else is
    /// touched. Left running, it would renew logins whose index is gone.
    #[test]
    fn a_schedule_that_cannot_be_taken_away_leaves_every_login_where_it_was() {
        use crate::host::fs::testing;
        for make in [machine, codex_machine] {
            let m = make("uninstall-stuck");
            schedule::install(&m.ctx).expect("scheduled");
            let dir = schedule::path(&m.ctx)
                .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
                .expect("where the scheduler keeps it");
            testing::deny_changes(&dir);
            let refused = uninstall(settle(&m.ctx, None).expect("nothing to recover").0);
            testing::allow_changes(&dir);

            let Err(error) = refused else {
                panic!("{:?}: uninstalled with the schedule still there", m.which);
            };
            assert_eq!(error.code(), "home_unwritable");
            let state = state::load(&m.ctx).expect("the account list");
            assert_eq!(state.accounts.len(), 2, "{:?}", m.which);
            let there = state.get(&m.key("there")).expect("`there` is enrolled");
            let park = there.parked.as_ref().expect("`there` is still parked");
            crate::park::load(&m.ctx, &m.key("there"), park).expect("its parked login is kept");
            assert!(home::dir(&m.ctx).is_dir());
            assert!(matches!(
                schedule::status(&m.ctx),
                schedule::Installed::Yes { .. }
            ));
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
    /// the schedule to the Pitboard it serves.
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

    /// A Claude Desktop switch killed after it moved the account signed in into a park it
    /// had not yet recorded. While Claude is open recovery cannot finish it, and uninstalling
    /// past it would delete that park with the directory: the only copy of a login.
    #[test]
    fn an_unfinished_desktop_switch_stops_the_uninstall() {
        use super::super::harness::{APP_PATH, desktop_machine};
        let m = desktop_machine("uninstall-unfinished");
        assert_eq!(
            m.crash_at("tree.item_parked").unwrap_err(),
            "tree.item_parked"
        );
        let during = m.inodes();
        m.mem.runs_within(APP_PATH);

        let refused = uninstall(settle(&m.ctx, None).expect("goes ahead, with a warning").0);

        let Err(error) = refused else {
            panic!("uninstalled with a switch unfinished");
        };
        assert_eq!(error.code(), "recovery_waiting");
        assert_eq!(m.inodes(), during, "every item is where it was");
        assert!(home::dir(&m.ctx).join("state.json").is_file());
    }

    /// What Pitboard set aside from Claude's data folder, and a park nothing names any more,
    /// are never deleted: everything else goes, and they stay where they are.
    #[test]
    fn what_was_found_in_claudes_data_folder_outlives_the_uninstall() {
        use super::super::harness::desktop_machine;
        use crate::provider::desktop::paths;
        let m = desktop_machine("uninstall-strays");
        let stray = paths::strays_dir(&m.ctx).join("1-Cookies");
        std::fs::create_dir_all(stray.parent().unwrap()).unwrap();
        std::fs::write(&stray, b"made by Claude").unwrap();
        let orphan = paths::parks_dir(&m.ctx).join("pitboard-tree-orphan-1/Cookies");
        std::fs::create_dir_all(orphan.parent().unwrap()).unwrap();
        std::fs::write(&orphan, b"a login an abandoned switch left").unwrap();
        let kept: Vec<_> = [&stray, &orphan]
            .iter()
            .map(|p| crate::store::tree::inode(p).unwrap())
            .collect();

        let removed =
            uninstall(settle(&m.ctx, None).expect("nothing to recover").0).expect("uninstalled");

        assert_eq!(removed.parks, 1, "`there`'s park, which Pitboard recorded");
        assert!(!paths::parks_dir(&m.ctx).join(m.there_park()).exists());
        assert!(!removed.home_removed);
        assert_eq!(
            removed.kept,
            [paths::parks_dir(&m.ctx), paths::strays_dir(&m.ctx)]
        );
        let now: Vec<_> = [&stray, &orphan]
            .iter()
            .map(|p| crate::store::tree::inode(p).unwrap())
            .collect();
        assert_eq!(now, kept, "the same files, where they were");
        assert!(!home::dir(&m.ctx).join("state.json").exists());
        let left: Vec<_> = std::fs::read_dir(home::dir(&m.ctx))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, ["desktop"], "nothing else of Pitboard's is left");
    }
}
