//! Running a helper program with a deadline. A helper that never answers must not hold
//! pitboard's lock, or an app's worker, forever.

use std::io::{self, Read, Write};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// `command`'s output, with `input` on its stdin. Past `limit` the process is killed and the
/// answer is `TimedOut`. Its output is drained on other threads, so a chatty helper cannot
/// fill a pipe and stall.
pub fn output_within(mut command: Command, input: &[u8], limit: Duration) -> io::Result<Output> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> thread::JoinHandle<Vec<u8>> {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    }
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    // Dropping stdin closes it, which is how a helper reading commands learns there are no
    // more. A helper that exits without reading is not an error.
    if let Some(mut stdin) = child.stdin.take() {
        match stdin.write_all(input) {
            Err(e) if e.kind() != io::ErrorKind::BrokenPipe => return Err(e),
            _ => {}
        }
    }

    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("no answer within {}s", limit.as_secs()),
            ));
        }
        thread::sleep(Duration::from_millis(5));
    };
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// How many processes are running a program called `program`, by the name of the file they
/// were started from. `None` where the process list could not be read, which is not the
/// same as none running.
///
/// Read from `ps`, which both platforms pitboard runs on ship with the same flags, rather
/// than from `/proc` on one and `sysctl` on the other. What is compared is the executable's
/// own name, so a script or a shell that merely mentions the program is not counted.
#[cfg(target_os = "linux")]
pub fn running(program: &str) -> Option<usize> {
    // Linux says it in /proc, which every Linux has, where `/bin/ps` is not on every one:
    // a slim container or NixOS has none, and the warning this feeds would quietly vanish.
    let mut names = String::new();
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit())
            && let Ok(comm) = std::fs::read_to_string(entry.path().join("comm"))
        {
            names.push_str(comm.trim());
            names.push('\n');
        }
    }
    Some(count_named(&names, program))
}

#[cfg(not(target_os = "linux"))]
pub fn running(program: &str) -> Option<usize> {
    let mut ps = Command::new("/bin/ps");
    ps.args(["-A", "-o", "comm="]);
    let out = output_within(ps, b"", Duration::from_secs(5)).ok()?;
    if !out.status.success() {
        return None;
    }
    Some(count_named(&String::from_utf8_lossy(&out.stdout), program))
}

/// The processes in a `ps -o comm=` listing whose executable is called `program`. macOS
/// lists the full path and Linux the name alone, so the name is what is compared.
fn count_named(listing: &str, program: &str) -> usize {
    listing
        .lines()
        .map(str::trim)
        .filter(|line| {
            std::path::Path::new(line)
                .file_name()
                .is_some_and(|name| name == program)
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_counted_by_its_own_name_and_nothing_else() {
        let listing = "/Users/a/.codex/packages/standalone/bin/codex\n\
                       codex\n\
                       /bin/zsh\n\
                       /usr/bin/codex-helper\n\
                       node\n";
        assert_eq!(count_named(listing, "codex"), 2);
        assert_eq!(count_named(listing, "gemini"), 0);
    }

    /// The list is readable on every machine these tests run on, and this test's own
    /// process is in it, found by its own name.
    #[test]
    fn the_process_list_can_be_read() {
        assert_eq!(running("definitely-not-a-program-name"), Some(0));
        let me = std::env::current_exe().expect("this test's own binary");
        let name = me.file_name().unwrap().to_string_lossy().into_owned();
        // Linux keeps fifteen characters of a process's name, and test binaries are longer.
        let name: String = if cfg!(target_os = "linux") {
            name.chars().take(15).collect()
        } else {
            name
        };
        assert!(running(&name).is_some_and(|n| n >= 1), "{name} is running");
    }

    #[test]
    fn a_prompt_answer_is_returned_whole() {
        let mut cat = Command::new("cat");
        cat.arg("-");
        let out = output_within(cat, b"hello", Duration::from_secs(5)).unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, b"hello");
    }

    #[test]
    fn a_helper_that_never_answers_is_stopped_at_the_deadline() {
        let mut sleep = Command::new("sleep");
        sleep.arg("30");
        let started = Instant::now();
        let err = output_within(sleep, b"", Duration::from_millis(200)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_large_answer_cannot_stall_the_helper() {
        let mut yes = Command::new("head");
        yes.args(["-c", "1000000", "/dev/zero"]);
        let out = output_within(yes, b"", Duration::from_secs(10)).unwrap();
        assert_eq!(out.stdout.len(), 1_000_000);
    }
}
