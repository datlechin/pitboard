use super::*;

#[test]
fn asking_names_each_service_it_asks_once() {
    let account = |label: &str, detail| crate::state::Account {
        label: label.into(),
        id: label.into(),
        account_uuid: label.into(),
        email: String::new(),
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
    let desktop = || crate::state::Detail::Desktop {};
    let named = |accounts: Vec<crate::state::Account>| {
        let mut f = facts();
        f.state = Ok(State {
            accounts,
            ..State::default()
        });
        judge_asking(&f).name
    };
    assert_eq!(named(vec![account("work", desktop())]), "asking Anthropic");
    assert_eq!(
        named(vec![account("alpha", claude()), account("work", desktop())]),
        "asking Anthropic"
    );
    assert_eq!(
        named(vec![
            account("alpha", claude()),
            account("main", codex()),
            account("work", desktop()),
        ]),
        "asking Anthropic and OpenAI"
    );
}

#[test]
fn a_login_others_on_a_windows_pc_can_read_is_made_private_one_path_at_a_time() {
    let mut f = facts();
    f.os = Os::Windows;
    f.sid = Some(SID.into());
    let (claude, vault, park) = (
        r"C:\Users\dana\.claude\.credentials.json",
        r"C:\Users\dana\AppData\Local\Pitboard\vault",
        r"C:\Users\dana\AppData\Local\Pitboard\vault\pitboard-park-x.json",
    );
    f.readable_by_others = vec![
        (claude.into(), Kind::File, open_to_users()),
        (vault.into(), Kind::Directory, open_to_users()),
        (park.into(), Kind::File, open_to_users()),
    ];
    let checks = evaluate(&f);
    let found = check(&checks, "private_on_disk");
    assert_eq!(found.level, Level::Fail);
    assert_eq!(
        found.detail,
        [claude, vault, park]
            .map(|path| words::open_to(path, &open_to_users()))
            .join("; ")
    );
    // A folder's inheritance on a file leaves it open to nobody, its owner too.
    let [
        [claude, claude_grant],
        [vault, vault_grant],
        [park, park_grant],
    ] = [(claude, false), (vault, true), (park, false)]
        .map(|(path, folder)| crate::host::acl::icacls(folder, path, SID));
    assert_eq!(
        found.advice,
        words::others_can_read(
            Os::Windows,
            &[claude, claude_grant, vault, vault_grant, park, park_grant]
        )
    );
    f.sid = None;
    assert_eq!(
        check(&evaluate(&f), "private_on_disk").advice,
        words::others_can_read(Os::Windows, &[]),
        "no command without the SID"
    );
}

#[test]
fn a_pitboard_folder_others_on_a_windows_pc_can_read_says_so() {
    let mut f = facts();
    f.os = Os::Windows;
    f.sid = Some(SID.into());
    f.home = PathBuf::from(r"C:\Users\dana\AppData\Local\Pitboard");
    f.home_access = Some(open_to_users());
    let checks = evaluate(&f);
    let home = check(&checks, "home");
    assert_eq!(home.level, Level::Warn);
    assert_eq!(
        home.detail,
        words::open_to(f.home.display(), &open_to_users())
    );
    let [reset, grant] = crate::host::acl::icacls(true, &f.home.display().to_string(), SID);
    assert_eq!(
        home.advice,
        words::only_you_should_read_the_home(&[reset, grant])
    );
}

// Read off a real disk: a gatherer that found nothing would pass every facts test.
#[test]
fn a_world_readable_park_is_found_on_the_disk() {
    use crate::host::fs::testing;

    let scratch = tempfile::tempdir().expect("a scratch home");
    let root = scratch.path().to_path_buf();

    let ctx = Context::new(root.clone())
        .with_pitboard_home(root.join(".pitboard"))
        .with_memory_stores(crate::host::memory::MemoryHost::new());
    let vault = store::vault_dir(&ctx);
    crate::host::fs::create_private_dir(Permit::for_a_test(), &vault).expect("a vault");
    assert!(
        loose_logins(&ctx).is_empty(),
        "a private vault is not loose"
    );

    let park = vault.join("pitboard-park-x.json");
    std::fs::write(&park, "{}").expect("a park");
    testing::open_to_others(&park).expect("opened to others");
    let found = loose_logins(&ctx);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(Some(&found[0].2), crate::host::fs::access(&park).as_ref());
    assert!(found[0].0.ends_with("pitboard-park-x.json"), "{found:?}");
    assert_eq!(found[0].1, Kind::File);

    testing::make_private(&park).expect("made private");
    assert!(loose_logins(&ctx).is_empty(), "a private park is not loose");

    testing::open_to_others(&vault).expect("opened to others");
    let found = loose_logins(&ctx);
    let folder = found
        .iter()
        .find(|(path, ..)| *path == vault.display().to_string())
        .expect("the vault");
    assert_eq!(folder.1, Kind::Directory, "{found:?}");
}

// A seal stops others reading a park, not writing it.
#[test]
fn a_sealed_park_others_can_change_is_reported() {
    let scratch = tempfile::tempdir().expect("a scratch home");
    let root = scratch.path().to_path_buf();
    let ctx = Context::new(root.clone())
        .with_pitboard_home(root.join(".pitboard"))
        .with_memory_stores(crate::host::memory::MemoryHost::sealed());
    let vault = store::vault_dir(&ctx);
    crate::host::fs::create_private_dir(Permit::for_a_test(), &vault).expect("a vault");
    let park = vault.join("pitboard-park-x.json");
    std::fs::write(&park, "sealed").expect("a park");
    crate::host::fs::testing::open_to_others(&park).expect("opened to others");
    let found = loose_logins(&ctx);
    assert!(
        found
            .iter()
            .any(|(path, ..)| *path == park.display().to_string()),
        "a sealed park others can change: {found:?}"
    );

    crate::host::fs::testing::make_private(&park).expect("made private");
    crate::host::fs::testing::open_to_others(&vault).expect("opened to others");
    let found = loose_logins(&ctx);
    assert!(
        found
            .iter()
            .any(|(path, kind, _)| *path == vault.display().to_string()
                && *kind == Kind::Directory),
        "the opened vault: {found:?}"
    );
}

// A restore, a `cp -r` or a careless umask changes a mode quietly.
#[test]
fn a_login_anyone_on_this_machine_can_read_is_a_failure() {
    let mut f = facts();
    f.os = Os::Linux;
    assert_eq!(check(&evaluate(&f), "private_on_disk").level, Level::Ok);

    f.readable_by_others = vec![
        (
            "/home/a/.pitboard/vault/pitboard-park-x.json".into(),
            Kind::File,
            mode(0o644),
        ),
        (
            "/home/a/.claude/.credentials.json".into(),
            Kind::File,
            mode(0o640),
        ),
    ];
    let checks = evaluate(&f);
    let found = check(&checks, "private_on_disk");
    assert_eq!(found.level, Level::Fail);
    assert!(found.detail.contains("mode 644"), "{}", found.detail);
    assert!(found.detail.contains("mode 640"), "{}", found.detail);
    // The advice has to be something a person can run, not a description of a problem.
    assert!(found.advice.contains("chmod go-rwx"), "{}", found.advice);
    assert!(!healthy(&checks));
}

#[test]
fn a_loose_home_directory_is_flagged() {
    let mut f = facts();
    f.home_access = Some(mode(0o755));
    let checks = evaluate(&f);
    let home = check(&checks, "home");
    assert_eq!(home.level, Level::Warn);
    assert!(home.detail.contains("755"));
}

#[test]
fn a_machine_without_a_stable_identifier_is_flagged() {
    let mut f = facts();
    f.machine_id_known = false;
    f.home = PathBuf::from("/elsewhere/pitboard");
    let checks = evaluate(&f);
    assert_eq!(check(&checks, "machine_id").level, Level::Warn);
    assert_eq!(
        check(&checks, "machine_id").advice,
        words::never_copy_between_machines(f.home.display()),
        "the folder in use"
    );
    for os in [Os::MacOs, Os::Linux] {
        f.os = os;
        f.machine_id_known = true;
        assert!(
            evaluate(&f).iter().all(|c| c.code != "machine_id"),
            "{os:?}"
        );
    }
}

// The sysprep page names the SID it replaces, and not the MachineGuid machine-uid reads.
#[test]
fn a_windows_pc_is_told_a_copy_of_it_keeps_its_machine_guid() {
    let mut f = facts();
    f.os = Os::Windows;
    let checks = evaluate(&f);
    let note = check(&checks, "machine_id");
    assert_eq!(
        (note.level, note.detail.as_str(), note.advice.as_str()),
        (
            Level::Ok,
            words::MACHINE_GUID,
            words::A_COPY_KEEPS_THE_MACHINE_GUID
        )
    );
    f.machine_id_known = false;
    assert_eq!(check(&evaluate(&f), "machine_id").level, Level::Warn);
}

#[test]
fn an_interrupted_switch_and_leftover_parks_are_reported() {
    let mut f = facts();
    f.interrupted = true;
    f.state = Ok(State {
        discarded: vec![crate::state::Dropped {
            service: "pitboard-park-x-1".into(),
            kind: crate::state::ParkKind::Vault,
            forgotten: false,
        }],
        ..State::default()
    });
    let checks = evaluate(&f);
    assert_eq!(check(&checks, "interrupted_switch").level, Level::Warn);
    assert_eq!(check(&checks, "discarded").level, Level::Warn);
    assert!(healthy(&checks), "neither stops Pitboard working");
}

// A keychain can be locked; a folder of files Pitboard reads as itself cannot.
#[test]
fn parks_left_to_delete_or_resolve_are_waited_on_as_their_vault_keeps_them() {
    let mut f = facts();
    f.state = Ok(State {
        discarded: vec![crate::state::Dropped {
            service: "pitboard-park-x-1".into(),
            kind: crate::state::ParkKind::Vault,
            forgotten: false,
        }],
        ..State::default()
    });
    f.pending_parks = vec!["pitboard-park-y-1".into()];
    for (vault, discarded, pending) in [
        (
            store::Backend::Keychain,
            words::DELETED_ONCE_THE_KEYCHAIN_OPENS,
            words::UNLOCK_AND_RUN_ANY_COMMAND,
        ),
        (
            store::Backend::File,
            words::DELETED_ON_THE_NEXT_CHANGE,
            words::RUN_ANY_COMMAND,
        ),
        (
            store::Backend::SealedFile,
            words::DELETED_ON_THE_NEXT_CHANGE,
            words::RUN_ANY_COMMAND,
        ),
    ] {
        f.vault = vault;
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "discarded").advice, discarded, "{vault:?}");
        assert_eq!(check(&checks, "pending_parks").advice, pending, "{vault:?}");
    }
}

