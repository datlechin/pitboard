//! The `PATH` the person's own terminal has, asked of their login shell, the POSIX way.
//!
//! The system starts an app with its own directories on `PATH` and nothing else, and a tool
//! installed through a version manager or an npm prefix is found only on the `PATH` a login
//! shell builds from its startup files. So an app asks one, the way editors on macOS do: the
//! person's shell, as a login shell and an interactive one, runs its startup files and prints
//! `PATH` between two markers, so that whatever those print is not taken for the answer.
//!
//! The command line never asks: it was started from a shell, and has its `PATH` already.

use crate::context::Environment;
use crate::host::LoginPath;
use std::ffi::CString;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long the shell has to answer. Startup files that load a version manager take a second
/// or so; past this the shell is stopped and the answer is unknown.
pub(crate) const PATIENCE: Duration = Duration::from_secs(5);

/// What running a shell came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ran {
    /// It printed this, and exited by itself with success.
    Said(String),
    /// It could not be started, or exited with anything but success. Asking again would get
    /// the same.
    Failed,
    /// It had not finished in time, and was stopped with everything it started.
    Late,
}

/// How a system keeps the rest of this process's descriptors from the shell, but the three it
/// is handed: what `posix_spawn` is told, and a file action that closes every descriptor from 3
/// on, where the system has one. Each system's face says which (`SPAWN`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Spawn {
    pub(crate) flags: libc::c_short,
    // Added after the three are set up, since it closes the pipe they are copied from too.
    pub(crate) close_the_rest: Option<fn(&mut Actions) -> bool>,
}

/// The login shell's `PATH`, asked once, with the shell started as `spawn` says.
pub(crate) fn login_path(env: &Environment, spawn: Spawn) -> LoginPath {
    let marker = marker();
    path(env, &marker, |shell, arguments| {
        run(shell, arguments, env, PATIENCE, spawn)
    })
}

/// `login_path` with the shell run by `run`, so a test can say what a shell printed without
/// starting one.
pub(crate) fn path(
    env: &Environment,
    marker: &str,
    run: impl FnOnce(&Path, &[String]) -> Ran,
) -> LoginPath {
    let Some(shell) = shell(env) else {
        return LoginPath::Unknown;
    };
    match run(&shell, &arguments(marker)) {
        Ran::Said(output) => path_in(&output, marker).map_or(LoginPath::Unknown, LoginPath::Said),
        Ran::Failed => LoginPath::Unknown,
        Ran::Late => LoginPath::Late,
    }
}

/// A marker no startup file prints: this process and the moment it asked.
fn marker() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("pitboard-path-{}-{nanos:x}", std::process::id())
}

/// What the shell is asked to run. `printenv` by its full path, because a startup file can
/// define a function by that name, and not `echo $PATH`, which fish prints as a list. All
/// three of zsh, bash and fish take the flags and the command as written.
pub(crate) fn arguments(marker: &str) -> Vec<String> {
    vec![
        "-l".into(),
        "-i".into(),
        "-c".into(),
        format!("echo {marker}; /usr/bin/printenv PATH; echo {marker}"),
    ]
}

/// The shell to ask: `$SHELL`, which an app the system started is given, or else the one
/// the account names.
pub(crate) fn shell(env: &Environment) -> Option<PathBuf> {
    env.path("SHELL")
        .filter(|shell| !shell.is_empty())
        .map(PathBuf::from)
        .or_else(super::user::login_shell)
}

/// What stands between the two markers in a shell's output, which is all of it that is the
/// answer. `None` when either marker is missing or nothing is between them.
pub(crate) fn path_in(output: &str, marker: &str) -> Option<String> {
    let (_, after) = output.split_once(marker)?;
    let (between, _) = after.split_once(marker)?;
    // Foundation's newlines, which is what the app trimmed when it asked for itself.
    let newline = |c: char| {
        matches!(c, '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}')
            || matches!(c, '\u{2028}' | '\u{2029}')
    };
    let path = between.trim_matches(newline);
    (!path.is_empty()).then(|| path.to_owned())
}

