use super::*;

/// A document whose `claudeAiOauth` is there and is not an object is a shape that
/// moved, which nothing should be written into until it is understood.
#[test]
fn a_credential_whose_login_is_not_an_object_is_a_failure() {
    let mut f = facts();
    f.credential = Ok(Some(json!({"claudeAiOauth": "something else"})));
    let checks = evaluate(&f);
    assert_eq!(check(&checks, "credential").level, Level::Fail);
    assert!(!healthy(&checks));
}

#[test]
fn an_unreadable_store_fails_rather_than_reporting_nothing_stored() {
    let mut f = facts();
    f.credential = Err(store::Error::Unreadable("security exited 1".into()));
    f.backend = Err(store::Error::Unreadable("security exited 1".into()));
    let checks = evaluate(&f);
    assert_eq!(check(&checks, "credential").level, Level::Fail);
    assert_eq!(check(&checks, "credential_store").level, Level::Fail);
}

/// A keychain locked where it cannot ask to be unlocked, as over SSH, is one thing to
/// fix, said once with what fixes it and what Claude Code does meanwhile. Every parked
/// login behind it used to fail as well, each advised to sign in again for nothing.
#[test]
fn a_locked_keychain_is_one_failure_and_sends_nobody_to_sign_in_again() {
    let mut f = facts();
    f.os = Os::MacOs;
    f.credential = Err(store::Error::Locked);
    let mut behind = parked("work", Some(NOW + 20 * 86_400));
    behind.unreadable = Some(Unreadable::Locked);
    f.parks = vec![behind];

    let checks = evaluate(&f);
    let credential = check(&checks, "credential");
    assert_eq!(credential.level, Level::Fail);
    assert_eq!(
        credential.detail,
        "the keychain is locked, and cannot ask to be unlocked from here"
    );
    assert!(
        credential.advice.contains("`security unlock-keychain`")
            && credential.advice.contains("keeps the login it last read"),
        "{}",
        credential.advice
    );
    let park = named(&checks, "account work");
    assert_eq!(park.level, Level::Warn, "{}", park.detail);
    assert_eq!(park.detail, "not read: the keychain is locked");
    assert!(!park.advice.contains("--sign-in"), "{}", park.advice);
    assert_eq!(
        checks.iter().filter(|c| c.level == Level::Fail).count(),
        1,
        "one cause, one failure"
    );

    // Where a login is left in the fallback file, a session started here signs in
    // with it, and the advice says so.
    f.fallback_login = Some(FallbackLogin {
        path: f.credential_file.clone(),
        fingerprint: Ok(Some("0123456789abcdef".into())),
    });
    let advice = check(&evaluate(&f), "credential").advice.clone();
    assert!(
        advice.contains("signs in with the login in /home/x/.claude/.credentials.json"),
        "{advice}"
    );
}

/// A login left in `.credentials.json` behind the one in the keychain is what a Claude
/// Code that cannot read the keychain signs in with, and no switch reaches it.
#[test]
fn a_login_left_behind_the_keychain_is_said_with_what_reads_it() {
    let mut f = facts();
    f.os = Os::MacOs;
    assert!(evaluate(&f).iter().all(|c| c.code != "fallback_login"));

    f.fallback_login = Some(FallbackLogin {
        path: f.credential_file.clone(),
        fingerprint: Ok(Some("0123456789abcdef".into())),
    });
    let checks = evaluate(&f);
    let said = check(&checks, "fallback_login");
    assert_eq!(said.level, Level::Warn);
    assert_eq!(said.name, "fallback login");
    assert_eq!(
        said.detail,
        "/home/x/.claude/.credentials.json  ·  refresh 0123456789abcdef"
    );
    assert!(
        said.advice.contains("over SSH")
            && said.advice.contains(
                "While it is there, Claude Code sessions already running at a switch keep \
                 the account they are on until their login is next renewed"
            )
            && said.advice.ends_with(
                "`pitboard stow` keeps this login for its account, where Pitboard holds no \
                 other it can switch to, then deletes the file, which leaves one login for \
                 every session."
            ),
        "{}",
        said.advice
    );
    assert!(!said.advice.contains("rm "), "{}", said.advice);
    assert!(healthy(&checks), "nothing Pitboard does is stopped by it");
}

