//! Where a home keeps its parked logins of Claude Code and Codex, as `pitboard vault` says it,
//! and a move of them between the forms its system offers, as Linux offers the Secret Service
//! and files.
//!
//! A move is recorded before anything is copied (`vault.moving_to`). Every park this home's
//! records name is written into the new form and read back, the save that names the new form
//! lists what leaves the old one (`vault.leaving`), and each is then deleted from there and taken
//! off the list. Every change settles first, so a move cut short at any step is finished forward
//! by the next change before anything else writes a park, and renewal waits for it. A park
//! another home keeps in a vault every home shares is that home's, and stays where it is.

use super::Settled;
use crate::context::Context;
use crate::error::{Error, Result};
use crate::host::Vault;
use crate::permit::Permit;
use crate::state::{self, Leaving, ParkKind, State, VaultForm};
use crate::warning::Warning;
use crate::{fault, pending, store};
use std::path::PathBuf;

/// Where a home keeps its parked logins of Claude Code and Codex, from its files and from what
/// answers on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keeping {
    /// `None` where the vault this home opens is none a person looks in.
    pub place: Option<Vault>,
    /// Whether every Pitboard directory of the Mac, or of the session on Linux, parks there.
    pub shared: bool,
    /// The folder of files this home keeps them in, or may move them to.
    pub folder: Option<PathBuf>,
    /// Whether a Secret Service answers on this session's bus, where this system parks in one.
    pub secret_service: Option<bool>,
    /// The form a move cut short goes to, which the next change finishes.
    pub moving_to: Option<VaultForm>,
    /// How many parks a move left in the form it moved from, which the next change deletes.
    pub leaving: Option<(VaultForm, usize)>,
}

/// What a move of the parks did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    /// The form the parks are kept in now.
    pub to: VaultForm,
    /// Whether they were kept another way before.
    pub changed: bool,
    /// The parked logins this home holds there now, from the form they were in.
    pub moved: usize,
    /// Those still in the form they were in, which the next change deletes from there.
    pub leaving: usize,
}

pub(crate) fn keeping(ctx: &Context) -> Result<Keeping> {
    let state = state::load(ctx)?;
    let host = ctx.host();
    let form = store::home_form(ctx)?;
    let place = store::vault_place(ctx);
    let in_files = matches!(place, Some(Vault::Files | Vault::SealedFiles))
        || host.vault_forms().contains(&VaultForm::Files);
    let recorded = state.vault.as_ref();
    Ok(Keeping {
        place,
        shared: host.vault_is_shared(form),
        folder: in_files.then(|| store::vault_dir(ctx)),
        secret_service: host
            .vault_forms()
            .contains(&VaultForm::SecretService)
            .then(|| host.answers(ctx, VaultForm::SecretService)),
        moving_to: recorded.and_then(|vault| vault.moving_to),
        leaving: recorded
            .and_then(|vault| vault.leaving.as_ref())
            .map(|leaving| (leaving.from, leaving.parks.len())),
    })
}

/// Moves every park this home holds into `to`. Refused where the system keeps parks one way, and
/// before anything is recorded where either form does not answer, as a locked login keyring.
pub(crate) fn move_to(settled: Settled, to: VaultForm) -> Result<(Moved, Vec<Warning>)> {
    let Settled {
        _exclusive,
        mut state,
        ctx,
        permit,
    } = settled;
    let host = ctx.host();
    if !host.vault_forms().contains(&to) {
        return Err(Error::VaultFormUnsupported { form: to });
    }
    let from = state
        .vault
        .as_ref()
        .map(|vault| vault.form)
        .or_else(|| host.new_home_form(&ctx))
        .unwrap_or(to);
    if from == to {
        return Ok(left(&ctx, permit, &mut state, to, false, 0));
    }
    // A move whose names were still in two forms would delete the parks it had just written.
    take_out(&ctx, permit, &mut state)?;
    let names = held(&ctx, &state);
    host.vault(&ctx, Some(from)).list()?;
    let there = host.vault(&ctx, Some(to)).list()?.unwrap_or_default();
    if host.vault_is_shared(Some(to)) {
        for name in names.iter().filter(|name| there.contains(name)) {
            state.written_elsewhere(name);
        }
    }
    state.vault = Some(state::Vault {
        form: from,
        moving_to: Some(to),
        leaving: None,
    });
    state::save(&ctx, permit, &state)?;
    fault::point("vault.recorded");
    let moved = forward(&ctx, permit, &mut state)?;
    Ok(left(&ctx, permit, &mut state, to, true, moved))
}

