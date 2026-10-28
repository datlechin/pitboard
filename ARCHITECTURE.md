# Architecture

This file maps Pitboard's code and the rules it keeps. What the code rests on about Claude
Code, OpenAI's Codex CLI and Claude Desktop is in each tool's [register](#tool-registers), and
what it rests on about the systems, libraries and sites around it is in
[docs/facts](docs/facts/README.md). How a module works is in its own header. This file changes
when the layout changes, not with each commit.

## Bird's eye view

Pitboard switches Claude Code, Codex and, on macOS, Claude Desktop between a person's own
accounts on one machine. It also shows how much of each Claude Code and Codex account's
limits is left. A switch parks the login in use and puts another account's parked login in
its place. For Claude Code and Codex a login is a document in a store. For Claude Desktop it
is items of Claude's data folder and keys of its `config.json`, moved while Claude is
closed. A person asks for each switch, unless they asked Pitboard to switch Claude Code by
itself before the account in use runs out: an app with its setting on, or `pitboard watch`.

One crate, `pitboard-core`, does this for every tool. It reads and writes each tool's login,
keeps Pitboard's index of accounts, and asks Claude Code's and Codex's services for usage.

Three front ends use the core: the command line, `pitboard`; the menu bar app on macOS 14 or
later; and the Windows app in the notification area. The apps call the core through UniFFI
bindings, and show and send what one model in Rust decides. The command line inside each app
is what its daily renewal runs, and what the `pitboard-app` cask and the MSI put on `PATH`.

Each app also gives each enrolled Claude Code and Codex account a window on its tool's site,
claude.ai or chatgpt.com, where the site's own pages run in WebKit on macOS and WebView2 on
Windows. The windows take the list of accounts from the core, and nothing else.

Pitboard has no server of its own. The core sends requests only to Anthropic, for Claude
Code, and to OpenAI, for Codex. It sends none for a Claude Desktop account. An account's
window loads its site, and whatever the site's pages load, as a browser would.

## Code map

`crates/pitboard-core`, the engine, which the front ends reach through `service::Pitboard`
built with a `context::Context`:

- `service.rs`: `Pitboard`, each operation run as every front end must, settled first, then
  audited.
- `context.rs`: everything the core takes from its environment, read in one place.
- `app.rs`: what an app finds for itself that a terminal is given: each tool's program, the
  `pitboard` a terminal runs, and the app's own files in Pitboard's directory.
- `provider/`: the boundary with a tool's login, `Provider` for a document in a store and
  `Folder` for an app's files, and `provider::of`, which says which a tool keeps.
  - `provider/claude/`: Claude Code's stores, slots, config, Credential Manager pieces,
    requests to Anthropic and headless warm-up run.
  - `provider/codex/`: Codex's stores and the configuration layers that choose one, where a
    running `codex` can be, and its requests to OpenAI.
  - `provider/desktop/`: Claude Desktop's data folder, `config.json`, session cookie and
    processes.
  - `provider/<tool>/assumptions.rs`: each tool's [register](#tool-registers).
  - `printed.rs`, `names.rs`, `jwt.rs`: what a terminal makes of a tool's output, the names a
    real login may be kept under, and a token's claims, never checked.
- `holder.rs`: what keeps a tool's login in memory while it runs.
- `host/`: the machine behind one seam, the `Host` trait a test replaces with `memory.rs`, and
  the `fs`, `proc` and `user` functions each system answers.
  - `host/macos/`, `host/linux/`, `host/unix/`: the keychain through `security`, `ps`,
    launchd, SQLite and managed preferences; `/proc` and systemd; and what both POSIX systems
    share.
  - `host/windows/`: Credential Manager, DPAPI seals, Task Scheduler, processes, tokens, jobs,
    the registry and files made private by their access list.
  - `acl.rs`, `owner.rs`, `replace.rs`, `token.rs`, `task.rs`: Windows' rules, compiled on
    every system so every system's tests read them.
  - `program.rs`, `administered.rs`: finding a program as the system's launcher does, and what
    an administrator set outside every home.
- `release.rs`: whether this build may do anything on its system.
- `home.rs`: Pitboard's own directory, and what every home must be.
- `permit.rs`: the gate every change passes, and the `Permit` only it makes.
- `atomic.rs`: a file replaced whole from a private temporary.
- `store/`: a login read and written in each store: a file, the vault, the folders under
  `parks` (`folder.rs`, the only code that touches them) and the stores in memory.
- `park.rs`, `pending.rs`: where a parked login waits, and the names a login is about to be
  written into.
- `state.rs`: `state.json`, the index of accounts ([The state file](#the-state-file)).
- `in_use.rs`: whose login each tool has stored, as its service last said.
- `label.rs`: what somebody typed, as one account, and the name an account gets where nobody
  typed one.
- `switch/`: every change to the index: switching, enrolling, adopting, renaming, forgetting,
  renewing, repairing, abandoning, uninstalling and putting away a login left in a file.
  - `journal.rs`, `settle.rs`: an interrupted switch finished, given up or said.
  - `identify.rs`, `refresh.rs`: whose login a tool has stored, and a Claude Code login
    renewed where Claude Code keeps it.
  - `auto.rs`, `warm.rs`, `stow.rs`: the automatic switch, the warm-up and putting away, under
    the lock every change takes.
  - `switch/folder/`: every change to a tool whose login is its app's files.
  - `crash.rs`, `harness.rs`, `two_tools.rs`, `pinned.rs`, `refusals.rs`, `foreign.rs`: the
    crash matrix and the changes' tests.
- `lock.rs`: Claude Code's locks around its login, taken as it takes them.
- `duty/`: what a front end does by itself: how often (`cadence.rs`), what it tried and said
  (`ledger.rs`), the automatic switch's rule (`autoswitch.rs`) and the warm-up (`warm/`).
- `usage/`: what is left: one reading per account (`readings.rs`), pace (`pace.rs`) and how
  often Anthropic is asked (`budget.rs`).
- `status.rs`, `statusline.rs`, `sessions.rs`, `schedule.rs`, `watch_service.rs`: the commands
  of those names, each session's last run, daily renewal and the watch service.
- `doctor/`: `pitboard doctor`, a module per concern, in the order `mod.rs` makes the checks,
  said by `words.rs`.
- `settings.rs`: what a Claude Code session here would sign in as, from its settings and
  environment.
- `api.rs`, `proxy.rs`: the agent every request goes out on, and the proxy it is given.
- `words.rs`, `time.rs`, `error.rs`, `warning.rs`: what Pitboard says in more than one place,
  each a function of typed values, and every error and warning with its code.
- `audit.rs`, `redact.rs`: the activity log, and a diagnosis made safe to paste.
- `assumptions.rs`: how a register is written.
- `stand_in.rs`, `desktop_testing.rs`, `fault.rs`, `helper.rs`: for tests alone: the stand-in
  program, the Claude Desktop the tests play, crash points and a test run as another process.

The other crates:

- `crates/pitboard`: the command line. `main.rs` holds the commands, `render/` what each says
  to a person, `ui.rs` its styles and column widths, `json.rs` the one writer of `--json`,
  `manpage.rs` the man page, and `watch.rs` and `warm.rs` their commands. `build/` is a
  Windows build's manifest and version resource, which the FFI crates include too.
  `tests/cli/` is its one integration test binary, with the `--json` snapshots, and its
  example `stand-in` the program every test starts in place of a tool.
- `crates/pitboard-model`: the model both apps run on, over the core, and what their UniFFI
  bindings export.
  - `lib.rs`: what the bindings export beside the model, `Tool` and `AccountKey` among them,
    and the free functions an app calls as it starts.
  - `launch.rs`: `AppCore`, the core the model's lanes call, which nothing exports.
  - `model/`: the app model, `PitboardModel`: its actor and `State::apply` (`state.rs`), its
    lanes, advice, preferences and the account windows' bookkeeping. Its tests are files by
    subject, `switching.rs`, `signing.rs` and the rest, and `threaded.rs` over the real core.
  - `present/`: each `Snapshot`, every sentence and row an app shows; `words/` what both
    apps say and the command line does not, a catalogue with a file per surface: the status
    item, the accounts, notices, sheets, setting up, Settings and Activity (`machine.rs`),
    Diagnostics (`checks.rs`), the account windows, failures and the core's refusals, and in
    `mod.rs` the tools every one words with.
  - `account_windows/`: the rules of an account's window that each app's web code asks.
  - `sites.rs`: `pitboard-sites`' sites as records of this crate's own.
  - `fixture/`: the worlds a debug build of either app launches into, behind the `fixture`
    feature.
- `crates/pitboard-ffi`: the static and the dynamic library the two apps link, which hold
  `pitboard-model` and nothing of their own (`lib.rs` says why).
- `crates/pitboard-sites`: the sites an account's window opens and what a link from outside
  may be (`site.rs`, `link.rs`), the Pitboard link (`handoff.rs`) and a link read as
  Foundation reads one (`address.rs`, `web.rs`).
- `crates/pitboard-share-ffi`: `pitboard-sites` as bindings for the macOS Share extension and
  `PitboardShare.exe`: `share_link`, and `share_words`, every sentence they show beside a
  refusal's own.
- `crates/uniffi-bindgen-swift`, `crates/uniffi-bindgen-csharp`: the bindings' generators, at
  exactly the UniFFI version the libraries use.
- `crates/pitboard-conformance`: checks a tool's register against a build of the tool.
- `crates/pitboard-probe`: the Windows measurement probe, never released. `win/` measures, and
  the rest is tested on every system.

The apps:

- `apps/macos/`: the menu bar app, a Swift package its tests load without starting it, and
  `project.yml`, which XcodeGen makes `Pitboard.xcodeproj` from.
  - `Sources/PitboardBindings`, `PitboardShareBindings`: the two libraries' bindings.
  - `Sources/PitboardKit`: `AppModel`, the model and its newest snapshot on the main actor.
  - `Sources/PitboardLinkTarget`: where a Pitboard link goes, for the app and the extension.
  - `Sources/PitboardGlance`: the glance and the account Shortcuts and widgets know, the App
    Group it is kept in and the widgets' and control's buttons, no Rust.
  - `Sources/PitboardWidgetViews`: what the widgets draw and their timeline, no Rust.
  - `Sources/PitboardApp`: the views (`MenuBar`, `Window`, `Sheets`, `Settings`, `Design`),
    the account windows (`AccountWindows`), the launch (`App`), what the model asks of macOS
    (`System`), what the Shortcuts actions do (`Intents`) and the fixtures' stand-ins
    (`Fixture`).
  - `App`, `ShareExtension`, `Widgets`, `UITests`: the app's target with its Shortcuts
    actions and App Shortcuts, the Share extension, the widget extension with its widgets and
    control, and the UI tests.
  - `scripts/`: the libraries' build, the app's build, which fails a bundle that breaks a
    rule below, its build number and the docs' pictures.
- `apps/windows/`: the Windows app.
  - `Pitboard.Core`, `Pitboard.Share.Core`: the two libraries' C# bindings, generated into
    `Generated/`, the second with what `PitboardShare.exe` does with a share, and their tests.
  - `Pitboard.Windows`: the glue, no XAML: `AppModel`, the icon, notifications, the Jump List,
    the other apps, updates, start at sign-in and the account windows' WebView2 code.
  - `Pitboard.Views`: every window and page, in WinUI.
  - `Pitboard.App`: `Pitboard.exe`, its composition root, and `--unregister`.
  - `Pitboard.Share`: `PitboardShare.exe`, the share target.
  - `Pitboard.Installer`: the MSI, built with WiX, and `Pitboard.Installer.Tests`, which reads
    its tables.
  - `Pitboard.Windows.Tests`, `Pitboard.UITests`: the app's tests, and the UI tests that drive
    the built app.
  - `Pitboard.Probe`: the session machine's measurements of the app's adapters, never shipped.
  - `Icon`: the drawings `Pitboard.ico` is made from.

The rest:

- `packaging/`: the casks, the Scoop manifest and `install.sh`, which a release fills.
- `docs/`: the source of docs.usepitboard.com, and `docs/facts`, kept off the site.
- `website/`: the usepitboard.com site.
- `.github/workflows/`: `ci.yml` for every push and pull request, `release.yml` for a `v` tag,
  `conformance.yml` for the registers, `windows-facts.yml` for the Windows runners and the
  session bundle, `rotation.yml` for the update key's rehearsal, and `sparkle.yml` and
  `winsparkle.yml` for new releases of the updaters and the Windows App SDK.
- `.github/actions/`: the steps the workflows share.
- `.github/scripts/`: what the workflows run, and its tests, `test-*.py`,
  `test-windows-app.ps1` and `test-windows-release.ps1`.

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
- Pitboard's own directory is never in a folder that syncs. `home::check_location` refuses one
  by the names sync clients give their folders and, on Windows, a sync root or a roaming
  profile's roaming part (`a_sync_clients_folder_is_refused_by_its_name_in_any_case`,
  `a_windows_folder_that_syncs_or_roams_is_refused`).
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

- Nothing outside `pitboard-core` writes Pitboard's index (`state::save` is `pub(crate)`).
  Every change goes through `switch`, which records what it is about to do first.
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
  on or `pitboard watch`, and switches Claude Code alone, which has no app to quit
  (`nothing_is_switched_by_itself_unless_somebody_turned_it_on`, `codex_is_never_switched`).
  Only those two call `Pitboard::auto_switch`, so the status line and daily renewal never
  switch.
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
  for tests (`a_test_never_reaches_the_systems_own_scheduler`). The Release build
  `ReleaseGateTests` starts at 1.x only reads Task Scheduler and writes no task.
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
- Nothing that ships is built for tests. `build-app.sh` fails a bundle whose core library,
  app or command line holds a fixture world's text or `PITBOARD_TEST_CLAUDE_DESKTOP`, or links
  a SQLite other than the system's, and `check-windows-build.ps1` and `check-windows-app.ps1`
  fail a Windows build that does the same, or that was built with `test-support`, `fixture` or
  `--cfg pitboard_unreleased_windows`.

### The apps

- Nothing `pitboard-model` exports is async or waits on the core: only the model's lanes call
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
- An account the app adds is enrolled as its sign-in ends, under a name the core picks under
  `state.lock`, and named after, so quitting before the name keeps it
  (`quitting_before_the_name_is_saved_keeps_the_account`).
- What the app says by itself is said once: a run-out once per reset across launches, kept in
  `told.json`; a replaced login once per account while the reads say it; a refusal of the
  automatic switch once until it next switches (`model/advising.rs`, `model/threaded.rs`,
  `model/automatic.rs`).
- The app's own preferences are the model's, in `app.json` in Pitboard's directory. A file
  that cannot be read is left as it is and never taken for a first launch, and the macOS
  app's UserDefaults are handed over once (`model/keeping.rs`).
- With its setting on, the app asks the core's look after every read and every
  `DECIDE_EVERY_SECONDS`, as `pitboard watch` decides, paces a refusal by
  `duty::cadence::retry_after`, and never says what it came to in an alert. Its Switch Back is
  a plain switch, taken only while the account it switched to is still in use
  (`model/automatic.rs`).
- A notice's button is a change of the app's own, never a switch: updating Claude Code's
  config is refused where another account is in use by then, and **Put Away** puts away only
  the file its sheet looked at (`model/changing.rs`, `model/stowing.rs`).
- Uninstalling from the app stops what runs by itself and closes every account window, and asks
  the core only once nothing that writes Pitboard's directory is under way, holding the app's
  `app.lock` until the directory goes. Nothing writes it after, and a refusal before anything
  went starts the model again (`model/removing.rs`,
  `after_uninstall_nothing_writes_pitboards_directory_again`).
- The glance for widgets and intents names each account by an id derived from its own, never
  by an email, a token, a uuid, a path or a notice's lines. The model hands it to the app before
  the snapshot it changed with, writes it only where the app names a file, again with each
  read, and deletes that file once the app uninstalls Pitboard
  (`the_glance_is_given_to_the_app_with_or_without_a_file`,
  `an_uninstall_deletes_the_glance_and_tells_the_widgets`). `glance_revision` moves only with
  what a widget shows, since WidgetKit budgets reloads, and once the file holds it. A
  request from outside does no more than a click in the app and waits for the launch's first
  read. One that would quit an app answers with the quit question and quits it only when asked
  again with `QuitAndSwitch`, and one that needs another question only the app asks is
  refused in words (`model/glancing.rs`).
- The model names a tool and an account by type: the core's `ProviderId` and `Key` inside, and
  its own `Tool` and `AccountKey` to an app. What crosses a process stays text: a
  notification's `clicked` argument, read back as the core reads an account typed, and the ids
  of the glance and of a request from outside. The one conversion out of the core's
  `non_exhaustive` `ProviderId`, into `Tool`, is the model's only arm for a tool it does not
  know (`every_tool_the_core_knows_is_one_the_apps_name`).
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
- A macOS debug build started without `PITBOARD_FIXTURE` quits before anything of the app
  starts, since Shortcuts, Spotlight, a widget or a control start it through Launch Services
  on the real home (`aDebugBuildStartedOutsideAFixtureStartsNothing`).
- The macOS app's Shortcuts actions run in the app, over its model, and the release holds
  the metadata Shortcuts and Spotlight read them and their App Shortcuts from
  (`scripts/build-app.sh`).
- The Windows app's own data folder is `Pitboard App` in the known folder Local AppData, never
  in Pitboard's directory, so `pitboard uninstall` never deletes the account windows'
  sign-ins (`host::pitboard_app_on_windows`).
- Only a Release build an MSI installed registers anything with Windows, and
  `Pitboard.exe --unregister` takes back what it registered. CI's Windows legs fail a run that
  leaves a link scheme, a notification registration, a start at sign-in, a `PATH` entry or a
  package behind.
- Only `PitboardShare.exe` has a package identity, which `check-windows-app.ps1` holds. It
  loads none of the core and hands a page to `Pitboard.exe` only as a Pitboard link
  ([Documented, not measured](docs/facts/windows-documented.md) has what is still to be seen).
- The Windows Jump List is Win32's, kept under the AppUserModelId with no package identity.
  Each task starts `Pitboard.exe` with a flag and the GUID the model gave it, read only where a
  link would be, and its switch is a request from outside (`LaunchingTests`, `model/jumping.rs`).
  Only a Release build writes the list, and `--unregister` deletes it.

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
  window and never switches an account, a site's sign-in link is refused, and only a person's
  choice opens a window, on the link's own site (`a_sign_in_link_is_refused_from_outside`,
  `a_pitboard_link_never_switches_an_account`).
- A window's page goes only to its site, its sign-in hosts and blank pages. Google's sign-in is
  refused, other pages go to the browser, a frame gets no sign-in window, and a window's last
  page is never a sign-in path (`account_windows/policy.rs`).
- The Pitboard link's format is a contract pinned by `handoff.rs`'s tests, and a release
  claims `pitboard://` and a debug build `pitboard-debug://`, which `build-app.sh` holds. On
  Windows only an installer registers `pitboard`, and a Debug build outside a fixture refuses
  a `pitboard-debug` link (`LaunchingTests`).
- The Share extension stays sandboxed and links `pitboard-share-ffi`, never the core: the
  core's bindings check every export as they load, and two Rust static libraries never meet in
  one binary. `build-app.sh` fails a signature that is not sandboxed and either binary that
  holds the other library's symbols.
- The widget extension stays sandboxed, links no Rust and reads only the glance, which holds
  no secret. Only a build signed by a team claims the App Group `<team>.com.usepitboard.Pitboard`,
  so an ad hoc build touches no group container and macOS never asks about one
  (`aBuildWithNoTeamKeepsOutOfEveryGroup`, `scripts/build-app.sh`). The widgets and the
  control reload only with `glance_revision`
  (`widgetsAndControlsReloadOnlyAsTheGlanceTheyShowMoves`). A widget's Switch, on macOS 27
  and from Xcode 27, runs in the app alone, and asked of the extension it switches nothing
  (`aSwitchTheWidgetExtensionIsAskedForSwitchesNothingAndSaysWhere`).
- `pitboard-sites` stays a leaf: no I/O, nothing of the core and no UniFFI (its `Cargo.toml`
  depends on `url` alone). Each binding crate declares its own types over it, since the C#
  generator cannot use another crate's.
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
- Each app and the core meet at the model. The app has no rule of its own for what the core
  decides: its homes, Pitboard's directory, whether a path is a program, the sites and the
  links it opens are the core's, and every word and button a view shows is the model's.
  `AppControl` names an app by the id holder detection gives it.
- Pitboard and each tool meet at the tool's [register](#tool-registers).
- Pitboard and each service meet at a few requests, which
  [What leaves your machine](https://docs.usepitboard.com/security#what-leaves-your-machine)
  lists with what each carries.
- Pitboard and claude.ai or chatgpt.com meet at an account's window, which a person opens. The
  site's own pages sign it in, WebKit or WebView2 keeps that sign-in in the account's store,
  and Pitboard decides only where each navigation goes. It makes, wipes and deletes a store,
  never reads what a site keeps there, adds no script, sets no user agent, and gives no window
  a Claude Code or Codex login.
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