#[test]
fn a_switch_the_next_change_cannot_finish_says_why_and_the_way_out() {
    let mut f = facts();
    f.interrupted = true;
    f.stuck = Some("an earlier switch from `work` to `personal` was interrupted".into());
    let checks = evaluate(&f);
    let said = check(&checks, "interrupted_switch");
    assert_eq!(said.level, Level::Warn);
    assert_eq!(Some(&said.detail), f.stuck.as_ref());
    assert_eq!(said.advice, words::ABANDON_KEEPS_EVERY_LOGIN);
    assert_eq!(
        checks
            .iter()
            .filter(|c| c.code == "interrupted_switch")
            .count(),
        1,
        "said once"
    );
}

// RF-A1 read UAC on on both images, so only Microsoft's word says what off does.
#[test]
fn windows_says_user_account_control_is_off_where_it_reads_so() {
    let mut f = facts();
    f.os = Os::Windows;
    let advice = |f: &Facts| {
        evaluate(f)
            .into_iter()
            .find(|c| c.code == "elevated")
            .map(|c| c.advice)
    };
    f.elevation = crate::host::Elevation::Elevated {
        why: crate::host::token::IN_EVERY_PROGRAM,
    };
    let way_out = |why| said::elevated(Os::Windows, Some(why)).way_out;
    let before = words::changes_nothing_this_way(way_out(crate::host::token::IN_EVERY_PROGRAM));
    for read in [None, Some(true)] {
        f.windows_settings = Some(crate::host::WindowsSettings {
            admin_approval: read,
            ..crate::host::WindowsSettings::default()
        });
        assert_eq!(advice(&f), Some(before.clone()), "{read:?}");
    }
    f.windows_settings = Some(crate::host::WindowsSettings {
        admin_approval: Some(false),
        ..crate::host::WindowsSettings::default()
    });
    assert_eq!(
        advice(&f),
        Some(words::changes_nothing_this_way(said::ADMIN_APPROVAL_OFF))
    );
    f.elevation = crate::host::Elevation::Elevated {
        why: crate::host::token::AS_ADMINISTRATOR,
    };
    assert_eq!(
        advice(&f),
        Some(words::changes_nothing_this_way(way_out(
            crate::host::token::AS_ADMINISTRATOR
        )))
    );
    f.elevation = crate::host::Elevation::Normal;
    assert_eq!(advice(&f), None);
}

