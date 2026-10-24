//! Taking Pitboard off a machine without leaving credentials behind.

use super::{Result, Settled, purge};
use crate::context::Context;
use crate::error::Error;
use crate::provider::{self, LoginKind, ProviderId};
use crate::schedule::Job;
use crate::service::{Permit, Warning};
use crate::state::Account;
use crate::{home, schedule, state};

/// What was removed, for the report. It may say more in a later release, so it cannot be
/// built outside this crate.
#[derive(Debug)]
#[non_exhaustive]
pub struct Removed {
    // Parked logins deleted: from the keychain or the vault, or a folder under `parks`.
    pub parks: usize,
    /// Parked logins that could not be deleted, and renewed logins not saved where their tool
    /// keeps them yet, which is why the home was kept.
    pub pending: usize,
    /// Parked logins left where they are because this Pitboard did not write them:
    /// `repair` gave them back from a store every Pitboard on the machine shares, so each
    /// may be another Pitboard's. Always none where the vault is inside Pitboard's own
    /// directory.
    pub left: usize,
    // Folders set aside, deleted with every parked login.
    pub set_aside: usize,
    // Whether Pitboard's directory is gone.
    pub home_removed: bool,
    // What another program still held in Pitboard's directory, by path, so it stayed.
    pub home_kept_by: Vec<std::path::PathBuf>,
    // A warm-up was running, so Pitboard's directory stayed for it.
    pub warming_up: bool,
    /// Whether the daily renewal schedule was taken away. `false` where there was none.
    pub schedule_removed: bool,
    /// Whether the watch service was stopped and taken away. `false` where there was none.
    pub watch_service_removed: bool,
    // A move of an app's login that waited, as it was settled first.
    pub warnings: Vec<Warning>,
}

