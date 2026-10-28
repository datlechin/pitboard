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