// What a move comes to once what it left where it was is deleted, as far as that goes now.
fn left(
    ctx: &Context,
    permit: Permit,
    state: &mut State,
    to: VaultForm,
    changed: bool,
    moved: usize,
) -> (Moved, Vec<Warning>) {
    let _ = take_out(ctx, permit, state);
    let leaving = leaving(state);
    let warnings = (leaving > 0)
        .then_some(Warning::ParksPendingRemoval(leaving))
        .into_iter()
        .collect();
    let moved = Moved {
        to,
        changed,
        moved,
        leaving,
    };
    (moved, warnings)
}

/// What settling does with a move of the parks: a move cut short is finished, and a change the
/// store refuses it for is refused with it; what a move left where it was is deleted, and tried
/// again at the next change where that fails.
pub(crate) fn finish(ctx: &Context, permit: Permit, state: &mut State) -> Result<()> {
    if state
        .vault
        .as_ref()
        .is_some_and(|vault| vault.moving_to.is_some())
    {
        forward(ctx, permit, state)?;
    }
    let _ = take_out(ctx, permit, state);
    Ok(())
}

/// How many parks a move left in the form it moved from.
pub(crate) fn leaving(state: &State) -> usize {
    state
        .vault
        .as_ref()
        .and_then(|vault| vault.leaving.as_ref())
        .map_or(0, |leaving| leaving.parks.len())
}

// Every name copied into the form a move goes to, then the save that names that form, and so
// how many parks this home holds there now. Each write reads its park back.
fn forward(ctx: &Context, permit: Permit, state: &mut State) -> Result<usize> {
    let Some(state::Vault {
        form: from,
        moving_to: Some(to),
        ..
    }) = state.vault.clone()
    else {
        return Ok(0);
    };
    let host = ctx.host();
    let (out, into) = (host.vault(ctx, Some(from)), host.vault(ctx, Some(to)));
    let names = held(ctx, state);
    let mut moved = 0;
    for name in &names {
        // There already, for the home that wrote it (`move_to`).
        if host.vault_is_shared(Some(to)) && state.is_foreign(name) {
            moved += 1;
            continue;
        }
        let Some(login) = out.read(name)? else {
            continue;
        };
        into.write(permit, name, &login)?;
        moved += 1;
        fault::point("vault.copied");
    }
    let mut parks: Vec<String> = names
        .iter()
        .filter(|name| !(host.vault_is_shared(Some(from)) && state.is_foreign(name)))
        .cloned()
        .collect();
    parks.extend(
        state
            .discarded
            .iter()
            .filter(|dropped| dropped.kind == ParkKind::Vault)
            .map(|dropped| dropped.service.clone()),
    );
    parks.sort();
    parks.dedup();
    state
        .discarded
        .retain(|dropped| dropped.kind != ParkKind::Vault);
    if !host.vault_is_shared(Some(to)) {
        for name in &names {
            state.used_here(name);
        }
    }
    state.vault = Some(state::Vault {
        form: to,
        moving_to: None,
        leaving: (!parks.is_empty()).then_some(Leaving { from, parks }),
    });
    state::save(ctx, permit, state)?;
    fault::point("vault.named");
    Ok(moved)
}

