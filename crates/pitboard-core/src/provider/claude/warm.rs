//! Claude Code's own headless run that starts an account's five-hour period, and what its
//! result says.

use super::engine::access_token;
use crate::context::Context;
use crate::duty::warm::Failure;
use crate::duty::warm::run::{self, Headless, Ran};
use crate::host::Os;
use crate::provider::{ProviderError, ProviderId};
use crate::service::Permit;
use serde_json::Value;
use std::path::Path;

/// Print mode on the smallest model, with no tools, no MCP servers, no slash commands and no
/// session kept, answering in JSON (the register's `headless_print_mode`). The access token
/// goes on descriptor 3, which `CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR` names, and is used with
/// no refresh token and no expiry and stored nowhere (`headless_token_descriptor`). Never on
/// standard input: print mode adds what it reads there to the prompt. `CLAUDE_CONFIG_DIR` is
/// the private home's, so its config, its keychain slot and what it fetches are its own. It
/// sends a request on the account's plan, so it is made only with the [`Permit`].
pub(crate) fn headless(
    ctx: &Context,
    permit: Permit,
    home: &Path,
    login: &Value,
) -> Result<Headless, ProviderError> {
    let token = access_token(login).ok_or_else(|| ProviderError::ShapeUnexpected {
        provider: ProviderId::Claude,
        detail: "it has no claudeAiOauth.accessToken".into(),
    })?;
    let mut command = run::command(ctx, permit, ProviderId::Claude, home).map_err(|_| {
        ProviderError::Unsupported {
            provider: ProviderId::Claude,
            reason: "Claude Code is not installed where Pitboard looks".into(),
        }
    })?;
    command
        .args([
            "-p",
            "Reply with the single word OK.",
            "--model",
            "haiku",
            "--tools",
            "",
            "--no-session-persistence",
            "--strict-mcp-config",
            "--disable-slash-commands",
            "--output-format",
            "json",
        ])
        .env("CLAUDE_CONFIG_DIR", home.join("config"))
        .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
        .env("DISABLE_AUTOUPDATER", "1");
    let token = match ctx.os {
        Os::MacOs | Os::Linux => {
            command.env("CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR", "3");
            Some(token.to_owned())
        }
        // It reads no descriptor there (`windows_token_handover`), and this ahead of any login.
        Os::Windows => {
            command
                .env("CLAUDE_CODE_OAUTH_TOKEN", token)
                .env("CLAUDE_CODE_SUBPROCESS_ENV_SCRUB", "1");
            None
        }
    };
    Ok(Headless { command, token })
}

