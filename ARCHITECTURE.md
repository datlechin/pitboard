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

- The core prints nothing.
- The core reads its environment in `Context::read`, from a map of variables, the same way
  for every front end. The command line passes its own through `Context::from_env`. An app
  passes the one it was started with through `AppContext::discover`, which also asks the
  person's login shell for `PATH`, because an app the system started has none of a
  shell's. On Windows it reads the registry's `Path` instead, which a program already
  running was not given after a change.
- Three variables are also read straight from the process. `PATH` is read when a context
  made with `Context::new` was given no search path (`context.rs`), and on Linux to find
  the program daily renewal runs (`host/linux`). `NO_COLOR` is read by the status line
  (`pitboard/src/main.rs`). `XPC_SERVICE_NAME`, which launchd sets, is read in
  `host/macos/launchd.rs`. `clippy.toml` refuses `std::env::var`, `var_os`, `vars` and
  `vars_os` outside `context.rs`, so every other read carries an `#[allow]` that says why
  it is meant.
- The core makes every file and folder through `host::fs`, so how each is made, and made
  private, is said once for each system. Its own `clippy.toml` refuses std's ways to make
  one, `fs::write`, `File::create`, `OpenOptions::create`, `fs::copy`, `create_dir` and the
  rest, outside the faces in `host/`, the stand-in, the Claude Desktop the tests play and
  the tests that set up a scratch folder, each of which says why in its `#[allow]`. It
  repeats the root's rules, since a crate's own `clippy.toml` replaces the root's, and a
  test holds it to them.
- `READ` in `context.rs` and `settings::OVERRIDING_ENV` name every variable a release of
  Pitboard reads. `READ_BY_BUILDS_FOR_TESTS` names the one only a build for tests reads,
  `PITBOARD_TEST_CLAUDE_DESKTOP`, and is empty in a release. `Environment` refuses, in a
  build with debug assertions, to read a variable none of them names. The integration tests
  withhold every one of them from each command they run, except `HOME` and `USER` on macOS
  and Linux, and `SystemRoot`, `TEMP`, `TMP` and `USERNAME` on Windows, and the core's unit
  tests make their context with `Context::for_unit_test`, which withholds all of them, so a
  variable exported where `cargo test` runs, such as `PITBOARD_CLAUDE`, never reaches a
  test.
- The proxy variables are among them, listed in `proxy.rs` in the order ureq tries them.
  Every request goes out on the agent its context makes, `api::agent`, which is given the
  proxy the context read, or none. ureq still reads this process's environment as each
  agent's configuration is made, and that reading is replaced before the agent exists.
  `clippy.toml` refuses `Proxy::try_from_env`, every way ureq offers to make an agent or a
  configuration (`Agent::new_with_defaults`, `new_with_config`, `with_parts` and
  `config_builder`, `RequestExt::with_default_agent` and the request functions), and the
  type `Config` wherever it is named, which covers `Config::default` and
  `Config::builder`. `api::made`, which makes `api::agent`'s agent, alone allows
  `config_builder` and `with_parts`, and says why. Proxies come from environment variables
  only, as the owner chose: a system proxy that no variable names is not followed.
- No request goes out past a proxy the person named. ureq is given an HTTP or HTTPS proxy
  and never a SOCKS one. Where a SOCKS proxy is named, the agent's middleware,
  `proxy::Refusal`, fails each request `NO_PROXY` does not exempt before anything is sent or
  looked up, and the agent follows no redirect, since ureq follows one inside the request
  the middleware handed on. ureq lets a single request change the agent's configuration
  (`RequestBuilder::config`, `Agent::configure_request`, `WithAgent::configure` and
  `RequestExt::middleware_config`). Measured, a request given its own `max_redirects`
  followed such a redirect directly, so `clippy.toml` refuses each of those, and every
  request goes out with the agent's configuration.
- No unit test reaches a real home. `Context::for_unit_test` names one folder for every
  home, the person's, Pitboard's, Claude Code's config directory and Codex's: a folder of
  that test's own under the temporary directory, which nothing makes. In any build for
  tests the passwd database is never asked for a home (`host::user::home`): a unit test
  that asks panics, unless it is a test of that lookup and says so, and any other build
  for tests, the command line the integration tests run and an app built with the
  fixtures among them, is told there is none, which is refused as below. On Windows every
  lookup of the account's own folders answers nothing in a build for tests, a unit test that
  does not say it tests one included.
- Every home Pitboard reads or writes under is a full path: `HOME`, `USERPROFILE` on
  Windows, Pitboard's own directory, `CLAUDE_CONFIG_DIR` and
  `CLAUDE_SECURESTORAGE_CONFIG_DIR` where they name a folder, and `CODEX_HOME` where it is
  set. One that is empty or relative is refused with `home_not_absolute`, naming the
  variable, by `home::check_absolute`, which the gate every change passes asks, and every
  read of the accounts. `doctor` fails its `homes` check over it and checks nothing else.
  The command line asks it before every command but `completions`, `manpage`, `doctor` and
  `renew`, which the gate refuses, so `log`, `schedule status` and the status line refuse it
  too. Nothing is resolved against the folder Pitboard runs in.
- Which system Pitboard runs on is decided in `host/mod.rs` and nowhere else. Anything that
  differs by system is either the host's to answer or a `match` on `host::OS`, so a system
  added to `host::Os` does not compile until it is said for every one. No such `match` has a
  `_` arm. Where Pitboard cannot tell what a tool does on Windows, its arm refuses through
  an answer its callers already have, never one that would let Pitboard act: Claude Code's
  live chain there is one store whose every call cannot be read (`store::Refused`) while
  Credential Manager holds a login nothing Pitboard reads turned on, and the Windows face
  answers as `host/windows` says.
- A context carries the system its environment was read as, so where the home and
  Pitboard's default folder are, and how a variable's name compares, are tested for
  Windows on every system.
- A `\\?\` path on Windows is for comparing alone. What Pitboard shows, names a lock by or
  takes for a tool's home is spelled without it, as libuv's realpath and Codex spell a
  canonical path (`host::without_verbatim_prefix`).
- With nobody to ask, the command line deletes nothing it was not told to. `forget`,
  `stow` and `uninstall` ask at a terminal, for a person. Through a pipe, from a script or
  with `--json`, each refuses with `confirmation_needed`, exit 2, before it changes
  anything, unless given `--yes` (`Confirm` in `pitboard/src/main.rs`). On 10 October 2026
  `pitboard stow < /dev/null`, run as a look, deleted the login file behind the keychain.
  The app asks in its own dialogs and does not go through `Confirm`. On Windows std's
  `is_terminal` also takes a pipe named as an msys or cygwin pty for a terminal (`msys_tty_on`
  in Rust 1.98.1's `library/std/src/sys/io/is_terminal/windows.rs`), so under mintty, Git
  Bash's own terminal, they ask as in a console, since mintty sets `TERM` (`xterm` by
  default). Without it anstream 1.0.0 writes through the console API, which the pipe refuses
  (run 38901801954).
- A Windows build of a release before Pitboard for Windows is released does nothing
  (`release.rs`). The gate every change passes asks first, so no file is written, no store
  of logins changed and no token renewed even by a caller that forgot to ask; every read
  of the account list asks too; the app's core asks before every read the model makes
  (`launch.rs`), so an app reads none of Pitboard's files and no login; and the command
  line asks before every command but `completions` and `manpage`. Only
  `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc` compile (`lib.rs`). The switch
  that opens a Windows build early is a `cfg` set in `RUSTFLAGS`, never a Cargo feature,
  since crates.io offers a published crate's features to anybody.
- The Windows app a release ships updates only through the Windows update key. Each Release
  build carries a key WinSparkle takes and the feed of full releases (`check-windows-app.ps1`),
  and the key the newest earlier MSI shipped unless `WINDOWS_UPDATE_KEY_ROTATION` names the
  release (`windows-update-key.sh`). WinSparkle checks no Authenticode signature, so there is
  no second route.
- Outside a host's own face, a test or a fixture sets a file's mode, makes a file runnable
  or makes a link only through that face's `fs::testing` (`pitboard_core::testing::fs`),
  never through `std::os::unix`. A link to a directory is asked for as one, `link_dir`, apart
  from a link to a file or to nothing, `link`. So a system whose files have no mode, or that
  links a directory otherwise than a file, says how in its own face and nowhere else. The
  integration tests' harness says what differs by system in
  `crates/pitboard/tests/cli/common/os.rs`, one `match` on `host::OS` for each fact: where
  Claude Code keeps the login it uses (`.credentials.json` on Windows, as on Linux, unless a
  test turns Credential Manager on, `Env::turn_credential_manager_on`, whose items in its own
  slot the harness deletes by name: `windows_backend_choice`), where Pitboard
  parks one (sealed to the person on Windows, which only the core's vault writes, so the
  harness parks through it), which variables a command is passed on, which folders
  of a person's account it is given as scratch ones of its own, what its `PATH` holds after
  the test's programs, what a program's file is called, how npm lays out a package's
  program, whether Claude Desktop runs there (`os::claude_desktop_runs_here`), and how a
  warm-up hands Claude Code its token (`os::warm_up_token`).
- The whole suite runs on macOS, Linux and Windows. A test put off on a system says why in
  its `ignore` reason, which libtest lists:
  - A test of something one system does not have or do names it, such as
    `ignore = "on Linux the file is Claude Code's only store, never behind a keychain"` on
    Linux and `ignore = "a firmlink to the Data volume is macOS's own"` off macOS.
  - A core test of Claude Desktop's folder calls, which Windows refuses, says
    `#[cfg_attr(windows, ignore = "Claude Desktop is macOS only: …")]`. It runs on macOS and
    Linux, against a scratch data folder.
- A test of Claude Desktop outside the core is not ignored. Where Pitboard does not support
  the tool, it returns at its start and passes. An integration test asks
  `os::claude_desktop_runs_here()`, which is false on Linux and Windows. A test of the app
  model's or a fixture's Claude Desktop asks `ProviderId::Desktop.supported_on(host::OS)`,
  since the app lists the tool only where that is true.
- A test a `cfg` compiles out on Windows, or that returns early there, says why in a
  `// windows: <reason>` line right above that `cfg` or that `if`, since neither takes a reason.
- `.github/scripts/windows-pending.py` lists the reasons a Rust or C# test may give for being
  left out on Windows, and CI fails on any other. A reason for a gap being closed has a ceiling
  there, so no new test gives it.
- No test addresses a name a real login may be kept under. `guard_not_live` refuses by
  pattern, never by a hash computed for a real home, the families
  `pitboard_core::provider::names` tells apart, which the measurement probe's leak check
  reads too: every Claude Code slot, bare or under an account and in each of its pieces,
  but the one hashed from the test's own folder; Codex's `cli|` and `secrets|` targets; and
  anything under `Codex MCP Credentials`.
- No test uses a folder that is, holds or lies in a real login place:
  `/Applications/Claude.app`, and the account's own `.claude`, `.claude.json`, `.codex`,
  Pitboard folder, Claude's data folder `Library/Application Support/Claude` and `Claude-3p`
  beside it. Those are taken from the home and the Pitboard folder the system names for the
  account whatever the environment says (the passwd database, or `FOLDERID_Profile` and
  `FOLDERID_LocalAppData`), and from those the environment the tests were started in names.
  One function answers it on every system, `host::lies_in`: in any case, through every link,
  one that leads nowhere yet included, and, on macOS and Linux, by device and inode. A test
  names its own Claude with `Context::with_scratch_claude`, which refuses a folder named
  through `..` or one that is, holds or lies in the account's own
  `Library/Application Support` or `/Applications/Claude.app`. A build for tests takes
  Claude's data folder from that scratch Claude alone, counts only its `Claude.app` as
  Claude, and reads no installed app's `Info.plist`. CI's Windows legs check after each run,
  with the probe and as the user the tests ran as, that no item of those families or
  `pitboard-citest-*` is in its Credential Manager, that Task Scheduler's `\Pitboard\` is
  empty, and that no real login folder appeared. A test that writes Credential Manager's
  items is in nextest's `credential-manager` group, which runs one at a time: two writers at
  once in one logon session can leave an item deleted for the next logon to find.
- Every program the command line's tests and `pitboard-ffi`'s tests start in place of a
  tool, or put where one is looked for, is one compiled program, the `pitboard` crate's
  example `stand-in`, playing the script written beside its copy
  (`pitboard_core::testing::stand_in`): never a shell script, so it is the same on every
  system. That covers `claude` and `codex` signing in, holding their output open, refusing
  a code and recording how they were started, Claude running under a test's own
  `Claude.app` for `ps` to list, npm's interpreter, a `systemctl`, and every
  program that is there to be found and never run, a fixture's tools in `pitboard-ffi`'s
  own tests among them. The stand-in makes each copy itself, so no test thread ever holds a
  program open for writing. A test finds it beside its own `deps` and refuses, naming
  `cargo build -p pitboard --example stand-in`, where it is not built, since only `cargo
  test` with no target named, or `cargo test -p pitboard`, builds examples; it is never
  built on demand, so a test binary run outside Cargo finds it too. Not it: the core's own
  tests, which cannot reach another crate's example, start no program a test wrote but the
  login shell's, in `host/unix/shell.rs`, whose `/bin/sh` is what they test, and the files
  they put where a lookup looks are never run; and a fixture an app's debug build launches
  into, which no stand-in comes with, writes its never-run programs as scripts.