/// A running session watches the file by its being there, so one with no login in it
/// holds sessions to their account after a switch all the same. It is said, with what
/// lets them follow, and the login check's advice does not send a session to it.
#[test]
fn a_file_behind_the_keychain_with_no_login_in_it_is_said_with_what_it_holds_back() {
    let mut f = facts();
    f.fallback_login = Some(FallbackLogin {
        path: f.credential_file.clone(),
        fingerprint: Ok(None),
    });
    let checks = evaluate(&f);
    let said = check(&checks, "fallback_login");
    assert_eq!(said.level, Level::Warn);
    assert_eq!(
        said.detail,
        "/home/x/.claude/.credentials.json  ·  no login in it"
    );
    assert_eq!(
        said.advice,
        "It holds no Claude Code login. While it is there, Claude Code sessions already \
         running at a switch keep the account they are on until their login is next \
         renewed, or until they are started again. `pitboard stow` deletes it, which lets \
         them follow a switch."
    );
    assert!(
        check(&checks, "auth_source")
            .detail
            .ends_with(", once a session reads it again"),
        "{}",
        check(&checks, "auth_source").detail
    );

    f.os = Os::MacOs;
    f.credential = Err(store::Error::Locked);
    let advice = check(&evaluate(&f), "credential").advice.clone();
    assert!(
        advice.ends_with("one started here is signed out."),
        "{advice}"
    );
}

/// A running session looks at the file and never reads it to decide, so one Pitboard
/// cannot read holds sessions to their account after a switch all the same. It is said
/// with why, without guessing whether a login is in it, and so is the login check's
/// advice for a session that cannot read the keychain.
#[test]
fn a_file_behind_the_keychain_that_cannot_be_read_is_said_with_why() {
    let mut f = facts();
    f.os = Os::MacOs;
    f.fallback_login = Some(FallbackLogin {
        path: f.credential_file.clone(),
        fingerprint: Err(store::Error::Unreadable(
            "cannot read /home/x/.claude/.credentials.json: Permission denied".into(),
        )),
    });
    let checks = evaluate(&f);
    let said = check(&checks, "fallback_login");
    assert_eq!(said.level, Level::Warn);
    assert_eq!(
        said.detail,
        "/home/x/.claude/.credentials.json  ·  not read: the credential store could not be \
         read: cannot read /home/x/.claude/.credentials.json: Permission denied"
    );
    assert_eq!(
        said.advice,
        "Pitboard could not read it, so whether it holds a login, which a session that \
         cannot read the keychain signs in with, cannot be told, and `pitboard stow` cannot \
         put it away. While it is there, Claude Code sessions already running at a switch \
         keep the account they are on until their login is next renewed, or until they are \
         started again. Deleting it lets them follow a switch, and takes whatever it holds \
         with it: `rm '/home/x/.claude/.credentials.json'`."
    );
    assert!(healthy(&checks), "nothing Pitboard does is stopped by it");

    f.credential = Err(store::Error::Locked);
    let advice = check(&evaluate(&f), "credential").advice.clone();
    assert!(
        advice.ends_with(
            "whether one started here signs in with what /home/x/.claude/.credentials.json \
             holds cannot be told, since Pitboard could not read it."
        ),
        "{advice}"
    );
}

// Behind Credential Manager, a network logon is what reads the file, and PowerShell deletes
// it, with each single quote in the path doubled.
#[test]
fn a_file_behind_credential_manager_is_said_in_the_words_of_windows() {
    let mut f = facts();
    f.os = Os::Windows;
    let path = PathBuf::from(r"C:\Users\o'neil\.claude\.credentials.json");
    f.fallback_login = Some(FallbackLogin {
        path: path.clone(),
        fingerprint: Ok(Some("0123456789abcdef".into())),
    });
    let said = check(&evaluate(&f), "fallback_login").advice.clone();
    assert!(
        said.starts_with(
            "Claude Code signs in with this wherever it cannot reach Credential Manager, \
             such as in a sign-in over SSH with a key, and no switch reaches it. A sign-in \
             made where Credential Manager could not be reached leaves one, as `/login` does \
             in a sign-in over SSH with a key. While it is there,"
        ),
        "{said}"
    );

    f.fallback_login = Some(FallbackLogin {
        path,
        fingerprint: Err(store::Error::Unreadable("access is denied".into())),
    });
    let said = check(&evaluate(&f), "fallback_login").advice.clone();
    assert!(
        said.starts_with(
            "Pitboard could not read it, so whether it holds a login, which a session that \
             cannot reach Credential Manager signs in with, cannot be told"
        ),
        "{said}"
    );
    assert!(
        said.ends_with(
            r"with it: `Remove-Item -LiteralPath 'C:\Users\o''neil\.claude\.credentials.json'`."
        ),
        "{said}"
    );
    assert_eq!(
        Os::Windows.remove_file_command(std::path::Path::new("C:\\it\u{2019}s")),
        "Remove-Item -LiteralPath 'C:\\it\u{2019}\u{2019}s'"
    );
    assert_eq!(
        Os::MacOs.remove_file_command(std::path::Path::new("/Users/x/.claude/.credentials.json")),
        "rm '/Users/x/.claude/.credentials.json'"
    );
    assert_eq!(
        Os::Linux.remove_file_command(std::path::Path::new(
            "/home/x/Application Support/it's/.credentials.json"
        )),
        r"rm '/home/x/Application Support/it'\''s/.credentials.json'"
    );
}

