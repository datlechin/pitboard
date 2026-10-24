//! Proves the environment is read the way Claude Code reads it, by the binary itself: the
//! combinations are unit-tested in `provider/claude/paths.rs`, and this checks that an
//! empty `CLAUDE_CONFIG_DIR`, which Claude Code takes as the folder it runs in, is refused
//! by the command line itself.

use crate::common;
use pitboard_core::host::{OS, Os};
use pitboard_core::testing::stand_in::{self, Script};
use std::path::{Path, PathBuf};
use std::process::Command;

/// `pitboard doctor --json`, with `vars` set besides, and the envelope it printed.
fn doctor(home: &Path, config_dir: Option<&str>, vars: &[(&str, &Path)]) -> serde_json::Value {
    let (envelope, _) = pitboard(&["doctor"], home, config_dir, vars);
    assert_eq!(envelope["command"], "doctor");
    envelope
}

/// `pitboard <args> --json`, with `vars` set besides, and the envelope it printed and the
/// status it exited with.
fn pitboard(
    args: &[&str],
    home: &Path,
    config_dir: Option<&str>,
    vars: &[(&str, &Path)],
) -> (serde_json::Value, Option<i32>) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pitboard"));
    // Nothing real is read, and nothing Pitboard reads is taken from whoever runs the tests.
    // The slot under test is the default one, so the keychain account is a name nobody has
    // and the lookup finds no item; Codex gets a home of its own; and PATH holds only the
    // system's directories, so no installed `claude` or `codex` is resolved either.
    for name in pitboard_core::testing::variables() {
        command.env_remove(name);
    }
    let account = home.parent().expect("the scratch folder").join("account");
    command
        .args(args)
        .arg("--json")
        .current_dir(home)
        .envs(common::os::account_folders(&account))
        .env("HOME", home)
        .env("USER", "pitboard-test-nobody")
        .env("PITBOARD_HOME", home.join("pitboard"))
        .env("CODEX_HOME", home.join("codex"))
        .env(
            "PATH",
            std::env::join_paths(common::os::system_folders()).expect("a search path"),
        );
    if let Some(v) = config_dir {
        command.env("CLAUDE_CONFIG_DIR", v);
    }
    for (name, value) in vars {
        command.env(name, value);
    }
    let out = command.output().expect("run Pitboard");
    let envelope: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("--json should be valid JSON");
    assert_eq!(envelope["v"], 1, "the contract version must be present");
    (envelope, out.status.code())
}

fn environment(home: &Path, config_dir: Option<&str>) -> serde_json::Value {
    doctor(home, config_dir, &[])["data"]["environment"].clone()
}

fn scratch() -> (tempfile::TempDir, PathBuf) {
    let scratch = tempfile::tempdir().unwrap();
    common::refuse_a_real_login_place(scratch.path());
    let home = scratch.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    common::os::make_account_folders(&scratch.path().join("account"));
    std::fs::write(home.join(".claude.json"), "{}").unwrap();
    (scratch, home)
}

/// Claude Code 2.1.289 reads an empty `CLAUDE_CONFIG_DIR` as unset for its config file and
/// its credential slot, but as the empty path for its config dir, so it keeps its settings,
/// the lock around its login and, where there is no keychain, the login itself in whatever
/// folder it runs in. No one folder holds that login. Pitboard read an empty one as unset,
/// and so took the lock and, on Linux, switched the login in `~/.claude`, where no Claude
/// Code started with it looks. It refuses one now as a home that is not a full path: every
/// command says so, and `doctor` fails its `homes` check and checks nothing else.
#[test]
fn an_empty_config_dir_is_refused() {
    let (_scratch, home) = scratch();
    let unset = environment(&home, None);
    assert_eq!(unset["credential_service"], "Claude Code-credentials");

    let empty = doctor(&home, Some(""), &[]);
    let checks = empty["data"]["checks"].as_array().expect("its checks");
    assert_eq!(checks.len(), 1, "nothing else is checked: {empty}");
    assert_eq!(checks[0]["code"], "homes");
    assert_eq!(checks[0]["level"], "fail");
    assert_eq!(checks[0]["detail"], "CLAUDE_CONFIG_DIR is empty");

    for args in [&["status"][..], &["status", "--offline"], &["use", "work"]] {
        let (refused, code) = pitboard(args, &home, Some(""), &[]);
        assert_eq!(code, Some(1), "{args:?}: {refused}");
        assert_eq!(refused["error"]["code"], "home_not_absolute", "{args:?}");
        assert!(
            refused["error"]["message"]
                .as_str()
                .is_some_and(|said| said.starts_with("CLAUDE_CONFIG_DIR is empty, ")),
            "{refused}"
        );
    }
    let left: Vec<_> = std::fs::read_dir(&home)
        .expect("the scratch home")
        .map(|entry| entry.expect("an entry").file_name())
        .collect();
    assert_eq!(left, [".claude.json"], "nothing is written");
}