/// What `shell` printed, run with `arguments` in `env`, unless it could not start, exited
/// with anything but success, or had not finished within `limit`, when it is stopped along
/// with everything it started.
///
/// It gets a session of its own, so an interactive shell finds no terminal to take over and
/// everything it starts is in one process group to stop. Its input is `/dev/null`, its
/// errors are discarded, and `how` keeps this process's other descriptors from it where the
/// system can.
pub(crate) fn run(
    shell: &Path,
    arguments: &[String],
    env: &Environment,
    limit: Duration,
    how: Spawn,
) -> Ran {
    let Ok((mut reading, writing)) = io::pipe() else {
        return Ran::Failed;
    };
    let Some(pid) = spawn(shell, arguments, env, writing.as_raw_fd(), how) else {
        return Ran::Failed;
    };
    // Only the shell writes to it now, so it ends when the shell and whatever it started
    // let go of it.
    drop(writing);

    let deadline = Instant::now() + limit;
    let mut output = Vec::new();
    let mut ended = false;
    let mut status: libc::c_int = 0;
    loop {
        // SAFETY: waitpid writes the status of this process's own child into `status`, which
        // lives for the call.
        let reaped = unsafe { libc::waitpid(pid, &raw mut status, libc::WNOHANG) };
        if reaped == pid {
            break;
        }
        if reaped < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            return Ran::Failed;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            stop(pid);
            return Ran::Late;
        }
        if ended {
            std::thread::sleep(Duration::from_millis(10));
        } else if readable(&reading, left.min(Duration::from_millis(50))) {
            ended = !read_some(&mut reading, &mut output);
        }
    }
    // What it printed before it exited is in the pipe already. Something its startup files
    // left running may still hold the pipe open, so this takes what is there and does not
    // wait for more.
    while !ended && Instant::now() < deadline && readable(&reading, Duration::ZERO) {
        ended = !read_some(&mut reading, &mut output);
    }
    if !(libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0) {
        return Ran::Failed;
    }
    Ran::Said(String::from_utf8_lossy(&output).into_owned())
}

/// Starts `shell` with `arguments` in `env`, writing to `output`, in a session of its own.
/// Its pid, or `None` where it could not be started.
fn spawn(
    shell: &Path,
    arguments: &[String],
    env: &Environment,
    output: libc::c_int,
    how: Spawn,
) -> Option<libc::pid_t> {
    let program = CString::new(shell.as_os_str().as_bytes()).ok()?;
    let mut argv = vec![program.clone()];
    for argument in arguments {
        argv.push(CString::new(argument.as_str()).ok()?);
    }
    // A variable that cannot be handed on, one with a NUL in it, is left out rather than
    // handed on changed.
    let envp: Vec<CString> = env
        .iter()
        .filter_map(|(name, value)| {
            let mut pair = name.as_bytes().to_vec();
            pair.push(b'=');
            pair.extend_from_slice(value.as_bytes());
            CString::new(pair).ok()
        })
        .collect();
    let pointers = |strings: &[CString]| -> Vec<*mut libc::c_char> {
        strings
            .iter()
            .map(|s| s.as_ptr().cast_mut())
            .chain(std::iter::once(std::ptr::null_mut()))
            .collect()
    };
    let (argv, envp) = (pointers(&argv), pointers(&envp));
    let null = c"/dev/null";

    let mut actions = Actions::new()?;
    let mut attributes = Attributes::new()?;
    // SAFETY: both are initialised and stay where they are, boxed, until they are destroyed
    // when dropped; `/dev/null` is a NUL-terminated literal.
    let prepared = unsafe {
        libc::posix_spawn_file_actions_addopen(actions.raw(), 0, null.as_ptr(), libc::O_RDONLY, 0)
            == 0
            && libc::posix_spawn_file_actions_adddup2(actions.raw(), output, 1) == 0
            && libc::posix_spawn_file_actions_addopen(
                actions.raw(),
                2,
                null.as_ptr(),
                libc::O_WRONLY,
                0,
            ) == 0
            && libc::posix_spawnattr_setflags(attributes.raw(), how.flags) == 0
    };
    if !prepared || !how.close_the_rest.is_none_or(|close| close(&mut actions)) {
        return None;
    }
    let mut pid: libc::pid_t = 0;
    // SAFETY: every pointer is to something alive for the call: the program and the
    // NULL-terminated argument and environment lists point into `program`, `argv` and
    // `envp`'s strings, and the actions and attributes are initialised.
    let started = unsafe {
        libc::posix_spawn(
            &raw mut pid,
            program.as_ptr(),
            actions.raw(),
            attributes.raw(),
            argv.as_ptr(),
            envp.as_ptr(),
        )
    };
    (started == 0).then_some(pid)
}

/// `posix_spawn`'s file actions, boxed so they never move once initialised, and destroyed
/// when dropped. Only ones whose initialisation succeeded are ever made.
pub(crate) struct Actions(Box<std::mem::MaybeUninit<libc::posix_spawn_file_actions_t>>);