/// What the checks above are given, read off the stores: the file while the keychain
/// holds the login in use, with the login in it, none, or why it cannot be read, and
/// nothing where the file is the store in use.
#[test]
fn a_login_left_behind_the_keychain_is_found_in_its_file() {
    use crate::host::memory::MemoryHost;
    use crate::store::memory::Fault;

    let host = MemoryHost::new();
    let ctx = Context::for_unit_test().with_memory_stores(Arc::clone(&host));
    let service = claude_paths::live_service(&ctx);
    let file = host.file_at(claude_live::credential_file(&ctx));
    let login = |token: &str| json!({"claudeAiOauth": {"refreshToken": token}}).to_string();

    host.live().plant(&service, &login("in use"));
    assert!(fallback_login(&ctx).is_none());

    file.plant(&service, &login("left"));
    let found = fallback_login(&ctx).expect("a login behind the keychain's");
    assert_eq!(found.path, claude_live::credential_file(&ctx));
    assert_eq!(
        found.fingerprint.expect("readable"),
        Some(store::fingerprint("left"))
    );

    file.plant(&service, &json!({"mcpOAuth": {}}).to_string());
    let found = fallback_login(&ctx).expect("what `/logout` leaves is still there");
    assert_eq!(
        found.fingerprint.expect("readable"),
        None,
        "and holds no login"
    );

    file.plant(&service, &login("left"));
    file.fault(
        &service,
        Fault::UnreadableContents("permission denied".into()),
    );
    let found = fallback_login(&ctx).expect("what cannot be read is still there");
    assert!(
        matches!(found.fingerprint, Err(store::Error::Unreadable(_))),
        "and what it holds is not guessed"
    );
    file.heal(&service);

    host.live().delete_everything();
    assert!(
        fallback_login(&ctx).is_none(),
        "the file is the login in use when the keychain holds none"
    );
}

// One that gives no date is no warning.
#[test]
fn the_login_in_use_is_judged_by_when_its_refresh_token_expires_as_a_park_is() {
    let credential = |f: &mut Facts, expires: Option<i64>| {
        let mut oauth = json!({"refreshToken": "r"});
        if let Some(at) = expires {
            oauth["refreshTokenExpiresAt"] = json!(at * 1000);
        }
        f.credential = Ok(Some(json!({ "claudeAiOauth": oauth })));
        let checks = evaluate(f);
        let said = check(&checks, "credential");
        (said.level, said.detail.clone(), said.advice.clone())
    };
    let mut f = facts();
    let now = f.now;
    let (level, detail, _) = credential(&mut f, Some(now + RENEW_WITHIN));
    assert_eq!(level, Level::Ok);
    assert!(detail.contains("  ·  good for 3d 0h  ·  "), "{detail}");
    let (level, detail, advice) = credential(&mut f, Some(now + RENEW_WITHIN - 1));
    assert_eq!(level, Level::Warn);
    assert!(detail.contains("  ·  expires in 2d 23h  ·  "), "{detail}");
    assert_eq!(
        advice,
        "This login expires soon and will need signing in again."
    );
    let (level, detail, advice) = credential(&mut f, Some(now - 60));
    assert_eq!(level, Level::Warn);
    assert!(detail.contains("  ·  expired  ·  "), "{detail}");
    assert_eq!(
        advice,
        "This login has expired: sign in to Claude Code again."
    );
    let (level, detail, _) = credential(&mut f, None);
    assert_eq!(level, Level::Ok);
    assert!(!detail.contains("days"), "{detail}");
}

