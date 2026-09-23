//! A switch of a tool that is not Claude Code, through the same engine.
//!
//! Codex disagrees with Claude Code on nearly every fact the boundary carries: its login
//! names its own account, nothing follows a switch until it is restarted, a park may never
//! be a copy, it takes no write lock and caches no identity. These are the cases that
//! prove the switch asks the tool rather than assuming Claude Code's answers.

use super::harness::{NOW, codex_access, codex_account, codex_login, codex_machine, hold};
use super::*;
use crate::api::scripted::Trouble;
use crate::provider::codex::api::Fresh;
use crate::provider::{Adoption, Credential};

fn whose(m: &super::harness::Machine) -> Option<String> {
    let live = m.live()?;
    provider::of(m.which)
        .identify(&m.ctx, &Credential::new(m.which, live))
        .ok()
        .map(|found| found.account_id)
}

/// The login moves: `there`'s goes live, `here`'s goes into the vault, and nothing of
/// either is left anywhere else. A copy of `there` left parked beside its live twin is a
/// token `codex logout` would revoke in both places at once.
#[test]
fn a_codex_switch_moves_one_login_in_and_one_out() {
    let m = codex_machine("moves");
    let settled = settle(&m.ctx).expect("nothing to recover").0;

    let (outcome, _) = switch(settled, &m.key("there")).expect("switched");

    assert_eq!(whose(&m).as_deref(), Some("there"));
    let Outcome::Switched { adoption, .. } = outcome else {
        panic!("expected a switch, got {outcome:?}");
    };
    assert_eq!(
        adoption,
        Adoption::RestartRequired { program: "codex" },
        "a running codex never notices, so it must not be told it will"
    );
    let state = state::load(&m.ctx).expect("state");
    assert_eq!(state.active_for(ProviderId::Codex), Some("there"));
    assert!(state.get(&m.key("there")).unwrap().parked.is_none());
    let parked = state
        .get(&m.key("here"))
        .unwrap()
        .parked
        .clone()
        .expect("here parked");
    assert_eq!(
        m.mem.vault().services(),
        vec![parked.service],
        "one copy in the vault, and it is the outgoing login"
    );
    hold(&m, "after a Codex switch");
}

/// Claude Code's login sits in a keychain item that Codex's chain never reads. A Codex
/// switch that reached it would sign somebody out of the other tool.
#[test]
fn a_codex_switch_leaves_claude_code_alone() {
    let m = codex_machine("claude-untouched");
    let claude = crate::provider::claude::paths::live_service(&m.ctx);
    m.mem
        .live()
        .plant(&claude, "{\"claudeAiOauth\":{\"refreshToken\":\"c\"}}");

    let settled = settle(&m.ctx).expect("nothing to recover").0;
    switch(settled, &m.key("there")).expect("switched");

    assert_eq!(
        m.mem.live().peek(&claude).as_deref(),
        Some("{\"claudeAiOauth\":{\"refreshToken\":\"c\"}}")
    );
}

/// Both tools have a `there`. The Codex one is switched to, and the Claude Code one is
/// neither parked, nor made active, nor looked up in its place.
#[test]
fn the_same_label_on_two_tools_is_two_accounts() {
    let m = codex_machine("same-label");
    let mut state = state::load(&m.ctx).expect("state");
    state.upsert(super::harness::account("there", "claude-there", None));
    state::save(&m.ctx, &state).expect("saved");

    let settled = settle(&m.ctx).expect("nothing to recover").0;
    switch(settled, &m.key("there")).expect("switched");

    let state = state::load(&m.ctx).expect("state");
    let claude = Key::new(ProviderId::Claude, "there");
    assert_eq!(state.get(&claude).unwrap().account_uuid, "claude-there");
    assert!(state.get(&claude).unwrap().parked.is_none());
    assert_eq!(state.active_for(ProviderId::Claude), None);
    assert_eq!(state.active_for(ProviderId::Codex), Some("there"));
}

/// Codex's login names its account whether or not OpenAI still accepts it, so the park
/// going in is asked about first. Refused as lapsed, it is renewed and the fresh tokens
/// are what goes live.
#[test]
fn a_park_openai_refuses_is_renewed_before_it_goes_live() {
    let m = codex_machine("renewed-first");
    m.api
        .token_trouble(&codex_access("there-refresh"), Trouble::Unauthorized);
    let fresh_access = codex_access("there-renewed");
    m.api.codex_renews(
        "there-refresh",
        Fresh {
            id_token: None,
            access_token: Some(fresh_access.clone()),
            refresh_token: Some("there-renewed".into()),
            at: Some(NOW),
        },
    );

    let settled = settle(&m.ctx).expect("nothing to recover").0;
    switch(settled, &m.key("there")).expect("switched");

    let live = m.live().expect("a live login");
    assert_eq!(live["tokens"]["refresh_token"], "there-renewed");
    assert_eq!(live["tokens"]["access_token"], fresh_access.as_str());
    assert_eq!(whose(&m).as_deref(), Some("there"));
    hold(&m, "after a renewed Codex switch");
}

/// A park OpenAI will not renew is dropped and nothing moves: `here` is still signed in
/// and still the only copy of itself.
#[test]
fn a_park_openai_will_not_renew_moves_nothing() {
    let m = codex_machine("refused");
    m.api
        .token_trouble(&codex_access("there-refresh"), Trouble::Unauthorized);
    m.api
        .codex_renew_trouble("there-refresh", Trouble::InvalidGrant);
    let before = m.live();

    let settled = settle(&m.ctx).expect("nothing to recover").0;
    let refused = switch(settled, &m.key("there")).expect_err("refused");

    assert_eq!(refused.code(), "parked_login_refused");
    assert_eq!(m.live(), before);
    let here = store::fingerprint("here-refresh");
    for service in m.mem.vault().services() {
        let parked: Value =
            serde_json::from_str(&m.mem.vault().peek(&service).expect("listed")).expect("json");
        assert_ne!(
            provider::of(ProviderId::Codex).fingerprint(&parked),
            here,
            "nothing of `here` was parked"
        );
    }
    let state = state::load(&m.ctx).expect("state");
    assert!(
        state.get(&m.key("there")).unwrap().parked.is_none(),
        "the refused park is dropped rather than offered again"
    );
    hold(&m, "after a refused Codex switch");
}

/// A park that could not be kept is found before the live login is replaced. For a tool
/// whose park may never be a copy, the outgoing login stays where it was: it is the only
/// one there is.
#[test]
fn a_park_that_does_not_stick_leaves_the_live_login_in_place() {
    let m = codex_machine("park-lost");
    m.mem
        .vault()
        .fault_all(crate::store::memory::Fault::DeletedAfterWrite);
    let before = m.live();

    let settled = settle(&m.ctx).expect("nothing to recover").0;
    assert!(switch(settled, &m.key("there")).is_err());

    assert_eq!(m.live(), before, "`here` must still be signed in");
    m.mem.vault().heal_all();
    hold(&m, "after a park that did not stick");
}

/// A Codex account enrolled alongside keeps its own detail: the plan its login names.
#[test]
fn a_codex_account_is_a_codex_account() {
    let account = codex_account("work", "acc", None);
    assert_eq!(account.provider(), ProviderId::Codex);
    assert!(account.claude().is_none());
    let login = codex_login("acc", "r");
    assert_eq!(
        provider::of(ProviderId::Codex).fingerprint(&login),
        store::fingerprint("r")
    );
}
