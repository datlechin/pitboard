//! A tool's own headless run, which starts an account's period: the program, what it is
//! given, how long it may take, and what it came to.
//!
//! The run is given the login's access token on file descriptor 3, or on Windows in a variable,
//! and nothing else of it, so it can spend no refresh token a park or the tool itself still
//! holds. Its environment is built from nothing: where programs are, who the person is, the
//! proxy Pitboard was given, and a home of its own under Pitboard's directory, so no variable
//! that signs the tool in some other way reaches it (`settings::OVERRIDING_ENV`), and nothing
//! of the person's own configuration, history or sessions is read or written.

use crate::context::Context;
use crate::host::{Input, Os};
use crate::permit::Permit;
use crate::provider::ProviderId;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// How long a run is given before it is stopped. The measured runs took 7 to 15 seconds.
const CEILING: Duration = Duration::from_secs(120);

// A run about to start: the command, and the access token on descriptor 3, where one is handed.
pub(crate) struct Headless {
    pub(crate) command: Command,
    pub(crate) token: Option<String>,
}

pub(crate) use crate::host::Ran;

// Under Pitboard's directory; `prepare` makes its `home`, `tmp`, `work` and `config` folders.
pub(crate) fn home(ctx: &Context, tool: ProviderId) -> PathBuf {
    crate::home::dir(ctx).join("warm").join(tool.code())
}

/// Makes `home` afresh, each folder private to this user. Nothing one run left is there for
/// the next, which may be another account's: what a tool keeps of its own, such as the
/// policies and profile it fetches with a login, is that login's. Every run was measured
/// this way, from an empty home.
pub(crate) fn prepare(permit: Permit, home: &Path) -> std::io::Result<()> {
    if home.exists() {
        crate::home::remove_all(permit, home)?;
    }
    if let Some(parent) = home.parent().filter(|parent| !parent.is_dir()) {
        crate::host::fs::create_private_dir(permit, parent)?;
    }
    for dir in [
        home.to_path_buf(),
        home.join("home"),
        home.join("tmp"),
        home.join("work"),
        home.join("config"),
    ] {
        crate::host::fs::create_private_dir(permit, &dir)?;
    }
    Ok(())
}

/// Removes `home` once a run is over, with whatever the tool kept in it of the login it ran on.
pub(crate) fn clear(permit: Permit, home: &Path) {
    let _ = crate::home::remove_all(permit, home);
}

// Of this process's environment, only `PATH`, the person's name, the proxy and `SystemRoot`.
pub(crate) fn command(
    ctx: &Context,
    permit: Permit,
    tool: ProviderId,
    home: &Path,
) -> std::io::Result<Command> {
    let mut command = crate::provider::command(ctx, permit, tool)?;
    let path: Option<OsString> = command
        .get_envs()
        .find(|(name, _)| *name == "PATH")
        .and_then(|(_, value)| value.map(ToOwned::to_owned));
    command.env_clear();
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command
        .env("HOME", home.join("home"))
        .env("TMPDIR", home.join("tmp"))
        .env("LANG", "en_US.UTF-8")
        .env("TERM", "dumb")
        .env("NO_COLOR", "1")
        .current_dir(home.join("work"));
    if ctx.os == Os::Windows {
        // Bun takes its home from `USERPROFILE` there, and Winsock its providers from `SystemRoot`.
        command
            .env("USERPROFILE", home.join("home"))
            .env("TEMP", home.join("tmp"))
            .env("TMP", home.join("tmp"));
        if let Some(root) = &ctx.system_root {
            command.env("SystemRoot", root);
        }
    }
    if let Some(user) = &ctx.user {
        command.env(ctx.os.user_variable(), user);
    }
    for (name, value) in ctx.proxy.variables() {
        command.env(name, value);
    }
    Ok(command)
}

/// What a test or a fixture plays a run as, in place of starting a program: given what the run
/// would have been started as, what it came to.
#[cfg(any(test, feature = "test-support"))]
pub trait WarmScript: Send + Sync + std::fmt::Debug {
    fn run(&self, started: &Started) -> Ran;
}

/// What a run was started as, for a [`WarmScript`]: the program, its arguments, its whole
/// environment, the folder it runs in, and what it was handed on descriptor 3.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub dir: Option<PathBuf>,
    pub token: Option<String>,
}

#[cfg(any(test, feature = "test-support"))]
impl Started {
    // On descriptor 3, or on Windows in `CLAUDE_CODE_OAUTH_TOKEN` (`windows_token_env`).
    pub fn access_token(&self) -> Option<&str> {
        self.token.as_deref().or_else(|| {
            self.env
                .iter()
                .find(|(name, _)| name == "CLAUDE_CODE_OAUTH_TOKEN")
                .map(|(_, token)| token.as_str())
        })
    }
}

/// Runs `headless` for at most [`CEILING`], or plays it where a test or a fixture said to.
// Starting the tool asks for the permit, as a sign-in's `Provider::sign_in` does.
pub(crate) fn run(ctx: &Context, _: Permit, headless: Headless) -> std::io::Result<Ran> {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(script) = ctx.warm_script() {
        let command = &headless.command;
        let text = |s: &std::ffi::OsStr| s.to_string_lossy().into_owned();
        let started = Started {
            program: PathBuf::from(command.get_program()),
            args: command.get_args().map(text).collect(),
            env: command
                .get_envs()
                .filter_map(|(name, value)| Some((text(name), text(value?))))
                .collect(),
            dir: command.get_current_dir().map(Path::to_path_buf),
            token: headless.token.clone(),
        };
        return Ok(script.run(&started));
    }
    #[cfg(not(any(test, feature = "test-support")))]
    let _ = ctx;
    let Headless { command, token } = headless;
    let input = token
        .as_deref()
        .map_or(Input::Nothing, |token| Input::Descriptor3(token.as_bytes()));
    match crate::host::proc::run_within(command, input, CEILING) {
        Ok(out) => Ok(Ran {
            exit: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            timed_out: false,
        }),
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(Ran {
            timed_out: true,
            ..Ran::default()
        }),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    #[test]
    fn a_home_is_made_afresh_for_each_run() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path().join("dir");
        let home = dir.join("warm").join("claude");
        prepare(Permit::for_a_test(), &home).expect("made");
        std::fs::write(home.join("home").join("left.json"), "{}").expect("written");
        prepare(Permit::for_a_test(), &home).expect("made again");
        assert!(!home.join("home").join("left.json").exists());
        for sub in ["home", "tmp", "work", "config"] {
            assert!(home.join(sub).is_dir(), "{sub}");
            assert!(
                crate::host::fs::testing::is_private(&home.join(sub)).expect("read"),
                "{sub}"
            );
        }
        clear(Permit::for_a_test(), &home);
        assert!(!home.exists());
    }
}
