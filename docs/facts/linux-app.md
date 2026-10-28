# What the Linux app rests on

Read on 28 October 2026 in the sources named, each at the version Debian 13 ships where that
matters, and measured where a line says so in Docker on the owner's Mac, as uid 1000 in
`debian:13@sha256:8c4733a92aacd0bcff3779eadc35a4a5147f36cb20a5fcb56504cbc740212259` (arm64)
with Debian's packages of that day.

## The floor

- Debian 13 ships GTK 4.18.6, libadwaita 1.7.6 and GLib 2.84.4 (Debian's madison, trixie).
  The app builds against gtk4 0.11.5 with `v4_18`, libadwaita 0.9.2 with `v1_7`, and gio and
  glib 0.22.10 with `v2_84`, so it calls nothing those lack and needs no check of a version
  at run time. Ubuntu 26.04 ships 4.22.4, 1.9.1 and 2.88.0, Fedora 44 4.22.5, 1.9.4 and
  2.88.4 (Launchpad and Fedora's mdapi, read for the plan the same day).
- gtk4 0.11.5 and glib 0.22.10 declare `rust_version` 1.92 (crates.io), past the 1.91 the
  published crates keep, so `pitboard-linux` names its own. Debian 13's rustc is 1.85.1, so a
  build there takes rustup's.
- Beside those, Debian 13 ships gnome-shell 48.7, kf6-krunner 6.13.0, plasma-workspace 6.3.6,
  desktop-file-utils 0.28, AppStream 1.0.5, Meson 1.7.0, dbus 1.16.2 and hicolor-icon-theme
  0.18, which `meson.build`'s `meson_version` and the readings below are held to.

## The builds and their names

Only Meson builds the release, with `PITBOARD_BUILD=release` (`src/config.rs`). Anything Cargo
builds alone is the debug build, `--release` or not, and installs nothing, since nothing
installs it.

| | Release | Debug |
|---|---|---|
| App id, and bus name | `com.usepitboard.Pitboard` | `com.usepitboard.Pitboard.Devel` |
| Link scheme | `pitboard` | `pitboard-debug` |
| Object path | `/com/usepitboard/Pitboard` | `/com/usepitboard/Pitboard/Devel` |
| Search provider | `/com/usepitboard/Pitboard/SearchProvider` | none installed |
| KRunner runner | `/com/usepitboard/Pitboard/Runner` | none installed |
| App icon | `com.usepitboard.Pitboard` | `com.usepitboard.Pitboard.Devel`, in the GResource alone |

`.Devel` is GNOME's suffix for a development build's id, and the object path is the id with
each dot a slash, as GApplication exports it. A D-Bus service starts its program in the bus's
activation environment, not the caller's (dbus-update-activation-environment(1), dbus
1.16.2), so a debug build started that way would run on the real homes: it has neither a
service file nor a launcher, and quits before it reaches the bus (`launch.rs`).

## Decisions and what they rest on

- **One D-Bus stack, GDBus.** GApplication owns the session connection and the app's name, and
  `org.freedesktop.Application`, `org.gtk.Actions` and GNotification ride on it. The search
  provider's `BusName` and the runner's service are that name, so their objects are served on
  that connection. ksni 0.3.7 is on zbus 5 with `tokio` its default feature (crates.io), while
  zbus is at 6.1.0: a second connection and an executor beside GLib's main loop. So the app
  serves StatusNotifierItem and DBusMenu itself, and links no zbus, ksni or ashpd.
- **Views are GtkBuilder `.ui` XML** in the GResource `build.rs` compiles, so `cargo test`
  reads them with no Meson. Blueprint's latest is 0.22.2 (GNOME/blueprint-compiler tags), a
  0.x language whose output is the same XML.
- **The app is a workspace member, not a default member.** The core, the command line and the
  app share one `Cargo.lock`. GTK's development files are on no machine but the app's own, so
  `cargo build`, `clippy` and `nextest` without `-p pitboard-linux` or `--workspace` leave it
  out, and its crate root, its program and its tests compile as nothing off Linux. Cargo uses
  `default-members` where no package is named (Cargo 1.98.0's reference, `workspaces.md`, The
  `default-members` field). Its dependencies are under `cfg(target_os = "linux")`, its
  build-dependency too, which Cargo builds "only when the host platform matches"
  (`specifying-dependencies.md:547-548`), so `build.rs` compiles the GResource only where the
  machine that builds is Linux. `cargo hack --workspace` takes `--exclude pitboard-linux`, since
  the app's floor is 1.92. Of what the app brings, `cargo deny` refused only target-lexicon
  0.13.5, `Apache-2.0 WITH LLVM-exception`, which system-deps' build scripts use through
  cfg-expr, and which `deny.toml` allows for that crate alone.
- **Neither GNOME's search nor KRunner starts Pitboard.** gnome-shell 48.7 reads `AutoStart`
  from a provider's ini and, where it is false, makes its proxy with
  `DO_NOT_AUTO_START`, so a search asks only a provider that is running
  (`js/ui/remoteSearch.js:108-113,196-203`). It loads providers only from the system's data
  folders (`:144`) and only for a `DesktopId` that is shown (`:97-102`). KRunner 6.13.0 takes a
  `X-Plasma-DBusRunner-Service` ending in `*` as every registered name it prefixes, found and
  then followed by `NameOwnerChanged`, and calls a name with no `*` whether it runs or not,
  "as it could be used with DBus-activation" (`src/dbusrunner.cpp:44-77`). It reads the
  metadata from `krunner/dbusplugins` in every data folder, the person's own included
  (`src/runnermanager.cpp:509-511`), and of it only `Name`, `Icon`, `Comment`,
  `X-KDE-PluginInfo-*` and the `X-Plasma-*` keys (`src/kpluginmetadata_utils_p.h:27-58`).
  Pitboard reads its accounts as it starts, which asks Anthropic and OpenAI, so a keystroke
  must not start it.
- **The tray's icon follows the level**, as the Windows icon's does, with `ToolTip` from
  `tip` and `NeedsAttention` from `attention`. No `XAyatanaLabel`: it is Ayatana's, outside
  StatusNotifierItem 0.1 (specifications.freedesktop.org), and plasma-workspace's tray reads no
  such property (`applets/systemtray` at 1ae2a7d8). So no Menu Bar Shows on Linux either: a
  setting one host honours is not a setting.
- **The Plasma widget reads a D-Bus feed, never a file.** Qt 6.10.3's XMLHttpRequest reads no
  local file unless `QML_XHR_ALLOW_FILE_READ` is set in its environment (Qt's XMLHttpRequest
  page), which a person would have to set for plasmashell. Plasma's `org.kde.plasma.workspace.dbus` has only a service watcher at
  plasma-workspace v6.1.0, calls and messages from v6.3.0, and `DBusProperties` and
  `DBusSignalWatcher` from v6.4.0 (`components/dbus/` at each tag), so the widget follows the
  feed without a poll from Plasma 6.4. Debian 13's 6.3.6 has neither, so the widget is
  packaged apart.
- **Ctrl+? is an `AdwDialog` of `AdwActionRow`s.** `AdwShortcutsDialog` is libadwaita 1.8, and
  GTK 4.18.6 deprecates `GtkShortcutsWindow` and `GtkShortcutLabel` ("Deprecated: 4.18",
  `gtk/deprecated/gtkshortcutswindow.c:119`, `gtk/deprecated/gtkshortcutlabel.c:36`). It becomes
  `AdwShortcutsDialog` once the floor reaches 1.8.
- **UI tests find a widget by its AT-SPI `AccessibleId`**, which GTK 4.22.0 answers with the
  widget's GtkBuilder id (`gtk/gtkwidget.c:8873-8878`, `gtk/a11y/gtkatspicontext.c:802-808`).
  GTK 4.18.6 answers `""` (`gtk/a11y/gtkatspicontext.c:770-771`), so the UI tests run where
  GTK is 4.22, Ubuntu 26.04's, and the floor's leg runs the rest.

## The install layout

Under Meson's prefix, `/usr` for a package:

| Path | From |
|---|---|
| `bin/pitboard-app` | Cargo's release, through `scripts/cargo-build.sh` |
| `share/applications/com.usepitboard.Pitboard.desktop` | `data/com.usepitboard.Pitboard.desktop.in` |
| `share/dbus-1/services/com.usepitboard.Pitboard.service` | `data/com.usepitboard.Pitboard.service.in` |
| `share/metainfo/com.usepitboard.Pitboard.metainfo.xml` | `data/com.usepitboard.Pitboard.metainfo.xml.in` and `CHANGELOG.md` |
| `share/icons/hicolor/scalable/apps/com.usepitboard.Pitboard.svg` | `data/icons/scalable/apps` |
| `share/icons/hicolor/symbolic/apps/com.usepitboard.Pitboard-symbolic.svg` | `data/icons/symbolic/apps` |
| `share/icons/hicolor/symbolic/status/com.usepitboard.Pitboard-<level>[-attention]-symbolic.svg` | `data/icons/symbolic/status`, the levels `plenty`, `low`, `out` and `none` |
| `share/gnome-shell/search-providers/com.usepitboard.Pitboard.search-provider.ini` | `data` |
| `share/krunner/dbusplugins/com.usepitboard.Pitboard.desktop` | `data/krunner` |

The Plasma widget, at `share/plasma/plasmoids/com.usepitboard.Pitboard/`, comes behind a Meson
option of its own and is packaged apart, since it needs Plasma 6.4. `pitboard` itself, beside
`pitboard-app`, is the packages' to install.

## The metainfo

- AppStream 1.0.5's validator takes any `https://`, `http://` or `ftp://` in a description's
  text for a link, a `<code>` span's included, and warns `description-has-plaintext-url`, which
  fails `appstreamcli validate` (`src/as-validator.c:771-789,1114-1119`). CHANGELOG.md's one
  such text is 0.12.0's install command. So `metainfo-releases.py` refuses a web address, and
  turns a link into its text.
- The specification asks for a `<release>` for each release, newest first, and has a listing
  show stable releases alone (AppStream's Releases page, freedesktop.org). Pitboard for Linux
  ships in 1.x, never in 0.x, and listed, 0.15.2 would read as the app's newest stable
  release. So the metainfo lists the releases from 1.0.0-beta.1 on.
- `appstreamcli news-to-metainfo` reads GNOME's NEWS form, a `Version` line over a `~~~~` or
  `----` rule with `Released:` under it (`src/as-news-convert.c:522-640`), not Keep a
  Changelog's headings, so the script reads CHANGELOG.md itself.

## Measured in the container

In `debian:13` by the digest above, with GTK's, libadwaita's and GLib's development packages,
mutter, Meson, desktop-file-utils, AppStream, dbus-daemon and librsvg at Debian 13's versions,
rustup's 1.98.1 and 1.92, run as uid 1000 with every home fresh:

- dbus-daemon 1.16.2 does not start for a uid the password database lacks ("Could not get
  password database information for UID of current process"), so the image has a user for
  uid 1000.
- mutter 48.7 runs `--headless --wayland --no-x11 --virtual-monitor 1280x800` as uid 1000 with
  no seat and no logind, on a "surfaceless renderer without GPU", and warns only that
  `org.freedesktop.locale1` and colord are absent.
- desktop-file-validate 0.28 passes the launcher and KRunner's runner, its `Type=Service`
  included. librsvg 2.60's `rsvg-convert` writes only a regular file, not `/dev/null`.
- Meson 1.7.0 configures a project that names no compiler language, which the app's, that
  only runs Cargo, is. `meson install --destdir` writes the install layout above and nothing
  else, `bin/pitboard-app` at mode 0755, and the release's program holds neither `.Devel` nor
  `pitboard-debug`. appstreamcli 1.0.5 passes the metainfo with `validate --no-net`.
- `cargo hack check --rust-version -p pitboard-linux --all-targets` passes on Rust 1.92.

## The icons

- GNOME's app icon guidelines (developer.gnome.org/hig, App Icons) draw on a 128 px canvas
  with a 2 px grid, the object's bottom on the template's baseline, its front profile darker
  than its top and at most 4 px tall, flat colours on flat surfaces, no shadow outside the
  silhouette, a distinct icon for a development build and a symbolic one beside it. App Icon
  Preview 3.5.1's template (`data/resources/templates/empty_project.svg`) puts the baseline
  117 px from the top and its landscape guide 112 px wide. So the app icon is the macOS icon's
  board and lit row seen from the front, 112 px wide on its post, the post's foot on y 116, the
  grid line next to the baseline. Its bezel and post are GNOME's palette, Light 4 over Light 5
  and Light 5 over Dark 1 for each front profile, its face a flat tone of the macOS face's
  gradient, its lit row the macOS icon's green and its second row the Windows icon's grey. The
  development build's lit row is the macOS debug icon's amber.
- GTK 4.18.6 draws a `-symbolic.svg` through a stylesheet that sets `fill` on every `rect`,
  `circle` and `path` with `!important` (`gtk/gdktextureutils.c:231-241`), so the symbolic
  icons draw only those, and their tones with `opacity`.
- KIconLoader 6.13.0 replaces the text of a `<style id="current-color-scheme">` with the colour
  scheme's stylesheet (`src/kiconloader.cpp:686-694`), whose `.ColorScheme-Text` sets `color`
  (`src/kiconcolors.cpp:16`). So each status icon carries that style, and its shapes the class
  and `fill="currentColor"`, as the Windows icon's drawings are inked.
- GtkIconTheme finds a GResource's icons only in the folders the hicolor theme defines
  (`gtk/gtkicontheme.c:1560-1576`).
