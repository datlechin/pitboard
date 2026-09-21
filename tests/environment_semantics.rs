//! The rules themselves are unit-tested in `claude.rs`. What can only be proved by running
//! the binary is that they are actually wired to the file it opens and the slot it reads —
//! so exactly one test lives here, and the combinatorics stay in-process.

use std::path::PathBuf;
use std::process::Command;

fn status(home: &PathBuf, config_dir: Option<&str>) -> serde_json::Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pitboard"));
    command
        .args(["status", "--json"])
        .env("HOME", home)
        .env("PITBOARD_HOME", home.join("pitboard"))
        .env_remove("CLAUDE_SECURESTORAGE_CONFIG_DIR");
    match config_dir {
        Some(v) => command.env("CLAUDE_CONFIG_DIR", v),
        None => command.env_remove("CLAUDE_CONFIG_DIR"),
    };
    let out = command.output().expect("run pitboard");
    serde_json::from_slice(&out.stdout).expect("status --json should be valid JSON")
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
    let unset = status(&home, None);
    let empty = status(&home, Some(""));

    assert_eq!(
        empty["config_file"], unset["config_file"],
        "an empty CLAUDE_CONFIG_DIR must resolve exactly as an unset one"
    );
    assert_eq!(
        empty["store"]["service"], "Claude Code-credentials",
        "and must leave pitboard on the default credential slot"
    );
    let _ = std::fs::remove_dir_all(&home);
}
