//! The stand-in the tests start in place of a program: one compiled program that plays
//! `claude`, `codex`, the interpreter npm installs beside Codex, a service manager, or
//! whatever else a test has run, as the script beside it says. It is the same program on
//! every system. The shell scripts it replaced ran only where `/bin/sh` does, and needed
//! `shasum` and `security` besides.
//!
//! The `pitboard` crate builds it as its example `stand-in`. `cargo test` with no target
//! named builds every example as a program; `cargo install`, cargo-binstall and the build
//! `cargo publish` verifies never do, and nothing installs one. A test puts a copy where a
//! program is looked for with [`install`], and the script beside that copy, as
//! `<name>.stand-in.json`. [`built`] finds the stand-in beside the test that is running, and
//! refuses, saying how to build it, where it is not built. A test run alone, as `cargo test
//! -p pitboard --test <name>` runs one, builds no example, and neither does a run of
//! `pitboard-ffi`'s tests. Nor do `--example stand-in`, `--examples` or `--all-targets`
//! given to `cargo test`, which build it as a test of its own and not as the program, as
//! cargo 1.98.1 did when tried. It is found, never built on demand, so a test binary run
//! outside Cargo, as one run as another user is, finds it as well.
//!
//! What it exits with, where a step does not say:
//! - 0 once every step is taken;
//! - 64 for arguments the script refuses, and where it has no script;
//! - 65 where the variable that says where a login goes is unset or empty;
//! - 66 for a keychain item that holds a real login, which it never asks the keychain about;
//! - 70 for a step it could not take, saying why on its standard error.

use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fmt;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

/// What a stand-in does once started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Script {
    /// Plays a program. Given `args`, it refuses any other arguments, exiting 64 before it
    /// does anything. Then it takes each step in turn, and exits 0 after the last, unless a
    /// step has ended it first.
    Plays {
        args: Option<Vec<String>>,
        steps: Vec<Step>,
    },
    /// An interpreter, as `node` is to the `codex.js` npm installs: it plays the script in
    /// the file its first argument names, after that file's `#!` line, with the arguments
    /// after that one.
    Interprets,
}

impl Script {
    /// A program that is there to be found and never meant to run. Run, it says `said` on its
    /// standard error and exits 64, doing nothing.
    pub fn refusing(said: &str) -> Script {
        Script::Plays {
            args: None,
            steps: vec![Step::Warns(said.to_owned()), Step::Exits(64)],
        }
    }
}

/// One thing a stand-in does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step {
    /// Writes this to its standard output, as it is, and nothing after it.
    Says(String),
    /// Writes this to its standard error, as it is.
    Warns(String),
    /// Waits this many milliseconds.
    Waits(u64),
    /// Exits with this code.
    Exits(u8),
    /// Writes its own process id at this path, whole: under a name of its own beside it, then
    /// moved into place, so a test reading it as it is written never finds it half there.
    WritesItsPid(PathBuf),
    /// Adds a line to the file at `at` saying how it was started, as one JSON object: the
    /// program as it was named, its arguments, and the value of each variable of
    /// `variables`, null for one it was not given.
    Records { at: PathBuf, variables: Vec<String> },
    /// Writes `contents` to the file `name` in the directory the variable `under` names, then
    /// makes it private to its owner, as Claude Code and Codex leave a login in a file:
    /// written, then its access set.
    WritesLogin {
        under: String,
        name: String,
        contents: String,
    },
    /// Stores `contents` in the login keychain where Claude Code keeps the login for the
    /// config directory the variable `under` names: a generic password of the account
    /// `account`, named `Claude Code-credentials-` and the first eight hex digits of the
    /// SHA-256 of that directory, through `/usr/bin/security`, replacing one already there.
    /// A name in `never` is refused before the keychain is asked anything: the harness names
    /// the slots that hold a real login there.
    StoresInKeychain {
        under: String,
        account: String,
        contents: String,
        never: Vec<String>,
    },
    /// Unless something is at `pid` already, makes the file `while_present` and starts
    /// another of itself, which holds this one's output open for as long as that file is
    /// there, and writes that one's process id at `pid`, whole. It outlives this one, as a
    /// program a tool started outlives the tool: the test lets it go by taking the file away.
    HoldsOutput {
        while_present: PathBuf,
        pid: PathBuf,
    },
    /// Reads lines typed to it, as Claude Code reads the code its browser shows. It writes
    /// every line typed so far at `typed`, whole, each with its newline. A line with a `#`
    /// that has something on each side, `<code>#<state>`, it takes: it says `takes` on its
    /// standard output and exits 0. Any other line it refuses with `refuses` on its standard
    /// error, and reads on. Once its input ends, it exits 1. An unfinished line at the end is
    /// not read, as the shell's `read` reads none.
    ReadsCodes {
        typed: PathBuf,
        takes: String,
        refuses: String,
    },
    /// Listens on this port of 127.0.0.1, as a sign-in's own server does, until it exits.
    HoldsPort(u16),
    /// Keeps the file at `at` open, made if it is not there, until it exits: with `locked`,
    /// with an exclusive lock on it.
    HoldsFile { at: PathBuf, locked: bool },
}