#[test]
fn windows_is_told_how_it_runs_and_which_build_it_needs() {
    let mut facts = facts();
    facts.os = Os::Windows;
    let refusing = |facts: &Facts| -> Vec<(&'static str, String, String)> {
        evaluate(facts)
            .into_iter()
            .filter(|c| ["elevated", "system_too_old"].contains(&c.code))
            .map(|c| (c.code, c.detail, c.advice))
            .collect()
    };
    assert!(refusing(&facts).is_empty());

    facts.elevation = crate::host::Elevation::Elevated {
        why: crate::host::token::AS_ADMINISTRATOR,
    };
    let elevated = |why| {
        let told = said::elevated(Os::Windows, Some(why));
        (
            "elevated",
            told.because,
            words::changes_nothing_this_way(told.way_out),
        )
    };
    assert_eq!(
        refusing(&facts),
        [elevated(crate::host::token::AS_ADMINISTRATOR)]
    );

    facts.elevation = crate::host::Elevation::Elevated {
        why: crate::host::token::IN_EVERY_PROGRAM,
    };
    facts.floor = crate::host::Floor::Below { build: 22631 };
    assert_eq!(
        refusing(&facts),
        [
            (
                "system_too_old",
                said::too_old(Some(22631)).because,
                words::changes_nothing_here(said::too_old(Some(22631)).way_out)
            ),
            elevated(crate::host::token::IN_EVERY_PROGRAM),
        ]
    );
}
