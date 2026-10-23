use super::super::harness::{
    HOME, Machine, NOW, STRANGER, WORK, as_a_dead_runs, audit_lines, data, desktop_machine,
    desktop_pair, hold, hold_where_deleted, hold_where_twice, recover, rested, settled,
    sign_in_inside, snapshot, tree, unchanged, whereabouts,
};
use super::super::{Outcome, enroll, forget, repair, settle, switch, update_config};
use crate::desktop_testing::{EMPTIED, JarAs, Person, plant, write_stand_in_jar};
use crate::error::Error;
use crate::host::Cookie;
use crate::provider::ProviderId;
use crate::service::{Permit, Warning};
use crate::state::{self, Key, Park, ParkKind, SetAside};
use crate::store::folder::Manifest;
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;

// `work`, in a session from before the one Claude holds.
const WORK_BEFORE: Person = Person {
    uuid: WORK.uuid,
    session: "work-session-before",
};

fn enrol(m: &Machine, label: &str) -> crate::error::Result<(super::Enrolled, Vec<Warning>)> {
    enroll(settled(m), &m.key(label), None)
}

fn use_(m: &Machine, label: &str) -> crate::error::Result<Outcome> {
    switch(settled(m), &m.key(label)).map(|(outcome, _)| outcome)
}

fn forget_(m: &Machine, label: &str) -> crate::error::Result<String> {
    forget(settled(m), &m.key(label)).map(|(email, _)| email)
}

fn confirm_(m: &Machine, label: &str) -> crate::error::Result<Outcome> {
    update_config(settled(m), &m.key(label)).map(|(outcome, _)| outcome)
}

fn code<T: std::fmt::Debug>(result: crate::error::Result<T>) -> &'static str {
    result.expect_err("refused").code()
}

// Claude's settings with `key` set to `value`, or taken out, in place.
fn set(m: &Machine, key: &str, value: Option<Value>) {
    let path = data(m).join("config.json");
    let mut settings: Map<String, Value> =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("settings")).expect("JSON");
    match value {
        Some(value) => {
            settings.insert(key.into(), value);
        }
        None => {
            settings.shift_remove(key);
        }
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&settings).expect("JSON"),
    )
    .expect("set");
}

fn names(m: &Machine, who: &Person) {
    set(m, "lastKnownAccountUuid", Some(json!(who.uuid)));
}

fn session_of(who: Option<&Person>) -> Vec<Cookie> {
    who.map(|who| Cookie {
        host: ".claude.ai".into(),
        name: "sessionKey".into(),
        path: "/".into(),
        stored: [b"v10".as_slice(), who.session.as_bytes()].concat(),
        expires_utc: 0,
    })
    .into_iter()
    .collect()
}

// The jar holding `who`'s session, or none.
fn jar(m: &Machine, who: Option<&Person>) {
    write_stand_in_jar(&data(m).join("Cookies"), 24, &session_of(who));
}

// `work` and `home` enrolled, `work` in use: what two enrolments over one Claude leave.
fn two_accounts(name: &str) -> Machine {
    let m = desktop_machine(name);
    jar(&m, Some(&HOME));
    names(&m, &HOME);
    enrol(&m, "home").expect("`home` enrolled");
    jar(&m, Some(&WORK));
    names(&m, &WORK);
    enrol(&m, "work").expect("`work` in use again");
    m
}

fn items() -> &'static [&'static str] {
    match crate::provider::of(ProviderId::Desktop) {
        crate::provider::LoginKind::Folder(app) => app.items(),
        crate::provider::LoginKind::Document(_) => unreachable!("a folder tool"),
    }
}

// Claude's folder with no jar and none of an account's other items.
fn without_items(m: &Machine) {
    for item in items() {
        let path = data(m).join(item);
        match std::fs::symlink_metadata(&path) {
            Ok(found) if found.is_dir() => std::fs::remove_dir_all(&path).expect("an item"),
            Ok(_) => std::fs::remove_file(&path).expect("an item"),
            Err(_) => {}
        }
    }
}

fn in_use(m: &Machine) -> Option<String> {
    let state = state::load(&m.ctx).expect("state");
    state
        .account_in_use(ProviderId::Desktop)
        .map(|a| a.label.clone())
}

// No mover exists yet to make a folder park.
fn park_by_hand(m: &Machine, key: Option<&Key>, name: &str, who: &Person) -> Park {
    let folder = crate::store::folder::at(&m.ctx, name);
    std::fs::create_dir_all(folder.join("items")).expect("a park");
    write_stand_in_jar(&folder.join("items/Cookies"), 24, &session_of(Some(who)));
    let keys = format!(
        "{{\"oauth:tokenCache\": \"acct:{0}|t\", \"lastKnownAccountUuid\": \"{0}\"}}",
        who.uuid
    );
    std::fs::write(folder.join("keys.json"), &keys).expect("keys");
    let digests = crate::provider::folder::Keys::from_json(keys.as_bytes())
        .expect("keys")
        .digests();
    let manifest = Manifest {
        v: 1,
        tool: "desktop".into(),
        account: Some(format!("desktop_{}", who.uuid)),
        named: Some(who.uuid.into()),
        login: Some(who.login()),
        session_expires_at: None,
        parked_at: NOW,
        machine: m.ctx.this_machine(),
        items: vec!["Cookies".into()],
        keys: digests,
        left_in_folder: Vec::new(),
    };
    std::fs::write(
        folder.join("manifest.json"),
        serde_json::to_vec(&manifest).expect("JSON"),
    )
    .expect("manifest");
    let park = Park {
        service: name.into(),
        kind: ParkKind::Folder,
        parked_at: NOW,
        login: who.login(),
        access_expires_at: None,
        refresh_expires_at: None,
        session_expires_at: None,
    };
    if let Some(key) = key {
        let mut state = state::load(&m.ctx).expect("state");
        state.park(key, park.clone());
        state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
    }
    park
}