/// What [`install`] and a stand-in that holds output start the built stand-in for, rather
/// than to play a script. The stand-in's own, which Pitboard never reads or sets.
const JOB: &str = "STAND_IN_JOB";

/// What the stand-in does when it is started for [`JOB`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Job {
    /// Copies itself to this path, ready to run.
    CopiesItselfTo(PathBuf),
    /// Writes `contents` to `to`, ready to run.
    Writes { to: PathBuf, contents: String },
    /// Holds its output open for as long as this file is there.
    HoldsOutputWhile(PathBuf),
}

/// The stand-in, as the example `stand-in` runs it: does the job it was started for, or
/// plays the script beside it.
pub fn main() -> ExitCode {
    #[allow(
        clippy::disallowed_methods,
        reason = "the stand-in's own job, which only the stand-in sets"
    )]
    let job = std::env::var_os(JOB);
    if let Some(job) = job {
        return match serde_json::from_str::<Job>(&job.to_string_lossy()) {
            Ok(job) => done(job.run()),
            Err(error) => done(Err(io::Error::other(format!("{JOB} unread: {error}")))),
        };
    }
    let mut argv = std::env::args_os().map(|arg| arg.to_string_lossy().into_owned());
    let program = argv.next().unwrap_or_default();
    let args: Vec<String> = argv.collect();
    let me = match std::env::current_exe().and_then(|me| me.canonicalize()) {
        Ok(me) => me,
        Err(error) => return done(Err(error)),
    };
    let beside = script_beside(&me);
    let script = match std::fs::read_to_string(&beside) {
        Ok(text) => serde_json::from_str::<Script>(&text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!(
                "a test's stand-in with no script at {}, so nothing to play",
                beside.display()
            );
            return ExitCode::from(64);
        }
        Err(error) => return done(Err(error)),
    };
    let script = match script {
        Ok(script) => script,
        Err(error) => return done(Err(io::Error::other(format!("its script: {error}")))),
    };
    #[allow(
        clippy::disallowed_methods,
        reason = "the stand-in reads what its own environment says, as the tool it plays does"
    )]
    let variable = |name: &str| std::env::var_os(name);
    let (stdin, stdout, stderr) = (io::stdin(), io::stdout(), io::stderr());
    let mut run = Run {
        me: &me,
        program,
        variable: &variable,
        input: &mut stdin.lock(),
        output: &mut stdout.lock(),
        errors: &mut stderr.lock(),
    };
    ExitCode::from(run.script(&script, &args))
}

/// 0 for a job done; 70 for one that could not be, said on its standard error.
fn done(did: io::Result<()>) -> ExitCode {
    match did {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("a test's stand-in could not do what it was asked: {error}");
            ExitCode::from(70)
        }
    }
}

