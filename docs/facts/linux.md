# Linux

What the core and the command line rest on about Linux and what runs there, each as it was read
or measured. Read them again before changing code that rests on them.

## XDG base folders

Read on 28 October 2026 in the [XDG Base Directory
Specification](https://specifications.freedesktop.org/basedir/latest/), version 0.8 of 8 May 2021.

- Configuration goes in `$XDG_CONFIG_HOME`, data in `$XDG_DATA_HOME` and what may be thrown away
  in `$XDG_CACHE_HOME`. Each one that is unset or empty is `$HOME/.config`,
  `$HOME/.local/share` and `$HOME/.cache`.
- "All paths set in these environment variables must be absolute. If an implementation
  encounters a relative path in any of these variables it should consider the path invalid and
  ignore it." So `Context::config_home`, `data_home` and `cache_home` take a relative one for
  unset (`a_relative_xdg_folder_is_taken_for_unset`).
- GLib does not. `g_build_user_data_dir` in GLib 2.84.0's `glib/gutils.c` takes any non-empty
  `XDG_DATA_HOME` as it is, a relative one included, and only an unset or empty one falls back
  to `.local/share`. Its config and cache folders are built the same way there.
- Claude Code reads `XDG_CONFIG_HOME` its own way, trimmed and taken where relative, for its
  Anthropic profiles (`profile_folder` in its register). `settings::Profiles` keeps that
  reading, and Pitboard's own folders keep the spec's.
- The Linux app's own folders are `com.usepitboard.Pitboard` in the data and cache folders, and
  `com.usepitboard.Pitboard.Devel` for a debug build. A build for tests refuses both in the
  account's own defaults and in the folders it was started with
  (`host::pitboard_app_on_linux`).

## The Linux app's programs

- The app's program is `pitboard-app`, installed beside `pitboard` in one `bin`: `/usr/bin` from
  the packages, Meson's `bindir` from a build. Lane C installs it under that name, and the core
  reads it there alone: `Os::app_command_line` names the `pitboard` beside a `pitboard-app`, and
  `Os::an_apps_own_program` takes a schedule that runs `pitboard-app` for one that renews
  nothing (`each_systems_app_names_the_command_line_it_comes_with`,
  `a_schedule_running_the_linux_app_is_at_fault`).

## AppImage

Read on 28 October 2026 in AppImage's runtime,
[type2-runtime](https://github.com/AppImage/type2-runtime) `src/runtime/runtime.c` at commit
`fedfad51acc2860b2916a1eef32ba4a500ff31ef` of 19 October 2026, the one its `continuous` release
is built from. No AppImage was run.

- An AppImage runs from a folder its runtime mounts, `build_mount_point`'s
  `$TMPDIR/.mount_<name>XXXXXX`: `<name>` is up to six bytes of its own file's name, and
  `mkdtemp` fills the six `X`s. `$TMPDIR` unset is `P_tmpdir`, `/tmp`. Once the AppImage's own
  process ends, the keepalive pipe it holds breaks, and the FUSE process unmounts the folder
  and removes it.
- With `--appimage-extract-and-run` or `APPIMAGE_EXTRACT_AND_RUN` set, it extracts itself into
  `$TMPDIR/appimage_extracted_<MD5 of the file, in lower-case hex>` and deletes that folder once
  `AppRun` ends, unless `NO_CLEANUP` is set.
- So a `pitboard` inside either folder is gone by the next run, and a schedule or the watch
  service refuses it with `schedule_program_temporary` (`a_temporary_copy_is_told_by_its_path`).

## Homebrew on Linux

- Its install script puts Homebrew in `/home/linuxbrew/.linuxbrew`, so its programs are in
  `/home/linuxbrew/.linuxbrew/bin`: Homebrew's `docs/Homebrew-on-Linux.md` at commit
  `5cec10029857fd8082da6b11bac5878419f4772a` of 23 September 2026, read on 28 October 2026, when
  Homebrew 7.0.11 was its newest release.
- formulae.brew.sh's JSON, read on 28 October 2026: the `claude-code` cask, 2.1.308, has
  `x86_64_linux` and `arm64_linux` variations, which download Claude Code's `linux-x64` and
  `linux-arm64` builds, and links `claude` as `$HOMEBREW_PREFIX/bin/claude`. The `codex` cask,
  0.165.0, has the same two, which download Codex's `x86_64-unknown-linux-musl` and
  `aarch64-unknown-linux-musl` packages, and links `bin/codex` as `$HOMEBREW_PREFIX/bin/codex`.
  There is no `codex` formula.
- So an app on Linux looks for `claude`, `codex` and `pitboard` in that `bin` too, after the
  login shell's `PATH` (`Os::package_bins`, `an_app_on_linux_looks_in_homebrews_bin`). Homebrew
  installed in another prefix is found only on the login shell's `PATH`.

## The login shell's descriptors

- glibc 2.34 added `posix_spawn_file_actions_addclosefrom_np`, "enabling posix_spawn and
  posix_spawnp to close all file descriptors greater than or equal to a given integer" (its
  `NEWS`, and `posix/spawn.h` at tag `glibc-2.34`, read on 28 October 2026). The libc crate
  0.2.191 binds it in `unix/linux_like/linux/gnu` alone.
- The floor's glibc, read on 28 October 2026: Debian 13 `2.41-12+deb13u4` (packages.debian.org),
  Ubuntu 26.04 `2.43-2ubuntu2.4` (Launchpad), Fedora 43 `2.42-17.fc43` (Fedora's mdapi). Each is
  past 2.34.
- musl's `include/spawn.h` at tag `v1.2.6` has no such action, only `addclose`, `adddup2`,
  `addopen`, `addchdir_np` and `addfchdir_np`.
- Neither has a `posix_spawn` flag like macOS's `POSIX_SPAWN_CLOEXEC_DEFAULT`: glibc's
  `spawn.h` at 2.34 and 2.42 and musl's at 1.2.6 define `RESETIDS` to `SETSID`, and glibc 2.42
  `SETCGROUP` too. So on glibc the login shell is started with `addclosefrom_np(actions, 3)`
  after its three are set up, and on musl with nothing more
  (`a_login_shell_is_handed_only_its_three_descriptors`).
- Only an app asks for the login shell. Linked by cargo-zigbuild 0.24.0 against glibc 2.28 on 28
  October 2026, the debug `pitboard` for `x86_64-unknown-linux-gnu` named no
  `posix_spawn_file_actions_addclosefrom_np` and no symbol version past `GLIBC_2.28`, so a glibc
  build of the command line still links before glibc 2.34.

## System Monitor

Read on 28 October 2026 in the desktop file each desktop's process monitor installs.

- GNOME's, `gnome-system-monitor`, is `Name=System Monitor` in
  `data/org.gnome.SystemMonitor.desktop.in.in` at tag `48.1` (commit
  `68dab9ea315aae5a6318153e05797657092d9d9e`) and tag `51.1` (commit
  `386dbf1754ec3d6063a0575ccc4e937aef3cc712`).
- KDE Plasma's, `plasma-systemmonitor`, is `Name=System Monitor` in
  `org.kde.plasma-systemmonitor.desktop` at tag `v6.3.6` (commit
  `be4f1594995698cac59959b8baa4421f435a1015`) and tag `v6.8.2` (commit
  `e4cd4bcdfe56eb67c08bcf42b5af708f9d96a8c2`).
- So a process left of an app is quit in System Monitor on Linux, whichever of the two runs
  (`what_is_left_of_an_app_is_quit_where_the_system_quits_it`).

## The user's systemd

Read on 29 October 2026 in systemd's source at tag `v262` (commit
`8cc40e0c5e9234bf45084751ac53b1fbfe70b492`), its man pages and the files named. No user manager
was asked: a container has none without systemd as its first process, and no build for tests
asks the person's ([not-measured.md](not-measured.md#linux)).

- `systemctl --user` reaches the manager in `bus_connect_transport_systemd`
  (`src/shared/bus-util.c`). With `XDG_RUNTIME_DIR` set, `bus_connect_user_systemd` connects to
  `unix:path=$XDG_RUNTIME_DIR/systemd/private` with `sd_bus_new`, which sends no `Hello`, and
  `bus_check_peercred` refuses a peer that is neither root nor this user. A private socket that
  does not answer is the error, with no fallback. Only with `XDG_RUNTIME_DIR` unset and
  `DBUS_SESSION_BUS_ADDRESS` set does it take the session bus (`sd_bus_default_user`).
- sd-bus finds the session bus in `DBUS_SESSION_BUS_ADDRESS`, else at
  `unix:path=$XDG_RUNTIME_DIR/bus` (`bus_set_address_user` in
  `src/libsystemd/sd-bus/sd-bus.c`), and waits 25 seconds for an answer (`BUS_DEFAULT_TIMEOUT`
  in `bus-internal.h`). So `host/linux/bus.rs` and `Context::session_bus`
  (`the_user_manager_is_reached_as_systemctl_reaches_it`,
  `the_session_bus_is_found_as_sd_bus_finds_it`).
- org.freedesktop.systemd1(5): `Manager.UnitPath`, an `as`, "encodes the currently active unit
  file search path". `Manager.Environment`, an `as` of `NAME=value`, "encodes the environment
  block passed to all executed services". In `src/core/dbus-manager.c`, `UnitPath` is
  `lookup_paths.search_path`, and `Environment` is `manager_get_effective_environment`, the
  manager's own environment merged with what clients set. A user manager starts with its own
  environment, `PATH` replaced and the session's variables dropped (`manager_default_environment`
  in `src/core/manager.c`), so the `HOME` there is the one its units run with. Pitboard asks both
  before it writes or removes a unit
  (`the_user_manager_is_asked_on_its_private_socket_for_its_folders_and_home`).
