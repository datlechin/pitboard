use super::*;

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
        Some(Unreadable::Broken(words::MISSING_FROM_THE_VAULT.into()))
    );
    host.vault().fault_all(Fault::Locked);
    assert_eq!(
        park_facts(&ctx, &state)[0].unreadable,
        Some(Unreadable::Locked(store::Backend::Keychain))
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
    assert_eq!(
        (dormant.detail.as_str(), dormant.advice.as_str()),
        (
            words::not_switched_to_for(31 * 86_400).as_str(),
            words::forget_if_not_coming_back("work").as_str()
        )
    );

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
    unusable.unreadable = Some(Unreadable::Broken(words::MISSING_FROM_THE_VAULT.into()));
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
        let label = name.trim_start_matches("account ");
        assert_eq!(c.account, Some(format!("claude/{label}")), "{name}");
        if level != Level::Ok {
            let label = name.trim_start_matches("account ");
            assert_eq!(c.advice, words::sign_in_again(label));
        }
    }
    // The column `pitboard status` shows, after the check's own words.
    let life = |left: i64| said::parked_life_column(NOW + left, NOW);
    assert_eq!(
        named(&checks, "account fine").detail,
        words::parked_with(&life(20 * 86_400))
    );
    assert_eq!(
        named(&checks, "account soon").detail,
        words::park_life(&life(86_400))
    );
    assert_eq!(
        named(&checks, "account gone").detail,
        words::park_life_since(&life(-1), &time::moment(NOW - 1, NOW))
    );
    assert_eq!(
        named(&checks, "account broken").detail,
        words::park_unusable(words::MISSING_FROM_THE_VAULT)
    );
    assert_eq!(
        named(&checks, "account empty").detail,
        words::NOTHING_PARKED
    );
    assert_eq!(
        named(&checks, "account live").detail,
        words::SIGNED_IN_PARKED_ON_A_SWITCH
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
        (Level::Ok, "parked logins", words::SEALED_TO_THIS_ACCOUNT)
    );

    let lost = Some(Unreadable::Sealed(Some(DPAPI_KEY_RESET)));
    f.parks[0].unreadable = lost.clone();
    let checks = evaluate(&f);
    assert_eq!(check(&checks, "vault").level, Level::Ok, "one of two");
    assert_eq!(
        (
            named(&checks, "account work").level,
            named(&checks, "account work").detail.as_str(),
            named(&checks, "account work").advice.clone()
        ),
        (
            Level::Fail,
            words::PARK_DOES_NOT_OPEN,
            words::sign_in_again("work")
        )
    );

    f.parks[1].unreadable = lost;
    let checks = evaluate(&f);
    let vault = check(&checks, "vault");
    assert_eq!(vault.level, Level::Fail);
    assert_eq!(vault.detail, words::none_opens(2));
    assert_eq!(
        vault.advice,
        words::sealed_by_a_lost_key(&[
            words::enroll_signing_in("work"),
            words::enroll_signing_in("codex/home")
        ])
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

// Each keeps its own finding: nobody is told to sign every account in again.
#[test]
fn missing_sealed_parks_are_not_a_reset_finding() {
    let mut f = facts();
    f.vault = store::Backend::SealedFile;
    f.parks = vec![parked("work", None), codex_parked("home", None)];
    let missing = Some(Unreadable::Broken(words::MISSING_FROM_THE_VAULT.into()));
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

// A logon without the key, as over SSH, is no administrator's reset.
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
    assert_eq!(
        vault.advice,
        words::not_open_in_this_logon(&[
            words::enroll_signing_in("work"),
            words::enroll_signing_in("codex/home")
        ])
    );
}

// `pitboard enroll work --sign-in` about a Codex account would enrol a Claude Code one.
#[test]
fn advice_names_a_codex_account_the_way_it_is_typed() {
    let mut f = facts();
    let mut codex = codex_parked("work", Some(NOW - 1));
    codex.last_used_at = Some(NOW - 400 * 86_400);
    // Both tools have a `work`, so the name gathered for Claude Code's is the qualified one.
    let mut claude = parked("work", Some(NOW - 1));
    claude.name = "claude/work".into();
    claude.last_used_at = Some(NOW - 400 * 86_400);
    f.parks = vec![claude, codex];
    let checks = evaluate(&f);

    assert_eq!(
        named(&checks, "account codex/work").advice,
        words::sign_in_again("codex/work")
    );
    assert_eq!(
        named(&checks, "account claude/work").advice,
        words::sign_in_again("claude/work"),
        "a name a command here takes, which a bare `work` is not"
    );

    let dormant: Vec<&Check> = checks
        .iter()
        .filter(|c| c.code.ends_with("dormant_account"))
        .collect();
    assert_eq!(dormant.len(), 2);
    for typed in ["codex/work", "claude/work"] {
        assert!(dormant.iter().any(|c| c.name == format!("account {typed}")
            && c.advice == words::forget_if_not_coming_back(typed)
            && c.account.as_deref() == Some(typed)));
    }
}

// Asked of Claude Code's config, a signed-in Codex account read as one with nothing parked.
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

// A home's parks on Linux, as `pitboard vault` reads them.
fn on_linux(kept: switch::Keeping, locked: bool) -> Facts {
    let mut f = facts();
    f.os = Os::Linux;
    f.vault = match kept.place {
        Some(crate::host::Vault::SecretService) => store::Backend::SecretService,
        _ => store::Backend::File,
    };
    f.vault_form = Some(FormFact { kept, locked });
    f
}

fn kept_in(place: crate::host::Vault, answers: bool) -> switch::Keeping {
    switch::Keeping {
        place: Some(place),
        shared: place == crate::host::Vault::SecretService,
        folder: Some(PathBuf::from("/home/x/.pitboard/vault")),
        secret_service: Some(answers),
        moving_to: None,
        leaving: None,
    }
}

fn vault_check(f: &Facts) -> (Level, String, String) {
    let checks = evaluate(f);
    let vault = check(&checks, "vault");
    assert_eq!(vault.name, "parked logins");
    (vault.level, vault.detail.clone(), vault.advice.clone())
}

#[test]
fn files_where_a_secret_service_answers_are_warned_of() {
    let f = on_linux(kept_in(crate::host::Vault::Files, true), false);
    assert_eq!(
        vault_check(&f),
        (
            Level::Warn,
            "in 0600 files in /home/x/.pitboard/vault, though a Secret Service answers on this \
             session's bus"
                .into(),
            "Run `pitboard vault move secret-service` to keep them in the login keyring instead."
                .into()
        )
    );
}

// Where the parks are, whether that answers, and what a move under way or one that could not
// delete everything left.
#[test]
fn the_vault_check_on_linux() {
    use crate::host::Vault;
    use crate::state::VaultForm;
    let keyring = || kept_in(Vault::SecretService, true);
    let files = || kept_in(Vault::Files, false);
    let cases = [
        (
            on_linux(keyring(), false),
            Level::Ok,
            words::IN_THE_LOGIN_KEYRING.to_string(),
            "",
        ),
        (
            on_linux(keyring(), true),
            Level::Warn,
            words::IN_A_KEYRING_THAT_DOES_NOT_ANSWER.to_string(),
            words::IN_THE_DESKTOP_SESSION_THEN_DOCTOR,
        ),
        (
            on_linux(kept_in(Vault::SecretService, false), true),
            Level::Warn,
            words::IN_A_KEYRING_THAT_DOES_NOT_ANSWER.to_string(),
            words::IN_THE_DESKTOP_SESSION_THEN_DOCTOR,
        ),
        (
            on_linux(files(), false),
            Level::Ok,
            "in 0600 files in /home/x/.pitboard/vault; no Secret Service answers on this \
             session's bus"
                .to_string(),
            "",
        ),
        (
            on_linux(
                switch::Keeping {
                    moving_to: Some(VaultForm::SecretService),
                    ..files()
                },
                false,
            ),
            Level::Warn,
            "a move into the login keyring is under way".to_string(),
            words::THE_NEXT_RENEWAL_FINISHES_IT,
        ),
        (
            on_linux(
                switch::Keeping {
                    leaving: Some((VaultForm::Files, 2)),
                    ..keyring()
                },
                false,
            ),
            Level::Warn,
            "2 parked login(s) wait to be deleted from 0600 files in /home/x/.pitboard/vault"
                .to_string(),
            words::THE_NEXT_CHANGE_DELETES_THEM,
        ),
    ];
    for (f, level, detail, advice) in cases {
        assert_eq!(vault_check(&f), (level, detail, advice.to_string()));
    }
    assert!(
        evaluate(&facts()).iter().all(|c| c.code != "vault"),
        "macOS's keychain has no such check"
    );
}

// Read where the home is, before any park, and a locked keyring said once.
#[test]
fn doctor_reads_where_the_home_keeps_its_parks() {
    use crate::host::memory::MemoryHost;
    use crate::store::memory::Fault;
    let host = MemoryHost::by_form();
    host.secret_service_answers(false);
    let m = switch::harness::machine_on(Arc::clone(&host), "doctor-vault-files");
    host.secret_service_answers(true);
    let vault = |ctx: &Context| {
        let diagnosis = run(ctx, &time::moment);
        let vault = check(&diagnosis.checks, "vault");
        (vault.level, vault.detail.clone())
    };
    let folder = home::dir(&m.ctx).join("vault");
    assert_eq!(
        vault(&m.ctx),
        (
            Level::Warn,
            words::in_files_though_a_secret_service_answers(&folder.display().to_string())
        )
    );

    let m = switch::harness::machine_on(Arc::clone(&host), "doctor-vault-locked");
    host.vault().fault_all(Fault::Locked);
    assert_eq!(
        vault(&m.ctx),
        (
            Level::Warn,
            words::IN_A_KEYRING_THAT_DOES_NOT_ANSWER.to_string()
        )
    );
}
