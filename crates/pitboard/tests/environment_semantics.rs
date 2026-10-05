//! Proves the environment is read the way Claude Code reads it, by the binary itself: the
//! combinations are unit-tested in `claude.rs`, and this checks that an empty
//! `CLAUDE_CONFIG_DIR` reaches the file opened and the slot read as unset.

use std::path::{Path, PathBuf};
use std::process::Command;

/// `pitboard doctor --json`, with `vars` set besides, and every home it reads in `home`.
fn command(home: &Path, config_dir: Option<&str>, vars: &[(&str, &Path)]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pitboard"));
    // Nothing real is read, and nothing Pitboard reads is taken from whoever runs the tests.
    // The slot under test is the default one, so the keychain account is a name nobody has
    // and the lookup finds no item; Codex gets a home of its own; and PATH holds only the
    // system's directories, so no installed `claude` or `codex` is resolved either. Claude
    // Desktop's data folder and app are pointed at places that do not exist, so neither the
    // person's own data nor their install is read.
    for name in pitboard_core::testing::variables() {
        command.env_remove(name);
    }
    command
        .args(["doctor", "--json"])
        .env("HOME", home)
        .env("USER", "pitboard-test-nobody")
        .env("PITBOARD_HOME", home.join("pitboard"))
        .env("CODEX_HOME", home.join("codex"))
        .env("PITBOARD_CLAUDE_DESKTOP_DIR", home.join("claude-desktop"))
        .env("PITBOARD_CLAUDE_DESKTOP_APP", home.join("Claude.app"))
        .env("PATH", "/usr/bin:/bin");
    if let Some(v) = config_dir {
        command.env("CLAUDE_CONFIG_DIR", v);
    }
    for (name, value) in vars {
        command.env(name, value);
    }
    command
}

/// `pitboard doctor --json`, with `vars` set besides, and the envelope it printed.
fn doctor(home: &Path, config_dir: Option<&str>, vars: &[(&str, &Path)]) -> serde_json::Value {
    let out = command(home, config_dir, vars)
        .output()
        .expect("run Pitboard");
    let envelope: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("doctor --json should be valid JSON");
    assert_eq!(envelope["v"], 1, "the contract version must be present");
    assert_eq!(envelope["command"], "doctor");
    envelope
}

fn environment(home: &Path, config_dir: Option<&str>) -> serde_json::Value {
    doctor(home, config_dir, &[])["data"]["environment"].clone()
}

/// Doctor reads Claude Desktop's data folder and app too, so they are pointed into the
/// scratch home as well, whatever the environment running the suite says: neither exists
/// there, so the person's own Claude is never read.
#[test]
fn doctor_reads_no_real_claude_desktop() {
    let home = scratch("desktop");
    let command = command(&home, None, &[]);
    for name in ["PITBOARD_CLAUDE_DESKTOP_DIR", "PITBOARD_CLAUDE_DESKTOP_APP"] {
        let set = command
            .get_envs()
            .find(|(k, _)| *k == name)
            .and_then(|(_, v)| v)
            .unwrap_or_else(|| panic!("{name} is not pointed anywhere"));
        assert!(
            Path::new(set).starts_with(&home),
            "{name} is {set:?}, outside the scratch home"
        );
    }
    let _ = std::fs::remove_dir_all(&home);
}

fn scratch(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("pitboard-env-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    std::fs::write(p.join(".claude.json"), "{}").unwrap();
    p
}

#[test]
fn an_empty_config_dir_means_unset() {
    let home = scratch("empty");
    let unset = environment(&home, None);
    let empty = environment(&home, Some(""));

    assert_eq!(
        empty["config_file"], unset["config_file"],
        "an empty CLAUDE_CONFIG_DIR must resolve exactly as an unset one"
    );
    assert_eq!(
        empty["credential_service"], "Claude Code-credentials",
        "and must leave Pitboard on the default credential slot"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// `PITBOARD_CLAUDE` and `PITBOARD_CODEX` name the program the command line runs for each
/// tool, as they name the app's, wherever its `PATH` would find another or none. Neither is
/// ever run: doctor reads which build each is off the path it resolves to, which is where
/// each tool's installer puts the version.
#[test]
fn a_program_the_environment_names_is_the_one_the_command_line_runs() {
    use std::os::unix::fs::PermissionsExt;
    let home = scratch("named");
    let claude = home.join("elsewhere/claude/versions/9.9.9");
    let codex = home.join("elsewhere/codex/releases/9.9.8-aarch64-apple-darwin/bin/codex");
    for program in [&claude, &codex] {
        std::fs::create_dir_all(program.parent().expect("its directory")).unwrap();
        std::fs::write(program, "#!/bin/sh\nexit 64\n").unwrap();
        std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let named = doctor(
        &home,
        None,
        &[("PITBOARD_CLAUDE", &claude), ("PITBOARD_CODEX", &codex)],
    );
    assert_eq!(
        named["data"]["environment"]["codex"]["version"], "9.9.8",
        "{named}"
    );
    let claude_build = named["data"]["checks"]
        .as_array()
        .expect("a list of checks")
        .iter()
        .find(|check| check["code"] == "claude_version")
        .expect("a check of Claude Code's build");
    assert!(
        claude_build["detail"]
            .as_str()
            .is_some_and(|detail| detail.starts_with("9.9.9 installed")),
        "{claude_build}"
    );

    let empty = doctor(
        &home,
        None,
        &[
            ("PITBOARD_CLAUDE", Path::new("")),
            ("PITBOARD_CODEX", Path::new("")),
        ],
    );
    assert_eq!(
        empty["data"]["environment"]["codex"]["version"],
        serde_json::Value::Null,
        "empty names nothing, so PATH is looked on, and nothing is there: {empty}"
    );
    let _ = std::fs::remove_dir_all(&home);
}