#[test]
fn the_successor_backend_alone_does_not_move_the_login() {
    let server_on = || {
        let mut f = facts();
        f.config = Ok(json!({"cachedGrowthBookFeatures": {"tengu_hover_rest": true}}));
        f
    };
    let env_on = || {
        let mut f = facts();
        f.hover_rest_env = true;
        f
    };
    for mut f in [server_on(), env_on()] {
        // On the keychain, the flag changes nothing Pitboard reads.
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "storage_v5").level, Level::Ok);
        // In the fallback, it is the one case worth saying something about.
        f.backend = Ok(store::Backend::File);
        let checks = evaluate(&f);
        assert_eq!(check(&checks, "storage_v5").level, Level::Warn);
    }
}

#[test]
fn the_successor_backend_is_quiet_when_it_is_off() {
    let mut f = facts();
    let checks = evaluate(&f);
    assert_eq!(check(&checks, "storage_v5").level, Level::Ok);
    // The fallback store on its own is not the successor backend.
    f.backend = Ok(store::Backend::File);
    let checks = evaluate(&f);
    assert_eq!(check(&checks, "storage_v5").level, Level::Ok);
}

// On macOS the file is a fallback after a keychain write failed. On Windows the chain is
// the file only where nothing turns Credential Manager on and it holds no login.
#[test]
fn a_file_store_is_the_whole_answer_on_linux_and_windows() {
    let mut f = facts();
    f.backend = Ok(store::Backend::File);
    for (os, level, advice) in [
        (Os::Linux, Level::Ok, ""),
        (
            Os::MacOs,
            Level::Warn,
            "Claude Code fell back to a file, which means a keychain write failed at some \
             point.",
        ),
        (Os::Windows, Level::Ok, ""),
    ] {
        f.os = os;
        let checks = evaluate(&f);
        let store = check(&checks, "credential_store");
        assert_eq!(
            (store.level, store.advice.as_str()),
            (level, advice),
            "{os:?}"
        );
        assert!(store.detail.starts_with("plaintext file"), "{os:?}");
    }
}

/// The failure that would follow Claude Code moving where it keeps a login: not an
/// absence, a mismatch, and the difference is what decides whether the advice is "sign
/// in" or "Pitboard is looking in the wrong place".
#[test]
fn a_config_that_names_somebody_signed_in_with_no_login_anywhere_is_a_failure() {
    let mut f = facts();
    f.backend = Ok(store::Backend::Absent);
    let checks = evaluate(&f);
    let store = check(&checks, "credential_store");
    assert_eq!(store.level, Level::Fail);
    assert!(store.detail.contains("a@b.c"));

    // Nobody signed in at all is an ordinary state with an ordinary answer.
    f.config_account = None;
    let checks = evaluate(&f);
    let store = check(&checks, "credential_store");
    assert_eq!(store.level, Level::Warn);
    assert!(store.advice.contains("Nothing is signed in"));
}

// Enterprise persistence (`credman_attributes`): the login roams with a roaming profile.
#[test]
fn credential_manager_is_named_by_its_item_and_a_roaming_profile_is_warned_of() {
    let mut f = facts();
    f.os = Os::Windows;
    f.backend = Ok(store::Backend::CredentialManager);
    f.account = "claude-code-user".into();
    f.windows = Some(WindowsHomes {
        home: PathBuf::from(r"C:\Users\dana"),
        default_pitboard_home: PathBuf::from(r"C:\Users\dana\AppData\Local\Pitboard"),
        local_app_data: None,
        profile: None,
        codex_home_set: false,
        roams: false,
    });
    let checks = evaluate(&f);
    let store = check(&checks, "credential_store");
    assert_eq!(store.level, Level::Ok);
    assert_eq!(
        store.detail,
        "Credential Manager  ·  Claude Code-credentials/claude-code-user  ·  every logon of \
         yours on this PC reads it"
    );
    if let Some(homes) = f.windows.as_mut() {
        homes.roams = true;
    }
    let checks = evaluate(&f);
    let store = check(&checks, "credential_store");
    assert_eq!(store.level, Level::Warn);
    assert!(
        store.advice.starts_with("Your profile roams"),
        "{}",
        store.advice
    );
}

#[test]
fn a_login_in_credential_manager_is_counted_in_pieces_and_257_fail() {
    let mut f = facts();
    f.os = Os::Windows;
    f.backend = Ok(store::Backend::CredentialManager);
    let size = |f: &Facts, needs: usize| {
        let mut f_cost = f.credential_cost.expect("a cost");
        f_cost.needs = needs;
        judge_pieces(f_cost, &f.credential_parts)
    };
    f.credential_cost = Some(store::Cost {
        needs: 900,
        limit: 460_800,
        second_route: false,
    });
    assert_eq!(
        check(&evaluate(&f), "credential_size").detail,
        "900 bytes, in one item"
    );
    let pieces = size(&f, 2401);
    assert_eq!(pieces.level, Level::Ok);
    assert_eq!(pieces.detail, "2401 bytes, in 2 pieces of 2400 characters");
    let past = size(&f, 460_801);
    assert_eq!(past.level, Level::Fail);
    assert_eq!(
        past.detail,
        words::past_credential_manager(460_801, 460_800)
    );
}