impl Job {
    fn run(self) -> io::Result<()> {
        match self {
            Job::CopiesItselfTo(to) => {
                let me = std::env::current_exe()?;
                ready_to_run(&to, |part| std::fs::copy(&me, part).map(|_| ()))
            }
            Job::Writes { to, contents } => {
                ready_to_run(&to, |part| std::fs::write(part, contents.as_bytes()))
            }
            Job::HoldsOutputWhile(present) => {
                while present.exists() {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Ok(())
            }
        }
    }
}

/// A program at `to`, made by `make` under a name of its own beside it, made runnable, then
/// moved into place: one that runs there already, a stand-in still running from before, is
/// replaced and never written over, and nothing ever runs one half written.
fn ready_to_run(to: &Path, make: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<()> {
    let part = sibling(to, &format!("part-{}", std::process::id()));
    make(&part)?;
    crate::host::fs::testing::make_runnable(&part)?;
    std::fs::rename(&part, to)
}

/// `at`'s name with `.<suffix>` after it, in the same directory.
fn sibling(at: &Path, suffix: &str) -> PathBuf {
    let mut name = at.file_name().map(OsString::from).unwrap_or_default();
    name.push(".");
    name.push(suffix);
    at.with_file_name(name)
}

/// Where the script a stand-in at `program` plays is kept: beside it, as
/// `<name>.stand-in.json`.
pub fn script_beside(program: &Path) -> PathBuf {
    sibling(program, "stand-in.json")
}

/// `contents` at `at`, whole: written under a name of this process's own beside it, then
/// moved into place, so two stand-ins writing the same file never move each other's half.
fn write_whole(at: &Path, contents: &str) -> io::Result<()> {
    let part = sibling(at, &format!("part-{}", std::process::id()));
    std::fs::write(&part, contents)?;
    std::fs::rename(&part, at)
}

/// Puts a stand-in at `at` that plays `script`: the script beside it, then a copy of the
/// built stand-in, which the stand-in itself makes.
///
/// Another process writes the program, so this one never holds it open for writing. Tests
/// run on threads, and on Linux a thread that starts a process while another holds such a
/// descriptor gives the child a copy of it; running the file then fails with ETXTBSY, "Text
/// file busy", until that child has started its own program. CI met it on Linux (run
/// 36457687750), in the test that ran a fake `codex` it had just written. The script is data
/// the stand-in reads, never runs, so it is written here.
pub fn install(at: &Path, script: &Script) -> io::Result<()> {
    let built = built().map_err(|missing| io::Error::new(io::ErrorKind::NotFound, missing))?;
    let text = serde_json::to_string_pretty(script).map_err(io::Error::other)?;
    write_whole(&script_beside(at), &text)?;
    run_job(&built, &Job::CopiesItselfTo(at.to_path_buf()))
}

/// A program at `at` holding `contents`, such as a script starting with a `#!` line, written
/// by the built stand-in and made runnable, for the reason [`install`] has another process
/// write the program.
pub fn write_program(at: &Path, contents: &str) -> io::Result<()> {
    let built = built().map_err(|missing| io::Error::new(io::ErrorKind::NotFound, missing))?;
    run_job(
        &built,
        &Job::Writes {
            to: at.to_path_buf(),
            contents: contents.to_owned(),
        },
    )
}

/// Starts the built stand-in for `job`, and waits for it to be done.
fn run_job(built: &Path, job: &Job) -> io::Result<()> {
    let job_text = serde_json::to_string(job).map_err(io::Error::other)?;
    let out = Command::new(built)
        .env(JOB, job_text)
        .stdin(Stdio::null())
        .output()?;
    if out.status.success() {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "{} could not do what it was asked, {job:?}: {}",
        built.display(),
        String::from_utf8_lossy(&out.stderr).trim()
    )))
}

/// The stand-in as `cargo build -p pitboard --example stand-in` builds it, for the test that
/// is running: in `examples` beside the `deps` it runs from.
pub fn built() -> Result<PathBuf, NotBuilt> {
    let test = std::env::current_exe().map_err(|_| NotBuilt { looked_at: None })?;
    built_for(&test)
}

/// The stand-in built for the test `test`: `<profile>/examples/stand-in` for a test at
/// `<profile>/deps/<test>`, where Cargo puts both.
fn built_for(test: &Path) -> Result<PathBuf, NotBuilt> {
    let at = test.parent().and_then(Path::parent).map(|profile| {
        profile
            .join("examples")
            .join(format!("stand-in{}", std::env::consts::EXE_SUFFIX))
    });
    match at {
        Some(at) if at.is_file() => Ok(at),
        looked_at => Err(NotBuilt { looked_at }),
    }
}

/// The stand-in is not built where the running test looks for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotBuilt {
    looked_at: Option<PathBuf>,
}

