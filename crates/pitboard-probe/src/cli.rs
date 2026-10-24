//! The probe's command line, shared by every program the crate builds. Each subcommand is
//! one measurement a VM session or the runner-facts step needs; the hidden ones are the
//! children a measurement starts, and they check what they are given as the public ones do:
//! a job name is the probe's, a program to start is one of the probe's own, a page to open
//! is the probe's stand-in. On a system that is not Windows every subcommand prints that the
//! probe is for Windows and exits non-zero without touching anything.

use crate::report::Report;
use clap::{Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

// CF8's link scheme, and where a probe handed a link of it as its only argument writes its report.
pub const CF8_SCHEME: &str = "pitboard-probe-cf8";
pub const HANDED_OUT: &str = "PITBOARD_PROBE_CF8_HANDED_OUT";

/// Measure the Windows facts Pitboard for Windows is built on. A disposable machine and a
/// throwaway account only: never an account that holds a real login.
#[derive(Parser, Debug)]
#[command(name = "pitboard-probe", version, disable_help_subcommand = true)]
pub struct Cli {
    /// Also write the report to this file: an absolute path whose name begins
    /// pitboard-probe-, in a folder the write guard allows, in a marked account. A child
    /// started with no console, a task's action, or a process started through runas has no
    /// other way to hand its report back.
    #[arg(long, global = true)]
    pub out: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// The logon session this process runs in, by type.
    Logon,
    /// A1: the token's elevation, type, integrity and default owner; with --scratch, the
    /// owner of a file it creates there.
    Tokens {
        #[arg(long)]
        scratch: Option<PathBuf>,
    },
    /// A1, runner facts: a Safer normal-user token computed from this one, read; with
    /// --spawn, a probe started under it reports its own token.
    Safer {
        #[arg(long)]
        spawn: bool,
        #[arg(long)]
        scratch: Option<PathBuf>,
    },
    /// A2, B1-B3: the build, every home a tool or the standard library may use, the profile
    /// type, the account name's facts, and the strings each tool may hash a home from.
    Homes {
        /// A Codex home to give the hash candidates of (B2, H4).
        #[arg(long)]
        codex_home: Option<PathBuf>,
        /// A Claude Code config folder to give the hash candidates of (B2, G4).
        #[arg(long)]
        claude_config_dir: Option<PathBuf>,
    },
    /// B4: which spellings of the search-path variable this process got, and what a child
    /// gets when its parent sets both PATH and Path.
    PathVars {
        #[arg(long)]
        scratch: PathBuf,
    },
    #[command(hide = true)]
    EnvChild,
    /// C1/C2: the access of the login folders (in a marked account), by relation; a
    /// protected-DACL file made, kept, opened from another token, removed.
    Acl {
        #[arg(long)]
        scratch: Option<PathBuf>,
        /// Make pitboard-probe-protected under --scratch with a protected DACL.
        #[arg(long)]
        create: bool,
        /// Leave the file made by --create for another token to --open.
        #[arg(long)]
        keep: bool,
        /// Try to read, write and delete-open this probe file under --scratch. Given alone it
        /// changes nothing, so it needs the scratch check and not the marker.
        #[arg(long)]
        open: Option<PathBuf>,
        /// Delete this probe file under --scratch.
        #[arg(long)]
        remove: Option<PathBuf>,
        /// Read one probe file's owner and DACL under --scratch, changing nothing.
        #[arg(long)]
        describe: Option<PathBuf>,
    },
    /// C3: the volume's file system, POSIX flag and both rename routes, each with the target
    /// held open with and without FILE_SHARE_DELETE.
    Volume {
        #[arg(long)]
        scratch: PathBuf,
    },
    /// C4: replace a file many times while readers open and read it by path, recording every
    /// error and every torn or missing read.
    ReplaceLoop {
        #[arg(long)]
        scratch: PathBuf,
        #[arg(long, default_value_t = 1000)]
        rounds: u32,
        /// Seconds to wait after seeding the target, for helpers to start against it.
        #[arg(long, default_value_t = 0)]
        lead_seconds: u64,
        /// The share mode of the reader this process runs beside the loop.
        #[arg(long, value_enum, default_value_t = ReaderShare::Delete)]
        reader: ReaderShare,
        /// How each replacement renames over the target.
        #[arg(long, value_enum, default_value_t = Route::Movefile)]
        route: Route,
        /// Stop after this many seconds even with rounds left; 0 runs every round.
        #[arg(long, default_value_t = 0)]
        seconds: u64,
    },
    /// C4: a reader in a process of its own against the replace loop's target.
    ReplaceReader {
        #[arg(long)]
        scratch: PathBuf,
        #[arg(long, default_value_t = 60)]
        seconds: u64,
        #[arg(long, value_enum, default_value_t = ReaderShare::Delete)]
        share: ReaderShare,
    },
    /// C5: flush a directory handle opened two ways, and the file system's mtime precision.
    FlushDir {
        #[arg(long)]
        scratch: PathBuf,
    },
    /// C5: proper-lockfile's directory lock, held with its heartbeat or checked, for the Bun
    /// helper to run against from the other side.
    Lock {
        #[arg(long)]
        scratch: PathBuf,
        #[arg(long, value_enum, default_value_t = LockMode::Hold)]
        mode: LockMode,
        #[arg(long, default_value_t = 30)]
        seconds: u64,
        #[arg(long, value_enum, default_value_t = LockTiming::Write)]
        timing: LockTiming,
    },
    /// C6: LockFileEx on a home's state.lock: held, read from another process, or the home
    /// removed while another process holds it.
    Lockfileex {
        #[arg(long)]
        scratch: PathBuf,
        #[arg(long, value_enum, default_value_t = LfxMode::Hold)]
        mode: LfxMode,
        #[arg(long, default_value_t = 20)]
        seconds: u64,
        #[arg(long, value_enum, default_value_t = ReaderShare::Delete)]
        share: ReaderShare,
    },
    /// D1-D3: seal and unseal a dummy secret with the vault's flags and entropy. seal keeps
    /// the sealed bytes in a scratch file, so unseal can open them after a password change,
    /// from a task or from another token. unseal only reads, so it needs the scratch check
    /// and not the marker, which a restricted token such as Codex's sandbox may not see.
    Dpapi {
        #[arg(long)]
        scratch: PathBuf,
        #[arg(long, value_enum, default_value_t = DpapiAction::RoundTrip)]
        action: DpapiAction,
        /// The sealed file's name under --scratch, for seal and unseal: a pitboard-probe-*
        /// name.
        #[arg(long, default_value = "pitboard-probe-sealed.bin")]
        file: String,
    },
    /// D2b/H7: build a restricted token shaped like Codex 0.162.1's unelevated sandbox
    /// (DISABLE_MAX_PRIVILEGE | LUA_TOKEN | WRITE_RESTRICTED, restricting to a capability SID,
    /// the logon SID and Everyone, with Codex's default DACL), then from it, on a private
    /// desktop, run the probe's `dpapi --action unseal` on a park it sealed and `acl --open`
    /// on a protected file it made, to see whether a token that cannot see the marker still
    /// opens a vault-style file. Writes those two files first, so it needs the marker; the
    /// children only read, so they need the scratch check alone.
    RestrictedToken {
        #[arg(long)]
        scratch: PathBuf,
    },
    /// Warm-up: hand a dummy on an anonymous pipe as C runtime descriptor 3 through
    /// `STARTUPINFOW.lpReserved2`, start `bun <script>` (fd3-read.mjs), and report what it
    /// read back, the way Claude Code reads its token descriptor. The dummy is never a real
    /// token.
    FdHandover {
        #[arg(long)]
        bun: PathBuf,
        #[arg(long)]
        script: PathBuf,
        /// The bytes handed on descriptor 3. A dummy, never a token.
        #[arg(long, default_value = "pitboard-probe: a dummy handed on descriptor 3")]
        token: String,
        /// Whether descriptor 3 is an anonymous pipe or a loopback socket.
        #[arg(long, value_enum, default_value_t = HandoverKind::Pipe)]
        kind: HandoverKind,
    },
    /// E1-E6: register, run, stop, read, list or delete a probe task through ITaskService.
    Task {
        #[arg(long, value_enum, default_value_t = TaskAction::List)]
        action: TaskAction,
        #[arg(long)]
        scratch: Option<PathBuf>,
        /// The task's label: it is named pitboard-probe-<label>.
        #[arg(long, default_value = "e1")]
        label: String,
        #[arg(long, value_enum, default_value_t = TaskFolder::Probe)]
        folder: TaskFolder,
        /// Append the account's SID to the name, as W25 might.
        #[arg(long)]
        sid_suffix: bool,
        /// E6: read Pitboard's own `\Pitboard\Watch <SID>` in place of a probe task.
        #[arg(long, conflicts_with_all = ["label", "folder", "sid_suffix"])]
        watch_service: bool,
        /// Which of the probe's programs the action runs.
        #[arg(long, value_enum, default_value_t = TaskExec::Probe)]
        exec: TaskExec,
        /// The action's arguments.
        #[arg(long, allow_hyphen_values = true)]
        args: Option<String>,
        /// A one-time trigger this many minutes from now.
        #[arg(long)]
        at_minutes: Option<u32>,
        /// T8: a daily trigger whose start boundary passed this many minutes ago, as the
        /// renewal's daily trigger is once a start was missed.
        #[arg(long, conflicts_with_all = ["at_minutes", "daily_in_minutes"])]
        daily_since_minutes: Option<u32>,
        /// T8: a daily trigger whose start boundary is this many minutes from now.
        #[arg(long, conflicts_with = "at_minutes")]
        daily_in_minutes: Option<u32>,
        /// Register it disabled, for `enable` to turn on once its start has passed.
        #[arg(long)]
        disabled: bool,
        #[arg(long)]
        start_when_available: bool,
        /// Let it start and keep running on battery.
        #[arg(long)]
        allow_battery: bool,
        /// E6: a trigger at this account's sign-in that names its SID, as the watch service's.
        #[arg(long)]
        logon: bool,
        /// How long a run may last, in ISO 8601; the watch service's is PT0S.
        #[arg(long, default_value = "PT10M")]
        time_limit: String,
    },
    /// E4: whether each console variant gets a console and a window, started each way.
    Console {
        #[arg(long)]
        scratch: PathBuf,
        /// Also start each variant as a task's action (a marked account only).
        #[arg(long)]
        with_task: bool,
    },
    #[command(hide = true)]
    ConsoleChild,
    #[command(hide = true)]
    ConsoleLaunch {
        #[arg(long)]
        exe: PathBuf,
        #[arg(long)]
        flags: u32,
        #[arg(long)]
        child_out: PathBuf,
    },
    /// F1: assign a spawn chain to a Job Object right after starting it, over many rounds,
    /// and count the grandchildren that escaped.
    Job {
        #[arg(long)]
        scratch: PathBuf,
        #[arg(long, value_enum, default_value_t = Chain::Exe)]
        chain: Chain,
        #[arg(long, default_value_t = 100)]
        rounds: u32,
        #[arg(long, value_enum, default_value_t = JobStart::Running)]
        start: JobStart,
        /// Let the job's processes break away: JOB_OBJECT_LIMIT_BREAKAWAY_OK.
        #[arg(long)]
        allow_breakaway: bool,
    },
    /// F2: open a local stand-in page from a process in a Job Object, then end the job one
    /// way, and see whether the browser was in it and survived.
    JobBrowser {
        #[arg(long)]
        scratch: PathBuf,
        #[arg(long, value_enum, default_value_t = JobEnd::TerminateJob)]
        end: JobEnd,
        #[arg(long, default_value_t = 8)]
        wait_seconds: u64,
        #[arg(long, value_enum, default_value_t = BrowserOpen::Shell)]
        open: BrowserOpen,
    },
    #[command(hide = true)]
    JobLeaf {
        #[arg(long)]
        job_name: String,
    },
    #[command(hide = true)]
    JobMid {
        #[arg(long)]
        job_name: String,
        #[arg(long)]
        breakaway: bool,
    },
    #[command(hide = true)]
    JobOpenPage {
        #[arg(long)]
        page: PathBuf,
        #[arg(long)]
        job_name: String,
        #[arg(long, default_value_t = 0)]
        linger_seconds: u64,
        #[arg(long, value_enum, default_value_t = BrowserOpen::Shell)]
        open: BrowserOpen,
    },
    /// H1/I1: the named tools' processes through WTS (any session) and Toolhelp: image path,
    /// session, package family, parent, and whether each runs as this user.
    Processes {
        /// An image to report, such as codex.exe: a tool's, a runtime's, Pitboard's or the
        /// probe's. Repeatable; required, since nothing else is ever named.
        #[arg(long, required = true)]
        find: Vec<String>,
    },
    /// G2-G4, H4, M: the live login families' and pitboard-* items in Credential Manager.
    /// Names only, but in a marked account their attributes too; never a blob.
    CredmanNames {
        /// Fail if a live family or a pitboard-citest-* name is present, or a family could
        /// not be read.
        #[arg(long)]
        leak_check: bool,
        /// A home folder to test each target's hash suffix against (repeatable).
        #[arg(long)]
        config_dir: Vec<PathBuf>,
    },
    /// M: write, read or delete one pitboard-probe-* Credential Manager item.
    CredmanItem {
        #[arg(long, value_enum)]
        action: ItemAction,
        #[arg(long, default_value = "pitboard-probe-item")]
        name: String,
        #[arg(long, value_enum, default_value_t = Persist::LocalMachine)]
        persist: Persist,
        #[arg(long, default_value_t = 64)]
        blob_bytes: u32,
    },
    /// R1: whether a program takes its C runtime from a DLL, from its import table. The
    /// probe itself, or a program copied under --scratch.
    PeImports {
        #[arg(long)]
        scratch: Option<PathBuf>,
        #[arg(long)]
        exe: Option<PathBuf>,
    },
    /// K1 and CF8: package identity, and writes, renames and reads under %LOCALAPPDATA%\Pitboard
    /// and HKCU from this process, its children and the program its link opens.
    Sparse {
        #[arg(long, value_enum, default_value_t = SparseAction::Identity)]
        action: SparseAction,
        #[arg(long, default_value = "self")]
        tag: String,
        #[arg(long)]
        scratch: Option<PathBuf>,
    },
    /// F3: which stand-in std's Command starts for a bare name, in each arrangement.
    ExeLookup {
        #[arg(long)]
        scratch: PathBuf,
        #[arg(long, default_value = "pitboard-probe-lookup")]
        name: String,
    },
    #[command(hide = true)]
    ExeLookupHost {
        #[arg(long)]
        name: String,
        #[arg(long)]
        child_path: Option<PathBuf>,
    },
    #[command(hide = true)]
    ExeLookupWhoami,
    /// G6: replace a file in a scratch folder with another one there, the way W16 will. Both
    /// are pitboard-probe-* files the owner staged there, so neither can be a file the probe
    /// did not make.
    Swap {
        #[arg(long)]
        scratch: PathBuf,
        /// The file to replace: a pitboard-probe-* name in --scratch.
        #[arg(long)]
        target: String,
        /// The file whose bytes replace it: a pitboard-probe-* name in --scratch. Left in
        /// place.
        #[arg(long)]
        source: String,
        #[arg(long, value_enum, default_value_t = Route::Movefile)]
        route: Route,
    },
    /// G8: run a program with no window and piped stdio, passing typed lines on with the
    /// chosen line ending. Refuses unless CLAUDE_CONFIG_DIR and CODEX_HOME are scratch
    /// folders.
    Windowless {
        #[arg(long)]
        scratch: PathBuf,
        #[arg(long, value_enum, default_value_t = Newline::Crlf)]
        newline: Newline,
        #[arg(last = true, required = true)]
        program: Vec<String>,
    },
    /// G9: daemon.lock's fields against the process it names, the cc-daemon pipes, and
    /// pipe.key's size, in a scratch Claude Code config folder.
    Daemon {
        #[arg(long)]
        config_dir: PathBuf,
        #[arg(long)]
        pipe_key: Option<PathBuf>,
    },
    /// Runner facts: whether this account can make a file symbolic link under --scratch.
    Symlink {
        #[arg(long)]
        scratch: PathBuf,
    },
    /// The facts every Windows CI leg reports about its runner, as one report.
    RunnerFacts {
        #[arg(long)]
        scratch: PathBuf,
    },
    /// E5: the path this program reports for itself (std's current_exe and
    /// GetModuleFileNameW), as started through a link, a junction, a shim or a copy.
    ExePath,
    /// T8: FILE_ID_INFO of a folder, a file, a hard link and a junction to them, another
    /// folder and file, and the file once renamed, in a folder the block makes under
    /// --scratch.
    FileId {
        #[arg(long)]
        scratch: PathBuf,
    },
    /// T8: what GetFullPathNameW makes of reserved device names and of trailing dots and
    /// spaces, and the name a file made with each of the others is stored by, in a folder
    /// the block makes under --scratch.
    ReservedNames {
        #[arg(long)]
        scratch: PathBuf,
    },
    /// T8: a folder and a file past 260 characters made and read without `\\?\`, in a
    /// folder the block makes under --scratch, and whether long paths are on for this
    /// process.
    LongPath {
        #[arg(long)]
        scratch: PathBuf,
    },
    /// T8: the creation time GetProcessTimes gives for children of this probe, against the
    /// clock around each start, each child's own reading, and after it exits.
    ProcessTimes {
        #[arg(long, default_value_t = 20)]
        children: u32,
    },
    #[command(hide = true)]
    ProcessTimesChild,
    /// T8: watch for WM_SETTINGCHANGE in a never-shown top-level window and a message-only
    /// one, reading a variable from this process's environment and from HKCU\Environment
    /// meanwhile; or broadcast one for "Environment".
    SettingChange {
        #[arg(long, value_enum, default_value_t = SettingAction::Watch)]
        action: SettingAction,
        /// The variable to read: a PITBOARD_PROBE_* name.
        #[arg(long)]
        name: String,
        #[arg(long, default_value_t = 60)]
        seconds: u64,
        /// For watch: a scratch folder, where it writes pitboard-probe-sc-ready once its
        /// windows are up.
        #[arg(long)]
        scratch: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ReaderShare {
    /// FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, as libuv opens a file.
    Delete,
    /// FILE_SHARE_READ | FILE_SHARE_WRITE.
    NoDelete,
    /// No reader.
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum LockMode {
    Hold,
    Check,
}

// Claude Code 2.1.300's write lock and refresh lock, as `pitboard_core::lock` keeps them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum LockTiming {
    Write,
    Refresh,
}

impl LockTiming {
    pub fn stale_ms(self) -> u64 {
        match self {
            LockTiming::Write => 15_000,
            LockTiming::Refresh => 60_000,
        }
    }

    pub fn heartbeat_ms(self) -> u64 {
        match self {
            LockTiming::Write => 7_500,
            LockTiming::Refresh => 5_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum LfxMode {
    Hold,
    Read,
    RemoveHome,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum DpapiAction {
    RoundTrip,
    Seal,
    Unseal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum HandoverKind {
    /// An anonymous pipe, which Bun's fstat reads as a FIFO.
    Pipe,
    /// A loopback socket, to see whether Bun's fstat reads it as a socket.
    Socket,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum TaskAction {
    Register,
    Run,
    Stop,
    Read,
    List,
    Delete,
    Enable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum TaskFolder {
    /// \pitboard-probe\, the probe's own folder.
    Probe,
    /// \Pitboard\, W25's candidate folder.
    Pitboard,
    /// The root folder.
    Root,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum TaskExec {
    Probe,
    Detached,
    DetachedAsmv1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Chain {
    /// probe, then probe, then the probe leaf.
    Exe,
    /// cmd.exe running a .cmd, then node.exe, then the probe leaf, as an npm shim does.
    CmdNodeExe,
    /// probe, then probe, then the leaf started with CREATE_BREAKAWAY_FROM_JOB.
    ExeBreakaway,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum JobStart {
    /// Assigned to the job right after it starts.
    Running,
    /// Started with CREATE_SUSPENDED, assigned, then its one thread resumed: W19's route.
    Suspended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum BrowserOpen {
    /// ShellExecuteW's open of the page.
    Shell,
    /// The http open command from AssocQueryStringW, started with CreateProcessW.
    Assoc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum JobEnd {
    TerminateJob,
    /// Terminate only the job's probe processes, as an allowlist would.
    Allowlist,
    /// Let the process that opened the page exit, and close the job.
    ParentExit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ItemAction {
    Write,
    Read,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Persist {
    Session,
    LocalMachine,
    Enterprise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum SparseAction {
    /// Whether this process has package identity.
    Identity,
    /// Write, rename and read back a file and an HKCU value tagged --tag.
    Write,
    /// From an identity process: start the plain probe and a stand-in claude.exe, each
    /// writing with a tag of its own.
    Children,
    /// From an identity process: open the pitboard-probe-cf8 link, whose handler writes too.
    Launch,
    /// From an identity process: start the plain probe with a pitboard-probe-cf8 link as its
    /// only argument, as PitboardShare.exe starts Pitboard.exe.
    Handoff,
    /// Read back every tag, from wherever this process runs.
    Read,
    /// Remove everything the block made.
    Clean,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Route {
    /// MoveFileExW with MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH.
    Movefile,
    /// SetFileInformationByHandle with FileRenameInfoEx and POSIX semantics.
    Posix,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum SettingAction {
    Watch,
    Broadcast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Newline {
    Lf,
    Crlf,
}

/// What a run hands back.
#[derive(Debug)]
pub enum Outcome {
    /// A report, printed and, with --out, written.
    Report(Report),
    /// An exit code and nothing printed: a helper whose answer is its code.
    Code(u8),
    /// One line printed: a helper whose answer is that line.
    Line(String),
}

/// The probe's `main`, for every program the crate builds.
pub fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if let Some(link) = handed_link(&args) {
        let out = std::env::var_os(HANDED_OUT).map(PathBuf::from);
        return finish(handed(link), out.as_deref());
    }
    let cli = Cli::parse();
    finish(run(&cli), cli.out.as_deref())
}

fn finish(outcome: Outcome, out: Option<&Path>) -> ExitCode {
    match outcome {
        Outcome::Report(report) => {
            let text = report.render();
            let mut failed = report.failed();
            if let Some(out) = out
                && let Err(reason) = write_out(out, &text)
            {
                let _ = writeln!(std::io::stderr(), "pitboard-probe: --out: {reason}");
                failed = true;
            }
            // A child started with no console may have no standard output at all.
            let _ = writeln!(std::io::stdout().lock(), "{text}");
            if failed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Outcome::Code(code) => ExitCode::from(code),
        Outcome::Line(line) => {
            let _ = writeln!(std::io::stdout().lock(), "{line}");
            ExitCode::SUCCESS
        }
    }
}

// PitboardShare.exe hands Pitboard.exe the link as its only argument, so the stand-in takes nothing else.
pub fn handed_link(args: &[OsString]) -> Option<&str> {
    let [only] = args else { return None };
    let link = only.to_str()?;
    let (scheme, _) = link.split_once(':')?;
    scheme.eq_ignore_ascii_case(CF8_SCHEME).then_some(link)
}

#[cfg(windows)]
fn run(cli: &Cli) -> Outcome {
    crate::win::run(&cli.command, cli.out.as_deref())
}

#[cfg(windows)]
fn write_out(out: &Path, text: &str) -> Result<(), String> {
    crate::win::write_out(out, text)
}

#[cfg(windows)]
fn handed(link: &str) -> Outcome {
    crate::win::handed(link)
}

#[cfg(not(windows))]
fn run(cli: &Cli) -> Outcome {
    Outcome::Report(Report::refused(
        block_name(&cli.command),
        crate::logon::LogonSession::unknown(),
        "pitboard-probe measures Windows facts and runs on Windows only; this is another \
         system",
    ))
}

#[cfg(not(windows))]
fn write_out(_out: &Path, _text: &str) -> Result<(), String> {
    Err("the probe writes nothing on a system that is not Windows".into())
}

#[cfg(not(windows))]
fn handed(_link: &str) -> Outcome {
    Outcome::Report(Report::refused(
        "sparse",
        crate::logon::LogonSession::unknown(),
        "pitboard-probe measures Windows facts and runs on Windows only; this is another \
         system",
    ))
}

/// The block a command reports as.
pub fn block_name(command: &Command) -> &'static str {
    match command {
        Command::Logon => "logon",
        Command::Tokens { .. } => "tokens",
        Command::Safer { .. } => "safer",
        Command::Homes { .. } => "homes",
        Command::PathVars { .. } => "path-vars",
        Command::EnvChild => "env-child",
        Command::Acl { .. } => "acl",
        Command::Volume { .. } => "volume",
        Command::ReplaceLoop { .. } => "replace-loop",
        Command::ReplaceReader { .. } => "replace-reader",
        Command::FlushDir { .. } => "flush-dir",
        Command::Lock { .. } => "lock",
        Command::Lockfileex { .. } => "lockfileex",
        Command::Dpapi { .. } => "dpapi",
        Command::RestrictedToken { .. } => "restricted-token",
        Command::FdHandover { .. } => "fd-handover",
        Command::Task { .. } => "task",
        Command::Console { .. } => "console",
        Command::ConsoleChild => "console-child",
        Command::ConsoleLaunch { .. } => "console-launch",
        Command::Job { .. } => "job",
        Command::JobBrowser { .. } => "job-browser",
        Command::JobLeaf { .. } => "job-leaf",
        Command::JobMid { .. } => "job-mid",
        Command::JobOpenPage { .. } => "job-open-page",
        Command::Processes { .. } => "processes",
        Command::CredmanNames { .. } => "credman-names",
        Command::CredmanItem { .. } => "credman-item",
        Command::PeImports { .. } => "pe-imports",
        Command::Sparse { .. } => "sparse",
        Command::ExeLookup { .. } => "exe-lookup",
        Command::ExeLookupHost { .. } => "exe-lookup-host",
        Command::ExeLookupWhoami => "exe-lookup-whoami",
        Command::Swap { .. } => "swap",
        Command::Windowless { .. } => "windowless",
        Command::Daemon { .. } => "daemon",
        Command::Symlink { .. } => "symlink",
        Command::RunnerFacts { .. } => "runner-facts",
        Command::ExePath => "exe-path",
        Command::FileId { .. } => "file-id",
        Command::ReservedNames { .. } => "reserved-names",
        Command::LongPath { .. } => "long-path",
        Command::ProcessTimes { .. } => "process-times",
        Command::ProcessTimesChild => "process-times-child",
        Command::SettingChange { .. } => "setting-change",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn out_is_global_and_processes_needs_a_name() {
        let cli = Cli::try_parse_from(["pitboard-probe", "logon", "--out", "x.json"]).unwrap();
        assert_eq!(cli.out, Some(PathBuf::from("x.json")));
        assert!(Cli::try_parse_from(["pitboard-probe", "processes"]).is_err());
        let cli =
            Cli::try_parse_from(["pitboard-probe", "processes", "--find", "codex.exe"]).unwrap();
        assert!(matches!(cli.command, Command::Processes { ref find } if find == &["codex.exe"]));
    }

    #[test]
    fn windowless_takes_the_program_after_a_double_dash() {
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "windowless",
            "--scratch",
            r"C:\s",
            "--",
            "claude",
            "auth",
            "login",
        ])
        .unwrap();
        match cli.command {
            Command::Windowless { program, .. } => assert_eq!(program, ["claude", "auth", "login"]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn task_arguments_may_begin_with_a_dash() {
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "task",
            "--action",
            "register",
            "--args",
            "--out x dpapi",
        ])
        .unwrap();
        assert!(
            matches!(cli.command, Command::Task { args: Some(ref a), .. } if a == "--out x dpapi")
        );
    }

    #[test]
    fn sparse_opens_its_link_with_a_scratch_folder() {
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "sparse",
            "--action",
            "launch",
            "--scratch",
            r"C:\s",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Sparse {
                action: SparseAction::Launch,
                scratch: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn a_lone_link_of_its_scheme_is_a_handoff() {
        let args = |list: &[&str]| list.iter().map(OsString::from).collect::<Vec<_>>();
        let link = "pitboard-probe-cf8://open?url=https%3A%2F%2Fexample.com%2F";
        assert_eq!(handed_link(&args(&[link])), Some(link));
        assert_eq!(
            handed_link(&args(&["PITBOARD-PROBE-CF8://open"])),
            Some("PITBOARD-PROBE-CF8://open")
        );
        for refused in [
            &[][..],
            &[link, link][..],
            &["sparse"][..],
            &["--out", link][..],
            &["pitboard://open?url=x"][..],
            &["pitboard-probe-cf8x://open"][..],
        ] {
            assert_eq!(handed_link(&args(refused)), None, "{refused:?}");
        }
        assert!(Cli::try_parse_from(["pitboard-probe", "sparse", "--action", "handoff"]).is_ok());
    }

    #[test]
    fn the_t8_blocks_take_what_their_rows_give() {
        for (args, block) in [
            (&["file-id", "--scratch", r"C:\s"][..], "file-id"),
            (
                &["reserved-names", "--scratch", r"C:\s"][..],
                "reserved-names",
            ),
            (&["long-path", "--scratch", r"C:\s"][..], "long-path"),
            (&["process-times"][..], "process-times"),
            (&["process-times-child"][..], "process-times-child"),
        ] {
            let cli =
                Cli::try_parse_from(std::iter::once("pitboard-probe").chain(args.iter().copied()))
                    .unwrap();
            assert_eq!(block_name(&cli.command), block);
        }
        assert!(Cli::try_parse_from(["pitboard-probe", "file-id"]).is_err());
        let cli = Cli::try_parse_from(["pitboard-probe", "process-times"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::ProcessTimes { children: 20 }
        ));
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "setting-change",
            "--name",
            "PITBOARD_PROBE_SETTINGCHANGE",
            "--seconds",
            "90",
            "--scratch",
            r"C:\s",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::SettingChange {
                action: SettingAction::Watch,
                seconds: 90,
                scratch: Some(_),
                ..
            }
        ));
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "setting-change",
            "--action",
            "broadcast",
            "--name",
            "PITBOARD_PROBE_SETTINGCHANGE",
        ])
        .unwrap();
        assert_eq!(block_name(&cli.command), "setting-change");
        assert!(Cli::try_parse_from(["pitboard-probe", "setting-change"]).is_err());
    }

    #[test]
    fn a_task_takes_a_daily_trigger_that_already_passed() {
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "task",
            "--action",
            "register",
            "--daily-since-minutes",
            "5",
            "--start-when-available",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Task {
                daily_since_minutes: Some(5),
                start_when_available: true,
                ..
            }
        ));
        assert!(
            Cli::try_parse_from([
                "pitboard-probe",
                "task",
                "--at-minutes",
                "1",
                "--daily-since-minutes",
                "5",
            ])
            .is_err()
        );
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "task",
            "--action",
            "register",
            "--daily-in-minutes",
            "3",
            "--disabled",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Task {
                daily_in_minutes: Some(3),
                disabled: true,
                ..
            }
        ));
        let cli = Cli::try_parse_from(["pitboard-probe", "task", "--action", "enable"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Task {
                action: TaskAction::Enable,
                ..
            }
        ));
    }

    #[test]
    fn a_task_takes_the_watch_services_shape_and_reads_its_task() {
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "task",
            "--action",
            "register",
            "--logon",
            "--time-limit",
            "PT0S",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Task { logon: true, ref time_limit, .. } if time_limit == "PT0S"
        ));
        let cli = Cli::try_parse_from(["pitboard-probe", "task", "--action", "stop"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Task { action: TaskAction::Stop, logon: false, ref time_limit, .. }
                if time_limit == "PT10M"
        ));
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "task",
            "--action",
            "read",
            "--watch-service",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Task {
                watch_service: true,
                ..
            }
        ));
        for named in [
            &["--label", "e6"][..],
            &["--folder", "root"],
            &["--sid-suffix"],
        ] {
            let args = ["pitboard-probe", "task", "--watch-service"];
            assert!(
                Cli::try_parse_from(args.iter().chain(named)).is_err(),
                "{named:?}"
            );
        }
    }

    #[test]
    fn every_command_has_a_block_name() {
        let cli = Cli::try_parse_from(["pitboard-probe", "symlink", "--scratch", "x"]).unwrap();
        assert_eq!(block_name(&cli.command), "symlink");
        let cli = Cli::try_parse_from(["pitboard-probe", "exe-path"]).unwrap();
        assert_eq!(block_name(&cli.command), "exe-path");
    }

    // windows: the probe refuses off Windows
    #[cfg(not(windows))]
    #[test]
    fn off_windows_every_command_refuses() {
        let cli = Cli::try_parse_from(["pitboard-probe", "tokens"]).unwrap();
        match run(&cli) {
            Outcome::Report(r) => {
                assert!(r.failed());
                assert_eq!(r.to_json()["block"], "tokens");
            }
            _ => panic!("a report"),
        }
        assert!(write_out(Path::new("x"), "y").is_err());
        match handed("pitboard-probe-cf8://open") {
            Outcome::Report(r) => {
                assert!(r.failed());
                assert_eq!(r.to_json()["block"], "sparse");
            }
            _ => panic!("a report"),
        }
    }

    #[test]
    fn the_lock_takes_claude_codes_write_and_refresh_timing() {
        assert_eq!(
            (
                LockTiming::Write.stale_ms(),
                LockTiming::Write.heartbeat_ms()
            ),
            (15_000, 7_500)
        );
        assert_eq!(
            (
                LockTiming::Refresh.stale_ms(),
                LockTiming::Refresh.heartbeat_ms()
            ),
            (60_000, 5_000)
        );
        let cli = Cli::try_parse_from(["pitboard-probe", "lock", "--scratch", r"C:\s"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Lock {
                timing: LockTiming::Write,
                ..
            }
        ));
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "lock",
            "--scratch",
            r"C:\s",
            "--mode",
            "check",
            "--timing",
            "refresh",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Lock {
                mode: LockMode::Check,
                timing: LockTiming::Refresh,
                ..
            }
        ));
    }

    #[test]
    fn the_replace_loop_takes_a_route_and_a_time_bound() {
        let cli =
            Cli::try_parse_from(["pitboard-probe", "replace-loop", "--scratch", r"C:\s"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::ReplaceLoop {
                route: Route::Movefile,
                seconds: 0,
                ..
            }
        ));
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "replace-loop",
            "--scratch",
            r"C:\s",
            "--route",
            "posix",
            "--seconds",
            "240",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::ReplaceLoop {
                route: Route::Posix,
                seconds: 240,
                ..
            }
        ));
    }

    #[test]
    fn acl_describes_one_file() {
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "acl",
            "--scratch",
            r"C:\s",
            "--describe",
            r"C:\s\pitboard-probe-bunmode.txt",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Acl { describe: Some(ref d), create: false, .. }
                if d == &PathBuf::from(r"C:\s\pitboard-probe-bunmode.txt")
        ));
    }

    #[test]
    fn the_job_takes_a_breakaway_chain_and_a_suspended_start() {
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "job",
            "--scratch",
            r"C:\s",
            "--chain",
            "exe-breakaway",
            "--start",
            "suspended",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Job {
                chain: Chain::ExeBreakaway,
                start: JobStart::Suspended,
                ..
            }
        ));
        let cli = Cli::try_parse_from(["pitboard-probe", "job", "--scratch", r"C:\s"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Job {
                start: JobStart::Running,
                allow_breakaway: false,
                ..
            }
        ));
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "job",
            "--scratch",
            r"C:\s",
            "--chain",
            "exe-breakaway",
            "--allow-breakaway",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Job {
                chain: Chain::ExeBreakaway,
                allow_breakaway: true,
                ..
            }
        ));
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "job-mid",
            "--job-name",
            "pitboard-probe-job-1",
            "--breakaway",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::JobMid {
                breakaway: true,
                ..
            }
        ));
    }

    #[test]
    fn restricted_token_takes_a_scratch_folder() {
        let cli = Cli::try_parse_from(["pitboard-probe", "restricted-token", "--scratch", r"C:\s"])
            .unwrap();
        assert!(matches!(
            cli.command,
            Command::RestrictedToken { ref scratch } if scratch == &PathBuf::from(r"C:\s")
        ));
        assert_eq!(block_name(&cli.command), "restricted-token");
        assert!(Cli::try_parse_from(["pitboard-probe", "restricted-token"]).is_err());
    }

    #[test]
    fn fd_handover_takes_a_bun_a_script_and_a_default_dummy() {
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "fd-handover",
            "--bun",
            r"C:\bun\bun.exe",
            "--script",
            r"C:\h\fd3-read.mjs",
        ])
        .unwrap();
        match cli.command {
            Command::FdHandover {
                ref bun,
                ref script,
                ref token,
                kind,
            } => {
                assert_eq!(bun, &PathBuf::from(r"C:\bun\bun.exe"));
                assert_eq!(script, &PathBuf::from(r"C:\h\fd3-read.mjs"));
                assert!(token.starts_with("pitboard-probe:"));
                assert_eq!(kind, HandoverKind::Pipe);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(block_name(&cli.command), "fd-handover");
        let sock = Cli::try_parse_from([
            "pitboard-probe",
            "fd-handover",
            "--bun",
            r"C:\bun\bun.exe",
            "--script",
            r"C:\h\fd3-read.mjs",
            "--kind",
            "socket",
        ])
        .unwrap();
        assert!(matches!(
            sock.command,
            Command::FdHandover {
                kind: HandoverKind::Socket,
                ..
            }
        ));
        assert!(Cli::try_parse_from(["pitboard-probe", "fd-handover", "--bun", "b"]).is_err());
    }

    #[test]
    fn the_browser_block_opens_through_the_shell_or_the_association() {
        let cli = Cli::try_parse_from([
            "pitboard-probe",
            "job-browser",
            "--scratch",
            r"C:\s",
            "--open",
            "assoc",
            "--end",
            "parent-exit",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::JobBrowser {
                open: BrowserOpen::Assoc,
                end: JobEnd::ParentExit,
                ..
            }
        ));
        let cli =
            Cli::try_parse_from(["pitboard-probe", "job-browser", "--scratch", r"C:\s"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::JobBrowser {
                open: BrowserOpen::Shell,
                ..
            }
        ));
    }
}