#[test]
fn pitboard_no_argv_is_said_to_change_nothing_on_windows() {
    let mut f = facts();
    f.no_argv = true;
    f.os = Os::MacOs;
    assert!(evaluate(&f).iter().all(|c| c.code != "no_argv"));
    f.os = Os::Windows;
    let checks = evaluate(&f);
    let said = check(&checks, "no_argv");
    assert_eq!(said.level, Level::Warn);
    assert_eq!(said.detail, "set, and it changes nothing on Windows");
}

// A network logon, as over sshd with a key, has no credential set: 1312 (MC-net).
#[test]
fn credential_manager_out_of_this_logons_reach_is_said_in_its_own_words() {
    let mut f = facts();
    f.os = Os::Windows;
    f.credential_file = PathBuf::from(r"C:\Users\dana\.claude\.credentials.json");
    f.credential = Err(store::Error::Locked);
    let checks = evaluate(&f);
    let said = check(&checks, "credential");
    assert_eq!(said.level, Level::Fail);
    assert_eq!(
        said.detail,
        "Credential Manager cannot be reached from this logon"
    );
    assert!(
        said.advice.ends_with(
            r"A Claude Code session started from this logon reads only C:\Users\dana\.claude\.credentials.json."
        ),
        "{}",
        said.advice
    );
    assert!(!said.advice.contains("keychain"));
}

#[test]
fn a_login_credential_manager_holds_that_nothing_turns_on_is_said_to_sign_sessions_out() {
    let mut f = facts();
    f.os = Os::Windows;
    f.backend = Err(store::Error::Unreadable(
        "Credential Manager holds a login".into(),
    ));
    let checks = evaluate(&f);
    let said = check(&checks, "credential_store");
    assert_eq!(said.level, Level::Fail);
    assert!(
        said.advice.ends_with(
            "its sessions read only .credentials.json, so a login left only in Credential \
             Manager is signed out to them."
        ),
        "{}",
        said.advice
    );
}

#[test]
fn a_login_past_the_ceiling_reads_differently_when_the_way_past_it_is_refused() {
    let mut f = facts();
    let checks = evaluate(&f);
    assert_eq!(check(&checks, "credential_size").level, Level::Ok);

    f.credential_cost = Some(store::Cost {
        needs: 8503,
        limit: 4032,
        second_route: true,
    });
    let checks = evaluate(&f);
    let written = check(&checks, "credential_size");
    assert_eq!(written.level, Level::Warn);
    assert!(written.detail.contains("argument line"));

    f.credential_cost = Some(store::Cost {
        needs: 8503,
        limit: 4032,
        second_route: false,
    });
    let checks = evaluate(&f);
    let refused = check(&checks, "credential_size");
    assert_eq!(
        refused.level,
        Level::Fail,
        "there is no third way to write it"
    );
    assert!(refused.detail.contains("PITBOARD_NO_ARGV"));
}

/// An account nobody has come back to keeps a live, continuously rotated refresh token
/// on this machine for as long as it stays enrolled. Nothing is dropped on a timer
/// Pitboard chose; the threshold is the token's own lifetime and all it does is say so.
/// A login that will not fit is almost never the login. On one real machine the OAuth
/// block was 506 bytes and eleven MCP server tokens were 3679, and the check said
/// "8503 of 4032 bytes" and left the person to guess which of those to act on.
#[test]
fn a_login_too_big_says_what_is_taking_up_the_room() {
    let mut f = facts();
    f.credential_cost = Some(store::Cost {
        needs: 8503,
        limit: 4032,
        second_route: true,
    });
    f.credential_parts = parts_of(&json!({
        "claudeAiOauth": {"refreshToken": "r"},
        "mcpOAuth": {
            "one": {"token": "x".repeat(300)},
            "two": {"token": "y".repeat(200)},
            "three": {"token": "z".repeat(100)},
        },
    }));

    let checks = evaluate(&f);
    let size = check(&checks, "credential_size");
    assert!(size.detail.contains("mcpOAuth one"), "{}", size.detail);
    assert!(size.detail.contains("mcpOAuth two"), "{}", size.detail);
    assert!(
        !size.detail.contains("claudeAiOauth"),
        "three is enough to act on, and the login itself is never the problem: {}",
        size.detail
    );
}

