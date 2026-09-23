//! Parked logins `repair` gave back that this pitboard never wrote.
//!
//! On macOS every `PITBOARD_HOME` shares the login keychain, so a park this home never wrote
//! down may be another pitboard's. Giving one back to the account whose name it carries is
//! additive and safe; what was not safe was what followed. Once given back it was this
//! home's to delete, so the next switch away from its account, a forget or an uninstall
//! deleted another pitboard's parked login. Now only using one deletes it. Every case runs
//! for both tools, because who wrote a park is a fact about the store and not about a tool.
//! Off macOS the vault is a directory inside the home, which no other pitboard parks in, so
//! there a park `repair` finds is this pitboard's own.

use super::harness::{
    Machine, NOW, account, codex_access, codex_account, codex_id, codex_login, codex_machine,
    machine, oauth, owner, renews,
};
use super::*;
use crate::api::scripted::Trouble;
use serde_json::json;

type Make = fn(&str) -> Machine;

const MACHINES: [Make; 2] = [machine, codex_machine];

/// The account id this machine's tool gives `who`.
fn id(m: &Machine, who: &str) -> String {
    match m.which {
        ProviderId::Claude => who.to_string(),
        ProviderId::Codex => codex_id(who),
    }
}

/// `who`'s account, enrolled the way this machine's tool enrols one.
fn enrolled(m: &Machine, who: &str, parked: Option<Park>) -> Account {
    match m.which {
        ProviderId::Claude => account(who, who, parked),
        ProviderId::Codex => codex_account(who, &codex_id(who), parked),
    }
}

/// A login of `who`'s on a refresh chain of its own, as this machine's tool parks one, and
/// a service that answers for it.
fn login(m: &Machine, who: &str, refresh: &str) -> Value {
    match m.which {
        ProviderId::Claude => {
            m.api.owned_by(&format!("access-{refresh}"), owner(who));
            oauth(refresh, 30)
        }
        ProviderId::Codex => {
            m.api.using(
                &codex_access(refresh),
                crate::usage::Snapshot {
                    windows: Vec::new(),
                    observed_at: Some(NOW),
                    account_uuid: None,
                    source: crate::usage::Source::Live,
                },
            );
            codex_login(who, refresh)
        }
    }
}

/// The same, with its access token lapsed, so the next renewal is due.
fn lapsed(m: &Machine, who: &str, refresh: &str) -> Value {
    let mut document = login(m, who, refresh);
    match m.which {
        ProviderId::Claude => document["expiresAt"] = json!((NOW - 60) * 1000),
        ProviderId::Codex => {
            document["tokens"]["access_token"] = json!(crate::provider::jwt::unsigned(
                &json!({"exp": NOW - 60, "for": refresh})
            ));
        }
    }
    document
}

/// `away` is enrolled, not signed in, and holds nothing: its park is gone, the way a
/// refused one goes.
fn enrol_away(m: &Machine) {
    let mut state = state::load(&m.ctx).expect("state");
    state.upsert(enrolled(m, "away", None));
    state::save(&m.ctx, &state).expect("saved");
}

/// Another pitboard on the same machine, with a home of its own and the same keychain,
/// parks a login of `who`'s and records it, the way its own switch would have.
fn parked_elsewhere(m: &Machine, who: &str, document: &Value) -> (Context, Park) {
    let other = m
        .ctx
        .clone()
        .with_pitboard_home(m.ctx_home().join(".pitboard-elsewhere"));
    let service = park::reserve(&other, &id(m, who)).expect("a free name");
    let parked = park::store_at(&other, m.which, &service, document).expect("parked");
    let mut theirs = State::default();
    theirs.accounts.push(enrolled(m, who, Some(parked.clone())));
    state::save(&other, &theirs).expect("saved");
    (other, parked)
}

fn repair_here(m: &Machine) -> Reclaimed {
    repair(settle(&m.ctx, None).expect("nothing to recover").0).expect("repaired")
}