- A read never settles an interrupted switch or an interrupted move. Every change settles an
  interrupted switch first, under Pitboard's lock. Where that change would stop at it as
  `recovery_undetermined`, `status`, `status_offline` and `doctor` say so in its words, from
  the same reads and the same decision (`read` and `decide` in `switch/journal.rs`, through
  `switch::stuck`), and that check takes no lock and writes nothing. `status_offline` and
  `doctor` send no request, so they say it only where the record and the logins tell it
  without one: a Codex login names its own account, and a Claude Code login renewed since
  the switch stopped is one only Anthropic can name. While a switch is waiting, telling
  reads the tool's login and the copy the switch parked, which on macOS are keychain items;
  with no record there, the check is one look at whether the file is there.
- `status` and `status_offline` say a Claude Desktop move waiting in `state.json`'s `moving`
  as `move_waiting` (`status::standing`), and `doctor` as its `desktop_move` check. A read
  opens no jar, so it does not predict what settling the move will do.
- Pitboard changes nothing where it runs as root or under sudo, or elevated on Windows, or
  where the host cannot say whether it does: a file it wrote would be root's or the
  administrators', and a keychain item might be, where the person's own runs might not read
  or replace it. On Linux root changes things where root owns every place a change writes in
  (`home::written`: the home, Pitboard's directory, Claude Code's config folder, its
  `.claude.json` and its login's folder, and Codex's home, each at its nearest folder there
  is), as on a machine whose only user is root. It is refused where another user owns one, as
  after `su -m`, which keeps the caller's `HOME`, plain `su`, which keeps their `CODEX_HOME`
  and the others, or a container given the caller's `~/.claude`, and where an owner cannot be
  read. On Windows `TokenElevation` decides, `TokenElevationType` says which words
  (Run as administrator, or an account whose every program runs elevated, which no terminal
  of it changes), a token of LocalSystem, LocalService or NetworkService is elevated
  whatever it says, and a token not read in full is unknown, which refuses. Nor does it
  change anything on a Windows older than 11 24H2, build 26100, which Windows Server 2025
  shares, read as `RtlGetVersion` gives the build and never from the product name; that
  refusal is `system_too_old`, said before how the process runs, since no way of running
  Pitboard changes the system. One gate, `permit::gate`, asks the host's `floor` and
  `elevation` and hands out a `Permit`, a value only it makes,
  and everything that changes anything takes one as an argument: `atomic`'s writes, every
  function in `host::fs` that makes, moves or removes a file or a directory,
  `RawStore::write` and `delete` (the keychain among the stores), `Scheduler::put` and
  `remove` and the service manager they ask, a tool's sign-in or warm-up run started
  (`Provider::sign_in`, `host::proc::run_tool`), and the token exchange, `Provider::renew`
  down to the request.
  So a change that skips the gate does not compile, and nothing rotates a refresh token it
  could not then write down. Every change asks the gate before it reads, locks or records
  anything, so a refused one leaves even the audit log as it was. Where the gate refuses,
  `status` answers what `status_offline` answers, with a `read_only` warning, the status
  line writes neither sessions nor readings, `doctor` fails its `system_too_old` or
  `elevated` check, first and in the gate's order, and an app writes no file of its own,
  since `app::write_file` takes a permit too. What each says, on each system, is written
  once, in `words::elevated` and `words::too_old`.
- A run of the daily renewal schedule renews the default home, whatever `PITBOARD_HOME` says,
  since the schedule is that home's alone: `Pitboard::renew` renews in the context
  `schedule::for_its_run` gives it. Such a run is told apart by what the scheduler says,
  launchd's job label in `XPC_SERVICE_NAME`, or, since what systemd passes a job is best
  effort and not to be relied on and Task Scheduler passes none, by the marker the unit's
  `ExecStart` and the task's action carry, `renew --scheduled` (`schedule::SCHEDULED_RUN`),
  which the command line passes on through `Context::started_by_the_schedule`. Only its
  parser knows the marker, as clap_complete writes a hidden argument into every shell's
  completions. The command line leaves `renew` to the gate, which asks of the homes the run
  renews in, so an empty or relative `PITBOARD_HOME` stops no run of the schedule. In a build
  for tests, such a run whose home is the account's own panics before it reads anything. For
  the same reason `schedule::install` refuses, with `schedule_not_default_home`, wherever
  `schedule::serves` is false, that is while `PITBOARD_HOME` names another directory.
  `uninstall` is not refused, so a schedule can be taken away from any home, and `repair` does
  nothing there, as before, without an error.
- The schedule's job is started with the system's environment, not the person's shell, so
  `schedule::install` gives it, through `Scheduler::put`, `PITBOARD_NO_ARGV` where the
  installing context refuses the argument line, and every proxy variable that context
  sets, under the name it was set by, even empty (`proxy::Proxies::variables`): the
  LaunchAgent's `EnvironmentVariables`, the systemd service's `Environment=` lines. A
  reinstall takes the current ones; `repair` keeps the proxy variables the installed file
  gives (`Scheduler::environment`). A proxy's address can hold a password, so
  `host::unix::service::write` writes every file of the job with `atomic::Perms::Secret`,
  mode 600 whatever was there, where it kept a file's own mode before. A task's definition
  has no environment, and an argument would show the password, so on Windows the proxy
  variables go in `schedule.env` in the default home, private as the parks are, and
  `schedule::for_its_run` gives a run the task started that file's variables over its own
  (`Scheduler::read_by_its_runs`). `PITBOARD_NO_ARGV` means nothing on Windows, so it is
  not kept. `doctor` reads the installed variables as a context's are read and warns,
  `schedule_proxy`, where they would send a request another way than this run's
  (`Proxies::same_way`). In the app, whose environment is the system's as the job's is,
  the same line is `ok` and says how each way of installing the schedule chooses its
  proxy: the app's switch would take a terminal's proxy away.
- The watch service is a second job on the same scheduler (`schedule::Job::Watch`,
  `watch_service.rs`). It runs `pitboard watch --scheduled --at <share>` on every system
  (`schedule::WATCH_RUN`): a LaunchAgent that runs at load and logs to
  `~/Library/Logs/Pitboard/watch.log`, a systemd user service enabled under `default.target`,
  or a task `\Pitboard\Watch <SID>` with a `LogonTrigger` for the person's SID and no time
  limit, which `put` starts at once after ending the run under the definition before. launchd
  (`KeepAlive`'s `SuccessfulExit` false and `ThrottleInterval`) and systemd (`on-failure`,
  `RestartSec`) start a run that failed or crashed again a minute later
  (`schedule::RESTART_AFTER_SECONDS`): a panic exits 101, which neither counts as a crash, and
  a run an update left behind ends on the state file the update's build wrote, which a start
  from the job's path mends. A run Task Scheduler started that ended starts again at the next
  sign-in. Its rules are the schedule's: it serves the default home alone
  (`schedule::serves`, `schedule_not_default_home`), a run of it watches that home whatever
  `PITBOARD_HOME` says (`Context::started_by_the_watch_service`, `schedule::for_its_run`), it
  is given the installing Pitboard's proxy variables, in `watch.env` on Windows, and the gate
  refuses it where Pitboard may change nothing. `uninstall` stops and removes it before the
  schedule, since it writes in the directory as it runs, and `doctor` reports it as
  `watch_service`. Its run is ended only while Pitboard's lock is held, so never in the middle
  of a renewal or a switch; a removal of renewal's task leaves its run to finish. On Linux, a
  watch service systemd will not stop stays installed and says why. The app may be switching too: a switch is decided again under the lock
  (`switch::auto`), and `tests/cli/watch.rs` has four processes decide from one reading and make
  one switch.
- On Windows `doctor` also reads what Task Scheduler keeps of the task (`Scheduler::task`):
  when it last ran and how that run ended, and the settings Pitboard writes that were changed
  there since (`task::drifted`). A result that is neither 0 nor Task Scheduler's own for a
  run not yet made or under way is a warning that names `pitboard renew`. A changed setting
  is a warning whose fix is turning daily renewal off and on again, which writes the task as
  Pitboard writes it; nothing rewrites it unasked, since the change may be the person's. The
  app's start-up repair (`schedule::repair`) points a schedule at the command line inside the
  app where doctor fails it (`schedule::fault`): the Pitboard it runs is gone, on every
  system, or it runs an app's own program. A task whose settings changed in Task Scheduler is
  left as it is. The
  MSI's full uninstall, through `Pitboard.exe --unregister`, takes each task away, renewal's
  and the watch service's (`schedule::uninstall_for_app`), only where it runs the
  `bin\pitboard.exe` beside that `Pitboard.exe`, whatever its settings, since the removal
  deletes that program. A task that
  runs Scoop's, WinGet's or another copy stays, and so do the parked logins and the Pitboard
  directory. A read Task Scheduler answers with an error, not with no such task, stops an
  install and an uninstall before either changes anything, so an uninstall never leaves the
  task running; the status and doctor take it for no task.
- No test starts the person's own login shell. A test names a shell of its own in `SHELL`,
  one that is not there, or hands in what a shell said.
- A test never reaches the system's own scheduler. A test context schedules through
  `MemoryHost`, which writes the files in the test's home and asks no service manager, and
  on Windows keeps the task's definition there as `Tasks\Pitboard\<name>`, and the real
  hosts refuse to ask launchd, systemd or Task Scheduler from any build for tests: a unit
  test, the command line the integration tests run, or an app built with the fixtures. The
  exception is the Release build the release gate's UI test starts at 1.x as a runner's fresh
  standard user. Its start, its Settings and its `--unregister` read that account's
  `\Pitboard\Renew <SID>`, where there is none, and change nothing in Task Scheduler.
- `pitboard-ffi` exports records, enums, three error types, free functions, one object,
  `PitboardModel`, and four traits an app implements, `ModelListener`, `AppControl`,
  `Notifications` and `LocalTime`. Nothing it exports is async. The core it runs on is
  `launch.rs`'s `AppCore`, which it does not export: a call to that is synchronous and may
  block on the keychain, a lock, the network or the person's login shell, so only the
  model's lanes make one. Making it blocks on none of them, since it reads its environment
  on first use. `PitboardModel` blocks on none of them anywhere: making one starts its threads,
  `send` only posts an intent, `snapshot` only copies the last snapshot, `shutdown` waits
  only for the actor to take what is already in its mailbox and for each sign-in under way
  to stop, and the core, a tool's sign-in and the app's `AppControl`, `Notifications` and
  `LocalTime` are called on the model's own threads. The free functions block on nothing,
  and an app may make them where it likes: `check_name` and `downloads_quit_question`,
  which read only what they are given;
  the account windows' rules, such as `decide_navigation`, `decide_response` and
  `window_note`, which read only what they are given as well, so a web view's delegate asks
  them as it is asked; `download_destination`, which asks the file system whether each name
  it tries is taken; `app_command_line`, which only joins paths; `can_run`, which asks the
  file system about one path; `pitboard_directory`, which reads the environment it is given
  and, without `HOME`, this account's passwd entry, or without `USERPROFILE` on Windows, its
  profile folder; `windows_records_key`, which reads only what it is given;
  `launch_refusal`, which asks the system whether this process is elevated and which build
  it runs on; `unschedule_app`, which asks the same, then Task Scheduler, and may take the
  renewal task away; and `fixture_names` and `fixture_page`, which read only what they are
  given.
  `PitboardModel::fixture` and `PitboardModel::fixture_in` make the fixture's world in its
  folder before they answer, files in a temporary directory and nothing slower.
- What a snapshot says is made by `present`, which reads the state and the moment and asks
  nothing of anyone but the app's `LocalTime`, for each clock time and whether a moment is
  on another day than now, and for each date and time of the activity log. Where that cannot
  say, a clock time is said in UTC and named so, from the moments alone, and a change's time
  as the log keeps it, as `pitboard log` prints it, so the machine's own time zone is read by
  the app's `LocalTime` and nothing else. The actor makes it before it takes the lock
  `snapshot` takes, so a `LocalTime` may call the model back. Text that depends on the time
  alone is made again on the minute tick, once started, and a countdown is the app's own,
  from `PanelNotice::until`.
- A run-out is notified once for each reset of its limit, by the core's rule for one reset,
  across launches: what was notified is kept in `told.json` in Pitboard's directory, read
  as the model starts. Nothing is advised on until it is in, and what was read meanwhile is
  advised on then, so a read that lands first never notifies again what an earlier launch
  did; what stands in for a read that failed, the last numbers measured, is not, as the
  Swift model's fallback was not. A record that is there and cannot be read is nothing
  told, and is written whole over it the next time something is told, so at worst a run-out
  is notified once more. The window's advice is worked out as the Swift model worked it
  out, from what this launch has told, so it still says a run-out notified before a
  relaunch.