- systemd.unit(5) puts a user's own units in `$XDG_CONFIG_HOME/systemd/user`, else
  `~/.config/systemd/user`. `src/libsystemd/sd-path/path-lookup.c` builds `UnitPath` from the
  variables' text and resolves no link, and `SYSTEMD_UNIT_PATH` in the manager's environment
  replaces it. So a folder is compared by its components
  (`a_unit_is_written_only_where_the_user_manager_reads_its_folder`).
- systemd.timer(5): `OnUnitActiveSec=` and the other monotonic settings count on a clock that
  stops in suspend. `OnCalendar=` counts the wall clock, and "once the system is later resumed it
  will catch up and process all timers that triggered while the system was sleeping".
  `Persistent=` "only has an effect on timers configured with `OnCalendar=`", and its catch-up "is
  nonetheless subject to the delay imposed by `RandomizedDelaySec=`". So the timer Pitboard wrote
  before, `OnUnitActiveSec=86400` and `OnStartupSec=900` with `Persistent=true`, counted only time
  awake (`the_timer_runs_on_each_calendar_day_and_catches_up_after_sleep`).
- systemd.timer(5) asks for `systemctl clean --what=state` before a timer with `Persistent=` is
  uninstalled. `timer_clean` in `src/core/timer.c` deletes `stamp-<timer>` in
  `$XDG_DATA_HOME/systemd/timers`, else `~/.local/share/systemd/timers`, and only from a timer
  that is stopped.