impl fmt::Display for NotBuilt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let looked_at = self.looked_at.as_ref().map_or_else(
            || "beside this test".to_owned(),
            |at| at.display().to_string(),
        );
        write!(
            f,
            "the stand-in the tests start in place of claude, codex and every other program \
             they run is not built: nothing is at {looked_at}. `cargo test` and `cargo test \
             -p pitboard` build it with every other target. Before any other run, build it \
             with `cargo build -p pitboard --example stand-in`, given the --release, \
             --profile or --target the tests are built with"
        )
    }
}

impl std::error::Error for NotBuilt {}

/// What a stand-in runs with: its own program, how it was named, its environment and its
/// three streams.
struct Run<'a> {
    /// Its own program, which it starts again to hold its output.
    me: &'a Path,
    /// The program as it was named, as a shell's `$0` has it.
    program: String,
    variable: &'a dyn Fn(&str) -> Option<OsString>,
    input: &'a mut dyn BufRead,
    output: &'a mut dyn Write,
    errors: &'a mut dyn Write,
}

/// What a stand-in keeps until it exits.
enum Held {
    Port(#[allow(dead_code, reason = "held, never read")] std::net::TcpListener),
    File(#[allow(dead_code, reason = "held, never read")] std::fs::File),
}

/// How taking a step came out.
enum Took {
    /// On to the next.
    Next,
    /// Exit with this.
    Ends(u8),
}

impl Run<'_> {
    /// Plays `script` with `args`, and the code it exits with.
    fn script(&mut self, script: &Script, args: &[String]) -> u8 {
        match script {
            Script::Plays {
                args: wanted,
                steps,
            } => self.plays(wanted.as_deref(), steps, args),
            Script::Interprets => self.interprets(args),
        }
    }

    /// The script in the file `args[0]` names, after its `#!` line, played with the rest.
    fn interprets(&mut self, args: &[String]) -> u8 {
        let Some((file, rest)) = args.split_first() else {
            let _ = self
                .errors
                .write_all(b"an interpreter given no script to run\n");
            return 64;
        };
        let read = std::fs::read_to_string(file).and_then(|text| {
            let body = if text.starts_with("#!") {
                text.split_once('\n').map_or("", |(_, body)| body)
            } else {
                &text
            };
            serde_json::from_str::<Script>(body).map_err(io::Error::other)
        });
        match read {
            Ok(script @ Script::Plays { .. }) => self.script(&script, rest),
            Ok(Script::Interprets) => {
                let _ = self
                    .errors
                    .write_all(b"a script that is an interpreter again\n");
                64
            }
            Err(error) => self.failed(&format!("the script {file}"), &error),
        }
    }

    fn plays(&mut self, wanted: Option<&[String]>, steps: &[Step], args: &[String]) -> u8 {
        if wanted.is_some_and(|wanted| wanted != args) {
            return 64;
        }
        let mut held = Vec::new();
        for step in steps {
            match self.take(step, args, &mut held) {
                Ok(Took::Next) => {}
                Ok(Took::Ends(code)) => return code,
                Err(error) => return self.failed(&format!("{step:?}"), &error),
            }
        }
        0
    }

    /// Says on its standard error that `what` could not be done, and why, and gives 70.
    fn failed(&mut self, what: &str, error: &io::Error) -> u8 {
        let _ = writeln!(
            self.errors,
            "a test's stand-in could not take {what}: {error}"
        );
        70
    }

    fn take(&mut self, step: &Step, args: &[String], held: &mut Vec<Held>) -> io::Result<Took> {
        match step {
            Step::Says(text) => {
                self.output.write_all(text.as_bytes())?;
                self.output.flush()?;
            }
            Step::Warns(text) => {
                self.errors.write_all(text.as_bytes())?;
                self.errors.flush()?;
            }
            Step::Waits(millis) => std::thread::sleep(Duration::from_millis(*millis)),
            Step::Exits(code) => return Ok(Took::Ends(*code)),
            Step::WritesItsPid(at) => write_whole(at, &format!("{}\n", std::process::id()))?,
            Step::Records { at, variables } => self.records(at, variables, args)?,
            Step::WritesLogin {
                under,
                name,
                contents,
            } => {
                let Some(dir) = self.directory(under) else {
                    return Ok(Took::Ends(65));
                };
                let file = dir.join(name);
                std::fs::write(&file, contents)?;
                crate::host::fs::testing::make_private(&file)?;
            }
            Step::StoresInKeychain {
                under,
                account,
                contents,
                never,
            } => {
                let Some(dir) = self.directory(under) else {
                    return Ok(Took::Ends(65));
                };
                let service = claude_code_slot(&dir);
                if never.contains(&service) {
                    writeln!(
                        self.errors,
                        "{service} holds a real login, which a test never writes"
                    )?;
                    return Ok(Took::Ends(66));
                }
                let stored = Command::new("/usr/bin/security")
                    .args(["add-generic-password", "-U", "-a", account, "-s"])
                    .arg(&service)
                    .arg("-w")
                    .arg(contents)
                    .stdin(Stdio::null())
                    .status()?;
                if !stored.success() {
                    return Err(io::Error::other(format!("security exited {stored}")));
                }
            }
            Step::HoldsOutput { while_present, pid } => {
                if !pid.exists() {
                    std::fs::write(while_present, "")?;
                    let job = Job::HoldsOutputWhile(while_present.clone());
                    let holder = Command::new(self.me)
                        .env(JOB, serde_json::to_string(&job).map_err(io::Error::other)?)
                        .stdin(Stdio::null())
                        .spawn()?;
                    write_whole(pid, &format!("{}\n", holder.id()))?;
                }
            }
            Step::ReadsCodes {
                typed,
                takes,
                refuses,
            } => return self.reads_codes(typed, takes, refuses),
            Step::HoldsPort(port) => held.push(Held::Port(std::net::TcpListener::bind((
                std::net::Ipv4Addr::LOCALHOST,
                *port,
            ))?)),
            Step::HoldsFile { at, locked } => {
                let file = std::fs::File::options()
                    .create(true)
                    .truncate(false)
                    .write(true)
                    .open(at)?;
                if *locked {
                    file.lock()?;
                }
                held.push(Held::File(file));
            }
        }
        Ok(Took::Next)
    }

    /// The directory the variable `name` names, unless it is unset or empty.
    fn directory(&self, name: &str) -> Option<PathBuf> {
        (self.variable)(name)
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
    }

    fn records(&mut self, at: &Path, variables: &[String], args: &[String]) -> io::Result<()> {
        let given: serde_json::Map<String, serde_json::Value> = variables
            .iter()
            .map(|name| {
                let value = (self.variable)(name)
                    .map(|value| serde_json::Value::from(value.to_string_lossy().into_owned()));
                (name.clone(), value.unwrap_or(serde_json::Value::Null))
            })
            .collect();
        let line = serde_json::json!({
            "program": self.program,
            "args": args,
            "variables": given,
        });
        let mut file = std::fs::File::options()
            .create(true)
            .append(true)
            .open(at)?;
        writeln!(file, "{line}")
    }

    fn reads_codes(&mut self, typed: &Path, takes: &str, refuses: &str) -> io::Result<Took> {
        let mut so_far = String::new();
        loop {
            let mut line = Vec::new();
            self.input.read_until(b'\n', &mut line)?;
            let Some(line) = line.strip_suffix(b"\n") else {
                return Ok(Took::Ends(1));
            };
            let line = String::from_utf8_lossy(line);
            so_far.push_str(&line);
            so_far.push('\n');
            write_whole(typed, &so_far)?;
            if has_both_halves(&line) {
                self.output.write_all(takes.as_bytes())?;
                self.output.flush()?;
                return Ok(Took::Ends(0));
            }
            self.errors.write_all(refuses.as_bytes())?;
            self.errors.flush()?;
        }
    }
}

/// Whether `line` is `<code>#<state>` with something on each side of a `#`, as the shell
/// pattern `?*'#'?*` matched it.
fn has_both_halves(line: &str) -> bool {
    line.char_indices()
        .any(|(at, c)| c == '#' && at > 0 && at + 1 < line.len())
}

/// The keychain item Claude Code keeps the login of the config directory `dir` in: its
/// default name, `-`, and the first eight hex digits of the SHA-256 of the directory as
/// given, as `shasum -a 256 | cut -c1-8` read it.
fn claude_code_slot(dir: &Path) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(dir.as_os_str().as_encoded_bytes());
    format!(
        "{}-{}",
        crate::provider::claude::slot::LIVE_SERVICE,
        hex::encode(&digest[..4])
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A folder of the test's own, empty, which goes with it.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!(
                "pitboard-stand-in-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("a scratch folder");
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// What a stand-in played in this process came to: the code it exits with, and what it
    /// wrote to each stream.
    #[derive(Debug)]
    struct Played {
        code: u8,
        output: String,
        errors: String,
    }

    /// `script` played with `args`, `typed` as its input and `variables` as its environment,
    /// in this process. Nothing that starts another of itself is played this way: its own
    /// program here is this test.
    fn play(script: &Script, args: &[&str], typed: &str, variables: &[(&str, &Path)]) -> Played {
        let variables: HashMap<String, OsString> = variables
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.as_os_str().to_owned()))
            .collect();
        let variable = |name: &str| variables.get(name).cloned();
        let (mut output, mut errors) = (Vec::new(), Vec::new());
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        let code = Run {
            me: Path::new("/nowhere/stand-in"),
            program: "/somewhere/claude".into(),
            variable: &variable,
            input: &mut io::Cursor::new(typed.as_bytes().to_vec()),
            output: &mut output,
            errors: &mut errors,
        }
        .script(script, &args);
        Played {
            code,
            output: String::from_utf8(output).expect("UTF-8"),
            errors: String::from_utf8(errors).expect("UTF-8"),
        }
    }

    fn plays(args: Option<&[&str]>, steps: Vec<Step>) -> Script {
        Script::Plays {
            args: args.map(|args| args.iter().map(|arg| (*arg).to_owned()).collect()),
            steps,
        }
    }

    /// A script reads back as it was written, which is how a test hands one to a stand-in.
    #[test]
    fn a_script_reads_back_as_it_was_written() {
        let script = plays(
            Some(&["auth", "login"]),
            vec![
                Step::Says("Opening browser to sign in\n".into()),
                Step::Waits(5),
                Step::HoldsPort(1455),
                Step::HoldsFile {
                    at: "/x/held".into(),
                    locked: true,
                },
                Step::Exits(3),
            ],
        );
        let text = serde_json::to_string_pretty(&script).expect("written");
        assert_eq!(serde_json::from_str::<Script>(&text).expect("read"), script);
        let interprets = serde_json::to_string(&Script::Interprets).expect("written");
        assert_eq!(
            serde_json::from_str::<Script>(&interprets).expect("read"),
            Script::Interprets
        );
    }

    /// Arguments other than the ones a script wants are refused with 64, before any step:
    /// a stand-in for `claude auth login` started any other way does nothing.
    #[test]
    fn other_arguments_are_refused_before_any_step() {
        let scratch = Scratch::new("refused");
        let pid = scratch.0.join("pid");
        let script = plays(
            Some(&["auth", "login"]),
            vec![
                Step::WritesItsPid(pid.clone()),
                Step::Says("Opening browser\n".into()),
            ],
        );
        let refused = play(&script, &["auth"], "", &[]);
        assert_eq!(refused.code, 64);
        assert_eq!((refused.output.as_str(), refused.errors.as_str()), ("", ""));
        assert!(!pid.exists(), "nothing was done");

        let taken = play(&script, &["auth", "login"], "", &[]);
        assert_eq!(taken.code, 0);
        assert_eq!(taken.output, "Opening browser\n");
        assert_eq!(
            std::fs::read_to_string(&pid).expect("its process id"),
            format!("{}\n", std::process::id()),
            "written whole, with nothing left beside it"
        );
        let left: Vec<_> = std::fs::read_dir(&scratch.0)
            .expect("the scratch folder")
            .map(|entry| entry.expect("an entry").file_name())
            .collect();
        assert_eq!(left, ["pid"], "nothing left beside it");
    }

    /// Each stream gets what it is given, in order, and a stand-in exits with the code it is
    /// given, taking no step after it.
    #[test]
    fn it_says_what_it_is_given_and_exits_as_it_is_told() {
        let played = play(
            &plays(
                None,
                vec![
                    Step::Says("Paste code here if prompted > ".into()),
                    Step::Warns("Successfully logged in\n".into()),
                    Step::Exits(3),
                    Step::Says("never said".into()),
                ],
            ),
            &["whatever", "it", "is", "given"],
            "",
            &[],
        );
        assert_eq!(played.code, 3);
        assert_eq!(played.output, "Paste code here if prompted > ");
        assert_eq!(played.errors, "Successfully logged in\n");

        let refusing = play(&Script::refusing("not meant to run\n"), &[], "", &[]);
        assert_eq!(
            (refusing.code, refusing.errors.as_str()),
            (64, "not meant to run\n")
        );
    }

    /// A login goes in the directory the variable names, written, then private to its owner,
    /// as `codex login` and Claude Code leave one. Where the variable is unset or empty,
    /// nothing is written and it exits 65.
    #[test]
    #[cfg_attr(windows, ignore = "W15: files made private to the person on Windows")]
    fn a_login_is_written_where_the_environment_says_and_nowhere_else() {
        let scratch = Scratch::new("login");
        let script = plays(
            None,
            vec![Step::WritesLogin {
                under: "CODEX_HOME".into(),
                name: "auth.json".into(),
                contents: "{\"tokens\":{}}\n".into(),
            }],
        );
        let written = play(&script, &[], "", &[("CODEX_HOME", &scratch.0)]);
        assert_eq!(written.code, 0, "{}", written.errors);
        let login = scratch.0.join("auth.json");
        assert_eq!(
            std::fs::read_to_string(&login).expect("written"),
            "{\"tokens\":{}}\n"
        );
        assert!(crate::host::fs::testing::is_private(&login).expect("its access"));

        std::fs::remove_file(&login).expect("taken away");
        assert_eq!(play(&script, &[], "", &[]).code, 65, "unset");
        assert_eq!(
            play(&script, &[], "", &[("CODEX_HOME", Path::new(""))]).code,
            65,
            "empty"
        );
        assert!(!login.exists());
    }

    /// Claude Code's slot for a directory is the one Pitboard reads for it, and one the
    /// harness names as holding a real login is refused with 66 before the keychain is asked
    /// anything: this test's keychain would be the person's own.
    #[test]
    fn a_keychain_item_that_holds_a_real_login_is_never_written() {
        let dir = Path::new("/private/tmp/pitboard-stand-in-slot");
        let slot = claude_code_slot(dir);
        assert_eq!(
            slot,
            crate::provider::claude::slot::service_for_dir("/private/tmp/pitboard-stand-in-slot")
        );
        let played = play(
            &plays(
                None,
                vec![Step::StoresInKeychain {
                    under: "CLAUDE_CONFIG_DIR".into(),
                    account: "nobody".into(),
                    contents: "{}".into(),
                    never: vec!["Claude Code-credentials".into(), slot.clone()],
                }],
            ),
            &[],
            "",
            &[("CLAUDE_CONFIG_DIR", dir)],
        );
        assert_eq!(played.code, 66);
        assert!(played.errors.contains(&slot), "{}", played.errors);
    }

    /// Lines are read as Claude Code 2.1.289 reads a code: one without a `#` with something
    /// on each side is refused and the next is read, the first with both halves is taken,
    /// and every line typed so far is written down whole.
    #[test]
    fn a_code_without_both_halves_is_refused_and_the_next_one_read() {
        let scratch = Scratch::new("codes");
        let typed = scratch.0.join("typed");
        let script = plays(
            None,
            vec![Step::ReadsCodes {
                typed: typed.clone(),
                takes: "Login successful.\n".into(),
                refuses: "Invalid code.\n".into(),
            }],
        );
        let played = play(
            &script,
            &[],
            "half\n#state\ncode#\nthe-code#the-state\nafter\n",
            &[],
        );
        assert_eq!(played.code, 0);
        assert_eq!(played.output, "Login successful.\n");
        assert_eq!(played.errors, "Invalid code.\n".repeat(3));
        assert_eq!(
            std::fs::read_to_string(&typed).expect("written down"),
            "half\n#state\ncode#\nthe-code#the-state\n"
        );

        let ended = play(&script, &[], "half\nunfinished#line", &[]);
        assert_eq!(ended.code, 1, "its input ended");
        assert_eq!(
            std::fs::read_to_string(&typed).expect("written down"),
            "half\n",
            "an unfinished line is not read"
        );
    }

    /// How it was started is added to a file, a line at a time.
    #[test]
    fn how_it_was_started_is_recorded() {
        let scratch = Scratch::new("records");
        let at = scratch.0.join("ran");
        let script = plays(
            None,
            vec![
                Step::Records {
                    at: at.clone(),
                    variables: vec!["CODEX_HOME".into(), "ABSENT".into()],
                },
                Step::Exits(1),
            ],
        );
        for _ in 0..2 {
            assert_eq!(
                play(&script, &["login"], "", &[("CODEX_HOME", Path::new("/x"))]).code,
                1
            );
        }
        let recorded = std::fs::read_to_string(&at).expect("recorded");
        let lines: Vec<serde_json::Value> = recorded
            .lines()
            .map(|line| serde_json::from_str(line).expect("a JSON line"))
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            serde_json::json!({
                "program": "/somewhere/claude",
                "args": ["login"],
                "variables": {"CODEX_HOME": "/x", "ABSENT": null},
            })
        );
    }

    /// An interpreter plays the script in the file it is given, after that file's `#!` line,
    /// with the arguments after it, as `node` runs npm's `codex.js`.
    #[test]
    fn an_interpreter_plays_the_script_it_is_given() {
        let scratch = Scratch::new("interprets");
        let file = scratch.0.join("codex.js");
        let script = plays(Some(&["login"]), vec![Step::Warns("Logged in\n".into())]);
        std::fs::write(
            &file,
            format!(
                "#!/usr/bin/env fakenode\n{}",
                serde_json::to_string(&script).expect("written")
            ),
        )
        .expect("a script");
        let file = file.to_str().expect("UTF-8");
        let played = play(&Script::Interprets, &[file, "login"], "", &[]);
        assert_eq!((played.code, played.errors.as_str()), (0, "Logged in\n"));
        assert_eq!(
            play(&Script::Interprets, &[file, "logout"], "", &[]).code,
            64
        );
        assert_eq!(play(&Script::Interprets, &[], "", &[]).code, 64);
    }

    /// A port held is one nobody else can listen on until it is let go, and a file held with
    /// a lock is one nobody else can lock.
    #[test]
    fn a_port_and_a_file_are_held_until_it_exits() {
        let scratch = Scratch::new("holds");
        let free = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .and_then(|listener| listener.local_addr())
            .expect("a free port")
            .port();
        let at = scratch.0.join("held");
        let mut held = Vec::new();
        let (mut output, mut errors) = (Vec::new(), Vec::new());
        let mut run = Run {
            me: Path::new("/nowhere/stand-in"),
            program: String::new(),
            variable: &|_| None,
            input: &mut io::Cursor::new(Vec::new()),
            output: &mut output,
            errors: &mut errors,
        };
        for step in [
            Step::HoldsPort(free),
            Step::HoldsFile {
                at: at.clone(),
                locked: true,
            },
        ] {
            assert!(matches!(run.take(&step, &[], &mut held), Ok(Took::Next)));
        }
        assert!(std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, free)).is_err());
        let other = std::fs::File::options()
            .write(true)
            .open(&at)
            .expect("the file held");
        assert!(other.try_lock().is_err(), "locked by the stand-in");
        drop(held);
        assert!(other.try_lock().is_ok(), "let go of as it exits");
    }

    /// Where the stand-in is not built, the test that wanted it is told so and how to build
    /// it, with where it looked; once it is built it is found there.
    #[test]
    fn a_stand_in_not_built_is_refused_with_the_command_that_builds_it() {
        let scratch = Scratch::new("built");
        let test = scratch
            .0
            .join("debug")
            .join("deps")
            .join("switch_round_trip-0123456789abcdef");
        let missing = built_for(&test).expect_err("nothing is built");
        let said = missing.to_string();
        let at = scratch
            .0
            .join("debug")
            .join("examples")
            .join(format!("stand-in{}", std::env::consts::EXE_SUFFIX));
        assert!(
            said.contains("`cargo build -p pitboard --example stand-in`"),
            "{said}"
        );
        assert!(said.contains(&at.display().to_string()), "{said}");

        std::fs::create_dir_all(at.parent().expect("examples")).expect("made");
        std::fs::write(&at, "").expect("built");
        assert_eq!(built_for(&test), Ok(at));
    }

    /// The script is beside the program, under the program's whole name.
    #[test]
    fn the_script_is_beside_its_program() {
        assert_eq!(
            script_beside(Path::new("/x/bin/claude")),
            Path::new("/x/bin/claude.stand-in.json")
        );
        assert_eq!(
            script_beside(Path::new("/x/bin/claude.exe")),
            Path::new("/x/bin/claude.exe.stand-in.json")
        );
    }
}