// Each park a move left deleted from the form it left, and listed no more. One that resists stays
// listed, and the first failure is said.
fn take_out(ctx: &Context, permit: Permit, state: &mut State) -> Result<()> {
    let Some(Leaving { from, parks }) =
        state.vault.as_ref().and_then(|vault| vault.leaving.clone())
    else {
        return Ok(());
    };
    let out = ctx.host().vault(ctx, Some(from));
    let mut failed = None;
    let mut kept = Vec::new();
    for name in &parks {
        match out.delete(permit, name) {
            Ok(()) => fault::point("vault.deleted"),
            Err(why) => {
                failed.get_or_insert(why);
                kept.push(name.clone());
            }
        }
    }
    if kept.len() < parks.len()
        && let Some(vault) = state.vault.as_mut()
    {
        vault.leaving = (!kept.is_empty()).then_some(Leaving { from, parks: kept });
        state::save(ctx, permit, state)?;
    }
    failed.map_or(Ok(()), |why| Err(why.into()))
}

// Every vault name this home's records name: each account's park, each renewal's copy, and each
// name written down before a login was written into it.
fn held(ctx: &Context, state: &State) -> Vec<String> {
    let mut names: Vec<String> = state
        .accounts
        .iter()
        .filter_map(|account| account.parked.as_ref())
        .filter(|park| park.kind == ParkKind::Vault)
        .map(|park| park.service.clone())
        .chain(
            state
                .renewing
                .iter()
                .map(|renewing| renewing.service.clone()),
        )
        .chain(pending::vault_names(ctx))
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test reads the state file a home saved"
)]
mod tests {
    use super::super::harness::{
        self, NOW, Rig, account, machine_on, oauth, owner, parked_services, two_parked_on,
    };
    use super::super::{Due, renew_due, renew_parked, repair, settle, switch};
    use super::*;
    use crate::context::ScratchMachine;
    use crate::host::memory::MemoryHost;
    use crate::provider;
    use crate::state::Key;
    use crate::store::memory::Fault;
    use std::sync::Arc;

    fn moved(m: &Rig, to: VaultForm) -> Result<(Moved, Vec<Warning>)> {
        move_to(settle(&m.ctx, Permit::for_a_test(), None)?.0, to)
    }

    fn recorded(m: &Rig) -> Option<state::Vault> {
        state::load(&m.ctx).expect("state").vault
    }

    fn in_files(m: &Rig) -> Vec<String> {
        m.mem.files_vault(&m.ctx).services()
    }

    fn in_the_secret_service(m: &Rig) -> Vec<String> {
        m.mem.vault().services()
    }

    fn state_file(m: &Rig) -> Vec<u8> {
        std::fs::read(crate::home::dir(&m.ctx).join("state.json")).expect("saved")
    }

    // Every park this home holds is in `to`, none is left where it was, and nothing waits.
    fn all_in(m: &Rig, to: VaultForm, parks: &[String], after: &str) {
        assert_eq!(recorded(m), Some(state::Vault::kept_as(to)), "{after}");
        let (there, left) = match to {
            VaultForm::SecretService => (in_the_secret_service(m), in_files(m)),
            VaultForm::Files => (in_files(m), in_the_secret_service(m)),
        };
        assert_eq!(there, parks, "{after}");
        assert!(left.is_empty(), "{after}: {left:?}");
        harness::hold(m, after);
    }

    fn kept_on(host: Arc<MemoryHost>) -> (Keeping, PathBuf) {
        let scratch = tempfile::tempdir().expect("a scratch home");
        let root = scratch.path().to_path_buf();
        let machine = ScratchMachine::on(host, &root, |ctx| {
            ctx.with_pitboard_home(root.join(".pitboard"))
        });
        let kept = keeping(&machine.ctx).expect("read");
        (kept, root.join(".pitboard/vault"))
    }