/// Print mode's JSON result (`headless_result`): `is_error`, with `api_error_status` and
/// `api_error` where the service refused the request, and `result`, the answer or what went
/// wrong in Claude Code's words. `Ok` where the request was answered.
pub(crate) fn verdict(ran: &Ran) -> Result<(), Failure> {
    let Some(result) = ran
        .stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .filter(Value::is_object)
    else {
        return Err(Failure::Failed("Claude Code printed no result".into()));
    };
    let failed = result.get("is_error").and_then(Value::as_bool) != Some(false);
    if !failed && ran.exit == Some(0) {
        return Ok(());
    }
    let said = result
        .get("result")
        .and_then(Value::as_str)
        .unwrap_or("Claude Code failed")
        .to_owned();
    match (
        result.get("api_error_status").and_then(Value::as_u64),
        result.get("api_error").and_then(Value::as_str),
    ) {
        (Some(429), Some("usage_limit_reached")) => Err(Failure::LimitReached),
        (Some(401 | 403), _) => Err(Failure::LoginRefused),
        (Some(429 | 500..=599), _) => Err(Failure::Unreachable),
        _ => Err(Failure::Failed(said)),
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn windows_hands_the_token_in_claude_codes_variable_and_every_other_system_on_descriptor_3() {
        let dir = tempfile::tempdir().expect("a scratch folder");
        let program = dir.path().join("bin").join("claude.exe");
        std::fs::create_dir_all(program.parent().expect("a folder")).expect("bin");
        std::fs::write(&program, "never run").expect("a program");
        crate::host::fs::testing::make_runnable(&program).expect("runnable");
        let home = dir.path().join("warm").join("claude");
        let bash = dir.path().join("Git").join("bin").join("bash.exe");
        std::fs::create_dir_all(bash.parent().expect("a folder")).expect("Git");
        std::fs::write(&bash, "never run").expect("a Git Bash");
        let login = serde_json::json!({
            "claudeAiOauth": {"accessToken": "access-work", "refreshToken": "refresh-work"}
        });
        for os in [Os::MacOs, Os::Linux, Os::Windows] {
            let mut ctx = Context::new(dir.path().to_path_buf())
                .with_claude_program(program.clone())
                .with_user("dana".into());
            ctx.os = os;
            ctx.system_root = Some(PathBuf::from(r"C:\Windows"));
            ctx.git_bash = Some(bash.clone());
            let run = headless(&ctx, Permit::for_a_test(), &home, &login).expect("a run");
            let env: BTreeMap<String, String> = run
                .command
                .get_envs()
                .filter_map(|(name, value)| {
                    Some((
                        name.to_string_lossy().into_owned(),
                        value?.to_string_lossy().into_owned(),
                    ))
                })
                .collect();
            let names: Vec<&str> = env.keys().map(String::as_str).collect();
            let path = |under: &str| home.join(under).to_string_lossy().into_owned();
            assert_eq!(env["CLAUDE_CONFIG_DIR"], path("config"), "{os:?}");
            assert_eq!(env["HOME"], path("home"), "{os:?}");
            assert!(
                !env.values().any(|value| value.contains("refresh-work")),
                "{os:?}: no refresh token is handed on"
            );
            match os {
                Os::MacOs | Os::Linux => {
                    assert_eq!(run.token.as_deref(), Some("access-work"), "{os:?}");
                    assert_eq!(
                        names,
                        [
                            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
                            "CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR",
                            "CLAUDE_CONFIG_DIR",
                            "DISABLE_AUTOUPDATER",
                            "HOME",
                            "LANG",
                            "NO_COLOR",
                            "PATH",
                            "TERM",
                            "TMPDIR",
                            "USER",
                        ],
                        "{os:?}"
                    );
                    assert_eq!(env["CLAUDE_CODE_OAUTH_TOKEN_FILE_DESCRIPTOR"], "3");
                }
                Os::Windows => {
                    assert_eq!(run.token, None, "nothing on a descriptor");
                    assert_eq!(
                        names,
                        [
                            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
                            "CLAUDE_CODE_GIT_BASH_PATH",
                            "CLAUDE_CODE_OAUTH_TOKEN",
                            "CLAUDE_CODE_SUBPROCESS_ENV_SCRUB",
                            "CLAUDE_CONFIG_DIR",
                            "DISABLE_AUTOUPDATER",
                            "HOME",
                            "LANG",
                            "NO_COLOR",
                            "PATH",
                            "SystemRoot",
                            "TEMP",
                            "TERM",
                            "TMP",
                            "TMPDIR",
                            "USERNAME",
                            "USERPROFILE",
                        ]
                    );
                    assert_eq!(env["CLAUDE_CODE_OAUTH_TOKEN"], "access-work");
                    assert_eq!(env["CLAUDE_CODE_SUBPROCESS_ENV_SCRUB"], "1");
                    assert_eq!(
                        env["CLAUDE_CODE_GIT_BASH_PATH"],
                        bash.to_string_lossy(),
                        "so Claude Code asks no where.exe, which would get the token"
                    );
                    assert_eq!(env["USERPROFILE"], path("home"));
                    assert_eq!((&env["TEMP"], &env["TMP"]), (&path("tmp"), &path("tmp")));
                    assert_eq!(env["SystemRoot"], r"C:\Windows");
                    assert_eq!(env["USERNAME"], "dana");
                }
            }
        }
    }

    fn ran(exit: i32, stdout: &str) -> Ran {
        Ran {
            exit: Some(exit),
            stdout: stdout.into(),
            timed_out: false,
        }
    }

    #[test]
    fn each_result_print_mode_gives_is_read_as_what_it_came_to() {
        let cases = [
            (0, r#"{"is_error":false,"result":"OK"}"#, Ok(())),
            (
                1,
                r#"{"is_error":true,"api_error_status":429,"api_error":"usage_limit_reached","result":"limit"}"#,
                Err(Failure::LimitReached),
            ),
            (
                1,
                r#"{"is_error":true,"api_error_status":429,"api_error":"rate_limit_error","result":"slow down"}"#,
                Err(Failure::Unreachable),
            ),
            (
                1,
                r#"{"is_error":true,"api_error_status":401,"result":"Invalid API key"}"#,
                Err(Failure::LoginRefused),
            ),
            (
                1,
                r#"{"is_error":true,"api_error_status":403,"result":"forbidden"}"#,
                Err(Failure::LoginRefused),
            ),
            (
                1,
                r#"{"is_error":true,"api_error_status":529,"result":"Overloaded"}"#,
                Err(Failure::Unreachable),
            ),
            (
                1,
                r#"{"is_error":true,"result":"something else"}"#,
                Err(Failure::Failed("something else".into())),
            ),
            (
                1,
                r#"{"is_error":false,"result":"OK"}"#,
                Err(Failure::Failed("OK".into())),
            ),
            (
                0,
                "a warning first\n{\"is_error\":false,\"result\":\"OK\"}\n",
                Ok(()),
            ),
            (
                0,
                "",
                Err(Failure::Failed("Claude Code printed no result".into())),
            ),
            (
                0,
                "[1,2]",
                Err(Failure::Failed("Claude Code printed no result".into())),
            ),
        ];
        for (exit, stdout, expected) in cases {
            assert_eq!(verdict(&ran(exit, stdout)), expected, "{stdout}");
        }
    }
}
