# Architecture

This file describes how Pitboard's code is organised and what must stay true in it. It
also holds the measured facts the design rests on, about macOS, WebKit, Claude Code,
OpenAI's Codex CLI, Claude Desktop and the sites the app's account windows open. It changes
when the layout changes or a tool build moves a fact, not with each commit.

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
    with a ceiling (`proc::run_within`), the login name, whether this process runs as root,
    and the `PATH` the person's login shell builds, which `unix/shell.rs` asks for. `fs`
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
    a Claude Code or Codex login (`journal.rs`). `identify.rs` says whose login Claude Code
    or Codex has stored: the record's owner where the login's refresh token has the
    fingerprint the record was made for, which asks nobody, and the tool's service otherwise
    (`whose`). A login whose access token has expired is never asked about with it, and one
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
    `host::proc::run_within`: the access token on descriptor 3 through a pipe `dup2`ed in
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
    the variables a renewal schedule installed from that context is given. See
    [ureq's proxies](#ureqs-proxies).
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
    from one an older Pitboard wrote.
  - `status.rs`, `doctor.rs`, `statusline.rs` and `schedule.rs` serve the commands of the
    same names. A row's numbers in `status.rs` are its service's answer folded into the
    reading every front end records in `usage.json`, or that reading alone where no answer
    came, and never a tool's own cache (see [Claude Code](#claude-code)). A Claude Desktop
    row has no numbers, since no usage of it is read, and says when a change last read its
    login in use (`read_at`). A read asks first (`ask`), and makes its rows (`report`) once
    whose each login is has been recorded, so an account whose only login a sign-in outside
    Pitboard replaced reads `login_replaced` from that read on. A row whose parked login its
    service refused (`login_refused`) is not switchable (`Row::switchable`), which
    `pitboard status`, the automatic switch and the app all go by. The usage of a login nobody
    could name is not asked, and the account the record names stands in for it. Offline rows
    (`gather_offline`) take the account in use from `in_use::known`, an account nobody
    enrolled included, which gets its row as it does online. `Pitboard::standing` says what
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
    system it is given), which `pitboard stow` and the app's sheet both say, and what
    becomes of running sessions after a switch (`after_a_switch`). It also holds
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
  the `--json` contract, pinned by the snapshots in `crates/pitboard/tests/snapshots`.
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
  until that account warms up, told apart as the app tells it (`switch::warm::told_apart`):
  by the account and the code, never by Claude Code's words. No warm-up error ends it. `warm.rs` is
  `pitboard warm`. `build/versioninfo.rs` is the Windows version resource SignPath asks of
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
    rules: the records a snapshot carries of what the core answered, such as `Status`,
    `Account` and `Abandoned`, and three free functions, which the macOS app calls, none as
    it draws: `app_command_line`, for the command line inside its bundle, as it starts and
    as it links that one; `can_run`, whether that one runs, as it links it; and
    `pitboard_directory`, for where `app.json` is and the key the account windows' records
    are kept under, as it starts. The Windows app calls two more as it starts:
    `launch_refusal`, what the gate every change passes refuses once it has named the app's
    version to the release gate, the release gate first, then an elevated process, a
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
    why it cannot be read, and a notification once for the words of a warm-up by itself
    that came to nothing, keeps the app's own preferences, and keeps the daily renewal
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
    why what is typed cannot be a name,
    `machine.rs` what the settings and the window's other panes show of this machine, daily
    renewal, doctor's checks, the activity log and the command line,
    `windows.rs` the account windows, what a window says until it can show its page, what
    the **Open Link** window says, and the question asked before quitting stops the
    downloads an app has under way, and `words.rs` the sentences both apps say and the
    command line does not, each a function of typed values. What the command line says too
    is `pitboard_core::words`', called from there, such as a renewal's note and doctor's
    summary. A clock time, and a change's date
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
    as `new` takes them. `tests.rs` is the macOS app's former `FixtureTests.swift` ported,
    and what each UI test reads in its world.
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
    app read one before the rule was Rust. [Foundation's URLs](#foundations-urls) has what
    was measured.
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
  `crates/pitboard/tests/common/os.rs`, one `match` on `host::OS` for each fact: where
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
  as `move_waiting` (`Pitboard::standing`), and `doctor` as its `desktop_move` check. A read
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
  Pitboard changes the system. One gate, `service::gate`, asks the host's `floor` and
  `elevation` and hands out a `service::Permit`, a value only it makes,
  and everything that changes anything takes one as an argument: `atomic::write`, every
  function in `host::fs` that makes, moves or removes a file or a directory,
  `RawStore::write` and `delete` (the keychain among the stores), `Scheduler::put` and
  `remove` and the service manager they ask, a tool's sign-in or warm-up run started
  (`host::proc::run_within`), and the token exchange, `Provider::renew` down to the request.
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
  (`switch::auto`), and `tests/watch.rs` has four processes decide from one reading and make
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
  which read only what they are given; `sites_for` and `site_names`, which are
  `pitboard-sites`' and read only what they are given too;
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
  landed while a rename was made put away what the account's last switch said
  ([A look and the app's own changes](#a-look-and-the-apps-own-changes)).
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
  starting ([One sign-in at a time](#one-sign-in-at-a-time)).
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
  finalizer thread ([The C# bindings](#the-c-bindings)); `shutdown` waits for the actor and
  the sign-ins under way, neither of which waits on the listener.
- No test makes the app model over the machine's own environment. Its Rust tests run the
  real core over a context of their own, with `MemoryHost` and `ScriptedApi` and a home in a
  scratch directory, and so does a fixture. The C# tests make one only as `ModelTests.cs`
  does: every home a fresh folder, and never started or sent an intent, so it reads nothing,
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
  ([The fixtures](#the-fixtures)).
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
  ([Documented, not measured](#documented-not-measured)).
- On macOS, only `/usr/bin/security` reads or writes Claude Code's keychain item and
  Pitboard's parked items. No keychain item is touched through the Security framework. The
  reason is under [macOS](#macos) in Measured facts.
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
  person, such as Codex's unelevated sandbox: Codex 0.162.1 builds its token with
  `DISABLE_MAX_PRIVILEGE | LUA_TOKEN | WRITE_RESTRICTED`
  (`codex-rs/windows-sandbox-rs/src/token.rs`), and a program started with such a token
  unseals a park and opens its file to read or delete it, though not to write it
  ([Windows](#windows) in Measured facts). A park copied to another PC opens
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
  was not run ([Claude Desktop](#claude-desktop) in Measured facts).
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
  (`words::not_given_up_because`), each adding its own way out, and the summary of a move
  given up is `words::given_up` in both, each with its own way on.
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

On 22 September 2026, the twelve facts the Claude Code register then held were checked
against six macOS builds. They could be read from 2.1.273 to 2.1.278, and the check went
red on 2.1.124. That build predates the credential write lock, two of the five
account-scoped keys and the keychain error classification. On 29 September the register was
read again from the macOS and Linux builds of 2.1.278, 2.1.281 and 2.1.284: every fact
holds on 2.1.281 and 2.1.284, and on 2.1.278 the three that describe the 2.1.281 change are
reported moved, as they should be.

On 6 October 2026 the Claude Code and Codex registers were read for Windows, from bytes on
a Mac, and no Windows build was run:

- Claude Code 2.1.289's `win32-x64` and `win32-arm64` builds. Both are built from commit
  736d26e, as that version's macOS and Linux builds are. Seven of its 17 facts hold there:
  the write lock, the unlocked logout, the account's keys, the config file, the OAuth client
  and both sign-in facts. The two keychain facts are not read on Windows, and nor is
  `no_keyring_off_macos`, because the Windows build's Credential Manager store calls the
  `Bun.secrets` it rules out. The other seven wait on the Windows work.
- Codex 0.160.0's `win32-x64` and `win32-arm64` builds, with its source at tag
  `rust-v0.160.0`. Nine of its 16 facts hold there: the login's shape, the two facts on
  whose it is, its renewal and how a spent one is refused, the usage request, the revoke on
  Codex's own sign-out, a running Codex keeping its login, and the sign-in's address. The
  other seven wait on the Windows work.
- The same run read the macOS and Linux builds of both versions, and every fact read there
  still holds. Claude Code 2.1.110 and Codex 0.99.0 still go red.

On 8 and 9 October 2026 Claude Code 2.1.294 was read again: its macOS build on the 8th and
the 9th, and its `linux-x64`, `win32-x64` and `win32-arm64` builds on the 9th, as bytes on a
Mac, and none was run. All four are built from commit 8f033c6. For each fact read from them,
the code it names was compared with the macOS build's token by token, minified names apart,
and is the same in all four: `usage_cache_stamp_is_the_configs`,
`status_reads_the_config_usage_the_token`, `config_identity_is_the_last_writers`,
`status_line_input_names_no_account` and `fallback_file_pins_session_login`, read in the
macOS build on the 8th, and `refresh_lock`, read in all four on the 9th. The fallback file's
fact was read on macOS alone that day: on Linux the file is Claude Code's only store, and
its Windows reading came on 16 October from both Windows builds of 2.1.301, where the file
sits behind Credential Manager wherever that store is on
(`credman_unreachable_writes_file`). The checker finds every fact each build was read for.

On 20 October 2026 the Windows readings were read again as bytes on a Mac, from Claude Code
2.1.303's `win32-x64` and `win32-arm64` builds, built from commit 07df2ae as its macOS and
Linux builds are, and from Codex 0.163.0's, with its source at `rust-v0.163.0`. The checker
passes on all four:

- For each Claude Code fact read on Windows from 2.1.289 to 2.1.302, the code around its
  literals was compared with the build it was read from, minified names apart. Three facts
  moved. From 2.1.300 a sign-in also prunes `trustedDeviceEnrolledAfterRefusalAtMs` and
  `cloudSessionsOauth` (`account_scoped_keys`, read again in all four builds of 2.1.303), so
  `ACCOUNT_SCOPED` moves both. From 2.1.301 a diskless launch reads no `HKCU` policy
  (`windows_managed_settings`). From 2.1.302 a session started with `--session-tunnel` takes
  the `env` of its line of JSON (`windows_backend_choice`). The refresh lock takes a
  synchronous `fs` from 2.1.301, at the same path and timing. The facts read from one build on
  every system, 2.1.294's and 2.1.296's, keep that reading.
- 2.1.302 and 2.1.303 embed Bun 1.4.4 (0049fd75e), which has no public release. 2.1.289 to
  2.1.296 embed 1.4.3 at eecfd55de, and 2.1.300 and 2.1.301 at 9ecafc196, neither a public
  commit. So each fact's Bun half stays a reading of `bun-v1.4.3`'s source, and the runner
  rows run Bun 1.4.3 from its release zip. The session machine ran 2.1.302, on Bun 1.4.4.
- Codex's sources for the facts read at 0.160.0 changed only in comments, telemetry and an
  enterprise sign-out step by 0.163.0, and those for the facts read at 0.162.1 not at all.
  0.163.0's builds still have no resource directory.

On 12 October 2026 the checker was run by hand on the `app.asar` of an installed Claude
Desktop 2.7032.0, without running Claude. The register's 19 facts that name literals hold
there, and its other six are about behaviour. On 13 October it was run again with two more
facts that name no literal, and the 19 still hold.

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

How to run the checker and add a fact is in
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
rests on are under [WebKit and SwiftUI](#webkit-and-swiftui) and
[claude.ai and chatgpt.com](#claudeai-and-chatgptcom).

### Where the code is

- `crates/pitboard-sites`: what a site is, and what a link from outside may be, in the
  [code map](#code-map). The app reaches it through `pitboard-ffi`'s `Site` and `SiteLink`
  records and `site_names`, and the model reads each Pitboard link with `pitboard-sites`'
  `read_pitboard_link` and says a refusal in its words. The app tests build a site with
  `sites_for`. The Share extension reaches it through `pitboard-share-ffi`.
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
gave false and started no handler (CF8 under [Windows](#windows)). A refusal, a link it cannot
read and a start that fails each go to `ReportError`, whose sentence Windows shows. Its sparse
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

One thing differs from WebKit in how a store goes, read in Microsoft's documentation of
`CoreWebView2Profile.Delete` for WebView2 1.0.4258.31 on 16 October 2026: deleting a profile
marks it for deletion, closes the web views using it and raises its `Deleted` event, and its
folder is deleted only as the browser process exits, or at a later start where something
still held its files. Making a profile of the same name before then fails with
`HRESULT_FROM_WIN32(ERROR_DELETE_PENDING)`. A profile's folder is
`EBWebView\WV2Profile_<name>` under the user data folder, as
`CoreWebView2Profile.ProfilePath` read on both runners in run 38819589926. On the runners
the folder went as its browser process exited, or outlived it and went as the next one
started, and that next one refused the name for its first seconds either way (CF7 under
[Windows](#windows)). So `StoreJanitor` answers `StoreDeleted` where the store's folder is
not there, once a page of the launch has shown its `ProfilePath` there, and otherwise
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
144.0.3719.77, the Runtime of the WebView2 SDK the Windows App SDK 2.5.1 brings, 1.0.3719.77
(`CORE_WEBVIEW_TARGET_PRODUCT_VERSION` in its `WebView2EnvironmentOptions.h`). CI's
`windows-app` job reads each image's Runtime as Microsoft's distribution guide detects it
(CF4), and `ProfileTests.ThisPCsRuntimeTakesTheWindows` asks the app's own check there.

## Measured facts

These decide the design. Each gives the build it was read from, or the date it was measured
or written down. The conformance run reads only the facts in a tool's register, and only
by their literals.

### macOS

Measured on macOS 26 and written down on 22 September 2026. Before changing code that
depends on the `security -i` limit or the Security framework's cost, measure them again on
a scratch item.

- `security -i` reads at most 4097 bytes of command from standard input, with no line
  continuation. Its interactive `-w` prompt takes 128 bytes.
- A keychain item written in process, through the Security framework, stays slow to read.
  Every later read of it by `security` takes about a second instead of 0.01 seconds. On a
  scratch item, reads went from 0.01 seconds to 20.55, then settled around 0.8.
- Claude Code reads its login on every cache miss, so writing its item through the framework
  would slow Claude Code for good. Pitboard writes a large login on the argument line
  instead, where `ps` can see it for the length of one call.
- Undoing the framework's change needs `security set-key-partition-list`, which asks for the
  keychain password.
- APFS keeps a directory's mtime in nanoseconds, but not exactly: setting one and reading it
  straight back gives a value 18 to 60 nanoseconds away. A lock that remembered the value it
  asked for would abandon every switch, so `lock.rs` keeps the value read back.
- Pitboard lists its parked items with `security dump-keychain` without `-d`. It never
  prompts, and emits attributes only, no secret of any item. It exits 0 in 0.06 seconds
  against a keychain of 362 items.
- Reads after a `dump-keychain` take the usual 0.016 seconds, so listing has none of the
  access-list cost of an in-process read. Each service name is on a line of the form
  `"svce"<blob>="<name>"`.
- A lock taken with `flock` outlives the file it was taken on while another thread is
  starting a process, here as on Linux: [One sign-in at a time](#one-sign-in-at-a-time).

### Windows

Measured on GitHub's runners only, by `pitboard-probe` and `.github/scripts/runner-facts.ps1`
in `windows-facts.yml` run 38693947329 on 15 October 2026, unless a line names another run.
Every run met the same images: windows-2025 (Windows Server 2025 Datacenter, 24H2, build
26100, UBR 33438) and windows-11-arm (build 26200, UBR 9457, 25H2). The script runs the
probe as the job's user and as three throwaway users it makes: a standard user, a standard
user whose name is not ASCII, and a member of Administrators. Its Bun is 1.4.3 from Bun's
release, the version Claude Code 2.1.300 and 2.1.301 embed, with proper-lockfile 4.1.2; 2.1.302
and 2.1.303 embed 1.4.4, which has no public release. A fact holds on both images unless it
names one. The tokens these list, and the session machine's, are the only ones the Windows face
has been seen to read.

Accounts and tokens:

- The job's user has a token whose `TokenElevationType` is the default one, a token that is
  not split, and that is elevated, at high integrity. Files it makes are owned by
  Administrators. So every program the job starts is elevated, and Pitboard reads it as
  `token::IN_EVERY_PROGRAM`.
- A Safer normal-user token computed from the job's token, with `SaferComputeTokenFromLevel`
  at `SAFER_LEVELID_NORMALUSER`, is elevated still, at high integrity, and so is a program
  started with it. It cannot stand in for a person on the runners.
- UAC is on: `EnableLUA` is 1 and `ConsentPromptBehaviorAdmin` 0. A member of
  Administrators started with `Start-Process -Credential -LoadUserProfile` gets the limited
  token of a split pair, not elevated, at medium integrity, and owns the files it makes.
- A local standard user made in the job, and started with `Start-Process -Credential
  -LoadUserProfile`, its output sent to files, runs an unsigned program, exiting 0, never
  0xC0000142. Its token is the default type, not elevated, at medium integrity, so Pitboard
  reads it as the person. Started without `-Environment`, the program gets the job user's
  variables; in run 37660991344 that left the user's local app data naming the job user's
  folder, which it cannot open, and `SHGetKnownFolderPath` failed. Given its own profile's
  `USERPROFILE`, `HOMEDRIVE`, `HOMEPATH`, `APPDATA`, `LOCALAPPDATA`, `TEMP`, `TMP` and
  `USERNAME`, every known folder reads as its own. It can read and run what the job built in
  the workspace. This is how CI runs every Windows test, through
  `.github/scripts/test-as-standard-user.ps1`.
- That user's programs run in the job user's session, 2, and start inside a job already.
- WebView2 tries that user's password blank once for each new user data folder:
  `msedgewebview2.exe` calls `LogonUser` with an empty password, logon type 2, as Chromium's
  `CheckBlankPasswordWithPrefs` does, and the Security log records a failed logon
  (0xC000006A). Read on 154.0.4258.62 on windows-2025 and 155.0.4283.45 on windows-11-arm, in
  run 38825074643 thirteen folders made thirteen. With a folder per WinUI test the user was
  locked out on both images after ten in run 38822700418, so those tests share one user data
  folder for the run, each under profiles of its own. The UI tests' launches share one too,
  named to a Debug build in `PITBOARD_FIXTURE_WEBVIEW2`: a fixture's own folder per launch
  locked the user out on both images in run 38874616742.
- CF4: the WebView2 Runtime is 154.0.4258.62 on windows-2025 and 155.0.4283.45 on
  windows-11-arm, read from `pv` under the machine's EdgeUpdate client key in run
  38825074643.
- CF7, WebView2's downloads, on both images in run 38825074643: a page whose response says
  `Content-Disposition: attachment` raised `NavigationStarting`, then `NavigationCompleted`
  unsuccessful with `ConnectionAborted`, then `DownloadStarting`, and a frame of another
  host did the same with `FrameNavigationStarting` and `FrameNavigationCompleted`; a `blob:`
  or `data:` link raised `DownloadStarting` alone. An `<a download>` of a `pitboard-fixture`
  address raised nothing at all, as in runs 38819589926 and 38822700418. For a second
  `archive.tar.gz` WebView2 offered and saved `archive (1).tar.gz`. A paused download is
  `Interrupted`, `UserPaused`, and can resume.
- CF7: a page's own script that leaves the site raises `NavigationStarting` with
  `IsUserInitiated` false, on both images in run 38825074643.
- CF7, a deleted profile, on both images in run 38842851059: `CoreWebView2Profile.Delete`
  had the name refused with `ERROR_DELETE_PENDING` at once, and the folder outlived its
  browser process, holding `Platform Notifications` and `WebStorage` files on windows-2025
  and `DIPS`, `DashTrackerDatabase` and `Network Action Predictor` ones on windows-11-arm. A
  new environment over the user data folder left it for 20 s; a page of another profile
  started a browser process, and the folder went 5.1 s later. Microsoft's reference has it
  go as the browser process exits, or at a later start where something held its files.
- CF7, a deleted profile, in run 38852781290: on windows-2025 the folder went as its browser
  process exited, and the next browser process refused the name at once all the same; on
  windows-11-arm it outlived its browser process, holding `DIPS`, `DashTrackerDatabase` and
  `Network Action Predictor` files, and went 5127 ms after the next one started.
- CF7, a deleted profile, on both images in runs 38854886274 and 38856204289: with a page of
  another profile keeping the next browser process, that process refused the name for 5.2 to
  5.9 s after it started, took it again as the folder went or just after, and made a profile
  with none of the old one's cookies.
- WinUI's `WebView2` reported a page it could not make, a profile name WebView2 refused,
  only through `CoreWebView2Initialized`, and left `CoreWebView2` null once
  `EnsureCoreWebView2Async` had returned, on both images in run 38854886274.
- CF7, on both images in runs 38842851059 and 38846774768: a navigation that
  `NavigationStarting` cancelled still sent its own request, from a page of the fixture's
  scheme and from one on http, with `Sec-Fetch-Mode: navigate`, `Sec-Fetch-Dest: document`
  and no `Sec-Purpose`. WebView2 raised `NavigationStarting` first, then
  `WebResourceRequested` for it as a `Document` request, where the window's answer kept it
  from the stand-in site.
- CF7, downloads, on both images in run 38842851059: an `<a download>` in a frame of the
  stand-in site raised `FrameNavigationStarting`, a successful `FrameNavigationCompleted`,
  then `DownloadStarting` for the file. A download whose page was the last to close raised
  no `StateChanged`, and its browser process exited; one whose page closed while another
  kept the browser process was still in progress five seconds later.
- CF8, sparse packages, as the fresh standard user on both images in run 38928930271: with a
  throwaway certificate trusted in `LocalMachine\TrustedPeople` by the job's user,
  `Add-AppxPackage` registered the probe's package from either manifest for that user, status
  `Ok`, signature `Developer`. `pitboard-probe-sparse.exe` in its external location ran with
  the package's identity and an AUMID. The plain probe and a stand-in `claude.exe` it started
  had none, and their writes and its own, a file under `%LOCALAPPDATA%\Pitboard` and a value
  under HKCU, reached the real folder and key with either manifest, none the package's
  folder. `LaunchUriAsync` of the probe's link scheme from the identity process gave false and
  started no handler. With the certificate in the user's own `TrustedPeople`,
  `Add-AppxPackage` refused the package with 0x800B0109 (run 38882775456).
- CF8, the hand-off, as the fresh standard user on both images in runs 39291962999 and
  39301311398: the probe's identity process started the plain probe through `CreateProcessW`
  with a `pitboard-probe-cf8://open?url=…` link as its only argument. With either manifest,
  the child exited 0, got the link whole and had no package identity.
- CF8, PitboardShare's package, as the fresh standard user on both images in run
  38928930271: `PitboardShare.exe --register` from a Release publish, its package signed with
  that certificate, registered `PitboardShare` with the publish's folder as its external
  location, status `Ok`, and exited 0. `Pitboard.exe --unregister-share` from that folder
  exited 0, 4888 ms after its start on windows-2025 and 1245 ms on windows-11-arm, and left
  the user no `PitboardShare` package.
- CF9, Restart Manager, as the fresh standard user on both images in run 38895887782:
  `RmShutdown` without `RmForceShutdown`, given only the GUI process of a stand-in app the
  tests started then, ended a stand-in that answers `WM_QUERYENDSESSION` with `TRUE` and
  quits on `WM_ENDSESSION` with `ENDSESSION_CLOSEAPP`, and gave `ERROR_SUCCESS` after 37 ms
  on windows-2025 and 46 ms on windows-11-arm. A stand-in that answers `FALSE` ran on with
  its window, and `RmShutdown` gave `ERROR_FAIL_SHUTDOWN` after 15 and 19 ms. Each
  stand-in's console child, a `cmd.exe` never registered, ran on.
- A process that gave `RegisterApplicationRestart` `--updated` with `RESTART_NO_CRASH`,
  `RESTART_NO_HANG` and `RESTART_NO_REBOOT` reads them back through
  `GetApplicationRestartSettings`, which gives `HRESULT_FROM_WIN32(ERROR_NOT_FOUND)` before
  it registers (run 38895887782).
- Explorer, the job user's, takes no notification-area icon of that user's: `NIM_ADD` fails
  for an icon named by its window and id and for one named by a GUID (CI run 38753437460).
  So no UI test clicks Pitboard's icon: the view tests hold its menu, `ScreenshotTests` opens
  it with the message a right click sends, and the session's step A2 clicks it.
- A window of that user's program comes to the front and takes a click on windows-2025. On
  windows-11-arm a full-screen `Microsoft account` window, a `Windows.UI.Core.CoreWindow` of
  `WWAHost` in the job user's session, holds the foreground, and Windows keeps it there for a
  program the tests start, as `SetForegroundWindow` documents for a process not started by
  the one in front (runs 38795330950 and 38800569975). A click at Pitboard's window reaches
  that window (run 38797997319). So the test of the first launch is inconclusive where the
  window in front before the launch keeps the foreground, and no test there gives Pitboard
  real input. The UI tests type through the Value pattern, press through the Invoke pattern,
  and there ask for a row's shortcut menu through `IUIAutomationElement3::ShowContextMenu`,
  which WinUI answers with the row's `ContextRequested` (run 38865911995). Ctrl+N and the find
  bar, tests of a key itself, are inconclusive there.
- FlaUI.UIA3 5.0.0, run as that user, finds that user's WinUI elements by AutomationId and
  selects a NavigationView item through its SelectionItem pattern (CF2,
  `LifecycleTests.AWinUIElementIsFoundAndInvokedByItsAutomationId`, run 38804714545).
- A launch redirected to the running copy carries the second copy's arguments: a second Debug
  copy started with a link handed it to the one running, which opened its picker for it
  (`AccountPickerTests`, CI run 39254279757).
- The self-contained Debug app, started by that user, has its window up 1194 ms after the
  start on windows-11-arm (CF5, run 38804714545). Both images hold the Windows App Runtime
  2.5.1.0 package for some user, so that the app needs none is not measured here.
- The WinUI projects build in Debug and publish in Release for win-arm64 on windows-11-arm,
  XAML compiler included, so each architecture is built where it runs (CF6, runs
  38782142541 and 38804714545).
- Developer Mode is on, and that user makes file and directory symbolic links as it is. It
  makes them with the symbolic-link right granted too, but Developer Mode was on for that run
  as well, so the right alone is not measured.
- `RtlGetVersion` gives 10.0.26100 and 10.0.26200, the builds the registry's `CurrentBuild`
  gives. The registry's `ProductName` says `Windows 10 Enterprise` on windows-11-arm, so
  Pitboard never reads which Windows this is from it.
- `VerQueryValueW` reads a program's version resource: `cmd.exe`'s product version is 10.0
  with a build no later than the running Windows, and `pitboard.exe`'s names Pitboard and its
  version (`cmd_exe_carries_the_build_of_the_windows_it_ships_in` and
  `pitboard_exe_carries_its_version_resource`, CI run 39254279757).

Homes and variables:

- `GetProfileType` gives flags 0, a local profile, for every user.
- `FOLDERID_Profile` and `FOLDERID_LocalAppData` ignore `USERPROFILE` and `HOME`, and local
  app data is the profile's `AppData\Local`. `std::env::home_dir` follows `USERPROFILE` and
  ignores `HOME`.
- `os.homedir()` follows `USERPROFILE` in Node (22.23.3 on windows-2025, 24.21.0 on
  windows-11-arm) and in Bun. Bun's `os.userInfo().homedir` follows it too. Node's keeps the
  account's profile (run 38704825885).
- Git for Windows' `bash.exe -c` hands a program it starts `HOME` set to the profile, as a
  Windows path.
- A program gets only `Path`, and a child std's `Command` starts gets one `Path`. A raw
  environment block holding both `PATH` and `Path` hands both to the child, and `var_os`
  returns the first in block order for either spelling (run 38418961905).
- std's `Command` given a bare name runs the program beside the running one before one on the
  parent's `PATH`, and a `PATH` set for the child wins over both. The working folder is never
  searched, and a `.cmd` on `PATH` is never found by a bare name (run 38418961905).
- A task Task Scheduler starts gets this `PATHEXT`:
  `.COM;.EXE;.BAT;.CMD;.VBS;.VBE;.JS;.JSE;.WSF;.WSH;.MSC`. The runner's own programs carry
  `.CPL` after it, and pass it on to theirs.
- A value `setx` writes into the job user's `HKCU\Environment` reaches a task started after
  it.
- A value written straight into the job user's `HKCU\Environment` never reaches the
  environment of a program already running, even once `WM_SETTINGCHANGE` for `Environment` is
  broadcast as `setx` broadcasts it. That reaches the program's top-level window that is never
  shown, and not its message-only window. The Explorer already running starts a program with
  the new value before any broadcast (RF-T8, runs 39310064516 and 39320680285).
- A profile folder whose name is not ASCII, such as one holding `Đ` and `ạ` (10 UTF-16
  units, 13 UTF-8 bytes, NFC), has an ASCII 8.3 alias holding `~`, and so has the `Temp`
  inside it.
- `std::fs::canonicalize` of a folder in that profile gives the `\\?\` form with every long
  name, as stored and in NFC, whether the folder was named by its long or its 8.3 spelling.
  Without the prefix, it is the long spelling as given (run 38704825885).

Who may read a login:

- A profile folder is owned by SYSTEM, with a protected DACL granting SYSTEM,
  Administrators, the user and an app container SID. Its `AppData\Local`, the `Temp` there and
  a folder the user makes in it are the user's, inheriting SYSTEM, Administrators and the
  user. Nobody else can reach their data.
- A file Bun writes with mode 0o600 and then `chmod`s to 0o600, as Claude Code 2.1.300
  writes `.credentials.json`, keeps only the DACL it inherits. Bun reports its mode as 0o666.
- A file made with a protected DACL granting the user, SYSTEM and Administrators is owned by
  the user who made it, standard or limited administrator, and the elevated job user can
  read, write and delete it.
- A fresh administrator's limited token, in which Administrators is for deny only, gets from
  a file the elevated job user made, owned by Administrators with a protected DACL, what an
  ACE for that user grants and nothing an Administrators ACE does: read, write and delete
  with one for full control, read alone with one for reading, and nothing without one,
  `CreateFileW` failing 5. Delete works with read alone, since the folder it is in grants that
  user full control (RF-T8, run 39320680285).

Replacing a file:

- NTFS and ReFS report `FILE_SUPPORTS_POSIX_UNLINK_RENAME`. There the POSIX rename works over
  a target held open sharing delete and fails 32 over one held without. `MoveFileExW` fails
  5 whenever the target is held. The ReFS volume is plain on windows-2025 and a Dev Drive
  on windows-11-arm.
- exFAT and FAT32 lack the flag, and the POSIX rename fails 87 there. `MoveFileExW` works
  over a target nobody holds and fails 5 over a held one (run 38418961905).
- An SMB loopback share mapped as a drive reports NTFS, a remote drive and no POSIX flag,
  and renames as exFAT does.
- 10000 POSIX renames beside a probe reader and a Bun reader that share delete, Bun holding
  each open 50 ms, needed no retry: about 13 s on NTFS on both images, and 2000 took 1.4 s
  on ReFS. No reader saw a torn or empty file.
- A reader thread in the renaming process itself found the file missing in 10788 of its
  69694 opens during those renames on windows-2025, and in none on windows-11-arm. Readers in
  other processes never did.
- Beside readers that reopen the file without a pause, one not sharing delete,
  `MoveFileExW` fails 5 most of the time. In 180 s it replaced the file 154 times on
  windows-2025, 30 rounds running out of 50 retries, and 2439 times on windows-11-arm, in up
  to 33 attempts. No reader saw a torn or empty file.

Names, ids and long paths (RF-T8, runs 39310064516 and 39320680285):

- `FILE_ID_INFO` gives a folder, a file, a hard link to the file and a junction to the folder
  one volume serial number, and ids by which the hard link is its file, the junction is its
  folder and the junction's own reparse point is another. A file keeps its id across a
  rename, and another folder or file has another. The ids take at most 51 bits on NTFS and 55
  on the SMB share, and 75 on ReFS, past 64. On the share a hard link is made and a junction
  is not.
- exFAT and FAT32 give no `FILE_ID_INFO`: `GetFileInformationByHandleEx` fails 87. Neither
  takes a hard link or a junction, so there `host::lies_in` compares by path alone.
- `GetFullPathNameW` of a name in a folder makes a device of `NUL` alone, `\\.\NUL`. `CON`,
  `con.txt`, `NUL.json`, `AUX.json`, `PRN.json`, `COM1`, `COM0.json`, `LPT1.json`, `COM¹.json`,
  `LPT².json`, `CONIN$`, `CONOUT$` and `CLOCK$.json` are names a file is made and kept by, as
  written. A trailing dot or space goes: `work.`, `work..`, `work ` and `work. .` make
  `work`, and `work.json.` makes `work.json`. The vault refuses each as a park's name, more
  than Windows asks on Pitboard's floor.
- Both images set `LongPathsEnabled` to 1. There a program whose manifest holds
  `pitboard.exe`'s `longPathAware` makes, opens and finds a file and a folder at 328
  characters through `CreateFileW`, `CreateDirectoryW` and `GetFileAttributesW`, with no
  `\\?\`, and `RtlAreLongPathsEnabled` says so. Without the manifest each call fails 3.
  std's own calls reach the folder either way, adding `\\?\` past `MAX_PATH`.

Flushing and times:

- A standard user can flush a folder opened for generic write with `FlushFileBuffers` on
  NTFS and ReFS, as the job user can on exFAT and FAT32. A handle opened for write
  attributes alone fails 5. On the SMB share the flush fails 1.
- NTFS, ReFS and the SMB share keep mtimes in 100 ns steps, exFAT and FAT32 in whole seconds
  (run 38418961905).

Locks:

- proper-lockfile 4.1.2 under Bun and the probe's lock see each other's lock with both of
  Claude Code's timings: held at every one of 25 checks, never stale. Its heartbeat was at
  most 7.5 s old with the write lock and 5 s old with the refresh lock.
- With another process holding `LockFileEx` on `state.lock`, opening it sharing everything
  works and `ReadFile` fails 33. Opening it sharing read alone fails 32.
- On NTFS and ReFS, `remove_dir_all` removes a home while another process holds its
  `state.lock`. On exFAT and FAT32 it fails 145 and leaves the home, and works once the
  holder exits (run 38418961905).

DPAPI:

- `CryptProtectData` with the vault's flags seals in about 0.2 ms and unseals in 0.15 ms on
  windows-2025, 0.27 and 0.2 ms on windows-11-arm. A process's first seal takes 7 ms and
  1.3 ms.
- After the user changes its own password through ADSI, a program started with the new
  password unseals what was sealed before.
- After an administrator resets the password, nothing sealed before unseals: 0x8009000B.
- A task with the job user's interactive token, in its session, unseals what the job user
  sealed.
- A token made from the standard user's as Codex 0.162.1 makes its unelevated sandbox's,
  with `DISABLE_MAX_PRIVILEGE | LUA_TOKEN | WRITE_RESTRICTED`, a capability SID, the logon
  SID and Everyone as restricting SIDs, Codex's default DACL and a desktop of its own
  granted to the logon SID, is not elevated, at medium integrity. A program started with it
  unseals what the user sealed, its first unseal taking 1.5 ms on windows-11-arm and 1 ms on
  windows-2025. It opens a file whose protected DACL grants only the user, SYSTEM and
  Administrators for read and for delete, and fails 5 opening it for write (run
  38821312770). Without the capability SID, the default DACL and the desktop, its programs
  started but neither exited within 30 s nor wrote a report (run 38797729796).

Tasks:

- A plain console program started by a task, or with no console to inherit, gets a visible
  console window. Built with either spelling of the detached `consoleAllocationPolicy`, it
  gets none there, and still attaches to a parent's console (run 38418961905).
- A standard user can make `\Pitboard\` and register a task with an interactive token there
  or in the root. With no signed-in session of its own, the task never starts: its last
  result is 0x00041303.
- `GetFolder` of a missing folder and `GetTask` of a missing task fail with 0x80070002, and
  `CreateFolder` of a folder there with 0x800700B7. A second standard user opens and
  registers in a `\Pitboard\` the first made; the first's `DeleteFolder` fails with
  0x80070091 while the second's task is in it, and succeeds once it is gone (RF-E1 and
  RF-E1c, run 38922866517).
- The job user's task in either folder runs, with last result 0.

Processes and jobs:

- As a standard user, WTS at level 1 for any session lists every process Toolhelp does,
  session 0's among them. It gives the SID of the user's own `codex.exe` and `Codex.EXE` and
  of the elevated job user's `codex.exe` in the same session, and each of them opens for
  limited queries, with its token's user and its image path, on both images (run
  38741362483).
- `GetProcessInformation(ProcessMachineTypeInfo)` reports an x64 program emulated on
  windows-11-arm as x64. `IsWow64Process2` gives the native machine.
- A process a standard user starts is in a job already, and a job of its own still takes its
  children. Every leaf of 1000 probe, probe, probe chains and 1000 cmd, node, probe
  chains stayed in it, assigned right after its start or started suspended and resumed.
- In a job without `JOB_OBJECT_LIMIT_BREAKAWAY_OK`, `CREATE_BREAKAWAY_FROM_JOB` failed with 5
  all 200 times. In a job with it, all 200 leaves started with the flag left that job and
  stayed in the outer job the process was already in, however the chain was assigned (run
  38704825885).
- `GetProcessTimes` of a child opened by its pid for limited queries gives a creation time
  between the readings of `GetSystemTimePreciseAsFileTime` either side of its start, the same
  as the child's own reading as text, and the same once it has exited while a handle is open,
  its exit time later. Each of 50 children had its own (RF-T8, runs 39310064516 and
  39320680285).
- Once the probe holds no handle to an exited child, opening its pid at once gives the same
  exited process in 30 and 11 of 50 tries on windows-2025 and windows-11-arm, and fails 6 in
  the rest. 100 ms later it fails 6 in 99 of 100 (RF-T8, run 39320680285). A pid never given
  out fails 87, as the core's Windows tests read, and `proc::open` takes both for gone.
- `GetModuleFileNameW` and `current_exe` give the path a program was started by, through a
  file symbolic link, a junction or a hard link. Canonicalizing resolves the link and the
  junction, not the hard link.
- Edge, opened on a local page by `ShellExecuteW` or by the http association's command from
  a process in a job, runs all its 8 to 10 processes inside that job. `TerminateJobObject`
  ends them all. Ending only the probe's processes, or letting the opener exit, leaves Edge
  running.

Handing a descriptor:

- An anonymous pipe's read end, handed to Bun as C runtime descriptor 3 through
  `STARTUPINFOW.lpReserved2`, reaches it as descriptor 3. Bun's `fstat` calls it a FIFO,
  and a direct `readSync` reads all 46 bytes handed (run 38797729796).
- A loopback socket handed the same way is a FIFO to Bun's `fstat` too, never a socket,
  and opening `/proc/self/fd/3` fails ENOENT. So a reader that reads the descriptor
  directly only when `fstat` calls it a socket, and otherwise opens `/proc/self/fd/3` and
  falls back to the direct read only on EACCES or EPERM, as Claude Code 2.1.300's does,
  gets nothing from a pipe or a socket (run 38821312770).

Claude Code's remote token path, `/home/claude/.claude/remote/.oauth_token`, which it reads
where no descriptor is named (`remote_token_files`), names no drive. Bun 1.4.3 resolves it
against the drive of its working folder (RF-T7, runs 39310064516 and 39320680285):

- Run from a folder on a VHD holding a dummy at `\home\claude\.claude\remote\.oauth_token`,
  it read the dummy whole, a regular file of 39 bytes.
- Run from a folder on `C:`, which has no `C:\home\claude`, it got ENOENT.

So a session reads the file at the root of the drive its working folder is on, and doctor's
`auth_source` check and a warm-up's halt over `.api_key` look at the root of each fixed
drive. Each reads at most 64 KiB of a regular file, as Claude Code does, since any account on
the PC may put one there.

Credential Manager:

- A generic item's blob holds 2560 bytes, `CRED_MAX_CREDENTIAL_BLOB_SIZE`, and `CredWriteW`
  refuses 2561, as the fresh standard user (run 38858363708,
  `host::windows::credman::tests::an_item_holds_2560_bytes_and_no_more`).
- `Bun.secrets.set`, called as Claude Code 2.1.300 calls it, writes a generic item named
  `<service>/<name>`, with the name as its user name, enterprise persistence, no comment and
  no attributes, and the value's UTF-8 bytes as its blob: 30 UTF-16 units holding `Đ` and `ạ`
  made a 33-byte blob (run 38704825885).
- As the job's user, in an interactive logon, `credman-names` lists the prefixes of every
  live login family and `pitboard-*` without an error, and finds no item. As the fresh
  standard user, after each Windows test run, it does the same and finds nothing (run
  37762408901).

A network logon, by key through sshd as the standard user: windows-2025's own OpenSSH 9.5p2,
and on windows-11-arm the Win32-OpenSSH 10.0p2 RF-M installs (runs 38821312770, 38922866517,
38928930271, 39291962999, 39310064516 and 39320680285):

- It is logon type 3, in session 0.
- `CredWriteW`, `CredReadW`, `CredDeleteW` and `CredEnumerateW` fail 1312.
- WTS and Toolhelp name the same user's `codex.exe` in session 2.
- On windows-2025 `CryptProtectData` fails 5. On windows-11-arm it seals and unseals, and
  refuses the wrong entropy. Whether sshd's build or the image makes that difference is not
  measured: RF-M installs OpenSSH only where the image ships none, as AGENTS.md allows.

Scoop, winget and the user `PATH`, as the job's user, with the gated build in the zips
`package-windows.ps1` makes for 0.14.0 and 0.14.1, served on loopback (run 38725515950):

- ScoopInstaller/Install's `install.ps1` at 1e2f3340, given `-ScoopDir` and `-RunAsAdmin`,
  installs Scoop v0.6.0 there and puts its shims first on the user `PATH`.
- From a bucket holding `packaging/scoop/pitboard.json` as the release fills it, Scoop takes
  the `64bit` zip on windows-2025 and the `arm64` one on windows-11-arm, checks its hash,
  points `apps\pitboard\current` at the version's folder by a junction, and makes a
  `pitboard` shim that runs the zip's own `pitboard.exe`.
- `scoop update pitboard` while that `pitboard.exe` runs says it is running, skips it and
  exits 0, leaving `current` on the old version. Once it stops, the update goes through.
- A program started after the installs finds `pitboard` on the user `PATH` in this order:
  Scoop's shims, `~\.cargo\bin`, which the images put there, WinGet's Links, then a folder
  appended last, standing in for the MSI's `bin`. windows-11-arm, without winget, keeps the
  same order without Links.

On windows-2025, where winget is:

- winget, under Developer Mode, installs the zip from a local manifest naming its
  `pitboard.exe` a nested portable with the alias `pitboard`. It links `pitboard.exe` in
  `%LOCALAPPDATA%\Microsoft\WinGet\Links` to the exe in the package's folder by a symbolic
  link, and appends Links to the user `PATH`.
- `winget upgrade` while that `pitboard.exe` runs fails 0x8A150052, since it cannot remove the
  running version's folder, and leaves the link and that folder as they were. Once it stops,
  the upgrade goes through and the link leads to the new version.
- `winget validate` exits 0x8A150028, valid with warnings, for a manifest without schema
  headers.
- `winget uninstall --id Pitboard.CLI --exact` finds no package installed from a local
  manifest: 0x8A150014. In run 38741362483, `winget uninstall --manifest` given that manifest
  found it as `ARP\User\X64\Pitboard.CLI__DefaultSource` and left no link, package folder
  or Uninstall key.

On windows-11-arm, where real-time protection is on, with all 19 attack surface reduction
rules of Microsoft's reference in audit, Defender logged no event 1121 or 1122 while the zip's
`pitboard.exe`, marked as downloaded from the internet, ran with `--version`, with no argument
and with `completions bash`. The prevalence and ransomware rules need cloud-delivered
protection, which neither image has, so they judged nothing.

`test-windows-release.ps1` read bottom 0.14.9's x64 `btm.exe` with
`Get-AuthenticodeSignature` on both images in CI run 38722196167, and its ARM64 `btm.exe` on
both in CI run 38741365089. Each is `Valid`, with a `TimeStamperCertificate` and this
`SignerCertificate.Subject`, the form `WINDOWS_SIGNER_SUBJECT` takes:
`CN=SignPath Foundation, O=SignPath Foundation, L=Lewes, S=Delaware, C=US`.

WiX v7's ICE validation of `Pitboard.msi` on both images in CI run 38882810828, read against
the `runtimes-framework` folders of the Windows App SDK 2.5.1 packages as bytes:

- ICE03 refuses as invalid the languages of WinUI's MUI files for ug-CN (1152), mi-NZ (1153)
  and gd-gb (1169), the only files there whose primary language is past 0x7F.
- ICE03 finds the languages Windows Installer reads from `Microsoft.ui.xaml.dll` and
  `Microsoft.UI.Xaml.Phone.dll` longer than the File table's 20 characters, though each
  file's own version resource names 1033 alone.
- ICE57 takes a shortcut in `ProgramMenuFolder` for the user's in a package whose `ALLUSERS`
  is 2, and refuses it an HKMU key path.

The build suppresses ICE03, which checks every column of every table. `validate-windows-msi.ps1`
runs it on the built MSI and lets through only those eight rows, found by their names: an
invalid language on either library's `.mui` file in the ug-CN, mi-NZ or gd-gb folder, and a
language list too long on either library itself.

With ICE03, ICE33 and ICE61 suppressed, WiX v7 builds it on both images in run 38895887782
with no warning, ICE57 passing. `msiexec /a`, run by the job's user, lays out each image's
own MSI in `PFiles64\Pitboard`: the Release publish's 284 programs and libraries on
windows-2025 and 283 on windows-11-arm, no `.pdb`, and `bin\pitboard.exe`. The fresh
standard user opens the MSI read only through `msi.dll`, as `Pitboard.Installer.Tests` does.

The Component, Directory, File and Registry tables of the published MSIs of 1.0.0-beta.2 and
1.0.0-beta.3, read as bytes on 20 October 2026: from one beta to the next each machine's MSI
kept the GUID of every key path, 534 on x64 and 530 on ARM64, and beta.3's two MSIs give the 528
key paths they share the same GUIDs. A standard folder's Directory row goes by its property,
`ProgramFiles64Folder` under `TARGETDIR`, and each folder under `Pitboard` by an id WiX makes
up, such as `dGNBjMVUkheIrP_lIJTI5NBZzo_c` for `af-ZA`. So `check-msi-components.ps1` names a
key path by the standard folder and the long names below it.

The Authenticode certificate tables of the app folders 1.0.0-beta.3's MSIs carry, the artifacts
`app-windows-x64` and `app-windows-arm64` of release run 39205401692, read as bytes with
LibreSSL's `pkcs7 -print_certs` on 20 October 2026. Of the 512 programs on x64 and 511 on ARM64,
284 and 283 DLLs, three executables, 172 `.mui` and 53 `.winmd` files, seven are not signed:
Pitboard's six and `WinSparkle.dll`. The Windows Community Toolkit's five are signed by the .NET
Foundation, as `Community Toolkit (.NET Foundation)` or `Windows Community Toolkit (.NET
Foundation)`, and every other one by a certificate whose organisation is Microsoft Corporation,
issued by Microsoft's own code signing CAs. So `verify-windows-signatures.ps1` holds each to its
maker by its name, Microsoft for a name it does not list.

The images:

- Defender's real-time protection is off on windows-2025 and on, with behaviour monitoring
  and tamper protection, on windows-11-arm. Cloud-delivered protection is off on both:
  `MAPSReporting` is 0 and `SubmitSamplesConsent` 2, never send (run 38741362483). winget,
  v1.11.510, is on windows-2025 alone, and Scoop on neither.
- diskpart formats ReFS on windows-2025. On windows-11-arm, `Format-Volume -DevDrive` makes a
  trusted Dev Drive with Defender's filter attached.
- The Server service runs, and `New-SmbShare` serves a folder to `\\localhost`.
- windows-2025 runs OpenSSH Server, whose `AllowGroups` turned the standard user away until
  it joined OpenSSH Users (run 38691271016). windows-11-arm ships none, and
  `Add-WindowsCapability` did not install it within 10 minutes (run 38685119129). There RF-M
  installs Win32-OpenSSH 10.0p2 from Microsoft's release, its SHA-256 pinned, starts it and
  removes it after (runs 38821312770, 38922866517 and 38928930271).
- Neither image holds Codex's `%ProgramData%\OpenAI\Codex`, Claude Code's
  `C:\Program Files\ClaudeCode` or its policy keys.

### The session machine

Measured by hand on 18 October 2026 on the session machine: Windows 11 Pro 26H2, build 26300,
UBR 9711, on ARM64, an OSMates VM, with Defender's real-time protection on and no exclusion.
The bundle was `windows-facts.yml` session run 39114824545 at 91bd8452, the commit that holds
`Release 1.0.0-beta.1`, its sums checked on the Mac and in the guest. Claude Code 2.1.302 and
Codex 0.163.0 came from npm 24.20.0, installed with `winget`; `pitboard-conformance` passed on
both. The account was an administrator that signs in with a password, its programs run
limited at medium integrity. The probe's reports are kept out of the repository, which names
no account.

Steps 9 and 10, Codex's sign-ins and the ChatGPT app, were not done, nor the optional 15 and
16. What they measure waits on the next session: `codex_login_is_driveable`,
`codex_runs_inside_the_chatgpt_app`, `windows_credman_cache`, W18's package family and AUMID,
and the rows of [Documented, not measured](#documented-not-measured) that name them.

Logon and homes:

- The interactive logon is type 2. Its token is limited, not elevated, at medium integrity,
  owns what it makes, and reads as `not_elevated`, as Microsoft documents a split token.
- `FOLDERID_Profile` and `FOLDERID_LocalAppData` are the profile and its `AppData\Local`; the
  profile is local, its name ASCII. Only `Path` is spelled, and `CODEX_HOME`,
  `CLAUDE_CONFIG_DIR`, `PITBOARD_HOME` and `HOME` are unset. The WebView2 Runtime is
  155.0.4283.62.
- Windows PowerShell 5.1's execution policy is `Restricted`, so npm's `claude.ps1` and
  `npm.ps1` refuse to run there, and `claude.cmd` runs.

Smart App Control and SmartScreen:

- With Smart App Control on, the unsigned `pitboard.exe` marked as downloaded ran, and the
  unsigned MSI opened from File Explorer installed with no prompt; no CodeIntegrity event was
  logged, before or after a restart.
- With it off, the MSI so marked met SmartScreen's "Windows protected your PC", Publisher
  "Unknown publisher", passed with **More info** and **Run anyway**.

The MSI, `Pitboard.exe` and the notification area:

- It installs per user with no UAC prompt, context 2: `%LOCALAPPDATA%\Programs\Pitboard` with
  `Pitboard.exe` at its top and `pitboard.exe` alone in `bin`, the Start shortcut with the
  AppUserModelID `Pitboard.Pitboard`, `HKCU\Software\Classes\pitboard`, and `bin` last on the
  user `PATH`, which a terminal opened afterwards finds. Settings > Apps lists it as Pitboard,
  publisher Pitboard, with no Modify; its Uninstall key reads under HKLM's view. Pitboard does
  not start after the install.
- A first launch from Start opens the window. The icon starts in the overflow, named and
  tipped "Pitboard", or the account in use and its usage once there is one, and keeps its
  place on the taskbar across a restart. Closing the window hides it. Win+B, Enter and
  Shift+F10 reach the icon and its menu, which opens at the icon in the taskbar's mode.
- Registering for notifications worked with no Singleton package: it wrote
  `HKCU\Software\Classes\AppUserModelId\Pitboard.Pitboard` with `DisplayName`, `IconUri` and
  `CustomActivator`. A login replaced outside Pitboard was said in the window, and no
  notification came for it, though the model asks the app to post one.
- `msiexec /i` of a second MSI, one higher in the fourth field, upgraded 1.0.0.1 to 1.0.0.2
  with no UAC prompt in 7 s. The default schedule removed the old product first, so
  `bin\pitboard.exe` was missing for about 2 s: 9 of 416 runs of the status line every 250 ms
  found no `pitboard`. The MSI now removes it after the new one's files are in place.
- `msiexec /fa` with Pitboard open had Restart Manager close it, not forced, and start it
  again within 4 s; the icon kept its place. Of Pitboard's windows, only the hidden one, never
  shown, ends Pitboard on `WM_ENDSESSION` with `ENDSESSION_CLOSEAPP` or on `WM_CLOSE`, so one
  of them reached it. Which one was not recorded.
- With start at sign-in on, a sign-in started Pitboard with no window, its icon there 7 s
  after. Settings > Apps > Startup listed it; turning it off there wrote 01 as the first byte
  of `StartupApproved\Run`'s `Pitboard` and kept the `Run` value.
- `pitboard uninstall` with the app open deleted `%LOCALAPPDATA%\Pitboard` and the renewal
  task, and the app made the folder again as it quit; `uninstall` now refuses while the app is
  open. Removing Pitboard in Settings then took the install folder, the shortcut, the link
  scheme, the `PATH` entry, the `Run` value and `%LOCALAPPDATA%\Pitboard App`. It left the
  `AppUserModelId` key, which `--unregister` now deletes, and `StartupApproved\Run`'s value,
  Windows' own. Nothing in the MSI removed the renewal task, which `--unregister` now takes
  away where it runs this install's `bin\pitboard.exe`.
- The x64 MSI installed under emulation: `Pitboard.exe` and `bin\pitboard.exe` are machine
  0x8664, and both ran.

Claude Code 2.1.302:

- `pitboard enroll --sign-in` drove Claude Code's manual flow, the code pasted back; the app's
  **Add account** drove its localhost flow, with nothing to paste. Neither left a scratch slot
  in Credential Manager.
- A running session took a switch's `.credentials.json` at its next prompt
  (`windows_file_adoption`). Process Monitor saw it open its files sharing all three access
  modes, and no sharing violation on any `.claude` path (`windows_open_sharing`).
- A `settings.json` that starts with a byte order mark is read
  (`settings_byte_order_mark`), and its first move of a login into Credential Manager lost it
  (`credman_move_lost_a_piece`). A sign-in into an empty Credential Manager wrote one whole
  item of 524 bytes.
- It ran the status line in `powershell.exe` with neither PowerShell 7 nor Git installed, in
  `pwsh.exe` with PowerShell 7.6.7, and in Git 2.55.0's `bash.exe`
  (`windows_statusline_powershell`), an account named `bé` shown whole. Twenty runs of
  `pitboard statusline` took 16 ms each from PowerShell, and 132 ms each through Git Bash.

Daily renewal:

- `pitboard schedule install` registered `\Pitboard\Renew <SID suffix>` to run as the person,
  interactive only, least privilege, hidden. Run at once, it started `pitboard.exe renew
  --scheduled` under svchost with no console window, last result 0x0, and doctor said when it
  last ran.

### Documented, not measured

Windows facts Pitboard stands on that Microsoft documents and nothing has measured yet, read
on 15 and 16 October 2026. W27 took out the rows [The session machine](#the-session-machine)
measured on 18 October 2026, and those CI and the runner facts measured by 20 October 2026.

| Fact | Source | Why not measured |
|---|---|---|
| `GetProfileType` reports `PT_ROAMING` for a roaming profile | [GetProfileType](https://learn.microsoft.com/en-us/windows/win32/api/userenv/nf-userenv-getprofiletype) | The runners have local profiles only. |
| `CfGetSyncRootInfoByPath` finds a sync root such as OneDrive's | [CfGetSyncRootInfoByPath](https://learn.microsoft.com/en-us/windows/win32/api/cfapi/nf-cfapi-cfgetsyncrootinfobypath) | No sync provider runs on the runners. |
| WTS lists another session's processes to Administrators only | [WTSEnumerateProcessesExW](https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/nf-wtsapi32-wtsenumerateprocessesexw) | No row looked at another user's process in another session. A standard user's listing held session 0's processes. |
| `longPathAware` in a manifest lifts nothing where `LongPathsEnabled` is 0 or absent, as on a PC no policy or installer changed | [Maximum path length limitation](https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation) | Both images set it to 1, where the manifest lifts the limit (RF-T8, runs 39310064516 and 39320680285), and AGENTS.md does not let runner-facts.ps1 change it. |
| `StartWhenAvailable` runs, about 10 minutes later, a start missed while the PC slept or was off | [StartWhenAvailable](https://learn.microsoft.com/en-us/windows/win32/taskschd/tasksettings-startwhenavailable) | A runner never sleeps through a trigger. Task Scheduler took no missed start of the renewal's daily trigger begun before the task was registered, or passed while it was disabled: none of four tasks, two with the setting, ran within 21 to 29 minutes, each `0x00041303` with no missed run counted (RF-T8, runs 39310064516 and 39320680285). |
| A `LogonTrigger` whose `UserId` is the person's SID starts the task at their sign-in, for the watch service | [LogonTrigger.UserId](https://learn.microsoft.com/en-us/windows/win32/taskschd/logontrigger-userid) | No run of `windows-facts.yml` has registered the watch service's task yet. |
| `ExecutionTimeLimit` of `PT0S` lets the watch service's task run with no limit | [TaskSettings.ExecutionTimeLimit](https://learn.microsoft.com/en-us/windows/win32/taskschd/tasksettings-executiontimelimit) | As above. |
| `IRegisteredTask::Run` starts the task at once, and `Stop` ends its run under way | [IRegisteredTask](https://learn.microsoft.com/en-us/windows/win32/api/taskschd/nn-taskschd-iregisteredtask) | As above. |
| Restart Manager asks a console program, such as the `bin\pitboard.exe` the watch service runs, to end by `CTRL_C_EVENT`, and an MSI replaces a file it cannot free at the next restart. `/passive` then restarts Windows unasked; `/norestart`, which the update feed passes, never does | [Guidelines for Applications](https://learn.microsoft.com/en-us/windows/win32/rstmgr/guidelines-for-applications), [Standard Installer command-line options](https://learn.microsoft.com/en-us/windows/win32/msi/standard-installer-command-line-options) (updated 29 April 2025, read on 23 October 2026) | No runner updates Pitboard while the watch service runs. The next session's update should run with the service installed. |
| WSL 2's processes run in a utility VM | [Comparing WSL versions](https://learn.microsoft.com/en-us/windows/wsl/compare-versions) | No runner has WSL. |
| Each subkey of `HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss` is a distribution WSL holds for the account, named by its `DistributionName` | [How to manage WSL disk space](https://learn.microsoft.com/en-us/windows/wsl/disk-space) | No runner has WSL. |
| `EnableLUA` 0 turns Admin Approval Mode off, so every program an administrator starts runs elevated; 1 is the default | [User Account Control settings and configuration](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/user-account-control/settings-and-configuration) | Both images have UAC on (RF-A1). |
| `QueryUnbiasedInterruptTimePrecise` leaves out sleep and hibernation, which `QueryPerformanceCounter` counts | [QueryUnbiasedInterruptTimePrecise](https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-queryunbiasedinterrupttimeprecise), [Acquiring high-resolution time stamps](https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps) | A runner never sleeps. |
| A program ended by an unhandled exception exits with the exception's code, such as `0xC0000005` | [GetExitCodeProcess](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getexitcodeprocess) | No row crashes a program. |
| `GetPackageFamilyName` gives a packaged process's family, and `APPMODEL_ERROR_NO_PACKAGE` for an unpackaged one | [GetPackageFamilyName](https://learn.microsoft.com/en-us/windows/win32/api/appmodel/nf-appmodel-getpackagefamilyname) | No row starts a packaged process; session step 10 reads ChatGPT's. |
| A `CryptProtectData` blob opens only for the same user's credentials on the same computer | [CryptProtectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata) | The runners have one account per logon and one machine each. |
| A `CryptProtectData` blob opens on another PC only with a roaming profile, whose key roams | [CryptProtectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata) | The runners have local profiles only. |
| winget's validation installs a package as a standard user, runs its program with no argument and wants it to succeed | [Validation](https://github.com/microsoft/winget-pkgs/blob/master/doc/Validation.md), [Validation failure guide](https://github.com/microsoft/winget-pkgs/blob/master/doc/ValidationFailureGuide.md) | No runner submits a winget manifest. |
| winget's validation scans an installer with several antivirus engines, and Defender scans what it installed | [Validation](https://github.com/microsoft/winget-pkgs/blob/master/doc/Validation.md) | No runner submits a winget manifest. |
| Attack surface reduction's prevalence rule (`01443614`) and ransomware rule (`c1db55ab`) can block a new, unsigned program with no reputation yet, and need cloud-delivered protection | [ASR rules reference](https://learn.microsoft.com/en-us/defender-endpoint/attack-surface-reduction-rules-reference) | Cloud-delivered protection is off on both images, so the audit run of `pitboard.exe` saw neither rule judge it. |
| Moderators label a portable archive `Portable-Archive`, a blocking issue | [Validation failure guide](https://github.com/microsoft/winget-pkgs/blob/master/doc/ValidationFailureGuide.md) | No manifest has been submitted. |
| winget puts a portable package's install folder on `PATH`, not a link in Links, for a standard user without Developer Mode | [winget-pkgs FAQ](https://github.com/microsoft/winget-pkgs/blob/master/doc/FAQ.md) | Both images have Developer Mode on, and the job's user is an administrator. A standard user's install is the session's. |
| Since winget 1.29, an MSI wins over a portable installer whatever the manifest's order | [winget-cli 1.29.380](https://github.com/microsoft/winget-cli/releases/tag/v1.29.380) | windows-2025's winget is 1.11.510, and no runner installs from winget's sources. |
| A sign-out's `WM_ENDSESSION` reaches a top-level window that is never shown, and no message-only window | [Message-only windows](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features#message-only-windows) | RF-T8's `WM_SETTINGCHANGE` broadcast reached a never-shown top-level window and not a message-only one (runs 39310064516 and 39320680285). The session's `msiexec /fa` ended Pitboard through its hidden window by `WM_ENDSESSION` or `WM_CLOSE`, and did not record which ([The session machine](#the-session-machine)). No runner signs out. |
| Restart Manager sends `WM_CLOSE` to a program that agreed to end and has not exited | [Guidelines for Applications](https://learn.microsoft.com/en-us/windows/win32/rstmgr/guidelines-for-applications) | CF9's stand-in that agreed ended on `WM_ENDSESSION` (run 38895887782), and the session's `msiexec /fa` ended Pitboard without force; neither recorded a `WM_CLOSE`. |
| `GetApplicationUserModelId` gives a packaged process's AUMID, and `APPMODEL_ERROR_NO_APPLICATION` for one without an app | [GetApplicationUserModelId](https://learn.microsoft.com/en-us/windows/win32/api/appmodel/nf-appmodel-getapplicationusermodelid) | No runner starts a packaged process; the session's step A4 reads ChatGPT's through `Pitboard.Probe app-control running`. |
| `IApplicationActivationManager::ActivateApplication` opens a Store app by its AUMID from an unpackaged process | [IApplicationActivationManager](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-iapplicationactivationmanager) | No runner holds a Store app the test user may open; the session's step A4 opens ChatGPT through `Pitboard.Probe app-control reopen`. |
| Windows may refuse to bring a window forward once another app took the foreground | [SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow) | Only a person's session opens ChatGPT behind Pitboard: the session's step A4 records which window ends in front. |
| A notification icon's menu closes on a click elsewhere only when its window was made the foreground window first | [TrackPopupMenu](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-trackpopupmenu) | Only a person clicks elsewhere: the session's step A2. |
| A GUID icon is tied to the path of the exe that added it, unless both are signed by one company | [NOTIFYICONDATAW](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-notifyicondataw) | No Release build runs from two paths on a runner; the 1.0 session's P4 upgrades one. |
| A self-contained unpackaged app runs without the Windows App Runtime's packages | [Deploy self-contained apps](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/self-contained-deploy/deploy-self-contained-apps) | Both images hold the 2.5.1.0 package (run 38804714545); the session's step A5 records `Get-AppxPackage *WindowsAppRuntime*` on a clean PC. |
| A notification with the tag of one Windows still shows takes its place | [ToastNotification.Tag](https://learn.microsoft.com/en-us/uwp/api/windows.ui.notifications.toastnotification.tag) | The same; `Pitboard.Probe` posts twice at the session's step A5. |
| A click on a notification starts the app again once it called `Unregister` and quit | [AppNotificationManager.Register](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.windows.appnotifications.appnotificationmanager.register) | Only a person clicks a notification: the session's step A5. |
| A program given identity by a package with an external location gets a share with `AppInstance.GetActivatedEventArgs`, from Windows 10 2004 on | [Get activation info for packaged apps](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/get-activation-info-for-packaged-apps) | No runner registers Pitboard's package or shares a page; the 1.0 session's P1 shares from Edge, Chrome and Firefox. |
| The system shows the sentence a share target gives `ShareOperation.ReportError` | [ReportError](https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.datatransfer.sharetarget.shareoperation.reporterror) | Only a person shares a page: P1. |
| A sparse package registers only when signed by a certificate the machine trusts, its Publisher the certificate's subject, and its program's `<msix>` element names the same publisher, package and application | [Grant package identity by packaging with external location](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps) | CF8 registered PitboardShare's with its certificate in `LocalMachine\TrustedPeople`, and had the probe's refused with it in the user's own `TrustedPeople` (runs 38928930271 and 38882775456); no runner tried another publisher or `<msix>` element. P1 registers the signed release's. |
| `VerifyVersionInfoW` compares with the running Windows only in a program whose manifest names Windows 10, and with Windows 8 in one without a manifest, so WinSparkle reads a feed's `minimumSystemVersion` against the real build only there | [VerifyVersionInfoW](https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-verifyversioninfow) | No runner checks for an update; the 1.0 session's P3 is the first. |
| An Environment row `=-PATH` with `[~];<value>` appends to the user `PATH` and takes only its part off on uninstall; a new process sees it, a running one does not | [Environment table](https://learn.microsoft.com/en-us/windows/win32/msi/environment-table), [Environment variables](https://learn.microsoft.com/en-us/windows/win32/procthread/environment-variables) | The session of 18 October 2026 measured all but a terminal open before the install, which it could not read. |
| An exe custom action after `InstallFinalize`, asynchronous with no wait, starts `Pitboard.exe --updated` as the person once a `/passive` update ends | [Custom action return processing options](https://learn.microsoft.com/en-us/windows/win32/msi/custom-action-return-processing-options) | No runner updates Pitboard; the 1.0 session's P3 does. |
| A deferred custom action without `msidbCustomActionTypeNoImpersonate` runs as the person running the install or removal, so a per-user removal started unelevated has `Pitboard.exe --unregister` pass the core's gate and reach that person's renewal task. One started from an elevated terminal runs it elevated, the gate refuses it, and the task stays | [Custom action in-script execution options](https://learn.microsoft.com/en-us/windows/win32/msi/custom-action-in-script-execution-options) | The session of 18 October 2026 saw `--unregister` delete the person's `Run` value, and read no token. No runner installs an MSI; the next session removes Pitboard from a terminal that is not elevated and from one that is, and reads whether the task went. |
| `msiexec /a` extracts an MSI only on a machine whose platform its `Template` names, as an install would, so `windows-msi`, `windows-feed-key` and `windows-install` each read the MSI for their own machine | [Template Summary property](https://learn.microsoft.com/en-us/windows/win32/msi/template-summary) | Each runner extracts only its own MSI: no row tries the other's. |
| `RemovePackageAsync` shuts down every process of the package as it removes it, so `PitboardShare.exe` could not remove its own package | [PackageManager.RemovePackageAsync](https://learn.microsoft.com/en-us/uwp/api/windows.management.deployment.packagemanager.removepackageasync) | CF8 removed PitboardShare's with no process of the package running (run 38928930271). P1 uninstalls Pitboard and finds the package gone. |
| A process that `PitboardShare.exe` starts through `CreateProcess` carries none of its package identity, since a process with no desktop app policy set starts its children outside the package | [UpdateProcThreadAttribute](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute) | The probe's identity process, started by path, gave its `CreateProcessW` children none, and one handed a link as its only argument got it whole (runs 38928930271, 39291962999 and 39301311398, CF8 under [Windows](#windows)). No runner activates a share target, whose activation may set a policy that keeps children in the package: the 1.0 session's P1 reads `GetCurrentPackageFamilyName` in a `Pitboard.exe` a real share started and, with no copy running, whether it keeps running once `PitboardShare.exe` has returned. |

### One sign-in at a time

The core's one sign-in at a time is a lock on `signin.lock`, taken with `File::try_lock`,
which is `flock`.

- Measured on macOS 27.0 on 5 October 2026, with Rust 1.98.1: a lock taken with
  `File::try_lock` can stay held after its `File` is dropped, though std opens every file so
  that a program started after does not keep it. A process another thread is starting holds
  a copy of the descriptor until it runs its program, or ends. Dropped and taken again at
  once, in 3000 rounds each, the lock was still held 437 to 1042 times, for up to 5.5 ms,
  while another thread started processes the way std forks and execs, which it does for a
  program named bare with `PATH` set; 8 to 15 times, for up to 77 µs, where std uses
  `posix_spawn`; and never with no process started. Through the core's own sign-in, one
  started right after a cancel in the same home was refused as one already waiting 64 times
  in 100 while another thread started a program it could not find, and never in 100 while
  one started `/usr/bin/true` or none started anything.
- Measured on 6 October 2026, with Rust 1.98.1, on macOS 27.0 and on Debian 13 with glibc
  2.41 in Docker, both on arm64, by a program of the measurement's own, run three times on
  each: a lock taken on a file, let go of, and at once taken again on the file opened again,
  in 20,000 rounds 200 µs apart, while one other thread started one process after another
  and waited for each. Let go of by closing the file, the lock was still held:
  - with nothing started, never;
  - while the other thread named its program bare with `PATH` set, which std forks and execs
    for: found nowhere, 10 to 13 times on macOS, for up to 781 µs, and 71 to 97 times on
    Linux, for up to 231 µs; found, 4 to 6 times, for up to 731 µs, and 44 to 55, for up to
    269 µs;
  - while it named its program by its path, which std starts with `posix_spawn`: found, 7 to
    13 times on macOS, for up to 61 µs, and 54 to 60 on Linux, for up to 141 µs; where
    nothing is, 166 to 200 times, for up to 151 µs, and 151 to 171, for up to 109 µs,
    starting more since each fails at once.

  Let go of with `File::unlock` before the file was closed, it was never held, however the
  other thread started its processes, in any run on either system. Named by its path, a
  process holds the copy for less time on macOS and for about as long on Linux, but holds
  it: starting a program by the path it was found at does not close the window, and letting
  go of the lock by name does. So a `SignIn` lets go of its lock with `File::unlock` as it
  is dropped, after clearing its directory, and `WatchedSignIn::cancel` returns once it is
  free. The core already starts every tool it found by the path it found it at. It names a
  tool bare, with `PATH` set, only where it found none, in `provider::command`, and
  `ready_to_sign_in` refuses such a sign-in before it gets there, so that runs only for a
  program taken away between the two. It is left as it is.
- Cancel and then Sign In at once, through the app's model over the real core, measured on
  6 October 2026 on the same two systems with `pitboard-ffi`'s tests. Before the model
  waited for the sign-in cancelled and the lock was let go of by name,
  `a_sign_in_asked_for_at_once_after_a_cancel_starts_once_that_one_has_stopped`, with a
  stand-in for `claude`, failed in 13 of 50 runs alone and in 30 of 40 runs of the whole
  suite on macOS. On Linux it failed in 4 of 100 runs of the whole suite, and in 36 of 200
  with four suites running at once, where the fixture's
  `accounts_are_added_through_each_tools_sign_in` failed 5 times in all as CI met it (run
  37437790666) and `a_sign_in_asked_for_at_once_after_a_cancel_asks_for_the_code` 11. Each
  time the second sign-in was refused as `sign_in_in_progress`. After, none of them failed
  that way: in 50 runs alone and 140 of the whole suite on macOS, and in 100 of the whole
  suite and 200 with four at once on Linux. In 2 of the 140 on macOS the fixture's test
  failed otherwise, in its Codex sign-in, which cancels nothing: a look for changes that
  answered after the sign-in had finished took its enrolment for a change made elsewhere,
  so the read after the sign-in was dropped, and nothing said when the accounts were read.
- A program a tool started can hold the tool's output open once the tool has been stopped
  and waited for. Read on 6 October 2026: `bin/codex.js` of @openai/codex 0.149.1, the
  `codex` npm installs, starts the native `codex` with `stdio: "inherit"` and hands on only
  `SIGINT`, `SIGTERM` and `SIGHUP`, so killing it leaves `codex login` running with its
  output. Measured the same day on the same two systems with
  `a_sign_in_after_a_cancel_waits_for_no_output_the_tool_stopped_left_open`, whose stand-in
  for `claude` starts a shell that holds its output open until the test lets it go, then
  Cancel and Sign In. While the model waited for the cancelled sign-in's thread, the second
  sign-in asked for its code only once that output had closed: let go after 3 s, in 3.03 to
  3.07 s on macOS and 3.02 to 3.07 s on Linux; after 6 s, in 6.05 to 6.06 s on macOS; never
  let go, not within 20 s on either. Taking the stop's own word that it had stopped the
  tool, it asked in 6.8 to 9.7 ms on macOS and 1.4 to 1.9 ms on Linux, 20 runs each, with
  that output still open.

### A look and the app's own changes

Measured on 6 October 2026 on macOS 27.0 (arm64), with Rust 1.98.1 and `pitboard-ffi`'s
tests, and read in the macOS app's Swift at 0.7.0 (277539b).

- A look reads when the account index was last written on the lane of reads, while a change
  the app makes writes it on the lane of changes, or a sign-in's enrolment on that sign-in's
  own thread, and their answers reach the model in the order they are sent. A look that read
  the index after a change wrote it, and whose answer came after the change's, was compared
  once the change had taken the ticket of the read it asked for: it counted a change made
  elsewhere, and that read was dropped. The fixture's
  `accounts_are_added_through_each_tools_sign_in`, which signs a Codex account in and
  cancels nothing, met it in 2 of 140 runs of the whole suite, with nothing said of when the
  accounts were read.
- With looks back to back, each asked for as the last one answered, a rename, a forget,
  naming the login in use and a sign-in met it in none of 10 runs: the core writes its log
  of changes after the index and before the change answers, so the first look to read the
  change's write answered first. Held behind a read on the lane of reads instead, which
  waits for the lock usage readings are written under while the test holds it, a look reads
  the index after the change and answers after it every time. Against the model before it
  held the index for every change of its own,
  `a_rename_is_read_after_it_whatever_the_poll_finds_meanwhile`,
  `naming_the_login_in_use_is_read_after_it_whatever_the_poll_finds_meanwhile` and
  `a_sign_in_is_read_after_it_enrols_whatever_the_poll_finds_meanwhile` failed in 10 of 10
  runs, each with nothing read after the change; after, in none of 50.
- The Swift model at 0.7.0 had the same race. PitboardService.swift asked `changedAt` on its
  queue of reads, and enrolled, renamed, forgot, renewed and gave up on an interrupted switch
  on its queue of changes. AppModel.swift's `enrol`, `rename`, `forget`,
  `abandonStuckSwitch` and a sign-in's `watch` counted the change in `changesSeen` and then
  read, the read keeping the count it started with, and `noticeOtherChanges` counted any move
  of the index's time as a change made elsewhere unless `switching` was set. So a look whose
  `changedAt` came back after the change's write, and whose comparison ran once the read had
  taken its count, dropped the read and left `updatedAt` nil; one whose read of what is known
  landed before a rename's own code ran put away what the switch to that account had said.

### Claude Code

Read against Claude Code 2.1.284's own storage layer, macOS and Linux builds alike, on 29
September 2026, unless a fact gives its own date. The conformance runs of 24 and 28
September reported three facts moved on 2.1.281 and 2.1.283. All three were false: they
read the Linux build, which has no keychain code, and a `libsecret` that belongs to the Bun
runtime Claude Code ships in. The one real change, in 2.1.281, is how a locked keychain is
treated, and only the macOS build shows it. The run reads both builds since.

- A running session serves its login from a 30 second cache, so it picks up a switch within
  about 33 seconds, where no `.credentials.json` sits behind the keychain.
- Read on 8 October 2026 from the macOS build of 2.1.294, and on 9 October from its Linux
  and Windows builds, whose code here is the same, and not measured against a running
  session: before each request a session looks at `.credentials.json`, by its modification
  time alone. Where it is not there, or the look fails, the session reads the keychain
  through its 30 second cache. Where it is there, whatever it holds, `{}` included, and
  whether or not it can be read, the session keeps the login it holds while that login is
  usable. It reads again when the file's modification time changes, after a 401, at its own
  sign-in, and 5 minutes before its login expires, when it takes the store's login where it
  differs. So while the file sits behind the keychain, a session already running keeps its
  account after a switch until its login is next renewed or it is started again. On Linux
  the file is the store, and a switch's write changes its modification time.
- Every write of the login takes proper-lockfile's directory lock at
  `<storage dir>/.storage-write`: stale after 15000 ms, ten retries, 100 ms to 1000 ms of
  backoff. `lock.rs` carries the same numbers.
- Every write under that lock drops the read cache, reads the login again inside the lock,
  and abandons the write when that read fails. A stale account cannot be written back. From
  2.1.281, a locked keychain counts as a failed read here once the process has seen its
  item; before, it read as empty.
- A renewal of the login takes another lock first, proper-lockfile's directory lock at
  `<storage dir>/.oauth_refresh.lock`, stale after 60000 ms and touched every 5000 ms, then
  the legacy `<storage dir, links resolved>.lock` the same way, going without the second
  where it cannot be made. Under them it reads the login again and sends the refresh token,
  and takes the write lock only to save the answer, where the login stored still holds the
  token it sent; otherwise it saves nothing. So the write lock does not keep a renewal from
  spending a refresh token, and the refresh lock does. A holder is taken over before the
  lock is stale only where its owner record, `.oauth_refresh.lock.owner`, proves it gone,
  and Pitboard writes none. Read on 9 October 2026 in the macOS, Linux and Windows builds of
  2.1.294, whose code here is the same.
- Claude Code treats its own lock going missing as a warning and keeps writing. Pitboard
  cannot expect the other side to stop.
- A write can be marked as already locked without the lock being taken. `/logout` does this
  after retrying for 7.5 seconds, and deletes the login with no lock held.
- The keychain write is `security -i` while the command is at most 4032 bytes. Past that it
  is `add-generic-password -U -a <account> -s <service> -X <hex>`, on the argument line. It
  never refuses or splits a login, and each call has a 2 second timeout.
- A failed write is transient, and does not move the login to the plaintext file, when it
  timed out or, from 2.1.281, when it exited 36 after the process had seen its item. Any
  other failure moves the login to the file.
- The login goes hex-encoded, two characters a byte, so standard input carries about 2 KB of
  it. A larger login is on Claude Code's own argument line at every token refresh. Pitboard
  keeps its `security -i` command within the same 4032 bytes, the limit its messages give.
- The keychain read is `find-generic-password -a <account> -w -s <service>`. Exit 0 with
  output is the login; exit 0 with nothing, 44, or output that is not JSON is absent. Exit
  36, a locked keychain, is absent to an ordinary read, a failed read to a write once the
  process has seen its item (from 2.1.281), and a failed read outright only when a caller
  asks. Pitboard reads 36 as a locked keychain, never as empty, the strict end of that.
  `security show-keychain-info` exiting 36 only adds an unlock hint.
- On macOS the live chain is the keychain, with the plaintext file `.credentials.json`
  behind it. The successor backend, behind the `tengu_hover_rest` flag, replaces only the
  fallback half, and only for a caller that hands one in. An ordinary `claude` still reads
  the keychain first.
- Claude Code demotes to the plaintext file when a keychain write fails for good, and
  deletes the keychain item when it does. A locked keychain after the item was seen is not
  failing for good, from 2.1.281.
- Read in 2.1.294, and the same from 2.1.291: the keychain item is deleted on a demotion only
  when the keychain read non-empty before the write. A locked keychain reads empty to a
  session that has not seen the item, so a sign-in there, such as `/login` over SSH, writes
  `.credentials.json` and leaves the keychain's login in place. Measured on 8 October 2026.
- A keychain write that lands deletes `.credentials.json` only when the keychain held
  nothing before it. Once both hold a login, the file's outlives every token refresh, every
  sign-in from a desktop session and every switch, and a session that cannot read the
  keychain signs in with it. `doctor` names it as `fallback_login`, and every read that
  reads the keychain and every change warns while it is there, each naming `pitboard stow`,
  the one thing of Pitboard's that deletes it.
- While the keychain is locked, a session keeps serving the login it last read, cached again
  every 30 seconds, and follows no switch. One that has read none reads the keychain as
  empty and signs in with the file, or is signed out.
- The supervisor daemon records itself in `<config dir>/daemon.lock`, with its pid and the
  Claude Code version that started it. It leaves the file behind when it stops.
- Read on 16 October 2026 from the win32-x64 and win32-arm64 builds of 2.1.300
  (`windows_daemon_identity`): on Windows the lock also holds `procStartFt`, the daemon's
  creation time from `GetProcessTimes` as a decimal FILETIME, and Claude Code takes the pid
  for the daemon only while that time is the same text. Pitboard compares it the same way,
  so a pid Windows has reused reads as gone. The lock holds no `pidDomain`.
- Claude Code's storage backends are `keychain`, `plaintext` and `windows-credman`, the
  last behind the `tengu_windows_credman` flag. Its code has no `libsecret`,
  `org.freedesktop.secrets`, `gnome-keyring` or `SecretService`. The Linux binary does
  contain `libsecret`, in the Bun runtime it ships in, behind `Bun.secrets`, which Claude
  Code's code never calls.
- `secret-tool` and `kwallet-query` do appear in the bundle, in the credential helpers its
  Bash sandbox keeps out of a shell. So neither name is used to look for a keyring backend.
- On Linux, Claude Code has no keychain backend at all: the plaintext file holds the login.
  Claude Code writes it, then sets its mode to 0600, and Pitboard's Linux host matches
  that.
- The register holds that absence as `no_keyring_off_macos`, and the conformance run looks
  for `Bun.secrets` and each keyring name in every Linux build. A fact that rests on
  something not existing is wrong the moment it does.
- A Claude Code parked login holds the account's slice of Claude Code's credential
  document. On one real account, measured on 22 September 2026, the slice was 524 bytes
  against 506 for the OAuth block alone. An account holding a device token has not been
  measured.
- Claude Code's config can name another account than the login stored. Read from 2.1.294 in
  its macOS build on 8 October 2026 and in its Linux x64, Windows x64 and Windows arm64
  builds on 9 October, the same code in all four: a sign-in writes `oauthAccount`, and so
  does a process start where `profileFetchedAt` is missing, over 24 hours old or missing a
  profile field, from that process's own login. Every start signed in to claude.ai also
  writes the email and organisation Anthropic's bootstrap gives its login, where that names
  the config's account or none, so a process on another organisation of the same person
  moves it too. A token refresh writes none of them. On 8 October one machine's config named
  another account than the keychain's login for three hours. So Pitboard asks Anthropic
  whose a login is, records the answer (`in_use`), and takes a change of the config since as
  a sign that something signed in (the register's `config_identity_is_the_last_writers`).
- On 22 September 2026, the machine measured sat within 0.75 seconds of the `Date` header
  of api.anthropic.com across eight requests. `Date` has a granularity of one second.
- That spread is inside the noise, so Pitboard keeps no estimate of clock skew. A renewal's
  expiries are counted from the `Date` of the answer that carried them.
- Read from 2.1.289 on 5 October 2026: before it opens the browser, `claude auth login`
  writes an `https` address and `Paste code here if prompted > ` to stdout, and from then
  on reads a pasted code. So the app offers the code field with the address. The address
  is the manual one, whose page shows the code; the browser it opens goes to another,
  which comes back to the loopback. Piped, the address is bare unless `FORCE_HYPERLINK` is
  set or the environment names a terminal that takes hyperlinks, such as `TERM_PROGRAM` set
  to iTerm.app or `WT_SESSION` set. Then it is an OSC 8 hyperlink ended by BEL, with the
  address again as its text. The sign-in runs with the app's whole environment, so
  `provider/printed.rs` reads what it printed as a terminal does, and the address offered
  is where the hyperlink goes.
- Read from 2.1.289 on 5 October 2026, in the Windows arm64 build's sources and the macOS
  and Linux builds, where the conformance run finds each literal, and finds none in 2.1.110:
  `claude auth login` reads every line typed back while it waits. One that is not
  `<code>#<state>` with both halves, once trimmed and split at `#`, it refuses with `Invalid
  code. Please make sure the full code was copied.` on stderr, and goes on reading in the
  same process, with no new prompt. The first with both halves it takes, whatever its state
  half says. After that it still refuses a line without both halves, and ignores one with
  them; a code the token exchange refuses ends the sign-in with `Login failed: ` and exit
  status 1. So the model offers the code field again once Claude Code has refused one, to
  the same sign-in, and starts nothing again.
- Read on 5 October 2026 from the macOS builds of 2.1.283 and 2.1.289 and the Linux build of
  2.1.289: the native installer puts its launcher at `~/.local/bin/claude`, and says so
  when that directory is not on `PATH`. A global npm install puts `claude` in npm's global
  `bin`, which is Homebrew's `/opt/homebrew/bin` or `/usr/local/bin` where Homebrew
  installed Node, and `/usr/local/bin` where nodejs.org's installer did; the build lists
  both among npm's places. Claude Code tells a global npm install by
  `/node_modules/@anthropic-ai/` in the path the running program resolves to, and the
  Homebrew cask's by a path through a Homebrew `Caskroom`. So an app with no shell's `PATH`
  looks in `~/.local/bin`, `/opt/homebrew/bin` and `/usr/local/bin`, after the login
  shell's.
- Read on 15 October 2026 from the win32-x64 and win32-arm64 builds of 2.1.300, the npm
  package of that version and winget's manifest of 2.1.292
  (`windows_install_layout`): on Windows the native installer keeps its launcher as a
  copy, `claude.exe` in the `.local\bin` of `HOME`, else of `os.homedir()`, put in place
  by a rename once its checksum matches. npm's `claude.cmd` starts the package's
  `bin\claude.exe` as it is, which postinstall links or copies from the platform's
  package. winget's is the bare portable `claude.exe`, linked in its `Links`. So an app on
  Windows looks in that `.local\bin`, `%APPDATA%\npm`, `%LOCALAPPDATA%\Microsoft\WinGet\Links`
  and `%USERPROFILE%\scoop\shims`, after the registry's `Path`, each only where its
  variable is set.
- Read on 16 October 2026 from the win32-x64 and win32-arm64 builds of 2.1.300
  (`windows_browser_open`, `sign_in_output`, `sign_in_takes_another_code`): on Windows
  `claude auth login` prints its prompts whatever its output is, reads a pasted code with
  readline and trims it, so a line ended by `\r\n` reads as one ended by `\n`, and opens
  its page with `rundll32 url,OpenURL <address>`, a child of its own, unless its settings
  or `BROWSER` name a program. A private sign-in withholds
  `CLAUDE_CODE_FORCE_WINDOWS_CREDMAN`, and its fresh folder caches no feature, so the login
  lands in that folder's `.credentials.json` (`windows_backend_choice`, below).
- Read on 16 October 2026 from the win32-x64 and win32-arm64 builds of 2.1.301
  (`windows_backend_choice`): Claude Code keeps its login in Credential Manager, with
  `.credentials.json` behind it, where `CLAUDE_CODE_FORCE_WINDOWS_CREDMAN` is exactly `1` in
  its environment, asked on every store access, or where its config file caches
  `tengu_windows_credman` as true, read once a process; otherwise in the file alone. The
  variable reaches a session from the environment it starts with, `HKCU\Environment` over
  the machine's `Session Manager\Environment` for one started from Explorer, and from the
  `env` of the config file, of `settings.json` and of the managed settings it takes,
  `HKLM`'s policy over the folder unless that merges them (`windows_managed_settings`), in
  that order, at start-up. A project's settings pass it only once the folder is trusted, or
  in print mode. Pitboard reads every signal but a project's and a `--settings` file's, and
  takes managed sources that disagree, or one it cannot read that a session may take, as
  turning nothing on. It takes Claude Code's chain for the file only where none is on and
  Credential Manager holds nothing at the live target, `<service>/claude-code-user` or its
  manifest `#m` (`credman_account_name`). Where a signal turns the store on, Pitboard
  follows it, below. Where a login sits there that nothing Pitboard reads turned on, it
  refuses, since it cannot tell which store sessions read, and names every piece of that
  login and how to keep it. A session a project turned the store on for is caught that way
  once it wrote there.
- Read on 16 October 2026 from the same builds, with Bun 1.4.3's `Bun.secrets` as embedded
  (`credman_target`, `credman_chunking`, `credman_attributes`, `credman_migration`,
  `credman_probe`, `windows_credman_cache`): Claude Code's item is
  `<service>/claude-code-user`, the service hashed from the folder's string as given, so
  `C:\x` and `C:/x` are two items. `Bun.secrets` writes it generic, with the name as its
  user name, the UTF-8 text as its blob, enterprise persistence, no comment and no
  attributes (RF-G3, run 38704825885). Up to 2400 bytes the login is the item's value; above
  that, base64 pieces `#0` on of 2400 characters, at most 256, a manifest `#m` and a marker
  `#p`, each an item of its own. Claude Code reads pieces under a manifest that names their
  count and length, and never reads `#p`, so pieces replaced in place under a manifest of
  the same length can read half old and half new. Pitboard writes in Claude Code's order
  with one step of its own: while it replaces pieces in place, the manifest names a length
  no base64 has, and as many pieces as the write may leave, so Claude Code's read fails, and
  a session keeps the login it has, rather than read a mix. A write takes away every piece
  the manifest or a `#p` a stopped write left counts. Pitboard reads a manifest Claude Code
  would pass over, or `#p` beside one, as unreadable, and only a whole write ends a write
  that stopped there: a switch whose write failed partway writes the login that was there
  back, and one killed partway is finished by the next change, which writes the incoming
  login whole, without the outgoing document's part that is no account's, such as MCP
  tokens, since nothing reads that back. Every write goes to Credential Manager first, and
  deletes `.credentials.json` only where Credential Manager held no login before, as Claude
  Code's wrapper does; where Credential Manager fails, Claude Code writes the file instead
  and Pitboard does not, leaving the switch to its rollback and the journal's recovery. A
  session reads the item again 30 seconds after it last did (`windows_credman_cache`, read,
  pending session step 7), and a session whose start-up look at `Bun.secrets` fails, or
  gives no answer within 10 seconds, reads only the file for its life. Where the file is the
  store, a session reads it again once its modification time changes
  (`windows_file_adoption`, read, and measured on the session machine). A network logon has
  no credential set: every call fails 1312, the look at the live target before the chain is
  built included, which Pitboard reads as out of reach (`credential_store_locked`), never as
  no login.
- Measured on both runner images on 16 October 2026 (runs 38856939312 and 38858363708): a
  fresh standard user's Credential Manager takes about 237 pieces of 2400 characters, then
  `CredWriteW` fails with `ERROR_NOT_ENOUGH_MEMORY`, and pieces a failed run left count
  against that. A login near Claude Code's 256 pieces fails there, so the runner tests write
  at most 100 pieces, and a round trip over items in memory holds the 256-piece edge.
- Read on 16 October 2026 from the same builds (`credman_unreachable_writes_file`,
  `fallback_file_pins_session_login`): a session whose start-up look failed, as one in a
  network logon does, reads no login in Credential Manager and fails every write there, so
  its sign-in or renewal writes `.credentials.json` and leaves the item as it was, since the
  wrapper deletes that only where its read before the write held a login. The file then
  sits behind Credential Manager as it sits behind the keychain on macOS: every session
  looks at it before each request, the same code on every system, and keeps the login it
  holds while it is there.
- Read on 16 October 2026 from the same builds, with Bun 1.4.3 and libuv 8023581 as
  embedded (`windows_plaintext_write`, `windows_plaintext_strict_read`): Claude Code writes
  `.credentials.json` by a temporary file renamed over it with `MoveFileExW`, then `chmod`
  0600, which on Windows only clears the read-only attribute, and sets no access list. Its
  write fails over a read-only file, which Pitboard refuses too. Before a write it reads the
  file strictly, and any error but a missing file, a folder or a path through a file, a
  sharing violation's included, skips the write. So Pitboard's reads share reading, writing
  and deletion and are never held across Claude Code's lock.
- Read on 16 October 2026 from the win32-x64 and win32-arm64 builds of 2.1.301
  (`windows_statusline_shell`, `windows_statusline_powershell`): on Windows a status line
  runs as a hook that names no shell does, with `bash -c` in Git Bash where Claude Code finds
  one, and otherwise in PowerShell, `pwsh` before Windows PowerShell, with a notice once a
  session. It hands bash the command as written, so a path with backslashes breaks there,
  while the bare `pitboard statusline` runs in either. Doctor's `status_line` check finds
  Git Bash as Claude Code does and says whether a `pitboard.exe` is on the PATH it runs
  with, which a session started from the same shell hands its status line too. It warns of
  none only where `statusLine.command` in the managed settings or the person's own starts
  with a bare `pitboard`: a full path, or a script of the person's, needs no such PATH. It
  finds PowerShell as Claude Code does too, and names PowerShell 7 or Windows PowerShell;
  the session machine saw Claude Code 2.1.302 run each where it found it.
- Read on 15 October 2026 from the win32-x64 and win32-arm64 builds of 2.1.300
  (`windows_managed_settings`): on Windows managed settings are in `C:\Program
  Files\ClaudeCode`, as on the other systems in their folder, and in the value `Settings` of
  `HKLM\SOFTWARE\Policies\ClaudeCode`, which Claude Code reads with `reg.exe query` and takes
  over the folder unless `managedSourcesBehavior` merges them. The same value under `HKCU`
  counts only where neither holds a policy. Pitboard reads all three through the host, and
  the folder and `HKLM` both, so it may name an override a session would not take, never
  miss one.
- Read again on 23 October 2026 from the darwin-arm64, linux-x64, win32-x64 and win32-arm64
  builds of 2.1.309, on which pitboard-conformance passes every fact: `refresh_lock`,
  `keychain_write_route`, `keychain_absence_codes`, `supervisor_daemon`, the usage cache facts,
  `config_identity_is_the_last_writers` and `sign_in_output` hold. `login_carries_its_plan`
  moved: a save keeps `subscriptionTypeSource` beside the plan and takes a stamped profile's
  plan over the stored one, which `document::plan` reads the same. `windows_token_env`'s
  literals moved and its order holds. `credential_cache` waits on a measurement.
- Read on 23 October 2026 from the four builds of 2.1.309 (`auth_precedence`,
  `anthropic_profiles`, `profile_folder`, `windows_profile_folder`, `organisation_pin`): a
  session also signs in another way under `CLAUDE_CODE_USE_MANTLE`, an Anthropic profile, or
  managed settings that require the Cloud gateway's sign-in. `settings::overrides` names
  each, as it names a variable. `settings::Profiles` reads the profile variables from the
  environment only: Claude Code reads them from `process.env`, and Pitboard does not follow
  which settings `env` blocks reach it. A managed `forceLoginOrgUUID` makes Claude Code refuse
  a login of another organisation as a session starts, so `Provider::refuses` asks it of the
  account going in, by the organisation Pitboard recorded for it. Where several managed
  sources set it, a login none of them names is refused.
- Read on 23 October 2026 from the darwin-arm64 build of 2.1.309
  (`macos_managed_preferences`): on macOS a configuration profile's plist, the person's in
  `/Library/Managed Preferences/<user>` before the device's, is a managed source too. Claude
  Code converts the first it can read with `plutil` and takes it over the folder unless
  `managedSourcesBehavior` merges them, where it takes `HKLM` on Windows.
  `host::Host::managed_plist` converts it the same way, and `settings::managed_taken` puts it
  in `HKLM`'s place.
- Read on 15 October 2026 from the win32-x64 and win32-arm64 builds of 2.1.300
  (`windows_version_resource`): `claude.exe` carries a version resource whose product
  version is `2.1.300.0`. So doctor reads the release there on Windows, once npm's
  `claude.cmd` is followed to the program it starts, and never runs `claude --version`.
- Read on 6 October 2026 from the macOS, Linux and Windows builds of 2.1.289, whose code
  here is the same: the config dir is `CLAUDE_CONFIG_DIR ?? join(homedir(), ".claude")`,
  normalised to NFC, and the config file's base is `CLAUDE_CONFIG_DIR || homedir()`. So an
  empty `CLAUDE_CONFIG_DIR` leaves `.claude.json` in the home folder, but makes the config
  dir the empty path. A legacy `.config.json` is then looked for in the working directory,
  and so is `.credentials.json`, which is kept in the config dir unless
  `CLAUDE_SECURESTORAGE_CONFIG_DIR` is set. With that unset, the credential slot's name
  tests `!CLAUDE_CONFIG_DIR`, so an empty value names the default slot. Pitboard read an
  empty value as unset for all of them, which was right for the file and the slot, and not
  for the directory: it took Claude Code's lock and, on Linux, switched its login in
  `~/.claude`, where no Claude Code started with it looks. No one folder holds that login,
  so Pitboard refuses an empty `CLAUDE_CONFIG_DIR` as a home that is not a full path, and
  reads the file and the slot as Claude Code does. The win32-x64 build's JavaScript refuses
  a relative config dir itself in one of its features: "the configuration home
  (CLAUDE_CONFIG_DIR) is not an absolute path".
- Read on 8 October 2026 from the macOS build of 2.1.294, and on 9 October from its Linux
  and Windows builds, whose code here is the same: Claude Code fills `cachedUsageUtilization`
  in its config with the answer to a usage request made with the login its session holds,
  and stamps it with the `accountUuid` the config names. Nothing compares the two. A session
  that has not yet taken a switch writes the numbers of the account switched away from under
  the account switched to. Of five caches captured on one machine on 8 October, four held
  the numbers of another login than the account they named, one with no switch near it. So
  Pitboard takes no usage from the cache. Taken as the reading of the account in use, it
  showed the previous account's numbers after a switch, and the automatic switch acted on
  them.
- Read on the same days from the same four builds, whose code here is the same: `/status`
  prints the email and organisation of the config's `oauthAccount`, and a running session
  reads the config again within about a second of a change. `/usage` shows the numbers of
  the session's last response, or, where it has none, the config's usage cache where its
  stamp is the config's account and it is under an hour old. It then answers from that
  cache where the cache is under 60 seconds old and newer than the session's last response,
  and otherwise asks with the login the session holds. So a session that has not taken a
  switch names the account switched to in `/status`, while `/usage` and every request go on
  with the login it holds. What a switch says of running sessions names no account for that
  reason.
- Read on the same days from the same four builds, whose code here is the same: the JSON a
  session passes its status line holds `rate_limits.five_hour` and `seven_day`, each a share
  used and a reset, taken whole from one response's headers and only for windows whose
  reset is ahead. It names no account, organisation or email. Only the process's own
  sign-in or sign-out, the end of a remote attach after one, or a response while it holds no
  claude.ai login empties it. So a session passes the numbers of the login it holds,
  whatever the config names, and the status line tells whose they are by their windows (the
  register's `status_line_input_names_no_account`). The two windows of one run are of one
  response, so a five-hour window no answer has given yet is taken on the word of the weekly
  window passed with it.

Not measured. Switching by itself rests on the session cache's 33 seconds, measured on
2.1.278, and, while a file sits behind the keychain, on a reading of 2.1.294 that no running
session has been measured against. These were not measured, and the register cannot hold
them, since every fact in it is read from a build:

- Whether a request a session has under way at the moment of a switch finishes, on either
  account.
- What Claude Code does when an account reaches a limit: whether a session stops, waits or
  tries again, and whether it would take a switch made after that. Pitboard switches at a
  share under 100% so that a session following within the 33 seconds need not meet it,
  and the docs say neither is known.
- A busy session renewing its login while an automatic switch is under way, against a
  running build. The refresh lock a switch holds (`refresh_lock`) keeps a renewal from
  starting while the outgoing login is parked, and the read again under the write lock
  (`write_lock`) finds one that finished before it, as for any switch.
  `a_session_renewing_the_outgoing_login_during_a_switch_waits_for_it` in `switch/mod.rs`
  plays the first on the stores in memory, with a session that takes the refresh lock as
  2.1.294 does. `a_login_claude_code_renews_partway_through_is_not_written_over` in
  `switch/auto.rs` plays a login that changed under the switch all the same: the switch
  stops with `signed_in_account_changed`, nothing is written over the renewed login, the
  account it would have switched to keeps its parked login, and the next attempt, a minute
  on, switches.

### Anthropic's usage answer

Read on 8 October 2026, on one machine. Claude Code 2.1.294 keeps the `utilization` object
of its last `GET /api/oauth/usage` in `~/.claude.json`, and five backups of that file held
one each. Four, each a Team seat's, hold `limits` `session` and one model's `weekly_scoped`,
and no `weekly_all`. The fifth, an answer from 16:13:36, was noted with `session` alone and
not read again to say whether that was all it held. Pitboard's own reading of a personal Max
account that day, taken from Anthropic's answers, holds `session`, `weekly_all` and
`weekly_scoped`. So an answer lists every limit its account has, and one it leaves out is
one the account does not have. Anthropic documents none of this, and it was seen on one
machine on one day.

- `usage::answered` rests on it: an answer whose every row of `limits` was read replaces the
  reading whole (`Snapshot::lists_every_limit`).
- `usage::room` rests on it: a limit such a reading leaves out counts as one with none used,
  so a Team seat is a place to go once a personal account's weekly limit for all models
  reaches the share, or runs out for the app's advice.
- The status line rests on it: a session that passes a limit such a reading leaves out is
  not on that account. What 0.9.0's status line kept in `~/.pitboard/sessions.json` on
  that machine the same day agrees: the last runs of ten sessions in two Team seats'
  five-hour windows passed that window alone, and those of four sessions of personal
  accounts each passed a weekly window. Were a seat's session to pass one, nothing it
  passed would be filed under the seat.
- Other readings say nothing of a limit they leave out: one in the older named shape, one
  with a row that did not normalise, one an older Pitboard wrote, and every one of
  OpenAI's. Such a reading leaves no room unless it gives every limit of the account in
  use. It may leave out a model's limit other than the one at the share where it gives the
  five-hour and weekly limits: the older named shape names no model's, and plans limit
  models apart.
- Whether a plan with no limit for a model offers that model at all is not known. Pitboard
  counts it as room.

Measured on 12 October 2026 with Claude Code 2.1.296, on a Team 5x and a Max 20x account, by
asking the usage endpoint before and 20 seconds after one `claude -p` request on Haiku made
with each account's access token on a descriptor, from a private home. The register holds
each as a fact of its own (`request_starts_the_five_hour_period`,
`weekly_period_is_a_fixed_cycle`, `used_up_limit_refuses_every_request`).

- A five-hour period not running reads as the `session` row at 0% with `resets_at` null,
  and `five_hour` with `resets_at` null, in an answer that lists every limit. `warm::decide`
  reads a period with no reset, or a passed one, as not running.
- One request starts it, on the plan rather than the monthly API credit
  (`fallback_credit: null`), at the start of the ten minutes it fell in: a request at 17:52
  gave a reset at 22:50, one at 18:02 a reset at 23:00. Two samples, so the ten minutes are
  a reading, not a guarantee. A warm-up up to nine minutes after a reset loses nothing.
- The weekly limit, and a model's beside it, runs on a fixed cycle: two minutes after one
  reset, with no request between, the answer already gave the next week's reset at 0%. So
  only the five-hour period is warmed up.
- While a limit of the account's own is used up, a request is refused with
  `usage_limit_reached`, spends nothing and starts no period. So `warm::decide` warms up no
  account with one used up. A model's limit used up alone was not measured, and holds
  nothing back.

### Codex

Read against codex-cli 0.154.0: the binary, its public source at tag `rust-v0.154.0`, and a
real `auth.json` that build wrote. The register is `provider/codex/assumptions.rs`.

- The login is `$CODEX_HOME/auth.json`, by default `~/.codex/auth.json`, at mode 0600.
- `file` is the packaged default store on every platform, and the only one Pitboard
  supports. `keyring`, `auto` and `ephemeral` are the others.
- Read from 0.160.0's config loader on 7 October 2026 (`codex_store_layers` and
  `codex_managed_preferences`): the store is what the highest layer that sets
  `cli_auth_credentials_store` says, lowest first: the packaged default,
  `/etc/codex/config.toml`, an enterprise's cloud config, `$CODEX_HOME/config.toml`, a
  profile's file, a trusted project's `.codex/config.toml`, `-c`, then
  `/etc/codex/managed_config.toml` and, on macOS, the managed preference
  `config_toml_base64` of `com.openai.codex` that a configuration profile forces.
  `/etc/codex/requirements.toml`, and on macOS `requirements_toml_base64`, pin it over all
  of them. A layer that is there and that Codex cannot read stops Codex from starting, and
  so does a `profile = "<name>"` line in any layer, which 0.160.0 calls a legacy way to
  choose a profile. 0.99.0 has no such refusal and chooses a profile with the line. A
  profile's table holds no store either build reads, as read at each one's tag: neither
  one's `ConfigProfile` has `cli_auth_credentials_store`, each takes the store from the
  merged top level alone, 0.160.0 does not apply a profile's `[features]`, and none of
  0.99.0's features is about the store.
  - `provider/codex/layers.rs` reads every layer but three, in that order: the cloud
    config, which `codex login` does not load and whose requirements may not set the store,
    though its configuration may, and which a session of the TUI loads, with what that does
    to a running session's store not read; a profile's, which only a run given `--profile`
    reads and `codex login` refuses; and a project's, a fact about one folder, which
    `doctor` says it does not read.
  - A `profile` line chooses the profile a `[profiles.<name>]` table in any layer
    defines, as 0.99.0 reads it, by the owner's answer of 7 October 2026. Its table holds
    no store, so the store is still the layers'. A line naming a profile no table defines
    is a store nobody can tell, `Backend::Unknown`, which 0.99.0 refuses as not found and
    0.160.0 refuses as it refuses every such line. The words say which Codex does what.
  - A layer it cannot read is a store nobody can tell, `Backend::Unknown`, refused like a
    store Pitboard does not handle. A store a requirement pins is refused naming that file,
    since no line in the person's own config changes it.
  - `[features] secret_auth_storage` is read from the same layers, and is off by default
    on macOS and Linux (`cfg!(windows)` in 0.160.0). A requirement may name that table
    `[feature_requirements]` too, as `ConfigRequirementsToml` does; a configuration
    layer's table of that name sets nothing.
- Read from 0.164.0's source on 23 October 2026 (`codex_login_restrictions`):
  `forced_login_method` and `forced_chatgpt_workspace_id` come from the same layers as the
  store, and `allowed_login_methods` and `allowed_chatgpt_workspaces` from the requirements.
  As the TUI starts, Codex signs out of a login the forced two rule out, in every store, and
  deletes it with no revoke; `codex exec`, the app server and the ChatGPT app do not. Every
  Codex leaves unused a login that any of the four rules out, by its `tokens.account_id`
  first. A Codex park is the only copy, so a switch, and a sign-in that would replace the
  login in use, ask `Provider::refuses` of the login going in before anything moves, and stop
  with `login_restricted`. `doctor` names each setting and the accounts it rules out.
- Read from 0.163.0's config loader on 16 October 2026 (`codex_windows_store_layers`): on
  Windows this machine's `config.toml` and `requirements.toml` are in `OpenAI\Codex` under
  `FOLDERID_ProgramData`, or under `C:\ProgramData` where Windows cannot say that folder.
  There is no `managed_config.toml` layer, no managed preference and no registry layer, and
  `secret_auth_storage` is on by default, so `keyring` and `auto` keep the login in the
  secret store. Cloud requirements still may not set the store on any system, and a cloud
  configuration still may, for a session of the TUI.
- Read from 0.163.0's source on 16 October 2026 (`codex_windows_login_location`,
  `codex_windows_stores`): on Windows the login is `auth.json` in Codex's home, written in
  place with std's default sharing and no access list of its own. `keyring` and `auto` keep it
  in `secrets\codex_auth.age` under a passphrase in Credential Manager
  (`secrets|<16 hex>.codex`), or, with `secret_auth_storage` off, as
  `cli|<16 hex>.Codex Auth` itself. Pitboard switches the file and refuses the others, naming
  the line that puts the login in the file. While Codex keeps the file, doctor counts the
  `cli|` and `secrets|` items Credential Manager lists by prefix, by name alone, since a
  setting Pitboard does not read, such as a cloud configuration, can put a session's login
  there. `<key>.Codex MCP Credentials` cannot be listed by prefix and is not looked for.
- `keyring` and `auto` keep the login in a keychain item, `Codex Auth`, that Codex makes
  through the Security framework. With either of those, the `secret_auth_storage` feature
  keeps it in `secrets/codex_auth.age` instead, under a keychain key.
- Those items trust only `codex`. A read by another program brings up a permission prompt,
  and choosing **Always Allow** would change Codex's item. So Pitboard refuses `keyring`
  and `auto`, with or without that feature.
- The login document holds `auth_mode`, `OPENAI_API_KEY`, `last_refresh`, and `tokens` with
  `id_token`, `access_token`, `refresh_token` and `account_id`. `OPENAI_API_KEY` can hold an
  API key Codex obtained at sign-in.
- `codex login` and `codex logout` both POST the stored refresh token to
  `https://auth.openai.com/oauth/revoke` before clearing it. This is why parking a Codex
  login moves it (`ParkSemantics::MoveOnly`).
- A running Codex holds its login in memory for the life of the process and watches no file.
  It refuses a reload whose account id has changed (`Adoption::RestartRequired`), so it
  never picks up a switch.
- Read from 0.164.0's source on 23 October 2026 (`codex_never_follows_a_switch`): before
  each refresh a running Codex reads the file again, and takes it only where it names the
  same `account_id`. After a switch to another account id it makes no refresh, and fails
  with "...signed in to another account. Please sign in again." until it is started again.
  A refresh already under way when the file changes writes nothing there unless the file
  holds the same login, by `login_owner_id`, `account_id` and `chatgpt_user_id`, and fails
  with `Account credentials changed during refresh.` It has spent the refresh token it sent,
  the one Pitboard parked. Until 0.163.0 such a refresh wrote its own account's tokens under
  whatever account id it found there, which is why a login naming two accounts is never
  parked.
- Codex writes `auth.json` with no lock of any kind, so there is none for Pitboard to share.
- Measured on 2026-10-01 from the process list of a Mac running each of them:
  - OpenAI's ChatGPT app for macOS 26.928.31416, bundle id `com.openai.codex`, runs a codex
    0.159.2 of its own: two processes of
    `ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex`, children
    of the app. That closing its windows leaves it running, and quitting it stops them, is
    read from the app's code and not yet watched.
  - Codex's background app server runs from
    `$CODEX_HOME/packages/app-server-daemon/releases/<version>/bin/codex`, a child of
    launchd, and 0.159.3 has `codex app-server daemon restart`.
  - On macOS, `ps` gives what a process was started as. A `codex` started from a shell by
    its bare name lists as `codex`; one started by its path lists the path. Pitboard tells
    the kinds apart by the directories in that path, and a bare name is a `codex` session.
  - No editor with the Codex extension was running there. Its place, a folder named
    `openai.chatgpt-<version>`, is the extension's packaged layout, not measured.
- Read again in the source at tag `rust-v0.164.0` on 23 October 2026, whose four builds pass
  pitboard-conformance: every fact of the CLI's but the Windows ones, which keep their 0.162.1
  and 0.163.0 readings. `codex_app_server_daemon` moved: where `packages/app-server-daemon`
  has no `current` and an older Codex left a daemon's pid file or log, the daemon runs from
  `packages/standalone/current`. A second `app_server_daemon` holder takes those directories,
  `holder::Location::WithinEach`. On Linux the path resolves to a release, as a standalone
  install's sessions do, so such a daemon reads as a session there. One the daemon's updater
  starts again runs from the release itself (`resolved_managed_codex_bin`), and reads as a
  session on macOS too.
- Read on 23 October 2026 from the amd64 `.deb` of ChatGPT for Linux 26.1016.61520, as bytes
  and never run (`codex_linux_app`): it installs to `/usr/lib/chatgpt` and runs its own
  `resources/codex` as `codex app-server` with the app's environment. So a folder named
  `chatgpt` holds a `chatgpt_app`, whose remedy says to quit ChatGPT and open it again: whether
  closing its windows quits it turns on something of its own, not traced.
- Read on 16 October 2026 from Codex's source at tag `rust-v0.162.1` and its win32 builds:
  - (`codex_windows_app_server`) the background app server runs
    `<CODEX_HOME>\packages\app-server-daemon\current\bin\codex.exe`, `current` a
    junction Codex points at one of its releases. It is started with `DETACHED_PROCESS |
    CREATE_BREAKAWAY_FROM_JOB`, never from an elevated terminal, so its remedy there says to
    run `codex app-server daemon restart` from a terminal that is not elevated.
  - (`codex_windows_app_identity`) the desktop app for Windows, ChatGPT- or Codex-branded,
    is the package of family `OpenAI.Codex_2p2nqsd0c76g0`, its app
    `OpenAI.Codex_2p2nqsd0c76g0!App`, and Codex tells its process by that family. So
    Pitboard does too, wherever the package is installed. That the app runs its own
    `codex.exe` inside the package, and what closing its windows does, waits on the
    session's step 10. Read again the same day at `rust-v0.163.0` and in its win32 builds,
    unchanged.
  - The Windows package holds `codex-command-runner.exe`, `codex-code-mode-host.exe`,
    `codex-windows-sandbox-setup.exe`, `codex-voice-host.exe` and `rg.exe` beside
    `codex.exe`. Only a program named `codex.exe`, in any case, is a running Codex.
- The ID token names the account: `email`, and under `https://api.openai.com/auth`,
  `chatgpt_account_id` and `chatgpt_user_id`. A Team or Business workspace shares one
  `chatgpt_account_id`, and `chatgpt_user_id` is the person.
- Pitboard identifies a Codex account by that pair, with no network call.
- Renewal is a `POST` of a JSON body `{client_id, grant_type, refresh_token}`. Read from
  0.164.0's source on 23 October 2026 (`codex_renewal`, `codex_login_shape`), it goes to
  `<issuer>/oauth/token` as the client `tokens.oauth_provenance` records, and for a login
  that records none to `https://auth.openai.com/oauth/token` as
  `app_EMoamEEZ73f0CkXaXp7hrann`. `api::Authority` reads the record, and only an https
  issuer, since the refresh token goes there. Each token in the answer is written only if
  present, and every other field, `login_owner_id` included, is kept.
- A refusal for good while the access token names another client than the one asked, and a
  refusal of the client itself (`invalid_client`, `unauthorized_client`), keep the park and
  say why (`ProviderError::OtherClient`): they say nothing of the login, and a Codex park is
  its only copy.
- `last_refresh` must be in the login, as an RFC 3339 string, or Codex reads the login as
  having no token data.
- A spent or revoked refresh token answers 400 `invalid_grant`, or one of the older
  `refresh_token_expired`, `refresh_token_reused` and `refresh_token_invalidated`, or 401.
  Any other 400 is not a dead login.
- Usage is `GET https://chatgpt.com/backend-api/wham/usage` with `Authorization: Bearer`
  and `ChatGPT-Account-ID`, and spends no quota.
- The usage answer's shape was read from a live answer. A parser written from the source
  looked for the windows in the wrong place, and returned nothing while the request
  succeeded.
- Read from 0.164.0's source on 23 October 2026 (`codex_model_limits`), not from a live
  answer: `additional_rate_limits` lists a model's own `rate_limit` under its `limit_name`,
  which Pitboard reads as windows scoped to that name, as Anthropic's model limits are. The
  answer names `account_id` and `user_id`; where either names another than the login asked
  about, `api::answered_for` takes no numbers from it, as Codex's app server shows nothing
  bound to the account.
- `CODEX_HOME` moves everything Codex keeps, and an empty one means unset. Pitboard's own
  sign-in, the one `enroll --sign-in` runs, sets it to a directory that exists, and runs
  `codex login` from inside it.
- The sign-in starts inside that directory because Codex also reads `.codex/config.toml`
  from a trusted project it starts in. That file could name a keychain store, and so could
  `/etc/codex/config.toml`, which a private home does not move. So the sign-in is
  `codex -c cli_auth_credentials_store="file" login`: read from 0.160.0
  (`codex_login_takes_a_store_override`), a `-c` before `login` is the session-flags
  layer, over the home's, the system's and a project's config. Only
  `/etc/codex/managed_config.toml`, the managed preference and a requirement are over it,
  and they choose the live store too, which a sign-in refuses first.
- `codex login` revokes the login stored in its home before signing in. It opens the
  browser itself and reads nothing from standard input.
- Read from 0.160.0 on 5 October 2026: `codex login` prints to stderr its loopback address,
  `http://localhost:<port>`, and then, bare on a line of its own, the `https` address on
  auth.openai.com to open. So the first `https` address it prints is the one to open.
- Codex's standalone installer, `scripts/install/install.sh` at tag `rust-v0.159.2`, links
  `~/.local/bin/codex`, or `$CODEX_INSTALL_DIR/codex`, to
  `$CODEX_HOME/packages/standalone/current/bin/codex`; a standalone install of 0.159.2 on a
  Mac had left that link, read on 5 October 2026. Its macOS and Linux binaries name
  `npm install -g @openai/codex` and `brew upgrade --cask codex` as the other ways it is
  kept up to date. 0.154.0 names those too, but not the standalone package layout.
- Read on 15 October 2026 from install.ps1 at tag `rust-v0.162.1`, the npm packages of
  0.162.1 and winget's manifest of 0.161.0 (`codex_windows_install_places`): on Windows
  install.ps1 makes `CODEX_INSTALL_DIR`, or `%LOCALAPPDATA%\Programs\OpenAI\Codex\bin` where
  that is unset or white space, a junction to its `current\bin`. npm's `codex.cmd` starts
  `bin\codex.js`, which runs the platform package's `vendor\<target>\bin\codex.exe`.
  winget's is a nested portable `codex-<target>.exe`, linked in its `Links` as `codex`.
- Read on 16 October 2026 from `bin/codex.js` of `@openai/codex@0.162.1`: it finds the
  platform's package as Node finds one from its own `bin`, else uses its own `vendor`, and
  starts `codex.exe` with its arguments and its variables, `CODEX_MANAGED_PACKAGE_ROOT`
  set to its package's folder and `CODEX_MANAGED_BY_NPM` to `1`. So a sign-in starts that
  `codex.exe` with the same, and no `cmd.exe` or `node.exe` runs.
- Read on 16 October 2026 from `codex-rs/login/src/server.rs` at tag `rust-v0.162.1` and
  webbrowser 1.2.4: `codex login` listens on 127.0.0.1:1455. Where something holds it, it
  sends that `GET /cancel`, which ends a login server Codex left there, and falls back to
  1457. It opens the browser with the command Windows has for `http`
  (`AssocQueryStringW`), as a child of its own with no standard streams. So a browser that
  was not running starts in Pitboard's job, and a `codex login` a killed Pitboard left
  running is ended by the next one. Before a Codex sign-in on Windows, Pitboard says what
  Codex already runs, since a sign-in open there ends too.
- Read on 16 October 2026 from 0.163.0's source, reqwest 0.12.28 and hyper-util 0.1.20
  (`codex_windows_system_proxy`): on Windows Codex sends its requests through the proxy the
  variables name, and where `HTTPS_PROXY` and `https_proxy` are unset or empty, its https
  requests through `ProxyServer` in HKCU's `Internet Settings` where `ProxyEnable` is not 0,
  whatever `HTTP_PROXY` and `ALL_PROXY` say. It reads `ProxyServer` whole as one address, so
  a proxy for each protocol reaches it as none, and one such entry alone as a host that
  fails. Claude Code 2.1.301 and Pitboard follow the variables alone, Claude Code with no
  `ALL_PROXY`, so wherever `HTTPS_PROXY` is unset doctor says where each one's requests go,
  with any user name and password in `ProxyServer` shown as `***`.
- Read on 16 October 2026 from 0.163.0's source and its linux-x64 build (`codex_wsl`): the
  Linux build takes itself to run in WSL where `WSL_DISTRO_NAME` is set or `/proc/version`
  names Microsoft, and there starts no shared background server where its home is on 9p,
  as WSL 2 mounts Windows' drives. Doctor reads WSL the same way. On Windows it says a
  switch changes the login of a Codex in WSL that uses the Windows Codex home. Whether the
  ChatGPT app's WSL mode hands its Linux codex that home is not measured (session M8), so
  doctor says it of no distribution in particular.
- Read on 15 October 2026 from the win32 builds of 0.162.1 (`codex_windows_version`):
  `codex.exe` has no version resource. The package it runs from, npm's vendored one or a
  standalone release, holds `codex-package.json` beside its `bin`, whose `version` doctor
  reads.

### Claude Desktop

Read from five builds of Claude for macOS, and on no other system. The contributor measured
2.19675.0 with two claude.ai accounts: an inspection on 3 October 2026, experiments E1 to
E18 on 4 October and a reading of `main.log` on 5 October. This repository read the
`app.asar` and `Info.plist` of 2.7032.0 on 11 and 13 October, the `app.asar` and
`Electron Framework` of 2.42304.0 on 19 October, and the `app.asar` of 3.1623.0 and 3.6556.2
on 23 October, and never ran any. The register is
`provider/desktop/assumptions.rs`, and each entry says who read it, when and how. Each fact
here gives its build.

- Claude's bundle identifier is `com.anthropic.claudefordesktop`. Its main program is
  `Contents/MacOS/Claude`, and its helpers and `chrome_crashpad_handler` run from inside the
  bundle (2.19675.0, 3 October 2026).
- `Contents/Helpers/chrome-native-host` runs as Chrome's child for the browser extension,
  not as Claude's (2.19675.0, 3 October 2026).
- Claude keeps its data in `Library/Application Support/Claude` under the home the passwd
  entry names, and a packaged build deletes `CLAUDE_USER_DATA_DIR` from its environment
  (2.7032.0).
- Claude runs from `Claude-3p` beside it, and keeps its settings there, once its managed
  configuration turns on a third-party deployment. Picking claude.ai in its chooser writes
  `deploymentMode` `1p` there, and Claude runs from `Claude` again. A `Claude-3p` can hold
  only `claude_desktop_config.json`, as an earlier build left it on a Mac with no deployment
  (3.6556.2, 23 October 2026; issue 271).
- `Cookies`, `Cookies-journal`, `Local Storage`, `Session Storage`, the two
  `IndexedDB/https_claude.ai_0.indexeddb` stores, `WebStorage` and `File System`, with
  `config.json`'s `oauth:tokenCache`, `oauth:tokenCacheV2` and `lastKnownAccountUuid`, move
  an account whole, both ways (E2, 2.19675.0, 4 October 2026).
- The token caches are keyed by `lastKnownAccountUuid` and hold the account's OAuth tokens
  (E2 and E9, 2.19675.0; read again in 2.7032.0).
- Claude keeps every name of its data folder across another account's sign-in but those it
  clears, below. 2.42304.0, 3.1623.0 and 3.6556.2 run the same seventeen named resets on that
  change, while each build makes names of its own, such as `design`, Claude Design's window,
  from 3.1623.0. So a switch moves the account's items and leaves every other name in place
  (3.6556.2, 23 October 2026).
- A database's `-journal`, `-wal` and `-shm`, and a Claude store's `.journal`, are part of
  it, and Chromium names claude.ai's IndexedDB stores for `https_claude.ai_0`. Such a part
  that no item names stops the start of a move. So does a `Network` folder, where Chromium
  keeps the jar once it moves its network service's files, which Electron does on macOS
  only under a feature flag (SQLite's documentation; Electron 44.9.0's and 44.9.3's source).
- When another account signs in, Claude empties its preview browsers under `Partitions` and
  `chrome-import-sites.json`, and, under a Microsoft 365 policy, the connector's
  `custom3pMcpOAuth`, `builtin-mcp-auth` and `custom3p-builtin-mcp.json`. A switch puts the
  arriving account's `lastKnownAccountUuid` in, so Claude never sees the change, and these
  move with the login instead. `launchChromeImportPrompt` and the lists of preview browsers
  in `claude_desktop_config.json` stay, so an account that arrives is not asked again to
  import Chrome's sites, and the system's Microsoft broker keeps its sign-in. A first
  sign-in after Pitboard parks the only account runs Claude's preview resets, which empty
  `preview-last-frames` (2.42304.0, 19 October 2026).
- `Service Worker`, `Platform Notifications`, `CdmStorage.db` and
  `declarative_performance_observer.db` are Chromium's stores for each site, and
  `session-popouts.json` and an earlier build's `local-sessions.json` name no account, so
  they move with the login too (2.42304.0 and Chromium 152.0.7977.149's source).
- `Cookies` is SQLite at `meta.version` 24, and every value starts `v10`. A session cookie
  expires about four weeks after it is set. With Claude open, `Cookies-journal` is empty
  between writes and there is no `Cookies-wal` (2.19675.0, 3 October 2026).
- At `meta.version` 24 each cookie's value, decrypted, starts with a SHA-256 of its host,
  which Claude's own cookie importer checks and strips (Chromium 152.0.7977.130's source;
  2.7032.0).
- The session cookies are `sessionKey`, `sessionKeyV2`, `sessionKeyV3`, `sessionKeyLC` and
  `sessionKeyV3LC`, for `claude.ai` and `.claude.ai` (2.7032.0).
- A `sessionKey`'s stored value is the same after a quit and an open (E3, 2.19675.0).
- Claude can replace an account's session while it stays signed in: a session put back met
  `session_stale_relogin` one second after launch, and 66 seconds later Claude stored a
  `sessionKey` under the same `lastKnownAccountUuid` (2.19675.0, 5 October 2026, seen once).
  The contributor's build of Pitboard may have been asking claude.ai for usage at the time.
- **Log out** in Claude ends the session at claude.ai: a copy put back is refused (E1,
  2.19675.0).
- After **Log out**, `lastKnownAccountUuid` still names the account, the `sessionKey` row
  is gone, and both token caches hold a 28-character value (E1b, 2.19675.0).
- On a sign-in with no name recorded, Claude wrote `lastKnownAccountUuid` 15 to 35 seconds
  after launch, and the session reached the jar on disk 31 to 63 seconds after it (E4,
  2.19675.0, two sign-ins).
- Claude writes `lastKnownAccountUuid` whenever the account its page reports differs from
  the one it holds (2.7032.0).
- As it quits, Claude writes `Cookies`, `config.json`, `Local Storage`, `Session Storage`,
  `WebStorage` and `Preferences` (E8, 2.19675.0).
- Claude takes its single-instance lock only while it runs Reset App Data, and otherwise
  writes no `SingletonLock` (E14, 2.19675.0; 3.6556.2).
- From 3.1623.0, Reset App Data writes `reset-app-data` in the data folder and opens Claude
  again, which deletes what the folder holds before it shows a window. Beside
  `reset-app-data.given-up`, it deletes nothing. Pitboard moves nothing in a folder waiting
  for a reset, the recovery of an interrupted move included (3.6556.2, 23 October 2026).
- The helpers are gone once the main program has exited (E18, 2.19675.0).
- Claude does not open itself again after a quit, checked at 0, 2, 5 and 30 seconds (E12,
  2.19675.0, one quit).
- A quit can be refused by Claude's own question while Code or Cowork sessions run, and
  otherwise takes up to 75 + 5 + 15 seconds (2.7032.0).
- A write of `config.json` goes first to `config.json.journal`: a line of three SHA-256
  fields, of the body, of `config.json` on disk and of the write under way, then the body.
  At its start Claude writes a journal's body in where the hashes match, and drops it
  otherwise (2.7032.0).
- A `config.json` that does not parse is reset, or renamed
  `config.json.corrupt-<milliseconds>` and reset (2.7032.0).
- Claude records itself running as `updaterRunningInstance` in `config.json` and removes it
  as it quits. One left behind arms an install of a staged update within 30 minutes
  (2.7032.0).
- An account switcher of Claude's own sits behind server gate `96101707` and the policy
  `disableMultiAccount` (2.7032.0).

These were not measured. Each row says what Pitboard does where it cannot know.

| Not measured | What Pitboard does |
| --- | --- |
| Whether a sign-in over a stale `lastKnownAccountUuid`, followed by a quit within seconds, leaves the new session under the old name | A session it has not seen is taken as named in one case and is never deleted. Beside a token cache holding the emptied value it is a sign-in not finished, for every reader |
| How often Claude replaces a session in place after a login is put back (seen once) | It parks the session under the fingerprint found and says so. Nothing is deleted |
| What the names a build Pitboard has not read makes hold | They stay, as Claude keeps them when another account signs in. Only a part of a store the move carries that no item names stops the start of a move, and names itself |
| Whether each account keeps its own previews, connector sign-ins and session windows across a switch, and Claude makes a missing `Partitions` or `Service Worker` again (read in 2.42304.0, not run) | They move like the site's storage. An item in neither place is gone on without, and said |
| Whether 2.19675.0 and the builds people run write `config.json.journal` in 2.7032.0's format | A journal valid by its own hashes is the settings. A stale one is ignored. A header that does not parse refuses every change |
| Whether a stale journal Pitboard left in place can become valid again | Only where a later write makes `config.json` the file it was written over, and then its body is that run's settings |
| Whether the builds people run keep the five cookie names, `meta.version` 24 and `v10` | Signed out needs none of the five names and emptied caches. Another shape refuses |
| Whether Claude ever keeps a `sessionKey` for both `.claude.ai` and `claude.ai` | Refused, whatever the values: one session on both hosts is not told from two |
| Whether the emptied token cache is 28 characters on the builds people run | A cache of another length is not emptied, so the folder is never read as signed out |
| Whether the token caches change while an account is in use | Keys are never written over keys. A parked copy that differs goes aside |
| Whether a session with none of its three keys opens as its account and writes them again (read in 2.7032.0, not run) | Reached only by a kill between the keys and the jar. The next change reads again |
| What Claude does on a folder holding parts of two accounts | No step leaves one. A change stops at any session it did not record |
| Whether Claude makes a missing `Local Storage`, `Session Storage`, `IndexedDB` or `WebStorage` again | An item in neither place is gone on without, and said. The jar never is |
| Whether a session cookie's expiry slides, and how long a parked session lives | `session_expires_at` is a note in days, never a refusal |
| Whether an update, a reinstall or a keychain reset keeps `Claude Safe Storage` | Not checked: it would be a keychain call. A park from another Mac is refused by its manifest |
| How Claude's own account switcher keeps accounts on disk | Two rows of one session name for one host refuse. The recorded session under another name is doubt |
| Whether helpers outlive a crashed main program | Every program inside Claude's bundle counts as Claude open, whatever the bundle is named |
| Whether the updater's helper runs from outside the bundle between a quit and a relaunch | A program inside Claude's bundle or the data folder counts. So does one inside a `Claude.app` whose identifier does not read, and Claude's main program or a `Claude Helper` inside any bundle whose identifier does not read. One copied elsewhere does not |
| Whether the Claude Code under `claude-code/`, MCP servers or the Cowork VM outlive the main program | A program inside the data folder counts. The others do not |
| How long a quit takes, and a quit Claude's own question refuses | The core never waits for a quit. It refuses while Claude is listed |
| Whether a lock on a leveldb `LOCK` names the app, and whether inodes survive a launch | Neither is used |
| Whether Chromium tolerates a second reader of `Cookies` while it runs | The jar is opened only under `state.lock`, with Claude seen closed |
| Whether a read-only SQLite open of the jar writes anything beside it | Tested on a scratch jar on macOS: the folder's listing is the same after the read |
| Where the data folder is when `HOME` is not the passwd home | The tool is absent there |
| Whether the HTTP `Cache` can serve one account's answer to another | Not moved, not claimed |
| Whether `renamex_np(RENAME_EXCL)` is refused on a volume a home can be on | A refusal, with nothing changed. Never a rename that replaces |
| Two Pitboard homes changing one data folder at the same moment | Every change holds an advisory lock on the data folder, never waited for. The second is refused |
| Where each macOS build is published | No scheduled conformance run. `doctor` reports the build and gates nothing |
| Anything on Linux or Windows | Refused. Every register line is `NotRead` there |
| How many `in_the_way` folders a person gathers | Each is kept until `uninstall`. None holds a login |
| Whether Claude takes an advisory lock on its own data folder | Pitboard's lock is taken only once Claude is seen closed, and a lock held by anything refuses |
| What **Log out** does to other sessions of the same claude.ai account | No sentence claims it |

A person makes each hand measurement, M1 to M11, on a macOS user account made for it, which
holds no real login, with two throwaway claude.ai accounts. None is automated: no test or
script starts, quits or writes Claude. Each result goes into the register with its build
and date, or stays in the table of what was not measured, with what was seen.

| | Measures | Due |
| --- | --- | --- |
| M1 | The first-level names of the build people run, listed after Chat, the Code tab and Cowork were each used once, and again while Claude runs | Before a Claude Desktop login is moved |
| M2 | Whether `config.json.journal` is there after a quit and after a force quit, and which field each hash equals | Before a Claude Desktop login is moved |
| M3 | The cookie names and hosts, the first three bytes of each value, `meta`'s versions, and the token caches' lengths after a **Log out** | Before a Claude Desktop login is moved |
| M4 | The session fingerprint and the three keys' digests before and after ten put-backs across two days | Before a Claude Desktop login is moved |
| M5 | A sign-in over a stale name with a quit at 5, 10, 20, 40 and 70 seconds: whose name the session is under | Before a release |
| M6 | A quit with a task running: whether Claude asks, and how long each process stays listed | Before a release |
| M7 | Which processes are listed, each 200 milliseconds for 30 seconds after a quit, after a staged update, and for 35 minutes after a force quit | Before a release |
| M8 | An update, and a reinstall, between a park and its put-back: whether the account opens signed in | Before a release |
| M9 | Whether Claude makes a missing `Local Storage` again, and whether a login without its three keys opens signed in and writes them | Before a release |
| M10 | Claude's own switcher, where the account shows it: the session rows and `lastKnownAccountUuid` after an add and after a switch | Before a release |
| M11 | A switch each way with a preview signed in to a site and, under a Microsoft 365 policy, the connector signed in: whether each account finds its own, and whether Claude makes a missing `Partitions` and `Service Worker` again | Before a release |

### ureq's proxies

Read in ureq 3.4.2's `src/proxy.rs`, `src/config.rs`, `src/run.rs`, `src/middleware.rs` and
`src/unversioned/transport/`, and the socks crate 0.3.4's `src/lib.rs`, `src/v4.rs` and
`src/v5.rs`, on 7 October 2026, and measured on macOS 27.0 the same day.
`proxy.rs` reads the variables by these rules, and its tests check each row of their table
against ureq's own `Proxy::try_from_env` in a process given that row's variables.

- `Proxy::try_from_env` tries `ALL_PROXY`, `all_proxy`, `HTTPS_PROXY`, `https_proxy`,
  `HTTP_PROXY` and `http_proxy`, in that order, and takes the first whose value parses as a
  proxy's address. It does not look at the request's scheme, so `HTTP_PROXY` applies to an
  https request, and `ALL_PROXY` wins over `HTTPS_PROXY`. An empty value, or one that does
  not parse, is passed over. An address with no scheme is `http://`.
- `NoProxy::try_from_env` takes the first of `NO_PROXY` and `no_proxy` that is set, even
  empty, splits it at commas without trimming, and matches each entry against the request's
  host without regard to ASCII case: `*` matches every host, an entry starting with `*` or
  `.` matches the end of the host, one ending with `*` or `.` its start, and any other only
  the same host.
- `Config::default()`, where every agent's configuration starts, calls
  `Proxy::try_from_env`. `ConfigBuilder::proxy` replaces what it found; nothing else turns
  the read off. The `win-system-proxy` feature, which Pitboard does not turn on, adds the
  registry's proxy on Windows.
- The scheme is read without regard to case. `http` and `https` name a proxy ureq asks with
  `CONNECT`; `socks4`, `socks4a`, `socks5`, `socks5h`, and `socks`, which is `socks5`, a
  SOCKS one. Without the `socks-proxy` feature, a SOCKS proxy ureq read from the environment
  is gone around, straight to the host, with a logged warning, and one it is given panics
  on the first request, whether or not `NO_PROXY` names the host: both in
  `DefaultConnector`'s `WarnOnNoSocksConnector`.
- The feature, which brings the socks crate 0.3.4 and byteorder 1.5.0, puts its
  `SocksConnector` first in the default chain of connectors
  (`src/unversioned/transport/mod.rs`). That connector has two faults:
  - Its handshake has no time limit. `try_connect_single` runs the socks crate on a thread
    inside `thread::scope` and waits with `recv_timeout`, but the scope joins the thread
    before it returns, and the socks crate's `TcpStream::connect` and `read_exact` set no
    timeout. Measured with the feature on and a 5-second limit, against a loopback listener
    that takes each connection and never answers: a request through `socks4://`,
    `socks4a://`, `socks5://`, `socks5h://` or `socks://` was still waiting after 15
    seconds, where one through an `http://` proxy failed with `timeout: global` at 5.
  - It looks the proxy's own host up before it asks whether `NO_PROXY` exempts the
    request's host (`socks.rs`, lines 50 to 63), where the `CONNECT` connector asks first
    (`connect.rs`, lines 48 to 64). Measured with the feature on, a resolver that cannot
    find the proxy's name and `NO_PROXY` naming the host: every request to that host failed
    with `host not found`.
- The owner decided on 7 October 2026 to have both fixed in ureq rather than keep a SOCKS
  client of Pitboard's own, and that Pitboard refuses every SOCKS proxy until a ureq release
  has the fix: a handshake that gives up at the request's time limit, and `NO_PROXY` asked
  before the proxy's name is looked up. Then the feature can be turned on.
- Until then ureq is given no SOCKS proxy, and the agent's middleware, `proxy::Refusal`,
  takes each request first. A request to a host `NO_PROXY` names goes on, directly; any
  other fails with "Pitboard does not use SOCKS proxies yet, and ALL_PROXY names one", as
  `Error::Io`, before anything is sent or looked up. A middleware that returns without
  calling `next` sends nothing. ureq runs the middleware once for each request and follows
  redirects inside `run`, after it, so the agent is given `max_redirects(0)` while a SOCKS
  proxy is named: measured without it, a redirect from an exempt loopback host to a host
  `NO_PROXY` does not name went out directly and waited there for its answer.
- `run.rs` looks a request's host up here only where there is no proxy, `NO_PROXY` exempts
  the host, or the proxy's `resolve_target` is set, which `socks4`, `socks5` and `socks`
  set and `socks4a`, `socks5h`, `http` and `https` do not. With the feature, ureq's
  connector hands a proxy with `resolve_target` the first address the lookup gave, trying
  the next only where the proxy refused the TCP connection, and one without it the host's
  name and port. The socks crate fails an IPv6 address behind SOCKS4 with "SOCKS4 does not
  support IPv6" before it sends the proxy anything, so `localhost`, which this Mac looks up
  as `::1` before `127.0.0.1`, failed each request behind `socks4://`. It gives SOCKS4 an
  empty user id, whatever the address holds, and SOCKS5 the address's user name and
  password, offering no sign-in as well (`5, 2, 2, 0`).
- Measured against a loopback server with `ALL_PROXY` set and ureq, without the feature,
  reading it: `socks4://`, `socks5://` and `socks://` went around the proxy and the request
  was answered; `socks4a://` and `socks5h://` left the request with no address to connect
  to, and it failed with `Connection refused`, unless `NO_PROXY` named the host. That is
  what 0.7.0 did. With the refusal, each of the five fails before anything reaches the
  proxy or the host, in the core's tests (`proxy.rs`) and in `pitboard enroll`
  (`tests/proxy.rs`), and an exempt host is reached with nothing but its own address
  looked up.
- `Proxy::username` and `Proxy::password` are the address's own text, split at the last
  `:` before the `@`, and ureq sends them to an HTTP proxy as `Proxy-Authorization: Basic`.

### WebKit and SwiftUI

Measured on macOS 27.0 on 4 October 2026, with probe apps built by Xcode 27.1 against the
macOS 27 SDK, unless a fact gives its own date. Several were checked again with the app's
own code driving WebKit on a fixture's stand-in pages. Before changing code that depends on
one, measure it again.

- SwiftUI's `WebPage` cannot host an account window. `window.open` returns null for every
  trigger and no app callback fires, so a sign-in that opens a window never starts. A
  `target=_blank` link loads nothing unless the app loads it itself, which loses the
  opener. A download never reaches the app or the disk, and `WebPage` has no page zoom or
  print. So `Page` keeps a `WKWebView`, in `WebPage`'s shape.
- On a `WKWebView`, a `target=_blank` link reaches `decidePolicyFor` first, with no target
  frame. Cancelled there, it never reaches `createWebViewWith`; allowed, it does, with the
  same action. `window.open` goes straight to `createWebViewWith`.
- `window.open('')` reaches `createWebViewWith` with an empty address. The page in the new
  window then navigates, through its own `decidePolicyFor`. `webViewDidClose` fires on
  `window.close()`. A `target=_blank` link without `rel=opener` gives the new page no
  opener.
- WebKit quarantines a downloaded file itself, and makes the suggested name safe:
  `../../evil:name.txt` becomes `_.._evil_name.txt`. A response turned into a download also
  fails its navigation with code 102 in `WebKitErrorDomain`, which the window does not show.
  `WKDownload.originatingFrame` is macOS 15.2 and later.
- A response from an app's own scheme handler loses its headers and cannot become a
  download, so a fixture's downloads are `data:` and `blob:` links.
- `allDataStoreIdentifiers` and `remove(forIdentifier:)` each end in a segmentation fault in
  `WTF::RunLoop::dispatch` when they are a process's first WebKit call. Making any store
  first prevents it. So `WebKitDataStores` makes a non-persistent store before its first
  deletion, not at launch, and a person who never opens a window never starts WebKit.
- `remove(forIdentifier:)` reports a store in use for 20 to 60 ms after its last web view
  and store object are released, then succeeds. It reports it in use for as long as a
  `WKWebsiteDataStore` object for it is held. The janitor's retries, from 50 ms doubling to
  1600 ms, cover the first case, and the next read covers the second.
- `removeData(ofTypes:modifiedSince:)` with every type since `.distantPast`, on a store a
  page is using, clears every cookie, `HttpOnly` ones included, local and session storage,
  and IndexedDB. **Remove Website Data** rests on it to sign a window out while it stays
  open.
- A `WKWebView` answers neither `performFindPanelAction:` nor `performTextFinderAction:`.
  An `NSTextFinder` with the web view as its client and `WebContainer` as its bar's
  container shows the system find bar and steps through the matches.
- With incremental searching on, **Find Next** left the page's selection where it was, so
  it is off and a search runs on Return. A finder whose bar is already in the view, hidden,
  never shows it, so `WebContainer` adds the bar only while it shows.
  `TextEditingCommands()` adds the **Edit** > **Find** items, which send
  `performFindPanelAction:` with `NSTextFinder.Action` tags.
- Setting `pageZoom` leaves a pinch's `magnification` as it was, so **Actual Size** resets
  both.
- SwiftUI hands a URL to the `Window` scene that has `.handlesExternalEvents(matching:)`,
  through its `onOpenURL`, whether the app was running or not. It opens that window when
  none is open, reuses one that is, and opens no other window. With no scene claiming the
  URL, it went to the first window scene, the Pitboard window's, and opened that window.
- With an app delegate's `application(_:open:)` as well, the claiming scene's `onOpenURL`
  gets the URL and the delegate is then called with an empty list. With no scene claiming
  it, only the delegate gets it, and no window opens. So the **Open Link** window alone
  takes Pitboard links, and `AppDelegate` has no `application(_:open:)`.
- Those URLs were sent with `open -a` from a terminal. A cold launch that way left the app
  inactive, behind the terminal, so the **Open Link** window brings Pitboard forward when a
  link arrives. `AppPresence.comeForward` records that a link from the Share extension at a
  launch left Pitboard in the background too. A link opened from a browser was not measured.
- In CI, on macOS 26.6.2, the running app's **Open Link** window took each link too.
  `XCUIApplication.open(_:)` sent them, and it launched a second copy of the app rather
  than handing the link to the one running: the picker opened in one copy and the Pitboard
  window in the one the test watched. The copy left running kept its menu bar item, and
  with a few of those a later test's own item sat under the menus, out of reach. So the
  UI tests share a link through `XCUIDevice.shared.system.open(_:)`, which opens it with
  Launch Services, as the Share extension does, and so reaches the copy running.
- A window's root view gets `onAppear` when its window opens and `onDisappear` when it
  closes, not when it is minimised or becomes a background tab. `AppPresence` counts open
  windows by them. `openWindow(id:value:)` with the value of an open window brings that
  window back and opens no other.
- `SceneStorage`'s documentation in the macOS 27 SDK says its data is destroyed when a
  window is closed on macOS. So an account window's last page is kept in the app's
  preferences, by the window's store, and comes back in a window opened from a menu as well
  as in one macOS restores. This was read, not measured.
- The account picker's root view gets `onDisappear` before the account window it opened
  gets `onAppear`. Going back to `.accessory` in between gives the app's activation away, so
  the account window opens behind the browser; `AppPresence` waits a moment before going
  back to the menu bar.
- SwiftUI puts an item that opens each `Window` scene in the **Window** menu, unless
  `.commandsRemoved()` is applied, and does not list that scene's window there. A
  `WindowGroup`'s windows are listed while they are open.
- After `setActivationPolicy(.regular)` on an app that is already active,
  `NSWorkspace.menuBarOwningApplication` goes on naming the app before it, but the menu bar
  shows the app's own menus, seen in a screen capture on 4 October 2026 with a probe that
  has a main menu of its own. The reported owner is not what the person sees. An app made
  regular while inactive and then opened by Launch Services is reported as the owner as
  well. `NSApp.deactivate()` from an active app left it active.
- Measured on 4 October 2026 with a probe app driven by `open -g`: `NSApp.activate()` from
  an app in the background, with no click in it, is refused. Launch Services opening the
  app, `NSWorkspace.openApplication` with `activates` set, brings it forward. So
  `AppPresence.comeForward` asks Launch Services to open Pitboard when a shared link arrives
  while Pitboard is in the background.
- The macOS 27 SDK has no SwiftUI API for a Dock menu. `applicationDockMenu(_:)` on the app
  delegate is the API.
- Measured on 29 September 2026: WebKit keeps the persistent stores of an app that is not
  sandboxed under `~/Library/WebKit/<bundle id>/WebsiteDataStore/<identifier>`, cookies
  included, as ordinary files. It roots them at the Library folder Foundation gives the
  app, and Foundation does not read `HOME`. With `HOME` pointed at a scratch directory,
  `NSHomeDirectory()`, `.libraryDirectory` and `homeDirectoryForCurrentUser` all still gave
  the real home. The core reads `HOME` and `PITBOARD_HOME`, so the record of stores is kept
  per Pitboard directory.

### What the Swift app said

Measured on macOS 27.0 with Swift 6.4 on 5 October 2026, so that `pitboard-ffi`'s
`present` says what the Swift app said.

- VoiceOver heard a limit's reset as a span Foundation's `Duration.UnitsFormatStyle` said:
  days, hours and minutes, wide, at most two units, in `en_US_POSIX`, of at least a minute.
  Asked for 100 spans, it rounds the span to whole minutes, a half to the even one, to see
  which units it has, and says the two largest that are not nought, so 1 day and 1 minute
  is "1 day, 1 minute". The last unit it says is the rest of the span in that unit, rounded
  the same way, and a rest that rounds up to a whole one of the unit before it is carried:
  1 day 23 hours 30 minutes is "2 days, 0 hours". It writes no thousands separator: 11,574
  days is "11574 days, 2 hours". `words::spoken_span` is tested against every one.
- The menu bar cut a label past 12 `Character`s to 11 and an ellipsis, and Swift counts
  extended grapheme clusters. Over 22 labels, accents written as combining marks, flags,
  emoji joined by zero-width joiners, keycaps, tag sequences, Hangul written in jamo, Thai,
  Arabic's prepended number sign, a carriage return before a line feed and Devanagari
  conjuncts, joined and not, `unicode-segmentation` 1.13.3's extended graphemes counted and
  cut each as Swift did.

### Foundation's URLs

Measured on macOS 27.0 on 5 October 2026, by giving the Swift `SiteLink` and `Handoff` of
0.7.0, and `URLComponents(string:)`, the same text as `pitboard-sites`. `pitboard-sites`
reads a link as Foundation does, so a link from outside means what it meant to the macOS
app.

- `URLComponents(string:)` splits a link by RFC 3986. A path, query or fragment holding a
  character RFC 3986 does not allow there is kept with every such character
  percent-encoded, a `%` among them, so an escape already in it is encoded too: `/%41 x` is
  kept as `/%2541%20x`. One holding none is kept as written, escapes and all. A second `#`
  is `%23` in the fragment.
- A host is never encoded: one with a character RFC 3986 does not allow, or a `%` that
  starts no escape, makes no link. A host in plain ASCII is percent-decoded and kept in its
  case, `claude.ai%00` included. One that is not, or that has an `xn--` label, goes through
  ICU's IDNA: `ｃｌａｕｄｅ.ai` is `claude.ai`, `xn--bcher-kva.de` reads back as `bücher.de`,
  and a joiner in a label makes no link. Anything between brackets is an IP literal.
- `user` and `password` are nil where their bytes are not UTF-8, `port` is nil where an
  `Int` cannot hold the number, and `path` is empty where its bytes are not UTF-8. So the
  Swift `SiteLink` opened `https://%FF@claude.ai/` and `https://claude.ai:99999999999999999999/`
  without what it dropped, took `claude.ai/magic-link/%FF` for no sign-in link, and opened
  `claude.ai/x/../%FF` without seeing its dot segment. The Swift `Handoff` read
  `pitboard://open/%FF?url=claude.ai` as `pitboard://open?url=claude.ai`. `pitboard-sites`
  refuses each.
- Swift compares strings by grapheme cluster, so a combining mark or a joiner right after a
  `/` makes one character with it. The Swift `SiteLink` took `claude.ai/magic-link/%CC%81`
  for no sign-in link, and `chat.com/` followed by a combining mark for no link at all. It
  counted a link's length in clusters too, and opened one of 4106 clusters and 8194
  scalars. `pitboard-sites` compares the text, and counts a link's length in Unicode
  scalars.
- `CharacterSet.whitespacesAndNewlines`, which trims a link, is Unicode's `White_Space` and
  U+200B ZERO WIDTH SPACE, checked over every scalar.
- The WHATWG URL standard, which the `url` crate and WebKit follow, reads a link otherwise.
  It resolves dot segments, takes `\` for `/` in an `https` link, drops a default port such
  as `:443`, finds a host in `https:claude.ai`, strips a tab or a newline and leaves a second
  `#` as it is. It reads a host whose last label is a number as an IPv4 address, and decodes
  a host's escapes before IDNA. So `pitboard-sites` asks `url` for IDNA alone, with a label
  after the host so that no host is read as an address.
- 450,000 generated links and Pitboard links, ASCII and not, had the same answer from both
  apart from: Pitboard links that `URL(string:)` refuses, which never reach the macOS app,
  though the Windows app, reading a Pitboard link from its command line as text, can be
  given one; the things above that `pitboard-sites` refuses; grapheme clusters; and hosts
  that are not ASCII, where ICU's and WHATWG's IDNA differ over empty labels, escapes and
  what a label may hold. Each such host was refused by both, though one named it otherwise,
  apart from `ｃ%EF%BD%8Caude.ai`: `pitboard-sites` decodes its escapes before IDNA, as the
  WHATWG standard does, and takes it for `claude.ai`, where the Swift `SiteLink` said it was
  on `cｌaude.ai`.
- `URL(string:)` reads a host otherwise than `URLComponents`, and an account window's rules
  compared the `URL` WebKit handed them, so `pitboard-sites`' `WebAddress` reads one as
  `URL.host` does, measured on 5 October 2026. A host in ASCII is percent-decoded and kept
  in its case, `xn--bcher-kva.de` and `XN--BCHER-KVA.de` as written, and so is one with an
  `xn--` label IDNA refuses, `xn--claude-.ai`, for which `URLComponents.host` is nil. A
  host that is not ASCII is given in IDNA's ASCII form, `аpple.com` with a Cyrillic а as
  `xn--pple-43d.com`, and one IDNA refuses makes no `URL`. An IP literal is given without
  its brackets. `https:///x`, `https:claude.ai/x`, `blob:` and `data:` links name no host.
  `port` is `0` for `:0`, and nil for `claude.ai:`; `user` is empty, not nil, for
  `https://@claude.ai/`. `WebAddress` reads two things otherwise: it keeps a port no `Int`
  holds, and names no host in `//claude.ai/x`, where `URL` reads `claude.ai` on no scheme.
- `URL.path` is the path percent-decoded, then without the slashes it ends in, but for the
  one a path of nothing else keeps: `/c/shared/` is `/c/shared`, `/a%20b/%2F` is `/a b`, `//`
  is `/`, and no path at all is empty. A path whose bytes are not UTF-8 is empty, and
  neither the query nor the fragment is part of it. Measured with Swift 6.4 on macOS 27.0
  on 6 October 2026, on eleven addresses on `pitboard-fixture://`, which
  `WebAddress::path` reads the same.
- `NSString` splits a file name into a base and an extension at its last `.`, and finds no
  extension where it would be empty or hold a space, or where the base would be empty, `.`
  or `..`: `....a` has the extension `a`, `...a` none. `lastPathComponent` drops the
  slashes at the end, and is `/` for slashes alone. Measured the same day on 98 names, with
  the Swift `DownloadCenter` naming 52 of them as a download, free and with its own name
  taken.
- A file `URL`'s `path` is decomposed: `URL(fileURLWithPath:)` and `appendingPathComponent`
  give `Báo cáo.pdf` as `Ba\u{301}o ca\u{301}o.pdf`. So the Swift `DownloadCenter`, which
  compared `URL`s, numbered a download whose name a running one had in either form, though
  not `\u{F900}.pdf` against `\u{8C48}.pdf`, its canonical decomposition, nor two names
  that differ only in case. APFS, on this Mac's volume that is not case-sensitive, takes
  each pair for one file: one made under either name is found under the other. Measured the
  same day. `download_destination` compares the names it reserved decomposed, so it numbers
  the U+F900 pair too, and takes two that differ only in case for two, as the Swift did.
- `CharacterSet.whitespaces`, which trims a `Content-Disposition`, is a tab, Unicode's
  `Zs` and U+200B ZERO WIDTH SPACE, checked over every scalar. It holds no newline.

### claude.ai and chatgpt.com

Read and measured on 29 September 2026. Nobody has watched a sign-in complete in a window.
That needs a person, in a debug build with a scratch home.

- `chat.openai.com` answers 308, `www.chatgpt.com` 301 and `chat.com` 307, each to the same
  path and query on `chatgpt.com`, measured with `curl`. So `Site.chatGPT` takes them as
  aliases.
- OpenAI's help centre, article 7426629, lists signing in to ChatGPT with a password or with
  Google, Microsoft or Apple, and names `auth.openai.com` among the hosts its sign-in needs.
- Third-party code that drives the sign-in, and the buttons' connection names, put the
  Microsoft and Apple sign-ins on `login.live.com`, `login.microsoftonline.com` and
  `appleid.apple.com`. The same reading has the sign-in come back through
  `chatgpt.com/api/auth`. How chatgpt.com's Microsoft and Apple buttons open their sign-in,
  in a new window, by a link or by a redirect, is not known.
- Not measured: whether email sign-in completes in a window, whether a window's session
  survives a restart, and whether Cloudflare's challenges pass WebKit's own user agent.
- The Share menu lists app extensions in Safari's toolbar and **File** menu, and in
  **File** > **Share** in Chrome and in Firefox from 92. That was read from the browsers'
  source and bug trackers, and no browser was run. Which other browsers list the extension
  is not known.

### The C# bindings

Read in the C# that uniffi-bindgen-cs v0.11.0+v0.31.0 generated from `pitboard-ffi`, and
measured by `apps/windows/Pitboard.Core.Tests` against the debug library, on 5 October 2026.
Read again on 15 October 2026 in what v0.11.1+v0.31.0 generates, which differs only in how a
string crosses: it is read and written in the Rust buffer's own memory, with no `byte[]`
between.

- An exported object's class frees its Rust object from a finalizer: `~PitboardModel()`
  calls `Destroy()`. So .NET's finalizer thread can be the one that drops a
  `PitboardModel`, and its `Drop` waits on nothing.
- The first call loads the library, and before it answers, the bindings compare the
  checksum of every export, the model's constructor and methods and
  `ModelListener.changed` among them, and hand the library each foreign trait's table of
  calls.
- A trait an app implements is an interface of the trait's own name, `ModelListener`,
  `AppControl`, `Notifications` and `LocalTime`. Its error is an exception named for it,
  `PlatformException`, whose variant the app throws. An `Option<String>` it gives back is a
  `string?`.
- Each call the bindings write for a trait catches a `PlatformException` and hands Rust its
  variant, and catches any other exception and hands Rust its message, which
  `From<UnexpectedUniFFICallbackError>` makes `PlatformError::Failed`. So nothing an app's
  code throws goes past Rust. Read in the generated `AppControl` calls.
  `ModelTests.APlatformExceptionFromTheAppsAppControlStopsTheSwitch` measured the first on
  13 October 2026, in CI on Linux with the .NET SDK 10.0.401 and the release library built
  with the `fixture` feature: `RequestQuit` throwing `PlatformException.Failed` stopped the
  switch as an error from `request_quit` does, and `Running` was not asked again.
- A record holds a list as an array, and a C# record compares arrays by reference, so two
  snapshots read alike are not equal. `ModelTests.ASnapshotCarriesWhatWasRead` measures it.
  Two records C# makes with an empty collection expression, `[]`, for each list do compare
  equal: the compiler gives them one empty array, so an app's own test of that kind proves
  nothing about lists. `ModelTests.ASnapshotSaysWhatIsKnownOfTheMachine` measures it, and
  measured it with the .NET SDK 10.0.401 on 6 October 2026.
- A call's checksum is taken over what UniFFI records of it: its module, object and name,
  its arguments, the types it takes, gives and throws, and its doc comment. A record type is
  recorded by its module and name alone, so no checksum covers a record's fields. Read in
  `uniffi_macros` 0.31.2, `fnsig.rs` and `record.rs`.

On Windows, CI's `csharp-windows` job measured on windows-2025 and windows-11-arm, in run
38683164949 on 15 October 2026, where both passed:

- The C# generated from the release `pitboard_ffi.dll`, built as a 0.x release is and
  opened with its fixtures, is the Linux job's file byte for byte on both images. The DLL
  keeps through the release profile's strip the exports the generator reads, where Linux's
  shared library does not.
- A test program published self-contained for the image's machine loads either DLL as a
  fresh standard user, and its calls answer. A model over the gated DLL reads
  `windows_not_released` and leaves each home it was given empty.

### The Swift bindings

Measured with `build-xcframework.sh` and `swift test --package-path apps/macos`, with the
Swift UniFFI 0.31.2 generates and the Swift 6.4 of Xcode 27.1, on 5 October 2026.

- A method of a trait an app implements may not have a name on the generator's list of
  Swift keywords, which has `open` though Swift takes `open` as a method's name, as
  `PitboardApp`'s own `AppControl.open` was. The generator puts such a name in backticks, and
  writes the C header's table of calls through the same filter, so the header has
  `` UniffiCallbackInterfaceAppControlMethod2 _Nonnull `open`; ``, which clang refuses
  ("expected member name or ';' after declaration specifiers"), and the bindings do not
  build. Read in `uniffi_bindgen` 0.31.2: the list in `bindings/swift/gen_swift/mod.rs`, and
  `BridgingHeaderTemplate.h` naming each field through `var_name`. `AppControl`'s method that
  opens an app again is `reopen` for that reason.
- A type the bindings export that `PitboardApp` also declares is the app's own inside
  `PitboardApp`, and ambiguous in a module that imports both, as the app's tests do: with
  the bindings' `AppControl` beside the app's protocol of that name, `swift test` stopped at
  "'AppControl' is ambiguous for type lookup in this context" in `Fixtures.swift`. The app
  has implemented the bindings' trait, as `MacAppControl`, since it runs on the model, and
  declares no `AppControl` of its own.
- A type the bindings export is declared in the same Swift package as the app, so making
  one conform to a protocol, as `Sheet` to `Identifiable` for a sheet's binding, takes no
  `@retroactive`: Swift 6.4 refuses it there, "'retroactive' attribute does not apply;
  'Sheet' is declared in the same package". Measured on 6 October 2026.
- `MainActor.assumeIsolated` traps off the main thread, so `ModelListening`'s hop is
  tested by where it runs: with the snapshot queued on a global queue in place of the main
  one, the test of the listener's order stopped with signal 5 rather than failing an
  expectation. Measured on 6 October 2026.

### What the macOS app's glue rests on

Measured on macOS 27.0 with Swift 6.4 of Xcode 27.1, on 6 October 2026, unless a fact gives
its own date.

- Foundation's temporary directory does not read `TMPDIR`: with `TMPDIR=/private/tmp/xyz/`
  and with it unset, `NSTemporaryDirectory()` and `FileManager.default.temporaryDirectory`
  both gave the user's own `/var/folders/…/T/`. Rust's `std::env::temp_dir` reads `TMPDIR`
  first, and only then asks the system for that directory (Rust 1.98.1's
  `library/std/src/sys/paths/unix.rs`). So the app finds a fixture's folder by `TMPDIR`
  where it is set, as the core makes it, and by Foundation where it is not.
- `@Observable` does not tell an observer of an assignment of a value equal to the one held:
  over `withObservationTracking`, setting an equal `Equatable` struct, an equal `Int` or the
  same object again told nothing, and setting another struct told. So `AppModel`'s
  comparison before each assignment changes nothing that Observation does not already skip,
  and a test of what is drawn again passed with the comparison taken away. The comparison is
  kept so the rule does not rest on how the macro expands, and `apply` returns the parts it
  assigned, which its test reads: with the comparison taken away, that test fails.
- `Date.FormatStyle` with `time: .shortened` follows the locale's hour cycle, which macOS
  keeps in the locale: 14:05 in UTC is `2:05 PM` for `en_US`, with U+202F NARROW NO-BREAK
  SPACE before `PM`, `14:05` for `en_GB` and for `en_US@hours=h23`, and `2:05 pm` for
  `en_GB@hours=h12`. `MacLocalTime` formats with the person's own locale, and its test with
  `en_US` and `en_US@hours=h23`.
- Read on 12 October 2026: `NSWindow.h` in the macOS 27.0 SDK says a window that has a
  sheet queues another until the first is dismissed. So an alert attached to the window
  waits while a sheet is up, and one attached to the sheet's own view is a sheet of the
  sheet's window and shows over it. The window's failure alert and quit question are
  attached through `WindowAlerts` to whichever is in front.
  `testAddingAClaudeDesktopAccountAsksToQuitClaudeOverTheSheet` holds it.

### What the Windows app's glue rests on

Read on 15 October 2026, in the sources of the builds the app is made with and in
Microsoft's documentation.

- `Environment.GetFolderPath(SpecialFolder.LocalApplicationData)` asks
  `SHGetKnownFolderPath(FOLDERID_LocalAppData)`, and only where that fails, as on Nano
  Server, returns `LOCALAPPDATA` (dotnet/runtime v10.0.13,
  `System.Private.CoreLib/src/System/Environment.Win32.cs`). So `Shell.LocalAppData` asks
  `SHGetKnownFolderPath` itself, and no variable moves the app's data folder.
  `ShellTests` holds it on Windows.
- An unpackaged process's `AppInstance.FindOrRegisterForKey` holds a key as the named mutex
  `<module>_<key>_Mutex`, with every backslash in the key made an underscore, where
  `<module>` is `App` and a hash of the lowercased path of the process's executable (Windows
  App SDK v2.5.1, commit 13160d54, `dev/AppLifecycle/AppInstance.cpp` `TrySetKey` and
  `Association.cpp` `ComputeAppId`). So a key is shared only by copies of one executable: a
  Debug build and a Release in other folders never meet, and the fixture's folder in a Debug
  build's key keeps two fixtures of one build apart. `RedirectActivationToAsync` queues the
  launch for the running copy from a background thread and calls `AllowSetForegroundWindow`
  for it, so the running copy may bring its window forward. The running copy raises
  `Activated` for it once, on a thread pool thread, to the handlers connected then, and only
  then lets the second copy quit (`ProcessRedirectionRequests`). So `Program` connects its
  handler before it takes the key, and `Handovers` keeps a launch that arrives before the
  window can open: connected after, a launch handed over while the first copy started was lost
  without a word from either copy (run 38789385677). A second copy hands its launch over off
  the STA thread, as Microsoft's [App instancing](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/applifecycle/applifecycle-instancing)
  does it, to a copy that runs and to one that is starting, on both images (`LifecycleTests`,
  run 38795330950).
- `FindOrRegisterForKey` returns the `AppInstance` that `GetCurrent` does, so the handler
  `Program` connects before it takes the key hears each launch handed to that key. An
  unpackaged launch's arguments are its whole command line, as `GetCommandLine` has it, its
  program first, which the SDK splits with `CommandLineToArgvW` to find its own (commit
  13160d54 again, read on 16 October 2026: `AppInstance.cpp` `FindOrRegisterForKey` and
  `GetActivatedEventArgs`, and `LaunchActivatedEventArgs.h`). So `Handovers` keeps each
  launch with its link, and `Launching.Handed` splits the command line as the SDK does.
- Under `DispatcherShutdownMode.OnExplicitShutdown`, closing the main window leaves
  Pitboard.exe running ([DispatcherShutdownMode](https://learn.microsoft.com/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.application.dispatchershutdownmode)),
  on both images (`LifecycleTests.ClosingTheWindowLeavesPitboardRunning`, run 38804714545).
- Microsoft's NavigationApp template (WindowsAppSDK main,
  `dev/Templates/Source/ProjectTemplates/Desktop/CSharp/NavigationApp`, read on 16 October
  2026) puts a `TitleBar` with the app's icon and the pane toggle above a `NavigationView`
  under `MicaBackdrop`, extends the content into the title bar, asks for the tall one and calls
  `AppWindow.SetIcon`. `MainWindow` does the same, so Mica reaches the title bar, and its
  icon names the window in the title bar and Alt+Tab (`LifecycleTests.TheWindowCarriesPitboardsIcon`).
  The PRI file indexes `Pitboard.ico` as `Files/Pitboard.ico` (run 38774669459), which
  `ms-appx:///Pitboard.ico` reads. The template's pages own their titles, in
  `TitleTextBlockStyle` at the top of a Grid padded 24,16,24,16 with 24 between its rows, and
  the NavigationView page (updated 2 October 2026) recommends 12 epx margins for the content
  in Minimal and 24 otherwise, and navigates its frame only to a page not shown already. So
  `MainWindow` sets no `Header`: each page carries its title as a Level1 heading, its content
  24 epx under it and the sides the window sets from `DisplayModeChanged`, whose thicknesses
  `Styles.xaml` holds, and the lists and the settings' scroll viewer reach the window's edges
  and pad what they hold, which keeps a scroll bar at the edge. Its pages are kept
  (`NavigationCacheMode.Required`), with no back stack (`MainWindowTests`).
- `AppWindowTitleBar.PreferredTheme`'s reference gives `Legacy` as its default, and WinUI's
  window class carries no icon. So `WindowChrome` gives every window of Pitboard's
  `UseDefaultAppMode` and the app's icon (`WindowChromeTests`,
  `LifecycleTests.TheAccountAndOpenLinkWindowsCarryPitboardsIcon`).
  The WinUI Gallery's `TitleBarHelper` (`WinUIGallery/Helpers/TitleBarHelper.cs`, read on 17
  October 2026) sets the caption buttons' glyph and hover colours from the content's
  `ActualTheme`, on `Loaded` and `ActualThemeChanged`, because `AppWindowTitleBar` does not
  change them when the mode changes while the app runs, and WinUI's own TableView sample says
  the system draws them black whatever the content's theme. So `WindowChrome` does the same
  for a window whose content extends into its title bar, the main window
  (`WindowChromeTests.TheMainWindowsCaptionButtonsFollowItsTheme`). Microsoft's title bar
  guidance (updated 14 July 2026) has a title bar's colours follow a contrast theme, and the
  Windows App SDK's `ThemeSettings` (`Microsoft.InteractiveExperiences.Projection.xml`,
  InteractiveExperiences 2.1.9 in Windows App SDK 2.5.1) tells a Win32 app whether one is on
  and when that changes. So under a contrast theme `WindowChrome` sets none of the caption's
  colours, which the system then draws, and follows `ThemeSettings.Changed`
  (`WindowChromeTests.UnderAContrastThemeTheCaptionButtonsTakeWindowsOwnColours`). How they
  look under one is measured by hand.
- Microsoft's Manage app windows page (updated 14 October 2026) gives a window that belongs
  to another an owner window, set through Win32 interop, and the WinUI Gallery's
  `ModalWindow.xaml.cs` sets it with `SetWindowLongPtr(GWLP_HWNDPARENT)`. An owned window
  stays above its owner, is hidden while its owner is minimized and is destroyed with it
  ([Window features](https://learn.microsoft.com/windows/win32/winmsg/window-features#owned-windows),
  updated 21 February 2026). So a sign-in window is owned
  by its account window, and not modal: the account window's page stays usable behind it
  (`AccountWindowTests.ASignInWindowIsOwnedByItsAccountWindow`). The account and Open link
  windows are under `MicaBackdrop`, as the main window is, with the system title bar the
  Gallery's `ModalWindow` keeps.
- Microsoft's window management guidance (updated 11 March 2025) saves a window's monitor,
  size, place and state as it closes and restores them as it opens, fits a window whose last
  place no screen holds within one, cascades a new application window from the upper left,
  and has a resizable window keep a usable minimum. `SetWindowPlacement`'s reference (updated
  1 July 2025) moves a window its bounds would put wholly off screen onto one. So
  `WindowPlacement` keeps `GetWindowPlacement`'s restored bounds and whether the window was
  maximized, a minimized one by the state it comes back to, in `placement.json` in the app's
  data folder, and `MainWindow` puts them back with `SetWindowPlacement` before its first
  activation, which shows a window already maximized maximized (microsoft-ui-xaml at
  `winui3/release/2.5.1`, `DesktopWindowImpl.cpp` `ActivateImpl`). With none saved it opens
  at Windows' own place, 1100 by 720 epx, which `WindowChrome` fits within its screen's work
  area. A saved place is put back as it was, even partly under the taskbar, as the person left
  it. `WindowChrome` keeps the window at 640 by 500 epx at least at its screen's scale, as the
  WinUI Gallery's main window does. Exit closes the window first, so it is kept then too
  (`WindowPlacementTests`, the `LifecycleTests` of placement).
- Microsoft's keyboard accelerator page names F6 and Shift+F6 for the next and previous pane
  and Ctrl+W for Close, and its keyboard accessibility page has the app implement F6 itself,
  Shift+F6 the same cycle reversed. So F6 and Shift+F6 move between the main window's two
  regions, its navigation pane and its page, and Ctrl+W closes it as its caption's close does
  (`LifecycleTests.F6MovesFocusBetweenThePaneAndThePage`, `CtrlWClosesTheWindowAndPitboardKeepsRunning`).
- WinUI's WebView2 (microsoft-ui-xaml main at 1af995b, `controls/dev/WebView2/WebView2.cpp`)
  handles no `AcceleratorKeyPressed`, so no XAML accelerator hears a key pressed in the page,
  where WebView2's own browser keys serve Back, Forward and Reload. So an account window takes
  Ctrl+W, and the browser keys for its Back, Forward and Reload, on its root, for its toolbar
  and its note (`AccountWindowTests.TheWindowTakesTheKeysOfItsCommands`,
  `TheToolbarTakesBackAndCtrlW`). An accelerator for Alt+Left on the root missed the key with
  the command bar's More focused on windows-2025 (run 39310757537), though
  `CommandBar_Partial.cpp` at `winui3/release/2.5.1` moves focus on Left or Right only with
  Alt up, so Alt+Left and Alt+Right are read in the root's `PreviewKeyDown`, Alt from the
  thread's key state.
- Microsoft's User Interface Text guidelines (updated 15 April 2025) title a top-level
  document-centric window "document name - program name", the document first. So an account
  window is titled with its page's `DocumentTitle`, a hyphen and the account's title, where
  the macOS window shows the page as its subtitle
  (`AccountWindowTests.ItsTitleIsItsPagesAndThenItsAccounts`). An account window's list of
  downloads is WebView2's own, with no Clear of Pitboard's, so `OpenWindow.clear_downloads`
  is `None` on Windows (`model::windowing`'s
  `a_window_on_windows_offers_no_clear_of_its_downloads`).
- The Windows App SDK's `AppWindow` reference (`Microsoft.UI.xml` in InteractiveExperiences
  2.1.9, Windows App SDK 2.5.1, and Microsoft Learn, updated 28 July 2026) raises `Closing` for
  a close through a system affordance, which `Cancel` stops, and not for `Destroy`. `Hide`
  keeps the window alive and `Show` brings it back, and `Window.Close` destroys the window
  (updated 31 July 2026). So an account window that keeps its page for a download hears the
  caption's close and Alt+F4 in `Closing`, puts Ctrl+W through the same check, and closes
  itself with `Close` as its last download ends. The tests close it with `WM_SYSCOMMAND`
  `SC_CLOSE`, as Alt+F4 does.
- Microsoft's ResourceDictionary page (updated 13 October 2026) has `XamlReader.Load` parse in
  a context that knows no other ResourceDictionary, not even `Application.Resources`, and take
  no ThemeResource, and has `XamlControlsResources` merged first. So the views' shared styles
  are `Styles.xaml`, which `App.xaml` and the test app merge after it
  (`StylesTests.EachSharedStyleIsFoundBeforeAnyWindowShows`).
- Rust's `std::env::temp_dir` (Rust 1.98.1, `library/std/src/sys/paths/windows.rs`) and
  .NET's `Path.GetTempPath` (dotnet/runtime v10.0.13, `Path.Windows.cs`) both ask
  `GetTempPath2W`, .NET falling back to `GetTempPathW` where it is missing. So the app finds
  a fixture's folder, `pitboard-fixture`, where the core makes it.
- One ContentDialog shows per thread at a time, and opening a second throws, even in another
  window ([ContentDialog](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.contentdialog),
  updated 28 July 2026). So `DialogHost` queues the main window's dialogs, and while a sheet
  is up `WindowAlerts` leaves the failure alert and the quit question to the sheet's
  `AlertBar`, an InfoBar, where the macOS app shows them over the sheet.
  `DialogHostTests.OnlyOneContentDialogShowsAtATime` holds it on the runners.
- The WinUI Gallery's `ContentDialogPage` gives each dialog it makes its page's
  `ActualTheme`, and an Add account sheet opened after a switch to dark while Pitboard ran
  drew light under white text, where one opened in dark was right. So `DialogHost` gives every
  dialog its window content's theme and follows `ActualThemeChanged` while it is up
  (`DialogHostTests.ADialogTakesItsWindowsThemeAndFollowsIt`).
- An exception that ends a `DispatcherQueue` callback fails fast in `CoreMessagingXP.dll` and
  reaches no handler. So `Ui.Guarded` wraps each callback the app hands `TryEnqueue`, and
  writes what ended it, with the `RestrictedDescription` C#/WinRT keeps WinUI's words in,
  before throwing it on (`UiTests`).
- A sheet is a ContentDialog, which Microsoft's guidance gives for a question that asks for
  input, with its "do it" actions as the primary and secondary buttons and a safe close
  button that Esc presses ([Dialog controls](https://learn.microsoft.com/windows/apps/design/controls/dialogs-and-flyouts/dialogs),
  updated 15 July 2026). The tool is a RadioButtons group starting on the tool the sheet is
  about, as the combo box guidance gives fewer than five items to pick one of as radio
  buttons ([Combo box and list box](https://learn.microsoft.com/windows/apps/develop/ui/controls/combo-box),
  updated 21 September 2026). The account signed in to again is selectable text under a
  label, and the code field checks no spelling and predicts no text, as the text box guidance
  gives text never edited to a TextBlock and names `IsSpellCheckEnabled` and
  `IsTextPredictionEnabled` ([Text box](https://learn.microsoft.com/windows/apps/develop/ui/controls/text-box),
  updated 17 July 2026). A sign-in waits under an indeterminate ProgressRing, the control that
  the progress guidance names for "Signing in..."
  ([Progress controls](https://learn.microsoft.com/windows/apps/design/controls/progress-controls),
  updated 8 July 2026).
- WinUI 2.3.9's `Themes/generic.xaml` sizes a ContentDialog's `BackgroundElement` between
  `ContentDialogMinWidth` (320) and `ContentDialogMaxWidth` (548), so the Add account sheet
  was as wide as its widest line and changed width with the tool picked. `AccountSheet` sets
  its own `ContentDialogMinWidth` to the maximum, the lightweight styling Microsoft's XAML
  styles page describes, and keeps one width
  (`AccountSheetTests.TheSheetKeepsOneWidthWhateverItShows`).
- The same `generic.xaml` shows a TextBox's `Description` in `DescriptionPresenter`, a
  ContentPresenter with no `TextWrapping`, so a note given as a string stays on one line: the
  code field's ran past the sheet's edge. `AccountSheet` gives each field's note as a
  TextBlock that wraps (`AccountSheetTests.ACodeNoteLongerThanItsFieldWrapsUnderIt`).
- The code WinUI 2.3.9's XAML compiler writes for a OneWay `x:Bind` function calls it again,
  as a path in its argument changes, only for a value that is not null (`AccountRow.g.cs`,
  `Update_Account_Item_Plan`). So a row the list gave an account without a plan kept the tag
  of the account before, and a function there takes the account item, which is never null
  (`AccountRowTests.ARowGivenAnAccountWithoutAPlanHasNoTag`). The same compiler
  stops on WMC9999 where a template's `x:Bind` names both a member of its `x:DataType` and an
  element its page names: it goes to warn and finds no resources for the warning
  (`BindPathListener.ResolveNameOnRoot` in microsoft-ui-xaml's main, read on 16 October 2026).
  So each page names its title row `PageHeader`, which no template binds.
- A Selector's `IsSynchronizedWithCurrentItem` is null by default, which keeps its selection on
  a collection view's current item ([Selector.IsSynchronizedWithCurrentItem](https://learn.microsoft.com/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.primitives.selector.issynchronizedwithcurrentitem)).
  The Accounts page groups its rows through a CollectionViewSource, so twoTools opened with a
  row selected that nobody chose. Its list sets it false
  (`AccountsPageTests.NoRowIsSelectedUntilOneIsChosen`).
- Read in microsoft-ui-xaml at `winui3/release/2.5.1`, the WinUI of Windows App SDK 2.5.1.
  `ContentDialog_Partial.cpp`: Esc clicks the close button where it is enabled and
  otherwise hides the dialog through a `Closing` it may cancel; Enter clicks the default
  button where it is enabled; the first focusable element of the content takes focus as the
  dialog opens; a dialog made in code, outside the live tree, is put whole in its popup.
  `KeyboardInputProcessor.cpp` raises `PreviewKeyDown` and `KeyDown` with one event
  argument, so a `PreviewKeyDown` handled on the dialog keeps Esc from its close button.
  `InfoBar.cpp` announces an InfoBar as it opens, by its severity, title and message, only
  through an automation peer it has already. So `AccountSheet` cancels each button's click
  and leaves closing to the model, dims its close button through `CloseButtonStyle` while
  nothing may close it, keeps Esc from a sign-in that waits on its app, and keeps its
  failure's InfoBar in the tree while it is closed. `AccountSheetTests` holds what runs
  without a person on the runners.
- Microsoft's InfoBar page raises no notification for a bar changed while open, and has it
  closed and opened again for a screen reader to hear the new content
  ([InfoBar](https://learn.microsoft.com/windows/apps/develop/ui/controls/infobar), updated 25
  June 2026). So the sheet's failure bar closes and opens again for each new failure, an
  account window's note bar for a note in place of another, as the macOS window announces
  each (`AccountWindowTests.ANoteInPlaceOfAnotherOpensItsBarAgain`), and a
  code the tool refused is an error InfoBar under the code field, where plain text was. A new
  failure gives focus back to the field it is about, as the sheet's opening does, so the
  default button is the one Enter presses.
- WinUI 2.3.9's `Themes/generic.xaml`, in the Windows App SDK 2.5.1 package: the InfoBar's
  close button is the template part `CloseButton`, which `NoticeBarTests` presses, and a
  `ListViewItem` is padded `16,0,12,0`, which only the Activity page's column headers repeat,
  so they line up with the rows under them.
- `Shell_NotifyIconW` and `NOTIFYICONDATAW` (Microsoft Learn, updated 1 July 2025 and 20
  November 2024): `NIM_SETVERSION` follows every `NIM_ADD`; under `NOTIFYICON_VERSION_4` the
  event is `lParam`'s low word and the point `wParam`'s two words, the icon's upper-left
  corner when the keyboard sent it; the standard tooltip shows only with `NIF_SHOWTIP`;
  `szTip` holds 128 UTF-16 units with its null; and a GUID is tied to the path of the exe
  that added it. Microsoft's NotificationIcon sample in Windows-classic-samples reads
  `WM_CONTEXTMENU`'s point from `wParam`, makes its window the foreground window before its
  menu, and asks for Common Controls 6 to call `LoadIconMetric`. So `NotifyIcon` cuts the
  tooltip to 127 units, names its icon by a GUID in the Release build alone and falls back
  to its window when Explorer refuses the GUID, and `IconEvent` reads the point as signed.
  `NotifyIconTests` and `IconEventTests` hold them, and `HiddenWindowTests` the icon's
  return on `TaskbarCreated`.
- The notification area guidance (updated 15 April 2025) has a right click bring up a normal
  shortcut menu and a left click show what the person expects, the program window among
  them, and prefers the standard tooltip. Its Windows 7 page (updated 11 March 2025) gives
  the icon's menu no About command where the program has a window. The Windows 7 menu
  guidance (updated 26 July 2026) names Exit, puts no ellipsis on Help or Settings, shows a
  context menu's default command in bold and no shortcut keys, and writes labels in
  sentence-style capitalization, keeping title style for a legacy program that would
  otherwise mix styles. So the icon's menu says Exit, bolds Open Pitboard, shows no shortcuts
  and leaves About to the settings page, and the Windows app writes all its text in sentence
  case: its own, and the model's buttons, menu items and headings, which `words::titled`
  words for each system, title case on macOS as Apple's guidelines have it.
- WinUI 2.3.9's `Themes/generic.xaml` draws a `RadioMenuFlyoutItem`'s text in one
  TextBlock that trims, where the macOS menu puts an account's limits on a line under its
  name. So on Windows `AccountItem.menu_title` ends with the figure the menu bar would show
  of the account, `headline`'s, where that line says its limits, and the line is the item's
  tooltip and help text (`present::accounts`'
  `a_windows_menu_item_ends_with_the_figure_the_bar_would_show`).
- Read in microsoft-ui-xaml at `winui3/release/2.5.1`: a light-dismiss popup hides its
  flyout when its window is deactivated (`Popup_Partial.cpp`), so `MenuHost` is made the
  foreground window before the menu opens; a `MenuFlyoutItem` raises `Click` before it closes
  its menu (`MenuFlyoutItem_Partial.cpp`), so a choice runs once the menu has closed; a
  `RadioMenuFlyoutItem` is a `ToggleMenuFlyoutItem` inside that
  undoes an uncheck by choosing (`RadioMenuFlyoutItem.cpp`); and NavigationView names its
  settings item `SettingsItem` (`NavigationView.cpp`), which the UI tests select.
  `OverlappedPresenter.CreateForContextMenu` keeps a border (Microsoft Learn, updated 28 July
  2026), which `MenuHost` turns off.
- A `MenuFlyoutItem` goes to its `NarrowPadding` state only when a mouse, pen or keyboard
  opened its menu (`MenuFlyoutItemBase_Partial.cpp` at `winui3/release/2.5.1`), and WinUI
  2.3.9's `Themes/generic.xaml` pads it `11,8,11,9` otherwise and `11,4,11,5` then.
  `WM_CONTEXTMENU` from the notification area names no device, so the icon's rows were
  taller in one opening than in another. `IconMenuFlyout` gives each row
  `MenuFlyoutItemThemePaddingNarrow`, which the template binds the row's padding to outside
  that state, so every opening draws the rows a mouse would
  (`IconMenuFlyoutTests.EachRowIsPaddedAsAMouseOpenedMenu`).
- The .NET SDK 10.0.401 numbers an `ApplicationIcon` 32512, `IDI_APPLICATION`'s number, and
  copies it into the apphost on any system (dotnet/runtime v10.0.13,
  `src/installer/managed/Microsoft.NET.HostModel/AppHost/HostWriter.cs`): `llvm-readobj
  --coff-resources` on a win-x64 build made on macOS lists `GROUP_ICON` 32512 with eight
  `ICON` entries in the exe. So `Shell.AppIcon` loads 32512 from `Pitboard.exe`.
- Microsoft's high DPI desktop development page (updated 10 September 2026) has a
  per-monitor aware app load its images again for a new DPI, answers `GetSystemMetrics` at
  the primary display's DPI as the session started and `GetSystemMetricsForDpi` at any, and
  calls the primary display the one the system tray is on. So `IconImage` loads the icon
  with `LoadIconWithScaleDown` at `SM_CXSMICON` for the primary display's DPI, in place of
  `LoadIconMetric`, whose `LIM_SMALL` names no DPI, and loads it again when
  `HiddenWindow` hears `WM_DISPLAYCHANGE`, `WM_DPICHANGED` or `WM_SETTINGCHANGE` and that
  size has changed. `NotifyIcon` sends the new one with `NIM_MODIFY` before the one before
  is destroyed (`IconImageTests`, `NotifyIconTests.ANewImageModifiesTheIconAddedAlready`,
  `HiddenWindowTests.AChangeOfTheDisplaysReachesTheApp`). Which of the three a scale change
  sends a hidden window, and how the icon looks after one, are measured by hand.
- Microsoft's notification area guidance (`winenv-notification`) asks for icon variations to
  show changes in quantities, overlays to show a status, and no swaths of pure red, yellow or
  green in a base icon; the icon shows no title, where the macOS menu bar item shows the
  figure. So the model's `MenuBarText` carries the step of the limit the bar names and
  whether the menu opens on something to look at, and its tooltip as whether it does, which
  Narrator reads with the icon, then a line for each account in use. `Tray/Plenty.ico`,
  `Low.ico` and `Out.ico`, drawn in `apps/windows/Icon` at 16, 20, 24, 32, 40 and 48 px, the
  sizes `NotificationIconSize` gives up to 300%, light the mark's row in a neutral tone as
  long as what is left, `None.ico` lights none before a reading, and attention draws the
  shell's stock `SIID_WARNING` over the bottom right at five eighths of the icon.
  `DrawIconEx` blends into a 32-bit DIB premultiplied, and an icon's pixels are
  straight, so `Shell.NotificationIcon` straightens them before `CreateIconIndirect`
  (`IconImageTests.AttentionPutsTheStandardWarningOverTheBottomRight`, which compares the
  icon's top third with the plain one's at 20 px, where its edges are partly covered).
- Microsoft's app icon guidance (updated 14 July and 5 August 2026) asks for a transparent
  icon drawn straight on, half of it at 3:1 against light and dark backgrounds, at 16, 24,
  32, 48 and 256 px at least, and the notification area draws it at 16 px at 100% up to 64
  px at 400%. So `Pitboard.ico` holds 16, 20, 24, 32, 40, 48, 64 and 256 px of the macOS
  icon's board, untilted and unplated, whose grey rim, pole and dim row hold 3:1 against
  both #202020 and #f3f3f3: at each size 50% or more of it does against the first and 81%
  or more against the second.
- Microsoft's app settings guidance (updated 25 June 2026) puts settings behind
  NavigationView's own settings item, in one scrollable column about 1,000 px wide under
  BodyStrong headers, of cards with a header, a description and the control on the right,
  sub-options under an expander, and About at the bottom in an expander with the app's name
  and version, links and legal lines. It names the Windows Community Toolkit's SettingsCard
  and SettingsExpander, which the WinUI Gallery's settings page uses, so `SettingsPage` is
  built of them (`CommunityToolkit.WinUI.Controls.SettingsControls` 8.2.251219). A change
  takes effect at once, through a toggle switch for on and off (Toggle switches, updated 21
  September 2026), and a control that cannot change says why in its card's description.
- Read in the Community Toolkit for Windows at `rel/8.2.251219` (commit a6b4dc45),
  `components/SettingsControls` and `components/Triggers`. A SettingsCard gives a ToggleSwitch
  in it `RightAlignedCompactToggleSwitchStyle`, the switch at the card's right edge with its
  state before it, and its Disabled state dims its header and description
  (`SettingsCard.xaml`). A SettingsExpander's peer is a Group with no ExpandCollapse, and its
  template hands the Expander inside it its name, not its id (`SettingsExpander.xaml`), so the
  UI tests expand through that Expander. Its items come from `Items` or `ItemsSource`, never
  both, so the warm-up's hours sit in `ItemsHeader` and its notes in `ItemsFooter`, as cards
  drawn as the toolkit draws an item (58, 8, 44 and 8 epx of padding, 52 high, a top border).
  A clickable SettingsCard's peer returns itself for the Invoke pattern
  (`SettingsCardAutomationPeer.cs`), and WinUI 2.3.9's `ButtonBaseAutomationPeer` implements
  no `IInvokeProvider` (`Microsoft.WinUI.dll` metadata), so UI Automation could not press it:
  the page copies through a Copy button, as Settings > System > About copies its
  specifications, and links through HyperlinkButtons. `ControlSizeTrigger` is active while
  `MinWidth <= ActualWidth < MaxWidth` of an element it names (`ControlSizeTrigger.cs`), the
  trigger SettingsCard wraps its own content with; AdaptiveTrigger reads the window's width
  instead, so the Activity page stacks its changes by its own width with it.
- Run and RunOnce registry keys (updated 21 February 2026): a value under the person's `Run`
  key is a command line of at most 260 characters that Windows starts at each of their
  sign-ins, with no promise how soon, and a program started from the key should not write
  to it while it runs. So `RunValue` writes `"<path>\Pitboard.exe" --sign-in`, its path
  quoted, refuses a longer one, and writes only when somebody turns the switch, never as
  Pitboard starts. What Settings > Apps > Startup keeps, `StartupApproved\Run`, has no
  documented layout: turning Pitboard off there wrote 01 as its value's first byte and kept
  the `Run` value on the session machine. So `RunValue` reads an odd first byte as off, and
  the switch then shows off and refuses to turn on, its note sending the person to Settings,
  as Microsoft's support article on startup apps does. `RunValueTests` hold it.
- Read in microsoft-ui-xaml at `winui3/release/2.5.1`: a ToggleSwitch raises `Toggled`
  whenever `IsOn` changes (`ToggleSwitch_Partial.cpp`), a NumberBox raises `ValueChanged` on
  any change of `Value`, NaN for cleared text (`NumberBox.cpp`), and a TimePicker raises
  `SelectedTimeChanged` within the setter, with its clock the person's own through
  `DateTimeFormatter` unless set (`TimePicker_Partial.cpp`). So `SettingsViewModel` sends only
  a value that differs from the model's, which `SettingsViewModelTests` and
  `SettingsPageTests.ShowingWhatTheModelSaysSendsNothing` hold, the page shows the share
  again after a cleared box, and the warm-up pickers stay on the person's 12- or 24-hour
  clock, which `LocalTime::time_of_day` gives the hours' summary too.
- Read in the Windows App SDK at v2.5.1 (commit 13160d54), `dev/AppNotifications` and
  `dev/AppLifecycle`. `IsSupported` refuses only an elevated process. For an unpackaged
  process the manager takes the explicit AppUserModelID, and where none is set its
  constructor writes a key of its own under HKCU. `Register(displayName, iconUri)` writes
  `DisplayName`, `IconUri` and `CustomActivator` under
  `HKCU\Software\Classes\AppUserModelId\<id>` and a `LocalServer32` that starts the exe with
  `----AppNotificationActivated:`, takes the icon as a local `.bmp`, `.ico`, `.jpg` or
  `.png` file, and in a self-contained app leaves out the Singleton package's long-running
  process. A click on a running copy that listened before it registered raises
  `NotificationInvoked`. In a copy COM starts for a click, `Activate` keeps the arguments
  for `GetActivatedEventArgs`, which reports an `AppNotification` activation once `Register`
  has run, and raises no `NotificationInvoked` for them. `Unregister` revokes the class
  object alone; `UnregisterAll` deletes the keys, and first makes the AppUserModelId key as
  it looks for the class. The builder escapes `%`, `;` and `=` in an argument as
  `DecodeArguments` reads them back. So a Debug build never asks for the manager,
  `Shell.Name` sets the AppUserModelID first, `WindowsNotifications` checks `IsSupported`,
  listens, registers and then reads the launch, `Launching` drops a second copy started for
  a click, which has no arguments to hand over, `Pitboard.ico` ships beside `Pitboard.exe`,
  and `Pitboard.Probe` calls `UnregisterAll` only after a `Register`.
  `WindowsNotificationsTests` and `LaunchingTests` hold them.
- `AppNotificationManager.Setting` answers `Unsupported` where `IsSupported` refuses, and
  otherwise `ToastNotifications_QuerySettings` for the AppUserModelID its constructor took,
  registered or not (`dev/AppNotifications/AppNotificationManager.cpp` at v2.5.1, commit
  13160d54). `AppNotificationSetting` holds `Enabled`, `DisabledForApplication`,
  `DisabledForUser`, `DisabledByGroupPolicy`, `DisabledByManifest` and `Unsupported`
  (Microsoft.WindowsAppSDK.Foundation 2.3.12's metadata, which WindowsAppSDK 2.5.1 pulls
  in); Microsoft's reference calls the second an app setting of the person's and the third a
  global one. "Launch the Windows Settings app" (updated 26 September 2026) lists
  `ms-settings:notifications` and `ms-settings:startupapps`. So `WindowsNotifications.Read`
  asks for `Setting` only once it has registered, says each value but `Enabled`, and the
  settings link to Settings for the person's own two. Which value Windows gives an
  unpackaged Pitboard turned off in Settings is not measured. `WindowsNotificationsTests`
  holds it.
- The app notifications quickstart (updated 10 September 2026) listens, registers and then
  calls `GetActivatedEventArgs`, and says a cold start reports `Launch` with its arguments
  in `NotificationInvoked`, where the source above reports `AppNotification`; the app takes
  a click from either. The `Register` reference (updated 28 July 2026) asks for `Unregister`
  before the app ends, so a later click starts it again, and says the manager needs the
  Singleton package; the self-contained deployment guide (updated 2 October 2026) says app
  notifications do not, and has an API that may be missing checked with `IsSupported`.
  `ToastNotification.Tag` (updated 23 August 2026) holds 64 characters, and a warm-up's
  notice id carries its message, so the tag is the id's SHA-256 in hex. The app notification
  content guidance (updated 17 July 2026) gives a notification three text elements at most,
  the first its title, and keeps attribution text for a source; the UX guidance (updated 21
  April 2026) has a click on a notification open its context. So a notification says its
  title, its tool and its body, as the macOS one does, with Switch as its button, and a
  click on it opens the window on Accounts.
- WebView2 SDK 1.0.4258.31's reference (Microsoft Learn, updated 12 October 2026): a page
  set as `NewWindow` must be in the opener's environment and profile and not yet navigated,
  takes its settings before it is set and `WebResourceRequested` after, and is what
  `window.open` returns to the opener; with neither `NewWindow` nor `Handled` WebView2 opens
  a window of its own. So `WebSession` handles every request, makes a sign-in window's page
  in the account's profile, prepares it, sets it and only then serves a fixture's pages to
  it. The multiple profiles guide (updated 26 November 2025) gives each profile its own
  folder of cookies, storage and caches under one user data folder, and makes a profile that
  a controller names and none has yet. The distribution guide (updated 2 October 2026) reads
  the Runtime's version from `pv` under
  `EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}`, per machine and per user, and
  has an app feature-detect what a Runtime older than its SDK may lack. In run 38825074643
  `WebSessionTests.ASignInWindowKeepsItsOpenerAndClosesWhenItsPageIsDone` and
  `ProfileTests.TwoProfilesKeepTheirCookiesApart` passed on both images.
- Chromium 155.0.8059.87, read on 16 October 2026: a download whose name is taken is
  numbered by `CreateUniqueFilename` (`download_path_reservation_tracker.cc`), which puts
  ` (1)` before `FilePath::Extension`, and that keeps a second extension of one to four
  characters before `bz`, `bz2`, `gz`, `lz`, `lzma`, `lzo`, `xz`, `z` or `zst`, takes
  `user.js` whole, and lets an extension hold a space (`base/files/file_path.cc`). A
  download not yet named is written into a temporary file in the download manager's default
  folder (`base_file.cc`, `in_progress_download_manager.cc`). So `download_destination`
  splits a name that way on Windows, and `WebEnvironment` sets each profile's download
  folder. `CoreWebView2DownloadOperation`'s reference (updated 28 September 2026) has
  `Pause` leave a download interrupted with `CanResume` true, and `Downloads` ends one in
  the model only once it cannot resume. The reference gives the reason as `UserCanceled`;
  the runners give `UserPaused` (CF7 under [Windows](#windows)).
- Restart Manager's guidelines for applications (updated 11 March 2025) and the references
  of `RmStartSession` and `RmShutdown` (updated 1 July 2025), read on 16 October 2026: it
  asks a GUI app with `WM_QUERYENDSESSION`, then `WM_ENDSESSION`, both with
  `ENDSESSION_CLOSEAPP`, then `WM_CLOSE` where it does not end. One `FALSE` cancels, and
  only `RmForceShutdown` ends an app regardless. A console app gets `CTRL_C_EVENT`. Only the
  process that started a session may shut down through it, a user session holds at most 64,
  and the session lives in the registry, under the user's HKCU. So `WindowsApps` registers a
  Store app's windowed process alone, never its console `codex` children, ends the session
  once `RmShutdown` returns, and calls it without flags on a thread of its own, since it
  returns only once the app has ended or declined. Tests stand in for Restart Manager: on a
  PC only the app and `Pitboard.Probe` start a session, in the session's steps A4 and A7.
  `IApplicationActivationManager::ActivateApplication`'s reference (updated 22 February
  2024) offers no option that opens an app behind another window, so `WindowsApps` brings
  Pitboard's window forward after it.
- WinSparkle 0.9.4, read on 16 October 2026 in its source at tag `v0.9.4` and in its NuGet
  package. `win_sparkle_set_eddsa_public_key` returns 0 and keeps no key for anything but the
  base64 of 32 bytes, and with no key the downloader logs "Using unsigned updates!" and
  installs anyway (`dll_api.cpp`, `signatureverifier.cpp`, `updatedownloader.cpp`).
  Versions compare by Sparkle's comparator, which sorts `1.0.0-beta.1` and `1.0.0+<commit>`
  below `1.0.0`, against the build version where one is set (`updatechecker.cpp`,
  `settings.h`). The feed's parser takes an enclosure whose `sparkle:os` is empty,
  `windows` or the DLL's own `windows-x64` or `windows-arm64`, and `minimumSystemVersion`
  only as an element of the item, checked with `VerifyVersionInfoW` (`appcast.cpp`). The
  did-find-update callback names no version (`appcontroller.cpp`). A check that fails shows
  nothing unless WinSparkle's window is already open (`ui.cpp`). `win_sparkle_init` asks
  whether to check at the second launch unless `CheckForUpdates` is set, and starts the daily
  checks only where it is on (`dll_api.cpp`, `updatechecker.cpp`). The can-shutdown callback
  runs on WinSparkle's thread before the installer starts, and the shutdown request once it
  has (`ui.cpp`). Settings go to `HKCU\Software\<company>\<app>\WinSparkle` unless the
  app gives its own config methods (`settings.cpp`). The package's `runtimes/win-x64` and
  `win-arm64` DLLs are AMD64 and ARM64, import only system DLLs and export the API by its C
  names (llvm-readobj 22). So `SparkleUpdates` starts WinSparkle only with a key it takes and
  the version without a commit, turns automatic checks on before the first start, reads the
  version of an update found from the feed, asks the downloads question in the can-shutdown
  callback, and keeps WinSparkle's settings in `updates.json` in the app's data folder, which
  an uninstall removes with the folder. Turned on after a start with them off, automatic
  checks begin at Pitboard's next start. GitHub answered
  `releases/latest/download/appcast-windows.xml` with a redirect to v0.15.0's asset, which
  is missing, and then 404 (19 October 2026): no full release carries a Windows feed before
  1.0.0, and a beta's check ended in WinSparkle's error. So a beta reads the feed first and
  starts WinSparkle only on an answer other than 404, reading it again at each settings
  visit. `SparkleUpdatesTests` and `UpdateFeedTests` hold it.
  An update's MSI is checked whole with orlp's `ed25519_verify`, its signature the base64 of
  64 bytes (`signatureverifier.cpp`). Sparkle 2.10.0's `sign_update` signs a file that is no
  feed or release notes whole with the same library, reads the key's base64 seed from
  standard input with `--ed-key-file -` and prints the signature alone with `-p`
  (`sign_update/main.swift`, `common_cli/Secret.swift`, read the same day). A rehearsal on
  16 October 2026 signed two files with a throwaway seed, and `eddsa-verify.swift` took each
  signature under its public key and refused one another seed made. So `windows-update-sign`
  signs each MSI of every Windows release with `sign_update` and checks it with
  `eddsa-verify.swift`, and for the latest full release `windows-appcast.py` then writes the
  feed.
- The .NET 8 SDK adds the commit to `InformationalVersion`, which
  `IncludeSourceRevisionInInformationalVersion` turns off
  ([Source Link included in the .NET SDK](https://learn.microsoft.com/dotnet/core/compatibility/sdk/8.0/source-link),
  updated 26 July 2025), and a `Win32Resource` takes the place of the version information
  the compiler writes and cannot be given with a `Win32Manifest`
  ([Resource options](https://learn.microsoft.com/dotnet/csharp/language-reference/compiler-options/resources),
  updated 28 May 2026). So `Pitboard.App` turns the commit off, and carries the update key as
  `AssemblyMetadata("WindowsUpdatePublicKey")` from `WINDOWS_UPDATE_PUBLIC_KEY` rather than
  WinSparkle's `EdDSAPub` resource. `check-windows-app.ps1` refuses a Release build whose
  `Pitboard.exe` version carries a commit, whose `Pitboard.dll` carries another key than the
  one it was given or a key WinSparkle keeps none of, which reads another feed than the latest
  full release's, and a `WinSparkle.dll` for another machine. `read-assembly.ps1` reads the
  key and the feed from the assemblies' metadata without loading them.

### The model's timers

Read on 5 October 2026 in Rust 1.98.1's standard library and in the macOS SDK of Xcode 27.1,
with Swift 6.4.

- The model's timers run on `pitboard_core::host::awake`, the time the machine has been
  awake. On macOS and Linux that is `std::time::Instant`, which reads `CLOCK_UPTIME_RAW` on
  Apple's systems, which macOS's `clock_gettime(3)` says does not increment while the system
  is asleep, and `CLOCK_MONOTONIC` on Linux, which "does not count time that the system is
  suspended" (Linux man-pages 6.19, `clock_gettime(2)`, read 15 October 2026).
- The Swift model's loops slept with `Task.sleep(for:)`, whose clock is `.continuous` unless
  it is given another, and `ContinuousClock` does not stop while the system is asleep. After
  a long sleep its five-minute read came as the machine woke, where the model's comes up to
  five minutes of waking time later. An app sends `Intent::Woke` as the machine wakes, which
  reads at once: the macOS app sends it on `NSWorkspace.didWakeNotification`, where the
  Swift model read.
- The 30 seconds an app is given to quit run on time awake too, where the Swift model's ran
  on `ContinuousClock`: a machine put to sleep while an app is asked to quit gives it its
  30 seconds of waking time, where the Swift gave it none once the machine woke.
- The look whether to switch Claude Code by itself every 30 seconds, and the wait after a
  refused switch, run on time awake too, so they count waking time. After a sleep the read
  that `Intent::Woke` asks for is followed by a look at once where it lands, and one that
  fails, as before the network is back, leaves the next look to its timer. A refusal's
  wait goes on where it stopped.
- On Windows the standard library's `Instant` reads `QueryPerformanceCounter` (Rust 1.98.1's
  `library/std/src/sys/time/windows.rs`), which Microsoft documents as counting "the time
  when the machine was in a sleep state such as standby, hibernate, or connected standby"
  ([Acquiring high-resolution time stamps](https://learn.microsoft.com/windows/win32/sysinfo/acquiring-high-resolution-time-stamps)).
  There the timers read `QueryUnbiasedInterruptTimePrecise`, whose count "does not include
  time the system spends in sleep or hibernation", in units of 100 nanoseconds
  ([QueryUnbiasedInterruptTimePrecise](https://learn.microsoft.com/windows/win32/api/realtimeapiset/nf-realtimeapiset-queryunbiasedinterrupttimeprecise)),
  read 15 October 2026. With `Instant` there, an app asked to quit just before a sleep would
  have had its 30 seconds used up by the time the machine woke. Microsoft's contract: no
  machine has been put to sleep during a quit yet.

### The fixtures

Measured on 6 October 2026 on macOS 27.0, with `pitboard-ffi` built for
`aarch64-apple-darwin`, in release and in debug, with and without the `fixture` feature.

- The bindings are the same with the feature and without it: the Swift that
  `uniffi-bindgen-swift` generated from each release static library, `PitboardBindings.swift`,
  `PitboardFFI.h` and `module.modulemap`, and the C# that uniffi-bindgen-cs v0.11.0+v0.31.0
  generated, `pitboard_ffi.cs`, had the same SHA-256 each, and the C# generated from each
  debug library did too. The feature changes what the fixture's four exports do, never
  their names, arguments, types or doc comments, which are all a checksum covers
  ([The C# bindings](#the-c-bindings)). So one set of bindings links against either library.
- A library built without the feature holds none of a fixture's text: `grep -a -c` for a
  line of a stand-in page and an account of the worlds found it on no line of the release
  `libpitboard_ffi.a` or `libpitboard_ffi.dylib`, and on 2 lines of each built with it, one
  for each, and on 4 of the universal library `build-xcframework.sh --fixture` builds.
  CI's C# jobs check the Linux library and the gated Windows DLL the same way, and
  `build-app.sh` the macOS library and app.
- `fixture_page` gives, byte for byte, what the macOS app's `FixturePages.page(for:)` gives,
  for nine addresses on `pitboard-fixture://`: each site's home and a path of each, two of
  chatgpt.com's sign-in hosts, the artifact's frame, a host of no site, and chatgpt.com in
  capitals with an escape and a slash at its end. Measured by running the Swift's code,
  copied whole, with Swift 6.4 over the same site table.
- `ModelTests.AFixturesModelStartsAndTellsAListenerOfItsAccounts` starts a fixture's model
  from C#, against the debug library built with the feature, and its listener, written in
  C#, is told the accounts read on a thread that is neither the test's nor one of .NET's
  thread pool, with the .NET SDK 10.0.401. Against the library built without, the test is
  skipped and says why.
- The real core answers some things in the fixtures otherwise than the Swift `FixtureCore`
  did, which the UI tests were written against and read since the macOS app launches into
  the Rust worlds, each held by a test in `crates/pitboard-ffi/src/fixture/tests.rs`:
  - A read whose service cannot be reached answers, each account's numbers the last
    measured and saying "Anthropic could not be reached"; `status` fails only where the
    account index cannot be read, and the read of what is known fails with it. The Swift
    fixture failed readFailure's read, `unreachable`, and listed its accounts from what was
    known. So readFailure's account index is one nobody may read, mode `000`, given it once
    the world's history is made, and both its reads fail, `state_unreadable`: "could not
    read Pitboard's account list at …: Permission denied (os error 13)", on macOS 27.0 as an
    ordinary user. The menu's item and the window's notice say "Couldn’t read usage" with
    those words, and the window, with nothing known to list, says "Couldn’t Read Accounts"
    with Try Again, where the Swift fixture listed claude/work under the notice. A service
    out of reach is held in oneTool instead, made so in its test.
  - The fixture's Claude Code refuses a line typed back without both halves of
    `<code>#<state>`, with its own `Invalid code.` line, and reads another, as the register's
    `sign_in_takes_another_code` holds of 2.1.289; the Swift fixture took any line. So a UI
    test types a code with its `#`, such as `fixture-code#state`, and one typed without
    meets the sheet saying the code was refused, with the field offered again.
  - A switch is logged as `use`, after the command, where the Swift fixture logged `switch`.
  - Claude Code's sessions follow a switch within 33 seconds, `ADOPTION_CEILING_SECONDS`,
    where the Swift fixture said 45.
  - Doctor's checks are the core's: none is called "Keychain", and none warns of a schedule
    that is not there, which was the Swift fixture's one thing worth looking at. oneTool's
    one thing is the world's own instead: personal's parked login lapses in two days, where
    the Swift fixture's lasted eleven, and doctor warns of a parked login whose refresh token
    lapses within three days. So oneTool says "One thing is worth looking at." as before, of
    personal's parked login rather than of daily renewal. stuck has doctor's "interrupted
    switch" worth looking at besides, so it says "2 things are worth looking at.".
  - A read lists the account in use first, and the read of what is known lists only
    enrolled accounts, so a login nobody has named is listed once the first read has asked
    its service whose it is.
- leftInFile, which the Swift fixture never had, leaves a login of personal's in
  `.credentials.json` behind Claude Code's store. Its read warns `fallback_login` with the
  notice's Put Away…, the read of what is known says nothing of it, and Put Away keeps the
  login for personal and deletes the file.
- loginReplaced, which the Swift fixture never had, is oneTool after personal signed in
  with Claude Code itself over work's only login, recorded by a terminal's read ten minutes
  before. Every read, the one of what is known too, warns `login_replaced`, work's row reads
  `login_replaced`, and the app's first read posts it.
- stuck shows the notice offering Give Up… as the app starts, as the Swift fixture did, now
  from the core's own read: a read says an interrupted switch is waiting wherever the next
  change would be refused over it, as a warning with the refusal's code,
  `recovery_undetermined`, and its words. The read of what is known says nothing of it
  there, since only Anthropic could say whose Claude Code's login is.