- The app's own preferences, the tools somebody said "Not Now" to a second account for,
  whether the app has ever shown anybody anything, and whether it switches Claude Code by
  itself and at what share, are the model's, kept in `app.json` in
  Pitboard's directory, so they follow `PITBOARD_HOME`. Where that file is there it wins;
  where it is not, the model takes what the app's earlier store held, handed over in
  `AppLaunch::earlier_preferences`, and keeps it at once, so that store is read once. The
  model writes them only once they are read, from the file or, where there is none, from
  that store, and never in their place: a file that is there and cannot be read, because it
  is not this user's, a disk failed, another program holds it or it is not text, is left as
  it is for as long as the app is open, a "Not Now" said meanwhile holding until it quits,
  and is not taken for a first launch. Text that does not read as preferences is taken as
  no file, and what that store held is written over it. Until they are read no tool is
  nudged toward a second account, so a read that lands first never shows the step to
  somebody who declined it. No foreign trait reaches the app's own store for them.
- The app model's state belongs to its actor thread alone, and `State::apply` does no I/O:
  it calls neither the core nor the app, reads no clock and waits on nothing. It says what
  to run as jobs, which run on the model's lanes and answer as messages. A call the Swift
  model awaited is a job, except that a read's two stamps and the read are one job, and a
  look's two stamps are one, so no look lands between a read's stamps and the read. A read
  that started before a change, one the poll noticed or one this app made, counted in
  `changes_seen` when the read's job is made and compared when it lands, is dropped, and a
  read records when the index and the readings were written as they stood before it.
- A switch is claimed inside `State::apply`, as its intent is taken and before its first
  job is queued, and stays claimed until the read after it has landed, so one switch runs
  at a time and the poll leaves the account index alone meanwhile. The switch runs on the
  lane of changes, and what holds the login and quitting an app on the lane of processes,
  so a read answers while either waits.
- An app is quit only when the person says so, the way a person quits it and never by force.
  It is given 30 seconds, asked every 200 milliseconds whether it still runs and, once it
  does not, whether the core still finds anything holding its login (`Pitboard::holding`).
  What holds Claude Desktop's login is any program the core counts as Claude, so Claude's
  quit is over once the core finds none running. An app that has not quit by then stops the
  change before anything changes, and Pitboard says it is still open. A quit that lands
  later changes nothing and opens nothing. An `AppControl` that throws says the app still
  runs, so nothing is changed under an app nobody saw go.
- Pitboard opens again only the copy it quit, as soon as the change is over. A copy opened
  for a sign-in inside it comes to the front, and one opened after any other change stays
  behind Pitboard's window. Whether it opens turns on the tool:
  - The ChatGPT app, which holds Codex's login, always opens again, whether or not the
    switch worked.
  - Claude opens again behind Pitboard's window only where the core says its folder is
    whole (`Pitboard::whole`). The folder is not whole while `state.moving` holds the
    tool's record, nor where the index does not read. Nor is it where the tool's `in_use`
    names nobody, an account of it holds a folder park and no `awaiting` stands for it:
    Claude would open signed out with every account parked.
  - Claude opens for a sign-in inside it only where the index reads and no move of its
    login waits (`Pitboard::moving`).
- `Intent::QuitAndSwitch` names the account of the question it answers, as
  `AppModel.quitAndSwitch` took the question, so an app may close the question with
  `Intent::KeepAppOpen` before or after it sends the answer: a question closed unanswered is
  kept until another switch, sign-in or enrolment is asked for, and an answer is taken once.
  The snapshot's `WindowQuestion` carries both, the quit question's and the replace
  question's alike, so neither app words an answer.
- Changes to a tool whose login is its app's files never overlap. A switch waits until a
  park of the tool for a sign-in has started. No sign-in or recording of it starts while a
  switch of it is claimed. A copy another change quit meanwhile is opened after that
  switch, never while the switch moves the app's files.
- With its setting on, the app asks the core's look whether to switch Claude Code by itself
  after every read, after numbers or a change the poll found, as the setting is turned on or
  its share changed, and every 30 seconds after the last look ended, the core's
  `DECIDE_EVERY_SECONDS`, as `pitboard watch` decides. A launch looks first once its first
  read has landed and the preferences are read, or 30 seconds after the preferences where
  that read is slower: before it, the readings may be an older Pitboard's, which the look
  cannot judge, and a reason posted then would be gone seconds later. The look runs on the
  lane of reads and claims nothing. Only where it says a decision under the core's lock must
  say is a switch asked for, claimed the same way as one somebody asks for, with `switching`
  naming the tool rather than an account, since the core chooses which; and not while
  another switch, the question before one or another change of the app's own is under way.
  It runs on the lane of changes, behind any switch asked for, and asks nobody about
  quitting an app, since Claude Code follows a switch by itself. A refusal is the app's to
  pace, as it is `pitboard watch`'s, since the core records none it raised before deciding:
  the next is asked for no sooner than the core waits after as many failed attempts in a
  row, 60 seconds doubling to 15 minutes (`duty::cadence::retry_after`), and any outcome, of a
  look or of a decision, ends the row. A switch it made is taken as one asked for is, with
  the read after it, and said in a notification with no button; a reason it did not switch
  away from a limit, once for that limit and its reset, whether the decision gave it or a
  look stood on one recorded since, a reason it cannot judge, by what it names, each told
  apart as the core tells it (`Skip::told_apart`, `Blind::told_apart`), and a refusal, are
  said once each until it next switches, and never in an alert, since nobody asked. What
  the look or the decision came to last is said under the setting while it is on. Turned
  off, it says nothing there, and a look or a decision answered since asks for nothing,
  says nothing and holds nothing back, except a switch, which was made and is said. The
  setting is kept in `app.json` with the other preferences, and taken only once they are
  read.
- The notice that Claude Code's config names another account offers to write the account
  in use there: `Intent::UpdateConfig`, a change of the app's own like a rename and not a
  switch, so the row never reads as switching and nothing said about a switch or the
  account in use goes. The core refuses it, writing nothing, where another account is in
  use by then, so a button drawn before a switch landed never switches back. What it warned
  of is said beside the read after it, as a sign-in's is.
- The notice that a login is left in a file behind the keychain or Credential Manager offers
  to put it away:
  `Sheet::Stow`, which looks first, on the lane of reads (`Job::LookLeft`), and says whose
  the login is once that is known, in the words `pitboard stow` asks with, offering **Put
  Away** only for a login it can put away. `Intent::Stow` puts away the file as that look
  found it, on the lane of changes, a change of the app's own like a rename: what goes wrong
  is said in the sheet, which looks again, since what it found is out of date by then, and
  once it is done the sheet closes, what it did is said until it is dismissed, and the
  accounts are read again.