    // As macOS keeps them, as Windows does, and as Linux does each way, a Secret Service there
    // or not.
    #[test]
    fn the_vault_says_where_a_home_keeps_its_parks_and_who_else_parks_there() {
        let (kept, _) = kept_on(MemoryHost::new());
        assert_eq!(
            kept,
            Keeping {
                place: Some(Vault::Keychain),
                shared: true,
                folder: None,
                secret_service: None,
                moving_to: None,
                leaving: None,
            }
        );

        let (kept, folder) = kept_on(MemoryHost::sealed());
        assert_eq!(
            kept,
            Keeping {
                place: Some(Vault::SealedFiles),
                shared: false,
                folder: Some(folder),
                secret_service: None,
                moving_to: None,
                leaving: None,
            }
        );

        let (kept, folder) = kept_on(MemoryHost::by_form());
        assert_eq!(
            kept,
            Keeping {
                place: Some(Vault::SecretService),
                shared: true,
                folder: Some(folder),
                secret_service: Some(true),
                moving_to: None,
                leaving: None,
            }
        );

        let host = MemoryHost::by_form();
        host.secret_service_answers(false);
        let (kept, folder) = kept_on(host);
        assert_eq!(
            kept,
            Keeping {
                place: Some(Vault::Files),
                shared: false,
                folder: Some(folder),
                secret_service: Some(false),
                moving_to: None,
                leaving: None,
            }
        );
    }

    // Into files and back, with a switch after each: the parks are read where the record says.
    #[test]
    fn a_move_to_files_leaves_every_park_in_files_and_none_in_the_secret_service() {
        let m = two_parked_on(MemoryHost::by_form(), "move-to-files");
        let parks = parked_services(&m);
        all_in(&m, VaultForm::SecretService, &parks, "before");

        let (done, said) = moved(&m, VaultForm::Files).expect("moved");
        assert_eq!(
            done,
            Moved {
                to: VaultForm::Files,
                changed: true,
                moved: 2,
                leaving: 0,
            }
        );
        assert!(said.is_empty(), "{said:?}");
        all_in(&m, VaultForm::Files, &parks, "in files");

        let (done, _) = moved(&m, VaultForm::SecretService).expect("moved back");
        assert_eq!((done.changed, done.moved), (true, 2));
        all_in(&m, VaultForm::SecretService, &parks, "back");

        let (done, _) = moved(&m, VaultForm::SecretService).expect("there already");
        assert_eq!((done.changed, done.moved), (false, 0));
        let settled = settle(&m.ctx, Permit::for_a_test(), None)
            .expect("settled")
            .0;
        switch(settled, &m.key("there")).expect("switched");
        assert_eq!(in_the_secret_service(&m), parked_services(&m));
        assert!(in_files(&m).is_empty());
    }

    #[test]
    fn a_move_to_the_secret_service_leaves_every_park_there_and_none_in_files() {
        let host = MemoryHost::by_form();
        host.secret_service_answers(false);
        let m = two_parked_on(Arc::clone(&host), "move-to-the-secret-service");
        host.secret_service_answers(true);
        let parks = parked_services(&m);
        all_in(&m, VaultForm::Files, &parks, "before");

        let (done, _) = moved(&m, VaultForm::SecretService).expect("moved");
        assert_eq!((done.changed, done.moved, done.leaving), (true, 2, 0));
        all_in(
            &m,
            VaultForm::SecretService,
            &parks,
            "in the Secret Service",
        );
    }

