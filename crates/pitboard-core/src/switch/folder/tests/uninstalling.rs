use super::{
    code, forget_, killed, park_by_hand, park_name, park_of, set, state_of, two_accounts, use_,
    watched,
};
use super::{data, settled, snapshot, tree, unchanged, whereabouts};
use crate::provider::ProviderId;
use crate::service::Permit;
use crate::state::{self, SetAside};
use crate::switch::harness::{
    HOME, NOW, Rig, STRANGER, WORK, desktop_machine, desktop_pair, hold_where_twice, recover,
};
use crate::switch::{repair, settle};
use serde_json::{Map, Value};

#[test]
fn forget_and_uninstall_delete_the_folders_they_are_asked_to() {
    use crate::host::fs::testing;
    let m = two_accounts("deletes");
    let homes_park = park_name(&HOME);
    let (homes_aside, works_aside, nobodys) =
        ("pitboard-aside-1", "pitboard-aside-2", "pitboard-aside-3");
    park_by_hand(&m, Some(&m.key("home")), &homes_park, &HOME);
    let mut state = state::load(&m.ctx).expect("state");
    for (name, filed_under) in [
        (homes_aside, Some(&HOME)),
        (works_aside, Some(&WORK)),
        (nobodys, None),
    ] {
        let kept = park_by_hand(&m, None, name, &STRANGER);
        state.aside.push(state::Aside {
            name: name.into(),
            tool: ProviderId::Desktop,
            at: NOW,
            why: SetAside::Superseded,
            account: filed_under.map(|who| format!("desktop_{}", who.uuid)),
            login: Some(kept.login),
        });
    }
    state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
    testing::make_private(&crate::home::dir(&m.ctx)).expect("private");
    testing::make_private(&crate::store::folder::parks_dir(&m.ctx)).expect("private");
    let claudes = tree(&data(&m));
    let jar_of = |name: &str| crate::store::folder::item(&m.ctx, name, "Cookies");

    forget_(&m, "home").expect("forgotten");
    assert!(!crate::store::folder::at(&m.ctx, &homes_park).exists());
    assert!(!crate::store::folder::at(&m.ctx, homes_aside).exists());
    assert!(jar_of(works_aside).exists(), "another account's stays");
    assert!(jar_of(nobodys).exists(), "and nobody's");
    let state = state::load(&m.ctx).expect("state");
    assert!(state.discarded.is_empty(), "{:?}", state.discarded);
    assert_eq!(state.aside.len(), 2);
    assert_eq!(tree(&data(&m)), claudes);
    hold_where_twice(&m, "forgotten");

    let removed = crate::switch::uninstall(settled(&m)).expect("uninstalled");
    assert_eq!(
        (removed.set_aside, removed.parks, removed.pending),
        (2, 0, 0)
    );
    assert!(removed.home_removed);
    assert!(!crate::home::dir(&m.ctx).exists());
    assert_eq!(tree(&data(&m)), claudes, "Claude's folder is as it was");
}

#[test]
fn uninstall_refuses_where_claude_is_signed_out_whatever_the_record_says() {
    let uninstalled = |m: &Rig| {
        crate::switch::uninstall(
            settle(&m.ctx, Permit::for_a_test(), None)
                .expect("settled")
                .0,
        )
    };
    // An add left halfway.
    let m = desktop_pair("uninstall-awaited");
    watched(&m, "travel").expect("parked").cancel();
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let refused = uninstalled(&m).expect_err("refused");
    assert_eq!(refused.code(), "uninstall_would_strand");
    assert!(
        refused.to_string().contains("Run `pitboard use desktop/"),
        "{refused}"
    );
    unchanged(&m, &before, "nothing changed");

    // `work` recorded in use, and Claude logged out since.
    let m = desktop_pair("uninstall-logged-out");
    let claude = m.claude();
    claude.opens();
    claude.logs_out();
    claude.quits();
    recover(&m).expect("swept");
    let before = snapshot(&m);
    assert_eq!(
        uninstalled(&m).expect_err("refused").code(),
        "uninstall_would_strand"
    );
    unchanged(&m, &before, "logged out");
    assert!(crate::home::dir(&m.ctx).exists());

    // Claude in doubt: its row's own refusal.
    let m = desktop_pair("uninstall-in-doubt");
    set(&m, "lastKnownAccountUuid", None);
    recover(&m).expect("swept");
    let before = snapshot(&m);
    assert_eq!(
        uninstalled(&m).expect_err("refused").code(),
        "sign_in_unfinished"
    );
    unchanged(&m, &before, "in doubt");
    assert!(crate::home::dir(&m.ctx).exists());

    // Claude open, with an account parked.
    let m = desktop_pair("uninstall-open");
    m.claude().opens();
    recover(&m).expect("swept");
    let before = snapshot(&m);
    assert_eq!(uninstalled(&m).expect_err("refused").code(), "app_open");
    unchanged(&m, &before, "Claude open");

    // Signed in to an account Pitboard knows: every park goes, and Claude keeps its login.
    let m = desktop_pair("uninstall");
    let removed = uninstalled(&m).unwrap_or_else(|e| panic!("uninstalled: {e}"));
    assert_eq!(removed.parks, 1);
    assert!(removed.home_removed);
    assert_eq!(whereabouts(&m, &WORK), ["Claude"]);
}

