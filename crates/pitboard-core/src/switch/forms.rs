//! Where each home keeps its parked logins on a machine that offers two ways, as Linux offers
//! the Secret Service and files: the form a new home records, that a home keeps it, and that a
//! park another home keeps in the one store every home shares is that home's.

use super::harness::{self, Rig, account, machine, machine_on, oauth, owner};
use super::*;
use crate::host::memory::MemoryHost;
use crate::permit::Permit;
use crate::state::VaultForm;
use std::sync::Arc;

fn recorded(m: &Rig) -> Option<VaultForm> {
    state::vault_form(&m.ctx).expect("the state file is read")
}

fn in_files(m: &Rig) -> Vec<String> {
    m.mem.files_vault(&m.ctx).services()
}

fn in_the_secret_service(m: &Rig) -> Vec<String> {
    m.mem.vault().services()
}

fn parked(m: &Rig, label: &str) -> Park {
    state::load(&m.ctx)
        .expect("state")
        .get(&m.key(label))
        .and_then(|a| a.parked.clone())
        .unwrap_or_else(|| panic!("{label} is parked"))
}

fn switched(m: &Rig, to: &str) -> Result<(Outcome, Vec<Warning>)> {
    let settled = settle(&m.ctx, Permit::for_a_test(), None)?.0;
    switch(settled, &m.key(to))
}

// The harness parks `there` before its first save: that is where a new home's form is decided.
#[test]
fn a_new_home_records_the_form_its_host_offers() {
    for (answers, form) in [(true, VaultForm::SecretService), (false, VaultForm::Files)] {
        let host = MemoryHost::by_form();
        host.secret_service_answers(answers);
        let m = machine_on(host, &format!("new-home-{answers}"));
        assert_eq!(recorded(&m), Some(form));
        let there = vec![parked(&m, "there").service];
        let (holds, empty) = match form {
            VaultForm::SecretService => (in_the_secret_service(&m), in_files(&m)),
            VaultForm::Files => (in_files(&m), in_the_secret_service(&m)),
        };
        assert_eq!(holds, there, "{form:?}");
        assert!(empty.is_empty(), "{form:?}: {empty:?}");
    }

    let m = machine("new-home-one-way");
    assert_eq!(recorded(&m), None);
    let written = std::fs::read_to_string(home::dir(&m.ctx).join("state.json")).expect("saved");
    assert!(!written.contains("\"vault\""), "{written}");
}

// Files while the Secret Service answers: what a home recorded is where its parks are.
#[test]
fn a_home_keeps_the_vault_form_it_recorded() {
    let host = MemoryHost::by_form();
    host.secret_service_answers(false);
    let m = machine_on(Arc::clone(&host), "keeps-files");
    host.secret_service_answers(true);

    switched(&m, "there").expect("switched");

    assert_eq!(recorded(&m), Some(VaultForm::Files));
    assert_eq!(in_files(&m), vec![parked(&m, "here").service]);
    assert!(in_the_secret_service(&m).is_empty());
    harness::hold(&m, "a switch in files");
}

// Files would read as holding none of its parks, and a park written there is one nothing names.
#[test]
fn a_home_on_the_secret_service_is_refused_where_none_answers_and_never_falls_back_to_files() {
    let host = MemoryHost::by_form();
    let m = machine_on(Arc::clone(&host), "secret-service-gone");
    let state_file = home::dir(&m.ctx).join("state.json");
    let before = (m.live(), std::fs::read(&state_file).expect("saved"));
    host.secret_service_answers(false);

    let refused = switched(&m, "there").expect_err("nothing answers for the parks");

    assert_eq!(refused.code(), "credential_store_locked");
    assert!(in_files(&m).is_empty(), "{:?}", in_files(&m));
    assert_eq!(
        (m.live(), std::fs::read(&state_file).expect("saved")),
        before
    );
    assert_eq!(recorded(&m), Some(VaultForm::SecretService));
    host.secret_service_answers(true);
    harness::hold(&m, "a switch the Secret Service refused");
}

// As the keychain on macOS (`foreign.rs`): every home of the session parks in the one collection.
#[test]
fn another_homes_park_in_the_secret_service_is_foreign_not_orphaned() {
    for answers in [true, false] {
        let host = MemoryHost::by_form();
        host.secret_service_answers(answers);
        let m = machine_on(host, &format!("another-home-{answers}"));
        let other = m
            .ctx
            .clone()
            .with_pitboard_home(m.ctx_home().join(".pitboard-elsewhere"));
        m.api.owned_by("access-here-elsewhere", owner("here"));
        let theirs = oauth("here-elsewhere", 30);
        let service = park::reserve(&other, Permit::for_a_test(), "here").expect("a free name");
        let elsewhere = park::store_at(
            &other,
            Permit::for_a_test(),
            provider::document(m.which),
            &service,
            &theirs,
        )
        .expect("parked");
        let mut state = State::default();
        state
            .accounts
            .push(account("here", "here", Some(elsewhere.clone())));
        state::save(&other, Permit::for_a_test(), &state).expect("saved");

        let reclaimed = repair(
            settle(&m.ctx, Permit::for_a_test(), None)
                .expect("nothing to recover")
                .0,
        )
        .expect("repaired")
        .0;

        if !answers {
            assert!(reclaimed.is_empty(), "another home's files: {reclaimed:?}");
            continue;
        }
        assert_eq!(
            reclaimed.given_back,
            vec![(m.key("here").typed(), service.clone())]
        );
        assert_eq!(
            state::load(&m.ctx).expect("state").foreign,
            vec![service.clone()]
        );
        switched(&m, "there").expect("switched");
        assert_ne!(parked(&m, "here").service, service);
        assert_eq!(
            park::load(
                &other,
                provider::document(m.which),
                &m.key("here"),
                &elsewhere
            )
            .expect("the other home's park"),
            theirs
        );
    }
}