/// The whole of what went wrong. Another pitboard has `here` parked; this one has `here`
/// signed in and so holds nothing for it; `repair` gives the other's park to `here`; and
/// the next switch away parks the live login in its place. That park was never used here,
/// so it is still where the other pitboard left it, and still loads there.
#[test]
fn a_park_repair_gave_back_outlives_a_switch_away_from_its_account() {
    for make in MACHINES {
        let m = make("foreign-replaced");
        let theirs = login(&m, "here", "here-elsewhere");
        let (other, recorded) = parked_elsewhere(&m, "here", &theirs);

        let reclaimed = repair_here(&m);
        assert_eq!(
            reclaimed.given_back,
            vec![(m.key("here").typed(), recorded.service.clone())]
        );
        let state = state::load(&m.ctx).expect("state");
        assert_eq!(state.foreign, vec![recorded.service.clone()]);

        let settled = settle(&m.ctx, None).expect("nothing to recover").0;
        switch(settled, &m.key("there")).expect("switched");

        let state = state::load(&m.ctx).expect("state");
        let parked = state.get(&m.key("here")).unwrap().parked.clone();
        assert_ne!(
            parked.expect("the live login was parked").service,
            recorded.service
        );
        assert!(state.foreign.is_empty(), "nothing here holds it now");
        assert!(!state.discarded.contains(&recorded.service));
        assert_eq!(
            park::load(&other, &m.key("here"), &recorded).expect("the other pitboard's park"),
            theirs,
            "{:?}",
            m.which
        );
    }
}

/// Dropping the account that holds one does not delete it either.
#[test]
fn forgetting_its_account_leaves_a_park_repair_gave_back() {
    for make in MACHINES {
        let m = make("foreign-forgotten");
        enrol_away(&m);
        let (_, recorded) = parked_elsewhere(&m, "away", &login(&m, "away", "away-elsewhere"));
        assert_eq!(repair_here(&m).given_back.len(), 1);

        let settled = settle(&m.ctx, Some(m.which)).expect("nothing to recover").0;
        forget(settled, &m.key("away")).expect("forgotten");

        let state = state::load(&m.ctx).expect("state");
        assert!(state.foreign.is_empty());
        assert!(!state.discarded.contains(&recorded.service));
        assert!(
            m.mem.vault().peek(&recorded.service).is_some(),
            "{:?}: it is still where the other pitboard left it",
            m.which
        );
    }
}

/// Uninstalling deletes what this pitboard wrote and leaves the rest, and says how many it
/// left, because somebody told their logins were removed would otherwise not look for them.
#[test]
fn uninstalling_leaves_a_park_repair_gave_back_and_says_so() {
    for make in MACHINES {
        let m = make("foreign-uninstalled");
        enrol_away(&m);
        let (_, recorded) = parked_elsewhere(&m, "away", &login(&m, "away", "away-elsewhere"));
        assert_eq!(repair_here(&m).given_back.len(), 1);

        let removed =
            uninstall(settle(&m.ctx, None).expect("nothing to recover").0).expect("uninstalled");

        assert_eq!(removed.parks, 1, "`there`'s, which this pitboard wrote");
        assert_eq!(removed.left, 1);
        assert_eq!(removed.pending, 0);
        assert!(
            removed.home_removed,
            "a login left for another pitboard does not keep this one's home"
        );
        assert_eq!(
            m.mem.vault().services(),
            vec![recorded.service.clone()],
            "{:?}",
            m.which
        );
    }
}