fn park_name(who: &Person) -> String {
    crate::park::service_name(&format!("desktop_{}", who.uuid), NOW * 1000)
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn the_account_claude_is_signed_in_to_is_enrolled_and_nothing_moves() {
    let m = desktop_machine("enrolled");
    let twin = m.ctx_home().join("twin");
    plant(&twin, &WORK, JarAs::StandIn);
    assert_eq!(
        tree(&data(&m)),
        tree(&twin),
        "the enrolment moved nothing of Claude's"
    );
    let state = state::load(&m.ctx).expect("state");
    let work = state.get(&m.key("work")).expect("enrolled");
    assert_eq!(work.id, format!("desktop_{}", WORK.uuid));
    assert_eq!(work.email, "");
    assert_eq!(in_use(&m).as_deref(), Some("work"));
    let record = state.in_use(ProviderId::Desktop).expect("recorded");
    assert_eq!(record.login, WORK.login());
    assert_eq!(record.known_at, NOW);
    let before = snapshot(&m);
    assert!(matches!(
        enrol(&m, "work").expect("enrolled again"),
        (super::Enrolled::Current { .. }, _)
    ));
    assert_eq!(snapshot(&m).2, before.2, "nothing of Claude's moved");
    assert!(crate::store::folder::list(&m.ctx).expect("parks").is_none());
    hold(&m, "enrolled");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn use_of_an_account_with_nothing_parked_is_refused_and_the_one_in_use_is_not_forgotten() {
    let m = two_accounts("use");
    assert!(matches!(
        use_(&m, "work").expect("in use"),
        Outcome::AlreadyActive {
            config_updated: false,
            ..
        }
    ));
    assert!(matches!(
        confirm_(&m, "work").expect("in use"),
        Outcome::AlreadyActive { .. }
    ));
    let before = snapshot(&m);
    assert_eq!(code(confirm_(&m, "home")), "account_not_in_use");
    assert_eq!(code(use_(&m, "home")), "nothing_parked");
    assert_eq!(code(forget_(&m, "work")), "cannot_forget_active_account");
    assert_eq!(snapshot(&m), before);
    forget_(&m, "home").expect("forgotten");
    assert!(
        state::load(&m.ctx)
            .expect("state")
            .get(&m.key("home"))
            .is_none()
    );
    hold(&m, "forgotten");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_missing_jar_beside_an_item_or_a_key_is_never_signed_out() {
    let m = desktop_machine("no-jar");
    std::fs::remove_file(data(&m).join("Cookies")).expect("the jar taken away by hand");
    let before = snapshot(&m);
    let refused = enrol(&m, "work").expect_err("refused");
    assert_eq!(refused.code(), "login_folder_unknown");
    assert!(
        refused
            .to_string()
            .contains("has no cookie store and still holds `Cookies-journal`"),
        "{refused}"
    );
    assert_eq!(code(use_(&m, "work")), "login_folder_unknown");
    assert_eq!(
        code(forget_(&m, "work")),
        "login_folder_unknown",
        "doubt rule"
    );
    assert_eq!(snapshot(&m), before);

    // No item at all, and the keys still in the settings.
    let m = desktop_machine("no-items");
    without_items(&m);
    let before = snapshot(&m);
    let refused = enrol(&m, "work").expect_err("refused");
    assert_eq!(refused.code(), "login_folder_unknown");
    assert!(
        refused
            .to_string()
            .contains("has no cookie store and still holds `oauth:tokenCache`"),
        "{refused}"
    );
    assert_eq!(code(use_(&m, "work")), "login_folder_unknown");
    assert_eq!(snapshot(&m), before);

    // An item 2.42304.0 clears for another account, and no jar.
    let m = desktop_machine("no-jar-preview");
    without_items(&m);
    for key in [
        "oauth:tokenCache",
        "oauth:tokenCacheV2",
        "lastKnownAccountUuid",
    ] {
        set(&m, key, None);
    }
    std::fs::create_dir(data(&m).join("Partitions")).expect("a preview browser");
    let before = snapshot(&m);
    let refused = enrol(&m, "work").expect_err("refused");
    assert!(
        refused
            .to_string()
            .contains("has no cookie store and still holds `Partitions`"),
        "{refused}"
    );
    assert_eq!(snapshot(&m), before);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
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
    assert_eq!(refused.code(), "login_folder_unknown");
    assert!(
        refused.to_string().contains("does not exist yet"),
        "{refused}"
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
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
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
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_jar_that_does_not_read_this_time_refuses_every_change() {
    let m = two_accounts("unsettled");
    m.mem.jar_unsettled(&data(&m).join("Cookies"));
    let before = snapshot(&m);
    let refused = enrol(&m, "work").expect_err("refused");
    assert_eq!(refused.code(), "login_folder_unreadable");
    assert_eq!(refused.exit_code(), 1);
    assert!(
        refused.to_string().contains("(a test held the jar)"),
        "{refused}"
    );
    assert_eq!(code(use_(&m, "home")), "login_folder_unreadable");
    assert_eq!(code(confirm_(&m, "work")), "login_folder_unreadable");
    assert_eq!(
        code(forget_(&m, "work")),
        "login_folder_unreadable",
        "doubt rule"
    );
    assert_eq!(snapshot(&m), before);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
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
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
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
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
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

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn the_account_claude_names_is_not_filed_without_enroll() {
    // Row 14: the recorded session, named for another enrolled account.
    let m = two_accounts("row-14");
    names(&m, &HOME);
    let row_14 = snapshot(&m);
    for label in ["work", "home"] {
        assert_eq!(code(use_(&m, label)), "login_in_doubt", "14: use {label}");
    }
    assert_eq!(code(confirm_(&m, "work")), "account_not_in_use");
    assert_eq!(
        code(forget_(&m, "home")),
        "login_in_doubt",
        "14: doubt rule"
    );
    assert_eq!(snapshot(&m), row_14);

    // Row 15: the recorded session, named for an account nobody enrolled.
    let m = two_accounts("row-15");
    names(&m, &STRANGER);
    let before = snapshot(&m);
    let refused = use_(&m, "home").expect_err("refused");
    assert_eq!(refused.code(), "login_in_doubt");
    assert_eq!(refused.exit_code(), 3);
    assert!(
        refused
            .to_string()
            .contains("an account Pitboard has not enrolled"),
        "{refused}"
    );
    assert_eq!(snapshot(&m), before);

    // Row 17: a session not seen, named for the account in use, which holds a park.
    let m = two_accounts("row-17");
    park_by_hand(&m, Some(&m.key("work")), &park_name(&WORK), &WORK);
    jar(&m, Some(&STRANGER));
    let before = snapshot(&m);
    let refused = use_(&m, "home").expect_err("refused");
    assert_eq!(refused.code(), "login_in_doubt");
    assert_eq!(refused.exit_code(), 3);
    assert!(
        refused.to_string().contains("outside Pitboard"),
        "{refused}"
    );
    assert_eq!(code(forget_(&m, "work")), "login_in_doubt");
    assert_eq!(snapshot(&m), before);

    // Row 18: a session not seen, named for an enrolled account not in use.
    let m = two_accounts("row-18");
    jar(&m, Some(&STRANGER));
    names(&m, &HOME);
    let before = snapshot(&m);
    assert_eq!(code(use_(&m, "work")), "login_in_doubt");
    assert_eq!(snapshot(&m), before);

    // Row 19: a session not seen, named for an account nobody enrolled.
    let m = two_accounts("row-19");
    jar(&m, Some(&STRANGER));
    names(&m, &STRANGER);
    let before = snapshot(&m);
    assert_eq!(code(use_(&m, "home")), "live_account_not_enrolled");
    assert_eq!(snapshot(&m), before);

    // Row 20: a session a park of this home holds.
    let m = two_accounts("row-20");
    park_by_hand(&m, Some(&m.key("home")), &park_name(&HOME), &HOME);
    jar(&m, Some(&HOME));
    names(&m, &HOME);
    let before = snapshot(&m);
    assert_eq!(code(use_(&m, "home")), "login_in_doubt");
    assert_eq!(snapshot(&m), before);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn enroll_files_what_claude_names_and_sets_an_older_park_aside() {
    // Row 14: filed as the named account's, which is said.
    let m = two_accounts("file-14");
    names(&m, &HOME);
    let before = snapshot(&m);
    assert_eq!(code(enrol(&m, "work")), "label_taken");
    assert_eq!(code(enrol(&m, "other")), "already_enrolled");
    assert_eq!(snapshot(&m), before);
    let (_, warnings) = enrol(&m, "home").expect("filed as `home`'s");
    let taken = warnings
        .iter()
        .find(|w| w.code() == "login_taken_as_named")
        .expect("said");
    assert!(
        taken.to_string().starts_with(
            "Pitboard had recorded the login Claude held as `desktop/work`'s, and filed it as \
             `desktop/home`'s because Claude names that account."
        ),
        "{taken}"
    );
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    let state = state::load(&m.ctx).expect("state");
    assert_eq!(
        state.in_use(ProviderId::Desktop).expect("record").login,
        WORK.login()
    );
    assert!(
        state
            .get(&m.key("work"))
            .expect("kept")
            .replaced_at
            .is_some(),
        "`work`, with no park, is marked replaced"
    );
    hold(&m, "row 14");

    // Row 15: the account Claude names is nobody's yet.
    let m = two_accounts("file-15");
    names(&m, &STRANGER);
    let (_, warnings) = enrol(&m, "third").expect("filed as `third`'s");
    let taken = warnings
        .iter()
        .find(|w| w.code() == "login_taken_as_named")
        .expect("said");
    assert!(
        taken.to_string().starts_with(
            "Pitboard had recorded the login Claude held as `desktop/work`'s, and filed it as \
             `desktop/third`'s because Claude names that account."
        ),
        "{taken}"
    );
    assert_eq!(in_use(&m).as_deref(), Some("third"));
    hold(&m, "row 15");

    // Row 17: the account in use's older park set aside, never deleted.
    let m = two_accounts("file-17");
    let older = park_name(&WORK);
    park_by_hand(&m, Some(&m.key("work")), &older, &WORK);
    jar(&m, Some(&STRANGER));
    let before = snapshot(&m);
    assert_eq!(code(enrol(&m, "home")), "label_taken");
    assert_eq!(code(enrol(&m, "other")), "already_enrolled");
    assert_eq!(snapshot(&m), before);
    let (_, warnings) = enrol(&m, "work").expect("filed");
    let set_aside = warnings
        .iter()
        .find(|w| w.code() == "set_aside")
        .expect("said");
    assert!(
        set_aside.to_string().contains("deleted nothing"),
        "{set_aside}"
    );
    let state = state::load(&m.ctx).expect("state");
    assert!(state.get(&m.key("work")).expect("kept").parked.is_none());
    let aside = state
        .aside
        .iter()
        .find(|a| a.name == older)
        .expect("set aside");
    assert_eq!(aside.why, SetAside::Superseded);
    assert_eq!(
        aside.account.as_deref(),
        Some(format!("desktop_{}", WORK.uuid).as_str())
    );
    assert!(
        crate::store::folder::at(&m.ctx, &older)
            .join("items/Cookies")
            .exists()
    );
    assert_eq!(
        state.in_use(ProviderId::Desktop).expect("record").login,
        STRANGER.login()
    );
    assert!(!state.discarded.iter().any(|d| d.service == older));
    hold(&m, "row 17");

    // Row 18: the named account's park set aside, the session filed under it.
    let m = two_accounts("file-18");
    let homes = park_name(&HOME);
    park_by_hand(&m, Some(&m.key("home")), &homes, &HOME);
    jar(&m, Some(&STRANGER));
    names(&m, &HOME);
    let (_, warnings) = enrol(&m, "home").expect("filed");
    assert!(
        warnings.iter().any(|w| w.code() == "set_aside"),
        "{warnings:?}"
    );
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    assert!(
        state::load(&m.ctx)
            .expect("state")
            .aside
            .iter()
            .any(|a| a.name == homes)
    );

    // Row 20: the park that holds the same session set aside, the session filed.
    let m = two_accounts("file-20");
    let homes = park_name(&HOME);
    park_by_hand(&m, Some(&m.key("home")), &homes, &HOME);
    jar(&m, Some(&HOME));
    names(&m, &HOME);
    enrol(&m, "home").expect("filed");
    let state = state::load(&m.ctx).expect("state");
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    assert!(state.aside.iter().any(|a| a.name == homes));
    assert!(state.get(&m.key("home")).expect("kept").parked.is_none());
    hold_where_twice(&m, "row 20");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn nothing_is_read_or_changed_while_claude_runs_or_cannot_be_told() {
    let m = two_accounts("open");
    let claude = m.claude();
    claude.opens();
    let before = snapshot(&m);
    let asked = m.mem.jars_asked().len();
    assert_eq!(code(enrol(&m, "work")), "app_open");
    assert_eq!(code(use_(&m, "home")), "app_open");
    assert_eq!(code(confirm_(&m, "work")), "app_open");
    assert_eq!(
        code(forget_(&m, "work")),
        "app_open",
        "the account in use, by the doubt rule"
    );
    assert_eq!(m.mem.jars_asked().len(), asked, "no jar was opened");
    assert_eq!(snapshot(&m), before);

    claude.crashes();
    m.mem.without_a_process_list();
    assert_eq!(code(enrol(&m, "work")), "app_state_unknown");
    assert_eq!(code(use_(&m, "home")), "app_state_unknown");
    assert_eq!(m.mem.jars_asked().len(), asked);
    assert_eq!(snapshot(&m), before);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_refusal_names_what_of_claude_still_runs() {
    let m = desktop_machine("still");
    let claude = m.claude();
    claude.opens();
    let refused = enrol(&m, "work").expect_err("refused");
    assert!(
        matches!(&refused, Error::AppOpen { still, .. } if still.is_empty()),
        "{refused:?}"
    );
    assert!(
        refused.to_string().starts_with("Claude is open"),
        "{refused}"
    );

    claude.crashes();
    let ship_it = m
        .ctx_home()
        .join("claude/Claude.app/Contents/Frameworks/Squirrel.framework/Resources/ShipIt");
    m.mem
        .runs_at("ShipIt", &[ship_it.to_string_lossy().as_ref()]);
    let refused = enrol(&m, "work").expect_err("refused");
    let Error::AppOpen { still, .. } = &refused else {
        panic!("{refused:?}");
    };
    assert_eq!(still.len(), 1);
    assert!(still[0].starts_with("ShipIt (pid "), "{still:?}");
    assert!(refused.to_string().contains("ShipIt (pid"), "{refused}");
    assert!(
        refused.to_string().contains("Activity Monitor"),
        "{refused}"
    );
    assert_eq!(refused.exit_code(), 1);

    // A Claude that is not the test's own never counts.
    m.mem.runs_at("ShipIt", &[]);
    m.mem.runs_at(
        "Claude",
        &["/Applications/Claude.app/Contents/MacOS/Claude"],
    );
    enrol(&m, "work").expect("the developer's own Claude is not the test's");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_process_list_that_does_not_read_is_refused_with_why() {
    let m = desktop_machine("listless");
    m.mem.without_a_process_list();
    let refused = enrol(&m, "work").expect_err("refused");
    assert_eq!(refused.code(), "app_state_unknown");
    assert!(
        refused
            .to_string()
            .contains("(a test withheld the process list)"),
        "{refused}"
    );
    assert!(!refused.to_string().contains("doctor"), "{refused}");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_second_home_is_refused_while_another_holds_the_folder() {
    let m = two_accounts("busy");
    let held = crate::host::fs::lock_dir(&data(&m)).expect("another Pitboard holds it");
    let before = snapshot(&m);
    let refused = enrol(&m, "work").expect_err("refused");
    assert_eq!(refused.code(), "login_folder_busy");
    assert_eq!(code(use_(&m, "home")), "login_folder_busy");
    assert_eq!(snapshot(&m), before);
    held.unlock().expect("let go");
    enrol(&m, "work").expect("once the other has finished");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_sidecar_pitboard_cannot_read_refuses_every_change() {
    let m = desktop_machine("sidecar");
    std::fs::write(data(&m).join("config.json.journal"), "not a header\n{}").expect("sidecar");
    let before = snapshot(&m);
    let refused = enrol(&m, "work").expect_err("refused");
    assert_eq!(refused.code(), "login_folder_unknown");
    assert_eq!(refused.exit_code(), 3);
    assert!(
        refused.to_string().contains("config.json.journal"),
        "{refused}"
    );
    assert_eq!(snapshot(&m), before);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn repair_gives_a_folder_back_where_its_manifest_its_keys_and_its_jar_agree() {
    let given = |name: &str, spoil: &dyn Fn(&Machine, &str)| {
        let m = two_accounts(name);
        let homes = park_name(&HOME);
        park_by_hand(&m, None, &homes, &HOME);
        spoil(&m, &homes);
        let (reclaimed, _) = repair(settled(&m)).expect("repaired");
        let state = state::load(&m.ctx).expect("state");
        let parked = state
            .get(&m.key("home"))
            .and_then(|a| a.parked.clone())
            .map(|p| p.service);
        assert!(
            crate::store::folder::at(&m.ctx, &homes).exists(),
            "{name}: never deleted"
        );
        hold(&m, name);
        (parked == Some(homes.clone()), reclaimed)
    };
    let (back, reclaimed) = given("repair-agrees", &|_, _| {});
    assert!(back, "{reclaimed:?}");
    // A listed item that is not under `items/`, the jar still there.
    let (back, reclaimed) = given("repair-item-gone", &|m, name| {
        let path = crate::store::folder::at(&m.ctx, name).join("manifest.json");
        let mut manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("manifest")).expect("JSON");
        manifest["items"] = json!(["Cookies", "Local Storage"]);
        std::fs::write(&path, manifest.to_string()).expect("rewritten");
    });
    assert!(!back && reclaimed.set_aside.len() == 1, "{reclaimed:?}");
    // Another token cache under the same name: the manifest's digests are what differs.
    let (back, reclaimed) = given("repair-keys-differ", &|m, name| {
        std::fs::write(
            crate::store::folder::keys_file(&m.ctx, name),
            format!(
                "{{\"oauth:tokenCache\": \"other\", \"lastKnownAccountUuid\": \"{}\"}}",
                HOME.uuid
            ),
        )
        .expect("keys");
    });
    assert!(!back && reclaimed.set_aside.len() == 1, "{reclaimed:?}");
    // Parked on another Mac, whose key Claude's jar was written under.
    let (back, reclaimed) = given("repair-another-mac", &|m, name| {
        let path = crate::store::folder::at(&m.ctx, name).join("manifest.json");
        let mut manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("manifest")).expect("JSON");
        manifest["machine"] = json!("another Mac");
        std::fs::write(&path, manifest.to_string()).expect("rewritten");
    });
    assert!(!back && reclaimed.set_aside.len() == 1, "{reclaimed:?}");
    // The jar holds another session than the manifest says: never the wrong account's.
    let (back, reclaimed) = given("repair-jar-differs", &|m, name| {
        let jar = crate::store::folder::item(&m.ctx, name, "Cookies");
        write_stand_in_jar(&jar, 24, &session_of(Some(&STRANGER)));
    });
    assert!(!back && reclaimed.set_aside.len() == 1, "{reclaimed:?}");
    // No keys file is a folder nothing names, not one that could not be read.
    let (back, reclaimed) = given("repair-no-keys", &|m, name| {
        std::fs::remove_file(crate::store::folder::keys_file(&m.ctx, name)).expect("keys");
    });
    assert!(!back && reclaimed.set_aside.len() == 1, "{reclaimed:?}");
    assert!(reclaimed.unreadable.is_empty(), "{reclaimed:?}");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn repair_gives_nothing_back_to_the_account_in_use() {
    // Row 17, then `enroll`: the older login is set aside, and `repair` leaves it there.
    let m = two_accounts("repair-row-17");
    let older = park_name(&WORK);
    park_by_hand(&m, Some(&m.key("work")), &older, &WORK_BEFORE);
    jar(&m, Some(&STRANGER));
    enrol(&m, "work").expect("filed");
    let (reclaimed, _) = repair(settled(&m)).expect("repaired");
    assert!(reclaimed.is_empty(), "{reclaimed:?}");
    let state = state::load(&m.ctx).expect("state");
    assert!(state.get(&m.key("work")).expect("kept").parked.is_none());
    let aside = state.aside.iter().find(|a| a.name == older);
    assert_eq!(aside.map(|a| a.why), Some(SetAside::Superseded));
    assert_eq!(in_use(&m).as_deref(), Some("work"));
    hold(&m, "row 17, enrolled, repaired");

    // A folder nothing names, of the account in use: set aside, whatever session it holds.
    let m = two_accounts("repair-in-use");
    park_by_hand(&m, None, &older, &WORK_BEFORE);
    let (reclaimed, _) = repair(settled(&m)).expect("repaired");
    assert!(reclaimed.given_back.is_empty(), "{reclaimed:?}");
    assert_eq!(reclaimed.set_aside, [older.as_str()]);
    let state = state::load(&m.ctx).expect("state");
    assert!(state.get(&m.key("work")).expect("kept").parked.is_none());
    let aside = state.aside.iter().find(|a| a.name == older).expect("aside");
    assert_eq!(aside.why, SetAside::Orphan);
    assert_eq!(aside.login, Some(WORK_BEFORE.login()));
    hold(&m, "an older login of the account in use");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn an_aside_set_aside_while_the_machine_could_not_be_read_is_given_back() {
    let m = two_accounts("unknown-machine");
    let homes = park_name(&HOME);
    let park = park_by_hand(&m, None, &homes, &HOME);
    let mut state = state::load(&m.ctx).expect("state");
    state.aside.push(state::Aside {
        name: homes.clone(),
        tool: ProviderId::Desktop,
        at: NOW,
        why: SetAside::AnotherMachine,
        account: Some(format!("desktop_{}", HOME.uuid)),
        login: Some(park.login.clone()),
    });
    state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");

    let blind = m.ctx.clone().with_unknown_machine();
    let (unknown, _) = settle(&blind, Permit::for_a_test(), None).expect("settled");
    repair(unknown).expect("repaired");
    let state = state::load(&m.ctx).expect("state");
    assert!(state.get(&m.key("home")).expect("kept").parked.is_none());
    assert!(state.aside.iter().any(|a| a.name == homes), "kept aside");

    repair(settled(&m)).expect("repaired");
    let state = state::load(&m.ctx).expect("state");
    assert_eq!(
        state
            .get(&m.key("home"))
            .and_then(|a| a.parked.as_ref())
            .map(|p| p.service.as_str()),
        Some(homes.as_str())
    );
    assert!(!state.aside.iter().any(|a| a.name == homes));
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn repair_gives_an_account_its_park_back_where_the_one_recorded_is_gone() {
    // A backup of `state.json` from while `home` was parked, restored once it was parked again.
    let restored = |name: &str| {
        let m = desktop_pair(name);
        let file = crate::home::dir(&m.ctx).join("state.json");
        let backup = std::fs::read(&file).expect("a backup");
        let recorded = park_of(&m, "home").expect("parked").service;
        use_(&m, "home").expect("`home` in use");
        m.ctx.clock.pause(1);
        use_(&m, "work").expect("`work` in use again");
        recover(&m).expect("swept");
        let again = park_of(&m, "home").expect("parked again").service;
        assert_ne!(again, recorded);
        assert!(!crate::store::folder::at(&m.ctx, &recorded).exists());
        std::fs::write(&file, backup).expect("restored");
        (m, recorded, again)
    };
    let given_back = |m: &Machine, recorded: &str, again: &str, why: &str| {
        let state = state_of(m);
        assert_eq!(
            park_of(m, "home").map(|p| p.service).as_deref(),
            Some(again),
            "{why}"
        );
        assert!(!state.names(recorded), "{why}: the gone park is let go");
        assert!(state.aside.is_empty(), "{why}: {:?}", state.aside);
        use_(m, "home").unwrap_or_else(|e| panic!("{why}: use home: {e}"));
        assert_eq!(whereabouts(m, &HOME), ["Claude"], "{why}");
        rested(m, why);
    };

    let (m, recorded, again) = restored("repair-restored");
    let (reclaimed, _) = repair(settled(&m)).expect("repaired");
    assert!(reclaimed.set_aside.is_empty(), "{reclaimed:?}");
    assert_eq!(
        reclaimed.given_back,
        [("desktop/home".to_string(), again.clone())]
    );
    given_back(&m, &recorded, &again, "found under `parks`");

    // What an earlier `repair` left: the folder set aside under `home`.
    let (m, recorded, again) = restored("repair-restored-aside");
    let mut state = state_of(&m);
    state.aside.push(state::Aside {
        name: again.clone(),
        tool: ProviderId::Desktop,
        at: NOW,
        why: SetAside::Orphan,
        account: Some(format!("desktop_{}", HOME.uuid)),
        login: Some(HOME.login()),
    });
    state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
    repair(settled(&m)).expect("repaired");
    given_back(&m, &recorded, &again, "set aside");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn repair_never_lets_go_of_a_park_a_move_holds() {
    // An arrive of `home` waits with Claude open, its park deleted by hand, and another login
    // of `home` is found under `parks`.
    let m = desktop_pair("repair-move-holds");
    killed(&m, "arrive.recorded", "home");
    let held = park_of(&m, "home").expect("parked").service;
    m.claude().opens();
    std::fs::remove_dir_all(crate::store::folder::at(&m.ctx, &held)).expect("deleted by hand");
    let found = crate::park::service_name(&format!("desktop_{}", HOME.uuid), (NOW + 60) * 1000);
    park_by_hand(&m, None, &found, &HOME);

    let (reclaimed, warnings) = repair(settled(&m)).expect("repaired");

    assert!(
        matches!(warnings.as_slice(), [Warning::MoveNotSettled { .. }]),
        "{warnings:?}"
    );
    assert!(reclaimed.given_back.is_empty(), "{reclaimed:?}");
    assert_eq!(reclaimed.set_aside, [found.as_str()]);
    let state = state_of(&m);
    assert_eq!(park_of(&m, "home").map(|p| p.service), Some(held.clone()));
    assert!(state.moves(&held));
    assert!(!state.discarded.iter().any(|d| d.service == held));
    assert!(state.aside.iter().any(|aside| aside.name == found));
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_name_that_is_no_account_id_is_never_enrolled() {
    for (at, name, refusal, exit) in [
        (0, "", "sign_in_unfinished", 1),
        (1, "../x", "login_folder_unknown", 3),
        (2, "a b", "login_folder_unknown", 3),
    ] {
        let m = two_accounts(&format!("odd-name-{at}"));
        jar(&m, Some(&STRANGER));
        set(&m, "lastKnownAccountUuid", Some(json!(name)));
        let before = snapshot(&m);
        let refused = enrol(&m, "other").expect_err("refused");
        assert_eq!(
            (refused.code(), refused.exit_code()),
            (refusal, exit),
            "{name:?}: {refused}"
        );
        if exit == 3 {
            assert!(
                refused
                    .to_string()
                    .contains("whose `lastKnownAccountUuid` is not an account id"),
                "{refused}"
            );
        }
        assert_eq!(code(use_(&m, "home")), refusal, "{name:?}");
        assert_eq!(snapshot(&m), before, "{name:?}");
    }
}

// Row 4.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_data_folder_pitboard_moves_no_login_in_refuses_every_change() {
    let refuses = |name: &str, spoil: &dyn Fn(&Machine), words: &str| {
        let m = two_accounts(&format!("row-4-{name}"));
        spoil(&m);
        let before = snapshot(&m);
        let refused = enrol(&m, "work").expect_err("refused");
        assert_eq!(
            (refused.code(), refused.exit_code()),
            ("login_folder_unknown", 3),
            "{name}: {refused}"
        );
        assert!(refused.to_string().contains(words), "{refused}");
        assert_eq!(code(use_(&m, "home")), "login_folder_unknown", "{name}");
        assert_eq!(code(confirm_(&m, "work")), "login_folder_unknown", "{name}");
        assert_eq!(
            code(forget_(&m, "work")),
            "login_folder_unknown",
            "{name}: doubt rule"
        );
        assert_eq!(snapshot(&m), before, "{name}");
    };
    refuses(
        "third-party",
        &|m| {
            std::fs::create_dir_all(m.ctx_home().join("claude/data-3p")).expect("a folder beside");
        },
        "has a `Claude-3p` folder beside it, which a third-party deployment of Claude uses",
    );
    refuses(
        "reset",
        &|m| {
            let asked = br#"{"choice":"restart","tries":0}"#;
            std::fs::write(data(m).join("reset-app-data"), asked).expect("a reset asked for");
        },
        "is waiting for Reset App Data, which deletes what it holds the next time Claude opens, \
         so Pitboard moves no Claude Desktop login. Open Claude, quit it with Command-Q, then run \
         this again.",
    );
    refuses(
        "link",
        &|m| {
            let real = m.ctx_home().join("claude/real");
            std::fs::rename(data(m), &real).expect("moved");
            crate::host::fs::testing::link_dir(&real, &data(m)).expect("linked");
        },
        "data folder is a link",
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_reset_waiting_stops_the_recovery_of_a_move_as_it_stops_a_move() {
    let m = desktop_pair("reset-mid-move");
    killed(&m, "leave.jar_out", "home");
    let asked = br#"{"choice":"restart","tries":0}"#;
    std::fs::write(data(&m).join("reset-app-data"), asked).expect("a reset asked for");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let stopped = refused(&m, "home");
    assert_eq!(stopped.code(), "login_folder_unknown", "{stopped}");
    let said = stopped.to_string();
    assert!(said.contains("is waiting for Reset App Data"), "{said}");
    assert!(
        said.contains("Open Claude, quit it with Command-Q"),
        "{said}"
    );
    unchanged(&m, &before, "a reset waiting");
    assert!(
        !state_of(&m).moving.is_empty(),
        "the move waits for its recovery"
    );

    std::fs::write(data(&m).join("reset-app-data.given-up"), b"").expect("given up");
    used(&m, "home");
    assert!(
        state_of(&m).moving.is_empty(),
        "settled once Claude gave the reset up"
    );
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    hold(&m, "a reset given up");
    rested(&m, "a reset given up");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
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

    let removed = super::super::uninstall(settled(&m)).expect("uninstalled");
    assert_eq!(
        (removed.set_aside, removed.parks, removed.pending),
        (2, 0, 0)
    );
    assert!(removed.home_removed);
    assert!(!crate::home::dir(&m.ctx).exists());
    assert_eq!(tree(&data(&m)), claudes, "Claude's folder is as it was");
}

#[test]
fn the_desktop_provider_reaches_no_keychain_and_no_service() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for dir in ["provider/desktop", "switch/folder"] {
        for entry in std::fs::read_dir(root.join(dir)).expect("sources") {
            let path = entry.expect("an entry").path();
            if path
                .file_name()
                .is_some_and(|name| name == "tests.rs" || name == "assumptions.rs")
            {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("a source");
            for line in source
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
            {
                for needle in [
                    "security",
                    "keychain",
                    "vault_",
                    "ureq",
                    "crate::api",
                    "crate::proxy",
                    "names::",
                ] {
                    assert!(
                        !line.contains(needle),
                        "{}: `{needle}` in {line:?}",
                        path.display()
                    );
                }
                if line.contains("crate::store::") || line.contains("use crate::store") {
                    assert!(
                        line.contains("store::fingerprint") || line.contains("store::folder"),
                        "{}: {line:?}",
                        path.display()
                    );
                }
            }
        }
    }
}

// Without printing two indexes and two folders where they differ.
fn used(m: &Machine, label: &str) -> (Outcome, Vec<Warning>) {
    switch(settled(m), &m.key(label)).unwrap_or_else(|e| panic!("use {label}: {e}"))
}

fn park_of(m: &Machine, label: &str) -> Option<Park> {
    state::load(&m.ctx)
        .expect("state")
        .get(&m.key(label))
        .and_then(|a| a.parked.clone())
}

fn named_in_settings(m: &Machine) -> Option<String> {
    setting(m, "lastKnownAccountUuid").and_then(|name| name.as_str().map(str::to_string))
}

fn setting(m: &Machine, key: &str) -> Option<Value> {
    let mut settings: Map<String, Value> = serde_json::from_str(
        &std::fs::read_to_string(data(m).join("config.json")).expect("settings"),
    )
    .expect("JSON");
    settings.shift_remove(key)
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_switch_parks_the_account_in_use_and_puts_the_other_in() {
    let m = desktop_pair("switch");
    assert_eq!(in_use(&m).as_deref(), Some("work"));
    assert_eq!(whereabouts(&m, &WORK), ["Claude"]);
    let home_park = park_of(&m, "home").expect("`home` is parked");
    assert_eq!(whereabouts(&m, &HOME), vec![home_park.service.clone()]);
    assert_eq!(home_park.kind, ParkKind::Folder);
    assert!(home_park.session_expires_at.is_some());
    let inode = |path: PathBuf| crate::host::fs::testing::inode(&path).expect("there");
    let jar_before = inode(data(&m).join("Cookies"));

    let (outcome, warnings) = used(&m, "home");
    let Outcome::Switched {
        from,
        to,
        parked,
        adoption,
        ..
    } = outcome
    else {
        panic!("switched: {outcome:?}");
    };
    assert_eq!(
        (from.as_str(), to.as_str()),
        ("desktop/work", "desktop/home")
    );
    assert_eq!(
        adoption,
        crate::provider::Adoption::NextOpen { app: "Claude" }
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    assert_eq!(whereabouts(&m, &HOME), ["Claude"]);
    assert_eq!(whereabouts(&m, &WORK), vec![parked.service.clone()]);
    assert_eq!(named_in_settings(&m).as_deref(), Some(HOME.uuid));
    assert_eq!(
        inode(crate::store::folder::item(
            &m.ctx,
            &parked.service,
            "Cookies"
        )),
        jar_before,
        "moved by rename, never copied"
    );
    assert!(
        !crate::store::folder::at(&m.ctx, &home_park.service).exists(),
        "the emptied park is removed"
    );
    assert!(park_of(&m, "home").is_none());
    hold(&m, "a switch");
    rested(&m, "a switch");

    assert!(matches!(
        used(&m, "home").0,
        Outcome::AlreadyActive {
            config_updated: false,
            ..
        }
    ));
    used(&m, "work");
    assert_eq!(in_use(&m).as_deref(), Some("work"));
    assert_eq!(
        tree(&data(&m).join("Local Storage")),
        tree(&m.ctx_home().join("claude/data/Local Storage"))
    );
    hold(&m, "and back");
    rested(&m, "and back");
}

// What 2.42304.0 clears when another account signs in: a switch hides that change from it,
// so it carries them instead.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_switch_carries_what_claude_clears_for_another_account() {
    let m = desktop_pair("carried");
    let preview = data(&m).join("Partitions/launch-preview-static/Cookies");
    std::fs::create_dir_all(preview.parent().expect("a partition")).expect("a partition");
    std::fs::write(&preview, "work's preview").expect("a preview jar");
    std::fs::create_dir(data(&m).join("builtin-mcp-auth")).expect("a connector cache");
    for file in [
        "custom3p-builtin-mcp.json",
        "custom3p-builtin-mcp.json.journal",
        "chrome-import-sites.json",
        "session-popouts.json",
    ] {
        std::fs::write(data(&m).join(file), "{}").expect("a file of work's");
    }
    set(&m, "custom3pMcpOAuth", Some(json!({"grant": "work's"})));
    let carried = [
        "Partitions",
        "builtin-mcp-auth",
        "custom3p-builtin-mcp.json",
        "custom3p-builtin-mcp.json.journal",
        "chrome-import-sites.json",
        "session-popouts.json",
    ];

    // `home`'s park holds none of them, as one an earlier Pitboard made.
    let (_, warnings) = used(&m, "home");
    assert!(warnings.is_empty(), "{warnings:?}");
    for item in carried {
        assert!(!data(&m).join(item).exists(), "`{item}` left with `work`");
    }
    assert_eq!(setting(&m, "custom3pMcpOAuth"), None);
    hold(&m, "a switch");
    rested(&m, "a switch");

    used(&m, "work");
    for item in carried {
        assert!(data(&m).join(item).exists(), "`{item}` back with `work`");
    }
    assert_eq!(
        std::fs::read_to_string(&preview).expect("back"),
        "work's preview"
    );
    assert_eq!(
        setting(&m, "custom3pMcpOAuth"),
        Some(json!({"grant": "work's"}))
    );
    hold(&m, "and back");
    rested(&m, "and back");
}

// A leave stopped after the keys went, then Claude run signed out beside grants it kept.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_leave_finished_beside_connector_grants_verifies_and_commits() {
    let m = desktop_pair("grants-settled");
    killed(&m, "leave.item_out", "home");
    let claude = m.claude();
    claude.opens();
    claude.quits();
    set(&m, "custom3pMcpOAuth", Some(json!({"grant": "kept"})));

    let gave = gave_up(&m).expect("settled");
    assert!(gave.moves.is_empty(), "nothing given up: {gave:?}");
    assert!(
        matches!(
            gave.warnings.as_slice(),
            [Warning::MoveSettled(moved)]
                if moved.half == state::Half::Leave && moved.ended == super::Ended::Finished
        ),
        "{:?}",
        gave.warnings
    );
    assert_eq!(in_use(&m), None);
}

// A Log out leaves the connector grants, which hold no login of Claude's.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
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

// A switch Pitboard makes writes no `in-use` line. One into a Claude signed out since writes
// who left and whose only login went, as a read of any other tool would.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_use_into_a_signed_out_claude_says_who_left_in_the_activity_log() {
    let m = desktop_pair("use-signed-out");
    let before = audit_lines(&m, "in-use");
    used(&m, "home");
    assert_eq!(audit_lines(&m, "in-use"), before, "Pitboard's own switch");
    let claude = m.claude();
    claude.opens();
    claude.logs_out();
    claude.quits();
    used(&m, "work");
    let lines = audit_lines(&m, "in-use");
    assert_eq!(
        lines[before.len()..],
        [
            (String::new(), "signed_out_outside".to_string()),
            ("desktop/home".to_string(), "login_replaced".to_string()),
        ]
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn the_keys_leave_before_the_jar_and_arrive_after_it() {
    let m = desktop_pair("order");
    set(&m, "custom3pMcpOAuth", Some(json!({"grant": "work's"})));
    used(&m, "home");
    set(&m, "custom3pMcpOAuth", Some(json!({"grant": "home's"})));
    used(&m, "work");
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    // How many of the account's keys the settings hold.
    let look = |at: &'static str| {
        let (seen, root) = (std::rc::Rc::clone(&seen), data(&m));
        move || {
            let settings: Map<String, Value> = serde_json::from_str(
                &std::fs::read_to_string(root.join("config.json")).expect("settings"),
            )
            .expect("JSON");
            let keys = [
                "oauth:tokenCache",
                "oauth:tokenCacheV2",
                "lastKnownAccountUuid",
                "custom3pMcpOAuth",
            ]
            .into_iter()
            .filter(|key| settings.contains_key(*key))
            .count();
            seen.borrow_mut()
                .push((at, keys, root.join("Cookies").exists()));
        }
    };
    // `meanwhile` holds one hook at a time, so each point is looked at in a run of its own.
    for at in [
        "leave.keys_out",
        "leave.jar_out",
        "arrive.jar_in",
        "arrive.keys_in",
    ] {
        crate::fault::meanwhile(at, look(at), || used(&m, "home"));
        used(&m, "work");
    }
    assert_eq!(
        *seen.borrow(),
        [
            ("leave.keys_out", 0, true),
            ("leave.jar_out", 0, false),
            ("arrive.jar_in", 0, true),
            ("arrive.keys_in", 4, true),
        ]
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_session_under_the_recorded_name_is_parked_as_claude_names_it() {
    let m = desktop_pair("as-named");
    let claude = m.claude();
    claude.opens();
    claude.renews(&WORK, "work-session-renewed");
    claude.quits();
    let renewed = Person {
        uuid: WORK.uuid,
        session: "work-session-renewed",
    };

    let (outcome, warnings) = used(&m, "home");
    assert!(matches!(outcome, Outcome::Switched { .. }), "{outcome:?}");
    assert!(
        matches!(
            warnings.as_slice(),
            [Warning::TakenAsNamed { label, .. }] if label == "desktop/work"
        ),
        "{warnings:?}"
    );
    assert!(
        warnings[0]
            .to_string()
            .starts_with("Pitboard had not seen the login Claude held"),
        "{}",
        warnings[0]
    );
    let parked = park_of(&m, "work").expect("parked");
    assert_eq!(parked.login, renewed.login(), "under the fingerprint found");
    assert_eq!(whereabouts(&m, &renewed), [parked.service]);
    assert!(state::load(&m.ctx).expect("state").aside.is_empty());
    hold(&m, "row 16");
    rested(&m, "row 16");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_sign_in_parks_the_account_in_use_and_enrols_whoever_signs_in() {
    let m = desktop_machine("sign-in-inside");
    let (enrolled, warnings) = sign_in_inside(&m, "home", &HOME).expect("added");
    assert!(
        matches!(enrolled, super::Enrolled::InUse { again: false, .. }),
        "{enrolled:?}"
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    let parked = park_of(&m, "work").expect("`work` was parked for it");
    assert_eq!(whereabouts(&m, &WORK), [parked.service]);
    assert_eq!(whereabouts(&m, &HOME), ["Claude"]);
    let state = state::load(&m.ctx).expect("state");
    assert!(state.awaiting.is_empty() && state.moving.is_empty());
    hold(&m, "a sign-in inside Claude");
    rested(&m, "a sign-in inside Claude");
}

fn refused(m: &Machine, label: &str) -> Error {
    switch(settled(m), &m.key(label)).expect_err("refused")
}

fn state_of(m: &Machine) -> state::State {
    state::load(&m.ctx).expect("state")
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_login_is_never_moved_over_anything() {
    // `work` holds no `File System`; `home`'s park is made to, so the arrive needs the place.
    let with_a_folder = |name: &str| {
        let m = desktop_pair(name);
        let home_park = park_of(&m, "home").expect("parked");
        let listed = crate::store::folder::item(&m.ctx, &home_park.service, "File System");
        std::fs::create_dir_all(&listed).expect("an item");
        let mut manifest = crate::store::folder::manifest(&m.ctx, &home_park.service)
            .expect("reads")
            .expect("there");
        manifest.items.push("File System".into());
        crate::store::folder::write_manifest(
            &m.ctx,
            Permit::for_a_test(),
            &home_park.service,
            &manifest,
        )
        .expect("written");
        (m, listed)
    };

    // In the way before the arrive starts: its look refuses it, and nothing of `home` moves.
    let (m, listed) = with_a_folder("never-over");
    let in_the_way = data(&m).join("File System");
    let failed = crate::fault::meanwhile(
        "leave.done",
        {
            let in_the_way = in_the_way.clone();
            move || std::fs::write(&in_the_way, b"planted meanwhile").expect("planted")
        },
        || refused(&m, "home"),
    );
    assert_eq!(failed.code(), "switch_left_signed_out", "{failed}");
    assert_eq!(failed.exit_code(), 1);
    assert_eq!(
        std::fs::read(&in_the_way).expect("still there"),
        b"planted meanwhile"
    );
    assert!(listed.is_dir(), "`home`'s item stayed in its park");
    assert_eq!(in_use(&m), None, "Claude is signed out");
    assert!(park_of(&m, "work").is_some() && park_of(&m, "home").is_some());
    assert!(state_of(&m).moving.is_empty());
    hold(&m, "an arrive that did not start");
    std::fs::remove_file(&in_the_way).expect("out of the way");
    assert!(matches!(used(&m, "home").0, Outcome::Installed { .. }));
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    rested(&m, "arrived");

    // An empty folder made after every look, which a plain rename would replace unseen.
    let (m, listed) = with_a_folder("never-over-a-folder");
    let in_the_way = data(&m).join("File System");
    let planted = std::rc::Rc::new(std::cell::Cell::new(None));
    let failed = crate::fault::meanwhile(
        "arrive.recorded",
        {
            let (in_the_way, planted) = (in_the_way.clone(), std::rc::Rc::clone(&planted));
            move || {
                std::fs::create_dir(&in_the_way).expect("planted");
                planted.set(crate::host::fs::testing::inode(&in_the_way).ok());
            }
        },
        || refused(&m, "home"),
    );
    let planted = planted.get().expect("planted");
    assert_eq!(failed.code(), "switch_left_signed_out", "{failed}");
    assert!(
        failed
            .to_string()
            .contains("`File System` could not be moved"),
        "{failed}"
    );
    assert_eq!(
        crate::host::fs::testing::inode(&in_the_way).ok(),
        Some(planted),
        "the folder in the way is the one planted"
    );
    assert!(listed.is_dir(), "`home`'s item stayed in its park");
    assert_eq!(in_use(&m), None, "Claude is signed out");
    assert!(state_of(&m).moving.is_empty());
    hold(&m, "an arrive taken back");
    std::fs::remove_dir(&in_the_way).expect("out of the way");
    assert!(matches!(used(&m, "home").0, Outcome::Installed { .. }));
    rested(&m, "arrived over nothing");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_part_of_the_accounts_data_pitboard_has_not_read_stops_the_start_of_a_move_and_not_its_recovery()
 {
    let m = desktop_pair("row-21");
    std::fs::create_dir(data(&m).join("Network")).expect("the jar's other place");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let stopped = refused(&m, "home");
    assert_eq!(stopped.code(), "login_folder_unknown");
    assert_eq!(stopped.exit_code(), 3);
    assert!(stopped.to_string().contains("holds `Network`"), "{stopped}");
    unchanged(&m, &before, "nothing changed");
    assert!(matches!(used(&m, "work").0, Outcome::AlreadyActive { .. }));

    std::fs::remove_dir(data(&m).join("Network")).expect("gone");
    let died = crate::fault::killing("leave.jar_out", || used(&m, "home"));
    assert_eq!(died.expect_err("killed"), "leave.jar_out");
    std::fs::create_dir(data(&m).join("Network")).expect("there again");
    let stopped = refused(&m, "home");
    assert_eq!(
        stopped.code(),
        "login_folder_unknown",
        "the leave was finished first"
    );
    let state = state_of(&m);
    assert!(state.moving.is_empty());
    assert_eq!(in_use(&m), None);
    assert!(park_of(&m, "work").is_some());
    hold(&m, "recovered beside an entry not read");
    rested(&m, "recovered beside an entry not read");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_name_pitboard_does_not_move_stays_where_claude_keeps_it() {
    let m = desktop_pair("names-stay");
    let geometry = br#"{"width":1200,"height":800,"x":0,"y":25,"isMaximized":false}"#;
    std::fs::write(data(&m).join("design"), geometry).expect("Claude Design's window");
    // Issue 271's: an earlier Chromium's store, which Chromium 152 no longer opens.
    for name in ["SharedStorage", "SharedStorage-wal", "SharedStorage-shm"] {
        std::fs::write(data(&m).join(name), name).expect("a store nothing reads");
    }
    std::fs::create_dir(data(&m).join("one-shot-cwd")).expect("a folder of Claude 3's");
    std::fs::write(data(&m).join("session-popouts.json.tmp"), b"").expect("a write cut short");
    let elsewhere = data(&m).join("IndexedDB/https_www.example.com_0.indexeddb.leveldb");
    std::fs::create_dir_all(&elsewhere).expect("another site's store");
    for label in ["home", "work"] {
        assert!(
            matches!(used(&m, label).0, Outcome::Switched { .. }),
            "{label}"
        );
        assert_eq!(in_use(&m).as_deref(), Some(label));
        assert_eq!(
            std::fs::read(data(&m).join("design")).expect("still there"),
            geometry
        );
        for name in ["SharedStorage", "SharedStorage-wal", "SharedStorage-shm"] {
            assert_eq!(
                std::fs::read(data(&m).join(name)).expect(name),
                name.as_bytes()
            );
        }
        assert!(data(&m).join("one-shot-cwd").is_dir(), "{label}");
        assert!(
            data(&m).join("session-popouts.json.tmp").is_file(),
            "{label}"
        );
        assert!(elsewhere.is_dir(), "{label}");
    }
    hold(&m, "names that stay");
    rested(&m, "names that stay");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_pending_settings_write_of_claudes_survives_a_switch() {
    let m = desktop_pair("sidecar");
    let claude = m.claude();
    claude.opens();
    claude.signs_in(&WORK);
    claude.reports();
    claude.quits();
    let sidecar = data(&m).join("config.json.journal");
    let left = std::fs::read(&sidecar).expect("Claude left a settings write");
    assert!(
        std::fs::read_to_string(data(&m).join("config.json"))
            .expect("settings")
            .contains("updaterRunningInstance"),
        "the file still holds what the sidecar takes out"
    );

    used(&m, "home");
    let written = std::fs::read_to_string(data(&m).join("config.json")).expect("settings");
    assert!(
        !written.contains("updaterRunningInstance"),
        "Pitboard wrote the settings as the sidecar had them: {written}"
    );
    assert!(written.contains(HOME.uuid));
    assert_eq!(
        std::fs::read(&sidecar).expect("the sidecar stays"),
        left,
        "a settings write deletes nothing of Claude's"
    );
    hold(&m, "a switch over a sidecar");
    rested(&m, "a switch over a sidecar");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_leave_is_refused_where_this_mac_cannot_be_identified() {
    let mut m = desktop_pair("no-machine");
    let known = m.ctx.clone();
    m.ctx = m.ctx.clone().with_unknown_machine();
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let stopped = refused(&m, "home");
    assert_eq!(stopped.code(), "machine_unknown");
    assert_eq!(stopped.exit_code(), 3);
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    let stopped = pitboard
        .sign_in_watched("desktop/travel")
        .expect_err("refused")
        .error;
    assert_eq!(stopped.code(), "machine_unknown");
    unchanged(&m, &before, "nothing reserved, nothing moved");

    // An arrive alone: Claude signed out, `work` parked where the Mac was known.
    m.ctx = known.clone();
    let died = crate::fault::killing("leave.done", || used(&m, "home"));
    assert_eq!(died.expect_err("killed"), "leave.done");
    m.ctx = known.with_unknown_machine();
    recover(&m).expect("swept");
    let before = snapshot(&m);
    assert_eq!(refused(&m, "home").code(), "machine_unknown");
    unchanged(&m, &before, "nothing changed");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn nothing_is_set_aside_where_this_mac_cannot_be_identified() {
    let blind = |m: &mut Machine| {
        m.ctx = m.ctx.clone().with_unknown_machine();
        recover(m).expect("swept");
        snapshot(m)
    };

    // The start of a sign-in, which sets aside the keys a Log out left.
    let mut m = desktop_machine("no-machine-start");
    let claude = m.claude();
    claude.opens();
    claude.logs_out();
    claude.quits();
    let before = blind(&mut m);
    let stopped = watched(&m, "travel").expect_err("refused");
    assert_eq!(stopped.code(), "machine_unknown");
    unchanged(&m, &before, "the start of a sign-in");

    // Recovery of an arrive, which sets aside the keys Claude wrote over it.
    let mut m = desktop_pair("no-machine-settle");
    killed(&m, "arrive.jar_in", "home");
    let emptied: Value = serde_json::from_str(EMPTIED).expect("JSON");
    set(&m, "oauth:tokenCache", Some(emptied));
    let before = blind(&mut m);
    assert_eq!(code(confirm_(&m, "work")), "machine_unknown");
    unchanged(&m, &before, "recovery");
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());

    // A give-up, which sets aside what the move left beside another session.
    let mut m = mixed("no-machine-give-up");
    let before = blind(&mut m);
    assert_eq!(code(enrol(&m, "third")), "machine_unknown");
    unchanged(&m, &before, "a give-up");
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn an_arrive_into_a_folder_with_no_indexeddb_makes_its_parent() {
    let m = desktop_pair("no-indexeddb");
    let died = crate::fault::killing("leave.done", || used(&m, "home"));
    assert_eq!(died.expect_err("killed"), "leave.done");
    assert!(data(&m).join("IndexedDB").is_dir(), "the parent stays");
    std::fs::remove_dir(data(&m).join("IndexedDB")).expect("as a new folder has none");
    assert!(matches!(used(&m, "home").0, Outcome::Installed { .. }));
    assert!(
        data(&m)
            .join("IndexedDB/https_claude.ai_0.indexeddb.leveldb/000003.log")
            .is_file()
    );
    assert!(crate::host::fs::testing::is_private(&data(&m).join("IndexedDB")).expect("its mode"));
    rested(&m, "arrived");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_park_from_another_mac_is_refused_before_anything_moves() {
    let m = desktop_pair("another-mac");
    let park = park_of(&m, "home").expect("parked");
    let mut manifest = crate::store::folder::manifest(&m.ctx, &park.service)
        .expect("reads")
        .expect("there");
    manifest.machine = "f".repeat(64);
    crate::store::folder::write_manifest(&m.ctx, Permit::for_a_test(), &park.service, &manifest)
        .expect("written");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let stopped = refused(&m, "home");
    assert_eq!(stopped.code(), "parked_credential_corrupt");
    assert!(
        stopped.to_string().contains("parked on another Mac"),
        "{stopped}"
    );
    unchanged(&m, &before, "`work` did not leave for it");
    assert_eq!(in_use(&m).as_deref(), Some("work"));
}

// A park Pitboard cannot look at, or a file of it it cannot read, is neither missing nor
// another login, so nobody is sent to sign in again for it.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_park_that_cannot_be_looked_at_is_never_said_missing() {
    use crate::host::fs::testing;
    let m = desktop_pair("park-unread");
    let park = park_of(&m, "home").expect("parked").service;
    let folder = crate::store::folder::at(&m.ctx, &park);
    let items = folder.join("items");
    recover(&m).expect("swept");
    for (path, what) in [
        (crate::store::folder::parks_dir(&m.ctx), "the park"),
        (items.join("IndexedDB"), "a listed item"),
        (items.clone(), "its items"),
        (items.join("Cookies"), "its cookie store"),
        (folder.join("manifest.json"), "its manifest"),
        (crate::store::folder::keys_file(&m.ctx, &park), "its keys"),
    ] {
        let before = snapshot(&m);
        testing::deny_reading(&path).expect("not looked into");
        let stopped = refused(&m, "home");
        testing::make_private(&path).expect("looked into again");
        assert_eq!(
            stopped.code(),
            "credential_store_unreadable",
            "{what}: {stopped}"
        );
        assert!(
            !stopped.to_string().contains("--sign-in"),
            "{what}: {stopped}"
        );
        unchanged(&m, &before, what);
    }
    used(&m, "home");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn an_item_in_neither_place_is_gone_on_without_and_said() {
    let m = desktop_pair("item-gone");
    let gone = data(&m).join("Session Storage");
    let (_, warnings) = crate::fault::meanwhile(
        "leave.recorded",
        move || std::fs::remove_dir_all(&gone).expect("gone meanwhile"),
        || used(&m, "home"),
    );
    assert!(
        warnings.iter().any(|warning| matches!(
            warning,
            Warning::ItemGone { item, label, .. }
                if item == "Session Storage" && label == "desktop/work"
        )),
        "{warnings:?}"
    );
    let park = park_of(&m, "work").expect("parked");
    let manifest = crate::store::folder::manifest(&m.ctx, &park.service)
        .expect("reads")
        .expect("there");
    assert!(!manifest.items.contains(&"Session Storage".to_string()));
    used(&m, "work");
    rested(&m, "back without it");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
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
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
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
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
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
        matches!(super::open(&m.ctx, app), super::Open::No),
        "Claude is closed again"
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
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
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
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

// Every read, which opens no jar, lists no process and records nothing.
fn reads_only(m: &Machine, pitboard: &crate::service::Pitboard, why: &str) {
    let asked = |m: &Machine| {
        let state = std::fs::read(crate::home::dir(&m.ctx).join("state.json")).expect("state");
        (m.mem.jars_asked().len(), m.mem.listings(), state)
    };
    // Claude Code's own record, of nobody, is the first read's to make.
    pitboard.status(true).expect("read");
    let before = asked(m);
    pitboard.status_offline().expect("read");
    pitboard.status(true).expect("read");
    pitboard.doctor();
    pitboard.statusline("{}");
    let after = asked(m);
    assert_eq!(after.0, before.0, "{why}: no jar was opened");
    assert_eq!(after.1, before.1, "{why}: no process was listed");
    assert!(after.2 == before.2, "{why}: nothing was recorded");
}

fn killed(m: &Machine, at: &'static str, label: &str) {
    let died = crate::fault::killing(at, || switch(settled(m), &m.key(label)));
    assert_eq!(died.expect_err("killed"), at);
    as_a_dead_runs(m);
}

fn watched(m: &Machine, label: &str) -> crate::error::Result<super::super::WatchedSignIn> {
    crate::service::Pitboard::new(m.ctx.clone())
        .sign_in_watched(&format!("desktop/{label}"))
        .map_err(|failed| failed.error)
}

const SAYS_WITHIN: std::time::Duration = std::time::Duration::from_secs(10);

// Everything the wait has said so far, once it says `until` or its thread has ended.
fn said_until(said: &super::super::Said, until: &str) -> Vec<String> {
    let mut lines = Vec::new();
    loop {
        match said.within(SAYS_WITHIN) {
            Ok(line) => {
                let done = line.contains(until);
                lines.push(line);
                if done {
                    return lines;
                }
            }
            Err(RecvTimeoutError::Disconnected) => return lines,
            Err(RecvTimeoutError::Timeout) => panic!("waiting for {until:?}, said {lines:?}"),
        }
    }
}

// Every line until the wait's thread has ended.
fn said_to_the_end(said: &super::super::Said) -> Vec<String> {
    let mut lines = Vec::new();
    loop {
        match said.within(SAYS_WITHIN) {
            Ok(line) => lines.push(line),
            Err(RecvTimeoutError::Disconnected) => return lines,
            Err(RecvTimeoutError::Timeout) => panic!("the wait goes on, said {lines:?}"),
        }
    }
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_sign_in_left_halfway_is_said_by_every_read_and_put_back_by_use() {
    let m = desktop_pair("halfway");
    let waiting = watched(&m, "travel").expect("`work` parked, waiting");
    assert_eq!(waiting.put_back(), Some(m.key("work")));
    assert_eq!(waiting.steps().now(), Some(super::super::WaitStep::Waiting));
    let lines = said_until(&waiting.said(), "Waiting for Claude to quit");
    assert_eq!(
        lines,
        [
            "Parked `desktop/work`. Claude is signed out, and nothing was signed out at \
             claude.ai.\n",
            "Open Claude and sign in to the account to add as `desktop/travel`. When Claude \
             shows your chats, quit it with Command-Q.\n",
            "Do not choose Log out in Claude: it ends the login Claude holds at claude.ai, and \
             Pitboard cannot bring it back.\n",
            "Waiting for Claude to quit. Control-C stops waiting and keeps `desktop/work` \
             parked; `pitboard use desktop/work` puts it back.\n",
        ]
    );
    waiting.cancel();

    let state = state_of(&m);
    let awaited = state
        .awaiting(ProviderId::Desktop)
        .expect("the record stands");
    assert_eq!(
        (awaited.label.as_str(), awaited.account.as_deref()),
        ("travel", None)
    );
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    assert!(
        pitboard.whole(ProviderId::Desktop),
        "Claude may be opened to sign in"
    );
    reads_only(&m, &pitboard, "a sign-in awaited");
    for read in [
        pitboard.status_offline().expect("read"),
        pitboard.status(true).expect("read"),
    ] {
        assert!(
            read.warnings.iter().any(|warning| matches!(
                warning,
                Warning::SignInAwaited { label, put_back, again: false, .. }
                    if label == "desktop/travel" && put_back == "desktop/work"
            )),
            "{:?}",
            read.warnings
        );
        assert_eq!(read.value.awaiting.len(), 1);
        assert_eq!(read.value.awaiting[0].put_back, m.key("work"));
    }
    let checks = pitboard.doctor().checks;
    assert!(checks.iter().any(|check| check.code == "desktop_awaiting"));

    assert!(matches!(used(&m, "work").0, Outcome::Installed { .. }));
    assert!(state_of(&m).awaiting.is_empty(), "the arrive ends the wait");
    rested(&m, "put back");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn the_wait_reads_the_process_list_and_no_jar() {
    let m = desktop_pair("wait-reads");
    let index = crate::home::dir(&m.ctx).join("state.json");
    let waiting = watched(&m, "travel").expect("waiting");
    said_until(&waiting.said(), "Waiting for Claude to quit");
    let (listings, jars) = (m.mem.listings(), m.mem.jars_asked().len());
    let (settings, recorded) = (
        std::fs::read(data(&m).join("config.json")).expect("settings"),
        std::fs::read(&index).expect("state"),
    );
    listed(&m, listings + 5);
    assert_eq!(m.mem.jars_asked().len(), jars, "no jar");
    assert_eq!(std::fs::read(&index).expect("state"), recorded);
    assert_eq!(
        std::fs::read(data(&m).join("config.json")).expect("settings"),
        settings
    );
    waiting.cancel();
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_resumed_add_says_how_to_put_its_account_back() {
    let m = desktop_pair("resumed");
    watched(&m, "travel").expect("`work` parked").cancel();
    let resumed = watched(&m, "elsewhere").expect("taken up");
    assert_eq!(resumed.put_back(), Some(m.key("work")));
    let lines = said_until(&resumed.said(), "Waiting for Claude to quit");
    assert_eq!(
        lines.last().map(String::as_str),
        Some(
            "Waiting for Claude to quit. Control-C stops waiting and keeps `desktop/work` \
             parked; `pitboard use desktop/work` puts it back.\n"
        )
    );
    assert!(!lines.iter().any(|line| line.starts_with("Parked")));
    resumed.cancel();
    assert_eq!(
        state_of(&m)
            .awaiting(ProviderId::Desktop)
            .map(|a| (a.label.clone(), a.put_back.clone())),
        Some(("elsewhere".into(), format!("desktop_{}", WORK.uuid)))
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn cancelling_a_sign_in_that_parked_nothing_leaves_no_record() {
    let m = desktop_pair("parked-nothing");
    killed(&m, "leave.done", "home");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let waiting = watched(&m, "travel").expect("waiting, nothing to park");
    assert_eq!(waiting.put_back(), None);
    let lines = said_until(&waiting.said(), "Waiting for Claude to quit");
    assert!(
        !lines.iter().any(|line| line.starts_with("Parked")),
        "{lines:?}"
    );
    waiting.cancel();
    unchanged(&m, &before, "nothing to undo");
    assert!(state_of(&m).awaiting.is_empty());
}

// A move another run left while the wait went on: the wait's look settles it, and says so.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn the_wait_says_a_move_its_look_settled() {
    let m = desktop_pair("wait-settles");
    let waiting = watched(&m, "travel").expect("waiting");
    let (said, claude) = (waiting.said(), m.claude());
    said_until(&said, "Waiting for Claude to quit");
    killed(&m, "arrive.recorded", "home");
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.crashes();
    let lines = said_until(&said, "Claude quit with nobody signed in.");
    assert!(
        lines.iter().any(|line| line.starts_with(
            "an earlier move of `desktop/home`'s login was interrupted; Pitboard took it back out"
        )),
        "{lines:?}"
    );
    waiting.cancel();
    assert!(state_of(&m).moving.is_empty());
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn still_signed_out_is_still_waiting() {
    let m = desktop_pair("still-waiting");
    let waiting = watched(&m, "travel").expect("waiting");
    let (said, steps, claude) = (waiting.said(), waiting.steps(), m.claude());
    claude.opens();
    said_until(&said, "Claude is open.");
    assert_eq!(steps.now(), Some(super::super::WaitStep::Open));
    claude.quits();
    said_until(&said, "Claude quit with nobody signed in. Still waiting.");
    assert_eq!(steps.now(), Some(super::super::WaitStep::QuitSignedOut));
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.quits_before_reporting();
    said_until(&said, "Claude quit before it finished signing in.");
    assert_eq!(steps.now(), Some(super::super::WaitStep::QuitUnfinished));
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    let login = waiting.finish().expect("signed in at last");
    enroll(settled(&m), &m.key("travel"), Some(login)).expect("enrolled");
    assert_eq!(in_use(&m).as_deref(), Some("travel"));
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn the_wait_ends_where_its_sign_in_was_settled_elsewhere() {
    // Another command puts `work` back meanwhile: this wait enrolls nothing.
    let m = desktop_pair("settled-elsewhere");
    let waiting = watched(&m, "travel").expect("waiting");
    let (said, claude) = (waiting.said(), m.claude());
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.quits();
    said_until(&said, "Still waiting.");
    used(&m, "work");
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.crashes();
    let ended = waiting.finish().expect_err("settled elsewhere");
    assert_eq!(ended.code(), "sign_in_settled_elsewhere");
    assert_eq!(in_use(&m).as_deref(), Some("work"));
    // Nothing waits any more, so a second sign-in may start.
    watched(&m, "travel").expect("free to start").cancel();

    // The home removed meanwhile.
    let m = desktop_pair("home-removed");
    let waiting = watched(&m, "travel").expect("waiting");
    let said = waiting.said();
    m.claude().opens();
    said_until(&said, "Claude is open.");
    let home = crate::home::dir(&m.ctx);
    std::fs::remove_dir_all(&home).expect("removed meanwhile");
    m.claude().crashes();
    let ended = waiting.finish().expect_err("ended");
    assert_eq!(ended.code(), "sign_in_settled_elsewhere");
    assert!(!home.exists());
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn the_wait_never_makes_a_home_uninstall_removed() {
    let m = desktop_machine("home-gone");
    let claude = m.claude();
    claude.opens();
    claude.logs_out();
    claude.quits();
    let waiting = watched(&m, "work").expect("waiting, nothing parked");
    assert_eq!(waiting.put_back(), None);
    let lines = said_until(&waiting.said(), "Waiting for Claude to quit");
    assert!(
        lines
            .last()
            .expect("said")
            .ends_with("Control-C stops waiting.\n")
    );
    assert!(
        lines[0].contains("moved what it found in the way"),
        "the keys Claude kept were set aside first: {lines:?}"
    );
    assert!(
        named_in_settings(&m).is_none(),
        "the wait starts with no name"
    );
    assert!(
        state_of(&m).awaiting.is_empty(),
        "nothing parked, nothing recorded"
    );
    let home = crate::home::dir(&m.ctx);
    claude.opens();
    said_until(&waiting.said(), "Claude is open.");
    std::fs::remove_dir_all(&home).expect("uninstalled meanwhile");
    claude.crashes();
    let ended = waiting.finish().expect_err("ended");
    assert_eq!(ended.code(), "sign_in_settled_elsewhere");
    assert!(!home.exists(), "no directory was made");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn an_add_from_a_logged_out_claude_never_files_a_session_under_the_old_name() {
    let m = desktop_machine("logged-out");
    let claude = m.claude();
    claude.opens();
    claude.logs_out();
    claude.quits();
    assert_eq!(
        named_in_settings(&m).as_deref(),
        Some(WORK.uuid),
        "E1b keeps the name"
    );
    let waiting = watched(&m, "travel").expect("waiting");
    let said = waiting.said();
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.quits_before_reporting();
    said_until(&said, "Claude quit before it finished signing in.");
    assert!(
        named_in_settings(&m).is_none(),
        "no stale name to take it under"
    );
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    let login = waiting.finish().expect("signed in");
    let (enrolled, _) = enroll(settled(&m), &m.key("travel"), Some(login)).expect("enrolled");
    assert!(matches!(
        enrolled,
        super::Enrolled::InUse { again: false, .. }
    ));
    let state = state_of(&m);
    assert_eq!(
        state.get(&m.key("travel")).expect("enrolled").account_uuid,
        STRANGER.uuid
    );
    assert_eq!(state.aside.len(), 1, "the keys Claude kept, set aside");
    assert_eq!(state.aside[0].why, SetAside::InTheWay);
    hold(&m, "an add after a Log out");
}

// Rows 3, 5 and 10: `work`'s session is gone, so the start of a sign-in records nobody in use.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_sign_in_from_a_logged_out_claude_records_nobody_in_use() {
    // Rows 5 and 3: nothing to set aside, so the record is the save.
    let emptied = desktop_machine("emptied-nobody");
    without_items(&emptied);
    for key in [
        "oauth:tokenCache",
        "oauth:tokenCacheV2",
        "lastKnownAccountUuid",
    ] {
        set(&emptied, key, None);
    }
    let no_folder = desktop_machine("no-folder-nobody");
    std::fs::remove_dir_all(data(&no_folder)).expect("removed");
    for (m, row) in [(emptied, "row 5"), (no_folder, "row 3")] {
        watched(&m, "travel").expect("waiting").cancel();
        let state = state_of(&m);
        let record = state.in_use(ProviderId::Desktop).expect("recorded");
        assert_eq!((&record.owner, &record.named), (&None, &None), "{row}");
        assert!(state.aside.is_empty(), "{row}: {:?}", state.aside);
        assert!(
            audit_lines(&m, "in-use").contains(&(String::new(), "signed_out_outside".into())),
            "{row}: {:?}",
            audit_lines(&m, "in-use")
        );
        hold_where_deleted(&m, row);
    }

    // Row 10: the keys Log out left are set aside in the same save.
    let m = desktop_machine("logged-out-nobody");
    let claude = m.claude();
    claude.opens();
    claude.logs_out();
    claude.quits();
    let waiting = watched(&m, "travel").expect("waiting");
    said_until(&waiting.said(), "Waiting for Claude to quit");
    waiting.cancel();
    let state = state_of(&m);
    let record = state.in_use(ProviderId::Desktop).expect("recorded");
    assert_eq!((&record.owner, &record.named), (&None, &None));
    assert!(
        state
            .get(&m.key("work"))
            .expect("enrolled")
            .replaced_at
            .is_some(),
        "`work`'s only login was the one Log out ended"
    );
    assert!(
        audit_lines(&m, "in-use").contains(&(String::new(), "signed_out_outside".into())),
        "{:?}",
        audit_lines(&m, "in-use")
    );
    let read = crate::service::Pitboard::new(m.ctx.clone())
        .status_offline()
        .expect("read");
    assert!(
        !read
            .warnings
            .iter()
            .any(|warning| warning.code() == "in_use_unconfirmed"),
        "{:?}",
        read.warnings
    );
    hold(&m, "a sign-in started after a Log out");

    // Claude writes an emptied cache again: the next start saves its aside though the record
    // already names nobody.
    set(
        &m,
        "oauth:tokenCache",
        Some(serde_json::from_str(EMPTIED).expect("JSON")),
    );
    watched(&m, "travel").expect("waiting").cancel();
    let state = state_of(&m);
    let asides = state.aside.iter().filter(|a| a.why == SetAside::InTheWay);
    assert_eq!(asides.count(), 2, "{:?}", state.aside);
    hold(&m, "a sign-in started again after a Log out");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn signing_in_again_as_another_account_is_refused_and_named() {
    let m = desktop_pair("again");
    // Sign In Again of `home`, and Claude is signed in to somebody else.
    let waiting = watched(&m, "home").expect("waiting");
    let said = waiting.said();
    let lines = said_until(&said, "Waiting for Claude to quit");
    assert!(
        lines[1].contains("sign in to `desktop/home` again"),
        "{lines:?}"
    );
    let claude = m.claude();
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    let login = waiting.finish().expect("somebody signed in");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let refused = enroll(settled(&m), &m.key("home"), Some(login)).expect_err("refused");
    match &refused {
        Error::LabelTaken { label, who } => assert_eq!(
            (label.as_str(), who.as_str()),
            ("desktop/home", "another Claude Desktop account"),
            "who is signed in is not `home`'s account"
        ),
        other => panic!("{}: {other}", other.code()),
    }
    unchanged(&m, &before, "the record of the sign-in stands");
    assert!(state_of(&m).awaiting(ProviderId::Desktop).is_some());

    // Waited for under a new label, and Claude is signed in to an enrolled account.
    enrol(&m, "third").expect("the stranger enrolled under its own label");
    let waiting = watched(&m, "fourth").expect("`third` parked, waiting");
    let said = waiting.said();
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    let login = waiting.finish().expect("signed in");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let refused = enroll(settled(&m), &m.key("fourth"), Some(login)).expect_err("refused");
    assert_eq!(refused.code(), "already_enrolled");
    assert!(
        refused
            .to_string()
            .contains("Run `pitboard enroll desktop/third` to record that login"),
        "{refused}"
    );
    unchanged(&m, &before, "the record of the sign-in stands");
    assert!(state_of(&m).awaiting(ProviderId::Desktop).is_some());
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn forgetting_the_account_an_add_would_put_back_leaves_no_dangling_put_back() {
    let m = desktop_pair("forget-put-back");
    watched(&m, "travel").expect("`work` parked").cancel();
    assert!(state_of(&m).awaiting(ProviderId::Desktop).is_some());
    forget_(&m, "work").expect("forgotten: it is parked, and nobody is in use");
    let state = state_of(&m);
    assert!(state.awaiting.is_empty(), "nothing is left to put back");
    assert!(state.get(&m.key("work")).is_none());
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    let warnings = pitboard.status_offline().expect("read").warnings;
    assert!(
        !warnings
            .iter()
            .any(|w| matches!(w, Warning::SignInAwaited { .. })),
        "{warnings:?}"
    );
}

fn gave_up(m: &Machine) -> crate::error::Result<super::super::GaveUp> {
    super::super::abandon(&m.ctx, Permit::for_a_test())
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_custom_claude_code_endpoint_never_stops_a_claude_desktop_give_up() {
    let mut m = desktop_pair("custom-endpoint");
    tokens_beside_no_session(&m);
    m.ctx.custom_oauth = true;
    let gave = gave_up(&m).expect("no journal, so the endpoint is not asked");
    m.ctx.custom_oauth = false;
    assert_eq!(gave.moves.len(), 1, "{gave:?}");
    assert!(state_of(&m).moving.is_empty());
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn abandon_settles_a_move_it_can_before_giving_up_any() {
    let m = desktop_pair("abandon-settles");
    killed(&m, "leave.keys_out", "home");
    let gave = gave_up(&m).expect("settled");
    assert!(gave.switch.is_none() && gave.moves.is_empty());
    assert!(
        matches!(
            gave.warnings.as_slice(),
            [Warning::MoveSettled(moved)]
                if moved.ended == super::Ended::Undone && moved.half == state::Half::Leave
        ),
        "{:?}",
        gave.warnings
    );
    assert_eq!(gave.warnings[0].code(), "interrupted_move_undone");
    assert_eq!(
        gave.warnings[0].to_string(),
        "an earlier move of `desktop/work`'s login was interrupted; Pitboard undid it, and \
         `desktop/work` is in use"
    );
    assert_eq!(in_use(&m).as_deref(), Some("work"));
    rested(&m, "settled by abandon");
    let nothing = gave_up(&m).expect("nothing waits");
    assert!(nothing.switch.is_none() && nothing.moves.is_empty() && nothing.warnings.is_empty());
}

// A give-up reads no settings, which it never writes, and a jar or an item it cannot read is
// refused with its own way out, never with a sentence that sends the person back to `abandon`.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn abandon_gives_up_whatever_the_settings_and_says_why_a_jar_or_an_item_stops_it() {
    use crate::host::fs::testing;
    let m = desktop_pair("abandon-settings");
    killed(&m, "leave.jar_out", "home");
    std::fs::write(data(&m).join("config.json"), "[]").expect("settings not read");
    let gave = gave_up(&m).expect("given up");
    assert_eq!(gave.moves.len(), 1, "{gave:?}");
    assert!(state_of(&m).moving.is_empty());

    let m = desktop_pair("abandon-jar");
    killed(&m, "leave.keys_out", "home");
    write_stand_in_jar(&data(&m).join("Cookies"), 25, &session_of(Some(&WORK)));
    let stopped = gave_up(&m).expect_err("refused");
    assert_eq!(stopped.code(), "login_folder_unknown", "{stopped}");
    let said = stopped.to_string();
    assert!(said.contains("Update Pitboard"), "{said}");
    assert!(!said.contains("pitboard abandon"), "{said}");
    // The first give-up swept what the kill left; one refused moves and indexes nothing.
    let before = snapshot(&m);
    let again = gave_up(&m).expect_err("refused again");
    assert_eq!(again.code(), "login_folder_unknown", "{again}");
    unchanged(&m, &before, "a jar not read");
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());

    let m = desktop_pair("abandon-item");
    killed(&m, "leave.jar_out", "home");
    let stores = data(&m).join("IndexedDB");
    testing::deny_reading(&stores).expect("not looked into");
    let stopped = gave_up(&m).expect_err("refused");
    let before = snapshot(&m);
    let again = gave_up(&m).expect_err("refused again");
    unchanged(&m, &before, "an item not looked at");
    testing::make_private(&stores).expect("looked into again");
    for refused in [&stopped, &again] {
        assert_eq!(refused.code(), "login_folder_unknown", "{refused}");
        let said = refused.to_string();
        assert!(said.contains("could not look at"), "{said}");
        assert!(!said.contains("pitboard abandon"), "{said}");
    }
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());

    // What of the park Pitboard cannot look at or read is said as the park's, never Claude's.
    let m = desktop_pair("abandon-park");
    killed(&m, "leave.verified", "home");
    let park = state_of(&m)
        .moving(ProviderId::Desktop)
        .expect("waits")
        .park
        .clone();
    let items = crate::store::folder::at(&m.ctx, &park).join("items");
    let jar = items.join("Cookies");
    let refused_twice = |m: &Machine| {
        let stopped = gave_up(m).expect_err("refused");
        let before = snapshot(m);
        let again = gave_up(m).expect_err("refused again");
        unchanged(m, &before, "the park not looked at");
        [stopped, again]
    };
    let said_of = |refused: &[Error; 2], named: &std::path::Path| {
        for refused in refused {
            assert_eq!(refused.code(), "credential_store_unreadable", "{refused}");
            let said = refused.to_string();
            assert!(said.contains(&named.display().to_string()), "{said}");
            for never in ["Open Claude", "data folder", "pitboard abandon"] {
                assert!(!said.contains(never), "{said}");
            }
        }
    };
    let stores = items.join("IndexedDB");
    testing::deny_reading(&stores).expect("not looked into");
    let refused = refused_twice(&m);
    testing::make_private(&stores).expect("looked into again");
    said_of(&refused, &stores);
    testing::deny_reading(&items).expect("not looked into");
    let refused = refused_twice(&m);
    testing::make_private(&items).expect("looked into again");
    said_of(&refused, &jar);
    m.mem.jar_unsettled(&jar);
    let refused = refused_twice(&m);
    said_of(&refused, &jar);
    assert!(
        refused[0].to_string().contains("(a test held the jar)"),
        "{}",
        refused[0]
    );
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn giving_up_indexes_every_folder_moves_nothing_and_deletes_nothing() {
    let m = desktop_pair("give-up");
    killed(&m, "leave.jar_out", "home");
    // The jar is parked; its journal and the rest are in Claude's folder. Its keys come
    // back by hand, as tokens with no session: no row of recovery names that.
    let park = state_of(&m)
        .moving(ProviderId::Desktop)
        .expect("recorded")
        .park
        .clone();
    let kept =
        std::fs::read_to_string(crate::store::folder::keys_file(&m.ctx, &park)).expect("keys kept");
    let tokens: Map<String, Value> = serde_json::from_str(&kept).expect("JSON");
    set(
        &m,
        "oauth:tokenCache",
        tokens.get("oauth:tokenCache").cloned(),
    );
    let before = (
        tree(&data(&m)),
        tree(&crate::store::folder::parks_dir(&m.ctx)),
    );
    let stopped = code(confirm_(&m, "work"));
    assert_eq!(stopped, "recovery_undetermined");
    assert_eq!(
        code(forget_(&m, "home")),
        "recovery_undetermined",
        "no account is forgotten while a move waits"
    );
    assert!(state_of(&m).get(&m.key("home")).is_some());

    let gave = gave_up(&m).expect("given up");
    assert_eq!(gave.moves.len(), 1, "{gave:?}");
    let given = &gave.moves[0];
    assert_eq!((given.parked, given.set_aside), (1, 0), "{given}");
    assert!(given.left.contains(&"Local Storage".to_string()), "{given}");
    assert!(!given.left.iter().any(|item| item.starts_with("Cookies")));
    assert!(
        given
            .to_string()
            .contains("stay in Claude's folder. Open Claude once"),
        "{given}"
    );
    let manifest = crate::store::folder::manifest(&m.ctx, &park)
        .expect("reads")
        .expect("there");
    assert_eq!(manifest.left_in_folder, given.left);
    assert_eq!(manifest.login.as_deref(), Some(WORK.login().as_str()));
    let after = (
        tree(&data(&m)),
        tree(&crate::store::folder::parks_dir(&m.ctx)),
    );
    assert_eq!(after.0, before.0, "nothing of Claude's folder moved");
    let paths = |found: &[(PathBuf, Option<Vec<u8>>)]| -> Vec<PathBuf> {
        found.iter().map(|(path, _)| path.clone()).collect()
    };
    assert_eq!(
        paths(&after.1),
        paths(&before.1),
        "nothing under parks was moved or deleted"
    );
    let state = state_of(&m);
    assert!(state.moving.is_empty() && state.in_use(ProviderId::Desktop).is_none());
    assert_eq!(park_of(&m, "work").map(|p| p.service), Some(park));
    hold(&m, "given up");
}

// `work` half parked, then Claude signed in to an account nobody enrolled.
fn mixed(name: &str) -> Machine {
    let m = desktop_pair(name);
    killed(&m, "leave.item_out", "home");
    let claude = m.claude();
    claude.opens();
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    recover(&m).expect("swept");
    m
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn giving_up_refuses_where_parts_of_two_logins_would_be_left_unsaid() {
    let m = mixed("mixed");
    let before = snapshot(&m);
    let mixed = gave_up(&m).expect_err("refused");
    assert_eq!(mixed.code(), "move_mixed");
    assert_eq!(mixed.exit_code(), 3);
    assert!(
        mixed
            .to_string()
            .contains("`Local Storage` in Claude's folder"),
        "{mixed}"
    );
    unchanged(&m, &before, "nothing changed");

    // The record's own session in Claude's folder, under keys it does not name: nothing of
    // the folder is touched, and `enroll` under the account Claude names lands.
    let m = desktop_pair("mine-and-other");
    killed(&m, "leave.keys_kept", "home");
    names(&m, &HOME);
    assert_eq!(code(confirm_(&m, "work")), "recovery_undetermined");
    let folder = tree(&data(&m));
    let gave = gave_up(&m).expect("given up, nothing refused");
    assert_eq!(gave.moves.len(), 1, "{gave:?}");
    assert!(gave.moves[0].left.is_empty());
    assert_eq!(tree(&data(&m)), folder, "nothing of Claude's folder moved");
    // Row 18 now that nothing is recorded in use: `home`'s older park is set aside.
    let (_, warnings) = enrol(&m, "home").expect("filed as Claude names it");
    assert!(
        matches!(
            warnings.as_slice(),
            [Warning::SetAside { why: SetAside::Superseded, label: Some(label), .. }]
                if label == "desktop/home"
        ),
        "{warnings:?}"
    );
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    assert_eq!(whereabouts(&m, &WORK), ["Claude"]);
    hold(&m, "given up over the record's own session");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn enroll_refused_under_a_taken_label_gives_nothing_up() {
    let m = mixed("taken-label");
    let before = snapshot(&m);
    assert_eq!(code(enrol(&m, "home")), "label_taken");
    unchanged(&m, &before, "the record, the folder and parks as they were");
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn enroll_sets_aside_parts_left_beside_the_login_it_files() {
    let m = mixed("parts-aside");
    let jar = std::fs::read(data(&m).join("Cookies")).expect("the stranger's jar");
    let (_, warnings) = enrol(&m, "third").expect("the person's word is the way out");
    assert!(
        warnings.iter().any(|warning| matches!(
            warning,
            Warning::SetAside { why: SetAside::GivenUp, label: Some(label), .. }
                if label == "desktop/work"
        )),
        "{warnings:?}"
    );
    assert_eq!(
        std::fs::read(data(&m).join("Cookies")).expect("the jar"),
        jar
    );
    assert_eq!(in_use(&m).as_deref(), Some("third"));
    let state = state_of(&m);
    let aside = state
        .aside
        .iter()
        .find(|a| a.why == SetAside::GivenUp)
        .expect("set aside");
    assert_eq!(
        aside.account.as_deref(),
        Some(format!("desktop_{}", WORK.uuid).as_str())
    );
    assert!(!crate::store::folder::item(&m.ctx, &aside.name, "Cookies").exists());
    assert!(!crate::store::folder::item(&m.ctx, &aside.name, "Cookies-journal").exists());
    assert!(crate::store::folder::item(&m.ctx, &aside.name, "Local Storage").exists());
    assert!(!data(&m).join("Local Storage").exists());
    assert_eq!(
        whereabouts(&m, &WORK).len(),
        1,
        "`work`'s session is in its park alone"
    );
    assert!(park_of(&m, "work").is_some());
    hold(&m, "enrolled out of a mixed move");
    rested(&m, "enrolled out of a mixed move");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn enroll_is_the_way_out_of_a_stopped_move_with_claude_signed_in() {
    // Somebody signed in after an arrive was recorded: `home` keeps its park.
    let m = desktop_pair("way-out-arrive");
    killed(&m, "arrive.recorded", "home");
    let claude = m.claude();
    claude.opens();
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    assert_eq!(code(confirm_(&m, "work")), "recovery_undetermined");
    let parked = park_of(&m, "home").expect("parked").service;
    enrol(&m, "third").expect("enrolled");
    assert_eq!(in_use(&m).as_deref(), Some("third"));
    assert_eq!(park_of(&m, "home").map(|p| p.service), Some(parked));
    assert!(state_of(&m).moving.is_empty());
    hold(&m, "out of a stopped arrive");
    rested(&m, "out of a stopped arrive");

    // Claude replaced the session of the account arriving: it is filed under that account.
    let m = desktop_pair("way-out-renewed");
    killed(&m, "arrive.keys_in", "home");
    let claude = m.claude();
    claude.opens();
    claude.renews(&HOME, "home-session-renewed");
    claude.quits();
    assert_eq!(code(confirm_(&m, "work")), "recovery_undetermined");
    enrol(&m, "home").expect("enrolled");
    let renewed = Person {
        uuid: HOME.uuid,
        session: "home-session-renewed",
    };
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    assert_eq!(whereabouts(&m, &renewed), ["Claude"]);
    let state = state_of(&m);
    assert_eq!(
        state.in_use(ProviderId::Desktop).map(|r| r.login.clone()),
        Some(renewed.login())
    );
    hold(&m, "out of a stopped arrive Claude renewed");
    rested(&m, "out of a stopped arrive Claude renewed");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_give_up_never_moves_the_jar_in_claudes_folder() {
    for (at, who, label) in [
        ("leave.keys_out", WORK, "work"),
        ("arrive.jar_in", HOME, "home"),
        ("arrive.keys_in", HOME, "home"),
    ] {
        let m = desktop_pair(&format!("jar-stays-{at}"));
        killed(&m, at, "home");
        let claude = m.claude();
        claude.opens();
        claude.renews(&who, "a-session-claude-renewed");
        claude.quits();
        let inode = |item: &str| crate::host::fs::testing::inode(&data(&m).join(item)).ok();
        let jar = (inode("Cookies"), inode("Cookies-journal"));
        let park = state_of(&m)
            .moving(ProviderId::Desktop)
            .expect("recorded")
            .park
            .clone();
        assert!(jar.0.is_some() && jar.1.is_some(), "{at}");
        enrol(&m, label).unwrap_or_else(|e| panic!("{at}: {e}"));
        assert_eq!((inode("Cookies"), inode("Cookies-journal")), jar, "{at}");
        let renewed = Person {
            uuid: who.uuid,
            session: "a-session-claude-renewed",
        };
        let state = state_of(&m);
        assert_eq!(
            state.in_use(ProviderId::Desktop).map(|r| r.login.clone()),
            Some(renewed.login()),
            "{at}"
        );
        assert!(
            state
                .aside
                .iter()
                .all(|a| a.why != SetAside::GivenUp || a.name == park),
            "{at}: no aside made by the give-up: {:?}",
            state.aside
        );
        hold(&m, at);
        rested(&m, at);
    }
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_give_up_never_files_a_jar_it_did_not_identify() {
    let m = desktop_pair("unidentified-jar");
    killed(&m, "arrive.recorded", "home");
    let park = park_of(&m, "home").expect("parked").service;
    let parked = crate::store::folder::item(&m.ctx, &park, "Cookies");
    write_stand_in_jar(&parked, 24, &session_of(Some(&STRANGER)));
    assert_eq!(code(confirm_(&m, "work")), "recovery_undetermined");
    let gave = gave_up(&m).expect("given up");
    assert_eq!((gave.moves[0].parked, gave.moves[0].set_aside), (0, 1));
    assert!(park_of(&m, "home").is_none(), "`home` holds nothing");
    let state = state_of(&m);
    let aside = state.aside.iter().find(|a| a.name == park).expect("kept");
    assert_eq!(aside.why, SetAside::GivenUp);
    assert_eq!(aside.login.as_deref(), Some(STRANGER.login().as_str()));
    assert_eq!(
        aside.account.as_deref(),
        Some(format!("desktop_{}", HOME.uuid).as_str())
    );
    repair(settled(&m)).expect("repaired");
    assert!(park_of(&m, "home").is_none(), "never given back");
    assert!(crate::store::folder::at(&m.ctx, &park).is_dir());
    hold(&m, "a jar given up unidentified");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_give_up_never_points_a_manifest_at_a_session_it_did_not_park() {
    let m = desktop_pair("manifest-kept");
    killed(&m, "leave.jar_out", "home");
    let park = state_of(&m)
        .moving(ProviderId::Desktop)
        .expect("recorded")
        .park
        .clone();
    let parked = crate::store::folder::item(&m.ctx, &park, "Cookies");
    write_stand_in_jar(&parked, 24, &session_of(Some(&STRANGER)));
    gave_up(&m).expect("given up");
    let manifest = crate::store::folder::manifest(&m.ctx, &park)
        .expect("reads")
        .expect("there");
    assert_eq!(
        manifest.login.as_deref(),
        Some(WORK.login().as_str()),
        "the login the leave wrote"
    );
    let state = state_of(&m);
    let aside = state.aside.iter().find(|a| a.name == park).expect("kept");
    assert_eq!(aside.login.as_deref(), Some(STRANGER.login().as_str()));
    repair(settled(&m)).expect("repaired");
    let state = state_of(&m);
    assert!(park_of(&m, "work").is_none(), "given to nobody");
    assert!(state.aside.iter().any(|a| a.name == park));
}

// `work`'s leave given up with its items left in Claude's folder.
fn left_by_a_give_up(name: &str) -> Machine {
    let m = desktop_pair(name);
    tokens_beside_no_session(&m);
    let gave = gave_up(&m).expect("given up");
    assert!(!gave.moves[0].left.is_empty(), "{gave:?}");
    m
}

// Whose each folder set aside as in the way is, oldest first, as the index and its manifest
// file it.
fn in_the_way(m: &Machine) -> Vec<Option<String>> {
    state_of(m)
        .aside
        .iter()
        .filter(|aside| aside.why == SetAside::InTheWay)
        .map(|aside| {
            let manifest = crate::store::folder::manifest(&m.ctx, &aside.name)
                .expect("reads")
                .expect("there");
            assert_eq!(manifest.account, aside.account);
            aside.account.clone()
        })
        .collect()
}

// What the manifest of `label`'s park lists as left in Claude's folder.
fn listed_left(m: &Machine, label: &str) -> Vec<String> {
    let park = park_of(m, label).expect("parked").service;
    crate::store::folder::manifest(&m.ctx, &park)
        .expect("reads")
        .expect("there")
        .left_in_folder
}

// A listing a give-up wrote, which nothing took since.
fn list_left(m: &Machine, label: &str) {
    let park = park_of(m, label).expect("parked").service;
    let mut manifest = crate::store::folder::manifest(&m.ctx, &park)
        .expect("reads")
        .expect("there");
    manifest.left_in_folder = vec!["Local Storage".into()];
    crate::store::folder::write_manifest(&m.ctx, Permit::for_a_test(), &park, &manifest)
        .expect("written");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn what_a_give_up_left_is_filed_under_its_account_only_until_another_takes_it() {
    let work = Some(format!("desktop_{}", WORK.uuid));
    let m = left_by_a_give_up("left-filed");
    assert!(!listed_left(&m, "work").is_empty());
    let claude = m.claude();
    claude.opens();
    claude.quits();
    used(&m, "home");
    assert_eq!(
        in_the_way(&m),
        std::slice::from_ref(&work),
        "nothing took them since"
    );
    assert!(listed_left(&m, "work").is_empty(), "set aside");
    hold(&m, "left by a give-up, set aside");

    // The arrive killed once it set the first of them aside: recovery sets the rest aside with
    // it, and what Claude makes after is nobody's.
    let m = left_by_a_give_up("left-killed");
    let claude = m.claude();
    claude.opens();
    claude.quits();
    killed(&m, "arrive.item_aside", "home");
    // The next change settles the arrive first.
    assert_eq!(code(confirm_(&m, "work")), "account_not_in_use");
    assert_eq!(
        in_the_way(&m),
        std::slice::from_ref(&work),
        "set aside by recovery"
    );
    assert!(listed_left(&m, "work").is_empty(), "set aside by recovery");
    claude.opens();
    claude.quits();
    used(&m, "home");
    assert_eq!(in_the_way(&m), [work.clone(), None], "Claude's own since");
    hold(&m, "left by a give-up, set aside by recovery");
    rested(&m, "left by a give-up, set aside by recovery");

    // The arrive killed before its jar went aside: the listing stays with the items.
    let m = left_by_a_give_up("left-keys-killed");
    let claude = m.claude();
    claude.opens();
    claude.quits();
    killed(&m, "arrive.keys_cleared", "home");
    assert_eq!(code(confirm_(&m, "work")), "account_not_in_use");
    assert!(
        !listed_left(&m, "work").is_empty(),
        "still in Claude's folder"
    );
    used(&m, "home");
    assert_eq!(
        in_the_way(&m),
        [work.clone(), work.clone()],
        "the keys, then the items"
    );
    assert!(listed_left(&m, "work").is_empty(), "set aside");
    claude.opens();
    claude.logs_out();
    claude.quits();
    used(&m, "work");
    assert_eq!(
        in_the_way(&m),
        [work.clone(), work, None],
        "Claude's own since"
    );
    hold(&m, "left by a give-up, set aside after a kill");
    rested(&m, "left by a give-up, set aside after a kill");

    // Somebody signs in over them and is enrolled, leaves with them, then a Log out.
    let m = left_by_a_give_up("left-taken");
    let claude = m.claude();
    claude.opens();
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    enrol(&m, "j").expect("filed as the person says");
    assert!(listed_left(&m, "work").is_empty(), "filed under `j`");
    used(&m, "home");
    claude.opens();
    claude.logs_out();
    claude.quits();
    used(&m, "j");
    assert_eq!(in_the_way(&m), [None], "never `work`'s");
    hold(&m, "left by a give-up, then taken by another");
    rested(&m, "left by a give-up, then taken by another");

    // A listing nothing took goes with the next leave, and with the next arrive.
    let m = desktop_pair("left-moved");
    list_left(&m, "home");
    watched(&m, "travel").expect("`work` parked").cancel();
    assert!(
        listed_left(&m, "home").is_empty(),
        "taken by `work`'s leave"
    );
    list_left(&m, "home");
    used(&m, "work");
    assert!(
        listed_left(&m, "home").is_empty(),
        "gone by `work`'s arrive"
    );
    hold(&m, "a listing nothing took, then a leave and an arrive");
}

// A leave whose commit cannot clear a listing waits, and the next change finishes it.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_listing_that_cannot_be_cleared_leaves_the_leave_to_the_next_change() {
    let m = desktop_pair("left-unwritable");
    list_left(&m, "home");
    let park = crate::store::folder::at(&m.ctx, &park_of(&m, "home").expect("parked").service);
    let mode = std::fs::metadata(&park).expect("there").permissions();
    crate::host::fs::testing::deny_changes(&park).expect("read-only");
    let refused = use_(&m, "home");
    std::fs::set_permissions(&park, mode).expect("changeable again");
    assert_eq!(code(refused), "home_unwritable");
    let moving = state_of(&m)
        .moving(ProviderId::Desktop)
        .cloned()
        .expect("the leave waits");
    assert_eq!(moving.half, state::Half::Leave);
    assert_eq!(in_use(&m).as_deref(), Some("work"), "nothing committed");
    assert!(!listed_left(&m, "home").is_empty(), "kept");
    assert!(!data(&m).join("Cookies").exists(), "`work`'s jar left");
    assert!(
        crate::store::folder::item(&m.ctx, &moving.park, "Cookies").exists(),
        "`work`'s jar parked"
    );
    assert_eq!(code(confirm_(&m, "work")), "account_not_in_use");
    assert!(
        state_of(&m).moving(ProviderId::Desktop).is_none(),
        "finished"
    );
    assert!(listed_left(&m, "home").is_empty(), "taken by the leave");
    used(&m, "home");
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    hold(&m, "a leave finished by the next change");
    rested(&m, "a leave finished by the next change");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn giving_up_an_arrive_keeps_the_accounts_park() {
    let m = desktop_pair("arrive-kept");
    killed(&m, "arrive.recorded", "home");
    let parked = park_of(&m, "home").expect("parked");
    let claude = m.claude();
    claude.opens();
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    let gave = gave_up(&m).expect("given up");
    assert_eq!((gave.moves[0].parked, gave.moves[0].set_aside), (1, 0));
    assert_eq!(park_of(&m, "home"), Some(parked.clone()));
    assert_eq!(whereabouts(&m, &HOME), [parked.service]);
    let state = state_of(&m);
    assert!(state.moving.is_empty() && state.in_use(ProviderId::Desktop).is_none());
    hold(&m, "an arrive given up");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn giving_up_a_move_elsewhere_reads_no_claude_folder() {
    let m = desktop_pair("elsewhere");
    killed(&m, "leave.jar_out", "home");
    let mut state = state_of(&m);
    state.moving.get_mut("desktop").expect("recorded").slot = "/somewhere/else".into();
    let there = state
        .in_use(ProviderId::Desktop)
        .cloned()
        .expect("work in use");
    state
        .other_slots
        .entry("desktop".into())
        .or_default()
        .insert("/somewhere/else".into(), there);
    state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
    assert_eq!(code(confirm_(&m, "work")), "recovery_elsewhere");
    let here = state_of(&m).in_use(ProviderId::Desktop).cloned();
    let park = state_of(&m)
        .moving(ProviderId::Desktop)
        .expect("recorded")
        .park
        .clone();
    let (jars, listings) = (m.mem.jars_asked().len(), m.mem.listings());
    m.claude().opens();
    let gave = gave_up(&m).expect("given up, Claude open or not");
    let asked = m.mem.jars_asked();
    assert!(
        asked[jars..].iter().all(|jar| !jar.starts_with(data(&m))),
        "no jar of Claude's folder was read: {asked:?}"
    );
    assert_eq!(m.mem.listings(), listings, "no process was listed");
    assert_eq!(gave.moves[0].slot.as_deref(), Some("/somewhere/else"));
    assert_eq!((gave.moves[0].parked, gave.moves[0].set_aside), (1, 0));
    assert!(
        gave.moves[0]
            .to_string()
            .contains("which this run did not read")
    );
    let state = state_of(&m);
    assert!(state.moving.is_empty());
    assert!(state.aside.is_empty(), "{:?}", state.aside);
    assert_eq!(
        state.in_use(ProviderId::Desktop),
        here.as_ref(),
        "this data folder's record is kept"
    );
    assert!(
        state.other_slots.is_empty(),
        "the move's data folder records nobody: {:?}",
        state.other_slots
    );
    assert_eq!(
        park_of(&m, "work").map(|p| p.service),
        Some(park.clone()),
        "work's only session is its park again"
    );
    let manifest = crate::store::folder::manifest(&m.ctx, &park)
        .expect("reads")
        .expect("there");
    assert_eq!(manifest.items, ["Cookies"], "what the park holds");
    m.claude().crashes();
    hold(&m, "given up elsewhere");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn uninstall_refuses_where_claude_is_signed_out_whatever_the_record_says() {
    let uninstalled = |m: &Machine| {
        super::super::uninstall(
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
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
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

    let refused = super::super::uninstall(
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
    let removed = super::super::uninstall(
        settle(&m.ctx, Permit::for_a_test(), None)
            .expect("settled")
            .0,
    )
    .unwrap_or_else(|e| panic!("uninstalled: {e}"));
    assert!(removed.home_removed);
    assert!(!crate::home::dir(&m.ctx).exists());
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
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

    let removed = super::super::uninstall(
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
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn repair_says_a_move_it_cannot_settle_as_a_warning() {
    let m = desktop_pair("repair-waiting");
    let listings = m.mem.listings();
    let (_, warnings) = repair(settled(&m)).expect("repaired");
    assert!(warnings.is_empty());
    assert_eq!(
        m.mem.listings(),
        listings,
        "nothing of Claude is read where no move waits"
    );

    killed(&m, "leave.jar_out", "home");
    m.claude().opens();
    let (_, warnings) = repair(settled(&m)).expect("the rest is repaired");
    let not_settled = |warnings: &[Warning]| {
        let codes: Vec<&str> = warnings.iter().map(Warning::code).collect();
        assert_eq!(codes, ["move_not_settled"], "{warnings:?}");
        let said = warnings[0].to_string();
        assert!(said.contains("`desktop/work`'s login"), "{said}");
        said
    };
    let said = not_settled(&warnings);
    assert!(
        said.contains("because Claude is open. Quit Claude with Command-Q"),
        "{said}"
    );
    assert!(!said.contains("Keep Claude closed"), "{said}");
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());
    m.claude().crashes();
    let (_, warnings) = repair(settled(&m)).expect("repaired");
    assert!(
        matches!(warnings.as_slice(), [Warning::MoveSettled(moved)] if moved.ended == super::Ended::Finished),
        "{warnings:?}"
    );
    rested(&m, "settled by repair");

    // What no next change settles either is said with its own way out.
    let m = desktop_pair("repair-undetermined");
    killed(&m, "leave.jar_out", "home");
    std::fs::write(data(&m).join("config.json"), "[]").expect("settings not read");
    let (_, warnings) = repair(settled(&m)).expect("the rest is repaired");
    let said = not_settled(&warnings);
    assert!(said.contains("cannot tell where it stopped"), "{said}");
    assert!(said.contains("`pitboard abandon`"), "{said}");
    assert!(!said.contains("settles it first"), "{said}");
    assert!(
        !said.contains("Nothing has changed"),
        "repair went on: {said}"
    );
    assert!(
        state_of(&m).moving(ProviderId::Desktop).is_some(),
        "the move still waits"
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_failure_inside_recovery_keeps_the_record_and_never_settles_again() {
    let m = desktop_pair("recovery-fails");
    killed(&m, "arrive.jar_in", "home");
    let before = state_of(&m)
        .moving(ProviderId::Desktop)
        .cloned()
        .expect("recorded");
    let (stopped, tried) = crate::fault::failing_counted("move_in", || confirm_(&m, "work"));
    let stopped = stopped.expect_err("stopped");
    assert_eq!(stopped.code(), "recovery_undetermined");
    assert!(
        stopped.to_string().contains("could not be moved"),
        "{stopped}"
    );
    assert_eq!(tried, 1, "one move tried: recovery did not run again");
    assert_eq!(state_of(&m).moving(ProviderId::Desktop), Some(&before));
    assert_eq!(
        code(confirm_(&m, "work")),
        "account_not_in_use",
        "its next run lands"
    );
    rested(&m, "taken back");
    assert!(park_of(&m, "home").is_some());
}

// The jar goes back first, so a later item that will not move leaves something changed.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_recovery_that_stops_partway_never_says_nothing_has_changed() {
    let m = desktop_pair("recovery-partway");
    killed(&m, "arrive.jar_in", "home");
    assert_eq!(whereabouts(&m, &HOME), ["Claude"]);
    let before = state_of(&m)
        .moving(ProviderId::Desktop)
        .cloned()
        .expect("recorded");
    let stopped =
        crate::fault::failing_nth("move_in", 2, || confirm_(&m, "work")).expect_err("stopped");
    assert_eq!(stopped.code(), "recovery_undetermined");
    let said = stopped.to_string();
    assert!(!said.contains("Nothing has changed"), "{said}");
    assert!(
        said.contains("Keep Claude closed and run this again"),
        "{said}"
    );
    assert_eq!(
        whereabouts(&m, &HOME),
        std::slice::from_ref(&before.park),
        "its jar went back"
    );
    assert_eq!(state_of(&m).moving(ProviderId::Desktop), Some(&before));
    assert_eq!(
        code(confirm_(&m, "work")),
        "account_not_in_use",
        "its next run lands"
    );
    rested(&m, "taken back");
}

// A leave gone on with, and an arrive taken back past Claude's own emptied keys.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn every_row_that_stops_partway_never_says_nothing_has_changed() {
    let emptied: Value = serde_json::from_str(EMPTIED).expect("JSON");
    for (at, name, signed_out) in [
        ("leave.jar_out", "stops-jar-out", false),
        ("leave.item_out", "stops-item-out", false),
        ("arrive.jar_in", "stops-keys-aside", true),
    ] {
        let m = desktop_pair(name);
        killed(&m, at, "home");
        if signed_out {
            set(&m, "oauth:tokenCache", Some(emptied.clone()));
        }
        let before = state_of(&m)
            .moving(ProviderId::Desktop)
            .cloned()
            .expect("recorded");
        let folder = tree(&data(&m));
        let stopped =
            crate::fault::failing_nth("move_in", 2, || confirm_(&m, "work")).expect_err("stopped");
        assert_eq!(stopped.code(), "recovery_undetermined", "{at}");
        let said = stopped.to_string();
        assert!(!said.contains("Nothing has changed"), "{at}: {said}");
        assert_ne!(tree(&data(&m)), folder, "{at}: something moved first");
        let mut kept = state_of(&m)
            .moving(ProviderId::Desktop)
            .cloned()
            .expect("the record is kept");
        kept.asides
            .retain(|aside, _| before.asides.contains_key(aside));
        assert_eq!(kept, before, "{at}");
        assert_eq!(
            code(confirm_(&m, "work")),
            "account_not_in_use",
            "{at}: its next run lands"
        );
        rested(&m, at);
    }
}

// The parts left in Claude's folder go aside one at a time.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_give_up_that_stops_partway_never_says_nothing_has_changed() {
    let m = mixed("give-up-partway");
    let stopped =
        crate::fault::failing_nth("move_in", 2, || enrol(&m, "third")).expect_err("stopped");
    assert_eq!(stopped.code(), "recovery_undetermined");
    let said = stopped.to_string();
    assert!(!said.contains("Nothing has changed"), "{said}");
    assert!(state_of(&m).moving(ProviderId::Desktop).is_some());
    enrol(&m, "third").expect("the person's word is still the way out");
    assert_eq!(in_use(&m).as_deref(), Some("third"));
    hold(&m, "enrolled after a give-up that stopped");
    rested(&m, "enrolled after a give-up that stopped");
}

// A leave whose jar would not move is settled at once and undone.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_leave_that_fails_partway_is_undone_and_its_account_stays_in_use() {
    let m = desktop_pair("leave-undone");
    let keys = |m: &Machine| {
        let path = data(m).join("config.json");
        let settings: Map<String, Value> =
            serde_json::from_str(&std::fs::read_to_string(path).expect("settings")).expect("JSON");
        [
            "oauth:tokenCache",
            "oauth:tokenCacheV2",
            "lastKnownAccountUuid",
        ]
        .map(|key| settings.get(key).cloned())
    };
    let held = keys(&m);
    let undone = crate::fault::failing_nth("move_in", 1, || refused(&m, "home"));
    assert_eq!(undone.code(), "move_undone", "{undone}");
    assert!(
        undone.to_string().contains("`desktop/work` is in use"),
        "{undone}"
    );
    assert_eq!(in_use(&m).as_deref(), Some("work"));
    assert_eq!(keys(&m), held, "its keys are back");
    assert_eq!(whereabouts(&m, &WORK), ["Claude"]);
    assert!(state_of(&m).moving.is_empty());
    hold(&m, "a leave undone");
    rested(&m, "a leave undone");
}

// What Claude left after a Log out, set aside before the arrive failed and was undone, is
// said beside the refusal, whether only its keys or all of it had gone.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn an_arrive_undone_at_once_says_what_it_set_aside() {
    for nth in [1, 7] {
        let m = desktop_pair(&format!("arrive-undone-{nth}"));
        let claude = m.claude();
        claude.opens();
        claude.logs_out();
        claude.quits();
        let lines = audit_lines(&m, "in-use");
        let mut stopped = crate::fault::failing_nth("move_in", nth, || refused(&m, "home"));
        assert_eq!(stopped.code(), "switch_left_signed_out", "{nth}: {stopped}");
        // Saved with the arrive's record, whatever the arrive came to.
        assert_eq!(
            audit_lines(&m, "in-use")[lines.len()..],
            [
                (String::new(), "signed_out_outside".to_string()),
                ("desktop/work".to_string(), "login_replaced".to_string()),
            ],
            "{nth}"
        );
        let said = stopped.take_warnings();
        let aside = state_of(&m)
            .aside
            .iter()
            .find(|aside| aside.why == SetAside::InTheWay)
            .map(|aside| crate::store::folder::at(&m.ctx, &aside.name))
            .expect("set aside");
        assert!(
            said.iter().any(|warning| matches!(
                warning,
                Warning::SetAside { why: SetAside::InTheWay, path, .. } if *path == aside
            )),
            "{nth}: {said:?}"
        );
        assert!(
            !said
                .iter()
                .any(|warning| matches!(warning, Warning::MoveSettled(_))),
            "{nth}: the refusal says it was undone: {said:?}"
        );
        rested(&m, "an arrive undone at once");
    }
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn claude_seen_after_the_last_write_keeps_the_record() {
    let m = desktop_pair("third-look");
    let app = m.claude();
    let midway = crate::fault::meanwhile(
        "arrive.keys_in",
        move || app.opens(),
        || refused(&m, "home"),
    );
    assert_eq!(midway.code(), "app_opened_midway");
    assert!(
        midway.to_string().contains("moved `desktop/home`'s login"),
        "{midway}"
    );
    let state = state_of(&m);
    assert_eq!(
        state.moving(ProviderId::Desktop).map(|r| r.half),
        Some(state::Half::Arrive)
    );
    assert_eq!(in_use(&m), None, "nothing is recorded as done");
    m.claude().crashes();
    assert!(matches!(used(&m, "home").0, Outcome::AlreadyActive { .. }));
    rested(&m, "recorded by the next change");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_flush_that_fails_after_a_rename_is_a_move_made() {
    let m = desktop_pair("not-synced");
    let (outcome, _) = crate::fault::failing("move_in.flushed", || used(&m, "home"));
    assert!(matches!(outcome, Outcome::Switched { .. }));
    assert_eq!(in_use(&m).as_deref(), Some("home"));
    rested(&m, "moved, not synced");
}

// Claude Code's `here` in use and `there` parked, with Claude Desktop's pair beside them.
fn three_tools(name: &str) -> Machine {
    beside_desktop(super::super::harness::machine(name))
}

// `m`'s own tool's `here` in use and `there` parked, with Claude Desktop's pair beside them.
fn beside_desktop(mut m: Machine) -> Machine {
    m.ctx = m
        .ctx
        .clone()
        .with_awaited_every(std::time::Duration::from_millis(2))
        .with_scratch_claude(m.ctx_home().join("claude"));
    m.which = ProviderId::Desktop;
    m.claude().signed_in_as(&WORK);
    enrol(&m, "work").expect("`work` enrolled");
    sign_in_inside(&m, "home", &HOME).expect("`home` added");
    used(&m, "work");
    m
}

// A switch of `tool` to `there` killed after its record, before anything moved.
fn interrupted(m: &Machine, tool: ProviderId) {
    let (settled, _) = settle(&m.ctx, Permit::for_a_test(), Some(tool)).expect("nothing waits");
    let died = crate::fault::killing("switch.journal_written", || {
        switch(settled, &Key::new(tool, "there"))
    });
    assert_eq!(died.expect_err("killed"), "switch.journal_written");
    assert!(super::super::journal::pending(&m.ctx));
}

// Claude's data folder and Pitboard's parks, each entry with its bytes.
type Claudes = (
    Vec<(PathBuf, Option<Vec<u8>>)>,
    Vec<(PathBuf, Option<Vec<u8>>)>,
);

fn claudes(m: &Machine) -> Claudes {
    (
        tree(&data(m)),
        tree(&crate::store::folder::parks_dir(&m.ctx)),
    )
}

// The switch given up and said, and the move still waiting, said with why.
fn switch_given_up_beside(m: &Machine, gave: &super::super::GaveUp, why: &str) {
    assert!(gave.switch.is_some(), "{gave:?}");
    assert!(!super::super::journal::pending(&m.ctx));
    assert!(gave.moves.is_empty(), "{gave:?}");
    assert_eq!(
        gave.warnings.iter().map(Warning::code).collect::<Vec<_>>(),
        ["move_not_given_up"]
    );
    let said = gave.warnings[0].to_string();
    assert!(
        said.contains("`desktop/work`") && said.contains(why),
        "{said}"
    );
    assert!(
        !said.to_lowercase().contains("nothing has changed"),
        "{said}"
    );
    assert!(state_of(m).moving(ProviderId::Desktop).is_some());
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn claude_open_never_stops_abandon_giving_up_a_claude_code_switch() {
    let m = three_tools("abandon-open");
    killed(&m, "leave.jar_out", "home");
    interrupted(&m, ProviderId::Claude);
    m.claude().opens();
    let folder = claudes(&m);
    let gave = gave_up(&m).expect("the switch is given up");
    switch_given_up_beside(&m, &gave, "Claude is open");
    assert_eq!(
        gave.warnings[0].to_string(),
        "Pitboard did not give up the interrupted move of `desktop/work`'s login, because Claude \
         is open. Quit Claude with Command-Q, then run `pitboard abandon` again."
    );
    assert_eq!(claudes(&m), folder, "nothing of Claude's moved");
    let before = snapshot(&m);
    let refused = gave_up(&m).expect_err("the move alone, refused as before");
    assert_eq!(refused.code(), "app_open");
    unchanged(&m, &before, "nothing changed");
    m.claude().crashes();
    let gave = gave_up(&m).expect("settled");
    assert!(gave.switch.is_none() && gave.moves.is_empty());
    assert_eq!(
        gave.warnings.iter().map(Warning::code).collect::<Vec<_>>(),
        ["interrupted_move_finished"]
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_codex_switch_is_given_up_beside_a_move_that_cannot_be() {
    let m = beside_desktop(super::super::harness::codex_machine("abandon-mixed"));
    killed(&m, "leave.item_out", "home");
    let claude = m.claude();
    claude.opens();
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    interrupted(&m, ProviderId::Codex);
    let folder = claudes(&m);
    let gave = gave_up(&m).expect("the switch is given up");
    switch_given_up_beside(&m, &gave, "beside another login in Claude's folder");
    assert!(
        gave.warnings[0]
            .to_string()
            .contains("`Local Storage` in Claude's folder"),
        "{}",
        gave.warnings[0]
    );
    assert_eq!(claudes(&m), folder, "nothing of Claude's moved");

    let m = beside_desktop(super::super::harness::codex_machine("abandon-unreadable"));
    killed(&m, "leave.jar_out", "home");
    let park = state_of(&m)
        .moving(ProviderId::Desktop)
        .expect("recorded")
        .park
        .clone();
    m.mem
        .jar_unsettled(&crate::store::folder::item(&m.ctx, &park, "Cookies"));
    interrupted(&m, ProviderId::Codex);
    let folder = claudes(&m);
    let gave = gave_up(&m).expect("the switch is given up");
    switch_given_up_beside(&m, &gave, "(a test held the jar)");
    let said = gave.warnings[0].to_string();
    assert!(
        said.contains(
            &crate::store::folder::at(&m.ctx, &park)
                .display()
                .to_string()
        ),
        "{said}"
    );
    assert!(!said.contains("Open Claude"), "{said}");
    assert_eq!(claudes(&m), folder, "nothing of Claude's moved");
}

// The change settles the move and refuses. Run again, it refuses alone and changes nothing,
// so the first refusal changed nothing past the settle.
fn settled_then_refused(
    m: &Machine,
    code: &str,
    settled: &str,
    change: impl Fn(&crate::service::Pitboard) -> crate::service::Failed,
) {
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    let failed = change(&pitboard);
    assert_eq!(failed.error.code(), code, "{}", failed.error);
    // The service gives the refusal back as itself, as every front end matches it.
    assert!(
        !matches!(failed.error, Error::SaidFirst { .. }),
        "{:?}",
        failed.error
    );
    assert!(
        failed.warnings[..failed.said_first]
            .iter()
            .any(|warning| warning.code() == settled),
        "{code}: {:?}",
        failed.warnings
    );
    assert!(state_of(m).moving(ProviderId::Desktop).is_none(), "{code}");
    let before = snapshot(m);
    let again = change(&pitboard);
    assert_eq!(again.error.code(), code, "{}", again.error);
    assert!(
        !again
            .warnings
            .iter()
            .any(|warning| warning.code().starts_with("interrupted_move_")),
        "{code}: {:?}",
        again.warnings
    );
    unchanged(m, &before, code);
}

// Each change settles a move a killed leave left, then refuses, or forgets by the doubt rule.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_refusal_after_a_settled_move_says_it_settled() {
    let finished = "interrupted_move_finished";
    let undone = "interrupted_move_undone";
    let m = desktop_pair("settled-then-not-in-use");
    killed(&m, "leave.jar_out", "home");
    settled_then_refused(&m, "account_not_in_use", finished, |pitboard| {
        pitboard.update_config("desktop/work").expect_err("refused")
    });

    let m = desktop_pair("settled-then-unknown");
    killed(&m, "leave.jar_out", "home");
    std::fs::create_dir(data(&m).join("Network")).expect("an entry not read");
    settled_then_refused(&m, "login_folder_unknown", finished, |pitboard| {
        pitboard.switch_to("desktop/home").expect_err("refused")
    });

    let m = desktop_pair("settled-then-stranded");
    killed(&m, "leave.jar_out", "home");
    settled_then_refused(&m, "uninstall_would_strand", finished, |pitboard| {
        pitboard.uninstall().expect_err("refused")
    });
    assert!(crate::home::dir(&m.ctx).is_dir());
    assert!(state_of(&m).get(&m.key("work")).is_some());
    assert!(state_of(&m).get(&m.key("home")).is_some());

    let m = desktop_pair("settled-then-no-sign-in");
    killed(&m, "leave.jar_out", "home");
    std::fs::create_dir(data(&m).join("File System")).expect("an item the move never listed");
    settled_then_refused(&m, "login_folder_unknown", finished, |pitboard| {
        pitboard
            .sign_in_watched("desktop/travel")
            .expect_err("refused")
    });

    let m = desktop_pair("settled-then-signed-out");
    killed(&m, "leave.jar_out", "home");
    settled_then_refused(&m, "live_credential_absent", finished, |pitboard| {
        pitboard
            .enroll_current("desktop/travel")
            .expect_err("refused")
    });

    // Undone, so `work` is in use again.
    let m = desktop_pair("settled-then-in-use");
    killed(&m, "leave.keys_out", "home");
    settled_then_refused(&m, "cannot_forget_active_account", undone, |pitboard| {
        pitboard.forget("desktop/work").expect_err("refused")
    });
    assert!(state_of(&m).get(&m.key("work")).is_some());

    // With nobody recorded in use, the undone login is a session not seen: the doubt rule
    // refuses, since `work` holds no park.
    let m = desktop_pair("settled-then-doubt-unparked");
    killed(&m, "leave.keys_out", "home");
    let mut state = state_of(&m);
    state.in_use.remove(ProviderId::Desktop.code());
    state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
    settled_then_refused(&m, "login_in_doubt", undone, |pitboard| {
        pitboard.forget("desktop/work").expect_err("refused")
    });
    assert!(state_of(&m).get(&m.key("work")).is_some());

    // Forgotten by the doubt rule: `File System` was not there when the leave started.
    let m = desktop_pair("settled-then-doubt");
    killed(&m, "leave.jar_out", "home");
    std::fs::create_dir(data(&m).join("File System")).expect("an item the move never listed");
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    let done = pitboard.forget("desktop/home").expect("forgotten");
    assert!(
        done.warnings
            .iter()
            .any(|warning| warning.code() == "interrupted_move_finished"),
        "{:?}",
        done.warnings
    );
    assert!(state_of(&m).get(&m.key("home")).is_none());
}

// A step that fails after the change settled a move still says the move settled.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_failure_after_a_settled_move_still_says_it_settled() {
    use crate::host::fs::testing;
    let undone = "interrupted_move_undone";
    let m = desktop_pair("settled-then-scheduled");
    killed(&m, "leave.keys_out", "home");
    crate::schedule::install(&m.ctx, Permit::for_a_test()).expect("scheduled");
    let dir = crate::schedule::path(&m.ctx)
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
        .expect("where the scheduler keeps it");
    testing::deny_changes(&dir).expect("read-only");
    settled_then_refused(&m, "home_unwritable", undone, |pitboard| {
        pitboard.uninstall().expect_err("refused")
    });
    testing::allow_changes(&dir).expect("changeable again");
    assert!(crate::home::dir(&m.ctx).is_dir());

    let m = desktop_pair("settled-then-unsaved");
    let park = park_of(&m, "home").expect("parked").service;
    killed(&m, "leave.keys_out", "home");
    let file = crate::home::dir(&m.ctx).join("state.json");
    let kept = crate::home::dir(&m.ctx).join("state.kept");
    let failed = crate::fault::meanwhile(
        "settle.saved",
        {
            let (file, kept) = (file.clone(), kept.clone());
            move || {
                std::fs::rename(&file, &kept).expect("moved");
                std::fs::create_dir_all(file.join("in the way")).expect("a folder in its place");
            }
        },
        || forget(settled(&m), &m.key("home")),
    );
    std::fs::remove_dir_all(&file).expect("taken out of the way");
    std::fs::rename(&kept, &file).expect("put back");
    let mut failed = failed.expect_err("refused");
    assert_eq!(failed.code(), "state_write_failed", "{failed}");
    let said = failed.take_warnings();
    assert!(
        said.iter().any(|warning| warning.code() == undone),
        "{said:?}"
    );
    // What forget deletes goes only after its record is saved.
    assert!(crate::store::folder::at(&m.ctx, &park).is_dir());
    assert_eq!(whereabouts(&m, &HOME), [park]);
}

// Claude ran as whoever its folder held and renewed that account's tokens.
fn renewed(m: &Machine) {
    let path = data(m).join("config.json");
    let written = std::fs::read_to_string(&path).expect("settings");
    let renewed = [WORK, HOME].iter().fold(written.clone(), |held, who| {
        held.replacen(
            &format!("|{}\"", who.session),
            &format!("|{}-renewed\"", who.session),
            1,
        )
    });
    assert_ne!(renewed, written, "tokens to renew");
    std::fs::write(&path, renewed).expect("Claude's own write");
}

// What settling set aside is said before the refusal, with the move it settled. The keys
// Claude rewrote since the arrive are all the folder kept holds.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_refusal_after_a_settled_move_says_all_it_did_first() {
    let m = desktop_pair("superseded-then-refused");
    killed(&m, "arrive.keys_in", "home");
    renewed(&m);
    let failed = crate::service::Pitboard::new(m.ctx.clone())
        .update_config("desktop/work")
        .expect_err("refused");
    assert_eq!(
        failed.error.code(),
        "account_not_in_use",
        "{}",
        failed.error
    );
    let codes =
        |said: &[Warning]| -> Vec<&'static str> { said.iter().map(Warning::code).collect() };
    assert_eq!(
        codes(&failed.warnings),
        ["interrupted_move_finished", "set_aside"]
    );
    assert_eq!(failed.said_first, 2);
    assert_eq!(
        failed.warnings[1].to_string(),
        format!(
            "Claude rewrote `desktop/home`'s keys. Pitboard kept the earlier ones in {} and \
             deleted nothing. `pitboard doctor` lists them.",
            superseded(&m).display()
        )
    );
}

fn superseded(m: &Machine) -> PathBuf {
    state_of(m)
        .aside
        .iter()
        .find(|aside| aside.why == SetAside::Superseded)
        .map(|aside| crate::store::folder::at(&m.ctx, &aside.name))
        .expect("kept aside")
}

// Keys Claude rewrote while a leave waited are kept aside, and the change says so.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn keys_claude_rewrote_during_a_leave_are_said_set_aside() {
    let m = desktop_pair("keys-rewritten");
    killed(&m, "leave.keys_kept", "home");
    renewed(&m);
    let done = crate::service::Pitboard::new(m.ctx.clone())
        .update_config("desktop/work")
        .expect("`work` in use");
    // The leave never moved the jar: the folder kept holds only the keys.
    let said: Vec<String> = done
        .warnings
        .iter()
        .filter(|warning| warning.code() == "set_aside")
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        said,
        [format!(
            "Claude rewrote `desktop/work`'s keys. Pitboard kept the earlier ones in {} and \
             deleted nothing. `pitboard doctor` lists them.",
            superseded(&m).display()
        )]
    );
}

// The enrolment gives the move up and sets `home`'s older park aside, then cannot save: it
// says only what giving up saved.
#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn an_enrolment_that_cannot_save_says_nothing_it_did_not_save() {
    let m = desktop_pair("enrol-unsaved");
    killed(&m, "leave.keys_kept", "home");
    names(&m, &HOME);
    let file = crate::home::dir(&m.ctx).join("state.json");
    let kept = crate::home::dir(&m.ctx).join("state.kept");
    let failed = crate::fault::meanwhile(
        "give_up.done",
        {
            let (file, kept) = (file.clone(), kept.clone());
            move || {
                std::fs::rename(&file, &kept).expect("moved");
                std::fs::create_dir_all(file.join("in the way")).expect("a folder in its place");
            }
        },
        || enrol(&m, "home"),
    );
    std::fs::remove_dir_all(&file).expect("taken out of the way");
    std::fs::rename(&kept, &file).expect("put back");
    let mut failed = failed.expect_err("refused");
    assert_eq!(failed.code(), "state_write_failed", "{failed}");
    let said = failed.take_warnings();
    assert!(
        !said.iter().any(|warning| matches!(
            warning,
            Warning::SetAside {
                why: SetAside::Superseded,
                ..
            } | Warning::TakenAsNamed { .. }
        )),
        "{said:?}"
    );
    let state = state_of(&m);
    assert!(state.moving(ProviderId::Desktop).is_none(), "given up");
    assert!(
        state
            .aside
            .iter()
            .all(|aside| aside.why != SetAside::Superseded),
        "{:?}",
        state.aside
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_sign_in_inside_claude_says_what_settling_found_first() {
    let m = three_tools("sign-in-settles");
    interrupted(&m, ProviderId::Claude);
    let waiting = watched(&m, "travel").expect("waiting");
    let lines = said_until(&waiting.said(), "Waiting for Claude to quit");
    waiting.cancel();
    assert!(!super::super::journal::pending(&m.ctx), "settled");
    let parked = lines
        .iter()
        .position(|line| line.starts_with("Parked `desktop/work`"))
        .expect("parked");
    assert!(
        lines[..parked]
            .iter()
            .any(|line| line.contains("switch") && line.contains("interrupted")),
        "{lines:?}"
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_folder_move_waiting_does_not_stop_a_claude_code_switch() {
    let m = three_tools("three-tools");
    killed(&m, "leave.jar_out", "home");
    let waiting = state_of(&m).moving(ProviderId::Desktop).cloned();
    let folder = tree(&data(&m));
    let (listings, jars) = (m.mem.listings(), m.mem.jars_asked().len());
    let without = |state: &mut state::State| {
        state.moving.clear();
    };
    let as_if_none = || {
        let mut state = state_of(&m);
        without(&mut state);
        state
    };

    let ledger = crate::duty::autoswitch::Ledger::load(&m.ctx);
    let judged = |state: &state::State| {
        let here = state
            .get(&Key::new(ProviderId::Claude, "here"))
            .expect("enrolled");
        format!(
            "{:?}",
            crate::duty::autoswitch::judge(
                &m.ctx,
                state,
                here,
                &ledger,
                crate::duty::autoswitch::Threshold::DEFAULT,
                NOW
            )
        )
    };
    assert_eq!(judged(&state_of(&m)), judged(&as_if_none()));
    let was = state_of(&m).in_use(ProviderId::Claude).cloned();

    let (settled, _) =
        settle(&m.ctx, Permit::for_a_test(), Some(ProviderId::Claude)).expect("nothing waits");
    let (outcome, _) = switch(settled, &Key::new(ProviderId::Claude, "there")).expect("switched");
    assert!(matches!(outcome, Outcome::Switched { .. }), "{outcome:?}");
    let state = state_of(&m);
    assert_eq!(
        state
            .account_in_use(ProviderId::Claude)
            .map(|a| a.label.clone()),
        Some("there".into())
    );

    // `here`, parked by that switch, due: it is renewed as with no move waiting.
    let mut due = state_of(&m);
    let here = Key::new(ProviderId::Claude, "here");
    let park = due
        .accounts
        .iter_mut()
        .find(|a| a.is(&here))
        .and_then(|a| a.parked.as_mut())
        .expect("parked");
    park.access_expires_at = Some(NOW - 1);
    state::save(&m.ctx, Permit::for_a_test(), &due).expect("saved");
    m.api.renews(
        "here-refresh",
        crate::provider::claude::api::Renewed {
            access_token: "renewed-access".into(),
            refresh_token: Some("here-renewed".into()),
            expires_in: 3600,
            refresh_token_expires_in: Some(30 * 86_400),
            scopes: None,
            at: None,
            owner: None,
        },
    );
    let renewed = crate::switch::renew::renew_due(
        &m.ctx,
        Permit::for_a_test(),
        crate::switch::renew::Due::ToBeAsked,
    );
    assert!(
        matches!(renewed.as_slice(), [(key, renewal)] if *key == here && renewal.code() == "renewed"),
        "{renewed:?}"
    );

    // A record that says `here` while `there` is stored: the read is recorded.
    let found = state.in_use(ProviderId::Claude).cloned().expect("recorded");
    let mut stale = state_of(&m);
    stale
        .in_use
        .insert("claude".into(), was.expect("`here` was in use"));
    state::save(&m.ctx, Permit::for_a_test(), &stale).expect("saved");
    crate::switch::identify::record_read(
        &m.ctx,
        Permit::for_a_test(),
        &[(ProviderId::Claude, found.clone())],
        &std::collections::BTreeMap::new(),
    );
    let recorded = state_of(&m).in_use(ProviderId::Claude).cloned();
    assert!(
        recorded.is_some_and(|recorded| recorded.agrees(ProviderId::Claude, &found)),
        "the read was recorded"
    );
    assert_eq!(state_of(&m).moving(ProviderId::Desktop).cloned(), waiting);
    assert_eq!(
        tree(&data(&m)),
        folder,
        "Claude's folder as the kill left it"
    );
    assert_eq!(
        (m.mem.listings(), m.mem.jars_asked().len()),
        (listings, jars),
        "nothing of Claude was read"
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_name_refused_never_settles_a_folder_move() {
    let m = desktop_pair("refused-name");
    killed(&m, "leave.jar_out", "home");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let (listings, jars) = (m.mem.listings(), m.mem.jars_asked().len());
    let pitboard = crate::service::Pitboard::new(m.ctx.clone());
    let refused = pitboard.switch_to("no-such-account").expect_err("refused");
    assert!(refused.warnings.is_empty(), "{:?}", refused.warnings);
    assert!(pitboard.renew().is_ok());
    unchanged(&m, &before, "the move was not settled by any of it");
    assert_eq!(
        (m.mem.listings(), m.mem.jars_asked().len()),
        (listings, jars)
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn recovery_stops_where_the_jar_is_in_neither_place_or_both() {
    let m = desktop_pair("neither-or-both");
    killed(&m, "leave.jar_out", "home");
    let park = state_of(&m)
        .moving(ProviderId::Desktop)
        .expect("recorded")
        .park
        .clone();
    let parked = crate::store::folder::item(&m.ctx, &park, "Cookies");
    // In both: the parked jar copied back, as a restore from a backup would.
    std::fs::copy(&parked, data(&m).join("Cookies")).expect("copied");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let stopped = confirm_(&m, "work").expect_err("stopped");
    assert_eq!(stopped.code(), "recovery_undetermined");
    assert!(stopped.to_string().contains("in both places"), "{stopped}");
    unchanged(&m, &before, "in both places");
    std::fs::remove_file(data(&m).join("Cookies")).expect("the copy gone");

    // In neither: the parked jar taken away.
    let away = m.ctx_home().join("Cookies-away");
    std::fs::rename(&parked, &away).expect("taken away");
    let before = snapshot(&m);
    let stopped = confirm_(&m, "work").expect_err("stopped");
    assert_eq!(stopped.code(), "recovery_undetermined");
    assert!(
        stopped.to_string().contains("in neither place"),
        "{stopped}"
    );
    unchanged(&m, &before, "in neither place");

    std::fs::rename(&away, &parked).expect("back");
    assert_eq!(code(confirm_(&m, "work")), "account_not_in_use");
    assert!(park_of(&m, "work").is_some(), "the leave was finished");
    rested(&m, "finished once the jar was back");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_leave_finished_after_claude_ran_signed_out_verifies_and_commits() {
    let m = desktop_pair("ran-signed-out");
    killed(&m, "leave.item_out", "home");
    let claude = m.claude();
    claude.opens();
    claude.quits();
    let inode = |item: &str| crate::host::fs::testing::inode(&data(&m).join(item)).ok();
    let own = (inode("Cookies"), inode("Cookies-journal"));
    assert!(
        own.0.is_some() && own.1.is_some(),
        "Claude made its own jar"
    );
    let settings = std::fs::read(data(&m).join("config.json")).expect("settings");
    assert!(String::from_utf8_lossy(&settings).contains(EMPTIED));

    let gave = gave_up(&m).expect("settled");
    assert!(gave.moves.is_empty(), "nothing given up: {gave:?}");
    assert!(
        matches!(
            gave.warnings.as_slice(),
            [Warning::MoveSettled(moved)]
                if moved.half == state::Half::Leave && moved.ended == super::Ended::Finished
        ),
        "{:?}",
        gave.warnings
    );
    assert_eq!((inode("Cookies"), inode("Cookies-journal")), own);
    assert_eq!(
        std::fs::read(data(&m).join("config.json")).expect("settings"),
        settings,
        "the settings are not written"
    );
    let parked = park_of(&m, "work").expect("parked");
    let manifest = crate::store::folder::manifest(&m.ctx, &parked.service)
        .expect("reads")
        .expect("there");
    assert!(manifest.items.contains(&"Local Storage".to_string()));
    assert!(!data(&m).join("Local Storage").exists());
    assert_eq!(whereabouts(&m, &WORK), [parked.service]);
    assert_eq!(in_use(&m), None);
    hold(&m, "a leave finished");
    rested(&m, "a leave finished");
    assert!(matches!(used(&m, "home").0, Outcome::Installed { .. }));
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_temporary_left_inside_a_settings_write_is_swept() {
    let m = desktop_pair("temporary");
    let settings = std::fs::read(data(&m).join("config.json")).expect("settings");
    killed(&m, "settings.temp_written", "home");
    let temporaries = || {
        std::fs::read_dir(data(&m))
            .expect("listed")
            .flatten()
            .filter(|entry| {
                crate::atomic::temp_owner(&entry.file_name().to_string_lossy()).is_some()
            })
            .count()
    };
    assert_eq!(temporaries(), 1, "the kill left one");
    assert!(matches!(
        confirm_(&m, "work").expect("undone"),
        Outcome::AlreadyActive { .. }
    ));
    assert_eq!(temporaries(), 0, "swept");
    assert_eq!(
        std::fs::read(data(&m).join("config.json")).expect("settings"),
        settings
    );
    rested(&m, "swept");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_power_cut_that_undid_a_committed_rename_is_refused_and_moves_nothing() {
    let m = desktop_pair("power-cut");
    killed(&m, "leave.done", "home");
    let park = park_of(&m, "work").expect("parked").service;
    std::fs::rename(
        crate::store::folder::item(&m.ctx, &park, "Cookies"),
        data(&m).join("Cookies"),
    )
    .expect("the rename undone");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    let stopped = refused(&m, "home");
    assert!(
        ["sign_in_unfinished", "login_in_doubt"].contains(&stopped.code()),
        "row 12 or 20: {stopped}"
    );
    unchanged(&m, &before, "nothing deleted, nothing moved");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
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
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn no_folder_name_reaches_the_vault() {
    let m = desktop_pair("no-vault");
    used(&m, "home");
    killed(&m, "leave.jar_out", "work");
    gave_up(&m).expect("given up");
    repair(settled(&m)).expect("repaired");
    forget_(&m, "home").expect("forgotten");
    let asked: Vec<_> = m
        .mem
        .trail()
        .into_iter()
        .filter(|touch| touch.store == "vault")
        .filter(|touch| {
            touch.service.starts_with("pitboard-park-desktop_")
                || touch
                    .service
                    .starts_with(crate::store::folder::ASIDE_PREFIX)
        })
        .collect();
    assert!(asked.is_empty(), "{asked:?}");
    assert!(
        crate::store::folder::list(&m.ctx)
            .expect("parks")
            .is_some_and(|names| !names.is_empty()),
        "folders there were, all the while"
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn uninstall_and_repair_read_nothing_of_claude_where_nothing_of_it_is_parked() {
    let settled_all = |m: &Machine| {
        settle(&m.ctx, Permit::for_a_test(), None)
            .expect("settled")
            .0
    };
    // No scratch Claude named: main's machine.
    let m = super::super::harness::machine("nothing-of-claude");
    repair(settled_all(&m)).expect("repaired");
    super::super::uninstall(settled_all(&m)).expect("uninstalled");
    assert_eq!((m.mem.listings(), m.mem.jars_asked().len()), (0, 0));

    // Claude Desktop enrolled, in use, nothing of it parked.
    let m = desktop_machine("nothing-parked");
    let (listings, jars) = (m.mem.listings(), m.mem.jars_asked().len());
    repair(settled_all(&m)).expect("repaired");
    let removed = super::super::uninstall(settled_all(&m)).expect("uninstalled");
    assert!(removed.warnings.is_empty());
    assert_eq!(
        (m.mem.listings(), m.mem.jars_asked().len()),
        (listings, jars)
    );
    assert_eq!(whereabouts(&m, &WORK), ["Claude"]);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn a_second_aside_never_writes_over_the_first() {
    let m = desktop_pair("two-asides");
    killed(&m, "arrive.jar_in", "home");
    // `home`'s jar is in Claude's folder with no keys. Claude's own, emptied, come by hand.
    let emptied: Value = serde_json::from_str(EMPTIED).expect("JSON");
    set(&m, "oauth:tokenCache", Some(emptied.clone()));
    let recovering = |at: &'static str| {
        let died = crate::fault::killing(at, || confirm_(&m, "work"));
        assert_eq!(died.expect_err("killed"), at);
        as_a_dead_runs(&m);
        state_of(&m)
            .moving(ProviderId::Desktop)
            .expect("the record is kept")
            .asides
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };
    let first = recovering("settle.aside_kept");
    assert_eq!(first.len(), 1, "{first:?}");
    let kept = |name: &str| {
        std::fs::read(crate::store::folder::keys_file(&m.ctx, name)).expect("its keys")
    };
    let first_keys = kept(&first[0]);
    set(&m, "oauth:tokenCacheV2", Some(emptied));
    let both = recovering("settle.keys_cleared");
    assert_eq!(
        both.len(),
        2,
        "a new aside, never the one the record names: {both:?}"
    );
    assert_eq!(
        kept(&first[0]),
        first_keys,
        "the first is as it was written"
    );
    let second = both
        .iter()
        .find(|name| **name != first[0])
        .expect("new")
        .clone();
    let second_keys = kept(&second);
    assert_ne!(second_keys, first_keys);

    // Recovery takes `home`'s jar back and is killed once it has; then somebody signs in.
    let died = crate::fault::killing("settle.item_back", || confirm_(&m, "work"));
    assert_eq!(died.expect_err("killed"), "settle.item_back");
    as_a_dead_runs(&m);
    let claude = m.claude();
    claude.opens();
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    assert_eq!(code(confirm_(&m, "work")), "recovery_undetermined");
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let (ctx, named) = (m.ctx.clone(), std::rc::Rc::clone(&seen));
    let read = move || {
        let state = state::load(&ctx).expect("state");
        let moving = state.moving(ProviderId::Desktop).expect("recorded");
        *named.borrow_mut() = moving.asides.keys().cloned().collect::<Vec<_>>();
    };
    crate::fault::meanwhile("give_up.aside_named", read, || enrol(&m, "third"))
        .expect("enrolled, what `home` left set aside");
    let all = seen.borrow().clone();
    assert_eq!(all.len(), 3, "a third, never one the record names: {all:?}");
    let given_up = all.iter().find(|name| !both.contains(name)).expect("new");
    assert_eq!(kept(&first[0]), first_keys, "each as it was written");
    assert_eq!(kept(&second), second_keys, "each as it was written");

    let state = state_of(&m);
    let why = |name: &str| {
        let aside = state.aside.iter().find(|a| a.name == name);
        aside.map(|a| (a.why, a.login.clone()))
    };
    for name in &both {
        assert_eq!(why(name), Some((SetAside::InTheWay, None)), "{name}");
    }
    assert_eq!(why(given_up), Some((SetAside::GivenUp, None)));
    assert!(park_of(&m, "home").is_some(), "`home` keeps its park");
    assert_eq!(in_use(&m).as_deref(), Some("third"));
    hold(&m, "three asides");
    rested(&m, "three asides");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn the_start_of_a_sign_in_writes_a_manifest_before_it_sets_keys_aside() {
    for at in ["start.aside_kept", "start.keys_cleared"] {
        let m = desktop_machine(&format!("start-{at}"));
        let claude = m.claude();
        claude.opens();
        claude.logs_out();
        claude.quits();
        let died = crate::fault::killing(at, || watched(&m, "travel").map(drop));
        assert_eq!(died.expect_err("killed"), at);
        let asides = crate::store::folder::list(&m.ctx)
            .expect("parks")
            .expect("there");
        assert_eq!(asides.len(), 1, "{at}: {asides:?}");
        let manifest = crate::store::folder::manifest(&m.ctx, &asides[0])
            .expect("reads")
            .expect("written before the settings");
        assert_eq!((manifest.account, manifest.login), (None, None));
        let kept = std::fs::read_to_string(crate::store::folder::keys_file(&m.ctx, &asides[0]))
            .expect("the keys, kept");
        assert!(kept.contains(WORK.uuid), "{at}: {kept}");
        assert_eq!(
            named_in_settings(&m).is_some(),
            at == "start.aside_kept",
            "{at}: the keys are in the settings or in the aside, never in neither"
        );
        assert!(
            state_of(&m).aside.is_empty(),
            "{at}: only `pending` names it"
        );

        recover(&m).expect("the next command's sweep");
        let state = state_of(&m);
        assert_eq!(state.aside.len(), 1, "{at}");
        assert_eq!(state.aside[0].why, SetAside::Orphan, "{at}");
        assert!(
            crate::store::folder::at(&m.ctx, &asides[0]).is_dir(),
            "{at}: kept"
        );
        watched(&m, "travel")
            .expect("the sign-in starts again")
            .cancel();
    }
}

// Until the process list has been asked `n` times in all: a wait that stops asking fails.
fn listed(m: &Machine, n: usize) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while m.mem.listings() < n {
        assert!(
            std::time::Instant::now() < deadline,
            "the wait stopped asking the process list"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn the_wait_goes_on_where_claude_opens_again_or_another_pitboard_holds_the_folder() {
    let m = desktop_pair("wait-goes-on");
    let waiting = watched(&m, "travel").expect("waiting");
    let (said, steps, claude) = (waiting.said(), waiting.steps(), m.claude());
    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.reports();
    // Claude quits and is open again before the wait has looked: it waits for the next quit.
    let change = super::super::exclusive(&m.ctx, Permit::for_a_test()).expect("state.lock");
    claude.quits();
    // A listing counted after this read Claude closed: the wait is in its look, behind the lock.
    let quit = m.mem.listings();
    listed(&m, quit + 1);
    claude.opens();
    drop(change);
    let listings = m.mem.listings();
    listed(&m, listings + 5);
    assert_eq!(steps.now(), Some(super::super::WaitStep::Open));
    // Another Pitboard holds the folder as Claude quits: the wait looks again at the next tick.
    let held = crate::host::fs::lock_dir(&data(&m)).expect("another home holds the folder");
    claude.quits();
    let listings = m.mem.listings();
    listed(&m, listings + 5);
    held.unlock().expect("let go");
    let login = waiting
        .finish()
        .expect("it looked again, and somebody is signed in");
    let lines = said_to_the_end(&said);
    assert!(lines.is_empty(), "no quit was said: {lines:?}");
    let (enrolled, _) = enroll(settled(&m), &m.key("travel"), Some(login)).expect("enrolled");
    assert!(matches!(
        enrolled,
        super::Enrolled::InUse { again: false, .. }
    ));
    assert_eq!(in_use(&m).as_deref(), Some("travel"));
    rested(&m, "enrolled after the wait");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn the_wait_runs_on_its_own_thread_and_finish_joins_it() {
    // Made with no wait of its own, so a wait that never ends fails this test and not its set-up.
    let m = desktop_machine("own-thread");
    // It returns while Claude has never been opened, and asks the process list on its own.
    let waiting = watched(&m, "travel").expect("returns at once");
    let (said, claude) = (waiting.said(), m.claude());
    said_until(&said, "Waiting for Claude to quit");
    let listings = m.mem.listings();
    listed(&m, listings + 3);
    // It holds no lock while it waits but the sign-in's own.
    let change = super::super::try_exclusive(&m.ctx, Permit::for_a_test());
    assert!(change.is_some(), "state.lock is free");
    drop(change);
    assert!(
        crate::host::fs::lock_dir(&data(&m)).is_ok(),
        "the folder is free"
    );
    let second = watched(&m, "another").expect_err("refused");
    assert_eq!(second.code(), "sign_in_in_progress");

    claude.opens();
    said_until(&said, "Claude is open.");
    claude.signs_in(&STRANGER);
    claude.reports();
    claude.quits();
    let login = waiting.finish().expect("signed in");
    assert_eq!(
        said.within(SAYS_WITHIN),
        Err(RecvTimeoutError::Disconnected),
        "the thread has ended, and its lines with it"
    );
    enroll(settled(&m), &m.key("travel"), Some(login)).expect("enrolled");

    // `cancel` joins it too, and lets the sign-in's lock go.
    let waiting = watched(&m, "fourth").expect("`travel` parked, waiting");
    let said = waiting.said();
    waiting.cancel();
    let lines = said_to_the_end(&said);
    assert!(
        lines
            .last()
            .is_some_and(|line| line.starts_with("Waiting for Claude to quit"))
    );

    // `finish` answers what the thread ended with.
    let waiting = watched(&m, "fourth").expect("free to start again");
    let said = waiting.said();
    claude.opens();
    said_until(&said, "Claude is open.");
    m.mem.jar_unsettled(&data(&m).join("Cookies"));
    claude.quits();
    let ended = waiting.finish().expect_err("the look refused");
    assert_eq!(ended.code(), "login_folder_unreadable", "{ended}");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn an_item_or_its_folder_on_another_volume_refuses_before_anything_moves() {
    let m = desktop_pair("another-volume");
    recover(&m).expect("swept");
    let before = snapshot(&m);
    for at in ["Local Storage", "IndexedDB"] {
        let what = format!("holds `{at}` on another volume");
        let path = data(&m).join(at).to_string_lossy().into_owned();
        let stopped = crate::fault::failing_at("another_volume", &path, || refused(&m, "home"));
        assert_eq!(stopped.code(), "login_folder_unknown", "{at}: {stopped}");
        assert!(stopped.to_string().contains(&what), "{stopped}");
        unchanged(&m, &before, at);
    }

    // A link is followed by nothing: an item, and the folder between one and the root.
    let elsewhere = m.ctx_home().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("a folder outside");
    for linked in ["Session Storage", "IndexedDB"] {
        std::fs::rename(data(&m).join(linked), elsewhere.join(linked)).expect("moved out");
        crate::host::fs::testing::link_dir(&elsewhere.join(linked), &data(&m).join(linked))
            .expect("linked");
        let before = snapshot(&m);
        let stopped = refused(&m, "home");
        assert_eq!(
            stopped.code(),
            "login_folder_unknown",
            "{linked}: {stopped}"
        );
        assert!(
            stopped
                .to_string()
                .contains(&format!("holds `{linked}` as a link")),
            "{stopped}"
        );
        unchanged(&m, &before, linked);
        std::fs::remove_file(data(&m).join(linked)).expect("unlinked");
        std::fs::rename(elsewhere.join(linked), data(&m).join(linked)).expect("back");
    }
    assert!(matches!(used(&m, "home").0, Outcome::Switched { .. }));
    rested(&m, "switched once every item is in place");
}

// A refusal and what it is refused over: set up, then asked.
struct Refusal {
    code: &'static str,
    set_up: fn(&Machine),
    asked: fn(&Machine) -> Error,
}

fn tokens_beside_no_session(m: &Machine) {
    killed(m, "leave.jar_out", "home");
    let park = state_of(m)
        .moving(ProviderId::Desktop)
        .expect("recorded")
        .park
        .clone();
    let kept =
        std::fs::read_to_string(crate::store::folder::keys_file(&m.ctx, &park)).expect("keys");
    let tokens: Map<String, Value> = serde_json::from_str(&kept).expect("JSON");
    set(
        m,
        "oauth:tokenCache",
        tokens.get("oauth:tokenCache").cloned(),
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Claude Desktop is macOS only: Windows refuses its folder calls"
)]
fn every_refusal_leaves_the_folder_and_the_index_as_they_were() {
    let refusals: [Refusal; 15] = [
        Refusal {
            code: "different_volume",
            set_up: |_| {},
            asked: |m| {
                let parks = crate::store::folder::parks_dir(&m.ctx);
                let parks = parks.to_string_lossy();
                crate::fault::failing_at("another_volume", &parks, || refused(m, "home"))
            },
        },
        Refusal {
            code: "parks_not_private",
            set_up: |m| {
                let parks = crate::store::folder::parks_dir(&m.ctx);
                crate::host::fs::testing::allow_changes(&parks).expect("opened");
            },
            asked: |m| refused(m, "home"),
        },
        Refusal {
            code: "login_folder_busy",
            set_up: |_| {},
            asked: |m| {
                let _held = crate::host::fs::lock_dir(&data(m)).expect("another home");
                refused(m, "home")
            },
        },
        Refusal {
            code: "app_open",
            set_up: |m| m.claude().opens(),
            asked: |m| refused(m, "home"),
        },
        Refusal {
            code: "app_state_unknown",
            set_up: |m| m.mem.without_a_process_list(),
            asked: |m| refused(m, "home"),
        },
        Refusal {
            code: "login_folder_unknown",
            set_up: |m| std::fs::create_dir(data(m).join("Network")).expect("an entry not read"),
            asked: |m| refused(m, "home"),
        },
        Refusal {
            code: "login_folder_unreadable",
            set_up: |m| m.mem.jar_unsettled(&data(m).join("Cookies")),
            asked: |m| refused(m, "home"),
        },
        Refusal {
            code: "sign_in_unfinished",
            set_up: |m| set(m, "lastKnownAccountUuid", None),
            asked: |m| refused(m, "home"),
        },
        Refusal {
            code: "parked_credential_corrupt",
            set_up: |m| {
                let park = park_of(m, "home").expect("parked");
                let mut manifest = crate::store::folder::manifest(&m.ctx, &park.service)
                    .expect("reads")
                    .expect("there");
                manifest.machine = "f".repeat(64);
                crate::store::folder::write_manifest(
                    &m.ctx,
                    Permit::for_a_test(),
                    &park.service,
                    &manifest,
                )
                .expect("written");
            },
            asked: |m| refused(m, "home"),
        },
        Refusal {
            code: "live_account_not_enrolled",
            set_up: |m| {
                let claude = m.claude();
                claude.opens();
                claude.signs_in(&STRANGER);
                claude.reports();
                claude.quits();
            },
            asked: |m| watched(m, "travel").expect_err("refused"),
        },
        Refusal {
            code: "recovery_undetermined",
            set_up: tokens_beside_no_session,
            asked: |m| confirm_(m, "work").expect_err("refused"),
        },
        Refusal {
            code: "recovery_undetermined",
            set_up: tokens_beside_no_session,
            asked: |m| forget_(m, "home").expect_err("refused"),
        },
        Refusal {
            code: "recovery_elsewhere",
            set_up: |m| {
                killed(m, "leave.jar_out", "home");
                let mut state = state_of(m);
                state.moving.get_mut("desktop").expect("recorded").slot = "/elsewhere".into();
                state::save(&m.ctx, Permit::for_a_test(), &state).expect("saved");
            },
            asked: |m| confirm_(m, "work").expect_err("refused"),
        },
        Refusal {
            code: "move_mixed",
            set_up: |m| {
                killed(m, "leave.item_out", "home");
                let claude = m.claude();
                claude.opens();
                claude.signs_in(&STRANGER);
                claude.reports();
                claude.quits();
            },
            asked: |m| gave_up(m).expect_err("refused"),
        },
        Refusal {
            code: "uninstall_would_strand",
            set_up: |m| killed(m, "leave.done", "home"),
            asked: |m| {
                let settled = settle(&m.ctx, Permit::for_a_test(), None).expect("settled");
                super::super::uninstall(settled.0).expect_err("refused")
            },
        },
    ];
    let parks = |m: &Machine| crate::store::folder::parks_dir(&m.ctx);
    for (n, refusal) in refusals.iter().enumerate() {
        let m = desktop_pair(&format!("refusal-{n}"));
        (refusal.set_up)(&m);
        recover(&m).expect("swept");
        let before = snapshot(&m);
        let jars = m.mem.jars_asked().len();
        let error = (refusal.asked)(&m);
        assert_eq!(error.code(), refusal.code, "{n}: {error}");
        unchanged(&m, &before, refusal.code);
        for jar in &m.mem.jars_asked()[jars..] {
            assert!(
                *jar == data(&m).join("Cookies") || jar.starts_with(parks(&m)),
                "{}: read {}",
                refusal.code,
                jar.display()
            );
        }
    }
}