/// Takes away the watch service and the daily renewal schedule, deletes every parked login
/// this Pitboard wrote, then removes Pitboard's own directory. Each tool's login is left
/// exactly as it is: whoever is signed in stays signed in.
///
/// The jobs go first. The watch service writes in the directory as it runs, and the schedule
/// runs `pitboard renew` every day, so either would make a new directory once this one is
/// gone.
///
/// The directory is removed last and only when every parked login is gone, because
/// state.json is the only index of those keychain items. Deleting it first would leave live
/// refresh tokens on the machine with no way left to name them. A login `repair` gave back
/// is not this Pitboard's to delete, so it is left, and does not keep the directory. A
/// renewed login settling could not save yet is the only copy of that login, so it is
/// neither deleted nor left unnamed: it keeps the directory too.
pub(crate) fn uninstall(settled: Settled) -> Result<Removed> {
    let Settled {
        _exclusive: exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    if crate::app::Open::is_held(&ctx) {
        return Err(Error::PitboardOpen {
            home: home::dir(&ctx),
        });
    }
    // An app's folder is read only where a login of it is parked or set aside or a move of one
    // waits, and held until every park is deleted. Refused before anything is taken away.
    let mut warnings = Vec::new();
    let mut _closed = Vec::new();
    for &tool in ProviderId::ALL {
        match provider::of(tool) {
            LoginKind::Document(_) => {}
            LoginKind::Folder(app) => {
                if let Some((closed, said)) =
                    super::folder::before_uninstall(&ctx, permit, &mut state, app)?
                {
                    _closed.push(closed);
                    warnings.extend(said);
                }
            }
        }
    }
    match take_away(&ctx, permit, &mut state, exclusive) {
        Ok(removed) => Ok(Removed {
            warnings,
            ..removed
        }),
        Err(refused) => Err(refused.after(warnings)),
    }
}

// The rest of `uninstall`, once each app's folder is held and a move of it settled.
fn take_away(
    ctx: &Context,
    permit: Permit,
    state: &mut state::State,
    exclusive: std::fs::File,
) -> Result<Removed> {
    let watch_service_removed = remove_job(ctx, permit, Job::Watch)?;
    let schedule_removed = remove_job(ctx, permit, Job::Renew)?;
    let held = state.accounts.iter().filter_map(|a| a.parked.as_ref());
    let left = held
        .clone()
        .filter(|park| state.is_foreign(&park.service))
        .count();
    let parks = held.count() - left;
    let asides: Vec<String> = state.aside.iter().map(|aside| aside.name.clone()).collect();
    for key in state.accounts.iter().map(Account::key).collect::<Vec<_>>() {
        state.remove(&key);
    }
    state.forget_asides();
    state.forget_discarded();
    state.forget_in_use();
    state::save(ctx, permit, state)?;
    let undeleted = purge(ctx, permit, state);
    let asides_left = asides
        .iter()
        .filter(|name| {
            state
                .discarded
                .iter()
                .any(|listed| listed.service == **name)
        })
        .count();
    let pending = undeleted + state.renewing.len();
    // The sweep in settle has already resolved every outstanding name, so what is left
    // refers to nothing. The home goes next, and an index of names with no home is noise.
    if pending == 0 {
        crate::pending::clear(ctx, permit);
        discard_signins(ctx, permit);
    }
    let home = (pending == 0).then(|| remove_home(ctx, permit, exclusive));
    Ok(Removed {
        parks: parks.saturating_sub(undeleted - asides_left),
        pending,
        left,
        set_aside: asides.len() - asides_left,
        home_removed: matches!(home, Some(Home::Removed)),
        warming_up: matches!(home, Some(Home::WarmingUp)),
        home_kept_by: match home {
            Some(Home::Kept(by)) => by,
            _ => Vec::new(),
        },
        schedule_removed,
        watch_service_removed,
        warnings: Vec::new(),
    })
}

enum Home {
    Removed,
    WarmingUp,
    Kept(Vec<std::path::PathBuf>),
}

// A sign-in killed before its cleanup left its login in its tool's slot for the sign-in's
// folder, which a keychain or Credential Manager keeps once the folder is gone. One under way
// keeps its own.
fn discard_signins(ctx: &Context, permit: Permit) {
    let Ok(_one_at_a_time) = super::enroll::one_at_a_time(ctx, permit) else {
        return;
    };
    super::enroll::discard_signins(ctx, permit, &super::enroll::signin_dir(ctx));
}

/// `job`, where it is this home's. One that serves another home is that home's to take away,
/// and a machine with no scheduler has nothing to take.
fn remove_job(ctx: &Context, permit: Permit, job: Job) -> Result<bool> {
    if !schedule::serves(ctx) {
        return Ok(false);
    }
    let removed = match job {
        Job::Renew => schedule::uninstall(ctx, permit),
        Job::Watch => crate::watch_service::taken_away(ctx, permit),
    };
    match removed {
        Err(Error::ScheduleUnsupported { .. }) => Ok(false),
        removed => removed,
    }
}

// A warm-up runs its tool in a home inside this one for as long as it holds `warm.lock`. This
// run's own `state.lock` is let go of first: on exFAT and FAT32 a file held open keeps its
// folder (RF-C6x).
fn remove_home(ctx: &Context, permit: Permit, exclusive: std::fs::File) -> Home {
    if super::warm::RunLock::is_held(ctx) {
        return Home::WarmingUp;
    }
    drop(exclusive);
    match home::remove_all(permit, &home::dir(ctx)) {
        Ok(()) => Home::Removed,
        Err(kept) => Home::Kept(kept.left),
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::super::harness::{codex_machine, machine};
    use super::super::settle;
    use super::*;

    #[test]
    fn uninstalling_takes_the_renewal_schedule_with_it() {
        for make in [machine, codex_machine] {
            let m = make("uninstall-schedule").in_the_default_home();
            schedule::install(&m.ctx, Permit::for_a_test()).expect("scheduled");

            let removed = uninstall(
                settle(&m.ctx, Permit::for_a_test(), None)
                    .expect("nothing to recover")
                    .0,
            )
            .expect("uninstalled");

            assert!(removed.schedule_removed);
            assert!(removed.home_removed);
            assert_eq!(schedule::status(&m.ctx), schedule::Installed::No);
        }
    }

    // Left running, the watch service would write a new Pitboard directory as it goes.
    #[test]
    fn uninstalling_takes_the_watch_service_with_it() {
        for make in [machine, codex_machine] {
            let m = make("uninstall-watch").in_the_default_home();
            crate::watch_service::install(
                &m.ctx,
                Permit::for_a_test(),
                crate::duty::autoswitch::Threshold::DEFAULT,
            )
            .expect("installed");

            let removed = uninstall(
                settle(&m.ctx, Permit::for_a_test(), None)
                    .expect("nothing to recover")
                    .0,
            )
            .expect("uninstalled");

            assert!(removed.watch_service_removed);
            assert!(!removed.schedule_removed, "none was there");
            assert!(removed.home_removed);
            assert_eq!(
                crate::watch_service::status(&m.ctx),
                crate::watch_service::Installed::No
            );
        }
    }

    /// A schedule that cannot be taken away stops the uninstall before anything else is
    /// touched. Left running, it would renew logins whose index is gone.
    #[test]
    fn a_schedule_that_cannot_be_taken_away_leaves_every_login_where_it_was() {
        use crate::host::fs::testing;
        for make in [machine, codex_machine] {
            let m = make("uninstall-stuck").in_the_default_home();
            schedule::install(&m.ctx, Permit::for_a_test()).expect("scheduled");
            let dir = schedule::path(&m.ctx)
                .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
                .expect("where the scheduler keeps it");
            testing::deny_changes(&dir).expect("read-only");
            let refused = uninstall(
                settle(&m.ctx, Permit::for_a_test(), None)
                    .expect("nothing to recover")
                    .0,
            );
            testing::allow_changes(&dir).expect("changeable again");

            let Err(error) = refused else {
                panic!("{:?}: uninstalled with the schedule still there", m.which);
            };
            assert_eq!(error.code(), "home_unwritable");
            let state = state::load(&m.ctx).expect("the account list");
            assert_eq!(state.accounts.len(), 2, "{:?}", m.which);
            let there = state.get(&m.key("there")).expect("`there` is enrolled");
            let park = there.parked.as_ref().expect("`there` is still parked");
            crate::park::load(&m.ctx, provider::document(m.which), &m.key("there"), park)
                .expect("its parked login is kept");
            assert!(home::dir(&m.ctx).is_dir());
            assert!(matches!(
                schedule::status(&m.ctx),
                schedule::Installed::Yes { .. }
            ));
        }
    }

    // On the session's PC (2026-10-18) the Windows app made %LOCALAPPDATA%\Pitboard again,
    // with state.json, usage.json and asking.json, once `pitboard uninstall` had taken it away.
    #[test]
    fn uninstalling_refuses_while_the_app_is_open() {
        for make in [machine, codex_machine] {
            let m = make("uninstall-app-open").in_the_default_home();
            schedule::install(&m.ctx, Permit::for_a_test()).expect("scheduled");
            let open = crate::app::Open::hold(&m.ctx, Permit::for_a_test()).expect("held");
            let refused = uninstall(
                settle(&m.ctx, Permit::for_a_test(), None)
                    .expect("nothing to recover")
                    .0,
            );
            let Err(error) = refused else {
                panic!("{:?}: uninstalled with the app open", m.which);
            };
            assert_eq!(error.code(), "pitboard_open");
            assert_eq!(state::load(&m.ctx).expect("kept").accounts.len(), 2);
            assert!(matches!(
                schedule::status(&m.ctx),
                schedule::Installed::Yes { .. }
            ));

            drop(open);
            let removed = uninstall(
                settle(&m.ctx, Permit::for_a_test(), None)
                    .expect("nothing to recover")
                    .0,
            )
            .expect("uninstalled once the app quit");
            assert!(removed.home_removed);
        }
    }

    #[test]
    fn uninstalling_takes_a_killed_sign_ins_login_with_it_and_none_in_use() {
        let m = machine("uninstall-signin-left");
        let dir = super::super::enroll::signin_dir(&m.ctx);
        let left = provider::claude::slot::service_for_dir(&dir.to_string_lossy());
        m.mem.live().plant(&left, r#"{"claudeAiOauth":{}}"#);
        let in_use = m.mem.live().peek(&m.service);

        let removed = uninstall(
            settle(&m.ctx, Permit::for_a_test(), None)
                .expect("nothing to recover")
                .0,
        )
        .expect("uninstalled");

        assert!(removed.home_removed);
        assert_eq!(m.mem.live().peek(&left), None);
        assert!(in_use.is_some());
        assert_eq!(m.mem.live().peek(&m.service), in_use);
    }

    #[test]
    fn uninstalling_where_nothing_is_scheduled_is_not_a_failure() {
        let m = machine("uninstall-unscheduled");
        let removed = uninstall(
            settle(&m.ctx, Permit::for_a_test(), None)
                .expect("nothing to recover")
                .0,
        )
        .expect("uninstalled");
        assert!(!removed.schedule_removed);
        assert!(removed.home_removed);
    }

    // A warm-up started by the app or `pitboard watch` holds `warm.lock` while its tool runs in
    // a home inside Pitboard's directory.
    #[test]
    fn uninstalling_while_a_warm_up_runs_keeps_the_directory_for_it() {
        use crate::helper::{Helper, Job};
        let m = machine("uninstall-warming");
        let warm = home::dir(&m.ctx).join("warm.lock");
        let running = Helper::start(Job::Locks, &warm);
        let uninstalled = || {
            uninstall(
                settle(&m.ctx, Permit::for_a_test(), None)
                    .expect("nothing to recover")
                    .0,
            )
            .expect("uninstalled")
        };

        let removed = uninstalled();
        assert!(removed.warming_up);
        assert!(!removed.home_removed);
        assert_eq!(removed.home_kept_by, Vec::<std::path::PathBuf>::new());
        assert!(warm.exists());
        assert_eq!(state::load(&m.ctx).expect("state").accounts.len(), 0);

        running.stop();
        let removed = uninstalled();
        assert!(!removed.warming_up);
        assert!(removed.home_removed);
        assert!(!home::dir(&m.ctx).exists());
    }

    // On exFAT and FAT32 a file held open keeps its folder (RF-C6x): uninstalling lets go of
    // its own `state.lock` first, and names a file another Pitboard still holds.
    #[cfg(windows)]
    #[test]
    fn uninstalling_on_exfat_lets_go_of_its_lock_and_names_what_another_holds() {
        use crate::helper::{Helper, Job, exfat};
        let m = machine("uninstall-exfat");
        let on_exfat = m
            .ctx
            .clone()
            .with_pitboard_home(exfat("pitboard-uninstall"));
        let uninstalled = || {
            uninstall(
                settle(&on_exfat, Permit::for_a_test(), None)
                    .expect("nothing to recover")
                    .0,
            )
            .expect("uninstalled")
        };
        let removed = uninstalled();
        assert!(removed.home_removed, "{:?}", removed.home_kept_by);

        home::ensure(&on_exfat, Permit::for_a_test()).expect("a home");
        let usage = home::dir(&on_exfat).join("usage.lock");
        let reading = Helper::start(Job::Locks, &usage);
        let removed = uninstalled();
        assert!(!removed.home_removed);
        assert_eq!(removed.home_kept_by, vec![usage.clone()]);
        reading.stop();
        let removed = uninstalled();
        assert!(removed.home_removed);
        assert!(!home::dir(&on_exfat).exists());
    }

    /// A renewed login waiting in the vault to be saved where Claude Code keeps it is the
    /// only copy of that login, and state.json the only index naming it. Uninstalling while
    /// it cannot be saved keeps both, and uninstalling again once it can removes the home.
    #[test]
    fn uninstalling_keeps_a_renewed_login_not_saved_yet() {
        use super::super::harness::{lapsed, owner, renews};
        use crate::store::memory::Fault;
        let m = machine("uninstall-renewing");
        m.sign_in(&lapsed("here-renewed"));
        renews(&m, "here-renewed", "here-again");
        m.api.owned_by("access-here-again", owner("here"));
        let pitboard = crate::service::Pitboard::new(m.ctx.clone());
        let died = crate::fault::killing("refresh.exchanged", || pitboard.switch_to("there"));
        assert_eq!(died.err().as_deref(), Some("refresh.exchanged"));
        let copies = |m: &super::super::harness::Rig| -> Vec<String> {
            m.mem
                .vault()
                .services()
                .into_iter()
                .filter(|service| service.contains("-renewing-"))
                .collect()
        };
        let [copy] = copies(&m).try_into().expect("one copy");
        m.fault_live(Fault::Locked);

        let removed = uninstall(
            settle(&m.ctx, Permit::for_a_test(), None)
                .expect("nothing to recover")
                .0,
        )
        .expect("uninstalled");

        assert!(!removed.home_removed);
        assert_eq!(removed.pending, 1);
        assert!(m.mem.vault().peek(&copy).is_some());
        assert!(state::load(&m.ctx).expect("state").names(&copy));

        m.mem.live().heal_all();
        let removed = uninstall(
            settle(&m.ctx, Permit::for_a_test(), None)
                .expect("nothing to recover")
                .0,
        )
        .expect("uninstalled");
        assert!(removed.home_removed);
        assert_eq!(copies(&m), Vec::<String>::new());
        assert_eq!(
            m.live()
                .and_then(|stored| stored["claudeAiOauth"]["refreshToken"]
                    .as_str()
                    .map(str::to_owned)),
            Some("here-again".into())
        );
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "Claude Desktop is macOS only: Windows makes and removes no folder"
    )]
    fn uninstalling_counts_the_set_aside_folders_it_deleted_and_keeps_the_rest_listed() {
        use crate::host::fs::testing;
        use crate::provider::ProviderId;
        let m = machine("uninstall-asides");
        let parks = crate::store::folder::parks_dir(&m.ctx);
        let names = ["pitboard-aside-1", "pitboard-aside-2"];
        let mut state = state::load(&m.ctx).expect("state");
        for name in names {
            std::fs::create_dir_all(parks.join(name).join("items")).expect("a folder");
            std::fs::write(parks.join(name).join("items/Cookies"), b"a jar").expect("a jar");
            state.aside.push(state::Aside {
                name: name.into(),
                tool: ProviderId::Desktop,
                at: 1,
                why: state::SetAside::InTheWay,
                account: None,
                login: None,
            });
        }
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        let uninstalled = || {
            uninstall(
                settle(&m.ctx, Permit::for_a_test(), None)
                    .expect("nothing to recover")
                    .0,
            )
            .expect("uninstalled")
        };

        // Open to others: nothing under it is deleted, and the vault's park still is.
        testing::allow_changes(&parks).expect("open to others");
        let removed = uninstalled();
        assert_eq!(
            (removed.parks, removed.set_aside, removed.pending),
            (1, 0, 2)
        );
        assert!(!removed.home_removed);
        for name in names {
            assert!(parks.join(name).join("items/Cookies").exists(), "{name}");
            assert!(state::load(&m.ctx).expect("state").names(name), "{name}");
        }

        testing::make_private(&parks).expect("private");
        let removed = uninstalled();
        assert_eq!(removed.pending, 0);
        assert!(removed.home_removed);
    }

    /// launchd, systemd and Task Scheduler renew the default home, so uninstalling any other
    /// one leaves the schedule to the Pitboard it serves.
    #[test]
    fn uninstalling_another_home_leaves_the_schedule_alone() {
        let m = machine("uninstall-elsewhere").in_the_default_home();
        schedule::install(&m.ctx, Permit::for_a_test()).expect("scheduled");
        let elsewhere = m
            .ctx
            .clone()
            .with_pitboard_home(m.ctx_home().join("elsewhere"));

        let removed = uninstall(
            settle(&elsewhere, Permit::for_a_test(), None)
                .expect("nothing to recover")
                .0,
        )
        .expect("uninstalled");

        assert!(!removed.schedule_removed);
        assert!(matches!(
            schedule::status(&m.ctx),
            schedule::Installed::Yes { .. }
        ));
    }
}