/// Installed, it has been used, whoever wrote it. For Codex a parked copy of the login now
/// signed in must not stay beside it, and for Claude Code the park is the live login from
/// now on, so it goes the way every installed park goes.
#[test]
fn a_park_repair_gave_back_is_deleted_once_a_switch_installs_it() {
    for make in MACHINES {
        let m = make("foreign-installed");
        enrol_away(&m);
        let theirs = login(&m, "away", "away-elsewhere");
        let (_, recorded) = parked_elsewhere(&m, "away", &theirs);
        assert_eq!(repair_here(&m).given_back.len(), 1);

        let settled = settle(&m.ctx, None).expect("nothing to recover").0;
        switch(settled, &m.key("away")).expect("switched");

        let tool = provider::of(m.which);
        assert_eq!(
            tool.fingerprint(&m.live().expect("a live login")),
            tool.fingerprint(&theirs)
        );
        let state = state::load(&m.ctx).expect("state");
        assert!(state.foreign.is_empty());
        assert!(
            m.mem.vault().peek(&recorded.service).is_none(),
            "{:?}: used here, so deleted here",
            m.which
        );
    }
}

/// A renewal spends the refresh token it presents, so the copy it renewed is used up
/// whoever wrote it, and what it writes is this pitboard's own.
#[test]
fn a_park_repair_gave_back_is_deleted_once_a_renewal_spends_it() {
    for make in MACHINES {
        let m = make("foreign-renewed");
        enrol_away(&m);
        let (_, recorded) = parked_elsewhere(&m, "away", &lapsed(&m, "away", "away-elsewhere"));
        renews(&m, "away-elsewhere", "away-renewed");
        assert_eq!(repair_here(&m).given_back.len(), 1);

        let renewed = renew_due(&m.ctx, Due::ToBeAsked);

        assert_eq!(renewed.len(), 1);
        assert_eq!(renewed[0].1.code(), "renewed", "{:?}", m.which);
        let state = state::load(&m.ctx).expect("state");
        let fresh = state.get(&m.key("away")).unwrap().parked.clone();
        assert_ne!(fresh.expect("renewed").service, recorded.service);
        assert!(state.foreign.is_empty(), "what the renewal wrote is ours");
        assert!(
            m.mem.vault().peek(&recorded.service).is_none(),
            "{:?}: spent here, so deleted here",
            m.which
        );
    }
}

/// Killed after the service answered and before the answer was recorded, a renewal leaves
/// the fresh copy for the next change to give back in place of the one it spent. That one
/// was saved as used before the answer was written, so giving the fresh one back deletes it
/// rather than letting it go for a pitboard that would present a spent token.
#[test]
fn a_park_repair_gave_back_is_deleted_once_a_killed_renewal_spends_it() {
    for make in MACHINES {
        let m = make("foreign-renewal-killed");
        enrol_away(&m);
        let (_, recorded) = parked_elsewhere(&m, "away", &lapsed(&m, "away", "away-elsewhere"));
        renews(&m, "away-elsewhere", "away-renewed");
        assert_eq!(repair_here(&m).given_back.len(), 1);

        let died = crate::fault::killing("renew.park_stored", || renew_due(&m.ctx, Due::ToBeAsked));
        assert_eq!(died.unwrap_err(), "renew.park_stored", "{:?}", m.which);
        settle(&m.ctx, None).expect("the next change gives the fresh copy back");

        let state = state::load(&m.ctx).expect("state");
        assert_eq!(
            state
                .get(&m.key("away"))
                .and_then(|a| a.parked.as_ref())
                .map(|p| p.refresh_fingerprint.clone()),
            Some(store::fingerprint("away-renewed")),
            "{:?}",
            m.which
        );
        assert!(state.foreign.is_empty());
        assert!(
            m.mem.vault().peek(&recorded.service).is_none(),
            "{:?}: spent here, so deleted here",
            m.which
        );
        super::harness::hold(&m, &format!("{:?}, after a killed renewal", m.which));
    }
}

