use super::*;

/// Read from the vault, a park behind a locked keychain is locked, not broken: nothing
/// is known of it but that.
#[test]
fn a_park_behind_a_locked_keychain_is_read_as_locked() {
    use crate::host::memory::MemoryHost;
    use crate::store::memory::Fault;

    let host = MemoryHost::new();
    let ctx = Context::for_unit_test().with_memory_stores(Arc::clone(&host));
    let state = State {
        accounts: vec![crate::state::Account {
            label: "work".into(),
            id: "work-uuid".into(),
            account_uuid: "work-uuid".into(),
            email: "work@example.com".into(),
            parked: parked("work", Some(NOW + 20 * 86_400)).park,
            last_used_at: None,
            replaced_at: None,
            plan: None,
            detail: crate::state::Detail::Claude {
                organization_uuid: "org".into(),
                oauth_account: json!({}),
            },
        }],
        ..State::default()
    };
    assert_eq!(
        park_facts(&ctx, &state)[0].unreadable,
        Some(Unreadable::Broken("missing from the vault".into()))
    );
    host.vault().fault_all(Fault::Locked);
    assert_eq!(
        park_facts(&ctx, &state)[0].unreadable,
        Some(Unreadable::Locked)
    );
}

#[test]
fn an_account_nobody_has_come_back_to_is_said_out_loud() {
    let mut f = facts();
    f.parks = vec![ParkFact {
        provider: ProviderId::Claude,
        name: Key::new(ProviderId::Claude, "work").typed(),
        active: false,
        last_used_at: Some(f.now - 31 * 86_400),
        park: Some(Park {
            service: "pitboard-park-acc-1".into(),
            parked_at: f.now - 31 * 86_400,
            kind: crate::state::ParkKind::Vault,
            login: "f".into(),
            access_expires_at: Some(f.now + 3600),
            refresh_expires_at: Some(f.now + 30 * 86_400),
            session_expires_at: None,
        }),
        unreadable: None,
    }];
    let checks = evaluate(&f);
    let dormant = check(&checks, "dormant_account");
    assert_eq!(dormant.level, Level::Warn);
    assert!(dormant.advice.contains("pitboard forget work"));

    // A month is the token's own life. Inside it, there is nothing to say.
    f.parks[0].last_used_at = Some(f.now - 29 * 86_400);
    assert!(
        !evaluate(&f).iter().any(|c| c.code == "dormant_account"),
        "an account used within a token's lifetime is not dormant"
    );

    // Neither is one that is signed in, nor one holding nothing.
    f.parks[0].last_used_at = Some(f.now - 400 * 86_400);
    f.parks[0].active = true;
    assert!(!evaluate(&f).iter().any(|c| c.code == "dormant_account"));
    f.parks[0].active = false;
    f.parks[0].park = None;
    assert!(!evaluate(&f).iter().any(|c| c.code == "dormant_account"));
}

#[test]
fn every_parked_login_is_judged_and_a_way_back_is_offered() {
    let mut f = facts();
    let mut unusable = parked("broken", Some(NOW + 30 * 86_400));
    unusable.unreadable = Some(Unreadable::Broken("missing from the vault".into()));
    f.parks = vec![
        parked("fine", Some(NOW + 20 * 86_400)),
        parked("soon", Some(NOW + 86_400)),
        parked("gone", Some(NOW - 1)),
        unusable,
        ParkFact {
            provider: ProviderId::Claude,
            last_used_at: None,
            name: Key::new(ProviderId::Claude, "empty").typed(),
            active: false,
            park: None,
            unreadable: None,
        },
        ParkFact {
            provider: ProviderId::Claude,
            last_used_at: None,
            name: Key::new(ProviderId::Claude, "live").typed(),
            active: true,
            park: None,
            unreadable: None,
        },
    ];
    let checks = evaluate(&f);
    for (name, level) in [
        ("account fine", Level::Ok),
        ("account soon", Level::Warn),
        ("account gone", Level::Warn),
        ("account broken", Level::Fail),
        ("account empty", Level::Warn),
        ("account live", Level::Ok),
    ] {
        let c = named(&checks, name);
        assert_eq!(c.level, level, "{name}: {}", c.detail);
        if level != Level::Ok {
            let label = name.trim_start_matches("account ");
            assert!(
                c.advice
                    .contains(&format!("pitboard enroll {label} --sign-in"))
            );
        }
    }
    // A parked login's life is said in the column `pitboard status` shows, after the
    // check's own words, and an expired one with when it expired.
    assert_eq!(
        named(&checks, "account fine").detail,
        "parked, good for 20d 0h"
    );
    assert_eq!(
        named(&checks, "account soon").detail,
        "its parked login expires in 1d 0h"
    );
    assert_eq!(
        named(&checks, "account gone").detail,
        format!("its parked login expired {}", time::moment(NOW - 1, NOW))
    );
}