- systemd.unit(5): a unit whose condition fails is skipped, "mostly silently", and not put in
  the `failed` state. `ConditionFileIsExecutable=` checks that a path "exists, is a regular file,
  and marked executable". `config_parse_unit_condition_path` in `src/core/load-fragment.c`
  expands specifiers in the value and unquotes nothing (`a_service_whose_pitboard_is_gone_is_skipped`).
- `config_parse_exec` in the same file unquotes and unescapes `ExecStart=`'s first word, then
  expands its specifiers (`unit_path_printf`), so a `%` is written `%%`. systemd.service(5): no
  variable is expanded in the program a unit runs. `string_is_safe` refuses a program that is not
  UTF-8 or holds a control character, a backslash or a quote. The unit file parser,
  `src/shared/conf-parser.c`, strips the ends of each line and joins a line that ends in a
  backslash to the next (`a_program_path_with_a_space_or_a_percent_reads_back_as_written`,
  `a_program_path_no_unit_can_run_is_refused_before_anything_is_written`).
- environment.d(5): the user manager gives its services the variables of
  `~/.config/environment.d/*.conf`, or of `environment.d` in `XDG_CONFIG_HOME` where the manager's
  own is a full path, read as it starts. `systemctl --user set-environment` sets one until it
  stops. [Fix daily renewal](../troubleshooting/renewal.mdx) names both for `schedule_manager_elsewhere`.