    // `there` of another home on the session's bus, and the park of `here` that home wrote, which
    // `repair` gave back here: neither leaves the Secret Service, and that home reads both.
    #[test]
    fn a_move_leaves_another_homes_parks_where_they_are() {
        let host = MemoryHost::by_form();
        let m = machine_on(Arc::clone(&host), "move-another-home");
        let other = m
            .ctx
            .clone()
            .with_pitboard_home(m.ctx_home().join(".pitboard-elsewhere"));
        let tool = provider::document(m.which);
        let mut theirs = State::default();
        for (label, refresh) in [("there", "there-elsewhere"), ("here", "here-elsewhere")] {
            m.api.owned_by(&format!("access-{refresh}"), owner(label));
            let service =
                crate::park::reserve(&other, Permit::for_a_test(), label).expect("a free name");
            let park = crate::park::store_at(
                &other,
                Permit::for_a_test(),
                tool,
                &service,
                &oauth(refresh, 30),
            )
            .expect("parked");
            theirs.accounts.push(account(label, label, Some(park)));
        }
        state::save(&other, Permit::for_a_test(), &theirs).expect("saved");
        let given = repair(
            settle(&m.ctx, Permit::for_a_test(), None)
                .expect("settled")
                .0,
        )
        .expect("repaired")
        .0
        .given_back;
        assert_eq!(given.len(), 1, "{given:?}");
        let ours = parked_services(&m);
        let elsewhere: Vec<String> = theirs
            .accounts
            .iter()
            .filter_map(|a| a.parked.as_ref())
            .map(|park| park.service.clone())
            .collect();
        let read_there = || {
            for account in &theirs.accounts {
                let park = account.parked.as_ref().expect("parked");
                crate::park::load(&other, tool, &Key::new(m.which, &account.label), park)
                    .expect("that home reads its own park");
            }
        };

        moved(&m, VaultForm::Files).expect("moved");

        assert_eq!(in_files(&m), ours);
        let mut session = elsewhere.clone();
        session.sort();
        assert_eq!(in_the_secret_service(&m), session);
        assert!(state::load(&m.ctx).expect("state").foreign.is_empty());
        read_there();
        harness::hold(&m, "in files");

        moved(&m, VaultForm::SecretService).expect("moved back");

        assert!(in_files(&m).is_empty(), "{:?}", in_files(&m));
        let mut session: Vec<String> = elsewhere.iter().chain(&ours).cloned().collect();
        session.sort();
        session.dedup();
        assert_eq!(in_the_secret_service(&m), session);
        assert_eq!(
            state::load(&m.ctx).expect("state").foreign,
            vec![given[0].1.clone()],
            "the park that home wrote is that home's to delete"
        );
        read_there();
    }

    // macOS keeps parks in the keychain alone and Windows in sealed files alone.
    #[test]
    fn a_move_is_refused_where_the_system_keeps_one_form() {
        let said = match crate::host::OS {
            crate::host::Os::MacOs => {
                "On macOS, Pitboard keeps parked logins only in the login keychain"
            }
            crate::host::Os::Windows => {
                "On Windows, Pitboard keeps parked logins only in files sealed to your Windows \
                 account"
            }
            crate::host::Os::Linux => "Pitboard keeps parked logins one way on this computer",
        };
        for (host, name) in [
            (MemoryHost::new(), "move-one-way-keychain"),
            (MemoryHost::sealed(), "move-one-way-sealed"),
        ] {
            let m = machine_on(Arc::clone(&host), name);
            let before = (state_file(&m), host.vault().services());
            for (to, into) in [
                (VaultForm::Files, "files"),
                (VaultForm::SecretService, "the login keyring"),
            ] {
                let refused = moved(&m, to).expect_err("one way only");
                assert_eq!(refused.code(), "vault_form_unsupported");
                assert_eq!(
                    refused.to_string(),
                    format!("{said}, so they cannot be moved into {into}. Nothing has changed.")
                );
            }
            assert_eq!((state_file(&m), host.vault().services()), before);
        }
    }

