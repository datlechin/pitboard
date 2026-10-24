use super::{
    code, confirm_, enrol, forget_, jar, names, park_of, reads_only, refused, set, state_of,
    two_accounts, use_, used, watched, without_items,
};
use super::{data, snapshot};
use crate::desktop_testing::{EMPTIED, write_stand_in_jar};
use crate::host::Cookie;
use crate::provider::ProviderId;
use crate::service::{Permit, Warning};
use crate::state;
use crate::switch::harness::{
    HOME, STRANGER, WORK, desktop_machine, desktop_pair, hold, hold_where_deleted, hold_where_twice,
};
use serde_json::json;

#[test]
fn no_data_folder_and_an_empty_one_hold_nobody() {
    // Row 5: no jar, none of the items, none of the keys.
    let m = two_accounts("row-5");
    without_items(&m);
    for key in [
        "oauth:tokenCache",
        "oauth:tokenCacheV2",
        "lastKnownAccountUuid",
    ] {
        set(&m, key, None);
    }
    let before = snapshot(&m);
    assert_eq!(code(enrol(&m, "work")), "live_credential_absent");
    assert_eq!(code(use_(&m, "home")), "nothing_parked");
    assert_eq!(code(confirm_(&m, "work")), "account_not_in_use");
    assert_eq!(snapshot(&m), before);
    hold_where_deleted(&m, "row 5, whose items the test took away");

    // Row 3: no data folder. `forget` records nobody, and forgets any account.
    let m = two_accounts("row-3");
    std::fs::remove_dir_all(data(&m)).expect("the folder taken away by hand");
    let before = snapshot(&m);
    assert_eq!(code(enrol(&m, "work")), "live_credential_absent");
    let refused = use_(&m, "home").expect_err("refused");
    assert!(
        matches!(&refused, crate::error::Error::LoginFolderUnknown { what, .. } if what == "does not exist yet"),
        "{refused:?}"
    );
    assert_eq!(snapshot(&m), before);
    assert!(!data(&m).exists(), "nothing made it");
    forget_(&m, "work").expect("forgotten, though the record named it");
    let state = state::load(&m.ctx).expect("state");
    assert!(state.get(&m.key("work")).is_none());
    let record = state.in_use(ProviderId::Desktop).expect("recorded");
    assert!(record.owner.is_none() && record.login.is_empty());
}

#[test]
fn a_data_folder_that_cannot_be_looked_at_is_never_no_data_folder() {
    use crate::host::fs::testing;
    let m = two_accounts("unseen-folder");
    let above = m.ctx_home().join("claude");
    let index = crate::home::dir(&m.ctx).join("state.json");
    let before = std::fs::read(&index).expect("state");
    testing::deny_reading(&above).expect("closed");
    // The system's administrator is denied nothing, so there is nothing to see there.
    if std::fs::symlink_metadata(data(&m)).is_ok() {
        testing::make_private(&above).expect("opened again");
        return;
    }
    let refused = enrol(&m, "work");
    let used = use_(&m, "home");
    let confirmed = confirm_(&m, "work");
    let forgotten = [forget_(&m, "work"), forget_(&m, "home")];
    testing::make_private(&above).expect("opened again");
    let refused = refused.expect_err("refused");
    assert_eq!(refused.code(), "login_folder_unknown");
    assert!(
        refused.to_string().contains("could not be looked at ("),
        "{refused}"
    );
    assert_eq!(code(used), "login_folder_unknown");
    assert_eq!(code(confirmed), "login_folder_unknown");
    for forgotten in forgotten {
        assert_eq!(code(forgotten), "login_folder_unknown", "doubt rule");
    }
    assert_eq!(std::fs::read(&index).expect("state"), before);
    hold(&m, "unseen folder");
}

