# Architecture

This file describes how Pitboard's code is organised and what must stay true in it. What
the code rests on about macOS, Windows, WebKit, the libraries and the sites the app's account
windows open is in [docs/facts](docs/facts/README.md). What it rests on about Claude Code,
OpenAI's Codex CLI and Claude Desktop is in each tool's [register](#tool-registers). It
changes when the layout changes, not with each commit.

## Bird's eye view

Pitboard switches Claude Code, Codex and, on macOS, Claude Desktop between a person's own
accounts on one machine. It also shows how much of each Claude Code and Codex account's
limits is left. A switch parks the login in use and puts another account's parked login in
its place. For Claude Code and Codex a login is a document in a store. For Claude Desktop it
is items of Claude's data folder and keys of its `config.json`, moved while Claude is
closed. A person asks for each switch, unless they asked Pitboard to switch Claude Code by
itself before the account in use runs out: the app with its setting on, or `pitboard watch`
running in a terminal.

One crate, `pitboard-core`, does this for every tool. It reads and writes each tool's
login, keeps Pitboard's index of accounts, and asks Claude Code's and Codex's services for
usage.

Two front ends use the core: the command line, `pitboard`, on macOS and Linux, and the menu
bar app on macOS 14 or later. The app calls the core through UniFFI bindings. The command
line inside the app is what the app's daily renewal runs, and what the `pitboard-app` cask
puts on `PATH`.

The app also gives each enrolled Claude Code and Codex account a window on its tool's site,
claude.ai or chatgpt.com, where the site's own pages run in WebKit. The windows take the
list of accounts from the core, and nothing else. [Account windows](#account-windows)
describes them.

Pitboard has no server of its own. The core sends requests only to Anthropic, for Claude
Code, and to OpenAI, for Codex. It sends none for a Claude Desktop account. An account's
window loads its site, and whatever the site's pages load, as a browser would.

## Code map

- `crates/pitboard-core`: the engine. Parking, switching, recovery, the stores and usage.
  The front ends reach it through `service::Pitboard`, built with a `context::Context`.
  - `provider/`: one module per tool, `claude`, `codex` and `desktop`. Each module's
    `assumptions.rs` is that tool's register of facts, with a table saying what each fact
    is on macOS, Linux and Windows. A tool keeps one of two kinds of login, and
    `provider::of` is the one place that says which (`LoginKind`). How the tool signs in
    (`ProviderId::signs_in`) and whether its usage is read (`ProviderId::reads_usage`)
    follow from the kind. `ProviderId::supported_on` says which systems Pitboard supports a
    tool on: Claude Desktop on macOS alone, Claude Code and Codex on every system.
    - Claude Code and Codex keep a document in a store, and `claude` and `codex` implement
      the `Provider` trait in `provider/mod.rs`. The trait covers where the tool keeps its
      login, whose it is, how to renew it, what it has left and what its sign-in prints,
      which `provider::sign_in_view` reads for both apps, and when a session already running
      takes a switch made now (`Adoption`), which for Claude Code turns on whether a file
      sits behind the keychain or Credential Manager (`behind`). `provider/codex/holders.rs`
      names where a running `codex` can be, and what makes each take a switch.
      `provider/codex/layers.rs` reads which store Codex keeps its login in from every layer
      of configuration Codex reads outside a project, in Codex's order, for a switch and for
      a sign-in, and which logins Codex keeps (`Restrictions`), which `Provider::refuses`
      asks of a login before it goes in. `provider/printed.rs` reads what a tool printed as a terminal does: the
      text it shows, and where each hyperlink goes. Behind npm's `.cmd` shim,
      `provider::command` starts the program the shim would: the `claude.exe` it names, or
      the `codex.exe` its `codex.js` starts, with what that sets, and the `.cmd` itself only
      where neither is there. On Windows, `provider/claude/credman.rs` says whether Claude
      Code keeps its login in Credential Manager (`windows_backend_choice`) and builds its
      chain there: Credential Manager with `.credentials.json` behind it where anything
      turns that store on, the file alone where nothing does and Credential Manager holds no
      login at the live target. `provider/claude/pieces.rs` writes and reads a login in that
      store's pieces, in Claude Code's order (`credman_chunking`). `provider/names.rs` names
      the families of items a real login may be kept under, which a test and a build for
      tests never address, and by which doctor counts Codex's items.
    - Claude Desktop keeps items of Claude's data folder and keys of its settings file, and
      `desktop` implements the `Folder` trait in `provider/folder.rs`. Its files say where
      the data folder is (`paths.rs`), which part of an item's store no item names
      (`layout.rs`), what `config.json` and its sidecar hold (`settings.rs`), the session
      cookie's fingerprint (`jar.rs`) and which processes are Claude (`running.rs`). It
      reads no keychain item, decrypts nothing and sends no request. A person signs in
      inside Claude, and no usage of a Claude Desktop account is read.
  - `holder.rs`: what keeps a tool's login in memory while it runs, told apart by where
    its program runs from, or on Windows by the package it runs in. A switch's warning,
    `doctor` and the app's offer to quit an app all read it, so they cannot disagree.
  - `host/`: the machine, behind one seam. The `Host` trait is what a test replaces: the
    system's store of secrets, Credential Manager's names under a prefix and whether it
    holds an item (`credential_names`, `credential_held`), files, the vault, this user's
    processes in any session, each with its parent (`listing`), a Chromium cookie jar's
    rows, read only (`cookies`), an app bundle's version and identifier (`app_version`,
    `app_id`), the scheduler, whether this process runs as the person (`elevation`), whether
    the system is one Pitboard changes things on by its version (`floor`, `host::Floor`),
    and what an administrator set for a program outside every home (`administered_file`,
    `administered_file_up_to`, `administered_folder`, `managed_preference`, which
    `macos/preferences.rs` reads, and `policy`, a value under the Windows registry's `HKLM`
    or `HKCU`), the environment a Windows program started now is given (`user_environment`,
    `machine_environment`), Windows' `ProgramData` (`program_data`) and its fixed drives
    (`fixed_drives`), reached through `Context` and faked by `host/memory.rs`, which plays
    every answer. In a build for tests the real hosts read nothing an administrator set
    (`host/administered.rs`) and no app installed on the machine. `fs`, `proc` and `user`
    are plain functions for what the system does whoever asks: private files and
    directories, every other change to the disk, whether a process is alive, a program run
    with a ceiling (`proc::run_within`, the one way Pitboard waits on `security`, `ps` and
    `plutil`, which `proc::run_tool` runs a warm-up's tool through), the login name, whether
    this process runs as root, and the `PATH` the person's login shell builds, which
    `unix/shell.rs` asks for. `fs`
    creates each private file itself and hands it back open (`create_private`,
    `open_private_append` and `open_private_lock`), so how a file is made private is the
    face's alone. A login kept as files moves through four more:
    `fs::private_dir` says whether a folder is this user's own and closed to others,
    `fs::lock_dir` takes a folder's advisory lock without waiting, `fs::same_volume` says
    whether two paths can be renamed between, and `fs::move_new` is the kernel's rename that
    never replaces what is there, the one way an item of a Claude Desktop login moves.
    `fs::testing`, in a build for tests, is how a test or a fixture sets a file's access or
    makes a link; other crates reach it as `pitboard_core::testing::fs`. `program.rs` finds
    a program the way the system's launcher does: on Windows each folder is tried with the
    extensions of `PATHEXT` in its order, kept to `.exe`, `.bat` and `.cmd`, and a bare name
    is never handed to std's `Command`, which would look beside Pitboard first. `mod.rs`
    chooses the system, once: `macos/` (the keychain through `security`, `ps`, a cookie jar
    through the system's SQLite in `sqlite.rs`, a bundle's `Info.plist` in `bundle.rs`,
    launchd) or `linux/` (`/proc`, systemd, offered only where systemd started the system as
    sd_booted(3) tells, and whether it runs in WSL as Codex tells, which a build for tests
    reads from `WSL_DISTRO_NAME` alone), each with what `unix/` holds for
    both, or `windows/`. Linux and Windows refuse Claude Desktop's data folder before
    anything else is asked, so neither reads a cookie jar, and Linux lists no process with
    its parent.
    `windows/` is the face that refuses whatever of Windows is not built yet: none of the
    four calls that move Claude Desktop's data folder. It keeps Credential Manager's generic
    items as `Bun.secrets` writes Claude Code's, and reads, writes, deletes and lists them
    by name (`windows/credman.rs`); in a build for tests it refuses every family's name but
    the test's own slots. It seals each park with DPAPI under the person's own key
    (`windows/seal.rs`). It registers daily renewal's task through `ITaskService`, on a
    thread of its own in COM's multithreaded apartment, and a build for tests asks Task
    Scheduler nothing (`windows/tasks.rs`); `task.rs` writes and reads the task on every
    system, and keeps the proxy it is given in `schedule.env`, since a task's definition
    holds no environment. The task runs the path the command line was started by where an
    upgrade keeps it: Scoop's `apps\<app>\current`, never a version's folder, and winget's
    link in `WinGet\Links` (`program::lasting`). It lists processes from a Toolhelp
    snapshot, for their parents, and keeps this account's in any session: whose each is, WTS
    says without opening it, a process in another user's session is passed over unopened,
    and the rest's tokens are read (`owner.rs`). A program whose owner or path does not read
    makes `processes` unknown, since it may be any kind of holder. It finds programs as a
    Windows shell does, an app execution alias in a `WindowsApps` folder by its reparse tag,
    and reads the `Path` a program started now is given from the registry, the machine's and
    then the person's, which a build for tests reads in its own test alone, as it does the
    settings doctor names (`windows/settings.rs`): the distributions WSL holds, Windows' own
    proxy as hyper-util reads it for Codex, and `EnableLUA`. It starts a sign-in's tool
    suspended, puts it in a job of its own that lets a program break away, as Codex starts
    its daemons, then resumes it, so nothing it starts is outside the job
    (`windows/job.rs`). A cancel, and a sign-in let go of unfinished, end the job's
    `cmd.exe`, `conhost.exe`, `node.exe`, `claude.exe` and `codex.exe` and nothing else, so
    a browser the sign-in opened keeps running. Started by a Pitboard with no console, a
    tool gets no console window either. It also reads this process's token, through
    windows-sys (`windows/user.rs`), which `token.rs` makes an `Elevation` of and whose
    `TokenUser` is the person's SID, the account's own folders (`GetUserProfileDirectoryW`,
    `FOLDERID_Profile`, `FOLDERID_LocalAppData` and the profile type), which a build for
    tests answers nothing from, and the build `RtlGetVersion` gives, which
    `host::windows_floor` holds against 26100, the build of Windows 11 24H2 and Windows
    Server 2025. `windows/fs.rs` makes each private file and folder with a security
    descriptor from SDDL: owned by the person's SID, with a protected access list granting
    the person, SYSTEM and Administrators alone, which a folder passes on to what is made in
    it. A folder already there keeps its own list. `fs::access` reads a path's list, and
    `acl.rs` says who else it lets reach the data, by the names Windows shows, and the
    `icacls` lines that close it. `fs::rename` replaces a file whole: by the POSIX rename
    where the volume reports `FILE_SUPPORTS_POSIX_UNLINK_RENAME`, as NTFS and ReFS do, which
    lands over a file another program holds sharing its deletion, and by `MoveFileExW` with
    write-through on exFAT, FAT32 and an SMB share. `replace.rs` says which route a volume's
    flags give and which refusals are waited out: a sharing violation or access denied, for
    about 2.3 s. A read-only file is refused, never cleared. `fs::copy_access` gives what
    replaces another program's file that file's list, protected or inherited as it is, and
    `fs::sync_dir` flushes the folder a rename was made in, which a standard user may on
    NTFS, ReFS, exFAT and FAT32; an SMB share refuses, and there the write-through is what
    makes the rename last. `token.rs`, `acl.rs` and `replace.rs` are compiled on every
    system, so how a token and a list read and which route a volume takes is tested
    everywhere, and `words.rs` says each system's refusal from a `host::Os`. A fact that
    differs by system is a `match` on `host::OS`, such as the folders macOS asks about
    before an app may look in them, or `Os::default_pitboard_home`, where Pitboard keeps its
    files for a home when `PITBOARD_HOME` names nowhere else. `same_path_in_any_case` is how
    Windows compares paths, for the core and the apps alike, and `path_key_in_any_case` a
    key read from the text alone, which also reads `\\?\`, `.` and `..` away. `awake` is the
    time the machine has been awake, which the app's timers count.
  - `release.rs`: whether this build may do anything on its system. A Windows build of a
    0.x release may not, unless it was compiled with `--cfg pitboard_unreleased_windows`,
    which CI's Windows jobs set and no release does; macOS and Linux always may. The
    version is Pitboard's, which the command line and the app's library name with
    `set_version`, not the core's own. Every request's User-Agent names it too.
  - `home.rs`: Pitboard's own directory, and what every home must be before anything is
    read or written under it: a full path (`check_absolute`), and, for Pitboard's own, not
    in a folder that syncs (`check_location`). That is told by the text it always refused,
    such as `Sync`, anywhere in the path as written, so nothing it refused is let through,
    and by the names sync clients give their folders, one folder at a time and in any
    case, and macOS's `Library/CloudStorage` and `Library/Mobile Documents`. On Windows
    also by OneDrive's folders as its variables name them, a sync root
    (`CfGetSyncRootInfoByPath`), and a roaming profile but its `AppData\Local` and
    `AppData\LocalLow`.
  - `store/`: reading and writing logins, whichever store holds them: the chain rules, a
    file, the vault of files, the folders under `parks` and the stores in memory the tests
    use. On macOS, a parked Claude Code or Codex login is a keychain item. On Linux, it is a
    file in the vault. On Windows, it is a file in the vault sealed to the person's account,
    `store::Backend::SealedFile`: a versioned header, then a `CryptProtectData` blob, and
    `host/memory.rs` plays that vault on every system (`MemoryHost::sealed`). A parked
    Claude Desktop login is Claude's own files, in a folder of its own under
    `<pitboard home>/parks` with its manifest, its keys and its items, beside the folders
    Pitboard set aside. `store/folder.rs` is the only code that touches `parks`. It deletes
    a folder that holds an item only by name, for `park::FolderParks`, once the folder's
    account is forgotten.
  - `switch/`: every change to Pitboard's index (switching, by hand or by itself in
    `auto.rs`, enrolling, adopting, renaming, forgetting, renewing, repairing, abandoning,
    uninstalling, and putting away a login Claude Code left in a file behind the keychain or
    Credential Manager in `stow.rs`), and the journal that finishes an interrupted switch of
    a Claude Code or Codex login (`journal.rs`), which `settle.rs` settles before each
    change, gives up on, or reads for the refusal it would make. `identify.rs` says whose
    login Claude Code or Codex has stored: the record's owner where the login's refresh
    token has the fingerprint the record was made for, which asks nobody, and the tool's
    service otherwise (`whose`).
    A login whose access token has expired is never asked about with it, and one
    the service refuses is not asked about again (`told`): a change renews it first, and so
    does a read, where it takes `state.lock` without waiting and no switch waits to be
    finished (`told_by_a_read`); usage alone renews nothing, and a Codex login names its
    account by itself. `refresh.rs` is the one renewal of a login of Claude Code's where
    Claude Code keeps it, which that and `pitboard stow` both use, with `Refreshing`, Claude
    Code's refresh lock held, and `finish`, which saves a renewal a stopped run left in the
    vault, and `finished`, the same for one tool, which stops what acts on its login while a
    copy cannot be saved. A store with no login is recorded as holding none only where the
    tool's own record names nobody either; where it names somebody, the login is somewhere
    Pitboard does not look, and nothing is recorded. A change records what it finds under
    the lock it holds (`now`, or `look` then `record` where enrolling the account found
    names it first, or `look` then `keep` where the automatic switch has its own to do when
    it cannot be told); a read records it afterwards (`record_read`), only where it changed,
    only where it takes the lock without waiting, never while a switch waits to be finished,
    and only where the tool's own record names what it named and the store holds the login
    it asked about, both read again under the lock. Each writes an `in-use` line in the
    activity log for what changed outside Pitboard. With `test-support`, a context may hold
    a `SignInScript`, which plays a tool's own sign-in in place of its program, for a test
    or a fixture that may start none; everything around it is the core's own.
  - `switch/folder/`: every change to a tool whose login is its app's files. It names no
    tool: `provider::folder::Folder` is all it knows. A change asks the process list that
    the app is closed, holds the data folder, settles a move an earlier run left
    (`settle.rs`), reads whose session the folder holds (`verdict.rs`), and enrols it or
    moves it. `moves.rs` is the two halves of a move, a leave into a folder under `parks`
    and an arrive out of one, each recorded before its first rename. `awaited.rs` parks the
    account in use for a sign-in inside the app and waits, on a thread of its own, for the
    app to quit signed in.
  - `state.rs`: `state.json`, the index of accounts and where each one's login is parked,
    and `renewing`, the renewals of a tool's own login whose answer is not saved yet, at
    most one per slot. Three of its fields belong to a login kept as files: `moving`, a
    move under way; `aside`, the folders under `parks` no account holds; and `awaiting`, a
    sign-in inside the tool's app that Pitboard waits for.
  - `in_use.rs`: whose login each tool has stored, as its service last said, with the
    fingerprint of that login and the account the tool's own record named then. It is
    `state.json`'s `in_use`, written through `State::identified`. For Claude Code and Codex,
    a switch writes it, first for the login it finds in the store and then for the one it
    installs, and so do enrolling the account signed in, a sign-in put in use, finishing an
    interrupted switch and a read that finds it changed. For Claude Desktop, only a change
    writes it, with Claude closed, and its owner is the account Pitboard filed the session
    under, since no service names one. `known` is what every reader that does not read the
    store takes as the account in use: `status --offline`, the automatic switch's look,
    `doctor`'s accounts. It is the record, in doubt where there is none, where it came
    forward from schema 5, or where Claude Code's config names another account than when the
    record was checked against the store. For Codex it is the login's own claims, and the
    record where the login cannot be read. For Claude Desktop it is the record judged
    against the account `config.json` names, from one read of the settings
    (`switch::folder::read`). `Provider::own_record` reads Claude Code's config for that
    doubt and for the words that say it, never for whose a login is.
  - `duty/`: what every front end that acts by itself shares, whatever it does. `cadence.rs`
    is how often it looks at its files (`LOOK_EVERY_SECONDS`, 2), reads every account
    (`READ_EVERY_SECONDS`, 300) and decides with nothing written (`DECIDE_EVERY_SECONDS`,
    30), how long something that came to nothing waits before it is tried again
    (`retry_after`, 60 seconds doubling to 15 minutes), how many attempts are made at one
    thing (`ATTEMPTS`), and `Refusals`, a front end's refusals in a row and their wait, on
    its own clock. `ledger.rs` is `Tries`, the attempts and the reasons said about one
    thing, and how a duty's record is read and written, only under `state.lock`. `mod.rs`
    holds `Look`, what a duty's look at its files came to. The automatic switch and the
    warm-up are its duties.
  - `duty/warm/`: warming up an account, starting its five-hour period as soon as the last one
    resets, for the Claude Code accounts somebody turned it on for. `mod.rs` holds the
    setting, `warm.json`: the accounts turned on, by id, and `Range`, the span of each day,
    in minutes of the clock's time zone (`Clock::zone`), which may run past midnight. It also
    holds `Overview`, each Claude Code account with where it stands, both by itself and for a
    warm-up asked for now (`Choice::now`), and `Halt`, why nothing is warmed up on this
    machine: a switch interrupted, a custom OAuth endpoint, managed settings that sign every
    run of Claude Code in another way, or no program. `decide.rs` is the rule, from the
    reading Anthropic gave, which login of the account is at hand (`Login`), the span and
    `warmed.json`, today's attempts (`Ledger`, by the clock's day): a period running, or one a
    warm-up the reading came before started, is `Running`; a limit of the account's own used
    up holds it back, since Anthropic refuses every request then; after any attempt the next
    waits its turn (`retry_after`); and three attempts that waiting does not mend hold it back
    until the next day. Only the unscoped `session` limit is started: the weekly
    limit is a fixed cycle. `run.rs` runs the tool's own headless mode in `warm/<tool>/`, made
    afresh for each run and removed after it, with an environment built from nothing but
    `PATH`, the person's name, the proxy and, on Windows, `SystemRoot`, through
    `host::proc::run_tool`: the access token on descriptor 3 through a pipe `dup2`ed in
    `pre_exec`, for at most 120 seconds, its process group stopped past that, and whatever it
    left in the group stopped after it. On
    Windows, which reads no descriptor (`windows_token_handover`), the token goes in
    `CLAUDE_CODE_OAUTH_TOKEN` with `CLAUDE_CODE_SUBPROCESS_ENV_SCRUB=1` (`windows_token_env`),
    `CLAUDE_CODE_GIT_BASH_PATH` names the Git Bash `claude::paths::git_bash` finds, so Claude
    Code's own finder asks no `where.exe` with the token in its environment, `USERPROFILE`,
    `TEMP` and `TMP` are the private home's, and the run starts suspended in a
    job that kills on close and lets nothing break away, ended whole by `TerminateJobObject`
    once the run ends or passes its ceiling.
    `WarmScript` plays a run in a test or a fixture. `provider/claude/warm.rs` builds Claude
    Code's command and reads what it printed as a `Failure` or none. Codex has neither until
    when its period starts is measured.
  - `switch/warm.rs`: changes to `warm.json`, and the warm-up itself. Under `state.lock`, it
    decides again from the `Overview`, refuses where a `Halt` holds, takes `warm.lock` and
    records the attempt, so an account whose login cannot be had waits its turn behind the
    others; then takes the access token, of a park renewed where it lapses within 5 minutes
    (`renew_one`) or of Claude Code's login in use renewed under its locks
    (`identify::fresh`). It lets `state.lock` go while the tool runs, then asks the account's
    usage (`status::ask_one`), again 20 seconds on where the first answer has no period
    running, which says whether a period started. It records that under the lock again, the
    next attempt waiting from then, and writes the `warm-up` line.
  - `duty/autoswitch.rs`: switching Claude Code by itself, for a front end somebody asked to: the
    app with its setting on, or `pitboard watch`. `Threshold` is the share a limit switches
    at, 50 to 99, 95 unless chosen. `decide` is the rule, from the readings Anthropic gave
    that Pitboard already holds, `NoReading` where it gave none of the account in use. Every
    limit of that account at the share is judged, the furthest past it first: one already
    switched away from, or tried `ATTEMPTS` times, before it resets leaves the next to be
    judged, and where none is left it says why of the furthest at once. Where one is left,
    the account put in use within `SETTLING_SECONDS` holds it back, and so does an attempt
    at any limit that came to nothing, until its wait is over (`Waiting`); either is said of
    the first limit left, the one tried next. For the first limit left, it picks a switchable account with room under it in every limit it has, by
    `usage::room`, which the app's advice asks at 100. A limit a reading that lists every
    limit leaves out is one the account does not have. Any other reading leaves no room
    unless it gives every limit of the account in use, except a model's limit other than the
    one at the share where it gives the five-hour and weekly limits. `usage::preferred` puts
    an account without a weekly limit for all models first, since what it leaves unused in
    five hours is lost at the reset, and counts a reading that may leave limits out as one
    with it. Among those alike it takes the most room in that limit, then the least full in
    its others. Below
    the share it is `Below`, with the fullest limit still running. A limit at the share not
    switched away from is a `Hold` with its `Skip`: no room, naming the accounts it could go
    to that Anthropic gave no reading of, the limit already left, the attempts spent, the
    account settling, or Claude Code signed in another way. `judge` turns a decision into
    what a front end is told: `Auto::Watching` carries the account, its fullest limit, when
    that was read and until when its budget holds Anthropic's next answer off
    (`budget::held_until`). Each limit a decision gives carries the reset the ledger first
    kept its window under, where it keeps one, which a later answer may give a second apart,
    so a front end that tells one window from the next by it says once what was recorded
    once. `Ledger` is `autoswitch.json`, what was tried and said for each limit of each
    account until a minute after that limit resets, as late as another answer may give that
    reset (`usage::may_still_run`), or, with no reset, for a week after anything was last
    kept of it, as it is read and as it is written: the attempts, how many in a row failed
    for a reason waiting may mend, when the last began, whether one switched, the accounts
    passed over, and each `Skip` code said, with when; and `asked`, where whose login Claude
    Code has stored could not be told, when it was last asked, how many times in a row and
    why. A record of whose it is made since `asked`, by a read or a switch, ends that row.
    It is written only under `state.lock`. `look` judges from files alone, with the account
    in use from `in_use::known`: what stands (`Look::Stands`), or `Look::Act` where a switch
    may be due, that account is in doubt, or a `Hold` was not said yet in that limit's
    window. `Blind` is why nothing can be judged at all: a switch waiting to be finished, a
    custom OAuth endpoint, no login stored, a login of an account nobody enrolled, one
    nobody could tell whose it is, which `asked` paces as attempts are paced, or no reading
    Anthropic gave of the account in use. `asked` holds only what every front end would meet
    alike: a store this process cannot read, such as a keychain locked over SSH, is returned
    as its error, for its front end to pace by `retry_after`, the wait attempts and `asked`
    take. A front end decides with nothing written every `DECIDE_EVERY_SECONDS`, 30.
    `Auto::told_apart` is what every front end that says each outcome once tells one from
    another by: a `Skip` by the account, the limit, the reset the ledger kept it under and
    its code (`Skip::told_apart`), a `Blind` by its code and the email, the cause or the
    account it names, never by when it asks again (`Blind::told_apart`), and a wait by the
    account and when it ends. Nothing tells a switch apart, nor watching.
    `switch/auto.rs` tells whose login is stored under the lock, as a switch does, records
    it, decides once from that, and switches from that same login through
    `switch::switch_from`, or records the `Hold` once. An attempt that failed is judged
    again with it counted, and where that leaves no limit to try, its `Hold` is recorded
    with the failure. `service::Pitboard::auto_look` is the look and `auto_switch` joins
    the two. The audit log records a switch, or the error that stopped one, as
    `auto-switch`, of `claude` where no account was chosen yet; and as
    `auto-stay`, a login nobody could tell whose it is, of the account last known in use,
    once per wait, and each `Hold`, of the account in use, once per limit, reset and code.
  - `lock.rs`: the locks Claude Code takes around writes and renewals of its login, each
    taken the same way and kept with Claude Code's own numbers (`Timing`).
  - `context.rs`: what the core takes from its environment, read from a map of variables
    by the same code for every front end, apart from the `PATH` that `host/linux` reads to
    find the program daily renewal runs. On Windows a name is one in any case, and the
    first of two spellings wins, as Windows hands a program its variables.
  - `app.rs`: what an app finds for itself that a command typed at a prompt is given: where
    to look for each tool's program, on the login shell's `PATH`, or on Windows the
    registry's, and then where the tool's installers put it, which the core looks in each
    time it is asked, and the `pitboard` a terminal would run, there and then where each way
    of installing Pitboard puts it, under the app's home or on Windows beside the app, and
    where the system's package managers put programs, which Windows names by variables:
    npm's `%APPDATA%\npm`, winget's `Links` and Scoop's `shims`. And the files an app keeps of
    its own in Pitboard's directory, `told.json` and `app.json`, written as the core writes
    its own and read by nothing of the core, and `Open`, the `app.lock` an app holds for as
    long as it runs, which `uninstall` refuses over since the app would make the directory
    again.
  - `api.rs`: what every request shares, the agent each context's requests go out on,
    `api::agent`. The requests to Anthropic are in `provider/claude/api.rs`, and those to
    OpenAI in `provider/codex/api.rs`.
  - `proxy.rs`: the proxy those requests go through, read from the context's environment by
    the rules ureq 3.4.2 reads a process's with, the refusal of each request a SOCKS proxy
    would have carried (`proxy::Refusal`), what `doctor`'s `network` check says of it, and
    the variables a renewal schedule installed from that context is given. It reads Windows'
    own proxy setting too, which only doctor shows (`system_proxies`). See
    [ureq's proxies](docs/facts/ureq.md).
  - `service.rs`: `Pitboard`, each operation run as every front end must run it. It
    re-exports `Permit` (`permit.rs`, with the gate), `Warning` (`warning.rs`) and `Stored`
    (`in_use.rs`), so the front ends name them through it.
  - `usage/pace.rs`: how a limit's use compared with an even use across its window when it was
    read: the part of the window gone by then, how far the share used was from it, which
    side of a 5-point margin it was on, and for a limit over pace when it runs out at the
    window's own rate since it began, counted down to the moment it is told. A pace is its
    reading's: set against a later moment, the same share drifted towards under pace while
    nobody read the limit again. `first_to_run_out`, `lasts` and `until_reset` say it of an
    account, and an account not in use lasts until its first reset. Every surface asks it,
    so `status`, `--json`, the status line and the app say the same thing. It replaced a
    rate taken across fourteen days of readings in `readings/`, through every reset in
    them; `home::remove_retired` deletes the files that kept, and the folder where nothing
    else is in it.
  - `usage/mod.rs` and `usage/readings.rs`: an account's usage, and `usage.json`, the one reading of
    each account that every front end records into and shows. The service's answer about an
    account founds its reading and replaces what it gives (`usage::answered`), and a share a
    session moved after the answer was taken stands. Anthropic's answer lists every limit an
    account has, so where `usage::from_usage_object` read every row of its `limits`, the
    answer says so (`Snapshot::lists_every_limit`) and a limit it does not give is gone at
    once. Any other answer leaves a window it does not give standing, in its place, until
    its reset: one with a row that did not normalise, one in the older named shape, and
    every one of OpenAI's, which gives two windows, either of them null, with nothing
    measured to say what a null one is. A Claude Code session's numbers only move a limit
    an answer gave (`usage::moved`): a higher share in the same window, or the next window
    once the last has reset. They never add a limit or found a reading, since they do not
    say whose they are. A reading's `answered_at` is when its service last answered, absent
    from one an older Pitboard wrote. A limit's kind is a `usage::LimitKind`, read where
    Anthropic's answer, a session's status line and Codex's answer are read, so `five_hour`
    and `seven_day` are `session` and `weekly_all` from then on.
  - `status.rs`, `doctor/`, `statusline.rs` and `schedule.rs` serve the commands of the
    same names. `doctor/` has a module per concern, each with its facts, its gather and its
    judges: `mod.rs` makes the checks in the order `pitboard doctor` prints them, and
    `words.rs` says them. A row's numbers in `status.rs` are its service's answer folded into the
    reading every front end records in `usage.json`, or that reading alone where no answer
    came, and never a tool's own cache (the register's `usage_cache_stamp_is_the_configs`). A Claude Desktop
    row has no numbers, since no usage of it is read, and says when a change last read its
    login in use (`read_at`). A read asks first (`ask`), and makes its rows (`report`) once
    whose each login is has been recorded, so an account whose only login a sign-in outside
    Pitboard replaced reads `login_replaced` from that read on. A row whose parked login its
    service refused (`login_refused`) is not switchable (`Row::switchable`), which
    `pitboard status` and the app go by. The usage of a login nobody
    could name is not asked, and the account the record names stands in for it. Offline rows
    (`gather_offline`) take the account in use from `in_use::known`, an account nobody
    enrolled included, which gets its row as it does online. `status::standing` says what
    stands on every read: `login_replaced`, `in_use_unconfirmed` where the read could not
    ask and Claude Code's config has moved since the record was checked, or the record was
    never checked, whatever the config names (`Known::unconfirmed`, which doctor's `in_use`
    check follows too), and `config_names_another` where it names another account than the
    one in use.
    `statusline.rs` files what a Claude Code session passed under the account its windows
    prove: the one account whose reading, as Anthropic last answered for it, holds a window
    the session passed and does not rule the session out. A reading rules it out with
    another running window of a limit passed, or by listing every limit its account has and
    not one passed. It files only what moved since the session's last run, which
    `sessions.rs` keeps. Numbers that prove no account are shown as the session's and filed
    nowhere, under the account in use from `in_use::known` marked `?`, or under none where
    that account's reading rules the session out. A session that passed none shows the
    account in use, marked while the record is in doubt. Where the record names no enrolled
    account, both show `unenrolled`, or `?` while the record is in doubt. `schedule.rs`
    decides what daily renewal runs and whose it is, and refuses a program in the temporary
    copy macOS runs an app from, or Explorer one opened inside a zip, by
    `in_a_temporary_copy`, which an app asks of the command line inside it too; the host's
    scheduler writes it. `watch_service.rs` decides the same of the watch service, a second
    job on that scheduler (`schedule::Job`).
  - `words.rs`: the sentences and column words Pitboard says in more than one place, each
    a function of typed values: spans of time, a limit's names, when it resets, its pace and
    when it runs out, a parked login's life, a renewal run, doctor's summary, and what is
    left in a file behind the keychain, or Credential Manager on Windows, and what putting it
    away does and did (`left_lines`, `stowed_lines`, `nothing_left`, each in the words of the
    system it is given), which `pitboard stow` and the app's sheet both say, what becomes
    of running sessions after a switch (`after_a_switch`), why replacing a login refused for
    good loses nothing (`replacing`) and that Claude's settings name another account
    (`desktop_named`), each taking the account as the front end names it. It also holds
    `usage_level`, the steps at which a limit's colour changes, and the helpers every front
    end writes with, `capitalised`, `listed` and `figure`; `time.rs` holds the units of a
    span, and `state::Key::parse` reads back the name `Key::typed` writes. A thing said both
    in a column and in a sentence has a function for each form. The command line calls these
    functions directly, and so does `pitboard-ffi`'s `present/` as it makes the snapshot,
    which carries what an app shows of them. The bindings export none of them. Clock times
    are not in it. A sentence that says to run a tool names its program as `spelled` gives
    it: on Windows, the `claude.cmd` or `codex.cmd` npm installs where Pitboard found that,
    since Windows PowerShell 5.1 under its default policy refuses the `claude.ps1` beside
    it, and the bare name everywhere else. An error, a warning, a status row, the report and
    each kind of process found running a tool (`holder::Holding`) carry that name, since their
    words are made with no context.
- `crates/pitboard`: the command line. Arguments, rendering for people, the man page, and
  the `--json` contract, pinned by the snapshots in `crates/pitboard/tests/cli/snapshots`.
  `build.rs` gives a Windows build its manifest, which the linker embeds, and its version
  resource, which `rc.exe` or `llvm-rc` compiles, both through `build/versioninfo.rs`.
  `json.rs` is the one writer of that JSON, for every command and every error. Its example
  `stand-in` is the program every test starts in place of a tool: `claude`, `codex` and
  Claude's own program under a test's scratch `Claude.app`, which `ps` lists as Claude.
  Its code is `pitboard-core`'s `stand_in.rs`, compiled with test-support, and it plays the
  script written beside each copy of it. Nothing installs or ships it. A test's Claude data
  folder is written as Claude leaves one by `pitboard-core`'s `desktop_testing.rs`,
  compiled the same way. A command a test runs finds that folder and that `Claude.app`
  through `PITBOARD_TEST_CLAUDE_DESKTOP`, which only a build for tests reads. `watch.rs` is
  `pitboard watch`, a loop in the foreground until Ctrl-C: it reads every account every
  300 seconds, as the app does, looks every 2 seconds at when the index and the readings
  were last written, and decides again whenever either changed and at least every 30
  seconds. It says every switch, and each thing that stopped one once until the next
  switch, told apart as the core tells it (`Auto::told_apart`): a reason not to switch away
  from a limit once for that limit and its reset, a reason it cannot judge once for what it
  names, and a wait once for when it ends. An error is said once for its code. It says the
  account it watches, `idle` with its fullest limit, when that was read and until when
  Anthropic holds Pitboard off asking again, whenever its last line said something else.
  Each warning a decision found is said once until the next switch, even one that went away
  and came back, with that decision's line, which is said again to carry it where it was
  said before. A refusal is paced as the app paces one: no decision under the lock until
  `duty::cadence::retry_after` of the refusals in a row has passed, only the look meanwhile,
  and any outcome ends the row, as the look standing on what the core recorded with an
  attempt that failed, its own wait or why it tries no more, does. It ends on a refusal
  watching cannot mend: running elevated, a Windows build or Pitboard's own files unusable.
  At each decision it also asks the core to warm up whichever account turned on is due
  (`Pitboard::warm_up`, which looks at the files first), with a refusal row of its own, and
  says each warm-up, each reason an account is held back once until it warms up, and why
  nothing is warmed up here once until that ends. A warm-up that came to nothing is said once
  until that account warms up, by the rule the app keeps too (`switch::warm::Told`): told
  apart by the account and the code, never by Claude Code's words. No warm-up error ends it.
  `warm.rs` is `pitboard warm`. `build/versioninfo.rs` is the Windows version resource SignPath asks of
  what it signs: ProductName `Pitboard`, CompanyName `Pitboard` and the crate's version, and
  `pitboard.exe`'s manifest. The build scripts of `pitboard-ffi` and `pitboard-share-ffi`
  include it by `#[path]` and link the resource into their DLLs, compiled by the Windows
  SDK's `rc.exe` or by `llvm-rc`. Its FileVersion is `X.Y.Z.N` for beta N and `X.Y.Z.1000`
  otherwise, so each version numbers above the one before. A build without a resource
  compiler warns and goes on, unless `PITBOARD_REQUIRE_VERSIONINFO=1`, which CI's `windows`
  and `windows-app` jobs and the release set.
- `crates/pitboard-ffi`: the core as UniFFI bindings, for the apps: a static library for
  the macOS app, a dynamic one for the Windows app. An app reaches the core through the
  model alone.
  - `lib.rs` declares what the bindings export beside the model and the account windows'
    rules, and the records the model keeps of what the core answered, such as `Status`,
    `Account` and `Abandoned`, which no snapshot carries. The macOS app calls three of its
    free functions, none as it draws: `app_command_line`, for the command line inside its
    bundle, as it starts and as it links that one; `can_run`, whether that one runs, as it
    links it; and `pitboard_directory`, for where `app.json` is and the key the account
    windows' records are kept under, as it starts. The Windows app calls two more as it
    starts: `launch_refusal`, what the gate every change passes refuses once it has named the
    app's version to the release gate, the release gate first, then an elevated process, a
    Windows build below 26100 and a home that is not a full path, which it shows in place
    of a model; and `windows_records_key`, its records' key. `Pitboard.exe --unregister`
    calls `unschedule_app`, which takes the renewal task away where it runs the command line
    beside the app. The snapshot carries what an app shows of the tools, a sign-in, the
    command line a terminal runs, a limit and its pace, a parked login and doctor's checks,
    made by `present/` and the model's lanes, so no free function says it again. The
    helpers `present/` makes it with here, `tools`, `usage_level` and `doctor_summary`, are
    the crate's own.
  - `launch.rs` is the core the model's lanes call, `AppCore`, which nothing exports. It is
    made from the `AppLaunch` the app was started with, read as `AppContext::discover`
    reads it, the first time a lane needs it, and made once more a while after a login
    shell too slow to answer. Every read a lane makes with it asks first, as the command line
    asks before every command, whether this build may do anything on its system and whether
    every home is a full path, and answers that refusal, or nothing where it has no way to
    say one. Beside it are what it answers that only the model reads, such as what a switch,
    an enrolment or a renewal came to, doctor's checks, and `PitboardError`.
  - `sites.rs` gives both apps `pitboard-sites`' sites and links as records of their own,
    and says a site's sign-in steps as a window on this system can follow them: a window on
    a Mac cannot use a passkey.
  - `account_windows/` holds the rules of an account's window that each app's web code
    asks as its engine asks it: which accounts have a window and the store each one's data
    is kept in (`stores.rs`, `accounts.rs`), where a page may go and what becomes of a
    response (`policy.rs`), what a window says (`notes.rs`), what a page may do and how its
    dialogs say who asks (`pages.rs`), and what a download is called (`downloads.rs`). What
    differs by system there, such as how a copy of a file is numbered, which names are one
    file, or an alert's "on this Mac", is a `match` on `host::OS`, as what a window cannot
    sign in with is in `sites.rs`. Each takes the system as an argument, so every system's
    answer is tested on every system: Windows says "this PC", numbers a copy
    `report (1).pdf` and `archive (1).tar.gz`, takes names in any case and in no other form
    for one file, and promises no passkey. `records.rs` reads and writes the windows'
    records, the stores each Pitboard directory made and the page each window was last on,
    which the model keeps in `windows.json`.
  - `model/` is the app model the macOS app shows and the Windows app is to show. An app
    makes a `PitboardModel`, sends it an `Intent` for each thing asked of it, and its
    `ModelListener` is told of each numbered `Snapshot`; its `AppControl` quits and opens
    other apps, and its `Notifications` posts a limit at the share chosen, a login to sign
    in to again and what switching or warming up by itself came to. Each notice carries the
    text a click on it, or on its one button, hands back as `Intent::NotificationClicked`,
    which the model reads as one of four requests and nothing else. `state.rs` holds what
    the model knows and decides what follows each message. What follows an answer to the
    quit question is `After`, private to the model: a switch, a sign-in inside an app, or
    recording the account an app is signed in to. `advice.rs` says which account to offer
    once the one in use has run out, `preferences.rs` what the app's own preferences are,
    `machine.rs` what the model knows of this machine rather than its accounts, and
    `windows.rs` the account windows' bookkeeping; `lanes.rs` runs what it
    decides, on a lane of reads, which reads the schedule, doctor's checks and the log too
    and asks the core's look whether to switch Claude Code by itself, a lane of changes, one
    at a time, which also repairs and changes the schedule, a lane that lists processes and
    asks the app's `AppControl` about other apps, a lane that asks what is installed, reads
    whose login a tool stores once somebody asks (`Job::ReadSignedIn`) and looks for the
    `pitboard` a terminal runs, a thread of its own for each
    sign-in, a lane that types a code back to one or stops it, a lane that reads and writes
    what the model keeps, in Pitboard's directory and the windows' records in the app's own,
    and one that posts through the app's `Notifications`, and tells the listener on a thread
    of its own; `mod.rs` holds the exported types and the actor thread that owns the state.
    So far the model reads the accounts, looks every two seconds for a change made
    elsewhere, asks which tools are installed, switches, quits the app holding a tool's
    login when the person lets it and opens again the copy it quit, gives up on a stuck
    switch, keeps what each tool's last switch said, runs Claude Code's and Codex's own
    sign-in, parks the account in use for a sign-in inside Claude and waits for Claude to
    quit signed in, enrols the login signed in now, renames and forgets, writes the account
    in use into Claude Code's config, puts away a login left in a file behind the keychain
    or Credential Manager from a sheet that looks first, keeps the sheet over the main
    window, says which account to switch to once the one in use has run out, notifies once
    for each limit and reset at the share chosen, switches Claude Code by itself where that
    is chosen, asking the core's look after every read and every 30 seconds and saying under
    the setting what it came to, warms up the accounts turned on on a lane of its own,
    asking after every read and every 30 seconds, with every read's `Overview` of them, or
    why it cannot be read, and a notification of a warm-up by itself that came to nothing,
    once until that account warms up (`switch::warm::Told`), keeps the app's own preferences, and keeps the daily renewal
    schedule, makes doctor's checks, reads the activity log and finds the `pitboard` a
    terminal runs. It keeps the
    account windows' books too: which store is whose and each window's last page, which
    windows close and which stores go after a read, the link waiting for an account, with
    the wait before it can be opened, and the downloads. Its tests are files of their own
    there: `reading.rs`, `switching.rs`, `automatic.rs`, `signing.rs`, `changing.rs`,
    `stowing.rs`, `advising.rs`, `keeping.rs`, `maintaining.rs`, `presenting.rs`,
    `windowing.rs`, `warming.rs` and `cadence.rs` drive the state by hand, `lanes.rs` has
    the lanes' own, and `threaded.rs` drives the model through its threads over the real
    core.
  - `present/` makes each `Snapshot` from the model's state: `present` takes the state and
    the moment, and builds every sentence and row the menu bar, the menu and the window
    show, so a view decides nothing. `accounts.rs` is the menu bar's words and the
    accounts' sections and rows, with their limits and what each one's own menu offers,
    dimmed where it cannot be chosen now, which a shortcut menu leaves out, and the macOS
    Account menu's items while no account is selected, `notices.rs` the notices, their order and what the menu says of them, `setup.rs` the
    footing, the step it asks for and what stands in for an empty list (a tool nothing is
    enrolled of is read only once its step is taken up, `Intent::NameSignedIn`), `sheets.rs` the
    sheets with their default buttons' words, the quit question and a failure's alert, with
    `check_name`, the rule a sheet's default button and the model both save by, which says
    why what is typed cannot be a name or gives the intent the button sends,
    `machine.rs` what the settings and the window's other panes show of this machine, daily
    renewal, doctor's checks, the activity log and the command line,
    `windows.rs` the account windows, what a window says until it can show its page, what
    the **Open Link** window says, and the question asked before quitting stops the
    downloads an app has under way, and `words.rs` the sentences both apps say and the
    command line does not, each a function of typed values. What the command line says too
    is `pitboard_core::words`', called from there, such as a renewal's note and doctor's
    summary, and each such sentence, and each error and warning without words of the app's
    own, passes `words::typeset`, which curls its apostrophes, contracts the "not"s the apps
    contract and drops its backticks, as Apple's and Microsoft's style both write an app's
    text. A clock time, and a change's date
    and time, are the person's to read, so they are asked of the app's `LocalTime`.
    A button the snapshot offers comes with its words beside the intent it sends, and with
    whether it can be pressed now where it can be held back, so a view never words an
    intent or decides whether to offer one: a row's action, what an account's own menu
    offers (`AccountItem`'s `offers`, `windows` and `forget`), a pane's Try Again, a
    window's Clear and a sheet's default button. A control each app always has, such as its
    toolbar's "Add Account…" or "Quit Pitboard", held back where the snapshot says what it
    waits for is under way, and what is about the app's own system or done by the app alone,
    such as Copy Email Address, stay the app's.
  - `fixture/` holds the fixtures, the worlds a debug build of either app and its UI tests
    launch into by name, as the macOS app's debug build and UI tests do. Only the `fixture`
    feature compiles them, and it enables `pitboard-core`'s `test-support`. `worlds.rs`
    makes each of the fifteen on the real core: a home in the fixture's own folder in the
    temporary directory, `MemoryHost` and `ScriptedApi`, and every account put there by the
    core as a person would have, signed in, enrolled, parked and switched on a clock set to
    when it happened. `claudeDesktop` and `claudeDesktopOpen` hold Claude Desktop only where
    Pitboard supports it, on macOS: `desktop/home` enrolled from a terminal, then
    `desktop/work` added by a sign-in inside Claude, which parks home, while the core's wait
    asks every 10 milliseconds. `tools.rs` plays Claude Code's and Codex's sign-in through
    the core's `SignInScript`, with the person at the browser. `apps.rs` is the fixture's
    other apps and its notifications, which post nothing. Its ChatGPT runs Codex's login and
    quits when asked, from its bundle on macOS and its Store package on Windows. Its Claude
    is `testing::desktop::FakeClaude` over the fixture's machine, which writes the data
    folder as 2.7032.0 does as it quits. It quits when asked, and in `claudeDesktopOpen`,
    opened in front for a sign-in, it signs in to a third account after 5 seconds and quits
    2 seconds later. `pages.rs` is the stand-in pages an account window loads on
    `pitboard-fixture://`, from the site table. `mod.rs` holds what every build exports of
    them, the same in each: `PitboardModel::fixture`, `PitboardModel::fixture_in`,
    `fixture_names` and `fixture_page`, which without the feature refuse, naming it, or name
    none. `fixture` makes its model over the fixture's apps and notifications; `fixture_in`,
    for a test of an app's bindings, over the `AppControl` and `Notifications` it is handed,
    as `new` takes them. `tests.rs` is where each fixture starts, the tools and apps it
    stands in for, and what each UI test reads in its world.
- `crates/pitboard-sites`: the sites an account's window opens, and what a link from outside
  may be. A leaf, with no I/O and nothing of the core, whose one dependency is `url`, for
  IDNA alone.
  - `site.rs` declares each site as values: its host, the tool whose accounts it serves and
    the hosts that redirect to it. The hosts its sign-in goes to, the hosts it blocks, its
    sign-in paths, its store name, its sign-in steps and whether its sign-in offers a
    passkey are values too. Nothing else names a site, so a site is added to `ALL`, with its
    fixture page and tests.
  - `link.rs` checks a link from outside: a site's own host or alias, over `https`, with no
    port or user information, and never a sign-in path. `LinkRefusal` says why one is
    refused, in the sentence every front end shows.
  - `handoff.rs` writes and reads the Pitboard link, `<scheme>://open?url=<link>`.
  - `address.rs` splits a link as Foundation's `URLComponents` does, which is how the macOS
    app read one before the rule was Rust.
    [Foundation's URLs](docs/facts/foundation-urls.md) has what was measured.
  - `web.rs` reads a page's address, for an account window's rules, as Foundation's `URL`
    does: its scheme, and the host, port and user it names, with the host a request goes
    to, and its path, which a fixture's stand-in page says.
- `crates/pitboard-share-ffi`: `pitboard-sites` as UniFFI bindings with one function,
  `share_link`, a static library for the macOS Share extension and a dynamic one for the
  Windows app's `PitboardShare.exe`. It checks a shared page and writes the Pitboard link
  that hands it to the app, or says why not in the sentence the app shows. It holds nothing
  of the core.
- `crates/uniffi-bindgen-swift` and `crates/uniffi-bindgen-csharp`: generate the Swift and
  the C# bindings with exactly the UniFFI version the library uses. The bindings check
  method checksums when they load. The C# generator is NordSecurity's, pinned to a release
  built against that UniFFI; it is a build tool, so `deny.toml` leaves it out of the graph.
  Every type the core's bindings export is declared in `pitboard-ffi`, because the C#
  generator cannot use a type from another crate. `pitboard-share-ffi`, which Swift and C#
  both read, declares its own.
- `crates/pitboard-conformance`: checks a tool's register against a build of that tool: a
  program for macOS, Linux or Windows, or an Electron app's `app.asar`.
- `crates/pitboard-probe`: the Windows measurement probe. It is `publish = false`, never
  built by `release.yml`, and has a Windows-only measuring half (`win/`) beside a
  cross-platform half (the command line, the report shape, the write guard, the Credential
  Manager and process-image allowlists, the redactor, the logon and elevation naming, the
  PE reader), so that half is unit tested on every system. One subcommand per measurement
  the runner facts need; `.github/scripts/runner-facts.ps1` runs them in `windows-facts.yml`.
  Every writing subcommand refuses unless the account carries a throwaway marker, and
  refuses a scratch path that is relative or keeps a `..`, or that reaches a real login
  folder or file or Claude Desktop's folders, in any spelling Windows opens or by the
  identity of a folder on the way; every file it is named carries the `pitboard-probe-`
  prefix. `credman-names` asks Credential Manager only for the live login families' and
  `pitboard-*` prefixes and never reads a blob out. Reports name principals by their
  relation to the token rather than by SID, print paths with the profile folder and the
  account name replaced, and never hold two keys that differ only in case, which
  PowerShell's `ConvertFrom-Json` refuses. `helpers/` holds the Bun and Node scripts the
  runner facts run beside it, and `sparse/` the sparse-package manifests, script and steps
  the owner runs in the VM, which the runner facts' CF8 rows try on a runner too.
- `apps/`: the native apps, one directory for each system.
- `apps/windows/`: the Windows app. `Pitboard.Core` is the core's C# bindings as an
  assembly of their own, generated into `Generated/` and not committed, and
  `Pitboard.Core.Tests` calls the core through them. `Pitboard.Share.Core` is
  `pitboard-share-ffi`'s, generated the same way, beside `Sharing`, what `PitboardShare.exe`
  answers a share with, over an `IShareOperation` that `Pitboard.Share.Core.Tests` replaces,
  and `Handoff`, how it starts `Pitboard.exe` with the link. `Pitboard.slnx` holds the four,
  which build on every system. `Pitboard.Windows.slnx` adds the app, which builds on Windows
  only:
  - `Pitboard.Windows` is the glue, with no XAML. `AppModel` holds the `PitboardModel` and
    its newest snapshot, a property for each part, by the macOS `AppModel`'s rules; it
    compares a part's arrays by what they hold (`Structural`), and `ModelListening` hands it
    each snapshot on the app's `DispatcherQueue`. `WindowsLocalTime` is the person's clock.
    `Identity` names everything a build is known by, Debug or Release. `Launching` decides a
    launch before WinUI starts: the arguments, then `launch_refusal`, then the
    single-instance key. `HiddenWindow` hears the session's end, the system's broadcasts,
    `WM_CLOSE` and the icon's clicks, which `IconEvent` reads, and `Shell` is the known
    folder, the AppUserModelID, the app's icon and the refusal's message box. `NotifyIcon`
    keeps the icon in the notification area through `INotificationArea`, which
    `ShellNotificationArea` sends to `Shell_NotifyIconW` and tests replace, at the size
    `IconImage` loads it at for the primary display's scale and in the look the menu bar's
    level and attention give it, and `IconMenu`
    lists what the icon's menu offers, in the macOS menu's order. `WindowsNotifications`
    posts the model's notifications through `IAppNotifications`, which
    `SystemAppNotifications` sends to `AppNotificationManager` and tests replace, and refuses
    one Windows did not show by the exception's type and HRESULT alone; only a
    Release build registers, and a click comes back as `Intent::NotificationClicked` with the
    text the button or the notification itself carries. Its `Read` is why Windows shows none
    of them, from the
    manager's `Setting` at each settings visit, as a `NotificationsBlocked`. `WindowsApps` is the model's `AppControl`: through
    `PackagedApps` it finds the process of a Store app's package that owns a top-level
    window, asks Restart Manager to end it, never forced, through `IRestartManager`, which
    `SystemRestartManager` sends to Restart Manager and tests replace, and opens the app by
    its AUMID through the activation manager. `SparkleUpdates` is a Release build's updater:
    WinSparkle, through `ISparkle`, which `WinSparkle` sends to `WinSparkle.dll` and tests
    replace, over the feed of full releases, with WinSparkle's settings in
    `UpdateSettings`'s file in the app's data folder and the version of an update it finds
    read from the feed by `UpdateFeed`. `NoUpdates` is the updater of a build that cannot
    update, and says why.
    `StartAtSignIn` is the settings' start at sign-in over an `IStartupItem`: `RunValue`,
    which writes the `Run` value, `"<exe>" --sign-in`, through `IRunKey`, sent to the
    registry by `RegistryRunKey`
    and replaced in tests, refuses in a Debug build, and a fixture gets
    `FixtureStartupItem`, which `PITBOARD_FIXTURE_STARTUP=off` makes a value Settings turned
    off. It reads `StartupApproved\Run`, which only Settings > Apps > Startup writes: where
    that turned the `Run` value off, the switch is off and held back, its note says to turn it
    on there, and a link opens `ms-settings:startupapps`. The settings read it again at each
    visit and as `MainWindow` is activated while they show. Each read and change is said to
    the model as `Intent::OpensAtLogin`, on where Settings turned it off too, for its setup
    tip, whose button `AccountsPage` turns into a change of its own, or into Settings where
    Windows refuses it. `SettingsViewModel` is what the settings page shows and sends,
    beyond the model's words its own, each warm-up account a `WarmAccountRow`, and
    `Versions` the app's own version. `AccountsViewModel`, `ActivityViewModel` and `MachineViewModel` are what
    the accounts, activity and This PC pages bind to, told part by part as `AppModel` tells
    its own. `Keyed` keeps a shown row while the model lists it, so
    `ShownAccount` keeps its selection across snapshots, and sends its row's action. `WindowAlert` is the failure or quit question in
    front of the window, `SheetPage` what the sheet over it shows and sends, and `Countdown`
    a notice's timer. `IconCommands` turns the icon's click, its menu and each choice there
    into an intent or what `IIconTargets` does, and asks before Exit whether Pitboard may
    quit. `Lifetime` ends Pitboard: as the session ends it removes the icon and stops the
    updater, the model and the notifications, and at Exit stops the downloads first.
    `WindowPlacement` keeps where the main window was closed, and `Ui.Guarded` writes the
    exception that ends a dispatcher callback before throwing it on. The account windows'
    WebView2 glue is mapped under [Account windows](#account-windows).
  - `Pitboard.Views` is every window of the app's and what they show, a WinUI library so the
    test app shows them as `Pitboard.exe` does. `MainWindow` is a `TitleBar` over a
    `NavigationView`, whose `Frame` shows a page for each of the model's `Pane`s and the
    settings page, each navigated to with a `PaneContext` and kept for the next visit; it
    takes Ctrl+N, Ctrl+comma, Ctrl+W, F1, F6 and Shift+F6 from any of them. `AccountsPage` lists
    the accounts under their notices and setup step, each an `AccountRow` with an
    `AccountMenu`, and shows an `EmptyState` in place of an empty list; `ActivityPage` is the
    log, a table whose column headers are its list's header, each change stacked under its
    date on a page narrower than 720 epx; `MachinePage` is this PC's checks under doctor's
    last line in an InfoBar that scrolls with them; and `SettingsPage` is the settings, in the
    macOS Settings' sections, of the Community Toolkit's SettingsCards and SettingsExpanders.
    A notice is a `NoticeBar`, an InfoBar. `DialogHost` shows the ContentDialogs
    of every window one at a time, each in its window's theme, and `WindowAlerts` puts the
    model's alerts in one, or leaves them to the sheet's `AlertBar` while a sheet is up.
    `WindowSheets` puts the model's sheet up as an `AccountSheet`, a ContentDialog that adds
    an account, follows its sign-in, names, renames or puts away a login left in a file, and
    tells the model when somebody closes it, telling one sheet from another by the model's
    `Sheet`. `IconMenuFlyout` is the icon's menu as a MenuFlyout, its rows at one height, and
    `MenuHost` the pixel of a window it opens from. `AccountWindow`, with the `SignInWindow`s
    it owns, and `OpenLinkWindow` are the account windows and the **Open link** window, which
    `AccountWindows` opens. `WindowChrome`
    gives each window the app's icon, a title bar in the app mode, or in a contrast theme's
    colours, which `SystemContrast` reads through `IContrastTheme`, and a minimum size, and
    `Styles.xaml` holds the styles the views share.
  - `Pitboard.App` is `Pitboard.exe`, unpackaged and self-contained, carrying the icon that
    `Icon/make-ico.py` writes from the drawings beside it. `Program.Main` decides the
    launch. On `--unregister`, which the MSI's full uninstall runs, `Unregistering` deletes
    the `Run` value naming this `Pitboard.exe`, asks the core's `unschedule_app` to take away
    the renewal task where it runs the `bin\pitboard.exe` beside it, and deletes the
    notifications' registration: it asks `UnregisterAll`, then deletes the `AppUserModelId`
    key and the class it names by name, which `UnregisterAll` left on the session machine. It
    exits with no window and no single-instance key. On `--unregister-share`, which the MSI's
    full removal and rollback run, `SharePackage` removes `PitboardShare`'s package
    registered for this folder, the same way. `App` is the composition root: it makes
    the model, over this PC or in a Debug build started with `PITBOARD_FIXTURE` a fixture's,
    the icon, the notifications, the updater, `AccountWindows`, `IconCommands` and
    `Lifetime`, opens the main window when the model asks, and asks for it on a launch from
    Start or a shortcut, since Windows puts a new icon in the overflow; a start at sign-in,
    after an update or from a notification opens none. A Release build asks, through
    `RegisterApplicationRestart`, to be started with `--updated` after an update Restart
    Manager closed it for, and never after a crash, a hang or a reboot. Exit closes the main
    window first, so its placement is kept, and ends through `Lifetime`, which ends what
    the session's end does too, or ends where Restart Manager asks.
  - `Pitboard.Windows.Tests` is a WinUI app that hosts its own tests, the views' among them
    over snapshots made in C#, and `Pitboard.UITests` drives the built app with FlaUI.
    Neither registers anything with Windows but the Release build `ReleaseGateTests` starts at
    1.x, which `--unregister` takes back, and neither starts a Restart Manager session, which
    Restart Manager keeps under the user's HKCU. The icon's menu is held by the view
    tests, `IconMenuTests` and `IconMenuFlyoutTests`, and the click on the icon by the
    session's step A2. `ScreenshotTests` takes the docs' pictures of the window and of the
    menu, which it opens with the message Explorer sends on a right click.
  - `Pitboard.Share` is `PitboardShare.exe`, the share target: Native AOT, with no window,
    and the only program of Pitboard's with a package identity. Its manifest's `<msix>`
    element names the sparse package in `Package/`, the publisher written in at build time
    from `WINDOWS_SIGNER_SUBJECT`. Started by the share sheet, it hands `Sharing` the share
    operation through `SystemShareOperation`, and `Handoff` starts the `Pitboard.exe` beside
    it with the Pitboard link as its only argument. For the MSI, `--register` adds the package
    beside it with its own folder as the external location. `Pitboard.exe --unregister-share`
    removes the package registered for its folder, since removing a package ends the
    package's own processes. It loads `pitboard_share_ffi.dll` and none of the core.
  - `Pitboard.Installer` is the MSI, one per architecture, built with WiX v7 from the
    Release publish, `pitboard.exe` and the PowerShell completions the zip holds beside it.
    A dual-purpose package, per user unless told otherwise, as single package authoring has
    it, and its launch condition refuses a per-machine install, so per user
    `ProgramFiles64Folder` is `%LOCALAPPDATA%\Programs` and HKMU is HKCU: `Pitboard\` with
    `Pitboard.exe`, its runtime and `pitboard_ffi.dll`, the command line in `Pitboard\bin\`,
    never beside `Pitboard.exe`, and the completions in `Pitboard\completions\`; `bin`
    appended to the user `PATH` and taken off on uninstall; the Start menu shortcut with the
    AppUserModelID `Pitboard.Pitboard`, its key path under HKCU itself for ICE57; and
    `pitboard://`.
    `Manufacturer` is `Pitboard` and the x64 and ARM64 MSIs share one `UpgradeCode`, so
    either replaces the other. Settings offers no Modify, since there is one feature and no
    UI. `MajorUpgrade` replaces the same version, since the MSI's version is `X.Y.Z` for a
    final and `X.Y.Z.N` for beta N, as `windows-versions.sh` gives it, and Windows Installer
    compares three fields. It removes the old product once the new one's files are in place
    (`afterInstallExecute`), so `bin\pitboard.exe` is never missing in an upgrade, which needs
    each component to keep the GUID WiX makes from its key path, as `check-msi-components.ps1`
    holds it to the last release's; `REINSTALLMODE` `emus`
    replaces a file of an equal version too, as the other architecture's MSI of one version
    has. Only a full uninstall, with no `UPGRADINGPRODUCTCODE`, runs
    `Pitboard.exe --unregister` and removes `%LOCALAPPDATA%\Pitboard App` through WiX's
    `RemoveFolderEx`; `%LOCALAPPDATA%\Pitboard` is the command line's and stays.
    `LAUNCHAFTERINSTALL=1`, which WinSparkle's update passes, starts `Pitboard.exe --updated`
    once the install ends. Built with `PitboardShareSigned`, for a signed `PitboardShare.msix`,
    it also carries `PitboardShare.exe`, its package and logos. It runs
    `PitboardShare.exe --register` on each install, and `Pitboard.exe --unregister-share` on
    each full removal and on the rollback of an install that registered, each ignored on
    failure.
    `Pitboard.Installer.Tests` reads the built MSI's tables with WiX's DTF, and asks the
    `pitboard_ffi.dll` of the administrative image `msiexec /a` extracts whether it opens.
  - `Pitboard.Probe` is never shipped. The session machine runs it to measure the app's
    adapters by hand: it posts a notification through `WindowsNotifications` under an
    AppUserModelID of its own, waits for its Switch, unregisters, names a process's package,
    and finds, quits and opens an app by its AUMID through `WindowsApps`, saying what
    `RmShutdown` answered and which of the app's processes without a window run on. It
    refuses to run in an account without `pitboard-probe`'s throwaway marker.
- `apps/macos/`: the menu bar app, which shows what `pitboard-ffi`'s model says and sends it
  what was asked. The Swift package holds it as libraries its tests load without starting
  it. `PitboardKit` is the bindings and `AppModel.swift`: `AppModel`, which holds the
  `PitboardModel` and its newest snapshot on the main actor, a property for each part, and
  sends it each `Intent`, and `ModelListening`, the listener that hands it each snapshot on
  the main queue. `PitboardShareBindings` is `pitboard-share-ffi`'s, for the Share
  extension, `PitboardLinkTarget` is where a Pitboard link goes, which the app and the
  extension both link, and `PitboardApp` is the views and what only macOS does. The account
  windows are mapped under [Account windows](#account-windows).
  - `PitboardApp/App/Dependencies.swift` makes the model a launch runs on: over this Mac,
    from the app's environment and bundle, or in a debug build started with
    `PITBOARD_FIXTURE` a fixture's (`Fixture/Fixture.swift`), with the native stand-ins a
    fixture needs, and where the model keeps the account windows' records.
    `AppDelegate.swift` starts it once the app has launched, tells it of a wake and of a
    menu opening, and stops it as the app quits. `EarlierStore.swift` hands the model what
    UserDefaults held before `app.json` and `windows.json`, once.
  - `PitboardApp/System/MacPlatform.swift` is what the model asks of macOS:
    `MacAppControl` (`NSRunningApplication` and `NSWorkspace`), `MacNotifications`
    (Notification Center, a category for each button `notification_buttons` names, brought
    forward by the system where it opens a window, and permission asked at the first post),
    `NotificationsAllowed` (whether macOS turned them off, for the settings) and
    `MacLocalTime` (the person's locale, calendar and 12 or 24 hours). The settings' **Open
    Notification Settings** opens
    `x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=` and the app's
    bundle identifier, which Apple does not document. Read on macOS 27.0.1 (26A434): that
    extension takes the scheme (`allowsXAppleSystemPreferencesURLScheme`) and reveals an app
    by its bundle identifier; nobody has opened the URL to see it land on Pitboard's page.
    `LoginItem.swift` is opening at login (`SMAppService`) and `CommandLineTool.swift`
    linking the command line with an administrator's password, which stay the app's own.
    The model is told what macOS says of the login item, `Intent::OpensAtLogin`, at launch,
    on each change and as the app comes forward, for its setup tip, whose button the
    accounts pane turns into a change of the login item and not a message to the model.
  - `PitboardApp/MenuBar`, `Window`, `Sheets`, `Settings` and `Design` are the views. They
    show the snapshot's parts and send its intents, and decide only what is the platform's:
    symbols, tints, layout and which pane the window shows.
  - `project.yml` is the app itself, the spec XcodeGen generates `Pitboard.xcodeproj`
    from. Its `Pitboard` target in `App` starts `PitboardApp` and adds Sparkle, and
    `PitboardUITests` in `UITests` drives it. `PitboardShare`, from `ShareExtension`, is the
    Share extension the app embeds. Only the project's `Package.resolved` is committed,
    which pins Sparkle's revision.
  - A renewal schedule written by an app up to 0.3.0 starts the app with `renew`.
    `App/Main.swift` then replaces the process with the command line inside the app.
  - `scripts/build-xcframework.sh` builds the core, as `PitboardFFI.xcframework`, and
    `pitboard-share-ffi`, as `PitboardShareFFI.xcframework`, each with its Swift bindings,
    for both Mac architectures; `--fixture` builds the core with its fixtures.
    `scripts/build-app.sh` generates the project and builds `Pitboard.app` with
    `xcodebuild`, with the command line inside at `Contents/Helpers/pitboard`. It never
    passes `--fixture`. It fails where the core's library, the app or the command line
    inside it holds a fixture world's text or `PITBOARD_TEST_CLAUDE_DESKTOP`, the variable
    only a build for tests reads, and where the app or that command line links a SQLite
    other than the system's `/usr/lib/libsqlite3.dylib`. `scripts/build-number.sh` gives
    it the `CFBundleVersion` Sparkle orders updates by, with room for a beta below its
    release. A beta's `CFBundleShortVersionString` is its X.Y.Z alone, as Apple documents
    the key, and the app shows the whole version from its own Info.plist key,
    `PitboardVersion`.
- `packaging/`: the templates a release fills and publishes, and the tap's other files.
  The casks `pitboard.rb` for the command line and `pitboard-app.rb` for the app go into the
  tap `datlechin/homebrew-tap`, with `tap_migrations.json` and the tap's README.
  `scoop/pitboard.json`, Scoop's manifest for the command line on Windows, goes into the
  bucket `datlechin/scoop-bucket` with `scoop/README.md`. `install.sh`, the install script,
  goes on the release.
- `docs/`: the Mintlify source of docs.usepitboard.com.
- `website/`: the usepitboard.com site, built with Astro from `CHANGELOG.md` and the latest
  GitHub release into static files for Cloudflare Workers. `scripts/guard.mjs` checks the
  build; [CONTRIBUTING.md](CONTRIBUTING.md#website) has the commands.
- `.github/`:
  - `workflows/ci.yml` checks every push to `main` and every pull request, and
    `workflows/release.yml` turns a `v` tag into a release.
  - `workflows/conformance.yml` checks Claude Code's and Codex's newest builds, one for each
    system, against their registers.
  - `workflows/sparkle.yml` opens an issue when Sparkle has a release newer than the one
    `apps/macos/project.yml` pins, because Dependabot cannot read that pin.
    `workflows/winsparkle.yml` does the same for WinSparkle and the Windows App SDK, against
    `apps/windows/Directory.Packages.props`, since Dependabot raises no NuGet package.
  - `actions/xcodegen` puts the pinned XcodeGen on `PATH` for every job that builds the
    app.
  - `workflows/rotation.yml` rehearses rotating the update key.
  - `workflows/windows-facts.yml`, started by hand, measures the Windows runners, or with
    `session` builds the session bundle.
  - `actions/apple-keychain` imports the Developer ID certificate for every job that signs.
  - `actions/rust` installs the toolchain and, unless a job turns it off, restores the Cargo
    cache; nothing `release.yml` runs restores one. `actions/windows-builds` makes the builds
    both Windows workflows start from.
  - `actions/windows-release` builds, zips and checks `pitboard.exe` for `release.yml` from
    1.0.0, for CI on every pull request, and for `windows-facts.yml`'s session bundle.
    `actions/windows-app-libraries` builds the Windows app's two libraries and their bindings
    for `release.yml` and the session bundle.
  - `scripts/` holds the EdDSA key and signature helpers, and the scripts that add Sparkle,
    the command line and the Share extension's library to the app's bill of materials.
    `fill-template.sh` writes a release's version and checksums into a `packaging/`
    template, and `test-install.py` tests it and the install script. `release-kind.sh`
    says what a tag releases and whether it is latest, `core-unchanged.sh` refuses a
    `pitboard-core` that changed under a version crates.io already has,
    `check-winget-hashes.py` holds komac's winget manifests to a release's `SHA256SUMS`,
    `windows-appcast.py` writes `appcast-windows.xml` for a full release's MSIs and their
    signatures, `windows-versions.sh` gives a Windows build's MSI, file and package numbers,
    `windows-update-key.sh` refuses a Windows update key that changed since the last MSI
    unless a rotation names the release, `windows-update-sign.sh` signs a Windows release's
    MSIs and checks each signature with the public key, `session-gate.sh` refuses what the
    session bundle is never built from, and `test-release.py` tests the eight, the app's
    `build-number.sh` and the cargo-binstall metadata in `crates/pitboard/Cargo.toml`.
    `package-windows.ps1` zips a Windows build, `check-windows-build.ps1` checks one,
    `verify-windows-signatures.ps1` checks the signature of every program a Windows release
    hands on, in a zip or an MSI, an MSI's other programs against their makers, and of a sparse
    package, `pack-session-bundle.ps1` sums, zips and encrypts the session bundle, and
    `test-windows-release.ps1` tests the first three on zips and the fourth with a stand-in for
    age. `check-windows-app.ps1` checks the Windows app's Release build,
    `pack-windows-share.ps1` packs `PitboardShare.exe`'s sparse package beside it,
    `test-certificate.ps1` makes the throwaway certificate it and the runner facts sign with,
    `extract-msi.ps1` extracts an MSI's administrative image and `check-windows-msi.ps1` checks
    one, `check-msi-components.ps1` holds its components' GUIDs and key paths to the last
    release's MSIs, which `previous-msis.ps1` downloads, `validate-windows-msi.ps1` runs ICE03 on an MSI with the WiX that built it,
    `read-assembly.ps1` reads a value from a .NET assembly's metadata, and
    `test-windows-app.ps1` tests the checks and the signature check on the MSI and the package.
    `windows-pending.py` fails on a test Windows leaves out without a reason it accepts, and
    `test-windows-pending.py` tests it.
  - `dependabot.yml` asks for weekly updates of Cargo dependencies, GitHub Actions and the
    .NET SDK in `apps/windows/global.json`.
  - `zizmor.yml` configures the audit CI runs on the workflows.

## Invariants

Each rule names what holds it: a test, a lint, a type, a refusal or a check in CI. How the code
keeps it is in the header of the module named.

### The core and its environment

- The core prints nothing: a front end words what it returns. `lib.rs` warns on
  `print_stdout` and `print_stderr`, which only a test and the stand-in are let off.
- The core reads its environment in `Context::read`, from a map of variables, the same way for
  every front end. `clippy.toml` refuses `std::env::var` and its kin elsewhere, so every other
  read carries an `#[allow]` saying why.
- `context::READ` and `settings::OVERRIDING_ENV` name every variable a release reads, and
  `Environment` panics in a build with debug assertions on a variable they do not name.
  The integration harness withholds every one from each command it runs.
- The core makes every file and folder through `host::fs`, which says once per system how each
  is made private. Its `clippy.toml` refuses std's ways outside `host/`'s faces and repeats the
  root's rules
  (`the_crates_lints_keep_every_rule_of_the_workspaces_and_refuse_stds_creates`).
- Every request goes out on its context's agent, `api::agent`, given the proxy the context read
  from its variables alone. `clippy.toml` refuses every other way ureq makes an agent or lets a
  request change its configuration.
- No request goes past a SOCKS proxy the person named: `proxy::Refusal` fails each request
  `NO_PROXY` does not exempt before anything is sent, and the agent follows no redirect
  (`every_socks_proxy_is_refused_before_anything_is_sent`,
  `a_redirect_is_not_followed_past_a_socks_proxy`).
- Every home Pitboard reads or writes under is a full path. `home::check_absolute` refuses
  one that is empty or relative with `home_not_absolute`, before every change and every read
  of the accounts (`every_command_refuses_a_home_that_is_not_a_full_path`).
- Which system Pitboard runs on is decided in `host/mod.rs` alone, and anything else that
  differs by system is the host's to answer or a `match` on `host::OS` with no `_` arm, so a
  system added to `host::Os` does not compile until each is said. A context carries the system
  its environment was read as, so Windows' homes and variable names are tested everywhere.
- A `\\?\` path is for comparing alone: what Pitboard shows, locks by or hands a tool is
  spelled without it (`a_verbatim_path_is_spelled_as_libuv_and_codex_spell_it`).
- Pitboard changes nothing as root or under sudo, elevated on Windows, or on a Windows older
  than 11 24H2, but as root on Linux where root owns every place a change writes
  (`home::written`). `permit::gate` alone makes a `Permit`, and every write, deletion,
  scheduler change, tool run and token exchange takes one, so a change that skips it does not
  compile.
- A Windows build of a 0.x release does nothing (`release.rs`): the gate, every read of the
  accounts, the app's core and every command but `completions` and `manpage` refuse. Only
  `--cfg pitboard_unreleased_windows` opens one, never a feature, which crates.io would offer
  to anybody.
- The Windows app updates only through the Windows update key, since WinSparkle checks no
  Authenticode signature. `check-windows-app.ps1` fails a Release build without the key, and
  `windows-update-key.sh` a key that changed since the last MSI unless a rotation names the
  release.
- With nobody to ask, the command line deletes nothing it was not told to: `forget`, `stow`
  and `uninstall` refuse with `confirmation_needed` through a pipe, a script or `--json`
  unless given `--yes` (`Confirm` in `crates/pitboard/src/main.rs`, the `*_unconfirmed`
  snapshots). On 10 October 2026 `pitboard stow < /dev/null`, run as a look, deleted the login
  file behind the keychain.
- The `--json` contract changes only on purpose: a changed snapshot in
  `crates/pitboard/tests/cli/snapshots` is a changed contract. Its JSON is ASCII alone, each
  other character a `\u` escape (`json.rs`, `a_label_outside_ascii_is_printed_as_escapes`).

### Logins

- Nothing outside `pitboard-core` writes Pitboard's index. Every change goes through `switch`,
  which records what it is about to do first.
- Additive writes become durable before destructive ones: a run that dies midway through a
  Claude Code or Codex switch leaves a spare copy of a login, never a missing one
  (`switch/crash.rs`, `tests/cli/crash_recovery.rs`).
- A Claude Desktop login is never copied. Before a move's first rename, `state.json`'s
  `moving` names the two places each item may be, and settling never puts an item into
  Claude's folder (`switch/folder/crash.rs`).
- A read never settles an interrupted switch or move: every change settles an interrupted
  switch first, under `state.lock`, and a move of Claude Desktop, with Claude closed, before a
  change of Claude Desktop, `repair` or `uninstall`. `status` and `doctor` say a switch a
  change would stop at by the same decision, taking no lock (`switch::stuck`), and a waiting
  move as `move_waiting`.
- No Claude Code or Codex login moves until Pitboard knows whose it is: asked of its service,
  or known by its refresh token's fingerprint from an earlier answer about that login
  (`switch/identify.rs`, `a_login_with_no_fingerprint_is_always_asked_about`).
- Who is in use is one record per tool, `state.json`'s `in_use`, written under `state.lock`
  only where a service just answered for that login or a fingerprint ties it to an answer
  (`in_use.rs`). Every reader that does not read the store takes it from there
  (`in_use::known`).
- Claude Code's config is a sign that something signed in, and the words of a warning, never
  whose a login is. Only a switch and `use` of the account in use write it
  (`the_config_is_written_only_while_the_account_is_in_use`).
- Nobody is asked whose a Claude Desktop login is: Pitboard knows one by its session cookie's
  fingerprint, takes an unseen session as Claude names it only in the case
  `switch/folder/verdict.rs` reads, and refuses every change in doubt. A read opens
  `config.json`, its journal and `Claude-3p`'s settings, and none of them while no Claude
  Desktop account is enrolled.
- Locks are taken in one order: `signin.lock`, `state.lock`, then Claude Code's refresh lock,
  its legacy lock and its write lock, or Claude Desktop's data folder lock (`lock.rs`).
- Pitboard writes Claude Code's login where it lives, under Claude Code's write lock. A switch
  holds Claude Code's refresh lock from its last read of the outgoing login until it has
  recorded the one installed, so no session renews the copy it parks (the register's
  `refresh_lock`).
- Pitboard never answers a failed keychain write by writing Claude Code's plaintext file
  (`a_failed_keychain_write_never_falls_through_to_the_plaintext_file`).
- While `.credentials.json` sits behind the keychain or Credential Manager, running sessions
  keep their account until their next renewal: a switch says `Adoption::AtRenewal`, and every
  read and change warns with `fallback_login`. Nothing writes or deletes the file but
  `pitboard stow` and the app's **Put Away**, once somebody has confirmed what it holds and
  only while it still holds those bytes (`switch/stow.rs`).
- The login in use is its tool's to renew. Pitboard renews Claude Code's only where it must
  tell whose it is and its access token has lapsed, as Claude Code renews it
  (`switch/refresh.rs`); never Claude Desktop's or Codex's, and never for usage.
- A Codex login is moved, never copied (`ParkSemantics::MoveOnly`): Codex's own sign-in and
  sign-out revoke the stored refresh token. Codex takes no lock on `auth.json`, so a switch
  reads it again before replacing it, and a read that finds it half written reads again
  (`a_login_caught_part_way_is_read_again_ten_times_ten_ms_apart`).
- On macOS only `/usr/bin/security` reads or writes Claude Code's keychain item and Pitboard's
  parks (`host/macos/keychain.rs`; [macOS](docs/facts/macos.md) says why).
- On Windows a park is sealed with DPAPI to the person's account, in CurrentUser scope with
  entropy naming the park, never kept in Credential Manager, and one that does not open is
  unreadable, never absent (`store/vault.rs`,
  `a_seal_whose_key_is_not_found_is_unreadable_not_absent`).
- `abandon` gives up an interrupted switch first, whatever Claude Desktop's state, and `adopt`
  drops every waiting move and awaited sign-in, which were the other computer's. A change of
  Claude Desktop that fails carries what it did first in its error (`Error::SaidFirst`), which
  the front ends say ahead of the refusal (`switch/folder/tests/giving_up.rs`).

### Usage and switching by itself

- Pitboard files usage only under the account it is proven to be of: an answer for that
  account's own login, or a session's numbers whose windows that account's answered reading
  holds and no other's does, never by Claude Code's config or the time since a switch. An
  answer replaces a reading and a session only moves it (`usage::answered`, `usage::moved`,
  `statusline.rs`).
- What the status line writes after each response is replaced whole and not flushed
  (`atomic::write_unsynced`): Claude Code cancels a run still going 300 ms after a change (the
  register's `statusline_run_timing`). An answer's reading and an account forgotten are
  flushed.
- Pitboard switches by itself only for a front end somebody asked to, the app with its setting
  on or `pitboard watch`, and switches Claude Code alone. The status line and daily renewal
  never switch, and an automatic switch never quits an app.
- An automatic switch is decided once, under `state.lock`, from one telling of whose login
  Claude Code has stored, and made from that same login (`switch/auto.rs`), so front ends
  deciding from one reading make one switch
  (`front_ends_that_decide_at_once_from_one_reading_make_one_switch`).
- Each reason the automatic switch does not switch, and each wait, is recorded once for its
  account, limit, reset and code, and every front end tells one from another by the core's
  one rule, `Auto::told_apart`, in the core's words (`duty/autoswitch.rs`).

### Daily renewal and the watch service

- Daily renewal and the watch service serve the default home alone. A run of either acts there
  whatever `PITBOARD_HOME` says, and installing either refuses elsewhere with
  `schedule_not_default_home` (`schedule::serves`,
  `the_schedule_belongs_to_the_default_home_alone`).
- A job is given the installing context's proxy variables, in files written private, or in
  `schedule.env` and `watch.env` on Windows, and `doctor` warns with `schedule_proxy` where
  they would send a request another way than this run
  (`a_schedule_is_given_the_proxy_variables_of_whoever_installs_it`).
- The watch service's run is ended only while Pitboard's lock is held, so never in the middle
  of a change, and `uninstall` takes it away before the schedule
  (`the_services_run_is_ended_only_between_changes`,
  `uninstalling_takes_the_watch_service_with_it`).
- Nothing rewrites a task changed in Task Scheduler, which `doctor` warns of
  (`task::drifted`), and the app's start-up repair points a schedule at the command line
  inside the app only where its Pitboard is gone or it runs an app's own program
  (`schedule::fault`). The MSI's full uninstall takes away only the tasks
  that run its own `bin\pitboard.exe` (`schedule::uninstall_for_app`).

### Tests

- No unit test reaches a real home: `Context::for_unit_test` names a scratch folder for every
  home, and a build for tests never asks the passwd database or Windows for the account's own
  folders.
- No test starts the person's login shell or reaches the system's scheduler. A test names a
  shell of its own, and the real hosts refuse launchd, systemd and Task Scheduler in any build
  for tests (`a_test_never_reaches_the_systems_own_scheduler`), but for the Release build
  `ReleaseGateTests` starts at 1.x as a runner's fresh standard user.
- No test addresses a name a real login may be kept under, or a folder that is, holds or lies
  in a real login place: `guard_not_live` refuses `provider::names`' families, `host::lies_in`
  answers for folders, and CI's Windows legs check afterwards that nothing leaked
  (`tests/cli/harness.rs`). A test that writes Credential Manager runs in nextest's
  `credential-manager` group, one at a time.
- Every program a test starts in place of a tool is one compiled program, the `pitboard`
  crate's example `stand-in`, never a shell script, and a test refuses where it is not built
  (`a_stand_in_not_built_is_refused_with_the_command_that_builds_it`).
- Outside a host's own face, a test sets a file's mode or makes a link only through
  `fs::testing`. The integration harness says what differs by system in
  `crates/pitboard/tests/cli/common/os.rs`, one `match` on `host::OS` for each fact.
- The whole suite runs on macOS, Linux and Windows: a test put off on a system says why in its
  `ignore` reason or a `// windows:` line, which `.github/scripts/windows-pending.py` holds to
  the reasons it accepts. A test of Claude Desktop outside the core returns
  where the tool is not supported rather than being ignored.
- No test makes the app model over the machine's own environment. The Rust tests run the real
  core over a `ScratchMachine`, the C# and Swift tests only a fixture's model or one never
  started, and every `AppControl` and `Notifications` a test hands in records and posts
  nothing.
- A fixture is the real core and the real model over a machine of its own, `MemoryHost`,
  `ScriptedApi` and a folder in the temporary directory. Only the `fixture` feature builds one
  and only a debug build reads `PITBOARD_FIXTURE` ([The fixtures](docs/facts/fixtures.md)).

### The apps

- Nothing `pitboard-ffi` exports is async or waits on the core: only the model's lanes call
  `AppCore`, `send` only posts, `snapshot` only copies, `shutdown` waits only for the actor and
  the sign-ins under way, and a free function reads nothing slower than a file
  (`model/mod.rs`).
- A snapshot is made by `present`, which asks nothing but the app's `LocalTime`, and says a
  clock time in UTC, named so, where that cannot
  (`a_clock_the_app_cannot_read_is_said_in_utc_rather_than_not_at_all`).
- The model's state belongs to its actor thread, and `State::apply` does no I/O. A switch, and
  every other change the app makes to the index, is claimed there as its intent is taken and
  held until the read after it, so one runs at a time and the poll leaves the index alone
  (`model/switching.rs`, `model/changing.rs`).
- An app is quit only when the person says so, never by force, within 30 seconds, and
  Pitboard opens again only the copy it quit: Claude only where its folder is whole
  (`Pitboard::whole`). Changes of a tool whose login is its app's files never overlap
  (`model/switching.rs`).
- One sign-in runs at a time, on a thread of its own, told apart by an id, and enrols only if
  it is still the sign-in under way. It keeps the sheet it started from, and `shutdown` waits
  for each one's tool to stop (`model/signing.rs`).
- What the app says by itself is said once: a run-out once per reset across launches, kept in
  `told.json`; a replaced login once per account while the reads say it; a refusal of the
  automatic switch once until it next switches (`model/advising.rs`, `model/threaded.rs`,
  `model/automatic.rs`).
- The app's own preferences are the model's, in `app.json` in Pitboard's directory. A file
  that cannot be read is left as it is and never taken for a first launch, and the macOS
  app's UserDefaults are handed over once (`model/keeping.rs`).
- With its setting on, the app asks the core's look after every read and every
  `DECIDE_EVERY_SECONDS`, as `pitboard watch` decides, paces a refusal by
  `duty::cadence::retry_after`, and never says what it came to in an alert
  (`model/automatic.rs`).
- A notice's button is a change of the app's own, never a switch: updating Claude Code's
  config is refused where another account is in use by then, and **Put Away** puts away only
  the file its sheet looked at (`model/changing.rs`, `model/stowing.rs`).
- The app has no rule of its own for what the core decides: its homes, Pitboard's directory,
  whether a path is a program, the sites and the links it opens are the core's, and every
  word and button a view shows is the model's. `AppControl` names an app by the id holder
  detection gives it.
- One thread tells a `ModelListener`, in revision order and only a snapshot that differs from
  the last, so a test waits for what a snapshot says once something is over
  (`snapshots_made_while_many_threads_send_arrive_in_order`).
- One change to daily renewal runs at a time, and turning it on is refused where the command
  line inside this copy would not last (`schedule::in_a_temporary_copy`,
  `model/maintaining.rs`).
- The macOS app's `AppModel` shows only a snapshot newer than the one it shows
  (`aSnapshotThatIsNotNewerIsDropped`), and `ModelListening` holds it weakly, since the model
  holds the listener. The window opens once for each `WindowRequest.serial` (`WindowRequests`).
- The Windows app asks the core's gate, `launch_refusal`, before anything else a launch does
  but `--unregister` and `--unregister-share` (`ReleaseGateTests`).
- The Windows app's own data folder is `Pitboard App` in the known folder Local AppData, never
  in Pitboard's directory, so `pitboard uninstall` never deletes the account windows'
  sign-ins (`host::pitboard_app_on_windows`).
- Only a Release build an MSI installed registers anything with Windows, and
  `Pitboard.exe --unregister` takes back what it registered. CI's Windows legs fail a run that
  leaves a link scheme, a notification registration, a start at sign-in, a `PATH` entry or a
  package behind.
- Only `PitboardShare.exe` has a package identity, which `check-windows-app.ps1` holds, and it
  hands a page to `Pitboard.exe` only as a Pitboard link
  ([Documented, not measured](docs/facts/windows-documented.md) has what is still to be seen).

### Account windows

The account windows read the accounts from the model's last read, and their bookkeeping is the
model's, so both apps get the same decisions. [WebKit and SwiftUI](docs/facts/webkit.md),
[What the Windows app's glue rests on](docs/facts/windows-app.md) and
[claude.ai and chatgpt.com](docs/facts/services.md#claudeai-and-chatgptcom) hold what they rest
on.

- One store per account: a version 5 UUID of `<store name>:<account id>` in a namespace that
  never changes, so a rename keeps its sign-in
  (`an_enrolled_claude_account_has_a_store_derived_from_its_account_id`,
  `an_account_id_is_lowered_as_swift_lowered_it`).
- A store is recorded in `windows.json` before WebKit or WebView2 makes it, and deleted only
  where this Pitboard directory recorded it and a read that succeeded no longer derives it from
  an enrolled account. Another directory's record keeps it
  (`a_store_another_directory_recorded_is_not_deleted`).
- A records file that cannot be read is never written over, and nothing is deleted by it
  (`a_file_that_cannot_be_read_is_never_written_over`).
- Nothing from outside opens a window by itself. A Pitboard link only shows the **Open Link**
  window, a site's sign-in link is refused, and only a person's choice opens a window, on the
  link's own site (`a_sign_in_link_is_refused_from_outside`).
- A window's page goes only to its site, its sign-in hosts and blank pages. Google's sign-in is
  refused, other pages go to the browser, a frame gets no sign-in window, and a window's last
  page is never a sign-in path (`account_windows/policy.rs`).
- Hands off the session: Pitboard makes, wipes and deletes a store, never reads what a site
  keeps there, adds no script and sets no user agent, and no window is given a Claude Code or
  Codex login.
- The Pitboard link's format is a contract pinned by `handoff.rs`'s tests, and a release
  claims `pitboard://` and a debug build `pitboard-debug://`, which `build-app.sh` holds. On
  Windows only an installer registers `pitboard`, and a Debug build outside a fixture refuses
  a `pitboard-debug` link (`LaunchingTests`).
- The Share extension stays sandboxed and links `pitboard-share-ffi`, never the core: the
  core's bindings check every export as they load, and two Rust static libraries never meet in
  one binary. `build-app.sh` fails a signature that is not sandboxed and either binary that
  holds the other library's symbols.
- `pitboard-sites` stays a leaf: no I/O, nothing of the core and no UniFFI.
- A deleted WebView2 profile's folder goes only as its browser process exits, so on Windows
  `StoreJanitor` answers `StoreHeld` until it has gone and the model opens no window of a held
  store (`ProfileTests`).
- A download on Windows that nothing accounts for asks as a frame's would, and a window closed
  while one of its downloads runs waits out of sight until it ends (`DownloadTests`,
  `ClosedWhileItDownloadsItWaitsOutOfSightForTheDownload`).

## Boundaries

- The core and its front ends meet at `service::Pitboard`, `context::Context` and the types
  those two return, `pitboard-core`'s supported interface. Its other public modules serve this
  repository's front ends and may change in any release.
- In that interface, adding an error, warning or check code is not a breaking change; renaming
  or removing one is. `pitboard-core` has a version of its own, and a breaking change gets a
  new minor version of it.
- Pitboard and each tool meet at the tool's [register](#tool-registers).
- Pitboard and each service meet at a few requests, which
  [What leaves your machine](https://docs.usepitboard.com/security#what-leaves-your-machine)
  lists with what each carries.
- Pitboard and claude.ai or chatgpt.com meet at an account's window, which a person opens. The
  site's own pages sign it in, WebKit or WebView2 keeps that sign-in in the account's store,
  and Pitboard decides only where each navigation goes.
- Pitboard and a browser meet at the Share extension and `PitboardShare.exe`, which hand the
  app the one link shared, as a Pitboard link. Pitboard reads nothing a browser keeps.
- Pitboard and Claude meet at Claude's data folder. Pitboard reads `config.json`, its journal,
  the names of the folder's first-level entries and of `IndexedDB`'s, and
  `Claude-3p/claude_desktop_config.json`'s `deploymentMode` where `Claude-3p/config.json` is
  there. Of `Cookies` it keeps only the `sessionKey`'s expiry and an 8-byte fingerprint of its
  stored value. It decrypts nothing, reads no keychain item for Claude Desktop and asks
  claude.ai nothing. It opens `Cookies` and moves the items `Desktop::items` names only while
  Claude is closed, under an advisory lock on the folder, and deletes nothing of Claude's.
- Pitboard and a running app meet at the process list and the app's `AppControl`. The core
  tells that Claude runs by the bundle each program runs from, and neither starts nor quits it.
- The code and the release meet at a `v` tag, which `.github/workflows/release.yml` turns
  into a release. [RELEASING.md](RELEASING.md) has the procedure.

## The state file

`~/.pitboard/state.json` is Pitboard's index: which accounts it knows, where each one's login
is parked, and whose login each tool has stored. It carries a `schema` number.

- The app and the command line inside it update together, and a command line installed another
  way updates by its own route, so an older Pitboard can meet a file a newer one wrote.
- It is read forwards only. `state::migrate` adds a step after the ones before it for each
  bump, so a file several versions behind comes forward in one read, and each step's header
  says what its schema changed. A bump needs a test that loads a file the previous version
  wrote.
- An older Pitboard refuses a newer file and says to update
  (`a_file_from_a_newer_pitboard_is_refused`). A field an older version would drop at its next
  save, or a shape it would misread, is a bump, not a field beside the old ones, so that
  Pitboard refuses it (`renewing` at 7, typed `discarded` at 8).
- A file naming a tool this build does not know is a newer Pitboard's, never corrupt: the
  advice for a corrupt file is to delete it, which would orphan every parked login
  (`an_account_of_an_unknown_tool_asks_for_an_upgrade_not_a_deletion`).

What it holds besides the accounts:

- `in_use`: whose login each tool has stored, as its service last said, for the slot `slot`
  names; `other_slots` keeps the records of every other `CLAUDE_CONFIG_DIR` or `CODEX_HOME`.
- `renewing`: renewals of a tool's own login not saved where the tool keeps it yet.
- `discarded`: parked logins waiting to be deleted, by service and kind.
- `moving`: by tool, one half of a move of a login kept as files, written before its first
  rename.
- `aside`: each folder under `parks` no account holds, with why it was set aside. Only `forget`
  of its account and `uninstall` delete one, and `repair` gives one back.
- `awaiting`: by tool, a sign-in inside the tool's app that Pitboard waits for.

## Tool registers

Every fact Pitboard relies on about a tool was read out of one named build of that tool, and
lives in that tool's register, `crates/pitboard-core/src/provider/<tool>/assumptions.rs`:
`claude`, `codex` and `desktop`. [Not measured](docs/facts/not-measured.md) holds what nobody
has measured.

- A fact names literals a build must contain (`probe`), or literals whose arrival would
  disprove it (`absent`). A fact about behaviour names none.
- Each register's `PER_SYSTEM` table says what each fact is on macOS, Linux and Windows:
  `Read` from that system's build, `NotRead` with why, or `Pending` on a named pull request of
  the Windows work, which `assumptions::pending` lists. Where a tool does something else on
  Windows, that is a fact of its own, such as `credman_target` beside
  `credential_service_name`, never a second reading of one fact.
- `pitboard-conformance` reads a program as bytes, telling its system by its header, or an
  Electron app's `app.asar` for the system `assumptions::archive_platform` names. The check is
  shallow on purpose: a literal present proves nothing, and one gone, or one ruled out
  appearing, proves something moved.
- `.github/workflows/conformance.yml` checks four builds each of Claude Code's and Codex's
  newest against their registers twice a week. Claude Desktop's is checked by hand on an
  installed copy, since where its builds are published is not recorded.
- Adding a tool takes a register read out of a named build, with its table; a module under
  `provider/` implementing `Provider` or `Folder`; and, where its builds have a recorded
  source, an entry in `conformance.yml`'s matrix. `ProviderId::ALL` and the matches in
  `provider::of` and `assumptions` name every tool, so the compiler points at what an added
  one lacks.

[Tool registers in CONTRIBUTING.md](CONTRIBUTING.md#tool-registers) says how to run the checker
and add a fact.