#[test]
fn what_takes_up_the_room_is_listed_largest_first_and_named_per_server() {
    let parts = parts_of(&json!({
        "claudeAiOauth": {"refreshToken": "r"},
        "mcpOAuth": {"small": {"t": "x"}, "large": {"t": "y".repeat(500)}},
    }));
    let names: Vec<&str> = parts.iter().map(|(what, _)| what.as_str()).collect();
    assert_eq!(names[0], "mcpOAuth large", "largest first: {names:?}");
    assert!(names.contains(&"claudeAiOauth"));
    assert!(
        names.contains(&"mcpOAuth small"),
        "each server is named, because that is what a person signs out of"
    );

    // One server is not worth breaking apart; the key says it already.
    let single = parts_of(&json!({"mcpOAuth": {"only": {"t": "x"}}}));
    assert_eq!(single.len(), 1);
    assert_eq!(single[0].0, "mcpOAuth");
}

#[test]
fn the_daemon_is_reported_without_being_a_problem() {
    let mut f = facts();
    let checks = evaluate(&f);
    assert!(check(&checks, "claude_daemon").detail.contains("none"));

    f.daemon = Some(daemon::Daemon {
        pid: 4321,
        version: Some("2.1.278".into()),
        running: true,
    });
    let checks = evaluate(&f);
    let running = check(&checks, "claude_daemon");
    assert_eq!(running.level, Level::Ok);
    assert!(running.detail.contains("running"));
    assert!(running.detail.contains("2.1.278"));
    assert!(running.detail.contains("4321"));

    f.daemon.as_mut().expect("set above").running = false;
    let checks = evaluate(&f);
    let stopped = check(&checks, "claude_daemon");
    assert_eq!(stopped.level, Level::Ok);
    assert!(stopped.detail.contains("not running"));
}

/// The account in use is the one Anthropic last named for the login stored, known by
/// that login's fingerprint, and doctor says when it was named. Claude Code's config
/// naming another, once a read has found the login is still the one named, is said, and
/// moves nothing.
#[test]
fn the_account_in_use_is_the_one_anthropic_named() {
    use crate::switch::harness::{NOW, config_names, machine};
    let m = machine("doctor-in-use");
    let at = time::moment(NOW, NOW);

    let facts = gather(&m.ctx);
    let checks = evaluate(&facts);
    let said = check(&checks, "in_use");
    assert_eq!(said.level, Level::Ok);
    assert_eq!(said.detail, format!("`here`, as Anthropic said {at}"));
    assert_eq!(in_use(&facts), ["here"]);

    config_names(&m, "there");
    crate::service::Pitboard::new(m.ctx.clone())
        .status(false)
        .expect("a read");
    let facts = gather(&m.ctx);
    let checks = evaluate(&facts);
    let said = check(&checks, "in_use");
    assert_eq!(said.level, Level::Warn);
    assert_eq!(
        said.detail,
        format!("`here`, as Anthropic said {at}, and Claude Code's config names `there`")
    );
    assert_eq!(
        said.advice,
        "`/status` in Claude Code shows `there`. `pitboard use here` writes `here` into the \
         config."
    );
    assert_eq!(in_use(&facts), ["here"]);
}

/// Claude Code renews its login by itself, and a renewed login has another fingerprint.
/// That is what happens every few hours, so it is no warning: the next read asks whose it
/// is. Doctor asks nobody.
#[test]
fn a_renewed_login_is_not_a_warning() {
    use crate::switch::harness::{NOW, document, machine};
    let m = machine("doctor-renewed");
    m.sign_in(&document("here-renewed"));

    let checks = evaluate(&gather(&m.ctx));
    let said = check(&checks, "in_use");

    assert_eq!(said.level, Level::Ok);
    assert_eq!(
        said.detail,
        format!(
            "`here`, as Anthropic said {} of a login renewed or replaced since; the next \
             `pitboard status` asks whose it is",
            time::moment(NOW, NOW)
        )
    );
    assert_eq!(m.api.calls(), 0);
}