#[test]
fn no_session_cookie_beside_live_tokens_is_never_signed_out() {
    let m = desktop_machine("live-tokens");
    jar(&m, None);
    let before = snapshot(&m);
    let refused = enrol(&m, "work").expect_err("refused");
    assert_eq!(refused.code(), "login_folder_unknown");
    assert!(
        refused
            .to_string()
            .contains("holds no session and still holds a login's tokens"),
        "{refused}"
    );
    assert_eq!(snapshot(&m), before);
    // Both caches emptied, the name kept: what a Log out leaves, which is signed out.
    for key in ["oauth:tokenCache", "oauth:tokenCacheV2"] {
        set(&m, key, Some(serde_json::from_str(EMPTIED).expect("JSON")));
    }
    let before = snapshot(&m);
    assert_eq!(code(enrol(&m, "work")), "live_credential_absent");
    assert_eq!(snapshot(&m), before);
}

#[test]
fn a_session_with_no_name_is_a_sign_in_not_finished() {
    let m = desktop_machine("no-name");
    jar(&m, Some(&HOME));
    set(&m, "lastKnownAccountUuid", None);
    let before = snapshot(&m);
    for label in ["work", "home"] {
        assert_eq!(code(enrol(&m, label)), "sign_in_unfinished", "{label}");
    }
    assert_eq!(code(use_(&m, "work")), "sign_in_unfinished");
    assert_eq!(snapshot(&m), before);
}

#[test]
fn a_session_beside_emptied_caches_is_never_taken_as_named() {
    for (named, label) in [(&WORK, "work"), (&HOME, "home"), (&STRANGER, "stranger")] {
        let m = two_accounts(&format!("emptied-{label}"));
        jar(&m, Some(&STRANGER));
        names(&m, named);
        for key in ["oauth:tokenCache", "oauth:tokenCacheV2"] {
            set(&m, key, Some(serde_json::from_str(EMPTIED).expect("JSON")));
        }
        let before = snapshot(&m);
        for under in ["work", "home", "stranger"] {
            assert_eq!(
                code(enrol(&m, under)),
                "sign_in_unfinished",
                "named {label}, enrolled under {under}"
            );
        }
        assert_eq!(code(use_(&m, "home")), "sign_in_unfinished", "{label}");
        assert_eq!(snapshot(&m), before, "{label}");
    }
}

// A Log out leaves the connector grants, which hold no login of Claude's.
#[test]
fn a_log_out_beside_connector_grants_is_signed_out() {
    let m = desktop_machine("grants-kept");
    jar(&m, None);
    for key in ["oauth:tokenCache", "oauth:tokenCacheV2"] {
        set(&m, key, Some(serde_json::from_str(EMPTIED).expect("JSON")));
    }
    set(&m, "custom3pMcpOAuth", Some(json!({"grant": "kept"})));
    let before = snapshot(&m);
    assert_eq!(code(enrol(&m, "work")), "live_credential_absent");
    assert_eq!(snapshot(&m), before);
}

#[test]
fn hold_finds_a_session_beside_another_accounts_name() {
    let m = desktop_pair("beside");
    hold(&m, "work in use under its own name");
    set(&m, "lastKnownAccountUuid", Some(json!(HOME.uuid)));
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        hold(&m, "work's session, home's name");
    }));
    let said = failed.expect_err("hold fails");
    let said = said.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(
        said.contains("work's session beside another name"),
        "{said}"
    );
    hold_where_twice(&m, "a row built by hand");
}