#[test]
fn parks_sealed_to_the_account_are_said_so_and_all_lost_at_once_is_one_finding() {
    let mut f = facts();
    f.parks = vec![
        parked("work", None),
        codex_parked("home", None),
        ParkFact {
            park: None,
            ..parked("empty", None)
        },
    ];
    assert!(evaluate(&f).iter().all(|c| c.code != "vault"), "a keychain");

    f.vault = store::Backend::SealedFile;
    let checks = evaluate(&f);
    let vault = check(&checks, "vault");
    assert_eq!(
        (vault.level, vault.name.as_str(), vault.detail.as_str()),
        (
            Level::Ok,
            "parked logins",
            "sealed to this Windows account, not plain text"
        )
    );

    let lost = Some(Unreadable::Sealed(Some(DPAPI_KEY_RESET)));
    f.parks[0].unreadable = lost.clone();
    let checks = evaluate(&f);
    assert_eq!(check(&checks, "vault").level, Level::Ok, "one of two");
    assert_eq!(named(&checks, "account work").level, Level::Fail);
    assert!(
        named(&checks, "account work")
            .advice
            .contains("pitboard enroll work --sign-in")
    );

    f.parks[1].unreadable = lost;
    let checks = evaluate(&f);
    let vault = check(&checks, "vault");
    assert_eq!(vault.level, Level::Fail);
    assert_eq!(
        vault.detail,
        "none of the 2 parked logins opens for this Windows account"
    );
    assert!(
        vault.advice.ends_with(
            "Sign in to each again: `pitboard enroll work --sign-in` and \
             `pitboard enroll codex/home --sign-in`."
        ),
        "{}",
        vault.advice
    );
    for code in ["parked_login", "codex_parked_login"] {
        assert!(
            checks
                .iter()
                .all(|c| c.code != code || c.level != Level::Fail),
            "{code} is in the one finding"
        );
    }
    assert_eq!(
        named(&checks, "account empty").level,
        Level::Warn,
        "nothing parked"
    );
}

// Parks missing from the vault are not a lost key, so doctor does not tell the person to
// sign every account in again; each keeps its own per-park finding.
#[test]
fn missing_sealed_parks_are_not_a_reset_finding() {
    let mut f = facts();
    f.vault = store::Backend::SealedFile;
    f.parks = vec![parked("work", None), codex_parked("home", None)];
    let missing = Some(Unreadable::Broken("missing from the vault".into()));
    f.parks[0].unreadable = missing.clone();
    f.parks[1].unreadable = missing;
    let checks = evaluate(&f);
    assert_eq!(
        check(&checks, "vault").level,
        Level::Ok,
        "missing is not a lost key"
    );
    assert_eq!(named(&checks, "account work").level, Level::Fail);
    assert_eq!(named(&checks, "account codex/home").level, Level::Fail);
}

// A sealed park that will not open in this logon (a network logon over SSH, not the reset
// code) gets neutral wording, not "an administrator reset your password".
#[test]
fn sealed_parks_lost_to_this_logon_get_neutral_wording() {
    let mut f = facts();
    f.vault = store::Backend::SealedFile;
    f.parks = vec![parked("work", None), codex_parked("home", None)];
    let sealed = Some(Unreadable::Sealed(Some(5)));
    f.parks[0].unreadable = sealed.clone();
    f.parks[1].unreadable = sealed;
    let checks = evaluate(&f);
    let vault = check(&checks, "vault");
    assert_eq!(vault.level, Level::Fail);
    assert!(
        vault.advice.contains("from a desktop session"),
        "{}",
        vault.advice
    );
    assert!(!vault.advice.contains("administrator"), "{}", vault.advice);
}

/// A parked login is due to be renewed from three days before its refresh token expires
/// until it does. Past that it cannot be renewed, so it is not due: its account needs a
/// sign-in instead.

