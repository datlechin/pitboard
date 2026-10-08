# Architecture

This file describes how Pitboard's code is organised and what must stay true in it. It
also holds the measured facts the design rests on, about macOS, WebKit, Claude Code,
OpenAI's Codex CLI and the sites the app's account windows open. It changes when the layout
changes or a tool build moves a fact, not with each commit.

## Bird's eye view

Pitboard switches Claude Code or Codex between a person's own accounts on one machine. It
also shows how much of each account's limits is left. A switch parks the login in use and
puts another account's parked login in its place. A person asks for each switch, unless
they asked Pitboard to switch Claude Code by itself before the account in use runs out:
the app with its setting on, or `pitboard watch` running in a terminal.

One crate, `pitboard-core`, does this for both tools. It reads and writes each tool's login,
keeps Pitboard's index of accounts, and asks each tool's service for usage.

Two front ends use the core: the command line, `pitboard`, on macOS and Linux, and the menu
bar app on macOS 14 or later. The app calls the core through UniFFI bindings. The command
line inside the app is what the app's daily renewal runs, and what the `pitboard-app` cask
puts on `PATH`.

The app also gives each enrolled account a window on its tool's site, claude.ai or
chatgpt.com, where the site's own pages run in WebKit. The windows take the list of
accounts from the core, and nothing else. [Account windows](#account-windows) describes
them.

Pitboard has no server of its own. The core sends requests only to Anthropic, for Claude
Code, and to OpenAI, for Codex. An account's window loads its site, and whatever the site's
pages load, as a browser would.

## Code map

- `crates/pitboard-core`: the engine. Parking, switching, recovery, the stores and usage.
  The front ends reach it through `service::Pitboard`, built with a `context::Context`.
  - `provider/`: one module per tool, `claude` and `codex`, each implementing the
    `Provider` trait in `provider/mod.rs`. The trait covers where the tool keeps its login,
    whose it is, how to renew it, what it has left and what its sign-in prints, which
    `provider::sign_in_view` reads for both apps. Each module's `assumptions.rs` is
    that tool's register of facts, with a table saying what each fact is on macOS, Linux
    and Windows. `provider/codex/holders.rs` names where a running
    `codex` can be, and what makes each take a switch. `provider/codex/layers.rs` reads
    which store Codex keeps its login in from every layer of configuration Codex reads
    outside a project, in Codex's order, for a switch and for a sign-in.
    `provider/printed.rs` reads what a tool printed as a terminal does: the text it shows,
    and where each hyperlink goes.
  - `holder.rs`: what keeps a tool's login in memory while it runs, told apart by where
    its program runs from. A switch's warning, `doctor` and the app's offer to quit an app
    all read it, so they cannot disagree.
  - `host/`: the machine, behind one seam. The `Host` trait is what a test replaces: the
    system's store of secrets, files, the vault, this user's processes, the scheduler,
    whether this process runs as the person (`elevation`), whether the system is one
    Pitboard changes things on by its version (`floor`, `host::Floor`), and what an
    administrator set for a program outside every home (`administered_file`, and
    `managed_preference`, which `macos/preferences.rs` reads), reached through `Context`
    and faked by `host/memory.rs`, which plays every answer. The real hosts read nothing an
    administrator set in a build for tests (`host/administered.rs`). `fs`, `proc` and `user` are
    plain functions for what the system does whoever asks: private files and directories,
    every other change to the disk, whether a process is alive, the login name, whether
    this process runs as root, and the `PATH` the person's login shell builds, which
    `unix/shell.rs` asks for. `fs` creates each private file itself and hands it back open
    (`create_private`, `open_private_append` and `open_private_lock`), so how a file is made
    private is the face's alone. `fs::testing`, in a build for tests, is how a test or a
    fixture sets a file's access or makes a link; other crates reach it as
    `pitboard_core::testing::fs`. `program.rs` finds a program the way the
    system's launcher does. `mod.rs` chooses the system, once: `macos/` (the keychain
    through `security`, `ps`, launchd) or `linux/` (`/proc`, systemd), each with what
    `unix/` holds for both, or `windows/`, the face that refuses whatever of Windows is not
    built yet: no store of secrets, a vault of `store::Backend::Unknown` whose every call
    cannot be read, no process list, no scheduler, no home of the account's own, no file
    made and no program found. What it does read is this process's token, through
    windows-sys (`windows/user.rs`), which `token.rs` makes an `Elevation` of, and the build
    `RtlGetVersion` gives, which `host::windows_floor` holds against 26100, the build of
    Windows 11 24H2 and Windows Server 2025. `token.rs` is compiled on every system, so how a
    token reads is tested everywhere, and `words.rs` says each system's refusal from a
    `host::Os`. A fact that differs by system is a `match` on `host::OS`, such as the
    folders macOS asks about before an app may look in them, or `default_pitboard_home`,
    where Pitboard keeps its files for a home when `PITBOARD_HOME` names nowhere else.
    `same_path_in_any_case` is how Windows compares paths, for the core and the apps alike.
  - `release.rs`: whether this build may do anything on its system. A Windows build of a
    0.x release may not, unless it was compiled with `--cfg pitboard_unreleased_windows`,
    which CI's Windows jobs set and no release does; macOS and Linux always may.
  - `home.rs`: Pitboard's own directory, and what every home must be before anything is
    read or written under it: a full path (`check_absolute`), and, for Pitboard's own, not
    in a folder that syncs (`check_location`). That is told by the text it always refused,
    such as `Sync`, anywhere in the path as written, so nothing it refused is let through,
    and by the names sync clients give their folders, one folder at a time and in any
    case, and macOS's `Library/CloudStorage` and `Library/Mobile Documents`.
  - `store/`: reading and writing logins, whichever store holds them: the chain rules, a
    file, the vault of files and the stores in memory the tests use. On macOS, parked
    logins are keychain items. On Linux, they are files in the vault.
  - `switch/`: every change to Pitboard's index (switching, by hand or by itself in
    `auto.rs`, enrolling, adopting, renaming, forgetting, renewing, repairing, abandoning
    and uninstalling), and the journal that
    finishes an interrupted switch. With `test-support`, a context may hold a
    `SignInScript`, which plays a tool's own sign-in in place of its program, for a test or
    a fixture that may start none; everything around it is the core's own.
  - `state.rs`: `state.json`, the index of accounts and where each one's login is parked.
  - `autoswitch.rs`: switching Claude Code by itself, for a front end somebody asked to: the
    app with its setting on, or `pitboard watch`. `Threshold` is the share a limit switches
    at, 50 to 99, 95 unless chosen. `decide` is the rule, from the readings Pitboard already
    holds: which limit of the account in use reached the share, and which switchable account
    has room under it in every limit it reports. `Ledger` is `autoswitch.json`, what was
    tried for each limit of each account until that limit resets: the attempts, how many in
    a row failed for a reason waiting may mend, when the last began, whether one switched
    and the accounts passed over, written only under `state.lock`. `look` decides from files
    alone, and says why where Pitboard will not switch. `switch/auto.rs` decides again under
    the lock and switches only for the same plan, through `switch_held` given the account it
    expects to leave, which refuses with `switch_overtaken`, changing nothing, once that
    account is no longer the one signed in; the switch takes that as nothing to do.
    `service::Pitboard::auto_switch` joins the two, and the audit log records a switch, or
    the error that stopped one, as `auto-switch`.
  - `lock.rs`: the lock Claude Code takes around credential writes, taken the same way.
  - `context.rs`: what the core takes from its environment, read from a map of variables
    by the same code for every front end, apart from the `PATH` that `host/linux` reads to
    find the program daily renewal runs.
  - `app.rs`: what an app finds for itself that a command typed at a prompt is given: each
    tool's program, on the login shell's `PATH` and then where the tool's installers put
    it, and the `pitboard` a terminal would run, there and then where each way of installing
    Pitboard puts it, under the app's home and where the system's package managers put
    programs. And the files an app keeps of its own in Pitboard's directory, `told.json` and
    `app.json`, written as the core writes its own and read by nothing of the core.
  - `api.rs`: the requests to Anthropic, and the agent each context's requests go out on,
    `api::agent`. The requests to OpenAI are in `provider/codex/api.rs`.
  - `proxy.rs`: the proxy those requests go through, read from the context's environment by
    the rules ureq 3.4.2 reads a process's with, the refusal of each request a SOCKS proxy
    would have carried (`proxy::Refusal`), what `doctor`'s `network` check says of it, and
    the variables a renewal schedule installed from that context is given. See
    [ureq's proxies](#ureqs-proxies).
  - `pace.rs`: how a limit's use compared with an even use across its window when it was
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
  - `usage.rs` and `readings.rs`: an account's usage, and `usage.json`, the one reading of
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
    came, and never a tool's own cache (see [Claude Code](#claude-code)). `schedule.rs`
    decides what daily renewal runs and whose it is, and refuses a program in the temporary
    copy macOS runs an app from, by `in_a_temporary_copy`, which an app asks of the command
    line inside it too; the host's scheduler writes it.
  - `words.rs`: the sentences and column words Pitboard says in more than one place, each
    a function of typed values: spans of time, a limit's names, when it resets, its pace and
    when it runs out, a parked login's life, a renewal run and doctor's summary. It also holds
    `usage_level`, the steps at which a limit's colour changes. A thing said both in a
    column and in a sentence has a function for each form. The command line calls these
    functions directly, and so does `pitboard-ffi`'s `present/` as it makes the snapshot,
    which carries what an app shows of them. The bindings export none of them. Clock times
    are not in it.
- `crates/pitboard`: the command line. Arguments, rendering for people, the man page, and
  the `--json` contract, pinned by the snapshots in `crates/pitboard/tests/snapshots`.
  `json.rs` is the one writer of that JSON, for every command and every error. Its example
  `stand-in` is the program every test starts in place of a tool. Its code is
  `pitboard-core`'s `stand_in.rs`, compiled with test-support, and it plays the script
  written beside each copy of it. Nothing installs or ships it. `watch.rs` is
  `pitboard watch`, a loop in the foreground until Ctrl-C: it reads every account every
  300 seconds, as the app does, looks every 2 seconds at when the index and the readings
  were last written, and decides again whenever either changed and at least every 30
  seconds. It says every switch, and each thing that stopped one once until the next
  switch, and ends on a refusal watching cannot mend: running elevated, a Windows build or
  Pitboard's own files unusable.
- `crates/pitboard-ffi`: the core as UniFFI bindings, for the apps: a static library for
  the macOS app, a dynamic one for the Windows app. An app reaches the core through the
  model alone.
  - `lib.rs` declares what the bindings export beside the model and the account windows'
    rules: the records a snapshot carries of what the core answered, such as `Status`,
    `Account` and `Abandoned`, and three free functions, which the macOS app calls, none as
    it draws: `app_command_line`, for the command line inside its bundle, as it starts and
    as it links that one; `can_run`, whether that one runs, as it links it; and
    `pitboard_directory`, for where `app.json` is and the key the account windows' records
    are kept under, as it starts. The snapshot carries what an app shows of the tools, a
    sign-in, the command line a terminal runs, a limit and its pace, a parked login, a
    renewal run and doctor's checks, made by `present/` and the model's lanes, so no free
    function says it again. The helpers `present/` makes it with here, `tools`,
    `usage_level`, `renewal_note` and `doctor_summary`, are the crate's own.
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
    `report (1).pdf`, takes names in any case and in no other form for one file, and
    promises no passkey. `records.rs` reads and writes the windows' records, the
    stores each Pitboard directory made and the page each window was last on, which the
    model keeps in `windows.json`.
  - `model/` is the app model the macOS app shows and the Windows app is to show. An app
    makes a `PitboardModel`, sends it an `Intent` for each thing asked of it, and its
    `ModelListener` is told of each numbered `Snapshot`; its `AppControl` quits and opens
    other apps, and its `Notifications` posts what has run out. `state.rs` holds what the
    model knows and decides what follows each message, `advice.rs` which account to offer
    once the one in use has run out, `preferences.rs` what the app's own preferences are,
    `machine.rs` what the model knows of this machine rather than its accounts, and
    `windows.rs` the account windows' bookkeeping; `lanes.rs` runs what it decides, on a
    lane of reads, which reads the schedule, doctor's checks and the log too, a lane of
    changes, one at a time, which also repairs and changes the schedule and renews, a lane
    that lists processes and asks the app's `AppControl` about other apps, a lane that asks
    what is installed and looks for the `pitboard` a terminal runs, a thread of its own for
    each sign-in, a lane that types a code back to one or stops it, a lane that reads and
    writes what the model keeps, in Pitboard's directory and the windows' records in the
    app's own, and one that posts through the app's `Notifications`, and tells the listener
    on a thread of its own; `mod.rs` holds the exported types and the actor thread that owns
    the state. So far the model reads the accounts, looks every two seconds for a change
    made elsewhere, asks which tools are installed, switches, quits the app holding a tool's
    login when the person lets it, gives up on a stuck switch, keeps what each tool's last
    switch said, runs each tool's own sign-in, enrols the login signed in now, renames and
    forgets, keeps the sheet over the main window, says which account to switch to once the
    one in use has run out, notified once for each reset, keeps the app's own preferences,
    and keeps the daily renewal schedule, renews now, makes doctor's checks, reads the
    activity log and finds the `pitboard` a terminal runs. It keeps the account windows'
    books too: which store is whose and each window's last page, which windows close and
    which stores go after a read, the link waiting for an account, with the wait before it
    can be opened, and the downloads. Its tests are files of their own there:
    `reading.rs`, `switching.rs`, `signing.rs`, `changing.rs`, `advising.rs`, `keeping.rs`,
    `maintaining.rs`, `presenting.rs`, `windowing.rs` and `cadence.rs` drive the state by
    hand, `lanes.rs` has the lanes' own, and `threaded.rs` drives the model through its
    threads over the real core.
  - `present/` makes each `Snapshot` from the model's state: `present` takes the state and
    the moment, and builds every sentence and row the menu bar, the menu and the window
    show, so a view decides nothing. `accounts.rs` is the menu bar's words and the
    accounts' sections and rows, with their limits and what each one's own menu offers,
    `notices.rs` the notices, their order and what the menu says of them, `setup.rs` the
    footing, the step it asks for and what stands in for an empty list, `sheets.rs` the
    sheets with their default buttons' words, the quit question and a failure's alert, with
    `name_to_save`, the rule a sheet's default button and the model both save by,
    `machine.rs` what the settings and the window's other panes show of this machine, daily
    renewal, Renew Now, doctor's checks, the activity log and the command line,
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
    makes each of the ten on the real core: a home in the fixture's own folder in the
    temporary directory, `MemoryHost` and `ScriptedApi`, and every account put there by the
    core as a person would have, signed in, enrolled, parked and switched on a clock set to
    when it happened. `tools.rs` plays each tool's sign-in through the core's
    `SignInScript`, with the person at the browser; `apps.rs` is the fixture's other apps,
    where ChatGPT runs Codex's login and quits when asked, and its notifications, which post
    nothing; `pages.rs` the stand-in pages an account window loads on `pitboard-fixture://`,
    from the site table. `mod.rs` holds what every build exports of them, the same in each:
    `PitboardModel::fixture`, `PitboardModel::fixture_in`, `fixture_names` and
    `fixture_page`, which without the feature refuse, naming it, or name none. `tests.rs` is the macOS app's former
    `FixtureTests.swift` ported, and what each UI test reads in its world.
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
- `crates/pitboard-share-ffi`: `pitboard-sites` as UniFFI bindings for the macOS Share
  extension alone, a static library with one function, `share_link`. It checks a shared
  page and writes the Pitboard link that hands it to the app, or says why not in the
  sentence the app shows. It holds nothing of the core.
- `crates/uniffi-bindgen-swift` and `crates/uniffi-bindgen-csharp`: generate the Swift and
  the C# bindings with exactly the UniFFI version the library uses. The bindings check
  method checksums when they load. The C# generator is NordSecurity's, pinned to a release
  built against that UniFFI; it is a build tool, so `deny.toml` leaves it out of the graph.
  Every type the core's bindings export is declared in `pitboard-ffi`, because the C#
  generator cannot use a type from another crate. `pitboard-share-ffi`, which only Swift
  reads, declares its own.
- `crates/pitboard-conformance`: checks a tool's register against a build of that tool, for
  macOS, Linux or Windows.
- `crates/pitboard-probe`: the Windows measurement probe. It is `publish = false`, never
  built by `release.yml`, and has a Windows-only measuring half (`win/`) beside a
  cross-platform half (the command line, the report shape, the write guard, the Credential
  Manager and process-image allowlists, the redactor, the logon and elevation naming, the
  PE reader), so that half is unit tested on every system. One subcommand per measurement
  the VM sessions and the runner facts need; `.github/scripts/runner-facts.ps1` runs the
  safe ones on both Windows CI legs. Every writing subcommand refuses unless the account
  carries a throwaway marker, and refuses a scratch path that is relative or keeps a `..`,
  or that reaches a real login folder or file, in any spelling Windows opens or by the
  identity of a folder on the way; every file it is named carries the `pitboard-probe-`
  prefix. `credman-names` asks Credential Manager only for the live login families' and
  `pitboard-*` prefixes and never reads a blob out. Reports name principals by their
  relation to the token rather than by SID, print paths with the profile folder and the
  account name replaced, and never hold two keys that differ only in case, which
  PowerShell's `ConvertFrom-Json` refuses. `helpers/` holds the Bun and Node scripts, and
  `sparse/` the sparse-package manifests, script and steps, that the owner runs in the VM.
- `apps/`: the native apps, one directory for each system.
- `apps/windows/`: the Windows app. `Pitboard.Core` is the core's C# bindings as an
  assembly of their own, generated into `Generated/` and not committed, and
  `Pitboard.Core.Tests` calls the core through them.
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
    (Notification Center, the category with its Switch button, and permission asked at the
    first post) and `MacLocalTime` (the person's locale, calendar and 12 or 24 hours).
    `LoginItem.swift` is opening at login (`SMAppService`) and `CommandLineTool.swift`
    linking the command line with an administrator's password, which stay the app's own.
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
    passes `--fixture`, and fails where the core's library or the app holds a fixture
    world's text.
- `packaging/`: the files a release writes into the tap `datlechin/homebrew-tap`. They are
  the casks `pitboard.rb` for the command line and `pitboard-app.rb` for the app,
  `tap_migrations.json`, and the tap's README.
- `docs/`: the Mintlify source of docs.usepitboard.com.
- `website/`: reserved for the usepitboard.com site, to be built with Astro; empty.
- `.github/`:
  - `workflows/ci.yml` checks every push to `main` and every pull request, and
    `workflows/release.yml` turns a `v` tag into a release.
  - `workflows/conformance.yml` checks each tool's newest builds, one for each system,
    against its register.
  - `workflows/sparkle.yml` opens an issue when Sparkle has a release newer than the one
    `apps/macos/project.yml` pins, because Dependabot cannot read that pin.
  - `actions/xcodegen` puts the pinned XcodeGen on `PATH` for every job that builds the
    app.
  - `workflows/rotation.yml` rehearses rotating the update key.
  - `actions/apple-keychain` imports the Developer ID certificate for every job that signs.
  - `scripts/` holds the EdDSA key and signature helpers, and the scripts that add Sparkle,
    the command line and the Share extension's library to the app's bill of materials.
  - `dependabot.yml` asks for weekly updates of Cargo dependencies and GitHub Actions.

## Invariants

- The core prints nothing.
- The core reads its environment in `Context::read`, from a map of variables, the same way
  for every front end. The command line passes its own through `Context::from_env`. An app
  passes the one it was started with through `AppContext::discover`, which also asks the
  person's login shell for `PATH`, because an app the system started has none of a
  shell's.
- Three variables are also read straight from the process. `PATH` is read when a context
  made with `Context::new` was given no search path (`context.rs`), and on Linux to find
  the program daily renewal runs (`host/linux`). `NO_COLOR` is read by the status line
  (`pitboard/src/main.rs`). `XPC_SERVICE_NAME`, which launchd sets, is read in
  `host/macos/launchd.rs`. `clippy.toml` refuses `std::env::var`, `var_os`, `vars` and
  `vars_os` outside `context.rs`, so every other read carries an `#[allow]` that says why
  it is meant.
- `READ` in `context.rs` and `settings::OVERRIDING_ENV` name every variable Pitboard
  reads, and `Environment` refuses, in a build with debug assertions, to read one they do
  not name. The integration tests withhold every one of them from each command they run,
  except `HOME` and `USER`, and the core's unit tests make their context with
  `Context::for_unit_test`, which withholds all of them, so a variable exported where
  `cargo test` runs, such as `PITBOARD_CLAUDE`, never reaches a test.
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
  fixtures among them, is told there is none, which is refused as below.
- Every home Pitboard reads or writes under is a full path: `HOME`, Pitboard's own
  directory, `CLAUDE_CONFIG_DIR` and `CLAUDE_SECURESTORAGE_CONFIG_DIR` where they name a
  folder, and `CODEX_HOME` where it is set. One that is empty or relative is refused with
  `home_not_absolute`, naming the variable, by `home::check_absolute`, which the gate every
  change passes asks, and every read of the accounts. `doctor` fails its `homes` check
  over it and checks nothing else. The command line asks it before every command but
  `completions`, `manpage`, `doctor` and `renew`, which the gate refuses, so `log`,
  `schedule status` and the status line refuse it too. Nothing is resolved against the
  folder Pitboard runs in.
- Which system Pitboard runs on is decided in `host/mod.rs` and nowhere else. Anything
  that differs by system is either the host's to answer or a `match` on `host::OS`, so a
  system added to `host::Os` does not compile until it is said for every one. No such
  `match` has a `_` arm. Where a fact of Windows is not read yet, its arm refuses through
  an answer its callers already have, never one that would let Pitboard act: each tool's
  live chain on Windows is one store whose every call cannot be read (`store::Unbuilt`),
  never Claude Code's file alone, and the Windows face answers as `host/windows` says.
- A Windows build of a release before Pitboard for Windows is released does nothing
  (`release.rs`). The gate every change passes asks first, so no file is written, no store
  of logins changed and no token renewed even by a caller that forgot to ask; every read
  of the account list asks too; the app's core asks before every read the model makes
  (`launch.rs`), so an app reads none of Pitboard's files and no login; and the command
  line asks before every command but `completions` and `manpage`. Only
  `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc` compile (`lib.rs`). The switch
  that opens a Windows build early is a `cfg` set in `RUSTFLAGS`, never a Cargo feature,
  since crates.io offers a published crate's features to anybody.
- Outside a host's own face, a test or a fixture sets a file's mode, makes a file runnable
  or makes a link only through that face's `fs::testing` (`pitboard_core::testing::fs`),
  never through `std::os::unix`. A link to a directory is asked for as one, `link_dir`, apart
  from a link to a file or to nothing, `link`. So a system whose files have no mode, or that
  links a directory otherwise than a file, says how in its own face and nowhere else. The
  integration tests' harness says what differs by system in
  `crates/pitboard/tests/common/os.rs`, one `match` on `host::OS` for each fact: where
  Claude Code keeps the login it uses (on Windows the harness assumes `.credentials.json`,
  as on Linux, pending W22: Claude Code's register has `no_keyring_off_macos` unread on
  Windows, and which store its Windows build chooses, `windows_backend_choice`, is not read
  yet, so no test that plants a Claude Code login runs there before W22), where Pitboard
  parks one (sealed to the person on Windows, which only the core's vault writes, so the
  harness parks through it), which variables a command is passed on, which folders
  of a person's account it is given as scratch ones of its own, what its `PATH` holds after
  the test's programs, what a program's file is called, and how npm lays out a package's
  program. The whole suite runs on Windows, and a test that cannot pass there until a later
  pull request says which with `#[cfg_attr(windows, ignore = "W<n>: …")]`, the one way a
  test is put off there.
- No test addresses a name a real login may be kept under. `guard_not_live` refuses by
  pattern, never by a hash computed for a real home, the families
  `pitboard_core::provider::names` tells apart, which the measurement probe's leak check
  reads too: every Claude Code slot, bare or under an account and in each of its pieces,
  but the one hashed from the test's own folder; Codex's `cli|` and `secrets|` targets; and
  anything under `Codex MCP Credentials`.
- No test uses a folder that is, holds or lies in a real login place: the account's own
  `.claude`, `.claude.json`, `.codex` and Pitboard folder, from the home and the Pitboard
  folder the system names for the account whatever the environment says (the passwd
  database, or `FOLDERID_Profile` and `FOLDERID_LocalAppData`), and those the environment
  the tests were started in names. A folder is compared with each in any case, through
  every link the file system resolves. CI's Windows legs check after each run, with the
  probe and as the user the tests ran as, that no item of those families or
  `pitboard-citest-*` is in its Credential Manager, that Task Scheduler's `\Pitboard\` is
  empty, and that no real login folder appeared.
- Every program the command line's tests and `pitboard-ffi`'s tests start in place of a
  tool, or put where one is looked for, is one compiled program, the `pitboard` crate's
  example `stand-in`, playing the script written beside its copy
  (`pitboard_core::testing::stand_in`): never a shell script, so it is the same on every
  system. That covers `claude` and `codex` signing in, holding their output open, refusing
  a code and recording how they were started, npm's interpreter, a `systemctl`, and every
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
- A read never settles an interrupted switch. Every change settles one first, under
  Pitboard's lock. Where that change would stop at it as `recovery_undetermined`,
  `status`, `status_offline` and `doctor` say so in its words, from the same reads and the
  same decision (`read` and `decide` in `switch/journal.rs`, through `switch::stuck`), and
  that check takes no lock and writes nothing. `status_offline` and `doctor` send no
  request, so they say it only where the record and the logins tell it without one: a
  Codex login names its own account, and a Claude Code login renewed since the switch
  stopped is one only Anthropic can name. While a switch is waiting, telling reads the
  tool's login and the copy the switch parked, which on macOS are keychain items; with no
  record there, the check is one look at whether the file is there.
- Pitboard changes nothing where it runs as root or under sudo, or elevated on Windows, or
  where the host cannot say whether it does: a file it wrote would be root's or the
  administrators', and a keychain item might be, where the person's own runs might not read
  or replace it. On Windows `TokenElevation` decides, `TokenElevationType` says which words
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
  `remove` and the service manager they ask, a tool's sign-in started, and the token
  exchange, `Provider::renew` down to the request. So a change that skips the gate does
  not compile, and nothing rotates a refresh token it could not then write down. Every
  change asks the gate before it reads, locks or records anything, so a refused one leaves
  even the audit log as it was. Where the gate refuses, `status` answers what
  `status_offline` answers, with a `read_only` warning, the status line writes neither
  sessions nor readings, `doctor` fails its `system_too_old` or `elevated` check, first and
  in the gate's order, and an app writes no file of its own, since `app::write_file` takes a
  permit too. What each says, on each system, is written once, in `words::elevated` and
  `words::too_old`.
- A run of the daily renewal schedule renews the default home, whatever `PITBOARD_HOME`
  says, since the schedule is that home's alone: `Pitboard::renew` renews in the context
  `schedule::for_its_run` gives it. Such a run is told apart by what the scheduler says,
  launchd's job label in `XPC_SERVICE_NAME`, or, since what systemd passes a job is best
  effort and not to be relied on, by the marker the unit's `ExecStart` carries,
  `renew --scheduled` (`schedule::SCHEDULED_RUN`), which the command line passes on through
  `Context::started_by_the_schedule`. The command line leaves `renew` to the gate, which
  asks of the homes the run renews in, so an empty or relative `PITBOARD_HOME` stops no run
  of the schedule. In a build for tests, such a run whose home is the account's own panics
  before it reads anything. For the same reason `schedule::install` refuses, with
  `schedule_not_default_home`, wherever `schedule::serves` is false, that is while
  `PITBOARD_HOME` names another directory. `uninstall` is not refused, so a schedule can be
  taken away from any home, and `repair` does nothing there, as before, without an error.
- The schedule's job is started with the system's environment, not the person's shell, so
  `schedule::install` gives it, through `Scheduler::put`, `PITBOARD_NO_ARGV` where the
  installing context refuses the argument line, and every proxy variable that context
  sets, under the name it was set by, even empty (`proxy::Proxies::variables`): the
  LaunchAgent's `EnvironmentVariables`, the systemd service's `Environment=` lines. A
  reinstall takes the current ones; `repair` keeps the proxy variables the installed file
  gives (`Scheduler::environment`). A proxy's address can hold a password, so
  `host::unix::service::write` writes every file of the job with `atomic::Perms::Secret`,
  mode 600 whatever was there, where it kept a file's own mode before. `doctor` reads the
  installed variables as a context's are read and warns, `schedule_proxy`, where they
  would send a request another way than this run's (`Proxies::same_way`). In the app,
  whose environment is the system's as the job's is, the same line is `ok` and says how
  each way of installing the schedule chooses its proxy: the app's switch would take a
  terminal's proxy away.
- No test starts the person's own login shell. A test names a shell of its own in `SHELL`,
  one that is not there, or hands in what a shell said.
- A test never reaches the system's own scheduler. A test context schedules through
  `MemoryHost`, which writes the files in the test's home and asks no service manager, and
  the real hosts refuse to ask launchd or systemd from any build for tests: a unit test,
  the command line the integration tests run, or an app built with the fixtures.
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
  and an app may make them where it likes: `name_to_save` and `downloads_quit_question`,
  which read only what they are given; `sites`, `sites_for`, `site_names`, `site_link`,
  `read_pitboard_link`, `pitboard_link` and `link_refusal_reason`, which are
  `pitboard-sites`' and read only what they are given too; the account windows' rules, such
  as `store_id`, `window_accounts`, `decide_navigation` and `window_note`, which read only
  what they are given as well, so a web view's delegate asks them as it is asked;
  `download_destination`, which asks the file system whether each name it tries is taken;
  `app_command_line`, which only joins paths; `can_run`, which asks the file system about
  one path; `pitboard_directory`, which reads the environment it is given and, without
  `HOME`, this account's passwd entry; and `fixture_names` and `fixture_page`, which read
  only what they are given. `PitboardModel::fixture` and `PitboardModel::fixture_in` make
  the fixture's world in its folder before they answer, files in a temporary directory and
  nothing slower.
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
  so a read answers while either waits. An app is quit only when the person says so, the
  way a person quits it and never by force, and is given 30 seconds, asked every 200
  milliseconds whether it still runs; one that has not quit by then stops the switch before
  anything changes. An `AppControl` that throws says the app still runs, so nothing is
  switched under an app nobody saw go. Pitboard opens again only the copy it quit, as soon
  as the switch is over, whether or not it worked. `Intent::QuitAndSwitch` names the
  account of the question it answers, as `AppModel.quitAndSwitch` took the question, so an
  app may close the question with `Intent::KeepAppOpen` before or after it sends the answer:
  a question closed unanswered is kept until another switch is asked for, and an answer is
  taken once.
- A switch of Claude Code the app makes by itself, with its setting on, is claimed the same
  way, with `switching` naming the tool rather than an account, since the core chooses
  which: the model asks for one after it advises, where a limit of the Claude Code account
  in use has reached the share, and not while another switch, the question before one or
  another change of the app's own is under way, nor after a refusal until the app's next
  read lands. It runs on the lane of changes, behind any switch asked for, and asks nobody
  about quitting an app, since Claude Code follows a switch by itself. A switch it made is
  taken as one asked for is, with the read after it, and said in a notification with no
  button; a reason it did not switch, and a refusal, are said once each until it next
  switches, and never in an alert, since nobody asked. The setting is kept in `app.json`
  with the other preferences, and taken only once they are read.
- Every other change this app makes to the account index holds it as a switch does, and the
  poll leaves the index alone meanwhile: naming the login signed in now, a sign-in's
  enrolment, a rename, forgetting, giving up on an interrupted switch and renewing parked
  logins. Each holds the index inside `State::apply` from the
  moment its intent is taken, a sign-in's from the moment its thread is told to enrol, until
  the read after it is over, landed, dropped or failed, or until the change itself has
  failed, so a change made elsewhere is noticed by the next look once none is under way. A
  look can find the index as such a change wrote it and land after the change has answered,
  before the read the change asked for: taken for a change made elsewhere, it dropped that
  read as one that started before a change, and a look that landed while a rename was made
  put away what the account's last switch said
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
  found, never as a snapshot is made. A schedule an older app wrote is repaired once a
  launch, as the model starts, and read again only where it was. Renew Now runs one renewal
  at a time, and is over once the read after it, asking every service, is. Doctor's checks
  and the activity log are read each time their pane is shown, the checks counted while they
  run, as reads are. Opening at login and linking the command line onto the `PATH` with an
  administrator's password are each app's own, and once an app has linked it, it sends
  `Intent::LookForCommandLine` for the model to look for the command line again.
- `AppControl` names an app by one string, the id the core's holder detection gives it,
  which on macOS is its bundle id, in `provider/codex/holders.rs`. What names an app on
  Windows, and so what the core's holder detection gives there, is a question still open
  for the owner; the trait takes one string so that a Windows id fits it unchanged.
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
  snapshot and `Intent::Start` cross the bindings in a test, and the library calls a
  listener written in C# or Swift, from a thread of its own, against a library built with
  the `fixture` feature. Every `AppControl` a test hands the model is a stand-in that records
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
  `WebEnvironment.recordKey` alone and hands it to the model as `WindowsLaunch::key`.
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
- On macOS, only `/usr/bin/security` reads or writes Claude Code's keychain item and
  Pitboard's parked items. No keychain item is touched through the Security framework. The
  reason is under [macOS](#macos) in Measured facts.
- Pitboard takes Claude Code's write lock, the same way Claude Code takes it, before writing
  Claude Code's login. It writes the login where it already lives.
- Codex takes no lock on `auth.json`, so a switch reads the file again before replacing
  it. Every switch reads the live login back rather than trusting its own write.
- Pitboard never answers a failed keychain write by writing Claude Code's plaintext file.
  That demotion is Claude Code's to make.
- A Codex login is moved, never copied (`ParkSemantics::MoveOnly`). The parked login is
  read back before the incoming login is written. Codex's own sign-in and sign-out revoke
  the stored refresh token, so two usable copies of one login must never be at rest.
- No login moves until Pitboard knows whose it is. A Claude Code login's account is asked of
  Anthropic; a Codex login's is read from its ID token.
- Pitboard never renews the login in use. That is the tool's own job, and a second renewer
  would break it.
- Nothing outside `pitboard-core` writes Pitboard's index. Every change goes through
  `switch`, which records what it is about to do first and finishes an interrupted change
  before starting another.
- An answer from Anthropic that Pitboard read whole replaces a Claude Code account's
  reading, and a session only moves that reading, within the limits the answer gave. So
  numbers filed under the wrong account go at that account's next answer.
- Pitboard switches by itself only for a front end somebody asked to: the app with its
  setting on, or `pitboard watch` running. It never switches Codex by itself, since a
  running `codex` never follows a switch. The status line and the daily renewal schedule
  never switch, and an automatic switch never quits an app.
- An automatic switch is decided twice. `autoswitch::look` decides from files alone: it
  takes no lock, sends no request, reads no keychain and records nothing, so a look that
  finds nothing to do costs nobody anything. `switch/auto.rs` decides again under
  `state.lock`, from the files as they are then, and switches only for the same plan and
  only away from the account still signed in. So the app, `pitboard watch` and a person's
  own `pitboard use` never make two switches from one reading. The attempt is written to
  `autoswitch.json` before anything moves, so a switch killed midway still counts against
  that limit's attempts. One that failed for a reason trying again may mend, Anthropic out
  of reach or Claude Code writing its login, is taken back out of them, so an outage never
  uses them up. The next try waits a minute, then twice as long after each such failure in
  a row, up to 15 minutes.
- Additive writes become durable before destructive ones. A run that dies midway leaves a
  spare copy of a login, never a missing one.
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
  Renaming or removing one is. While Pitboard is at 0.x, a breaking change gets a new minor
  version, as in 0.3.0, and after 1.0 a new major version.
- Pitboard and each tool meet at the tool's register. What Pitboard relies on about a tool
  is written there, with the systems each fact was read on, and the conformance run checks
  it against the tool's newest builds twice a week.
- Pitboard and each service meet at a few requests. The list, with what each request
  carries, is in
  [What leaves your machine](https://docs.usepitboard.com/security#what-leaves-your-machine).
- Pitboard and claude.ai or chatgpt.com meet at an account's window, which a person opens.
  The site's own pages sign the window in, WebKit keeps that sign-in in the account's
  store, and Pitboard decides only where each navigation goes.
- Pitboard and a browser meet at the Share extension, which hands the app the one link the
  browser shares, as a Pitboard link. Pitboard reads nothing a browser keeps.
- The code and the release meet at a `v` tag, which `.github/workflows/release.yml` turns
  into a release. [RELEASING.md](RELEASING.md) has the procedure.

## The state file

`~/.pitboard/state.json` is Pitboard's index: which accounts it knows, and where each one's
login is parked. It carries a `schema` number.

The app and the command line inside it update together. A command line installed another
way updates by its own route. So on one machine, an older Pitboard can meet a file a newer
one wrote.

Reading forwards is `state::migrate`. Each schema bump adds a step after the ones before
it, so a file two versions behind comes forward in one read.

Reading backwards is not possible. The older Pitboard refuses the file and says to update
it. A bump needs a test that loads a file the previous version wrote.

Schema 4 records each account's tool, and which account is signed in for each tool. A
schema 3 file is brought forward on its first read, with no keychain item or vault file
touched.

Schema 5 gives each account an `id`, set at enrolment and never changed. Its parked logins,
readings, usage history, budget and windows are filed under it. A login is matched to its
account by its tool's identity, which for Claude Code is the account and the organisation
together (`Account::owned_by`), so one person's two organisations are two accounts. An
account brought forward from schema 4 keeps its account uuid as its `id`, and nothing filed
under it moves.

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
  `verified_against` is the build its macOS and Linux readings name, and
  `assumptions::verified_on` gives the build for any one system.
- `NotRead(reason)`: not read there, and why. Claude Code's Linux and Windows builds have no
  keychain backend, so its keychain facts are read from its macOS build alone.
- `Pending { by, reads }`: to be read there by a later pull request of the Windows work,
  `W2` to `W27`, which says what that pull request reads. `assumptions::pending` lists
  these, and the Windows work is not done while the list has anything in it.

Where a tool does something else on Windows, the Windows behaviour is a fact of its own,
such as `credman_target` beside the keychain's `credential_service_name`. It is never a
second reading under the first fact's name.

`pitboard-conformance` tells a build's system by its header: ELF for Linux, Mach-O for
macOS, and PE for Windows, for x64 and ARM64 only. It refuses any other file. It reports
which facts can still be read from the build, which have moved, which name nothing to look
for, which it skipped and why, and which wait on the Windows work.

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

On 6 October 2026 both registers were read for Windows, from bytes on a Mac, and no
Windows build was run:

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

On 8 and 9 October 2026 Claude Code 2.1.294 was read again: its macOS build on the 8th, and
its `linux-x64`, `win32-x64` and `win32-arm64` builds on the 9th, as bytes on a Mac, and
none was run. All four are built from commit 8f033c6. For each fact read from them, the code
it names was compared with the macOS build's token by token, minified names apart, and is
the same in all four: `usage_cache_stamp_is_the_configs`. The checker finds every fact each
build was read for.

`.github/workflows/conformance.yml` checks the newest builds of each tool against its
register on Mondays and Thursdays, or a version given by hand. It reads four builds of each
tool, `linux-x64`, `darwin-arm64`, `win32-x64` and `win32-arm64`, all on Linux. Its most
recent run says which facts can still be read from each build it checked, and which have
moved.

Adding a tool takes three things: a register read out of a named build, with its table, a
module under `provider/` implementing `Provider`, and a conformance job. `ProviderId`,
`ProviderId::ALL` and the matches in `provider::of`, `assumptions::of` and
`assumptions::per_system` name every tool. The compiler and the tests then point at what an
added tool has to fill in.

How to run the checker and add a fact is in
[Tool registers in CONTRIBUTING.md](CONTRIBUTING.md#tool-registers).

## Account windows

The app gives each enrolled account a window on its tool's site: claude.ai for a Claude
Code account, and chatgpt.com for a Codex account. The site's own pages run in it, in
WebKit, with a store of website data that belongs to that account alone. A page shared from
a browser's Share menu reaches the app as a Pitboard link, and the person chooses which
account's window opens it.

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
  records: its windows and menus ask `sites`, `sites_for` and `site_names`, and the model
  reads each Pitboard link with `pitboard-sites`' `read_pitboard_link` and says a refusal
  in its words. Only the tests make a link with `site_link`. The Share extension reaches it
  through `pitboard-share-ffi`.
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
  `store_id` writes in lower case.
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
  waits until the app has answered.
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

Nothing of the Windows app's account windows is written yet. Its WebView2 code would get
the same decisions from the model, and one thing differs from WebKit in how a store goes,
read in Microsoft's documentation of `CoreWebView2Profile.Delete` for WebView2 1.0.4191.47
on 6 October 2026: deleting a profile marks it for deletion, closes the web views using it
and raises its `Deleted` event, and its folder is deleted only as the browser process
exits, or at a later start where something still held its files. Making a profile of the
same name before then fails with `HRESULT_FROM_WIN32(ERROR_DELETE_PENDING)`. So the
Windows app cannot answer `StoreDeleted` when it asks for a deletion, as the macOS app does
once `remove(forIdentifier:)` returns: it answers `StoreHeld` while the folder is still
there, which keeps the store recorded and has the next read ask again, and `StoreDeleted`
once the folder has gone. A window of an account enrolled again meanwhile would fail to
make its profile, which the model's rule of a window waiting while its store is deleted
covers only while the ask is open, so the Windows app has to keep such a window from
making its profile until the folder has gone.

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
in CI run 37673429917 on 8 October 2026: windows-2025 (Windows Server 2025 Datacenter, 24H2,
build 26100, UBR 33438) and windows-11-arm (build 26200, UBR 9457, 25H2). The VM session's
block A1, every other kind of token, and A2, the build against `winver`, are not measured
yet. Until they are, the Windows face reads a token as Microsoft documents it, and these are
the only tokens it has been seen to read.

- The job's user has a token whose `TokenElevationType` is the default one, a token that is
  not split, and that is elevated, at high integrity, on both images. Files it makes are
  owned by Administrators. So every program the job starts is elevated, and Pitboard reads
  it as `token::IN_EVERY_PROGRAM`.
- A Safer normal-user token computed from the job's token, with `SaferComputeTokenFromLevel`
  at `SAFER_LEVELID_NORMALUSER`, is elevated still, at high integrity, and so is a program
  started with it. It cannot stand in for a person on the runners.
- A local standard user made in the job, and started with `Start-Process -Credential
  -LoadUserProfile`, its output sent to files, runs an unsigned program on both images,
  exiting 0, never 0xC0000142. Its token is the default type, not elevated, at medium
  integrity, so Pitboard reads it as the person. Started without `-Environment`, the
  program gets the job user's variables; in the earlier run 37660991344 that left the
  user's local app data naming the job user's folder, which it cannot open, and
  `SHGetKnownFolderPath` failed. Given its own profile's `USERPROFILE`, `HOMEDRIVE`,
  `HOMEPATH`, `APPDATA`, `LOCALAPPDATA`, `TEMP`, `TMP` and `USERNAME`, every known folder
  reads as its own. It can read and run what the job built in the workspace. This is how CI
  runs every Windows test, through `.github/scripts/test-as-standard-user.ps1`.
- Developer Mode is on on both images, and that user makes file and directory symbolic
  links as it is. It makes them with the symbolic-link right granted too, but Developer
  Mode was still on for that run, so the right alone is not measured; the script grants it
  only where Developer Mode is off, as Microsoft documents the right.
- `RtlGetVersion` gives 10.0.26100 and 10.0.26200, the builds the registry's `CurrentBuild`
  gives. The registry's `ProductName` says `Windows 10 Enterprise` on windows-11-arm, so
  Pitboard never reads which Windows this is from it.
- Defender's real-time protection is off on windows-2025 and on, with behaviour monitoring
  and tamper protection, on windows-11-arm. winget is on windows-2025 alone, and Scoop on
  neither.
- As the job's user, in an interactive logon, `credman-names` lists the prefixes of every
  live login family and `pitboard-*` without an error, and finds no item, on both images.
  As the fresh standard user, after each Windows test run, it does the same and finds
  nothing, on both images (run 37762408901). Neither image holds Codex's
  `%ProgramData%\OpenAI\Codex`, Claude Code's `C:\Program Files\ClaudeCode` or its
  policy keys.

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
  about 33 seconds.
- Every write of the login takes proper-lockfile's directory lock at
  `<storage dir>/.storage-write`: stale after 15000 ms, ten retries, 100 ms to 1000 ms of
  backoff. `lock.rs` carries the same numbers.
- Every write under that lock drops the read cache, reads the login again inside the lock,
  and abandons the write when that read fails. A stale account cannot be written back. From
  2.1.281, a locked keychain counts as a failed read here once the process has seen its
  item; before, it read as empty.
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
  keychain signs in with it. `doctor` names it as `fallback_login`, and every change to a
  Claude Code account warns while it is there.
- While the keychain is locked, a session keeps serving the login it last read, cached again
  every 30 seconds, and follows no switch. One that has read none reads the keychain as
  empty and signs in with the file, or is signed out.
- The supervisor daemon records itself in `<config dir>/daemon.lock`, with its pid and the
  Claude Code version that started it. It leaves the file behind when it stops.
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
- Claude Code's config file can be a day behind the login it describes, so Pitboard asks
  Anthropic whose a login is. This was written down on 21 September 2026, with no build
  named.
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

Not measured. Switching by itself rests on the session cache's 33 seconds. These were not
measured, and the register cannot hold them, since every fact in it is read from a build:

- Whether a request a session has under way at the moment of a switch finishes, on either
  account.
- What Claude Code does when an account reaches a limit: whether a session stops, waits or
  tries again, and whether it would take a switch made after that. Pitboard switches at a
  share under 100% so that a session following within the 33 seconds need not meet it,
  and the docs say neither is known.
- A busy session renewing its login while an automatic switch is under way, against a
  running build. The write lock and the read again under it (`write_lock`) keep the two
  apart, as for any switch. `a_login_claude_code_renews_partway_through_is_not_written_over`
  in `switch/auto.rs` plays it on the stores in memory: the switch stops with
  `signed_in_account_changed`, nothing is written over the renewed login, the account it
  would have switched to keeps its parked login, and the next attempt, a minute on,
  switches.

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
- A Codex refresh already under way when the file changes writes its own account's tokens
  under whatever account id it finds there.
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
- The ID token names the account: `email`, and under `https://api.openai.com/auth`,
  `chatgpt_account_id` and `chatgpt_user_id`. A Team or Business workspace shares one
  `chatgpt_account_id`, and `chatgpt_user_id` is the person.
- Pitboard identifies a Codex account by that pair, with no network call.
- Renewal is `POST https://auth.openai.com/oauth/token` with a JSON body
  `{client_id, grant_type, refresh_token}` and client id `app_EMoamEEZ73f0CkXaXp7hrann`.
  Each token in the answer is written only if present.
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
Nothing here ran on Windows.

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

Measured on macOS 27.0 with Swift 6.4 of Xcode 27.1, on 6 October 2026.

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

### The model's timers

Read on 5 October 2026 in Rust 1.98.1's standard library and in the macOS SDK of Xcode 27.1,
with Swift 6.4.

- `std::time::Instant` reads `CLOCK_UPTIME_RAW` on Apple's systems, which macOS's
  `clock_gettime(3)` says does not increment while the system is asleep. The model's timers
  run on it, so on macOS they count only the time the machine is awake.
- The Swift model's loops slept with `Task.sleep(for:)`, whose clock is `.continuous` unless
  it is given another, and `ContinuousClock` does not stop while the system is asleep. After
  a long sleep its five-minute read came as the machine woke, where the model's comes up to
  five minutes of waking time later. An app sends `Intent::Woke` as the machine wakes, which
  reads at once: the macOS app sends it on `NSWorkspace.didWakeNotification`, where the
  Swift model read.
- The 30 seconds an app is given to quit run on `Instant` too, where the Swift model's ran
  on `ContinuousClock`: a machine put to sleep while an app is asked to quit gives it its
  30 seconds of waking time, where the Swift gave it none once the machine woke.
- On Linux the standard library reads `CLOCK_MONOTONIC`, and on Windows
  `QueryPerformanceCounter`. How either counts a sleep was not read.

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
  The CI job for C# checks the Linux library the same way, and `build-app.sh` the macOS
  library and app.
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
    one thing is the world's own instead: personal's parked login lapses in two days, so
    that Renew Now renews it, where the Swift fixture's lasted eleven. Renew Now finds a
    parked login due that the read before it left alone only while its refresh token lapses
    within three days, and that is when doctor warns of it, since both ask
    `doctor::renewal_due`. So oneTool says "One thing is worth looking at." as before, of
    personal's parked login rather than of daily renewal. stuck has doctor's "interrupted
    switch" worth looking at besides, so it says "2 things are worth looking at.".
  - A read lists the account in use first, and the read of what is known lists only
    enrolled accounts, so a login nobody has named is listed once the first read has asked
    its service whose it is.
- stuck shows the notice offering Give Up… as the app starts, as the Swift fixture did, now
  from the core's own read: a read says an interrupted switch is waiting wherever the next
  change would be refused over it, as a warning with the refusal's code,
  `recovery_undetermined`, and its words. The read of what is known says nothing of it
  there, since only Anthropic could say whose Claude Code's login is.
