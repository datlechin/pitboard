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
