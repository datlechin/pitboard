//! The harness in `common`, which every other integration test stands on.

use crate::common;
use crate::common::{Env, account, guard_not_live, os, real_slots, refuse_a_real_login_place};
use pitboard_core::testing::fs as files;
use pitboard_core::testing::lies_in;

#[test]
fn a_folder_that_is_holds_or_lies_in_a_login_place_clashes() {
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path();
    let place = root.join("home").join(".claude");
    std::fs::create_dir_all(&place).unwrap();
    for scratch in [
        place.clone(),
        place.join("inside"),
        root.join("home"),
        root.join("home").join(".CLAUDE").join("inside"),
    ] {
        assert!(lies_in(&scratch, &place), "{}", scratch.display());
    }
    for scratch in [root.join("home").join(".claudex"), root.join("elsewhere")] {
        assert!(!lies_in(&scratch, &place), "{}", scratch.display());
    }
}

#[test]
fn a_link_into_a_login_place_clashes() {
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path();
    let place = root.join(".codex");
    std::fs::create_dir_all(&place).unwrap();
    files::link_dir(&place, &root.join("through")).unwrap();
    assert!(lies_in(&root.join("through").join("inside"), &place));
}

// /System/Volumes/Data/Users is /Users, and `canonicalize` keeps either name as it is.
#[test]
#[cfg_attr(
    not(target_os = "macos"),
    ignore = "a firmlink to the Data volume is macOS's own"
)]
fn a_folder_named_through_the_data_volume_clashes() {
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path();
    let place = root.join(".claude");
    std::fs::create_dir_all(&place).unwrap();
    let real = std::fs::canonicalize(&place).unwrap();
    let through =
        std::path::Path::new("/System/Volumes/Data").join(real.strip_prefix("/").unwrap());
    let caught = lies_in(&through.join("inside"), &place);
    assert!(caught, "{}", through.display());
}

#[test]
fn every_real_login_place_is_refused() {
    let places = pitboard_core::testing::real_login_places(&common::started_in()).unwrap();
    assert!(!places.is_empty());
    for place in places {
        let inside = place.join("pitboard-guard");
        let caught = std::panic::catch_unwind(|| refuse_a_real_login_place(&inside));
        assert!(caught.is_err(), "{} must be refused", inside.display());
    }
}

#[test]
fn the_guard_refuses_every_name_a_real_login_may_be_kept_under() {
    let mut refused: Vec<String> = [
        "Claude Code-credentials",
        "Claude Code-credentials/claude-code-user",
        "Claude Code-credentials/claude-code-user#0",
        "Claude Code-credentials/claude-code-user#12",
        "Claude Code-credentials/claude-code-user#m",
        "Claude Code-credentials/claude-code-user#p",
        "Claude Code-credentials-e80beed8",
        "Claude Code-credentials-e80beed8/claude-code-user#3",
        "Claude Code-credentials-deadbeef",
        "claude code-credentials/CLAUDE-CODE-USER",
        "LegacyGeneric:target=Claude Code-credentials/claude-code-user",
        "Claude Code-staging-credentials",
        "cli|1a2b3c4d5e6f7a8b",
        "CLI|1A2B3C4D5E6F7A8B",
        "secrets|1a2b3c4d5e6f7a8b",
        "Codex MCP Credentials",
        "Codex MCP Credentials/linear",
        "linear|3f2a.Codex MCP Credentials",
    ]
    .map(str::to_owned)
    .into();
    for slot in real_slots() {
        for account in ["claude-code-user", &account()] {
            refused.push(format!("{slot}/{account}"));
            for piece in ["#0", "#1", "#m", "#p"] {
                refused.push(format!("{slot}/{account}{piece}"));
            }
        }
        refused.push(slot);
    }
    for name in &refused {
        let caught = std::panic::catch_unwind(|| guard_not_live(name));
        assert!(caught.is_err(), "{name} must be refused");
    }
    for name in [
        "pitboard-citest-1",
        "pitboard-park-0a1b2c3d-1",
        "GitHub - https://github.com",
    ] {
        guard_not_live(name);
    }
}