## zbus

Read on 29 October 2026 in each crate as crates.io serves it.

- zbus 6.0.0, of 16 October 2026, has "Remove the blocking API" in its `CHANGELOG.md`.
  zbus_systemd 0.26201.0 depends on zbus 6 and is async only. So the core takes zbus 5.19 with
  `blocking-api` and `p2p`, and its own two-property proxy for the user manager.
- In zbus 5.19.0, the blocking `Connection::peer_creds` is declared an `async fn`
  (`src/blocking/connection/mod.rs`), and `peer_credentials`, deprecated since 5.13, is its
  blocking form, which `host/linux/bus.rs` asks.
- `async-io` runs zbus's own executor, so the musl command line links no tokio:
  `cargo tree -p pitboard --target aarch64-unknown-linux-musl -e normal,build -i tokio` finds
  nothing.

## The Secret Service

Read on 29 October 2026 in the [Secret Service API](https://specifications.freedesktop.org/secret-service/latest/),
draft 0.2, and in each crate as crates.io serves it. Measured the same day in a Debian 13
container on arm64, as uid 1000 on a bus of its own, with `gnome-keyring` 48.0 and `dbus` 1.16.2,
through secret-service 5.2.0 over zbus 5.19.0, in a DH session.

- A collection's alias `default` names where an item goes unless the program says otherwise. A
  freshly unlocked login keyring is `/org/freedesktop/secrets/collection/login`, which `default`
  names, beside a `session` collection that is kept only in memory. With no login keyring made,
  `ReadAlias("default")` is `/`, which secret-service reads as `NoResult`, and only `session` is
  there. So a park goes in the default collection alone, and a new home takes the Secret Service
  only where one answers with a default collection (`secret_service::answers`).
- An item's attributes are what was written, `xdg:schema` and `service`, whether the collection
  is locked or not, and its label reads back. Its content type does not: written
  `application/json`, it reads back `text/plain`. A `CreateItem` with `replace` and the same
  attributes replaces the item at the same path, with a warning on gnome-keyring's standard
  error (`a_park_round_trips_as_one_item_of_pitboards_schema`).
- A search by `xdg:schema` finds every item of the collection with it, whatever home wrote it,
  and nothing of another schema (`the_list_holds_every_park_on_the_bus_and_nothing_of_another_schema`).
- With nothing on the bus owning `org.freedesktop.secrets`, opening a session fails with
  `org.freedesktop.DBus.Error.ServiceUnknown` within 30 milliseconds. secret-service 5.2.0
  turns only an address or connection error into its `Unavailable`, so this one is a zbus
  method error, which the store takes for `Locked`
  (`no_secret_service_on_the_bus_is_locked_never_absent`).
- A collection locked through `Lock` keeps its items' attributes readable and lists them as
  locked; `GetSecret` and `CreateItem` fail with `org.freedesktop.Secret.Error.IsLocked`. A login
  keyring locked since gnome-keyring started, started again without `--unlock`, reports
  `Locked` true and answers every search, the service's and the collection's, with nothing. So
  the store asks the collection's `Locked` before and after each question, and a locked one is
  `Locked`, never empty (`a_locked_collection_is_locked_never_absent`).
- `Unlock` of a locked collection returns a prompt. With no `org.gnome.keyring.SystemPrompter`
  on the bus, gnome-keyring says "couldn't create system prompt" on its standard error and
  completes the prompt dismissed within 90 milliseconds, which secret-service reads as `Prompt`
  (`a_prompt_nobody_can_answer_is_refused_within_seconds`). A run the schedule or the watch
  service started calls neither `Unlock` nor `Prompt` (`a_run_a_job_started_never_asks_to_unlock`).
- secret-service is taken over oo7 0.6.0, whose `rust-version` is 1.92, past the core's 1.91, and
  which is async only and always pulls ashpd, and over keyring 4.2.0, whose crate docs tell an
  application that controls which store it uses not to link it, and whose `Entry` hides the
  attributes a list of parks searches by. Its crypto is pure Rust (aes 0.9, cbc 0.2, hkdf 0.13,
  sha2 0.11), so the musl command line links it, and it rides on the core's zbus 5 with
  `async-io`: `cargo tree -d -i zbus` names one zbus.