impl Actions {
    fn new() -> Option<Actions> {
        let mut raw = Box::new(std::mem::MaybeUninit::uninit());
        // SAFETY: initialises the actions in place, in memory this function owns.
        let made = unsafe { libc::posix_spawn_file_actions_init(raw.as_mut_ptr()) };
        (made == 0).then(|| Actions(raw))
    }

    pub(crate) fn raw(&mut self) -> *mut libc::posix_spawn_file_actions_t {
        self.0.as_mut_ptr()
    }
}

impl Drop for Actions {
    fn drop(&mut self) {
        // SAFETY: initialised when it was made, and destroyed once, here.
        unsafe { libc::posix_spawn_file_actions_destroy(self.raw()) };
    }
}

/// `posix_spawn`'s attributes, boxed, made and destroyed the same way.
struct Attributes(Box<std::mem::MaybeUninit<libc::posix_spawnattr_t>>);

impl Attributes {
    fn new() -> Option<Attributes> {
        let mut raw = Box::new(std::mem::MaybeUninit::uninit());
        // SAFETY: initialises the attributes in place, in memory this function owns.
        let made = unsafe { libc::posix_spawnattr_init(raw.as_mut_ptr()) };
        (made == 0).then(|| Attributes(raw))
    }

    fn raw(&mut self) -> *mut libc::posix_spawnattr_t {
        self.0.as_mut_ptr()
    }
}

impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: initialised when it was made, and destroyed once, here.
        unsafe { libc::posix_spawnattr_destroy(self.raw()) };
    }
}