/// The advice has to be something a person can type and have it act on the right
/// account. `pitboard enroll work --sign-in` about a Codex account would enroll a
/// Claude Code one.
#[test]
fn advice_names_a_codex_account_the_way_it_is_typed() {
    let mut f = facts();
    let mut codex = codex_parked("work", Some(NOW - 1));
    codex.last_used_at = Some(NOW - 400 * 86_400);
    // Both tools have a `work`, so a bare `work` would be refused as ambiguous, and the
    // name gathered for Claude Code's is the qualified one.
    let mut claude = parked("work", Some(NOW - 1));
    claude.name = "claude/work".into();
    claude.last_used_at = Some(NOW - 400 * 86_400);
    f.parks = vec![claude, codex];
    let checks = evaluate(&f);

    let codex_park = named(&checks, "account codex/work");
    assert!(
        codex_park
            .advice
            .contains("`pitboard enroll codex/work --sign-in`"),
        "{}",
        codex_park.advice
    );
    let claude_park = named(&checks, "account claude/work");
    assert!(
        claude_park
            .advice
            .contains("`pitboard enroll claude/work --sign-in`"),
        "a name a command here takes, which a bare `work` is not: {}",
        claude_park.advice
    );

    let dormant: Vec<&Check> = checks
        .iter()
        .filter(|c| c.code.ends_with("dormant_account"))
        .collect();
    assert_eq!(dormant.len(), 2);
    assert!(dormant.iter().any(
        |c| c.name == "account codex/work" && c.advice.contains("`pitboard forget codex/work`")
    ));
    assert!(
        dormant.iter().any(|c| c.name == "account claude/work"
            && c.advice.contains("`pitboard forget claude/work`"))
    );
}

/// Whether an account is the one signed in is its own tool's question: Claude Code's from
/// what Anthropic last said of its login, which the config naming another does not move,
/// and Codex's from its login's own claims. Asked of Claude Code's config for every
/// account, a signed-in Codex account read as one with nothing parked to switch to, and
/// the advice was to sign in again.
#[test]
fn an_account_is_active_by_its_own_tools_record() {
    use crate::host::memory::MemoryHost;

    let scratch = tempfile::tempdir().expect("a scratch home");
    let root = scratch.path().to_path_buf();
    let ctx = Context::new(root.clone())
        .with_pitboard_home(root.join(".pitboard"))
        .with_codex_home(root.join("codex").to_string_lossy().into())
        .with_memory_stores(MemoryHost::new());
    // Codex does not start on Windows where `CODEX_HOME` names no folder.
    std::fs::create_dir_all(root.join("codex")).expect("a Codex home");
    std::fs::write(
        root.join(".claude.json"),
        json!({"oauthAccount": {
            "accountUuid": "beta-uuid",
            "emailAddress": "b@example.com",
            "organizationUuid": "org",
        }})
        .to_string(),
    )
    .expect("a Claude Code config");

    let account = |label: &str, uuid: &str, detail: crate::state::Detail| crate::state::Account {
        label: label.into(),
        id: uuid.into(),
        account_uuid: uuid.into(),
        email: format!("{label}@example.com"),
        parked: None,
        last_used_at: None,
        replaced_at: None,
        plan: None,
        detail,
    };
    let claude = || crate::state::Detail::Claude {
        organization_uuid: "org".into(),
        oauth_account: json!({}),
    };
    let codex = || crate::state::Detail::Codex { workspace_id: None };
    let mut state = State {
        accounts: vec![
            account("alpha", "alpha-uuid", claude()),
            account("beta", "beta-uuid", claude()),
            account("work", "work-acc", codex()),
            account("home", "home-acc", codex()),
        ],
        ..State::default()
    };
    let alpha = crate::in_use::InUse::of(&state.accounts[0], "alpha-refresh", NOW);
    state.identified(ProviderId::Claude, alpha, NOW);
    let home = crate::in_use::InUse::of(&state.accounts[3], "home-refresh", NOW);
    state.identified(ProviderId::Codex, home, NOW);
    let active = |facts: &[ParkFact]| -> Vec<String> {
        facts
            .iter()
            .filter(|p| p.active)
            .map(ParkFact::typed)
            .collect()
    };

    // Nothing signed in to Codex: nobody, whatever Pitboard last recorded.
    assert_eq!(active(&park_facts(&ctx, &state)), ["alpha"]);

    // A Codex login that cannot be read: Pitboard's record of whose it was stands in.
    let live = crate::provider::document(ProviderId::Codex)
        .live(&ctx)
        .expect("Codex keeps its login in a file here");
    store::write_raw(
        &live.chain,
        Permit::for_a_test(),
        &live.service,
        "{\"auth_mo",
    )
    .expect("half a login");
    assert_eq!(active(&park_facts(&ctx, &state)), ["alpha", "codex/home"]);

    // Codex's login names `work`, whatever Pitboard last recorded.
    let login = json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": crate::provider::jwt::unsigned(&json!({
                "email": "work@example.com",
                "https://api.openai.com/auth": {"chatgpt_account_id": "work-acc"},
            })),
            "access_token": "a",
            "refresh_token": "r",
            "account_id": "work-acc",
        },
    });
    store::write_raw(
        &live.chain,
        Permit::for_a_test(),
        &live.service,
        &login.to_string(),
    )
    .expect("a login");
    assert_eq!(active(&park_facts(&ctx, &state)), ["alpha", "codex/work"]);
}
