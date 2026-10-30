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
- Read on 29 October 2026 in formulae.brew.sh's JSON and Homebrew 7.0.11's
  `Library/Homebrew/cask/artifact/generated_completion.rb`: the `claude-code` cask, 2.1.308,
  downloads its Linux builds from `https://downloads.claude.ai/claude-code-releases/`, and the
  `codex` cask's `generate_completions_from_executable` stanza runs `bin/codex completion` for
  each shell in its `install_phase`. So `linux-facts.yml` installs neither: one loads claude.ai's
  host, which AGENTS.md keeps every script off, and the other runs Codex.

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

Read on 29 and 30 October 2026 in systemd's source at tag `v262` (commit
`8cc40e0c5e9234bf45084751ac53b1fbfe70b492`), its man pages and the files named, and measured on
the Linux session machine as the first bullet says. No build for tests asks the person's manager
([not-measured.md](not-measured.md#linux)).

- Measured on 30 October 2026 on the Linux session machine: Fedora 44 on aarch64, systemd 259.5,
  a dbus-broker session bus and GNOME 50.6, with the 1.0.0-beta.8 rpm of `linux-facts.yml`'s
  session run 40986051417. That build asked the manager on its private socket,
  `$XDG_RUNTIME_DIR/systemd/private`, as `systemctl --user` reaches it. `pitboard schedule
  install`, run as the desktop user with `XDG_RUNTIME_DIR=/run/user/1000` and
  `DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus`, wrote no unit: zbus 5.19.0's executor
  thread panicked at `src/message/fields.rs:219` with "Invalid field reconstruction:
  InvalidName("Invalid unique name. ...")", and the question timed out. The Linux app's Watch in
  the Background and Keep Saved Sign-Ins Fresh switches refused the same way.
- On the private socket systemd sends `org.freedesktop.systemd1` as the sender of each message:
  `bus_on_connection` in `src/core/dbus.c` sets it with `sd_bus_set_sender`. zbus 5.19 reads a
  sender only as a unique name (z-galaxy/zbus#1497), where its maintainer says zbus 6 refuses
  such a message in place of panicking, and systemd/systemd#3610 says the private socket is not
  an API for other programs. So Pitboard never connects to it
  (`the_user_manager_is_asked_on_the_session_bus_for_its_folders_and_home`).
- `sd_bus_open_user` (`src/libsystemd/sd-bus/sd-bus.c`) connects as a bus client to
  `DBUS_SESSION_BUS_ADDRESS`, else to `unix:path=$XDG_RUNTIME_DIR/bus` (`bus_set_address_user`),
  and waits 25 seconds for an answer (`BUS_DEFAULT_TIMEOUT` in `bus-internal.h`). The user
  manager joins that bus with the same call in `bus_init_api`, and takes
  `org.freedesktop.systemd1` there in `bus_setup_api` (`src/core/dbus.c`), once its
  `dbus.socket` and `dbus.service` both run (`manager_dbus_is_running` in `src/core/manager.c`).
  So `host/linux/bus.rs` asks the manager there, on the bus `Context::session_bus` finds
  (`the_session_bus_is_found_as_sd_bus_finds_it`).
- systemd installs `org.freedesktop.systemd1.service` only among the system bus's services
  (`src/core/meson.build`), so on a session bus socket activation has just started nothing owns
  the name until the manager joins. dbus-daemon at tag `dbus-1.16.2`, run with
  `--systemd-activation` as its user `dbus.service` runs it (`bus/systemd-user/dbus.service.in`,
  shipped so by Debian 13 and Ubuntu 26.04), holds a call to the name until systemd connects
  (`bus_activation_activate_service` in `bus/activation.c`). dbus-broker at tag `v37`, Fedora's,
  refuses it at once as not activatable (`driver_forward_unicast` in `src/bus/driver.c`). So
  Pitboard waits up to
  5 seconds for the name to have an owner, and past that says the manager was not reached
  (`a_user_manager_that_joins_the_bus_late_is_waited_for`).
- A user manager listens on the session bus's socket from its start, lingering with no session
  or not: `dbus.socket`, `ListenStream=%t/bus`, is wanted by `sockets.target`, which systemd's
  user `basic.target` wants and `default.target` requires. Its `ExecStartPost=` sets
  `DBUS_SESSION_BUS_ADDRESS` in the manager's environment, so a unit's run is given the bus too.
  Read on 30 October 2026 in each distribution's aarch64 packages:
  - Debian 13: `dbus-user-session` 1.16.2-2 links it into `sockets.target.wants`. It is the
    only `default-dbus-session-bus`, `libpam-systemd` 257.13-1~deb13u1 recommends it, and
    `gnome-session-bin` 48.0-1+deb13u1 and `plasma-workspace` 4:6.3.6-2 depend on
    `default-dbus-session-bus | dbus-session-bus`.
  - Ubuntu 26.04: `dbus-user-session` 1.16.2-2ubuntu4, of priority important, ships the same
    files, and `libpam-systemd` 259.5-0ubuntu3 recommends it.
  - Fedora 44: `dbus-common` 1.16.2-1.fc44 has it `WantedBy=sockets.target`, and
    `fedora-release-common` 44-17's `90-default-user.preset` enables it and
    `dbus-broker.service`, which is `Alias=dbus.service`. `dbus-broker` 37-8 requires
    `dbus-common`, and the specs on each package's `f44` branch have `dbus` require
    `dbus-broker` where systemd is installed and `systemd` recommend `dbus`.
- On Debian 13, a system whose session bus is `dbus-x11`'s, which provides only
  `dbus-session-bus`, or one installed without recommends, has no `dbus.socket`: a lingering
  manager there has no session bus, and Pitboard reaches no user manager.
- Where `DBUS_SESSION_BUS_ADDRESS` names a bus the manager is not on, as one `dbus-run-session`
  or `dbus-launch` starts, sd-bus looks no further, and neither does Pitboard. `systemctl
  --user` reaches the manager there all the same, on its private socket.
- org.freedesktop.systemd1(5): `Manager.UnitPath`, an `as`, "encodes the currently active unit
  file search path". `Manager.Environment`, an `as` of `NAME=value`, "encodes the environment
  block passed to all executed services". In `src/core/dbus-manager.c`, `UnitPath` is
  `lookup_paths.search_path`, and `Environment` is `manager_get_effective_environment`, the
  manager's own environment merged with what clients set. A user manager starts with its own
  environment, `PATH` replaced and the session's variables dropped (`manager_default_environment`
  in `src/core/manager.c`), so the `HOME` there is the one its units run with. Pitboard asks both
  before it writes or removes a unit
  (`the_user_manager_is_asked_on_the_session_bus_for_its_folders_and_home`).
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
- `unit_file_mask` in `src/shared/install.c` links a unit to `/dev/null` in the user's own
  folder, `$XDG_CONFIG_HOME/systemd/user`, or with `--runtime` in
  `$XDG_RUNTIME_DIR/systemd/user`. `user_unit_search_dirs` in `path-lookup.c` lists the own
  folder before the runtime one, and `unit_file_build_name_map` in `src/shared/unit-file.c`
  takes each name from the first folder that holds it. So a mask made with `systemctl --user
  mask` is the file Pitboard writes the timer over, and one made with `--runtime` leaves the
  timer Pitboard wrote in force. `linux-facts.yml` measures both, and has a start refused by a
  drop-in with `RefuseManualStart=yes` instead (systemd.unit(5)).
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

## The graphical session's target

Read on 29 October 2026 in systemd's man pages and units at tag `v262` (commit
`8cc40e0c5e9234bf45084751ac53b1fbfe70b492`), gnome-session at tags `48.0` (commit
`d95cf6aa0fcbdc7f4d6fe1e4a2156f9822c99952`), `50.1` (commit
`cd403186acf6f0ca866d014fd76f6456b4f46884`) and `51.0` (commit
`aed00cf18229a98231c23e14f5798ddb918c4956`), and plasma-workspace at tags `v6.3.6` (commit
`63e4bad2fe4bfc2b8c42f6347f61d2418cc7143c`) and `v6.8.2` (commit
`30beb0095873364c6b34ebcfd292ecafd4db7f61`). No session was run.

- systemd.special(7) lists `graphical-session.target` among the special passive user units: it
  "is active whenever any graphical session is running", a target for a particular session
  "starts and stops `graphical-session.target` with `BindsTo=graphical-session.target`", and a
  service that "can be enabled independently" is started with it through a link in its
  `.wants/`. So renewal's service is `WantedBy=graphical-session.target`, which `systemctl
  enable` links (`the_renewal_service_also_runs_at_each_graphical_sign_in`).
- systemd's own `units/user/graphical-session.target` has `RefuseManualStart=yes` and
  `StopWhenUnneeded=yes`, so only a session's target starts it: no runner or container can start
  it with `systemctl --user start`.
- systemd.unit(5): `After=` orders two units only where both are being started, so a run the
  timer starts while no session is starting waits for none.
- gnome-session's `data/gnome-session.target` has `BindsTo=graphical-session.target` and
  `Before=graphical-session.target` at all three tags, and `gnome-session-shutdown.target` has
  `Conflicts=graphical-session.target`. So a GNOME sign-in starts the target once GNOME's own
  target is up, signing out stops it, and the next sign-in starts it again. At `51.0`,
  `gnome-session-services.target` has `Wants=oo7-daemon.service`, with the comment "Secret
  service implementation", and `gnome-portals.conf` names `oo7-portal;gnome-keyring;` for the
  Secret portal. At `48.0` and `50.1` no unit names a Secret Service, and the portal is
  `gnome-keyring`.
- plasma-workspace's `startkde/systemd/plasma-workspace.target` names `graphical-session.target`
  in `Requires=`, `BindsTo=` and `Before=` at both tags, and has
  `Wants=plasma-kwallet-pam.service`. Plasma starts through systemd unless `startkderc`'s
  `[General] systemdBoot` is `false`, or `plasma-workspace.target` or
  `xdg-desktop-autostart.target` is not installed (`useSystemdBoot` in
  `startkde/startplasma.cpp` at `v6.8.2`). Started otherwise, it starts no
  `graphical-session.target`.
- So renewal runs at each sign-in to GNOME, and to Plasma started through systemd. Whether the
  login collection is unlocked by then is [not measured](not-measured.md#linux).

## zbus

Read on 29 October 2026 in each crate as crates.io serves it.

- zbus 6.0.0, of 16 October 2026, has "Remove the blocking API" in its `CHANGELOG.md`.
  zbus_systemd 0.26201.0 depends on zbus 6 and is async only. So the core takes zbus 5.19 with
  `blocking-api`, and its own two-property proxy for the user manager.
- zbus 5.19.0 reads a message's sender as a `UniqueName` (`QuickFields::sender` in
  `src/message/fields.rs`) and panics where it is a well-known name, as on systemd's private
  socket ([The user's systemd](#the-users-systemd)).
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
- secret-service 5.2.0 waits for a prompt's `Completed` signal with no bound:
  `exec_prompt_blocking` (`src/util.rs`) reads the next signal, and zbus's `method_timeout`
  covers only method calls. The prompter puts its dialog on the display of the session whose bus
  it serves. Over SSH the bus `XDG_RUNTIME_DIR` names is the desktop's where the same person is
  signed in there, so a prompt from SSH would wait for an answer at that desktop. So a run a
  person started asks only in the desktop session: where `WAYLAND_DISPLAY` or `DISPLAY` is set,
  as every Wayland and X11 client needs, and `SSH_CONNECTION` is not, which sshd sets for each
  session (ssh(1), ENVIRONMENT), since a forwarded `DISPLAY` is the SSH client's. Anywhere else
  a locked collection is `Locked` with no `Unlock` called
  (`a_run_out_of_the_desktop_session_never_asks_to_unlock`).
- secret-service opens a connection for each question, and each one asks `Unlock` again while
  the collection is locked. A prompt the person dismissed, which secret-service reads as
  `Prompt`, is the run's answer: every copy of its context remembers it, and each later question
  is `Locked` with no `Unlock` called. Another run asks again
  (`a_prompt_dismissed_is_not_put_up_again_in_the_same_run`).
- Asking whether a Secret Service answers (`secret_service::answers`, which `pitboard vault`,
  doctor and a new home's form ask) opens a session on `org.freedesktop.secrets`. In the D-Bus
  specification (Message Bus Starting Services), a call to a name nobody owns starts the service
  a `.service` file names for it, unless the message carries `NO_AUTO_START`; zbus 5.19.0 sets
  that flag only where a call asks for `MethodFlags::NoAutoStart`, and secret-service asks for
  none. So the question can start gnome-keyring where the session has its service file. It
  unlocks nothing.
- secret-service is taken over oo7 0.6.0, whose `rust-version` is 1.92, past the core's 1.91, and
  which is async only and always pulls ashpd, and over keyring 4.2.0, whose crate docs tell an
  application that controls which store it uses not to link it, and whose `Entry` hides the
  attributes a list of parks searches by. Its crypto is pure Rust (aes 0.9, cbc 0.2, hkdf 0.13,
  sha2 0.11), so the musl command line links it, and it rides on the core's zbus 5 with
  `async-io`: `cargo tree -d -i zbus` names one zbus.

## The tools' processes

Measured on 29 October 2026 in Docker on the owner's Mac, arm64 under Docker Desktop's Linux
7.0.14, by `linux-runner-facts.sh installs` as a user whose uid is not 0: `ubuntu:26.04` with Node 22.22.1
and npm 9.2.0 from Ubuntu's archive, and Claude Code 2.1.312 and Codex 0.165.0 from npm in a
prefix of their own. Neither tool was run.

- `npm install --global --prefix` makes `bin/claude` a link to
  `../lib/node_modules/@anthropic-ai/claude-code/bin/claude.exe`. The package's postinstall,
  `install.cjs`, hard-links the platform package's program over that shell placeholder and does
  nothing else (read in 2.1.312's package), so the chain ends at Claude Code's linux-arm64
  build, an ELF for glibc's loader.
- `bin/codex` is a link to `../lib/node_modules/@openai/codex/bin/codex.js`, which starts with
  `#!/usr/bin/env node`. Codex's own program is
  `node_modules/@openai/codex-linux-arm64/vendor/aarch64-unknown-linux-musl/bin/codex` inside
  the package, static, and `codex.js` starts it by that path (read in 0.165.0's `codex.js`).
- With the stand-in in each program's place, started as a shell starts `claude` and `codex`:
  - npm's `claude` runs with `comm` `claude`, the link's name, and `exe` its `bin/claude.exe`.
  - npm's `codex` runs with `comm` `node`, since `env` starts node by name, and the command
    line `node <prefix>/bin/codex`, the link rather than `codex.js`.
  - Codex's own program runs with `comm` `codex` and `exe` its path in the package.
- So `procfs::processes` finds npm's Claude Code under `claude`, and npm's Codex by its own
  program under `codex`, never by its launcher. What either does to its name once it runs is
  [not measured](not-measured.md#linux).

## inotify

Measured the same day in that container, by `linux-runner-facts.sh inotify`'s first row on a
tmpfs, with inotify-tools 4.25.9.0 from Ubuntu's archive. `inotifywait` watched the Pitboard
directory while a debug `pitboard` built at 84c89168 took over a `state.json` planted as
another computer's, which `pitboard adopt` saves once, with every home fresh and on a bus of its
own.

- The save is `CREATE`, `OPEN`, `MODIFY` and `CLOSE_WRITE` of `.state.json.<pid>.0.pitboard`,
  then `MOVED_FROM` that name and `MOVED_TO` `state.json`, among the events of `state.lock` and
  `audit.log`. `state.json` itself is opened only to be read. So a watch of the directory sees
  a save of `state.json` as a `MOVED_TO` of its name.
- The runner's own disk, btrfs and NFS are [not measured](not-measured.md#linux).

## The headless display

Measured on 29 October 2026 in Docker on the owner's Mac, arm64, by `linux-runner-facts.sh`'s
display parts as [CONTRIBUTING.md](../../CONTRIBUTING.md#linux) runs them, twice, with the same
facts each time: `ubuntu:26.04` by its index digest
`sha256:0e73b71479e7bc5f006bca872e25c3cd680dc052a0116f90d63ae363537db0dd`, with mutter
50.1-0ubuntu2.5, GTK 4.22.4+ds-0ubuntu0.1, Mesa 26.0.8-1ubuntu0.3, at-spi2-core
2.60.4-0ubuntu0.1 and python3-gi 3.56.2-1 from Ubuntu's snapshot `20261028T000000Z`. Everything
but the packages ran as the user `display-setup` made, with no network, under `dbus-run-session`
with every home and XDG folder fresh and `GSETTINGS_BACKEND=memory`.

- `mutter --headless --wayland --no-x11 --virtual-monitor 1280x800` says "Created surfaceless
  renderer without GPU" and "Added virtual monitor Meta-0", warns only that
  `org.freedesktop.locale1` and colord are absent, and ends when the command it runs does.
- eglinfo on the Wayland platform names llvmpipe (LLVM 21.1.8, 128 bits) for OpenGL and
  OpenGL ES 3.2. With `GSK_RENDERER` unset, GTK finds no Vulkan driver, passes over GL ("Not
  using GL: renderer is llvmpipe"), finds no Vulkan again, and lands on GskGLRenderer. With
  `GSK_RENDERER=opengl` it tries no Vulkan, and draws the same pictures.
- On the private bus, the first GTK program's call starts `org.a11y.Bus`, then
  `org.a11y.atspi.Registry`, by activation. With `gtk4-builder-tool preview` showing a window,
  the AT-SPI desktop lists `gtk4-builder-tool preview` alone.
- A `GtkMenuButton` labelled Menu is three AT-SPI nodes of that name: a `button` with no
  action, a `toggle button` with one, `click`, and a `label` with eight. DoAction 0 on the
  toggle button shows the popover within 0.34 and 0.39 seconds: its label is SHOWING, under a
  `panel`.
- At 1280x800 mutter offers scales 1, 1.25 and 1.33 alone, and at 2560x1600 up to 2.67.
  `ApplyMonitorsConfig` with scale 2 there takes.
- `gtk4-builder-tool screenshot` of one window gives the same 390 x 226 PNG twice at scale 1
  and twice at scale 2: `f44b0f80c4746bb6e3bc2af2846374396eade5221f63bb92632dc8d08818f5a9` from
  GskGLRenderer, chosen or named, and
  `f7c86342630cdffb6c8ff683d6ad0fa715f08d4db48b00381c6723a2e4804aea` from cairo. The tool
  renders with `gsk_renderer_render_texture`, whose texture is the viewport's size in units
  (`gsk/gpu/gskgpurenderer.c` at GTK 4.22.4), so it makes no picture at 2x.
