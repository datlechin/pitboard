//! Processes, the POSIX way.

use crate::host::Input;
use crate::permit::Permit;
use std::ffi::CString;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

/// How long a run past its ceiling is given to end once asked, before it is made to.
const GRACE: Duration = Duration::from_secs(2);

// `security`, `ps` and `plutil`, whose answers are read whole.
#[cfg(any(target_os = "macos", test))]
pub(in crate::host) fn run_within(
    command: Command,
    input: Input<'_>,
    ceiling: Duration,
) -> io::Result<Output> {
    run(command, Stdio::piped(), input, ceiling, u64::MAX)
}

// A warm-up's tool, started by the gate's permit: its errors unread, its first MiB kept.
pub(crate) fn run_tool(
    _: Permit,
    command: Command,
    input: Input<'_>,
    ceiling: Duration,
) -> io::Result<Output> {
    run(
        command,
        Stdio::null(),
        input,
        ceiling,
        crate::host::TOOL_PRINTS,
    )
}

// Each in a process group of its own, ended past the ceiling (`TimedOut`) and once the run ended,
// so nothing it started outlives it.
fn run(
    mut command: Command,
    stderr: Stdio,
    input: Input<'_>,
    ceiling: Duration,
    kept: u64,
) -> io::Result<Output> {
    command
        .stdin(match input {
            Input::Stdin(_) => Stdio::piped(),
            Input::Nothing | Input::Descriptor3(_) => Stdio::null(),
        })
        .stdout(Stdio::piped())
        .stderr(stderr)
        .process_group(0);
    let reader = match input {
        Input::Descriptor3(bytes) => {
            let (reader, mut writer) = io::pipe()?;
            writer.write_all(bytes)?;
            drop(writer);
            Some(reader)
        }
        Input::Nothing | Input::Stdin(_) => None,
    };
    if let Some(reader) = &reader {
        let fd = reader.as_raw_fd();
        // SAFETY: the closure runs in the child between fork and exec, where only
        // async-signal-safe calls are allowed; `dup2` and `fcntl` are, and it touches no memory
        // the parent shares. The pipe's own descriptor is closed on exec; descriptor 3 is not.
        unsafe {
            command.pre_exec(move || {
                let handed = if fd == 3 {
                    libc::fcntl(3, libc::F_SETFD, 0)
                } else {
                    libc::dup2(fd, 3)
                };
                if handed == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let mut child = command.spawn()?;
    drop(reader);
    let stdout = crate::host::drained(child.stdout.take(), kept);
    let stderr = crate::host::drained(child.stderr.take(), kept);
    if let (Input::Stdin(bytes), Some(mut stdin)) = (input, child.stdin.take()) {
        let bytes = bytes.to_vec();
        // Closed once written, which is how a helper reading commands learns there are no more.
        // One that exits without reading them is not an error.
        std::thread::spawn(move || stdin.write_all(&bytes));
    }
    let exited = exits(&child);
    let ended = exited.recv_timeout(ceiling).is_ok();
    if !ended {
        signal(&child, libc::SIGTERM);
        if exited.recv_timeout(GRACE).is_err() {
            signal(&child, libc::SIGKILL);
            let _ = exited.recv();
        }
    }
    // Before the run is waited for, while its group is still its own.
    signal(&child, libc::SIGKILL);
    let status = child.wait()?;
    if !ended {
        return Err(crate::host::timed_out(ceiling));
    }
    crate::host::output(status, stdout, stderr)
}

// Once `child` ended, left unreaped so its pid is not given to another process meanwhile.
fn exits(child: &Child) -> mpsc::Receiver<()> {
    let pid = child.id();
    let (said, exited) = mpsc::channel();
    std::thread::spawn(move || {
        loop {
            // SAFETY: an all-zero `siginfo_t` is a valid value for `waitid` to overwrite.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            // SAFETY: waits on this process's own child, writing into `info`, which lives for
            // the call; `WNOWAIT` leaves it waitable.
            let waited = unsafe {
                libc::waitid(
                    libc::P_PID,
                    pid,
                    &raw mut info,
                    libc::WEXITED | libc::WNOWAIT,
                )
            };
            if waited == 0 || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                break;
            }
        }
        let _ = said.send(());
    });
    exited
}

// What a sign-in's tool prints, by chunk until its pipe closes, for its own words carry no
// newline after an address or a prompt.
pub(crate) fn pump(mut output: impl Read, said: &Sender<String>, _tool: u32) {
    let mut buffer = [0_u8; 1024];
    while let Ok(read) = output.read(&mut buffer) {
        if read == 0 {
            break;
        }
        if said
            .send(String::from_utf8_lossy(&buffer[..read]).into_owned())
            .is_err()
        {
            break;
        }
    }
}

// A sign-in's tool, which a cancel kills and reaps.
pub(crate) struct Tool(Child);

pub(crate) fn start(mut command: Command) -> io::Result<Tool> {
    command.spawn().map(Tool)
}

impl Tool {
    pub(crate) fn child(&mut self) -> &mut Child {
        &mut self.0
    }

    pub(crate) fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
        self.0.wait()
    }

    pub(crate) fn stop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Sends `sig` to the process group `child` leads.
fn signal(child: &Child, sig: libc::c_int) {
    let Ok(group) = libc::pid_t::try_from(child.id()) else {
        return;
    };
    // SAFETY: `killpg` takes plain integers and touches no memory; the group is the one the
    // run was started in, of which `child` is the leader.
    unsafe {
        libc::killpg(group, sig);
    }
}

/// Whether process `pid` may still be running. A pid that cannot be probed is taken as
/// running: the callers only ever leave alone what a live process may still be using.
pub(crate) fn may_be_running(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return true;
    };
    // SAFETY: signal 0 is never delivered; `kill` only reports whether `pid` exists and may
    // be signalled, and touches no memory of this process.
    let probed = unsafe { libc::kill(pid, 0) };
    probed == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Whether `path` is a program this user may run, as `execvp` judges it: a regular file,
/// once every link is followed, that `access` lets this user execute. An execute bit for
/// somebody else is not enough, and `access` alone would take a directory, which this user
/// may search, for one.
pub(crate) fn can_run(path: &Path) -> bool {
    let Ok(name) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `name` is a NUL-terminated string that outlives the call, and `access` only
    // reads it.
    let executable = unsafe { libc::access(name.as_ptr(), libc::X_OK) } == 0;
    executable && std::fs::metadata(path).is_ok_and(|found| found.is_file())
}

// Claude Code records a process's creation time as `procStartFt` on Windows alone.
pub(crate) fn started(_pid: u32) -> Option<u64> {
    None
}

// Only a Windows program carries a version resource.
pub(crate) fn product_version(_path: &Path) -> Option<[u16; 4]> {
    None
}

pub(crate) fn product_name(_path: &Path) -> Option<String> {
    None
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "a test sets up its scratch files as a person or a tool would"
)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_running_and_one_that_exited_is_not() {
        assert!(may_be_running(std::process::id()));
        let exited = {
            let mut child = Command::new("true").spawn().unwrap();
            let pid = child.id();
            child.wait().unwrap();
            pid
        };
        assert!(!may_be_running(exited));
    }

    /// A program is what `execvp` would start: a regular file, once every link is followed,
    /// that this user may execute. One only its group and others may execute is not this
    /// user's to run, however many execute bits it has; nor is a directory, which `access`
    /// would let this user search.
    #[test]
    fn a_program_is_a_regular_file_this_user_may_execute() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let dir = scratch.path().to_path_buf();
        let made = |name: &str, mode: u32| {
            let path = dir.join(name);
            std::fs::write(&path, "#!/bin/sh\n").expect("a file");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
                .expect("its mode");
            path
        };
        let everyone = made("everyone", 0o755);
        let mine = made("mine", 0o700);
        let theirs = made("theirs", 0o611);
        let plain = made("plain", 0o644);
        let folder = dir.join("folder");
        std::fs::create_dir(&folder).expect("a directory");
        let link = dir.join("link");
        std::os::unix::fs::symlink(&mine, &link).expect("a link");
        let dangling = dir.join("dangling");
        std::os::unix::fs::symlink(dir.join("gone"), &dangling).expect("a link");

        assert!(can_run(&everyone));
        assert!(can_run(&mine));
        assert!(can_run(&link), "a link to a program is that program");
        assert!(!can_run(&plain));
        assert!(!can_run(&folder), "a directory is not a program");
        assert!(!can_run(&dangling));
        assert!(!can_run(&dir.join("absent")));
        // SAFETY: geteuid only reports this process's effective user id.
        if unsafe { libc::geteuid() } != 0 {
            assert!(
                !can_run(&theirs),
                "only its group and others may execute it"
            );
        }
    }

    fn shell(script: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);
        command
    }

    fn printed(out: &Output) -> String {
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    #[test]
    fn what_is_written_on_standard_input_is_read_back_whole() {
        let out = run_within(
            shell("cat -"),
            Input::Stdin(b"hello"),
            Duration::from_secs(5),
        )
        .expect("ran");
        assert!(out.status.success());
        assert_eq!(out.stdout, b"hello");
    }

    #[test]
    fn what_a_run_says_on_its_errors_is_kept() {
        let out = run_within(
            shell("echo no >&2; exit 44"),
            Input::Nothing,
            Duration::from_secs(5),
        )
        .expect("ran");
        assert_eq!(out.status.code(), Some(44));
        assert_eq!(out.stderr, b"no\n");
    }

    #[test]
    fn a_large_answer_cannot_stall_the_run() {
        let mut head = Command::new("head");
        head.args(["-c", "1000000", "/dev/zero"]);
        let out = run_within(head, Input::Nothing, Duration::from_secs(10)).expect("ran");
        assert_eq!(out.stdout.len(), 1_000_000);
    }

    #[test]
    fn a_tools_run_keeps_its_first_mib_and_none_of_its_errors() {
        let out = run_tool(
            Permit::for_a_test(),
            shell("head -c 2000000 /dev/zero; echo no >&2"),
            Input::Nothing,
            Duration::from_secs(10),
        )
        .expect("ran");
        assert_eq!(out.stdout.len(), 1 << 20);
        assert!(out.stderr.is_empty());
    }

    /// What is handed arrives on descriptor 3 and nowhere else: not on standard input, which
    /// Claude Code's print mode adds to its prompt, nor in an argument or a variable.
    #[test]
    fn what_is_handed_arrives_on_descriptor_three_alone() {
        let out = run_within(
            shell(r#"printf '%s|' "$(cat <&3)" "$(cat)" "$*""#),
            Input::Descriptor3(b"the-token"),
            Duration::from_secs(10),
        )
        .expect("ran");
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(printed(&out), "the-token|||");
    }

    #[test]
    fn with_nothing_handed_descriptor_three_is_closed() {
        let out = run_within(
            shell("cat <&3 2>/dev/null; echo $?"),
            Input::Nothing,
            Duration::from_secs(10),
        )
        .expect("ran");
        assert_ne!(printed(&out).trim(), "0");
    }

    /// Past the ceiling the run is stopped, and so is the child it started, which holds a pipe
    /// this test reads to its end.
    #[test]
    fn a_run_past_the_ceiling_is_stopped_with_its_children() {
        let scratch = tempfile::tempdir().expect("a scratch folder");
        let held = scratch.path().join("held");
        let name = CString::new(held.as_os_str().as_bytes()).expect("a path");
        // SAFETY: a NUL-terminated path that outlives the call.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let (closed, closes) = mpsc::channel();
        let reading = held.clone();
        std::thread::spawn(move || {
            let mut pipe = std::fs::File::open(reading).expect("its writer");
            let _ = io::copy(&mut pipe, &mut io::sink());
            let _ = closed.send(());
        });
        let refused = run_within(
            shell(&format!("sleep 30 > '{}' & sleep 30", held.display())),
            Input::Nothing,
            Duration::from_secs(1),
        )
        .expect_err("stopped");
        assert_eq!(refused.kind(), io::ErrorKind::TimedOut);
        assert!(
            closes.recv_timeout(Duration::from_secs(10)).is_ok(),
            "the child it started was stopped too"
        );
    }

    /// A run that ended, leaving a child behind with its output open, is not waited on until
    /// that child ends: the child is stopped, and what the run printed is read whole.
    #[test]
    fn what_a_run_left_behind_holding_its_output_is_stopped() {
        let out = run_within(
            shell("sleep 30 & echo done"),
            Input::Nothing,
            Duration::from_secs(60),
        )
        .expect("ran");
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(printed(&out), "done\n");
    }
}