    // How a login keyring stops answering: locked, or nothing on the bus for it.
    type Locking = (&'static str, fn(&MemoryHost));

    // Asked before anything is recorded, both ways, whether it is locked or nothing answers.
    #[test]
    fn a_move_into_a_locked_keyring_changes_nothing() {
        let lockings: [Locking; 2] = [
            ("locked", |host| host.vault().fault_all(Fault::Locked)),
            ("gone", |host| host.secret_service_answers(false)),
        ];
        for (how, lock) in lockings {
            let host = MemoryHost::by_form();
            host.secret_service_answers(false);
            let m = two_parked_on(Arc::clone(&host), &format!("move-into-{how}"));
            host.secret_service_answers(true);
            lock(&host);
            let before = (state_file(&m), in_files(&m), in_the_secret_service(&m));

            let refused = moved(&m, VaultForm::SecretService).expect_err("the keyring refuses");

            assert_eq!(refused.code(), "credential_store_locked", "{how}");
            assert_eq!(
                (state_file(&m), in_files(&m), in_the_secret_service(&m)),
                before,
                "{how}"
            );

            let host = MemoryHost::by_form();
            let m = two_parked_on(Arc::clone(&host), &format!("move-out-of-{how}"));
            lock(&host);
            let before = (state_file(&m), in_files(&m), in_the_secret_service(&m));

            let refused = moved(&m, VaultForm::Files).expect_err("the keyring refuses");

            assert_eq!(refused.code(), "credential_store_locked", "{how}");
            assert_eq!(
                (state_file(&m), in_files(&m), in_the_secret_service(&m)),
                before,
                "{how}"
            );
        }
    }

    // The keyring locks once the parks are in files: they are read there, and the copies left in
    // the keyring wait, said, until a change finds it open.
    #[test]
    fn what_a_move_cannot_delete_where_it_was_is_listed_and_deleted_by_the_next_change() {
        let host = MemoryHost::by_form();
        let m = two_parked_on(Arc::clone(&host), "move-left-behind");
        let parks = parked_services(&m);
        let locks = {
            let host = Arc::clone(&host);
            move || host.vault().fault_all(Fault::Locked)
        };

        let (done, said) =
            fault::meanwhile("vault.named", locks, || moved(&m, VaultForm::Files)).expect("moved");

        assert_eq!((done.changed, done.moved, done.leaving), (true, 2, 2));
        assert_eq!(said.len(), 1, "{said:?}");
        assert_eq!(said[0].code(), "parks_pending_removal");
        assert_eq!(in_files(&m), parks);
        assert_eq!(in_the_secret_service(&m), parks);
        let kept = keeping(&m.ctx).expect("read");
        assert_eq!(kept.leaving, Some((VaultForm::SecretService, 2)));
        assert_eq!(kept.place, Some(Vault::Files));
        let refused = moved(&m, VaultForm::SecretService).expect_err("still leaving the keyring");
        assert_eq!(refused.code(), "credential_store_locked");

        host.vault().heal_all();
        settle(&m.ctx, Permit::for_a_test(), None).expect("settled");
        all_in(&m, VaultForm::Files, &parks, "once the keyring opens");
    }

    // The renewal inside every `pitboard` writes a park, which a move cut short would leave in
    // the form it is leaving. A run the schedule started waits too, and keeps no refusal.
    #[test]
    fn a_renewal_waits_for_a_move_cut_short() {
        let m = two_parked_on(MemoryHost::by_form(), "move-renewal-waits");
        let mut state = state::load(&m.ctx).expect("state");
        for account in &mut state.accounts {
            if let Some(park) = account.parked.as_mut() {
                park.access_expires_at = Some(NOW - 60);
            }
        }
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
        let died = fault::killing("vault.copied", || moved(&m, VaultForm::Files));
        assert_eq!(died.unwrap_err(), "vault.copied");
        let before = (state_file(&m), in_files(&m), in_the_secret_service(&m));

        assert!(renew_parked(&m.ctx, Permit::for_a_test()).is_empty());
        let scheduled = m.ctx.clone().started_by_the_schedule();
        assert!(renew_due(&scheduled, Permit::for_a_test(), Due::ToStayAlive).is_empty());

        assert_eq!(
            (state_file(&m), in_files(&m), in_the_secret_service(&m)),
            before
        );
        assert_eq!(crate::schedule::refused(&m.ctx), None);
    }
}
