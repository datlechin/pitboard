use super::{
    code, confirm_, enrol, forget_, in_use, jar, names, park_by_hand, park_name, set, two_accounts,
    use_,
};
use super::{data, snapshot, tree};
use crate::desktop_testing::{JarAs, plant};
use crate::provider::ProviderId;
use crate::state::{self, SetAside};
use crate::switch::harness::{HOME, NOW, STRANGER, WORK, desktop_machine, hold, hold_where_twice};
use serde_json::json;

#[test]
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
        (crate::switch::folder::Enrolled::Current { .. }, _)
    ));
    assert_eq!(snapshot(&m).2, before.2, "nothing of Claude's moved");
    assert!(crate::store::folder::list(&m.ctx).expect("parks").is_none());
    hold(&m, "enrolled");
}

#[test]
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