- A login replaced outside Pitboard, which every read says until it is put right
  (`login_replaced`), is posted once for each account while the reads that ask say it, and
  again only once one has stopped saying it and a later one says it, or after the app is
  opened again. The core says it only beside a read that answers. A failed read or a refused
  change whose warnings named one would post it the same way, which nothing sends today.
  Neither counts as a read that stopped saying one, since neither says whether a login it
  leaves out was put right. What was posted is kept by the account the warning is about
  (`Warning`'s `account`), since its words name whose login is stored now, which every
  switch changes, and not across launches.
- A notification the app's system refused is kept with its title, the system's reason and
  when, until the next one is posted, and not across launches. Doctor cannot know of it, so
  This Mac and This PC list it after doctor's checks (`notification_not_posted`), and the
  line over them counts it. `MacNotifications` refuses none.
- A read that asks nobody, after another front end changed the index or a session recorded
  numbers, that says Claude Code's config has named another account since Anthropic last
  named the login stored (`in_use_unconfirmed`) starts one read that asks, which settles
  whose the login is. Once for each thing it says, so a config that stays as it is costs no
  more reads. Claude Desktop's `in_use_unconfirmed` starts no read: no service is asked
  whose a Claude Desktop login is, so no read can settle it.
- Every other change this app makes to the account index holds it as a switch does, and the
  poll leaves the index alone meanwhile: naming the login signed in now, a sign-in's
  enrolment, parking the account in use for a sign-in inside its app, a rename, forgetting,
  writing the account in use into Claude Code's config, giving up on an interrupted switch
  and renewing parked logins. Each holds the index inside `State::apply` from the moment its
  intent is taken until the read after it is over, landed, dropped or failed, or until the
  change itself has failed. A sign-in's enrolment holds it from the moment its thread is
  told to enrol, and parking for a sign-in inside an app from the quit question's answer
  until the read after `SignInStarted`. So a change made elsewhere is noticed by the next
  look once none is under way. A look can find the index as such a change wrote it and land
  after the change has answered, before the read the change asked for: taken for a change
  made elsewhere, it dropped that read as one that started before a change, and a look that
  landed while a rename was made put away what the account's last switch said, as
  [A look and the app's own changes](docs/facts/app-model.md#a-look-and-the-apps-own-changes)
  measured.
- What a tool's last switch said is kept apart from the read's warnings, one per tool, until
  that tool no longer has the account it switched to signed in or the person puts it away:
  a Codex switch's warning that open sessions still use the account it parked, and must not
  sign out, outlives every read and every write that leaves that account signed in. A
  sign-in that puts a new login in use for the account in use is kept there too, beside
  what that tool's last switch said, since sessions already running keep the old login.
- One sign-in runs at a time, told apart by an id, on a thread of its own that starts the
  tool's own sign-in, hands on what the tool says and, once the tool has stopped saying
  anything, enrols what it signed in to only if it is still the sign-in under way. So a
  cancel comes before the tool has started, and stops it as it starts; while it waits on a
  browser, and stops it then; or once it is being enrolled, too late to stop, and the
  accounts are read. A code is typed back, and a tool stopped, on a lane of their own, never
  on the sign-in's own thread, which waits on the browser, and only while the tool has
  started and is still saying something. A code is typed only where the tool asks for one,
  once, and again once Claude Code has refused it: it reads another in the same sign-in, as
  its register's `sign_in_takes_another_code` holds. Once the model has gone, every sign-in
  still under way is stopped, so no thread waits on a browser for ever. `shutdown` returns
  once that has happened, each tool stopped and waited for, since an app's process ends once
  it returns and a tool the app started does not end with it; a sign-in still starting,
  which stops its tool as it starts, or enrolling what it signed in to, is waited for too,
  for no longer than ten seconds in all.
- A sign-in asked for after a cancel waits for the one cancelled: its thread is asked for
  only once that one has let go of the core's one sign-in at a time. The stop, which runs on
  the lane of sign-in calls, says so where it stopped the tool, once it has waited for it.
  Otherwise the cancelled sign-in's thread says so in its last answer, once its tool has
  failed to start, finished, or been stopped and waited for, or what it was doing has come
  to nothing. That answer also waits for the tool's output to close, which a program the
  tool started can hold open for as long as it runs, so the stop does not leave it to the
  thread. Meanwhile the new sign-in is shown as one whose tool has not started. Asked for at
  once, it started on a thread of its own before the stop and was refused as one already
  waiting. The core lets go of that lock by name as a sign-in is dropped, so
  `WatchedSignIn::cancel` returns once it is free, whatever processes other threads are
  starting ([One sign-in at a time](docs/facts/sign-in-lock.md)).
- A sign-in under way keeps the sheet it was started from: putting up another while it runs
  would leave the tool running with nothing on screen to finish or stop it. What goes wrong
  is said in that sheet, or in the window where the sheet has gone, and the sheet closes once
  the sign-in has finished.
- One change to the daily renewal schedule runs at a time, claimed inside `State::apply` as
  its intent is taken, and the settings' switch shows what it asked for until the schedule
  has been read back after it; a second press meanwhile does nothing. Turning renewal on is
  refused before the scheduler is asked, and nothing is read, where the command line inside
  this copy of the app is not one a schedule would keep reaching: none inside it, one in the
  temporary copy macOS runs an app from, by the core's own rule in `schedule.rs`, or one
  nobody may run. Turning it off never is. Whether that command line lasts is read from its
  file on the lanes, with the schedule, a change to it, the repair and the command line
  found, never as a snapshot is made. A schedule an older app wrote, or on Windows a task
  whose Pitboard is gone, is repaired once a launch, as the model starts, and read again only
  where it was. The app has no Renew Now: its reads renew parked logins, and `pitboard renew`
  renews what is due. Doctor's checks and the activity log are read each time their
  pane is shown, the checks counted while they run, as reads are. Opening at login and
  linking the command line onto the `PATH` with an administrator's password are each app's
  own, and once an app has linked it, it sends `Intent::LookForCommandLine` for the model to
  look for the command line again.
- `AppControl` names an app by one string, the id the core's holder detection gives it:
  on macOS its bundle id, on Windows its Application User Model ID. The ChatGPT app's are
  in `provider/codex/holders.rs`, and Claude's in `provider/desktop/mod.rs` (`HOLDER`).
- One thread tells a `ModelListener`, so snapshots arrive in revision order, and a snapshot
  is told only where it differs from the last, and of several waiting only the newest. So a
  test waits for what a snapshot says once something is over, never to be told one in
  between, such as one saying a read is under way, which can go untold. Nor is `snapshot`
  enough to see one in between: the actor hands an intent's jobs to their lanes before it
  publishes, so a lane can have started, even asked a service, while `snapshot` still gives
  the one from before. A test that must tell a read's end from its start holds the read, as
  the threaded tests hold one with the lock usage readings are written under. The model
  never holds a lock while it calls out, so a listener may call `snapshot`, `send` and
  `shutdown`. Dropping a `PitboardModel` waits on no thread, since .NET can free it from its
  finalizer thread ([The C# bindings](docs/facts/bindings.md#the-c-bindings)); `shutdown`
  waits for the actor and the sign-ins under way, neither of which waits on the listener.
- No test makes the app model over the machine's own environment. Its Rust tests run the
  real core over a `pitboard_core::testing::ScratchMachine`, a home in a scratch directory
  with `MemoryHost` and `ScriptedApi`, and so does a fixture. The C# tests make one only as
  `ModelTests.cs` does: every home a fresh folder, and never started or sent an intent, so
  it reads nothing,
  or a fixture's, which reads no machine and may be started. The Swift tests make one only
  as a fixture's, in `PitboardKitTests`, started from Swift; every other Swift test hands
  `AppModel` snapshots from a stand-in for `PitboardModelProtocol`, which keeps what it is
  sent. So the launch, a listener, an `AppControl`, `Notifications`, a `LocalTime`, a first
  snapshot and `Intent::Start` cross the bindings in a test. Against a library built with
  the `fixture` feature, the library calls a listener written in C# or Swift from a thread
  of its own, and an `AppControl`, `Notifications` and `LocalTime` written in C#, which a
  test hands `fixture_in`. Every `AppControl` a test hands the model is a stand-in that records
  what it was asked, never one that reaches a real app, and its `Notifications` keeps what
  it is given and posts nothing. A test that signs in runs the compiled stand-in as
  `claude`, in its scratch home, and plants the login it would have stored in
  `MemoryHost`'s keychain, or plays the tool's sign-in through the core's `SignInScript`,
  as a fixture does, which starts no program: never a real `claude` or `codex`. A test that
  schedules renewal does it through `MemoryHost`'s pretend scheduler, which writes its job
  into the scratch home and asks no service manager, and a test that looks for the
  `pitboard` a terminal runs finds one in its scratch home before it would reach any place
  of this machine's.
- A fixture is the real core and the real model over a machine of its own, never a core
  written again: what it plays is only what is outside the core, a tool's sign-in, the
  person at the browser, the other apps and the services. Nothing it does reaches this
  machine's homes, keychain, scheduler or network: its home, its tools' programs, which
  are never run, and the command line inside its stand-in app are files in its own folder
  in the temporary directory, its stores, process list and scheduler are `MemoryHost`'s,
  and Anthropic and OpenAI are `ScriptedApi`. Its preferences and what it has told are in
  its own Pitboard directory, and its account windows' records in its own folder, made
  again at each launch. Only a library built with the `fixture` feature has one, the
  bindings are the same either way, and nothing that ships is built with it. Only a debug
  build of the macOS app reads `PITBOARD_FIXTURE`, and one linked against a library without
  the feature stops at launch with the core's sentence, which names the flag. What the app
  adds to a fixture is native and in the fixture's folder or in memory: a login item that
  registers nothing, the command line linked in the folder's `bin` with no password, and
  the account windows' stand-in pages, which it serves from `fixture_page` on
  `pitboard-fixture://`. It finds that folder as Rust's `std::env::temp_dir` does, from
  `TMPDIR`, which Foundation's temporary directory does not read
  ([The fixtures](docs/facts/fixtures.md)).
- The app has no rule of its own for what the core decides: its home, Pitboard's directory,
  whether a path is a program, the sites and which links from outside it opens are asked of
  the core, and everything the menu bar, the menu, the window and the settings show, and
  what each button sends, is the model's, an account's own menu with it: what it offers,
  in what words, and what it holds back. Whether the copy of the app runs from a temporary
  place is the core's rule too, `schedule::in_a_temporary_copy`, which the model asks
  before it offers to link the command line. The account windows' records are kept
  under Pitboard's directory standardised as Foundation standardises a file URL, as the
  app kept them before the core said where it is: the app works that key out in
  `WebEnvironment.recordKey` alone and hands it to the model as `WindowsLaunch::key`. The
  Windows app asks the core for its key, `windows_records_key`: the directory as Windows
  compares paths, with `/` read as `\`, `.` and `..` taken out, no `\\?\` and no trailing
  separator, and each name in upper case one for one. It reads no file, so a short name or a
  link keys apart, as a symlink does on macOS.
- The macOS app's `AppModel` shows only a snapshot newer than the one it shows, so one that
  arrives late never puts back what a newer one replaced, and assigns each part only where
  it differs. `ModelListening` hands each snapshot to the main queue, which runs them in the
  order the model told them, and holds the `AppModel` weakly: the model holds the listener,
  and the `AppModel` holds the model. A view sends an intent and returns; nothing it does
  waits on the model. A sheet, the quit question and a failure's alert are the model's,
  shown from the snapshot and closed by an intent: `CloseSheet`, `KeepAppOpen` and
  `DismissFailure`. The window opens once for each `WindowRequest.serial`, on its pane where
  it names one: `WindowRequests` answers each serial once, seen as the menu bar item first
  appears or as the serial moves, so the first launch's request opens the window however
  early it comes, and one answered already is not answered again when the window opens some
  other way. The app stops the model as it quits, and `shutdown` returns once every sign-in
  under way has stopped.
- The macOS app hands the model what it kept in UserDefaults before `app.json`,
  `secondAccountDeclined`, `hasBeenSeen` and `hideSecondAccountNudge`, in
  `AppLaunch::earlier_preferences`, while `app.json` is not in the Pitboard directory the
  launch serves, and takes the keys out of UserDefaults once it is there: as it launches,
  or as it quits once the model has stopped. So a launch that stopped before the file was
  written hands them over again. What the old keys mean is the model's. It hands over the
  account windows' records, `webStores` and `windowPages`, the same way, in
  `WindowsLaunch::earlier`, and takes them out once `windows.json` is in the app's own
  folder.
- The Windows app asks the core's gate before anything else a launch does. `Launching` asks
  `launch_refusal`, which refuses where every change is refused: a release before Pitboard
  for Windows is released, a Windows build below the floor, an elevated process and a home
  that is not a full path. It asks before it judges a link or takes the single-instance key,
  and a refused launch shows the core's sentence and exits with nothing written
  (`ReleaseGateTests`, over the MSI's Release build). Only `--unregister` and
  `--unregister-share`, which the MSI runs, are read first. Neither asks a running copy, and
  `--unregister-share` asks nothing of the core. `--unregister` asks the core only to take
  away the renewal task (`unschedule_app`), which the gate every change passes refuses as it
  refuses any change.
- The Windows app's own data folder is never in Pitboard's directory. It is `Pitboard App`,
  or `Pitboard App Debug` for a Debug build, beside `Pitboard` in the known folder Local
  AppData, which `Shell.LocalAppData` asks `SHGetKnownFolderPath` for. So neither
  `PITBOARD_HOME` nor `LOCALAPPDATA` moves it, and `pitboard uninstall` never deletes the
  account windows' sign-ins. Under a fixture it is the fixture's own folder. A build for
  tests refuses both folders as real login places, compared by component, so `Pitboard App`
  never lies in `Pitboard` (`host::pitboard_app_on_windows`).
- Only a Release build an MSI installed registers anything with Windows. A Debug build and a
  fixture write no `Run` value, since `RunValue` refuses in a Debug build and a fixture gets
  `FixtureStartupItem`, register for no notifications (`WindowsNotifications`) and claim no
  link scheme ([On Windows](#on-windows) under Account windows). The MSI alone writes
  `pitboard` and the user `PATH` entry, and registers `PitboardShare.exe`'s package when it
  is signed; `Pitboard.exe --unregister` takes back what the app registered itself, and
  `--unregister-share` the package. CI's
  Windows legs fail a run that leaves a link scheme, a notification registration, a start at
  sign-in, a `PATH` entry or a package behind. At 1.x `ReleaseGateTests` starts the MSI
  image's Release build as the standard user, turns start at sign-in on, exits it and runs
  `--unregister`, which must take back both registrations, then removes the data folder as
  the MSI's `RemoveFolderEx` does.
- Only `PitboardShare.exe` has a package identity, which a share target needs. Its manifest
  holds the one `<msix>` element of Pitboard's programs, `check-windows-app.ps1` fails a
  Release build where another program holds one, and it hands a shared page to
  `Pitboard.exe` only as a Pitboard link, which `Pitboard.exe` checks as it checks any. That
  the `Pitboard.exe` it starts through `CreateProcess` carries none of its identity is
  measured only for a sparse-identity process started by path (CF8), not for one the share
  sheet activates, until the 1.0 session's P1, which shares from Edge, Chrome and Firefox,
  reads `GetCurrentPackageFamilyName` in the `Pitboard.exe` each share started
  ([Documented, not measured](docs/facts/windows-documented.md)).
- On macOS, only `/usr/bin/security` reads or writes Claude Code's keychain item and
  Pitboard's parked items. No keychain item is touched through the Security framework. The
  reason is in [docs/facts/macos.md](docs/facts/macos.md).
- On Windows a parked login is sealed with DPAPI to the person's Windows account, never
  kept in Credential Manager, the keyring crate or PasswordVault, and never read in the
  clear. It is sealed in CurrentUser scope, never with `CRYPTPROTECT_LOCAL_MACHINE`, with no
  UI, and with entropy binding a Pitboard constant to the park's name, so a file renamed onto
  another park's does not open. A file that does not open, after an administrator resets the
  person's password, on a profile restored on another PC, damaged, or planted as plain JSON,
  is unreadable and never absent, and a write opens what it wrote and compares the login.
  Sealing takes about 0.2 ms and opening 0.15 ms, so a `pitboard status` that renews a login
  and seals its copy costs nothing a person sees. The seal keeps other Windows accounts out
  and protects the file at rest. It does not protect against a program running as the
  person, such as Codex's unelevated sandbox (the register's `codex_windows_sandbox_token`),
  and a program started with such a token unseals a park and opens its file to read or delete
  it, though not to write it ([Windows runners](docs/facts/windows-runners.md)). A park copied to another PC opens
  there only for the same person with a roaming profile, which carries the key, and
  Pitboard's home is never in a roaming profile's roaming part. `doctor` says parks are
  sealed, leaves them out of the logins others can read, and makes two or more that all fail
  to open, as after a reset, one finding.
- Pitboard takes Claude Code's write lock, the same way Claude Code takes it, before writing
  Claude Code's login. It writes the login where it already lives. A switch takes Claude
  Code's refresh lock before that, as a renewal does (the register's `refresh_lock`), and
  holds it from before it reads the outgoing login for the last time until it has recorded
  the login installed: a session renewing meanwhile waits for it, so the copy parked for the
  account switched from is never one a session spent, with its renewal lost at a save that
  found another login stored. A renewal already under way holds that lock, and a switch
  waits for it as it waits for the write lock, then stops with `switch_in_progress`.
- Locks are always taken in one order. `signin.lock` comes first, where a sign-in holds it
  from its start to its enrolment, and `state.lock` next. A change of Claude Code then takes
  its refresh lock, its legacy one and its write lock, in that order. A change of Claude
  Desktop then takes the data folder's lock.
- Codex takes no lock on `auth.json`, so a switch reads the file again before replacing
  it. Every switch reads the live login back rather than trusting its own write.
- Codex truncates `auth.json` and writes it again in place, so a read that finds it empty or
  not JSON reads it again, up to ten times 10 ms apart, before taking it as it is.
- Pitboard never answers a failed keychain write by writing Claude Code's plaintext file.
  That demotion is Claude Code's to make.
- While Claude Code's `.credentials.json` sits behind the keychain, or behind Credential
  Manager on Windows, with a login in it, none, or what Pitboard cannot read, a session
  already running keeps its account after a switch until its login is next renewed.
  Pitboard tells the file by a look at it, as the session does, not by reading it. A switch,
  by hand or by itself, says so in place of a number of seconds (`Adoption::AtRenewal`), and
  every read that reads that store and every change warns while the file is there
  (`fallback_login`). The automatic switch still
  switches: new sessions take the account switched to at once, and running ones at their
  next renewal. A read and a switch never touch the file.
- Only `pitboard stow`, and the app's **Put Away** after its sheet, write or delete
  `.credentials.json` while the keychain, or Credential Manager on Windows, holds the login in
  use, and only once somebody has
  confirmed what the file holds. It looks first, taking no lock and writing nothing:
  `stow::find` reads the file, tells whose its login is by its refresh token's fingerprint
  where that is the login stored, a park's that reads back, or the record's, and otherwise
  asks Anthropic with the login's own access token, never with one that has expired; where
  that names an enrolled account, it tells whose the login stored is as a read does. Whether
  the file holds a login at all is `claude::live::holds_a_login`, a `claudeAiOauth` with a
  token in it, which every `fallback_login` warning and doctor tell it by too: a file that
  is not a JSON object, or holds no token, signs nobody in and is deleted as holding none,
  and a login with no refresh token whose access token has expired is dead. What the look
  found is shown with `Left::seen`, a fingerprint of the file's bytes, and `stow` goes ahead
  only while the file still holds those bytes, read again under `state.lock`, again under
  Claude Code's refresh lock (the register's `refresh_lock`), and again as the file goes.
  The refresh lock is held from that reading until the file is gone, taken as Claude Code
  takes it, `.oauth_refresh.lock` and then the legacy `<storage dir>.lock` with its 60
  second staleness: a session renewing a login takes it before it sends the refresh token,
  and saves the answer only where the stored login still holds the token it sent, so a
  renewal under way while the file went would spend the token Pitboard counted on and save
  nowhere. Claude Code's write lock, which excludes sign-ins but not that renewal, is taken
  after it, as Claude Code takes them, around the renewed login's write and the file's
  deletion only, so no network round trip holds up Claude Code's writes; a `/logout` that
  gave up waiting writes with no lock, so the file is read again under it. Additive before
  destructive: the file goes only once its login is parked for its account, or is shown to
  be kept already (the very login stored, or a park's that reads back), a second sign-in of
  the account in use, whose own login the keychain holds, or another login of an account
  holding a park it can be switched to, or dead, with Anthropic refusing its refresh token.
  A park that copies the file's login is written down as one, in `state.json`'s `from_file`,
  before the copy is written and until the file has gone. A run that stopped between parking
  the login and deleting the file leaves it in both, and a session that signs in with the
  file can renew it there, which spends the copy, and nothing local can tell that it did. So
  such a copy is never kept in place of what the file holds by then, which is parked in its
  place; any other park that reads back and has not expired is kept, whichever login is the
  newer. Where the file goes because of the login stored, the very login stored or a second
  sign-in, that login is read again as the file goes, and one a sign-in replaced meanwhile
  keeps the file (`stored_login_changed`). A login whose access token has expired is renewed
  as Pitboard renews the login stored to identify it (`switch::refresh`), read from the file
  itself, not through the keychain in front of it, and written back there before anything
  else. Where whose login is stored decides the outcome and that login has lapsed, the look
  takes it from the record, renewing nothing, and `stow` renews it as a change does. A
  login of an account nobody enrolled is refused, named by its email and organisation,
  without asking whose the login stored is, and nothing is deleted; the command line
  refuses it from the look, asking and trying nothing. Its other keys, such as
  `mcpOAuth`, are not Pitboard's to move: they go with the file, by name. Every error it
  stops with says how far it went (`Error::StowStopped`, with the code, cause and exit
  status of what stopped it): nothing changed, the login stored renewed to tell whose it is
  and kept for the next change to save, the file's login renewed and written back, renewed
  and kept for the next change to write back, renewed and lost, which leaves it spent, or
  parked, which stays. Where telling whose login is stored loses that login's renewal, the
  error is the renewal's own (`Error::RenewalLost`): nothing of the file's has moved by
  then. A run that stops anywhere leaves the login in the file, in a park, or in both, or,
  between Anthropic answering a renewal and the answer reaching the file, in the vault copy
  the next change writes there. A renewed login is lost where a sign-in or a `/logout`
  replaced the file's login meanwhile, and where neither the vault nor the file takes it, as
  the renewal of the login stored says below. A copy waiting for Claude Code's slot is saved
  before `stow` reads the file, and where it cannot be yet, `stow` stops with nothing
  changed. A session that signed in with the file, as one over SSH does, is signed out once
  it is gone. The activity log records it as `stow`. On Linux the file is the store, nothing
  is behind it, and there is nothing to put away. On Windows a session out of Credential
  Manager's reach, as one in a sign-in over SSH with a key, leaves one there, and it goes the
  same way (`credman_unreachable_writes_file`).
- A Codex login is moved, never copied (`ParkSemantics::MoveOnly`). The parked login is
  read back before the incoming login is written. Codex's own sign-in and sign-out revoke
  the stored refresh token, so two usable copies of one login must never be at rest.
- No Claude Code or Codex login moves until Pitboard knows whose it is: asked of its
  service, which for a Codex login is its ID token, or known by its refresh token's
  fingerprint from an earlier answer about that very login. A login with no refresh token
  has no fingerprint and is always asked about.
- Nobody can be asked whose a Claude Desktop login is: Pitboard asks claude.ai nothing and
  decrypts nothing. It knows a login by the fingerprint of its session cookie as stored,
  recorded when a person enrolled it or Pitboard put it in use. A session it has not seen
  is taken as Claude names it in one case: `config.json` names the account the record says
  is in use, that account holds no parked login, and neither token cache holds the value
  Claude leaves after a **Log out**. That is Claude replacing its own session, seen once on
  5 October 2026. It lets a read say the account is in use and a switch park it, and the
  switch says so. It never lets anything be deleted. A session it has not seen beside a
  token cache Claude emptied is a sign-in Claude has not finished, whatever the settings
  name. Every other state is doubt: each change refuses with nothing changed, and
  `pitboard enroll` under a label, the person's word, is the way out. This is the one place
  Pitboard leans on a tool's own record of who is signed in. Whether a sign-in over a stale
  `lastKnownAccountUuid` followed by a quick quit leaves the new session under the old name
  was not run ([Not measured](docs/facts/not-measured.md#claude-desktop)).
- Who is in use is one record per tool, `state.json`'s `in_use`, written under
  `state.lock`. Every reader that does not read the store takes it from there, and `forget`
  asks as a switch does. A Claude Code or Codex record is written only where a service has
  just answered for that login, or a fingerprint ties it to an earlier answer. A Claude
  Desktop record is written only by a change of Claude Desktop, which reads the folder with
  Claude closed.
- A read of Claude Desktop opens `config.json` and `config.json.journal`, and
  `Claude-3p/claude_desktop_config.json` where `Claude-3p/config.json` is there, and nothing
  else of Claude's, and none of it while no Claude Desktop account is enrolled. It lists no
  process, records nothing, and says the record with when a change last read the login
  (`read_at`).
- Claude Code's config is a sign that something signed in, and the words of a warning, never
  whose a login is: a Claude Code process started or signed in on another login can rewrite
  it with its own account. Of Pitboard, only a switch writes it, and `use` of the account
  already in use where it names another; the app's button for that is `use` refused where
  another account is in use by then. Each records `named` once the write lands, and keeps
  what the config named when the store was read where it fails, so every read says the
  config names another account. A read never writes it: a process starting on another login
  writes its own account back. A read records nothing for a tool whose config names another
  account than when the read began, so it never files what it saw over what a change wrote
  since.
- The login in use is its tool's to renew. Pitboard never renews a Claude Desktop login, in
  use or parked. It renews Claude Code's only where it must tell whose it is and its access
  token has lapsed, which a session signed in with a file behind the keychain leaves it:
  every session pinned to the file, nobody renews the keychain's. It renews it as Claude
  Code does (`switch::refresh`, the register's `refresh_lock`), so a session renews after
  it, never beside it: under Claude Code's refresh lock, holding `state.lock` first, it
  reads the login again and tells one renewed meanwhile as it is, sending nothing;
  otherwise Claude Code's write lock is taken once and the login read again before the
  refresh token is sent, so a writer in the way stops it with nothing sent. The name of a
  copy of the answer is then written down in `state.json`'s `renewing`, which the vault has
  to answer for, so a vault that cannot be read stops it with nothing sent. Once Anthropic
  has answered, the copy is written into the vault under that name, the write lock is asked
  for again for 30 seconds, twice its staleness, and the answer is then saved without it,
  as Claude Code writes on once its own lock is lost. It is saved as Claude Code saves a
  renewal: read from the store that keeps it, and written over the login there only while
  that still has the refresh token sent, other keys as they are by then. The copy goes once
  it is saved or the store holds another login by then, and a name with no copy written
  under it goes too. A run killed in between, or a save that fails, leaves it, one per
  slot, and settling saves it (`refresh::finish`). While it cannot be saved, the store
  still holds the spent refresh token, so no change renews, parks, replaces, puts away or
  deletes that slot's login: `refresh::finished`, which `identify::look`, `stow` and a read
  that must renew call before anything else, stops each with why, and a Codex change goes
  on. `uninstall` keeps the home while a copy waits. A copy whose write fails landed
  nothing, as a save counts it, and is let go of; one written and not read back may hold
  the answer, and is kept. One window is left, as in Claude Code's own renewal: on macOS
  the vault is the keychain Claude Code's login is in. Where macOS cannot ask for its
  password, as over SSH, a keychain that locks during the exchange, one round trip, takes
  neither the copy nor the answer. The renewal is then lost. A change says the refresh
  token stored is spent and to sign in (`Error::RenewalLost`, with the code of what stopped
  the save), and a read marks the account in use `login_refused`, as Anthropic refuses that
  token from then on. A save made without the write lock is said as `lock_compromised`. A
  login Anthropic refuses for good, or one with no refresh token, is `session_expired` with
  the cause `login_refused`, with nothing written; a 401 where nothing renewed stays
  `session_expired` with `token_expired`, telling the person to run the tool once. Usage
  alone never renews it, nor a look that takes no lock, such as the automatic switch's or
  `stow::find`, and Codex's never.
- Nothing outside `pitboard-core` writes Pitboard's index. Every change goes through
  `switch`, which records what it is about to do first.
- An interrupted switch of a Claude Code or Codex login, in `journal.json`, is finished
  before another change starts, a change of Claude Desktop included.
- An interrupted move of a Claude Desktop login, in `state.json`'s `moving`, is settled with
  Claude closed, before a change of Claude Desktop and before `repair` and `uninstall`. A
  change of Claude Code or Codex, a renewal, a read and the automatic switch neither settle
  it nor wait for it. `repair` goes on past a move it cannot settle and says why
  (`move_not_settled`).
- `abandon` gives up an interrupted switch first, whatever Claude Desktop's state. It then
  settles each waiting move with Claude closed, and gives up one that cannot be settled. A
  move recorded under another data folder than the one this run reads is given up without
  reading that folder. Where a move cannot be given up and `abandon` has done nothing yet,
  it refuses. Where it has already given up or settled something, it says the move as
  `move_not_given_up` instead, since the refusal's "nothing has changed" would be false.
  Why it was not given up is the core's clause in both front ends
  (`words::not_given_up_because`), each naming the account its own way and adding its own way
  out but for a park Pitboard could not look at (`words::readable_first`), and the summary of
  a move given up is `words::given_up` in both, each with its own way on.
- `adopt` drops each waiting move and awaited sign-in, since they were about the other
  computer's Claude. Each folder a dropped move names that no account holds is kept aside
  as `another_machine`.
- A change of Claude Desktop that refuses or fails carries what it had done before, a
  settled move included, in its error (`Error::SaidFirst`). The service puts that first in
  `Failed::warnings`, after any interrupted switch it finished before the change ran, and
  counts both in `Failed::said_first`. The command line says them ahead of the refusal.
- An answer from Anthropic that Pitboard read whole replaces a Claude Code account's
  reading, and a session only moves that reading, within the limits the answer gave. So
  numbers filed under the wrong account go at that account's next answer.
- What the status line writes after each response, `sessions.json` and a session's move of
  a reading in `usage.json`, is replaced whole and not flushed (`atomic::write_unsynced`):
  on a busy disk a flush took longer than 300 ms
  ([Windows runners](docs/facts/windows-runners.md)), and Claude Code cancels a run of the
  line still going when it starts the next, 300 ms after a change (`statusline_run_timing`).
  On exFAT, FAT32 and an SMB share the rename still writes through. A power cut may take
  either file back to an earlier copy or leave it unreadable, which reads as no reading and
  no run. An answer's reading and an account forgotten are flushed, so a forgotten
  account's reading does not come back.
- Pitboard files usage only under the account it is proven to be of: an answer for that
  account's own login, or a session's numbers whose windows that account's answered reading
  holds, where no other account's reading does. That reading holds no other running window
  of a limit the session passed, and where it lists every limit of its account, it lists
  each one passed. Never under the account Claude Code's config names, nor by the time
  since a switch: a session goes on with the login it holds for as long as it takes to
  follow one. Windows are told apart by their resets, to the minute. So a session in a
  window its account's reading does not hold yet can be filed under another account whose
  window resets within a minute of it, for as long as both windows run: each answer for
  that account puts its reading right only until the session's next response. A weekly
  window passed with it tells the two apart where that account's reading holds another
  running weekly window for all models, or lists every limit and none such. A session that
  passes the five-hour limit alone tells nothing apart.
- Pitboard switches by itself only for a front end somebody asked to: the app with its
  setting on, or `pitboard watch` running. It switches Claude Code alone. A running `codex`
  never follows a switch, and a Claude Desktop login moves only while Claude is closed. The
  status line and the daily renewal schedule never switch, and an automatic switch never
  quits an app.
- An automatic switch is decided once, under `state.lock`, from one telling of whose login
  Claude Code has stored, and made from that same login, so nothing comes between the
  decision and the switch. `autoswitch::look` comes first, from files alone: it takes no
  lock, sends no request, reads no keychain and records nothing, so a look that finds
  nothing to do costs nobody anything. It goes on to the lock only where a switch may be
  due, the record of whose login is stored is in doubt, a reason not to switch was not
  recorded yet, or another run is in the middle of a switch. A switch's record,
  `journal.json`, is of one interrupted (`switch_interrupted`) only while no run holds
  `state.lock`. So where the record is there, the look asks whether the lock is held, and
  still waits for nobody (`switch::unfinished`): it opens the lock's file to read, never
  making it, asks for the lock shared without waiting, and lets go at once. Held, the
  switch is that run's, and the decision waits for the lock and judges what that run
  leaves. Under the lock, `switch/auto.rs` tells whose it is as a switch does, by its
  fingerprint or by asking Anthropic, records it, and decides from that and the files as
  they are then. So the app, `pitboard watch` and a person's own `pitboard use` never make
  two switches from one reading, and a sign-in outside Pitboard is judged as the account it
  signed in. Where whose login is stored cannot be told, nothing is judged from the record,
  which may be another account's: that is recorded once, and asked again a minute later,
  then twice as long after each such failure in a row, up to 15 minutes. A record of whose
  it is made since, by a read or a switch, ends both the wait and the row. Only what every
  front end would meet alike is waited on so: a store one process cannot read, such as a
  keychain locked over SSH, is that front end's refusal, and holds back no other. The
  attempt is written to `autoswitch.json` once the switch is decided and before anything
  moves, so a switch killed midway still counts against that limit's attempts. One that
  failed for a reason trying again may mend, Anthropic out of reach or Claude Code writing
  its login, is taken back out of them, so an outage never uses them up. The next try waits
  as long, for every limit of the account, since what failed was the moment. One refused
  over the account switched to passes that account over for every limit of the account in
  use until the limit it was refused for resets; any other refusal counts, a login too
  large to write among them, which is about the machine and not the account.
- The automatic switch judges only a reading Anthropic gave for the account's own login, of
  the account in use and of each it could go to, and judges every limit at the share: one
  already switched away from, or tried three times, before it resets never hides another.
  Where none is left to try, that is said at once, never as a wait or the account settling,
  and recorded with the attempt that failed and left none, so a front end that waits after
  that failure finds it standing. A wait or the account settling is said of the limit tried
  next. Each reason it does not switch away from a limit at the share is said and recorded
  once for the account, the limit, its reset and the reason: under the lock, in
  `autoswitch.json` and as `auto-stay`. The look after that stands on the record and takes
  no lock. A front end is told the reset the reason was recorded under, and says it once
  for the same four. What is recorded of a window is kept until a minute past its reset, as
  late as an answer may give it, so an answer a second later neither forgets a limit
  already left nor says a reason again. What tells one reason, or one wait, from another
  is the core's one rule, `Auto::told_apart`, and every front end says by it, building no
  key of its own, so the app and `pitboard watch` never tell them apart differently. What
  each says is the core's sentence too (`words::auto_waiting`, `auto_switched`,
  `auto_skipped`, `auto_not_watching`), given clock times as each writes them.
- Additive writes become durable before destructive ones. A run that dies midway through a
  Claude Code or Codex switch leaves a spare copy of a login, never a missing one.
- A Claude Desktop login is never copied, so its move has no spare. Before the first
  rename, `state.json`'s `moving` names the two places each item may be: Claude's data
  folder and one folder under `parks`. A run that dies midway leaves each item in one of
  them. Settling takes items out of Claude's folder and never puts one in, and writes the
  account's keys back there only to undo a leave whose jar never left.
- The `--json` contract changes only on purpose. A change to a snapshot is a change to the
  contract.
- The JSON that `--json` prints is ASCII alone, on every system. Each character outside
  ASCII is a `\u` escape, and one beyond U+FFFF its UTF-16 surrogate pair, so a program
  that decodes the output in a code page other than UTF-8 parses the same values.
- The state file is read forwards only.

## Boundaries

- The core and its front ends meet at `service::Pitboard`, `context::Context` and the types
  those two return. That is the supported interface of `pitboard-core`. The other public
  modules are public only so the front ends in this repository can reach them, and may
  change in any release.
- In that interface, adding an error, warning or check code is not a breaking change.
  Renaming or removing one is. `pitboard-core` has a version of its own, apart from
  Pitboard's, and a breaking change gets a new minor version of it.
- Pitboard and each tool meet at the tool's register. What Pitboard relies on about a tool
  is written there, with the systems each fact was read on. The conformance run checks
  Claude Code's and Codex's registers against their newest builds twice a week. A person
  checks Claude Desktop's by hand on an installed copy, since where its builds are
  published is not recorded.
- Pitboard and each service meet at a few requests. The list, with what each request
  carries, is in
  [What leaves your machine](https://docs.usepitboard.com/security#what-leaves-your-machine).
- Pitboard and claude.ai or chatgpt.com meet at an account's window, which a person opens.
  The site's own pages sign the window in, WebKit keeps that sign-in in the account's
  store, and Pitboard decides only where each navigation goes.
- Pitboard and a browser meet at the Share extension, which hands the app the one link the
  browser shares, as a Pitboard link. Pitboard reads nothing a browser keeps.
- Pitboard and Claude meet at Claude's data folder. Pitboard reads `config.json`,
  `config.json.journal` and the names of the folder's first-level entries and of
  `IndexedDB`'s. Beside it, it looks for `Claude-3p/config.json`, and where that is there
  reads `deploymentMode` in `Claude-3p/claude_desktop_config.json`. From `Cookies` it
  reads the store's versions in `meta`. It reads the host, name, path, expiry and stored
  value of every cookie for `claude.ai` or `.claude.ai`, and of any cookie with a session
  cookie's name. It keeps only the `sessionKey`'s expiry and an 8-byte fingerprint of its
  stored value. It decrypts none of it, reads no keychain item for Claude Desktop and asks
  claude.ai nothing. It opens `Cookies` and changes the folder only while Claude is closed.
  It moves the items `Desktop::items` names by rename, and the settings keys by rewriting
  `config.json`, holding an advisory lock on the folder meanwhile. It deletes nothing of
  Claude's, `config.json.journal` included.
- Pitboard and a running Claude meet at the process list and at the app's `AppControl`.
  The core tells that Claude runs by the identifier in the `Info.plist` of the bundle each
  program runs from, and neither starts nor quits it. The app quits Claude only when the
  person says so, and opens again only the copy it quit.
- The code and the release meet at a `v` tag, which `.github/workflows/release.yml` turns
  into a release. [RELEASING.md](RELEASING.md) has the procedure.

## The state file

`~/.pitboard/state.json` is Pitboard's index: which accounts it knows, where each one's login
is parked, and whose login each tool has stored. It carries a `schema` number.

The app and the command line inside it update together. A command line installed another
way updates by its own route. So on one machine, an older Pitboard can meet a file a newer
one wrote.

Reading forwards is `state::migrate`. Each schema bump adds a step after the ones before
it, so a file two versions behind comes forward in one read.

Reading backwards is not possible. The older Pitboard refuses the file and says to update
it. A bump needs a test that loads a file the previous version wrote.

Schema 4 records each account's tool, and for each tool the account Pitboard last switched
to. A schema 3 file is brought forward on its first read, with no keychain item or vault
file touched.

Schema 5 gives each account an `id`, set at enrolment and never changed. Its parked logins,
readings, usage history, budget and windows are filed under it. A login is matched to its
account by its tool's identity, which for Claude Code is the account and the organisation
together (`Account::owned_by`), so one person's two organisations are two accounts. An
account brought forward from schema 4 keeps its account uuid as its `id`, and nothing filed
under it moves.

Schema 6 records whose login each tool has stored, as its service last said (`in_use`), in
place of the account Pitboard last switched to (`active`). A tool's record holds the owner the
service named, the fingerprint of the login it named it for, when, and the account the tool's
own record named then. A switch first records the login it finds in the store, then the one
it installs with what the tool's own record named, then, once it has written the incoming
account there, that it names that one. Enrolling the account signed in, a sign-in put in use
and finishing an interrupted switch record theirs, and a read records what it finds where
that changed. Each goes through `State::identified`. Where the owner changes and the account
before it has nothing parked and no other slot's record names it, that account's only login
is gone, and the account says when (`replaced_at`) until it holds a parked login or is in
use again: its row reads `login_replaced`, and every read warns with `login_replaced`.
Adopting a directory from another computer clears it, with the records, since both were of
that computer's stores.

Each `CLAUDE_CONFIG_DIR` or `CODEX_HOME` names a store of its own, its credential slot, so a
tool has a record for each slot it was read under. `in_use` holds the one of the slot `slot`
names; the others wait in `other_slots`, by slot, and `load` puts the slot it reads in place
of the one `slot` names. A read records what it finds, so two front ends under two slots
take turns writing the file, and a record either dropped would leave the next read under the
other nothing to tell a sign-in outside Pitboard by. An account another slot's record names
still has a login stored there, so a sign-in that replaces its login in one slot leaves it
that one, and nothing says it was replaced.

A schema 5 file comes forward with each tool's `active` label as that account's record, with
no login and `known_at` 0: nothing established which login the store held. A label naming no
account names nobody. Schema 6 is a bump rather than a field beside `active`, so an older
app or `pitboard watch` running beside a newer one refuses the file and says to update, and
does not go on deciding who is in use from Claude Code's config.

Schema 7 adds `renewing`, the renewals of a tool's own login not saved where the tool keeps
it yet, which a schema 6 file reads as none. The field alone would not do: 0.10.0 reads a
file with it and drops it at its next save, leaving the copy of the renewed login named by
nothing, so the next renewal of that login, by this version or by 0.10.0's `stow`, sends the
spent refresh token again. So it is a bump, and 0.10.0 refuses the file and says to update.

Schema 8 holds a login kept as files, Claude Desktop's, beside one kept as a document. A
park says its `kind`: an item of the vault leaves it out, and `folder` is a folder under
`parks`. A folder park's `refresh_fingerprint` holds its session's fingerprint, and its
`session_expires_at` the date its session cookie carries. `discarded` lists each parked
login waiting to be deleted as an object, its `service` and its `kind`, where schema 7
listed names. A schema 7 file's names come forward as items of the vault. A folder in
`discarded` says `forgotten` only where `forget` or `uninstall` listed it, and only then
is a folder that holds an item deleted. `park::purge`, `pending`'s sweep and `repair`
reach a park of each kind through `park::parks(kind)`. Claude Desktop's `in_use` record
names the account Pitboard filed the session under, since no service names one, and its
slot is the data folder's path. 0.13.0 and earlier read `discarded` as names and would
call a schema 8 file corrupt. So it is a bump, and every earlier release refuses the file
and says to update.

`aside` lists each folder under `parks` that no account holds as its park, with why it was
set aside (`in_the_way`, `superseded`, `another_machine`, `given_up` or `orphan`), the
account it was parked for and the fingerprint of the session it holds. `pitboard repair`
gives one back to its account where its manifest, its keys and its jar agree. `forget` of
that account and `uninstall` delete it, and nothing else does.

`moving` holds, by tool, one half of a move of a login kept as files: a `leave` into a
folder under `parks` or an `arrive` out of one. It is written before the half's first
rename and removed by the save that records what the half came to. It names the account,
the folder, the session's fingerprint, the items and the digests of the keys that move,
each folder the move reserved for what it sets aside, and the data folder.

`awaiting` holds, by tool, a sign-in inside the tool's app that Pitboard waits for: the name
asked, the account that name has already, and the account parked for the sign-in, which
`pitboard use` puts back.

A file naming a tool this build does not know is reported as written by a newer Pitboard,
not as corrupt. The advice for a corrupt file is to delete it, and following that here would
orphan every parked login.

## Tool registers

Every fact Pitboard relies on about a tool was read out of one build of that tool. Each tool
keeps its facts in a register, `crates/pitboard-core/src/provider/<tool>/assumptions.rs`,
dated with the build they were read from.

A fact names literals a build must contain (`probe`), or literals whose arrival would
disprove it (`absent`). A fact about behaviour, such as the 30 second cache, names no
literals.

A tool's builds for different systems carry different code, so beside its facts each
register keeps a table, `PER_SYSTEM`. It has one line for each fact, and the line says what
the fact is on macOS, on Linux and on Windows (`assumptions::OnSystem`):

- `Read(build)`: read from that system's build of that version. A fact's own
  `verified_against` is the build each of its macOS and Linux readings names, and
  `assumptions::verified_on` gives the build for any one system.
- `NotRead(reason)`: not read there, and why. Claude Code's Linux and Windows builds have no
  keychain backend, so its keychain facts are read from its macOS build alone. Pitboard
  supports Claude Desktop on macOS alone, so every one of its facts is `NotRead` on Linux
  and Windows.
- `Pending { by, reads }`: to be read there by a later pull request of the Windows work,
  `W2` to `W27` with `W23` split into `W23a` and `W23b`, or `W-warm`, which says what that
  pull request reads. `assumptions::pending` lists
  these, and the Windows work is not done while the list has anything in it.

Where a tool does something else on Windows, the Windows behaviour is a fact of its own,
such as `credman_target` beside the keychain's `credential_service_name`. It is never a
second reading under the first fact's name.

`pitboard-conformance` reads a tool's program, or the archive of a tool shipped as an
Electron app. It tells a program's system by its header: ELF for Linux, Mach-O for macOS,
and PE for Windows, for x64 and ARM64 only. An `app.asar` stores its files uncompressed and
has no system of its own, so the checker reads it for the system the tool's register names
in `assumptions::archive_platform`: macOS for Claude Desktop, and none for Claude Code or
Codex. It refuses any other file, the archive of a tool whose register names no system,
and the program of a tool whose register is read from its archive. It reports which facts
can still be read from the build, which have moved, which name nothing to look for, which
it skipped and why, and which wait on the Windows work.

The check is shallow on purpose. A literal being present does not prove the behaviour around
it is unchanged. A literal disappearing, or a ruled-out one appearing, does prove something
moved.

`.github/workflows/conformance.yml` checks the newest builds of Claude Code and Codex against
their registers twice a week, or a version given by hand. It reads four builds of each,
`linux-x64`, `darwin-arm64`, `win32-x64` and `win32-arm64`, all on Linux. Its most recent run
says which facts can still be read from each build it checked, and which have moved. No job
reads Claude Desktop's register: where Claude's builds are published is not recorded, so a
person runs the checker on an installed copy.

Adding a tool takes three things: a register read out of a named build, with its table; a
module under `provider/` implementing `Provider` for a login kept as one JSON document in a
store, or `Folder` for one kept as items of an app's data folder; and a check of later
builds, an entry in `conformance.yml`'s matrix where the tool's builds have a recorded
source. `provider::of` is the one place that says which kind a tool keeps. `ProviderId`,
`ProviderId::ALL` and the matches in `provider::of`, `assumptions::of`,
`assumptions::per_system` and `assumptions::archive_platform` name every tool. The compiler
and the tests then point at what an added tool has to fill in.

What nobody has measured of a tool is in [Not measured](docs/facts/not-measured.md). How to
run the checker and add a fact is in
[Tool registers in CONTRIBUTING.md](CONTRIBUTING.md#tool-registers).

## Account windows

The app gives each enrolled account a window on its tool's site: claude.ai for a Claude Code
account, and chatgpt.com for a Codex account. The site's own pages run in it, in WebKit on
macOS and WebView2 on Windows, with a store of website data that belongs to that account
alone. A page shared from a browser's Share menu reaches the app as a Pitboard link, and the
person chooses which account's window opens it.

The windows read the accounts from the model's last read, and their bookkeeping is the
model's, so the Windows app's WebView2 code gets the same decisions: which store is whose,
what each window opens at, which windows close and which stores go after a read, the link
waiting for an account and the downloads. Each app's web code says what happened and does
what the model says. No window is ever given a Claude Code or Codex login. The facts this
rests on are in [WebKit and SwiftUI](docs/facts/webkit.md) and
[claude.ai and chatgpt.com](docs/facts/services.md#claudeai-and-chatgptcom).

### Where the code is

- `crates/pitboard-sites`: what a site is, and what a link from outside may be, in the
  [code map](#code-map). The app reaches it through `pitboard-ffi`'s `Site` and `SiteLink`
  records, and the model reads each Pitboard link with `pitboard-sites`' `read_pitboard_link`
  and says a refusal, and what the Open Link window says with no link, in its words. The app tests build a site record by
  hand. The Share extension reaches it through `pitboard-share-ffi`.
- `apps/macos/Sources/PitboardLinkTarget`: where a Pitboard link goes, which the app and the
  Share extension both link. `LinkTarget.swift` names the Info.plist key `PitboardURLScheme`
  that gives each build its scheme, and finds the app an extension is inside.
- `crates/pitboard-ffi/src/account_windows`: the windows' rules, in the
  [code map](#code-map). Which accounts have a window, the store each one's data is kept
  in, the menus' entries and the forget alert's text; where each navigation, new window,
  response and download of a page goes; what a window says above its page and before it
  removes what it keeps; what a page may use and close; whose words a dialog says; what a
  download is called and where it comes from; what becomes of a page whose content stops;
  and how big a sign-in window opens. The Swift below turns what WebKit says into what
  these read, and does what they decide. `records.rs` is the windows' records.
- `crates/pitboard-ffi/src/model/windows.rs`: the windows' bookkeeping, in the model's
  state. It records a window's store before the window opens and says the page it opens
  at, puts away what a read no longer lists, asks the app to delete each store that goes
  and hears how it went, holds the link waiting for an account with the wait before Open
  answers, and follows each download from its start to its end. `present/windows.rs`
  says it in each snapshot, as `AccountWindowsShown`, with what a window says until it can
  show its page and each window's button that clears its downloads, and
  `downloads_quit_question` words the question before quitting for as many downloads as an
  app has under way. `model/windowing.rs` tests it.
- `apps/macos/Sources/PitboardApp/AccountWindows`: the windows.
  - `WindowAccount.swift` gives a window's store as the `UUID` WebKit names a store by and
    the scene keeps a window's value as.
  - `NavigationPolicy.swift` turns WebKit's URLs and a navigation's target frame into what
    `decide_navigation` reads. `WindowNote.swift` is what a window says above its page.
  - `Page.swift` owns one `WKWebView` and publishes its title, address, progress and
    history, in the shape of SwiftUI's `WebPage`. `PageDelegate.swift` answers WebKit's
    navigation and UI delegates for a page by asking the core's rules.
  - `WebSession.swift` is one open window: its account, policy, page, note and sign-in
    window. `PopupWindow.swift` is that sign-in window, an AppKit window, since WebKit needs
    its web view back before a SwiftUI scene could open.
  - `Downloads.swift` keeps every window's downloads past the window, gives each the name
    `download_destination` chooses, and asks before one the site's own page did not start.
    `PageDialogs.swift` shows a page's alerts, questions and file choosers as sheets, and
    asks that download question.
  - `WebViewHost.swift` places a page's web view in SwiftUI, with the system find bar above
    it. `AccountWindowView.swift` is the window, and `AccountWindowCommands.swift` its scene
    and its items in the **File**, **Edit**, **View** and **Go** menus.
  - `AccountWindows.swift` owns the feature as WebKit has it: the open sessions, windows
    asked for from the Dock, the store janitor and the downloads. It tells the model what a
    window did: that it opened or closed, the page it is on, and a link handed over. It
    makes a window's session as the model first lists the window in
    `AccountWindowsShown.open`, loading each page the model numbers for it once, and
    `AppModel.accountWindowsChanged` tells it of each change to the windows' part, which it
    follows: a session the model says closes stops, its view closing the window, and each
    store asked for is handed to the janitor.
  - `StoreJanitor.swift` makes, wipes and deletes stores: each ask the model makes once,
    tried again after each of its pauses while WebKit still holds the store, and told back
    as deleted or held. `WebEnvironment.swift` is the world a launch's windows run in: the
    sites and WebKit's stores, or a fixture's stand-ins, and where the model keeps their
    records.
  - `AccountPicker.swift` is the **Open Link** window, which shows what the model says of
    the link waiting and sends it the account chosen.
- `apps/macos/Sources/PitboardApp/System/WebsiteData.swift`: WebKit's persistent stores,
  behind the `WebsiteDataStores` protocol.
- `apps/macos/Sources/PitboardApp/App`: `AppDelegate.swift` owns the app's models and answers
  what only a delegate can: the Dock icon's menu, a click on the Dock icon, and quitting
  while a download runs, asked in the model's words of every download `DownloadCenter` has
  under way, one started a moment before Quit included. `AppPresence.swift` gives the app a
  Dock icon and its menus while any of its windows is open. `RefreshCommand.swift` is
  **View** > **Refresh**, whose title and action the window in front gives.
  `PitboardScenes.swift` adds the account windows' scene and the **Open Link** window, the
  one scene that takes a Pitboard link.
- `apps/macos/Sources/PitboardApp/Fixture/FixtureWeb.swift`: a fixture's stand-in pages for each
  site and sign-in host, on `pitboard-fixture://`, with stores in memory and links to
  anywhere else recorded and opened nowhere. The pages are `pitboard-ffi`'s `fixture_page`,
  made from the site table, so the Windows app can serve the same ones.
- `apps/macos/ShareExtension`: the `PitboardShare` target. It checks the shared page with
  `share_link`, which writes the Pitboard link too, then opens that link with the app it is
  inside, not whichever copy Launch Services would pick. The flow of an app extension stays
  Swift, and `PitboardLinkTarget` finds the app it is inside and the scheme its Info.plist
  names.

### What must stay true

- One store per account, derived from the account's `id`, which never changes. A window's
  store is a version 5 UUID of `<store name>:<account id>` in a fixed namespace, which
  `stores.rs` writes in lower case.
  It is also the window's value, so there is one window per account, and a rename keeps its
  sign-in. The namespace and the store names, `claude` and `codex`, never change: a change
  would leave every window without its data, and the next sweep would delete that data.
  Golden tests in `stores.rs` pin them, and that the account id is lowered a character at
  a time, as Swift's `lowercased()` lowered it for every store released, where
  `str::to_lowercase` lowers a sigma that ends a word otherwise. A store id an app hands
  back is compared without regard to case, since Foundation writes a UUID in upper case.
- A store is recorded before WebKit makes it, in `windows.json`, under the key of the
  Pitboard directory the app reads: the model lists a window in
  `AccountWindowsShown.open` only once the write recording its store has answered. A store
  WebKit made and nobody recorded would never be deleted. A write that fails opens the
  window all the same, and the next write writes this directory's records whole.
- The records are the app's, as its web stores are, so `windows.json` is in a folder of the
  app's own that the app names in `WindowsLaunch::directory`: on macOS its folder in
  Application Support, named by its bundle id, under the person's own Library whatever
  `HOME` says, and in a fixture the fixture's own folder. Each Pitboard directory's records
  are kept apart in it, and each write reads the file again and changes only this one's.
  Where the file is there it wins; where it is not, the model takes what the app's earlier
  store held, `webStores` and `windowPages` on macOS, and writes it at once. A file that is
  there and cannot be read, or whose text does not read as records, as a later version, a
  hand edit or damage may leave it, is never written over, and nothing is deleted by it:
  its windows' stores are held for the launch, so each window opens at its site's home
  with the sign-in note. Taken as no file, the first write would leave this directory's
  records alone in it, and a store another directory's account still uses would be
  deleted. A store id is compared without regard to case wherever it comes from, since
  Foundation wrote every one the macOS app recorded in upper case: compared as written,
  every one would be taken for an orphan and deleted with its sign-in. One that is not
  written as a UUID is dropped as the records are read, as `StoreRecord` dropped it, since
  no app can make or delete a store by it.
- A store is deleted only when this Pitboard directory recorded it, and a read that
  succeeded no longer derives it from any enrolled account. Every read that succeeds lists
  every enrolled account, from `state.json`, so that is a forgotten account, forgotten in
  the app or with `pitboard forget`. Nothing is deleted before the first read that
  succeeds, or after one that failed, or by what stands in for one, the last numbers
  measured. What the poll reads once the account index changes says who is enrolled too,
  whether or not reads that ask a service fail meanwhile and whether or not anything was
  shown before it: it is the index every read lists the accounts from. Each such read is
  acted on, and one that says what the last said changes nothing. The Swift model told its
  windows of fewer: not of a read saying, its numbers apart, what was shown, to the second,
  and not of the poll's read before anything was shown. Nothing is deleted before the
  records are read either: a read that lands first is acted on once they are in.
- The model claims each store it puts away before it asks anything, so a read meanwhile
  leaves it, then asks the app through `AccountWindowsShown.deleting`, one numbered ask at
  a time, and holds it recorded until the app answers `StoreDeleted`, when it goes from the
  records, or `StoreHeld`, when the next read asks again with a new number. An app deletes
  each ask once, however many snapshots list it, and a window of a store being deleted
  waits until the app has answered `StoreDeleted`.
- A store that another Pitboard directory recorded too is never deleted while that
  directory exists. The same account enrolled in both derives the same store, so deleting
  it would sign the other directory's window out. Forgetting the account in one removes
  only that directory's record.
- A store nobody recorded is left alone. WebKit keeps every store of one bundle under the
  person's own Library, whatever `HOME` says, so it can belong to a copy run with another
  home. A store something still holds is left recorded, and tried again at the next read.
- Nothing from outside opens a window by itself. A Pitboard link only shows the **Open
  Link** window, and only a person's choice there opens a window, on the link's own site.
  The app checks the link as strictly as the extension did, since anything on the Mac can
  open one. A site's sign-in link is refused, since it would sign the window in as whoever
  it belongs to.
- A window's page goes only to its site, the site's sign-in hosts and blank pages. Google's
  sign-in is refused, other web pages go to the default browser, an email address goes to
  the email app when clicked, and nothing else leaves. What the page embeds in its frames,
  such as an artifact, is the page's own choice, except a local file.
- A new window is decided only in `createWebViewWith`, where WebKit asks for the page to
  put in it. The navigation's own decision lets through a link that asks for one: refused
  there, WebKit would never ask for the window, and a sign-in link would open nothing.
- Only the site's own main page, or a sign-in page as the window's main page, opens a
  sign-in window. A frame, such as an artifact, gets none: it could fill the window with a
  page of its own, which nothing on the window's title would tell apart.
- A sign-in window is made from the configuration WebKit hands `createWebViewWith`, a copy
  of its opener's, so it shares the account's store and keeps `window.opener`. Its page
  loads only the site and its sign-in hosts, and goes blank only when the page itself asks,
  never its opener. It saves nothing and opens no window.
- Only a site's own pages are kept as a window's last page, and never one of its sign-in
  paths, which would sign the window in again with what it carried: one recorded before
  opens the window at the site's home. **Remove Website Data** takes it away, and so does a
  read that succeeded and no longer lists the account.
- Hands off the session. Pitboard makes a store, wipes one when asked and deletes one when
  its account is forgotten. It never reads, copies or changes what a site keeps there,
  adds no script or message handler to a page and sets no user agent of its own. No web
  session is made from a Claude Code or Codex login.
- The Pitboard link's format is a contract between the Share extension and the app, pinned
  by a golden test. A release claims `pitboard://` and a debug build `pitboard-debug://`,
  so a debug build never answers a link meant for an installed copy. A release build from
  `build-app.sh` claims `pitboard://` like the installed copy, until it is unregistered.
  `build-app.sh` fails a bundle whose app or extension names another scheme.
- The Share extension stays sandboxed, with no other entitlement, and links only
  `PitboardShareBindings` and `PitboardLinkTarget`. `build-app.sh` signs it with its
  entitlements before the app, and fails when its signature is not sandboxed. The release
  workflow checks the installed copy again.
- The extension checks a link by the same Rust as the app, `pitboard-sites`, but through
  `pitboard-share-ffi`, and never links the core: the core's bindings check every export's
  checksum when they load, so linking them brings all of it in. Two Rust static libraries
  never meet in one binary, since each carries its own copy of Rust's standard library.
  `build-app.sh` fails when the extension's binary has a `uniffi_pitboard_ffi_` symbol or
  none of `uniffi_pitboard_share_ffi_`, or the app's has a `uniffi_pitboard_share_ffi_`
  symbol or none of `uniffi_pitboard_ffi_`. No test target of the Swift package links
  `PitboardShareBindings`, since SwiftPM may link every test target into one bundle.
- `pitboard-sites` stays a leaf: no I/O, nothing of the core and no UniFFI, and `url` for
  IDNA alone. Each binding crate declares its own types over it, since the C# generator
  cannot use another crate's.

### On Windows

The Windows app's account windows run in WebView2, over the same decisions from the model.

- `apps/windows/Pitboard.Windows`: `WebEnvironment` is the world a launch's windows run in,
  decided once: the sites over `https`, with pages sent elsewhere opened by Windows, or a
  fixture's stand-ins on `pitboard-fixture`, served from `fixture_page` and sent nowhere.
  One `CoreWebView2Environment` serves every window, over `WebView2` in the app's data
  folder, the fixture's own folder in a fixture, so a fixture's scheme registration, which a
  browser process keeps for its life, never meets a real folder. A store is a WebView2
  profile named by the store id. `WebSession` is one open window: its page and the sign-in
  window one of its pages opened, each under its role. It turns `NavigationStarting`,
  `FrameNavigationStarting` and `NewWindowRequested` into what `decide_navigation` reads
  through `PageRequests`, and does what it decides: a sign-in window's page is made in the
  same environment and profile and set as `NewWindow` before it loads, which keeps
  `window.opener`; `LoadInPage` loads the link in the window's own page, and a page sent
  elsewhere goes to `Launcher`. A navigation it stops still sent its request on the runners,
  so it answers that document request itself, empty, through `WebResourceRequested`. It
  answers `WindowCloseRequested` with `page_may_close`, `PermissionRequested` with
  `page_may_use`, leaving anything else to WebView2's own prompt, a render process that
  ended with `after_content_process_ended`, and cancels every launch of another app's
  scheme. It hands `DownloadStarting` to `decide_response`, with `PageNavigations` saying
  whether the page's own navigation started it, and `Downloads` keeps every window's
  downloads. `StoreJanitor` answers each deletion the model asks for, and `HeldStores` says
  why a held store's entry opens nothing. `Launching` finds a launch's Pitboard link, and
  the one in a launch another copy hands over, and `Handovers` keeps what is handed over
  until the app has its model.
- `apps/windows/Pitboard.Views`: `AccountWindow` is the window, in XAML under Mica: a
  command bar of Back, Forward, Reload or Stop, and Downloads once the model lists a
  download of the window, with **Remove website data…** under More, the window's note in an
  InfoBar, and the page, or an `EmptyState` standing for it until the model lists the
  window. It asks the download question and the quit question as ContentDialogs. Find,
  zoom, script dialogs, file pickers, the download list and the browser keys are WebView2's
  own. `SignInWindow` is a sign-in window, at `sign_in_window_size` over the account's
  window and owned by it. The icon's menu and an account's shortcut menu offer each window,
  from `AccountWindowsShown.menus` and `AccountItem.windows`. `OpenLinkWindow` is the
  **Open link** window, in XAML under Mica, over `AccountWindowsShown.picker`, its accounts
  a ListView over the model's `PickerAccount`s and its buttons in a ContentDialog's order:
  the default first, Cancel last.
- `apps/windows/Pitboard.Views/AccountWindows.cs` opens a window per store and brings an open
  one forward, or back where it went out of sight, tells the model each opened and closed,
  closes those the model says close, and hands each store asked for to the janitor. It owns
  `Downloads`, and asks before Quit while one is under way. It brings the **Open link**
  window forward with each link that arrives, and withdraws it once none waits.

The share sheet reaches Pitboard through `PitboardShare.exe`, the one program of Pitboard's
with a package identity, which a share target needs. It is offered for a web link alone, as
the macOS extension is for a web URL, and starts the `Pitboard.exe` beside it through
`CreateProcess` with the Pitboard link `share_link` writes as its only argument, as the MSI's
`pitboard` command starts it. `Pitboard.exe`, with no identity, checks the link again as any
link. It does not open the link with `LaunchUriAsync`, which from a sparse package's identity
gave false and started no handler (CF8 in [Windows runners](docs/facts/windows-runners.md)).
A refusal, a link it cannot read and a start that fails each go to `ReportError`, whose
sentence Windows shows. Its sparse
package asks for nothing beyond `runFullTrust` and declares no write virtualization, since it
writes nothing. Only a signed build carries it: a sparse package registers only when signed by
a certificate the machine trusts, and its Publisher is the signer's subject.
`check-windows-app.ps1` fails a Release folder where another program carries an `<msix>`
element, where `PitboardShare.exe`'s names another publisher, package or application than
`PitboardShare.msix`, or where the package's Publisher is not the signer's.

A Pitboard link reaches `Pitboard.exe` as its first argument, and a launch with one reads no
flag after it: Microsoft's guide to URI schemes warns that a link's unencoded quotes or
spaces split it into more arguments. Only an installer registers `pitboard`, nothing
registers `pitboard-debug`, and neither build registers a scheme as it runs. A copy started
while another holds the key hands its launch over, and the running copy reads the link from
that launch's command line. A Debug build outside a fixture refuses a `pitboard-debug` link
before it takes the key or writes anything, since it would run on whatever homes its launch
has. **Open in browser** goes where a page sent elsewhere goes: to Windows, and in a fixture
nowhere. The `AccountPickerTests` hand their link over through a second copy.

One thing differs from WebKit in how a store goes: a deleted WebView2 profile's folder goes
only as its browser process exits, or at a later start, and its name is refused until then
([What the Windows app's glue rests on](docs/facts/windows-app.md), CF7 in
[Windows runners](docs/facts/windows-runners.md)). So `StoreJanitor` answers `StoreDeleted`
where the store's folder is not there, once a page of the launch has shown its `ProfilePath`
there, and otherwise
answers `StoreHeld`, which keeps the store recorded and has the next read ask again. A store
not yet marked it deletes through a page of its own, made in the app's hidden window. Once
no browser process runs, it sweeps: a page of the default profile starts one, kept until the
folder goes or for fifteen seconds. A store held at an earlier launch is swept rather than
marked first, as making its profile after a start had deleted the folder would make it anew,
and one a start has left since its mark is marked again. A window of an account enrolled
again meanwhile would fail to make its profile, so on Windows the model holds a store
answered `StoreHeld` until `StoreDeleted` comes: it asks for it again after every read,
enrolled again or not, opens no window of it, and lists it in `AccountWindowsShown.held`,
whose menu items say why with `WindowNoteKind::StoreHeld`. It keeps these in `windows.json`
too, as `held` beside this directory's stores, so after a restart the first read asks for
them again and their windows wait until the app answers `StoreDeleted`, then open with the
sign-in note. Another directory's record of a held store does not keep it either, since its
deletion is made already. A window whose name WebView2 still refuses keeps a page of the
default profile running and tries again, for thirty seconds at most, so one opened as
Pitboard starts waits for that start to let the name go. `ProfileTests` holds the contract
and the sweep on both images (runs 38846774768 and 38856204289),
`AWindowWhoseProfileIsStillBeingDeletedOpensOnceWebView2TakesItsName` that a window waits
for the name, and `TwoProfilesKeepTheirCookiesApart` that one profile's cookies are not
another's. On macOS a store WebKit kept holding is still whole, its sign-in included, so an
account enrolled again keeps it and its window opens on it.

**Remove website data** leaves the page for a blank one, clears the profile with
`ClearBrowsingDataAsync`, tells the model `WebsiteDataRemoved`, and opens the site's home
in a new web view, so Back cannot bring the signed-in page back.
`RemovingWebsiteDataEmptiesTheWindowsProfileAlone` holds that another profile keeps its
data.

Downloads go to the person's Downloads folder, `FOLDERID_Downloads`, and a fixture's to
`Downloads` in its own folder. Each profile's `DefaultDownloadFolderPath` is that folder
too, since Chromium writes a download into a temporary file there until it is named.
WebView2 has no response policy: it raises `DownloadStarting` for what it will not show,
naming no frame, so `decide_response` hears `can_show` false, the address and its
`Content-Disposition`, and `main_frame` true only where the page's own navigation to that
address is under way, or completed as one that became a download does, unsuccessful and
`ConnectionAborted`, and the page has not navigated since, and no frame's navigation there
has done either. A download nothing accounts for counts as a frame's, so the question shows
more often than on macOS, never less: the site's own `blob:` and `data:` downloads ask.
`Ignore` cancels it before it is named; a question holds the event's deferral and cancels on
Cancel, or as its window closes and withdraws it; a download saved is named by
`download_destination`, which on Windows numbers a copy `report (1).pdf` and splits a name
as WebView2 does, `archive (1).tar.gz`. `Downloads` tells the model each one's start, file
and end, and counts those under way for `downloads_quit_question`: Quit asks over a window
of an account whose download runs, brought back where the person closed it, or another
account window, then cancels them all. A paused download is under way: WebView2 leaves it
interrupted and able to resume, and one still under way as its browser process exits ends
cancelled. The download list is WebView2's own dialog, which Downloads in the command bar
opens, so no surface of Pitboard's reveals a file. On both images it lists each file with
Open file, and offers Open downloads folder and Search downloads, with no Show in folder of a
file's own (run 38872384945).
`ASiteDownloadIsListedInTheWindow` looks for Open downloads folder.
`DownloadTests` hold this against the stand-in site, and print for CF7 the events WebView2
raised around each kind of download.

A download ends with its browser process, which exits as its last page closes (CF7), where a
macOS download goes on after its window closes. So an account window the person closes, from
its caption, Alt+F4 or Ctrl+W, while one of its downloads is under way goes out of sight with
its page instead: muted, its sign-in window closed and its questions answered no. The model
hears `WindowClosed` then, and `WindowOpened` once opening the account's window, or Quit,
brings that window back. It closes as the last of its downloads ends, or before the model has
its store deleted, since a deletion closes the store's pages.
`AccountWindowTests.ClosedWhileItDownloadsItWaitsOutOfSightForTheDownload` holds this, and its
neighbours the question, the window brought back, Quit and the deletion.

A window refuses to open, saying why, on a PC whose WebView2 Runtime is older than
144.0.3719.77, the Runtime of the WebView2 SDK the Windows App SDK brings
([What the Windows app's glue rests on](docs/facts/windows-app.md)). CI's
`windows-app` job reads each image's Runtime as Microsoft's distribution guide detects it
(CF4), and `ProfileTests.ThisPCsRuntimeTakesTheWindows` asks the app's own check there.