/// Claude Code's config has named another account since Anthropic last named the login
/// stored, as a sign-in leaves it and another Claude Code process starting on another
/// login can. Something may have signed in, and only Anthropic can say whose the login is
/// now.
#[test]
fn a_config_that_moved_since_is_a_warning() {
    use crate::switch::harness::{NOW, config_names, machine};
    let m = machine("doctor-config-moved");
    config_names(&m, "there");

    let facts = gather(&m.ctx);
    let checks = evaluate(&facts);
    let said = check(&checks, "in_use");

    assert_eq!(said.level, Level::Warn);
    assert_eq!(
        said.detail,
        format!(
            "Claude Code's config has named `there` since Anthropic named `here` {}, so \
             another login may be stored",
            time::moment(NOW, NOW)
        )
    );
    assert_eq!(
        said.advice,
        "Run `pitboard status`, which asks Anthropic whose login Claude Code has stored."
    );
    assert_eq!(in_use(&facts), ["here"]);
    assert_eq!(m.api.calls(), 0);
}

// Claude Code 2.1.309 refuses, as a session starts, a login of an organisation a managed
// `forceLoginOrgUUID` leaves out (organisation_pin), so the setting is named as Codex's are.
#[test]
fn a_managed_organisation_pin_is_named_with_the_accounts_it_rules_out() {
    use crate::switch::harness::account;
    let host = crate::host::memory::MemoryHost::new();
    let ctx = Context::for_unit_test().with_memory_stores(host.clone());
    let mut state = State::default();
    state.accounts.push(account("here", "here", None));
    state.accounts.push(account("there", "there", None));
    let judged = |f: &Facts| evaluate(f);
    let mut f = facts();
    (f.organisation_pins, f.pinned_out) = login_pins(&ctx, Some(&state));
    assert!(judged(&f).iter().all(|c| c.code != "login_restrictions"));

    let file = crate::settings::managed_dir(ctx.os).join("managed-settings.json");
    host.administers(&file, r#"{"forceLoginOrgUUID": ["org-here"]}"#);
    (f.organisation_pins, f.pinned_out) = login_pins(&ctx, Some(&state));
    let checks = judged(&f);
    let found = check(&checks, "login_restrictions");
    assert_eq!(found.level, Level::Warn);
    assert_eq!(found.name, "login restrictions");
    assert_eq!(
        found.detail,
        format!(
            "`forceLoginOrgUUID` set to `org-here` in {}",
            file.display()
        )
    );
    assert_eq!(
        found.advice,
        "Claude Code refuses the logins of `there` as a session starts, so Pitboard does \
         not put them in use."
    );

    host.administers(&file, r#"{"forceLoginOrgUUID": ["org-here", "org-there"]}"#);
    (f.organisation_pins, f.pinned_out) = login_pins(&ctx, Some(&state));
    assert_eq!(check(&judged(&f), "login_restrictions").level, Level::Ok);
}

// Add account said Claude Code was not installed, and This PC said to run it.
#[test]
fn a_machine_without_claude_code_is_told_to_install_it() {
    let mut facts = facts();
    facts.claude_program = None;
    facts.config = Err(Error::ClaudeConfigMissing {
        path: PathBuf::from("/home/x/.claude.json"),
        spelling: words::Spelling::Bare,
    });
    facts.config_account = None;
    let checks = evaluate(&facts);
    let said = |code: &str| {
        let check = checks.iter().find(|c| c.code == code).expect(code);
        (check.detail.clone(), check.advice.clone())
    };
    assert_eq!(
        said("config_file"),
        (
            "Claude Code is not installed where Pitboard looks, and /home/x/.claude.json does \
             not exist"
                .into(),
            "Install Claude Code, then run `claude` once.".into()
        )
    );
    assert_eq!(
        said("identity").1,
        "It writes this after its first successful call. Install Claude Code, then run \
         `claude` once."
    );
}

// Windows PowerShell 5.1 refuses the `claude.ps1` beside npm's `claude.cmd`.
#[test]
fn a_machine_where_claude_code_never_ran_is_told_to_run_it() {
    for (os, found, spelling) in [
        (Os::MacOs, "/opt/homebrew/bin/claude", words::Spelling::Bare),
        (
            Os::Windows,
            "C:/Users/x/.local/bin/claude.exe",
            words::Spelling::Bare,
        ),
        (
            Os::Windows,
            "C:/Users/x/AppData/Roaming/npm/claude.cmd",
            words::Spelling::Cmd,
        ),
    ] {
        let program = spelling.of(ProviderId::Claude);
        let mut facts = facts();
        facts.os = os;
        facts.claude_program = Some(PathBuf::from(found));
        let missing = Error::ClaudeConfigMissing {
            path: PathBuf::from("/home/x/.claude.json"),
            spelling,
        };
        let detail = missing.to_string();
        facts.config = Err(missing);
        facts.config_account = None;
        let checks = evaluate(&facts);
        let said = |code: &str| {
            let check = checks.iter().find(|c| c.code == code).expect(code);
            (check.detail.clone(), check.advice.clone())
        };
        assert_eq!(
            said("config_file"),
            (detail, format!("Run `{program}` once.")),
            "{found}"
        );
        assert_eq!(
            said("identity").1,
            format!("It writes this after its first successful call. Run `{program}` once.")
        );
    }
}

// The session's fresh PC read "1 broken: do not switch accounts until fixed." for this
// alone, with nothing enrolled and Claude Code installed but never run.
#[test]
fn claude_code_that_never_ran_breaks_nothing() {
    let mut facts = facts();
    facts.config = Err(Error::ClaudeConfigMissing {
        path: PathBuf::from("/home/x/.claude.json"),
        spelling: words::Spelling::Bare,
    });
    facts.config_account = None;
    facts.in_use = None;
    facts.credential = Ok(None);
    for installed in [true, false] {
        facts.claude_program = installed.then(|| PathBuf::from("/opt/homebrew/bin/claude"));
        let checks = evaluate(&facts);
        assert_eq!(
            check(&checks, "config_file").level,
            Level::Warn,
            "{installed}"
        );
        assert!(
            checks.iter().all(|c| c.level != Level::Fail),
            "{installed}: {:?}",
            checks
                .iter()
                .filter(|c| c.level == Level::Fail)
                .map(|c| c.code)
                .collect::<Vec<_>>()
        );
    }
}

// What Claude Code 2.1.302 left on the session's PC (2026-10-18) as it moved its login
// into Credential Manager, which a fresh session read as "Not logged in".
#[test]
fn pieces_nobody_can_join_are_said_to_be_unreadable_to_claude_code_too() {
    for (found, spelling) in [
        ("C:/Users/x/.local/bin/claude.exe", words::Spelling::Bare),
        (
            "C:/Users/x/AppData/Roaming/npm/claude.cmd",
            words::Spelling::Cmd,
        ),
    ] {
        let program = spelling.of(ProviderId::Claude);
        let mut facts = facts();
        facts.os = Os::Windows;
        facts.claude_program = Some(PathBuf::from(found));
        facts.backend = Ok(store::Backend::CredentialManager);
        facts.credential = Err(store::Error::Broken {
            why: "`Claude Code-credentials/claude-code-user` lacks piece #2 of the 3 its \
                  manifest names"
                .into(),
            spelling,
        });
        let check = judge_credential(&facts);
        assert_eq!(check.level, Level::Fail);
        assert_eq!(
            check.detail,
            "`Claude Code-credentials/claude-code-user` lacks piece #2 of the 3 its manifest \
             names, which Claude Code cannot read either"
        );
        assert!(
            check
                .advice
                .starts_with(&format!("Run `{program} auth logout`")),
            "{}",
            check.advice
        );
    }
}

/// What `/logout` leaves is nobody signed in, not a login whose shape moved.
#[test]
fn a_signed_out_credential_is_said_to_be_one() {
    let mut facts = facts();
    facts.credential = Ok(Some(serde_json::json!({"mcpOAuth": {"server": {}}})));
    let check = judge_credential(&facts);
    assert!(matches!(check.level, Level::Warn), "{}", check.detail);
    assert!(check.detail.contains("signed out"), "{}", check.detail);
}

// A switch refuses it as a shape it does not know, so it is no signed-out document.
#[test]
fn a_credential_that_is_not_an_object_is_a_failure() {
    use crate::provider::ProviderError;
    let document = serde_json::json!(["claudeAiOauth"]);
    assert!(matches!(
        crate::provider::claude_code().slice(&document),
        Err(ProviderError::ShapeUnexpected { .. })
    ));
    let mut facts = facts();
    facts.credential = Ok(Some(document));
    let judged = judge_credential(&facts);
    assert_eq!(judged.level, Level::Fail);
    assert_eq!(judged.detail, "the stored login is not a JSON object");
}

// Which login it is cannot be told, which is not that none is stored.
#[test]
fn a_stored_login_that_is_not_an_object_leaves_the_account_in_use_as_named() {
    use crate::switch::harness::{NOW, machine};
    let m = machine("doctor-not-an-object");
    m.sign_in(&serde_json::json!(["claudeAiOauth"]));
    let checks = evaluate(&gather(&m.ctx));
    assert_eq!(
        check(&checks, "in_use").detail,
        format!("`here`, as Anthropic said {}", time::moment(NOW, NOW))
    );
}