#[test]
fn a_test_may_address_its_own_slot_alone() {
    let env = Env::new("own-slot");
    let own = env.service.clone();
    for spelling in [
        own.clone(),
        own.to_lowercase(),
        format!("{own}/claude-code-user"),
        format!("{own}/{}", account()),
        format!("{own}/claude-code-user#0"),
        format!("{own}/claude-code-user#7"),
        format!("{own}/claude-code-user#m"),
        format!("{own}/claude-code-user#p"),
        format!("{own}#0"),
    ] {
        env.guard(&spelling);
    }
    let other = pitboard_core::testing::service_for_dir(&env.root.join("other").to_string_lossy());
    for name in [
        other.clone(),
        format!("{other}/claude-code-user#0"),
        format!("{own}/claude-code-user/more"),
        format!("{own}#x"),
        format!("{own}0"),
    ] {
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| env.guard(&name)));
        assert!(caught.is_err(), "{name} must be refused");
    }
}

// Two tests given one name would share a root, and with it every keychain name made from it.
#[test]
fn a_name_makes_one_env_in_a_process() {
    let _env = Env::new("named-once");
    let again = std::panic::catch_unwind(|| Env::new("named-once"));
    assert!(again.is_err(), "a second Env took a name already taken");
}

// `delete_park` once deleted nothing on Linux, and nothing checked the harness itself.
#[test]
fn the_harness_can_park_a_login_find_it_and_take_it_away() {
    let env = Env::new("harness-round-trip");
    let own = pitboard_core::testing::dir_hash(&env.root.to_string_lossy());
    let service = format!("pitboard-park-{own}-1");

    assert!(!env.is_parked(&service), "nothing is parked to begin with");
    env.write_park(&service, r#"{"claudeAiOauth":{"refreshToken":"r"}}"#);
    assert!(env.is_parked(&service), "what was written is found");
    env.delete_park(&service);
    assert!(!env.is_parked(&service), "what was deleted is gone");
    // A test may delete a park that is already gone.
    env.delete_park(&service);
}

#[test]
fn a_discarded_test_park_is_deleted_by_env_drop() {
    let env = Env::new("discarded-park");
    let service = format!(
        "pitboard-park-{}-1",
        pitboard_core::testing::dir_hash("discarded-park")
    );
    std::fs::create_dir_all(env.root.join("pitboard")).unwrap();
    std::fs::write(
        env.root.join("pitboard/state.json"),
        serde_json::json!({"schema": 8, "discarded": [{"service": service}]}).to_string(),
    )
    .unwrap();
    env.write_park(&service, r#"{"claudeAiOauth":{"refreshToken":"r"}}"#);
    let ctx = env.context();
    drop(env);

    let left = pitboard_core::testing::vault_read(&ctx, &service).map(|found| found.is_some());
    let _ = pitboard_core::testing::vault_delete(&ctx, &service);
    assert!(
        matches!(left, Ok(false)),
        "{service} outlived the test that parked it: {left:?}"
    );
}

// `status` reads every tool's live login, and the suite once put the developer's Codex
// account in a snapshot.
#[test]
fn every_tool_is_pointed_at_a_scratch_home() {
    let env = Env::new("isolation");
    let command = env.command(&["status"]);
    let named: Vec<(String, String)> = command
        .get_envs()
        .filter_map(|(k, v)| {
            Some((
                k.to_string_lossy().into_owned(),
                v?.to_string_lossy().into_owned(),
            ))
        })
        .collect();
    let folders = os::account_folders(&env.root);
    let homes = [
        "CLAUDE_CONFIG_DIR",
        "CODEX_HOME",
        "PITBOARD_HOME",
        "PITBOARD_TEST_CLAUDE_DESKTOP",
    ]
    .into_iter()
    .chain(folders.iter().map(|(name, _)| *name));
    for (_, folder) in &folders {
        assert!(folder.is_dir(), "{} is not made", folder.display());
    }
    for home in homes {
        let set = named.iter().find(|(k, _)| k == home).unwrap_or_else(|| {
            panic!("{home} is not pointed anywhere, so a test reads a real one")
        });
        assert!(
            std::path::Path::new(&set.1).starts_with(&env.root),
            "{home} is {} , which is outside this test's own directory",
            set.1
        );
    }
    // Set at all, even empty, it pins Claude Code's default slot, the real login.
    let removed: Vec<String> = command
        .get_envs()
        .filter(|(_, v)| v.is_none())
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .collect();
    assert!(
        removed
            .iter()
            .any(|k| k == "CLAUDE_SECURESTORAGE_CONFIG_DIR"),
        "CLAUDE_SECURESTORAGE_CONFIG_DIR is inherited, so a test can read the real slot"
    );
    let given: Vec<String> = command
        .get_envs()
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .collect();
    for name in common::withheld() {
        assert!(
            given.iter().any(|k| os::same_variable(k, name)),
            "{name} is inherited from whoever runs the tests"
        );
    }
}
