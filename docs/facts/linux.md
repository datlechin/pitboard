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