// Never run: doctor reads each build off its path, or on Windows off npm's package beside
// the program `claude.cmd` starts, since the stand-in has no version resource.
#[test]
fn a_program_the_environment_names_is_the_one_the_command_line_runs() {
    let (_scratch, home) = scratch();
    let put = |program: &Path| {
        std::fs::create_dir_all(program.parent().expect("its directory")).unwrap();
        let never = Script::refusing("a test's stand-in, never meant to run\n");
        stand_in::install(program, &never)
            .unwrap_or_else(|e| panic!("no stand-in at {}: {e}", program.display()));
    };
    let elsewhere = home.join("elsewhere");
    let (claude, codex) = match OS {
        Os::MacOs | Os::Linux => {
            let claude = elsewhere.join("claude/versions/9.9.9");
            let codex = elsewhere.join("codex/releases/9.9.8-aarch64-apple-darwin/bin/codex");
            put(&claude);
            put(&codex);
            (claude, codex)
        }
        Os::Windows => {
            let npm = elsewhere.join("npm");
            let package = npm.join(r"node_modules\@anthropic-ai\claude-code");
            put(&package.join(r"bin\claude.exe"));
            std::fs::write(
                package.join("package.json"),
                r#"{"name": "@anthropic-ai/claude-code", "version": "9.9.9"}"#,
            )
            .unwrap();
            let shim = npm.join("claude.cmd");
            std::fs::write(
                &shim,
                "@ECHO off\r\n\"%dp0%\\node_modules\\@anthropic-ai\\claude-code\\bin\\claude.exe\"   %*\r\n",
            )
            .unwrap();
            let codex =
                elsewhere.join(r"codex\releases\9.9.8-x86_64-pc-windows-msvc\bin\codex.exe");
            put(&codex);
            (shim, codex)
        }
    };

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
}

// Codex 0.163.0's Linux build takes `WSL_DISTRO_NAME` for WSL, and Linux's doctor does too.
#[test]
fn doctor_in_wsl_says_what_pitboard_switches_there() {
    let (_scratch, home) = scratch();
    let wsl = |envelope: serde_json::Value| {
        envelope["data"]["checks"]
            .as_array()
            .expect("a list of checks")
            .iter()
            .find(|check| check["code"] == "wsl")
            .cloned()
    };
    assert_eq!(wsl(doctor(&home, None, &[])), None);
    let inside = wsl(doctor(
        &home,
        None,
        &[("WSL_DISTRO_NAME", Path::new("Ubuntu"))],
    ));
    match OS {
        Os::Linux => {
            let inside = inside.expect("a wsl check");
            assert_eq!(inside["level"], "ok");
            assert_eq!(inside["detail"], "inside WSL, in Ubuntu");
            assert_eq!(
                inside["advice"],
                "Pitboard here switches what is installed in this Linux. Claude Code and Codex \
                 installed on Windows keep their own logins, which Pitboard for Windows switches."
            );
        }
        Os::MacOs | Os::Windows => assert_eq!(inside, None),
    }
}