/// Whether `pipe` has something to read, or has ended, within `wait`.
fn readable(pipe: &io::PipeReader, wait: Duration) -> bool {
    let mut ready = libc::pollfd {
        fd: pipe.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let millis = libc::c_int::try_from(wait.as_millis()).unwrap_or(libc::c_int::MAX);
    // SAFETY: poll reads and writes the one `pollfd` it is given, which lives for the call.
    unsafe { libc::poll(&raw mut ready, 1, millis) > 0 }
}

/// Reads what `pipe` has into `output`. `false` once it has ended.
fn read_some(pipe: &mut io::PipeReader, output: &mut Vec<u8>) -> bool {
    let mut buffer = [0_u8; 4096];
    match pipe.read(&mut buffer) {
        Ok(0) => false,
        Ok(count) => {
            output.extend_from_slice(&buffer[..count]);
            true
        }
        Err(e) => matches!(
            e.kind(),
            io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
        ),
    }
}

/// Stops the shell and everything it started, and reaps it.
fn stop(pid: libc::pid_t) {
    // SAFETY: signals this process's own child, and its process group, which is its session;
    // neither touches this process's memory.
    unsafe {
        // An interactive shell ignores a polite signal.
        libc::kill(-pid, libc::SIGKILL);
        libc::kill(pid, libc::SIGKILL);
    }
    let mut status: libc::c_int = 0;
    loop {
        // SAFETY: as in `run`, the status of this process's own child into a local.
        let reaped = unsafe { libc::waitpid(pid, &raw mut status, 0) };
        if reaped >= 0 || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            break;
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    const MARKER: &str = "pitboard-path-test";

    fn env(pairs: &[(&str, &str)]) -> Environment {
        pairs.iter().copied().collect()
    }

    /// How this system's host has this module start the shell.
    fn spawning() -> Spawn {
        crate::host::os::SPAWN
    }

    /// Only what stands between the markers is the answer. Startup files print greetings,
    /// warnings and prompts of their own, sometimes on the same line as the marker.
    #[test]
    fn the_path_is_what_stands_between_the_markers() {
        let said = format!(
            "Last login: never\nnvm is loaded{MARKER}\n\
             /Users/x/.nvm/bin:/usr/bin:/bin\n{MARKER}\nbye\n"
        );
        assert_eq!(
            path_in(&said, MARKER).as_deref(),
            Some("/Users/x/.nvm/bin:/usr/bin:/bin")
        );
        assert_eq!(
            path_in(&format!("{MARKER}\r\n/usr/bin\r\n{MARKER}\r\n"), MARKER).as_deref(),
            Some("/usr/bin")
        );
    }

    /// A shell that stopped early, or said nothing between them, has not answered.
    #[test]
    fn without_both_markers_there_is_no_answer() {
        assert_eq!(path_in("", MARKER), None);
        assert_eq!(path_in("/usr/bin:/bin\n", MARKER), None);
        assert_eq!(path_in(&format!("{MARKER}\n/usr/bin:/bin\n"), MARKER), None);
        assert_eq!(path_in(&format!("{MARKER}\n\n{MARKER}\n"), MARKER), None);
    }

    /// The shell is asked as a login shell and an interactive one, since zsh reads `.zshrc`,
    /// where version managers put themselves, only for an interactive shell; and `PATH` is
    /// printed by `printenv` between the markers, which all of zsh, bash and fish run as
    /// written.
    #[test]
    fn the_login_shell_is_asked_for_its_path() {
        let mut asked = None;
        let fish = env(&[("SHELL", "/opt/homebrew/bin/fish")]);
        let answer = path(&fish, MARKER, |shell, arguments| {
            asked = Some((shell.to_path_buf(), arguments.to_vec()));
            Ran::Said(format!(
                "Welcome to fish\n{MARKER}\n/opt/homebrew/bin:/usr/bin\n{MARKER}\n"
            ))
        });
        assert_eq!(answer, LoginPath::Said("/opt/homebrew/bin:/usr/bin".into()));
        let (shell, arguments) = asked.expect("a shell was asked");
        assert_eq!(shell, PathBuf::from("/opt/homebrew/bin/fish"));
        assert_eq!(
            arguments,
            [
                "-l".to_string(),
                "-i".into(),
                "-c".into(),
                format!("echo {MARKER}; /usr/bin/printenv PATH; echo {MARKER}")
            ]
        );
    }

    /// An app is normally given `SHELL`; when it is not, or it is empty, the account's own
    /// shell is asked.
    #[test]
    fn without_shell_the_accounts_own_is_asked() {
        for given in [env(&[]), env(&[("SHELL", "")])] {
            let mut asked = None;
            let _ = path(&given, MARKER, |shell, _| {
                asked = Some(shell.to_path_buf());
                Ran::Failed
            });
            let asked = asked.expect("a shell was asked");
            assert!(asked.is_absolute(), "{}", asked.display());
        }
    }

    /// A shell that could not be run has no answer, whatever it might have printed, and
    /// asking again would get the same. One too slow to answer has none yet, and says so,
    /// since a later ask may have one.
    #[test]
    fn a_shell_that_failed_has_no_answer_and_a_late_one_says_so() {
        let zsh = env(&[("SHELL", "/bin/zsh")]);
        assert_eq!(path(&zsh, MARKER, |_, _| Ran::Failed), LoginPath::Unknown);
        assert_eq!(path(&zsh, MARKER, |_, _| Ran::Late), LoginPath::Late);
        assert_eq!(
            path(&zsh, MARKER, |_, _| Ran::Said("no markers\n".into())),
            LoginPath::Unknown
        );
    }

    /// A shell of the test's own, never the person's, which runs what it is asked with a
    /// plain `/bin/sh` that reads no startup files, after `before`. `before` runs in a
    /// directory of the shell's own, which is taken away with it.
    struct FakeShell {
        dir: tempfile::TempDir,
    }

    impl FakeShell {
        fn new(before: &str) -> FakeShell {
            let dir = tempfile::tempdir().expect("a scratch folder");
            let script = format!(
                "#!/bin/sh\n[ \"$1 $2 $3\" = \"-l -i -c\" ] || exit 64\n\
                 cd \"$(/usr/bin/dirname \"$0\")\" || exit 65\n{before}\nexec /bin/sh -c \"$4\"\n"
            );
            // Written by another process, so this one never holds it open for writing while
            // a test on another thread starts a program: on Linux that child would hold a
            // copy, and running the shell would fail with ETXTBSY.
            let status = std::process::Command::new("/bin/sh")
                .args(["-c", "printf %s \"$2\" > \"$1\" && chmod 755 \"$1\"", "sh"])
                .arg(dir.path().join("shell"))
                .arg(script)
                .status()
                .expect("sh runs");
            assert!(status.success(), "the shell could not be written");
            FakeShell { dir }
        }

        fn path(&self) -> PathBuf {
            self.dir.path().join("shell")
        }

        /// The processes `before` wrote down in `pids`, one per line.
        fn pids(&self) -> Vec<libc::pid_t> {
            std::fs::read_to_string(self.dir.path().join("pids"))
                .unwrap_or_default()
                .lines()
                .filter_map(|line| line.trim().parse().ok())
                .collect()
        }
    }

    /// Whether `pid` is gone, asked for a while, since a process killed a moment ago can take
    /// a moment to be reaped. One that has exited and that nobody has reaped yet is gone too.
    fn gone(pid: libc::pid_t) -> bool {
        (0..200).any(|_| {
            let state = std::process::Command::new("ps")
                .args(["-o", "stat=", "-p", &pid.to_string()])
                .output()
                .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
                .unwrap_or_default();
            if state.is_empty() || state.starts_with('Z') {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
            false
        })
    }

    /// The whole round: the shell is started with the environment given, prints `PATH`
    /// between the markers after whatever its startup files say, and the answer is read
    /// back.
    #[test]
    fn a_shell_is_run_and_its_path_read_back() {
        let shell = FakeShell::new("echo 'Last login: never'");
        let given = shell.path();
        let answer = login_path(
            &env(&[
                ("SHELL", given.to_str().expect("a path in UTF-8")),
                ("PATH", "/from/the/test/bin:/usr/bin:/bin"),
            ]),
            spawning(),
        );
        assert_eq!(
            answer,
            LoginPath::Said("/from/the/test/bin:/usr/bin:/bin".into())
        );
    }

    // One a library in the app left open on exec, far above the three, which the shell checks
    // for itself, since a program it starts would open descriptors of its own.
    #[test]
    #[cfg_attr(
        all(target_os = "linux", not(target_env = "gnu")),
        ignore = "musl has no file action that closes the rest, and only an app, which links \
                  glibc, asks for the login shell"
    )]
    fn a_login_shell_is_handed_only_its_three_descriptors() {
        let shell = FakeShell::new(
            "if [ -e \"/dev/fd/$LEFT_OPEN\" ]; then echo handed > fds; else echo kept > fds; fi",
        );
        // SAFETY: opens `/dev/null` for this test's own use, and copies it above 63 with no
        // close-on-exec, as `F_DUPFD` copies one; both are closed again below.
        let left_open = unsafe {
            let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY);
            let copy = libc::fcntl(null, libc::F_DUPFD, 64);
            libc::close(null);
            copy
        };
        assert!(left_open >= 64, "{}", io::Error::last_os_error());
        let number = left_open.to_string();
        let ran = run(
            &shell.path(),
            &arguments(MARKER),
            &env(&[("LEFT_OPEN", number.as_str())]),
            Duration::from_secs(5),
            spawning(),
        );
        // SAFETY: closes the copy this test made, which nothing else holds.
        unsafe { libc::close(left_open) };
        assert!(matches!(ran, Ran::Said(_)), "{ran:?}");
        let said = std::fs::read_to_string(shell.dir.path().join("fds")).expect("its answer");
        assert_eq!(said.trim(), "kept");
    }

    /// A shell that fails has no answer.
    #[test]
    fn a_shell_that_fails_has_no_answer() {
        let failing = FakeShell::new("exit 3");
        assert_eq!(
            run(
                &failing.path(),
                &arguments(MARKER),
                &env(&[]),
                Duration::from_secs(5),
                spawning()
            ),
            Ran::Failed
        );
        assert_eq!(
            run(
                Path::new("/nowhere/at/all/sh"),
                &arguments(MARKER),
                &env(&[]),
                Duration::from_secs(5),
                spawning()
            ),
            Ran::Failed,
            "a shell that is not there"
        );
    }

    /// A shell that never finishes is stopped rather than waited for, and so is everything
    /// its startup files started, which would otherwise go on running for as long as it
    /// likes.
    #[test]
    fn a_shell_that_hangs_is_stopped_with_everything_it_started() {
        let hanging = FakeShell::new("echo $$ > pids; /bin/sleep 30 & echo $! >> pids; wait");
        let started = Instant::now();
        assert_eq!(
            run(
                &hanging.path(),
                &arguments(MARKER),
                &env(&[]),
                Duration::from_secs(2),
                spawning()
            ),
            Ran::Late
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "it was stopped, not waited for: {:?}",
            started.elapsed()
        );
        let pids = hanging.pids();
        assert_eq!(
            pids.len(),
            2,
            "the shell and what it started, both started before the end"
        );
        for pid in pids {
            assert!(gone(pid), "{pid} is still running");
        }
    }

    /// Something a startup file leaves running can keep the shell's output open long after
    /// the shell is done. The answer is taken once the shell exits, not when the last holder
    /// lets go, which here is well after the shell's time is up.
    #[test]
    fn a_shell_is_done_when_it_exits_even_if_something_it_started_is_not() {
        let shell = FakeShell::new("/bin/sleep 10 & echo $! > pids");
        let given = shell.path();
        let answer = login_path(
            &env(&[
                ("SHELL", given.to_str().expect("a path in UTF-8")),
                ("PATH", "/usr/bin:/bin"),
            ]),
            spawning(),
        );
        for pid in shell.pids() {
            // SAFETY: signals the stand-in's own sleep, which this test started.
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
        assert_eq!(answer, LoginPath::Said("/usr/bin:/bin".into()));
    }
}