/// Refused is not spent: presenting it took nothing from it. The pitboard that wrote it is
/// refused the same way and drops it itself.
#[test]
fn a_park_repair_gave_back_that_the_service_refuses_is_let_go_and_left() {
    for make in MACHINES {
        let m = make("foreign-refused");
        enrol_away(&m);
        let (_, recorded) = parked_elsewhere(&m, "away", &lapsed(&m, "away", "away-elsewhere"));
        match m.which {
            ProviderId::Claude => m.api.renew_trouble("away-elsewhere", Trouble::InvalidGrant),
            ProviderId::Codex => m
                .api
                .codex_renew_trouble("away-elsewhere", Trouble::InvalidGrant),
        };
        assert_eq!(repair_here(&m).given_back.len(), 1);

        let renewed = renew_due(&m.ctx, Due::ToBeAsked);

        assert_eq!(renewed[0].1.code(), "parked_login_refused");
        let state = state::load(&m.ctx).expect("state");
        assert!(state.get(&m.key("away")).unwrap().parked.is_none());
        assert!(state.foreign.is_empty());
        assert!(
            m.mem.vault().peek(&recorded.service).is_some(),
            "{:?}",
            m.which
        );
    }
}

/// What this pitboard wrote down itself is its own, however it was found again. Written
/// down, written to, and killed before anything recorded it, it is given back like the
/// other pitboard's, and the same switch away deletes it as it always did.
#[test]
fn a_park_this_pitboard_wrote_down_and_lost_is_still_deleted_when_replaced() {
    for make in MACHINES {
        let m = make("ours-replaced");
        let service = park::reserve(&m.ctx, &id(&m, "here")).expect("a free name");
        park::store_at(&m.ctx, m.which, &service, &login(&m, "here", "here-lost")).expect("parked");

        let settled = settle(&m.ctx, None).expect("the sweep gives it back").0;
        let state = state::load(&m.ctx).expect("state");
        assert_eq!(
            state
                .get(&m.key("here"))
                .and_then(|a| a.parked.as_ref())
                .map(|p| p.service.as_str()),
            Some(service.as_str())
        );
        assert!(state.foreign.is_empty(), "this pitboard wrote it down");

        switch(settled, &m.key("there")).expect("switched");

        assert!(
            m.mem.vault().peek(&service).is_none(),
            "{:?}: its own, so deleted",
            m.which
        );
    }
}

/// A park written into this vault and then lost from this home's records, as a state file
/// restored from a backup leaves it. Only its account names it.
fn lost(m: &Machine, who: &str, document: &Value) -> Park {
    let service = park::service_name(&id(m, who), (NOW - 60) * 1000);
    park::store_at(&m.ctx, m.which, &service, document).expect("parked")
}

/// Off macOS the vault is a directory inside pitboard's own, which no other pitboard parks
/// in. A park `repair` finds there is this pitboard's even when nothing here wrote its name
/// down, so it is deleted like any other once it is let go, and `uninstall` leaves nothing
/// behind and does not say it did.
#[test]
fn a_park_found_in_a_vault_of_this_homes_own_is_this_pitboards() {
    for make in MACHINES {
        let m = make("found-replaced");
        m.mem.vault_of_its_own();
        let found = lost(&m, "here", &login(&m, "here", "here-lost"));
        assert_eq!(repair_here(&m).given_back.len(), 1);
        assert!(state::load(&m.ctx).expect("state").foreign.is_empty());

        let settled = settle(&m.ctx, None).expect("nothing to recover").0;
        switch(settled, &m.key("there")).expect("switched");
        assert!(
            m.mem.vault().peek(&found.service).is_none(),
            "{:?}: replaced, and this pitboard's, so deleted",
            m.which
        );
        super::harness::hold(&m, &format!("{:?}, after the switch away", m.which));

        let m = make("found-uninstalled");
        m.mem.vault_of_its_own();
        enrol_away(&m);
        lost(&m, "away", &login(&m, "away", "away-lost"));
        assert_eq!(repair_here(&m).given_back.len(), 1);

        let removed =
            uninstall(settle(&m.ctx, None).expect("nothing to recover").0).expect("uninstalled");
        assert_eq!(
            removed.parks, 2,
            "{:?}: `there`'s and the one found",
            m.which
        );
        assert_eq!(removed.left, 0, "{:?}", m.which);
        assert!(m.mem.vault().services().is_empty(), "{:?}", m.which);
    }
}