#[test]
fn uninstall_refuses_where_a_login_is_only_set_aside_and_claude_is_signed_out() {
    // `state.json` lost after a Log out in Claude: `repair` sets `home`'s park aside.
    let m = desktop_pair("uninstall-only-aside");
    let homes = park_of(&m, "home").expect("parked").service;
    let claude = m.claude();
    claude.opens();
    claude.logs_out();
    claude.quits();
    recover(&m).expect("swept");
    std::fs::remove_file(crate::home::dir(&m.ctx).join("state.json")).expect("lost");
    let (reclaimed, _) = repair(settled(&m)).expect("repaired");
    assert_eq!(reclaimed.set_aside, [homes.as_str()]);
    recover(&m).expect("swept");
    let before = snapshot(&m);

    let refused = crate::switch::uninstall(
        settle(&m.ctx, Permit::for_a_test(), None)
            .expect("settled")
            .0,
    )
    .expect_err("refused");

    assert_eq!(refused.code(), "uninstall_would_strand");
    let folder = crate::store::folder::at(&m.ctx, &homes);
    assert!(
        refused.to_string().contains(&folder.display().to_string()),
        "{refused}"
    );
    unchanged(&m, &before, "only set aside");
    assert_eq!(whereabouts(&m, &HOME), [homes.as_str()]);

    // The folder deleted by hand: nothing is left to strand.
    std::fs::remove_dir_all(&folder).expect("deleted by hand");
    let removed = crate::switch::uninstall(
        settle(&m.ctx, Permit::for_a_test(), None)
            .expect("settled")
            .0,
    )
    .unwrap_or_else(|e| panic!("uninstalled: {e}"));
    assert!(removed.home_removed);
    assert!(!crate::home::dir(&m.ctx).exists());
}

#[test]
fn uninstall_deletes_a_park_an_arrive_emptied_whatever_was_left_in_it() {
    // Finder's own file, written while `home` was parked.
    let m = desktop_pair("uninstall-stray-file");
    let homes = park_of(&m, "home").expect("parked").service;
    std::fs::write(
        crate::store::folder::at(&m.ctx, &homes).join("items/.DS_Store"),
        b"Finder",
    )
    .expect("a stray file");
    use_(&m, "home").expect("`home` in use");
    assert!(
        state_of(&m)
            .discarded
            .iter()
            .any(|dropped| dropped.service == homes && !dropped.forgotten),
        "kept listed"
    );

    let removed = crate::switch::uninstall(
        settle(&m.ctx, Permit::for_a_test(), None)
            .expect("settled")
            .0,
    )
    .expect("uninstalled");

    assert_eq!(removed.pending, 0);
    assert!(removed.home_removed);
    assert!(!crate::home::dir(&m.ctx).exists());
    assert_eq!(whereabouts(&m, &HOME), ["Claude"]);
}

#[test]
fn forgetting_an_account_while_a_move_waits_is_refused_and_deletes_nothing() {
    let m = desktop_pair("forget-waits");
    killed(&m, "leave.jar_out", "home");
    recover(&m).expect("swept");
    m.claude().opens();
    let before = snapshot(&m);
    for label in ["home", "work"] {
        assert_eq!(code(forget_(&m, label)), "app_open", "{label}");
    }
    m.claude().crashes();
    unchanged(&m, &before, "Claude open");

    // Tokens back beside no session: no row of recovery names it.
    let park = state_of(&m)
        .moving(ProviderId::Desktop)
        .expect("recorded")
        .park
        .clone();
    let kept =
        std::fs::read_to_string(crate::store::folder::keys_file(&m.ctx, &park)).expect("keys");
    let tokens: Map<String, Value> = serde_json::from_str(&kept).expect("JSON");
    set(
        &m,
        "oauth:tokenCache",
        tokens.get("oauth:tokenCache").cloned(),
    );
    let before = snapshot(&m);
    for label in ["home", "work"] {
        assert_eq!(
            code(forget_(&m, label)),
            "recovery_undetermined",
            "{label}: never the doubt rule"
        );
    }
    unchanged(&m, &before, "undetermined");
    assert!(state_of(&m).get(&m.key("home")).is_some());
}

#[test]
fn uninstall_and_repair_read_nothing_of_claude_where_nothing_of_it_is_parked() {
    let settled_all = |m: &Rig| {
        settle(&m.ctx, Permit::for_a_test(), None)
            .expect("settled")
            .0
    };
    // No scratch Claude named: main's machine.
    let m = crate::switch::harness::machine("nothing-of-claude");
    repair(settled_all(&m)).expect("repaired");
    crate::switch::uninstall(settled_all(&m)).expect("uninstalled");
    assert_eq!((m.mem.listings(), m.mem.jars_asked().len()), (0, 0));

    // Claude Desktop enrolled, in use, nothing of it parked.
    let m = desktop_machine("nothing-parked");
    let (listings, jars) = (m.mem.listings(), m.mem.jars_asked().len());
    repair(settled_all(&m)).expect("repaired");
    let removed = crate::switch::uninstall(settled_all(&m)).expect("uninstalled");
    assert!(removed.warnings.is_empty());
    assert_eq!(
        (m.mem.listings(), m.mem.jars_asked().len()),
        (listings, jars)
    );
    assert_eq!(whereabouts(&m, &WORK), ["Claude"]);
}