#[test]
fn whole_is_false_while_a_move_waits_and_after_a_switch_left_signed_out() {
    let m = desktop_pair("whole");
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    let whole = || pitboard.whole(ProviderId::Desktop);
    assert!(whole());
    assert!(
        pitboard.whole(ProviderId::Claude),
        "a document tool always is"
    );

    let died = crate::fault::killing("leave.jar_out", || used(&m, "home"));
    assert_eq!(died.expect_err("killed"), "leave.jar_out");
    assert!(!whole(), "a move waits");
    reads_only(&m, &pitboard, "a move waits");
    let standing = pitboard.status_offline().expect("read").warnings;
    assert!(
        matches!(
            standing.as_slice(),
            [Warning::MoveWaiting { label, .. }] if label == "desktop/work"
        ),
        "{standing:?}"
    );

    // Settled to signed out with both parked, as a switch left signed out leaves it.
    assert_eq!(code(confirm_(&m, "work")), "account_not_in_use");
    assert!(!whole());
    used(&m, "home");
    assert!(whole());

    let in_the_way = data(&m).join("File System");
    let home_park = park_of(&m, "home");
    assert!(home_park.is_none(), "`home` is in use");
    let work_park = park_of(&m, "work").expect("parked").service;
    let listed = crate::store::folder::item(&m.ctx, &work_park, "File System");
    std::fs::create_dir_all(&listed).expect("an item");
    let mut manifest = crate::store::folder::manifest(&m.ctx, &work_park)
        .expect("reads")
        .expect("there");
    manifest.items.push("File System".into());
    crate::store::folder::write_manifest(&m.ctx, Permit::for_a_test(), &work_park, &manifest)
        .expect("written");
    let failed = crate::fault::meanwhile(
        "leave.done",
        move || std::fs::write(&in_the_way, b"planted meanwhile").expect("planted"),
        || refused(&m, "work"),
    );
    assert_eq!(failed.code(), "switch_left_signed_out", "{failed}");
    assert!(!whole(), "nobody in use, both parked");

    let file = crate::home::dir(&m.ctx).join("state.json");
    std::fs::write(&file, b"not json").expect("broken");
    assert!(!whole(), "an index that does not read");
}

#[test]
fn a_move_left_waiting_for_the_apps_tests_is_the_switchs_own_record() {
    let m = desktop_pair("left-waiting");
    crate::desktop_testing::leave_a_move_waiting(&m.ctx, &m.mem, &m.key("work"));
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    assert!(!pitboard.whole(ProviderId::Desktop));
    assert!(pitboard.moving(ProviderId::Desktop));
    let state = state::load(&m.ctx).expect("state");
    let moving = state.moving(ProviderId::Desktop).expect("the record stays");
    assert_eq!(moving.label, "work");
    let crate::provider::LoginKind::Folder(app) = crate::provider::of(ProviderId::Desktop) else {
        unreachable!("a folder tool")
    };
    assert!(
        matches!(
            crate::switch::folder::open(&m.ctx, app),
            crate::switch::folder::Open::No
        ),
        "Claude is closed again"
    );
}

#[test]
fn a_session_the_apps_fixture_sees_is_claude_ais_own() {
    let m = desktop_machine("holds-a-session");
    let claude = m.claude();
    assert!(claude.holds_a_session());
    let elsewhere = Cookie {
        host: ".example.com".into(),
        name: "sessionKey".into(),
        path: "/".into(),
        stored: b"v10elsewhere".to_vec(),
        expires_utc: 0,
    };
    write_stand_in_jar(&data(&m).join("Cookies"), 24, &[elsewhere]);
    assert!(!claude.holds_a_session(), "another site's cookie");
    claude.opens();
    claude.logs_out();
    claude.quits();
    assert!(!claude.holds_a_session(), "logged out");
}

#[test]
fn moving_is_true_only_while_a_move_waits_or_the_index_does_not_read() {
    let m = desktop_pair("moving");
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    let moving = || pitboard.moving(ProviderId::Desktop);
    assert!(!moving());
    assert!(
        !pitboard.moving(ProviderId::Claude),
        "a document tool never is"
    );

    let died = crate::fault::killing("leave.jar_out", || used(&m, "home"));
    assert_eq!(died.expect_err("killed"), "leave.jar_out");
    let asked = m.mem.listings();
    assert!(moving(), "a move waits");
    assert_eq!(m.mem.listings(), asked, "asked of the index alone");

    // Signed out with both parked and nothing waiting: Claude may be opened for a sign-in.
    assert_eq!(code(confirm_(&m, "work")), "account_not_in_use");
    assert!(!moving());
    used(&m, "home");
    watched(&m, "travel")
        .expect("`home` parked, waiting")
        .cancel();
    assert!(state_of(&m).awaiting(ProviderId::Desktop).is_some());
    assert!(!moving(), "a sign-in waited for is no move");

    let file = crate::home::dir(&m.ctx).join("state.json");
    std::fs::write(&file, b"not json").expect("broken");
    assert!(moving(), "an index that does not read");
}
