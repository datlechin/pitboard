# What the Linux app rests on

Read on 28 and 29 October 2026 in the sources named, each at the version Debian 13 ships where that
matters, and measured where a line says so in Docker on the owner's Mac, as uid 1000 in
`debian:13@sha256:8c4733a92aacd0bcff3779eadc35a4a5147f36cb20a5fcb56504cbc740212259` (arm64)
with Debian's packages of that day, or in [the checks' images](#the-checks-images).

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
installs it. The release hands the model its program as Meson installs it,
`PITBOARD_BINDIR/pitboard-app`, and renewal and the watch service run the `pitboard` beside it
([linux.md](linux.md#the-linux-apps-programs),
`the_installed_program_names_the_command_line_beside_it`), each a unit the core writes only where
the user manager reads it for this home, and skipped once that `pitboard` is gone
([linux.md](linux.md#the-users-systemd)). The debug build hands none, so its Preferences say it
has no command line beside it to run either.

| | Release | Debug |
|---|---|---|
| App id, and bus name | `com.usepitboard.Pitboard` | `com.usepitboard.Pitboard.Devel` |
| Link scheme | `pitboard` | `pitboard-debug` |
| Object path | `/com/usepitboard/Pitboard` | `/com/usepitboard/Pitboard/Devel` |
| Search provider | `/com/usepitboard/Pitboard/SearchProvider` | none installed |
| KRunner runner | `/com/usepitboard/Pitboard/Runner` | none installed |
| Widget feed | `/com/usepitboard/Pitboard/WidgetFeed` | `/com/usepitboard/Pitboard/Devel/WidgetFeed`, which no widget reads |
| App icon | `com.usepitboard.Pitboard` | `com.usepitboard.Pitboard.Devel`, in the debug build's GResource alone |

`.Devel` is GNOME's suffix for a development build's id, and the object path is the id with
each dot a slash, as GApplication exports it. A D-Bus service starts its program in the bus's
activation environment, not the caller's (dbus-update-activation-environment(1), dbus
1.16.2). Measured in Debian 13's and Ubuntu 26.04's images with dbus-daemon 1.16.2 (M10, the
D-Bus half): a service file's program, started by `StartServiceByName` from a caller with
`HOME`, `PITBOARD_HOME` and `XDG_DATA_HOME` of its own, saw the `HOME` and `PITBOARD_HOME`
the bus was started with and no `XDG_DATA_HOME`. So a debug build started that way would run on
the real homes: it has neither a service file nor a launcher, and quits before it owns a name
unless `PITBOARD_FIXTURE` names a fixture it can make (`launch.rs`,
`a_debug_build_outside_a_fixture_quits_before_it_owns_a_name`). Whether autostart starts a
program on the session's homes is the Linux session's to measure (Documented, not measured).

## Decisions and what they rest on

- **One D-Bus stack, GDBus.** GApplication owns the session connection and the app's name, and
  `org.freedesktop.Application`, `org.gtk.Actions` and GNotification ride on it. The search
  provider's `BusName` and the runner's service are that name, so their objects are served on
  that connection. ksni 0.3.7 is on zbus 5 with `tokio` its default feature (crates.io), while
  zbus is at 6.1.0: a second connection and an executor beside GLib's main loop. So the app
  serves StatusNotifierItem and DBusMenu itself, and links no zbus, ksni or ashpd. Its UI tests
  alone link atspi 0.30.0, the latest, on zbus 5 with zbus's own async-io executor, as their
  AT-SPI client, which `deny.toml` allows zbus 5's unmaintained parts for.
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
  page), which a person would have to set for plasmashell. Plasma's
  `org.kde.plasma.workspace.dbus` has only a service watcher at plasma-workspace v6.1.0, calls
  and messages from v6.3.0, and `Properties` and `SignalWatcher` from v6.4.0 (`components/dbus/`
  at each tag, v6.3.0's read again on 29 October), so the widget follows the feed without a poll
  from Plasma 6.4. Debian 13's 6.3.6 has neither, so the widget is packaged apart. [The Plasma
  widget](#the-plasma-widget) has the rest.
- **Without a status item, closing the window quits**, and the watch service runs the
  automatic switch and the warm-ups once it has, which Preferences turns on (brief `close`):
  stock GNOME has no StatusNotifierItem host, and Background Apps lists no host app (synthesis
  1). The model offers it while the app reports no status item, reads it on each visit to
  Preferences and installs it through the core's watch service, which on Linux the core's
  systemd host writes as a user service: with `--at` and the share Limits has where its choice is
  Switch, and with `--no-switch` where it is Notify Me or Do Nothing, so it then only warms up
  (brief, 2026-10-29). The model installs it again as the choice or the share changes, and Limits
  and Advanced say what it does once the window is closed.
- **The app runs while something holds it.** `g_application_run` iterates while the use count
  is above nought or an inactivity timeout is pending, and in service mode waits 10 seconds
  with a use count of nought for the call that activated it (`gio/gapplication.c` at 2.84.0,
  `g_application_run`, `g_application_release`); GtkApplication holds the app for each window
  it has, shown or hidden, until the window is removed (`gtk/gtkapplication.c:462,491` at
  4.18.6). So the app holds itself while a tray shows its status item, from its start until the
  tray's first word but in service mode, and while a switch from outside waits on its answer;
  makes its window as soon as a launch asks for one; and destroys a window it hid for a tray as
  the tray goes. A launch at sign-in sets a 10 second inactivity timeout, GLib's own service
  wait, for a tray that starts after it.
- **Ctrl+? is an `AdwDialog` of `AdwActionRow`s.** `AdwShortcutsDialog` is libadwaita 1.8, and
  GTK 4.18.6 deprecates `GtkShortcutsWindow` and `GtkShortcutLabel` ("Deprecated: 4.18",
  `gtk/deprecated/gtkshortcutswindow.c:119`, `gtk/deprecated/gtkshortcutlabel.c:36`). It becomes
  `AdwShortcutsDialog` once the floor reaches 1.8.
- **UI tests find a widget by its AT-SPI `AccessibleId`**, which GTK 4.22.0 answers with the
  widget's GtkBuilder id (`gtk/gtkwidget.c:8873-8878`, `gtk/a11y/gtkatspicontext.c:802-808`).
  GTK 4.18.6 answers `""` (`gtk/a11y/gtkatspicontext.c:770-771`), so the UI tests run where
  GTK is 4.22, Ubuntu 26.04's x86_64 leg (`check.sh ui`), and the other legs run the rest.
- **The views' templates are checked by building each view.** `gtk4-builder-tool validate`
  4.22.4 refuses a template whose parent is libadwaita's, "Failed to lookup template parent type
  AdwApplicationWindow", since it registers GTK's types alone (measured in Ubuntu 26.04's
  image). So `each_view_is_built_from_its_template_with_no_warning` builds each with GTK's,
  libadwaita's and GObject's warnings fatal, and the UI tests and the pictures build them too.
- **No gschema, and the clock read where GNOME's schema is.** The window keeps its size in
  `window.json` in the app's data folder. The 12 or 24 hour clock is
  `org.gnome.desktop.interface` `clock-format`, read and followed through GSettings where the
  schema is installed, as it is in every image. Measured with dconf's GSettings backend, 0.40.0
  in Debian 13's image and 0.49.0 in Ubuntu 26.04's, on a private bus: a read and a `gsettings
  monitor` start no `ca.desrt.dconf`, and only a write starts it. So the clock starts no
  `dconf-service` in the person's session.

## GNOME's words

`Front::Gnome` words every surface for GNOME's Human Interface Guidelines, read on 29 October
2026 at developer.gnome.org/hig, and on every desktop the same: one front serves Plasma too, so
no word names one desktop's settings or another system's place
(`no_gnome_word_names_a_desktops_settings_or_another_systems_place`).

- **Header capitalisation** for "any headings, including headings in header bars, tab titles,
  and view titles", and for "short control labels that do not normally form proper sentences";
  sentence capitalisation "for labels that form sentences or that run on to other text"
  (Writing Style, Capitalization). Header capitalisation capitalises "all words with four or more
  letters", verbs and nouns of any length, the first and last word and each part of a hyphenated
  word. That is Apple's title case in every title Pitboard has but one with "with", so
  `titled` gives GNOME the title case and `use_with_codex` says "Use Pitboard With Codex". A
  menu line of limits runs on, so it is sentence case, as on Windows (`limits_line`).
- **Ellipses** "if further input or confirmation is required from the user", and "do not add an
  ellipsis to labels such as Properties or Preferences" (Writing Style, Ellipses). So Add
  Account…, Open Link…, Sign In Again… and Uninstall Pitboard… have one, the window's Add
  Account included, and Preferences none.
- **The primary menu** ends with Preferences, Keyboard Shortcuts, Help and About Pitboard, and
  "primary menus shouldn’t include menu items for Close or Quit"; its button's "tooltip and
  accessibility label should refer to the menu as ‘Main Menu’" (Menus). Quit stays in the
  status item's menu and on Ctrl+Q.
- **The app's settings are Preferences** on GNOME (`words::settings`), and a download's folder
  is opened with Open Containing Folder, after GTK's `gtk_file_launcher_open_containing_folder`,
  which "launches a file manager to show the file in its parent directory" (GTK 4.10 and later,
  docs.gtk.org), so no file manager is named.
- **A status item's menu entry** has a label and no subtitle (DBusMenu, [The status
  item](#the-status-item)), so an account's entry says its figure beside its name, as the
  Windows menu's does (`named_in_menu`).
- **After Uninstall** the app names its package manager's command: `apt` for the deb and `dnf`
  for the rpm (`package_removal`). Whether the rpm also serves openSUSE, whose `zypper` would
  remove it, is [not measured](not-measured.md#the-linux-app-on-a-desktop).

## Account windows

The Linux app opens none in v1 (brief `webwindows`): `Front::Gnome.offers_account_windows()` is
false, so no menu, row or quick action offers a window and a link that arrives is offered to the
browser alone (`PickerShown::NoWindows`), and the app's launch names no records folder. What they
wait for, from the plan's research of 28 October 2026 (`.claude/plans/linux-app/research`), read
from WebKit's and Epiphany's sources and not run:

- **WebKitGTK 2.56.** WebKit fe4fb973bb (2026-08-19, bug 321520) adds claude.ai to
  `urlRequiresFirefoxBrowser` in `Source/WebCore/platform/glib/UserAgentQuirks.cpp`, absent
  from tag `webkitgtk-2.54.2`; c22ba3278c (2026-10-13, bug 319164) adds a navigation's target
  and source `WebKitFrameInfo`, which `NavigationTarget::Frame` and `Asker` need. Pitboard sets
  no user agent, so neither works before 2.56. Debian 13 has 2.54.0 in trixie-security, and
  Ubuntu rebases WebKitGTK's majors into an LTS's security pocket, noble's from 2.44.0 to
  2.52.6 (Debian's and Launchpad's records, read for the critique), so the floor's
  distributions carry 2.56 some months after it ships.
- **The `webkit6` crate**, 0.6.1, stops at the feature `v2_52`. It is generated from GIR, so
  2.54's and 2.56's calls wait on a release regenerated upstream.
- **Gaps that stay on 2.56**, each a model word or a request upstream before windows ship:
  - `script-dialog`'s `WebKitScriptDialog` carries no frame and no origin
    (`WebKitScriptDialog.h.in`, main), so `dialog_title(host, main_frame)` cannot be honoured.
  - 2.56's `WebKitFrameInfo` has `is_main_frame` and no origin and no web view, so an opener
    navigating its sign-in window reads as that window's own, where `WebSession.swift:78-84`
    tells them apart.
  - Epiphany numbers a download that is there as `name(1).ext` (`ephy-download.c:246-266`),
    while the model numbers as Finder for `Engine::WebKitGtk` (`account_windows/downloads.rs`).
  - Intelligent Tracking Prevention is the app's to set (Epiphany sets it from a preference),
    and macOS's default is not read: the engine's default stays.
  - `context-menu` offers its stock download actions, which macOS removes: removed, or sent
    through `decide_response` asking first.
  - Ubuntu's AppArmor rule on unprivileged user namespaces may stop WebKit's bwrap sandbox for a
    host app, so the package may need a profile of its own (M13).

## Notifications

`src/platform/notifications.rs` posts through GNotification. Read in GLib 2.84.4 and GNOME Shell
48.7, Debian 13's:

- GApplication makes its notification backend at its first send or withdraw
  (`gio/gapplication.c:3104-3105,3146-3147`). GLib picks the backend once for the process: the
  one `GNOTIFICATION_BACKEND` names, else the first by priority whose check passes, and keeps
  the answer (`gio/giomodule.c:764-871`). `portal`, 110, only where GLib uses portals, in a
  sandbox (`gio/gportalnotificationbackend.c:44-53`); `gtk`, 100, where `org.gtk.Notifications`
  has an owner on the session bus when first asked (`gio/ggtknotificationbackend.c:44-77`);
  `freedesktop`, 0, always (`gio/gfdonotificationbackend.c:56-59,452-460`). So the app's tests
  run each backend in a program of its own.
- The `gtk` backend sends `AddNotification` with the app's id, the notice's id and the
  notification as a dictionary: `title`, `body`, `priority` by its nick, `default-action` and
  its target, and `buttons`, each a `label`, an `action` and a `target`
  (`gio/gnotification.c:766-857`).
- The `freedesktop` backend sends `Notify` with the actions `default` and each button's
  detailed action name, and the hints `desktop-entry`, the app's id, and `urgency`: 0 for low,
  1 for normal and high, 2 for urgent (`gio/gfdonotificationbackend.c:256-381`). It hears
  `ActionInvoked` from the owner of `org.freedesktop.Notifications` on the app's own
  connection, and activates the `app.` action named, with its target, only where the app has
  that action with that parameter type (`:135-240,464-500`). A click reaches a running app
  alone.
- GNOME Shell refuses a notification from an id with no `<id>.desktop` installed ("could not be
  found", `js/ui/notificationDaemon.js:491-498,663-675`), so the debug build's `.Devel` id,
  which installs no launcher, gets none: its fixture posts to a stand-in. It draws the first
  three buttons (`js/ui/messageList.js:20,752`), takes the priority nicks as its urgencies
  (`js/ui/notificationDaemon.js:404-409`), and activates an `app.` action by
  `org.freedesktop.Application.ActivateAction` on the app's bus name
  (`js/ui/notificationDaemon.js:513-517`, `src/shell-app.c:1523-1529`), which the bus starts
  where the app is not running.

A notification has a title and a body alone (`g_notification_set_title`, `_set_body`, GIO
2.84), so a model notice's subtitle, the tool beside another tool's accounts, is the body's first
line, and a notice has one button at most (`src/notices.rs`). Each is posted at
`G_NOTIFICATION_PRIORITY_NORMAL`, GNotification's default, which GIO keeps for most
notifications, such as completed downloads; `HIGH` is for what is time-sensitive, such as chat
messages, and `URGENT` for what needs an answer within a short time, such as a phone
call (`GNotificationPriority`, `gio/gioenums.h` at 2.84.4).

Measured on a bus of the test's own: a notice posted from another thread reaches a stand-in
`org.gtk.Notifications` at normal priority, with its default action and its button as
`app.notice-clicked` and their arguments, and a stand-in freedesktop server's `ActionInvoked`
for the default and for a button reaches the app's `notice-clicked` with each argument
(`tests/notifications_gtk.rs`, `tests/notifications_freedesktop.rs`). An app only registered
keeps its bus connection past its drop, its exported actions holding it, and `TestDBus` waits
30 seconds for it ("Weak notify timeout"); `g_application_run`'s end lets go of it
(`gio/gapplication.c:2729-2741`), so those tests run the app.

## Start at sign-in

`src/platform/autostart.rs` writes `$XDG_CONFIG_HOME/autostart/<app id>.desktop`, with
`Exec=<program> --sign-in` and `TryExec=<program>`. A flag of the app's own, since GLib keeps
`--gapplication-service` for D-Bus service files (`gio/gapplication.c:551-552`).

- The Desktop Application Autostart Specification 0.5 (13 February 2006): of entries of one
  name, the one in `$XDG_CONFIG_HOME/autostart` counts (2.1); an entry with `Hidden=true` is
  ignored, and hides the others of its name (2.2.1); one whose non-empty `TryExec` names no
  installed program is not started (2.2.3).
- The Desktop Entry Specification 1.5, The Exec key: an argument holding a reserved character is
  double quoted, with `"`, `` ` ``, `$` and `\` escaped, and a literal `%` is `%%`.
- systemd 257.13's XDG autostart generator, which turns entries into units of the user
  manager, reads the person's folder before `$XDG_CONFIG_DIRS`
  (`src/xdg-autostart-generator/xdg-autostart-generator.c:56-77`), and makes no unit for an
  entry with `Hidden=true` (`xdg-autostart-service.c:527-530`), one it cannot read, which it
  hides (`:357-361`), or one whose `TryExec` it cannot find (`:561-570`).

Measured in Debian 13's image (systemd 257.13) and Fedora 44's (259.10), as uid 1000, by
running the generator on the entry the app writes: it made
`app-com.usepitboard.Pitboard@autostart.service`, `Type=exec`,
`ExecStart=:<program> --sign-in`, `After=` and `PartOf=graphical-session.target`, wanted by
`xdg-desktop-autostart.target`. A program path with a space, quoted, came out as one argument.
`Hidden=true`, or the program removed, made no unit. So an entry starts its program as a unit of
the user manager, in the manager's environment, whatever the shell that wrote it had: a
fixture's stand-in writes in place of the entry.

## Waking

`src/platform/logind.rs` hears `PrepareForSleep` on the system bus.

- org.freedesktop.login1(5), systemd 257.13: `PrepareForSleep` is sent with `true` right
  before the system suspends or hibernates and with `false` right after
  (`man/org.freedesktop.login1.xml:769-783`).
- GLib 2.84.4 delivers a signal subscribed to by a well-known sender only from the name's
  current owner (`gio/gdbusconnection.c:4326-4350`).

Measured on a bus of the test's own: `PrepareForSleep(true)` from the owner of
`org.freedesktop.login1` tells nothing and `(false)` tells one wake, and a `(false)` from a
connection that owns no such name tells nothing (`tests/logind.rs`).

## Links

`src/platform/links.rs` hands the model what arrives, as text.

- GApplication makes a `GFile` of each link: on the command line through
  `g_file_new_for_commandline_arg` (`gio/gapplication.c:1209`), which takes an argument with a
  scheme to `g_file_new_for_uri` (`gio/gfile.c:7640-7650`), and in `Open` over the bus through
  `g_file_new_for_uri` (`gio/gapplicationimpl-dbus.c:240`).
- GLib's own VFS makes a link of a scheme it does not know a file that gives the text back as
  given (`gio/glocalvfs.c:92-106`, `gio/gdummyfile.c:141-149`). GVfs 1.57.2, Debian 13's,
  parses it into a mount and writes it again (`client/gdaemonvfs.c:173-260,389-422`,
  `client/gdaemonfile.c:207-214`).
- A scheme registered with `g_vfs_register_uri_scheme`, GLib 2.50's, is looked up before the
  VFS's own for that process alone (`gio/gvfs.c:195-252,387-470`).
- GTK 4.18.6's `GtkUriLauncher` opens a link through the OpenURI portal where it has the
  interface at version 3, and through GIO's handler for the scheme otherwise
  (`gtk/gtkurilauncher.c:320-328`, `gdk/gdk.c:486-508`).

Measured in Debian 13's image with Debian's `gvfs` 1.57.2 added, as uid 1000 under
`headless.sh`: `open` handed `pitboard-debug://open/?url=…` for `pitboard-debug://open?url=…`
and `pitboard-debug:///open` for `pitboard-debug:open`. Without GVfs, and with the app's scheme
registered in either image, each came as written. Registered with GLib's own VFS made first
inside the lookup, the run did not return in nine minutes and was stopped; made before, it
passed. So the app registers its scheme before it runs, and every check's image has GVfs.

## The status item

`src/platform/tray/` serves the item and its menu over GDBus, on the connection it is given.
Read in the sources named, each at the version Debian 13 ships where it ships one:

- StatusNotifierItem 0.1 (specifications.freedesktop.org/status-notifier-item, its date "TBD")
  has an app own `org.freedesktop.StatusNotifierItem-<pid>-<id>`, serve the item at
  `/StatusNotifierItem` and register its name with the watcher. The trays read speak KDE's names
  instead: Plasma 6.3.6's watcher owns `org.kde.StatusNotifierWatcher` and reads
  `org.kde.StatusNotifierItem` (`statusnotifierwatcher/statusnotifierwatcher.cpp:39-68`), as
  the AppIndicator extension does (v59, Debian 13's, and v66, `statusNotifierWatcher.js`, its
  `interfaces-xml/StatusNotifierItem.xml` "Based on" KDE's). So
  `data/dbus/org.kde.StatusNotifierItem.xml` is KStatusNotifierItem 6.13.0's
  `src/org.kde.StatusNotifierItem.xml`: the specification's members, with `IconThemePath`,
  `NewMenu` and `ProvideXdgActivationToken`. The item owns `org.kde.StatusNotifierItem-<pid>-1`.
- Plasma's watcher takes a name only while it has an owner and the item answers at
  `/StatusNotifierItem`, and lets go of it as the name goes (`:39-68,75-91`). The extension
  takes the name's owner at the same path, and refuses a name with none (v59
  `statusNotifierWatcher.js:174-202`). So the item owns its name before it registers, and
  registers again each time the watcher's name gets an owner, as a restarted plasmashell's
  would.
- Plasma 6.3.6's host reads every property with one `GetAll`, again on each `New*` signal
  (`applets/systemtray/statusnotifieritemsource.cpp:88-94,215-230`), drops a tool tip whose
  title is empty (`:344-348`), and draws the menu `Menu` names, calling `ContextMenu` only for
  an item with none (`:533-543`). The extension shows an item only with an `Id` and a `Menu`
  (v59 `appIndicator.js:476-524`).
- The specification lets a tool tip's text hold a subset of HTML. Plasma 6.3.6 shows it with
  Qt's `Text.AutoText`, as rich text only where it reads as markup
  (`applets/systemtray/package/contents/ui/items/StatusNotifierItem.qml:21-22`), and the
  extension shows no tool tip. So the text goes as written: escaped, an `&` would show as
  `&amp;` in Plasma.
- Plasma 6.3.6 asks KWin for an activation token and provides it with
  `ProvideXdgActivationToken` before each `Activate`, `SecondaryActivate` and `ContextMenu`
  (`applets/systemtray/statusnotifieritemjob.cpp:26-60`). The extension provides one before
  `Activate` (v59 `appIndicator.js:771-799`) and before the `Event` of a click on an entry
  (v59 `dbusMenu.js:667-672`, v66 `dbusMenu.js:678-690`). KStatusNotifierItem keeps the last
  token as the one the app activates with next (`src/kstatusnotifieritemdbus_p.cpp:294-298`).
  So the item hands the last token on with the next click, on the item or on an entry.
- `data/dbus/com.canonical.dbusmenu.xml` is libdbusmenu 18.10.20180917~bzr492's
  `libdbusmenu-glib/dbus-menu.xml`, Debian 13's, which lists each entry property with its
  default, and asks that a property at its default be left out. An underscore in a label marks
  the key that picks the entry and two show one, so the item doubles each underscore.
  libdbusmenu-glib serves version 3 (`libdbusmenu-glib/server.c:45`), keeps a number per entry,
  raises the revision only as entries are added, removed or moved and says a property's change
  with `ItemsPropertiesUpdated` (`:905-915,1097,1118,1222-1245`), and refuses a number it does
  not have (`:1340-1350`).
- Plasma 6.3.6's importer reads a level of the layout with every property, again on `LayoutUpdated`,
  but takes an entry's type, toggle type and submenu once, and of an entry it knows updates only the
  properties the new layout holds (`libdbusmenuqt/dbusmenuimporter.cpp:81-89,368-445`). A property
  gone back to its default comes back only through `ItemsPropertiesUpdated` (`:327-354`). The
  extension v59 reads the whole layout with `type` and `children-display` alone, and an entry's
  properties only when it first sees its number (`dbusMenu.js:340-430`). So an entry keeps its
  number while it stays, a radio entry and a plain one of the same id are two entries, and a change
  to an entry that stays is said with `ItemsPropertiesUpdated`.
- As a menu opens, Plasma 6.3.6 calls `AboutToShow(0)` and sends `opened`
  (`dbusmenuimporter.cpp:463-479`), and the extension does both too (v59 `dbusMenu.js:938-952`),
  and calls `AboutToShow(0)` once as it first reads the menu (`:878-879`). A click on an entry is
  `Event(<number>, "clicked", …)`. So the item takes `AboutToShow` of the root for a glance.

Measured on a bus of the test's own, as uid 1000 in each of [the checks'
images](#the-checks-images), the tests failing first in Debian 13's against an item that did
nothing: an item registers with a stand-in watcher once its name has an owner and the watcher's has
one, and again as the watcher's name comes back; with no watcher, nothing registered and a service
file that would have started one was not started, which a `StartServiceByName` of it then did; a
stand-in host reads the properties above and hears `NewStatus`, `NewIcon` and `NewToolTip` only for
what changed, a new entry raises the revision by one with `LayoutUpdated` while the others keep
their numbers, and a radio entry chosen or an entry enabled is said with `ItemsPropertiesUpdated`
alone (`tests/presence`).

## Search and KRunner

`src/platform/search.rs` and `src/platform/krunner.rs` answer on the app's own connection,
under its object path, with the results the app gives for a search's terms. Read at GNOME Shell
48.7 and KRunner 6.13.0, Debian 13's, and at KRunner 6.17.0:

- `data/dbus/org.gnome.Shell.SearchProvider2.xml` is GNOME Shell 48.7's
  `data/dbus-interfaces/org.gnome.ShellSearchProvider2.xml`. A subsearch "may return less
  results, but not more or different results" than those it narrows, so the provider answers
  only results among them. GNOME Shell splits what is typed into its words for the terms
  (`js/ui/searchController.js:19-24`), asks the metas of the results it shows, and takes a
  meta's `icon` back with `g_icon_deserialize` (`js/ui/remoteSearch.js:221-245,280-305`). It
  activates a result, or launches a search of the app's own, with the terms and a timestamp
  of its own, and with no activation token (`:328-336`).
- `data/dbus/org.kde.krunner1.xml` is KRunner 6.17.0's `src/data/org.kde.krunner1.xml`, the
  first release with `SetActivationToken`, whose file 6.30.0 still holds as it is. On Wayland
  KRunner 6.17.0 asks KWin for a token and sets it just before `Run`
  (`src/dbusrunner.cpp:270-312`). KRunner 6.13.0 has no such call and runs with no token
  (`src/dbusrunner.cpp:269-287`). Ubuntu 26.04 ships KRunner 6.24.0 and Fedora 44 6.25.0
  (Launchpad, Fedora's mdapi).
- KRunner hands a match's id back to `Run` with an empty action for its default, and calls
  `Teardown` as a session that matched ends (6.17.0 `src/dbusrunner.cpp:84-93,270-290`). With
  `X-Plasma-API=DBus`, the runner's, it calls no `Config` (`:33,106-113`). A match's fourth
  field is its category's relevance, which orders the categories, and its fifth orders the
  matches within one, from 0 to 1 (`src/querymatch.h:63-136`). Plasma 6.3.6's applications
  runner puts an application at Moderate, High where the query starts its name and Highest
  where the query is its name (`runners/services/servicerunner.cpp:255-290`). So Pitboard's
  matches are Moderate, their relevance falling in the order the app gave them.

Measured on a bus of the test's own, as uid 1000 in each of the checks' images, the tests failing
first in Debian 13's against a provider and a runner that served nothing: a stand-in GNOME Shell
gets the ids of the results the app gives for its terms, a subsearch's among those it narrows alone,
metas whose icon `g_icon_deserialize` makes a themed icon of that name, an activation of a result
given and a launched search told, and nothing told for an id the search did not give; a stand-in
KRunner gets the same results as matches at Moderate, the first at 1 and the next at 0.5, and a run
told with the token set before it, and none after `Teardown` (`tests/presence`).

## The Plasma widget

The app serves `com.usepitboard.Pitboard.WidgetFeed`, Pitboard's own interface
(`data/dbus/com.usepitboard.Pitboard.WidgetFeed.xml`), at `<app path>/WidgetFeed` on its own
connection, from its registration on (`src/widget_feed.rs`). `Feed` is the model's
`widget_feed()`, the glance as JSON, empty before the first read and once Pitboard is removed;
`Revision` is the snapshot's `widget_revision` it was taken at. `PropertiesChanged` carries both,
with nothing invalidated, as the revision moves and only then. With no file named, the model
moves the revision only as what a widget shows changes, not as the time of the read does
(`model/state.rs` `glanced`), so the feed's **Updated** can be older than the last read, as on
macOS. Measured on a bus of the tests' own, as uid 1000 in Ubuntu 26.04's image with GLib
2.88.0, the tests failing first to build without the module: GDBus refuses a `Set` of either
property and a call of any method on the interface, and introspection lists the two properties
read only, no method and no signal (`tests/widget_feed.rs`).

The widget is QML on Kirigami, PlasmaComponents 3 and `org.kde.plasma.workspace.dbus`, read at
plasma-workspace v6.4.0 and v6.8.2, which differ there in no call or property the widget uses:

- `Properties` reads the interface with `GetAll` and `Introspect` once its service, path and
  interface are set, then takes each `PropertiesChanged` itself, inserting the changed values
  and emitting `propertiesChanged` (`components/dbus/dbusproperties.cpp:59-126,352-363`). So
  the widget needs no `SignalWatcher`.
- Each call the module makes is a `QDBusMessage::createMethodCall`, which leaves the bus to start
  the service a well-known name names (`dbusproperties.cpp:35,62,107`,
  `dbuspendingreply.cpp:24,75`). Measured in Fedora 44's image with Plasma 6.7.5, Qt 6.11.2 and
  dbus-daemon 1.16.2, on a private bus with a service file for `com.usepitboard.Pitboard`: a
  `Properties` on that name with no owner had the bus start the service; a call to a unique
  name with no owner answered `org.freedesktop.DBus.Error.ServiceUnknown` and started nothing.
  So the widget reads and asks only the owner's unique name, which it takes with `GetNameOwner`
  once `DBusServiceWatcher` says the name is registered. The watcher asks the bus itself,
  `NameHasOwner` and `NameOwnerChanged` through `QDBusServiceWatcher`
  (`dbusservicewatcher.cpp:56-106`), which starts nothing.
- `SessionBus.asyncCall` with a `signature` sends at once and introspects nothing
  (`dbuspendingreply.cpp:21-41`). `ActivateAction`'s `(sava{sv})` takes the feed's id as the one
  variant of its `av` (`dbusencoder.cpp:54-129`), as `app.switch-from-outside` takes a string.
- With no `compactRepresentation`, a `PlasmoidItem` that has a full one takes the shell
  package's default compact representation, Plasma's own icon of `Plasmoid.icon` (libplasma
  v6.4.0 `src/plasmaquick/appletquickitem.cpp:703-710`). So the widget sets the icon and draws
  only its full representation.
- KPackage 6.13.0 resolves a package named by its id as the folder of that name under each data
  folder's package root (`src/kpackage/package.cpp:470-492`), so the widget's folder is its
  `KPlugin.Id`, which `check-linux-app.sh` holds.
- PlasmaComponents 3's `ProgressBar` draws `widgets/bar_meter_horizontal` in the colours of the
  item's `Kirigami.Theme` (libplasma v6.4.0
  `src/declarativeimports/plasmacomponents3/ProgressBar.qml`), so a bar takes the colour of its
  limit's step from a `highlightColor` set on it: neutral for low and negative for out, as the
  pictures show.

The Plasma each distribution ships: Debian 13 6.3.6, so no widget; Ubuntu 26.04 6.6.4 at its
release and 6.6.6 in its updates (Launchpad); Fedora 43 6.4.5 at its release and 6.7.5 in its
updates; Fedora 44 6.6.4 and 6.7.5 (dnf in each image, 29 October).

`tst_widget.qml` runs under `qmltestrunner -platform offscreen` in Fedora 44's image, through
`check.sh widget`, with KDE's platform theme (`QT_QPA_PLATFORMTHEME=kde`, plasma-integration
6.7.5) reading BreezeLight's, then BreezeDark's colour scheme from `kdeglobals`. Its stand-in
for the app, `tests/stand-in.py` on PyGObject, owns the name only once the bus starts it from a
service file `run-tests.sh` writes after the bus has started, which dbus-daemon 1.16.2 finds at
the activation. Measured there, the tests failing first to compile with no widget: the widget
shows the account in use, each of its limits and the feed's words alone; its Switch asks
`ActivateAction("switch-from-outside", [<id>])`; and with no Pitboard running it asks nothing,
the bus starting the stand-in only for the test's own `StartServiceByName`. A TestCase is not
drawn unless made visible: its items said they were hidden and drew nothing until then.

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

The Plasma widget, `share/plasma/plasmoids/com.usepitboard.Pitboard/` with its `metadata.json`
and `contents/ui/{main,Feed,FullRepresentation,AccountRow}.qml`, is installed while Meson's
`plasma_widget` is on, as it is by default, and packaged apart, since it needs Plasma 6.4.
`check-linux-app.sh` holds a tree that has it to those files. `pitboard` itself, beside
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
- desktop-file-validate 0.28 passes the launcher and KRunner's runner, its `Type=Service`
  included. librsvg 2.60's `rsvg-convert` writes only a regular file, not `/dev/null`.
- Meson 1.7.0 configures a project that names no compiler language, which the app's, that
  only runs Cargo, is. `meson install --destdir` writes the install layout above and nothing
  else, `bin/pitboard-app` at mode 0755, and the release's program holds neither `.Devel` nor
  `pitboard-debug`. appstreamcli 1.0.5 passes the metainfo with `validate --no-net`.
- `cargo hack check --rust-version -p pitboard-linux --all-targets` passes on Rust 1.92.

## The checks' images

`apps/linux/scripts/in-container.sh` and CI's jobs run the checks in these images, measured on
arm64 in Docker 29.8.2 on the owner's Mac on 28 October 2026, as uid 1000:

| | Image | Packages from | GTK, libadwaita, GLib | mutter | Mesa, LLVM |
|---|---|---|---|---|---|
| Ubuntu 26.04 | `ubuntu:26.04@sha256:0e73b714…` | snapshot `20261028T000000Z` | 4.22.4, 1.9.1, 2.88.0 | 50.1 | 26.0.8, 21.1.8 |
| Debian 13 | `debian:13@sha256:8c4733a9…` | snapshot `20261028T000000Z` | 4.18.6, 1.7.6, 2.84.4 | 48.7 | 25.0.7, 19.1.7 |
| Fedora 44 | `fedora:44@sha256:43b29f65…` | `fedora` and `updates`, that day | 4.22.5, 1.9.4, 2.88.4 | 50.6 | 26.2.5, 22.1.8 |

- Each image has GVfs, as GNOME desktops do ([Links](#links)), since 29 October 2026: 1.60.0 in
  Ubuntu and 1.57.2 in Debian from the same snapshot, and 1.60.3 in Fedora, whose packages that
  day had the versions above. GVfs's daemon starts on the private bus when GIO first asks, and
  says "A connection to the bus can't be made" as the bus ends with each run.
- apt 3.2.0 (Ubuntu) and 3.0.3 (Debian) take `--snapshot <id>` with `install --update`, and
  read every suite of the image's sources from `snapshot.ubuntu.com/ubuntu/<id>`, or
  `snapshot.debian.org/archive/debian/<id>` and `debian-security/<id>`, with no change to the
  sources. On arm64 Ubuntu, whose sources name `ports.ubuntu.com`, apt reads
  `snapshot.ubuntu.com/ubuntu` too, which serves arm64's indexes and packages. A snapshot holds
  its versions: at `20260601T000000Z` Ubuntu's apt takes libgtk-4-1 4.22.2+ds-1ubuntu1 and
  mutter 50.1-0ubuntu2.1, where the archive has 4.22.4+ds-0ubuntu0.1 and 50.1-0ubuntu2.5.
- Debian's `trixie-updates` and `trixie-security` Release files are valid for a week. At
  `20260601T000000Z` apt refused both as expired 142 days before, and took them with
  `Acquire::Check-Valid-Until=false`, the option snapshot.debian.org's own page gives for a
  snapshot older than a dozen days. Ubuntu's snapshot of that day expired nothing.
- The Ubuntu and Debian images hold no CA certificates, and apt reads a snapshot over https
  alone ("certificate verify failed"), so `install-deps.sh` takes `ca-certificates` from the
  image's own archive first. That call also leaves the archive's Release files in place, which
  apt takes the snapshot's address from: in an image with no lists, one `install --update
  --snapshot` fetched the archive's own indexes alone, then found no package.
- Fedora keeps no snapshot. Its image enables `fedora`, `updates` and
  `fedora-cisco-openh264`, and `install-deps.sh` names the first two.
- Debian 13 ships no Adwaita Sans: gsettings-desktop-schemas 48.0-1 overrides `font-name` with
  Cantarell 11 (bugs.debian.org/1093547), which `GSETTINGS_BACKEND=memory` keeps, so its image
  takes `fonts-cantarell`. Ubuntu 26.04 ships `fonts-adwaita` 49.0, Fedora 44
  `adwaita-sans-fonts`.
- Fedora's image has `useradd` but not `setpriv`, which is util-linux's, beside the
  util-linux-core it ships. Ubuntu's and Debian's have both. Ubuntu's image has a user at
  uid 1000, `ubuntu`; the others' Containerfiles make one, since dbus-daemon needs it.
- Fedora's image also has the Plasma widget's packages: `plasma-workspace`, which
  holds `org.kde.plasma.workspace.dbus`, `libplasma`, `kf6-kirigami`, `kf6-qqc2-desktop-style`,
  `breeze-icon-theme`, `qt6-qtdeclarative-devel` for `qmllint` and `qmltestrunner`, and
  `python3-gobject-base`: 362 packages more, 349 MiB, 1 GiB installed, plasma-workspace 6.7.5,
  KF 6.31.0 and Qt 6.11.2 on 29 October. They bring xdg-desktop-portal, which the private bus
  then starts in each run; its document portal fails to mount FUSE in the container and says
  so, and the tests pass.
- Debian's 397 packages, 198 MB, came from snapshot.debian.org in 15 s, and Fedora's 414,
  279 MiB, in a 93 s step. snapshot.ubuntu.com answered each request in about 2 s, a cached one
  included: Ubuntu's 400 packages, 215 MB, took 3 min 39 s, and in two other runs apt waited
  more than ten minutes on one file, which curl fetched in 7 s. On 29 October its 453 packages
  with GVfs, 223 MB, took 1 h 18 min.

## The headless display

`apps/linux/scripts/headless.sh` runs the tests in mutter's headless Wayland compositor, as
GTK's own CI does (`.gitlab-ci/run-tests.sh`, GTK main, read for the plan on 28 October 2026).
Measured in each image above, as uid 1000:

- mutter 48.7, 50.1 and 50.6 each run `--headless --wayland --no-x11 --virtual-monitor
  1280x800` under `dbus-run-session`, with no seat (`XDG_SEAT` unset, no `/run/systemd/seats`)
  and no logind. Each says "Created surfaceless renderer without GPU" and "Added virtual
  monitor Meta-0", warns only that `org.freedesktop.locale1` and colord are absent, and gives
  its command `WAYLAND_DISPLAY=wayland-0` in the `XDG_RUNTIME_DIR` the script made, mode 0700.
- GTK on that display takes EGL 1.5 from Mesa and an OpenGL ES 3.2 context
  (`GDK_DEBUG=opengl`), on llvmpipe: eglinfo, on the same Wayland platform, names the
  renderer as the table's last column has it. Mesa 26, Ubuntu's and Fedora's, first tries
  zink, and warns on stderr that Vulkan has no driver, before it takes llvmpipe.
- GTK 4.16 made Vulkan the default renderer on Wayland (`gtk-4.16.0.news`). With
  `GSK_RENDERER` unset, GTK 4.22.4 and 4.22.5 try Vulkan, which no image has a driver for,
  pass over GL ("Not using GL: renderer is llvmpipe", `GSK_DEBUG=renderer`), try Vulkan again,
  and land on GskGLRenderer. GTK 4.18.6 passes over GL the same way and lands on it too. So the
  script names the renderer, rather than leave it to what an image lacks. `GSK_RENDERER=help`
  lists `opengl` and `gl` in each and `ngl` in 4.18.6 alone. `gl` warns in 4.18.6 that the old
  GL renderer is gone, `ngl` warns in 4.22 that it was renamed to `gl`, and `opengl` takes
  GskGLRenderer in each with no warning, so the script sets `opengl`.
- On the private bus, the first GTK client's call starts `org.a11y.Bus` from at-spi2-core's
  service file (2.56.2 in Debian, 2.60.4 in Ubuntu, 2.60.7 in Fedora), whose bus is a
  socket under the script's `XDG_RUNTIME_DIR/at-spi`. `org.a11y.atspi.Registry` starts on that
  bus when asked, and with `gtk4-builder-tool preview`'s window open its root lists that
  program as its one child. So an AT-SPI client finds the app on the private bus, and nothing
  of a desktop's.
- `gtk4-builder-tool validate` needs a display: without one it says "Could not initialize
  windowing system" and exits 1 on a valid file too. Under the script it passes a valid file of
  GTK's types alone, and refuses a template whose parent is libadwaita's (Decisions).
- nextest 0.9.146 writes a profile's JUnit report under the workspace's `target/nextest`, not
  the archive's target folder, in a run from an archive too: a fresh user's run in a checkout
  root owns failed with "failed to create store dir at `/src/target/nextest/ci`" until that
  folder was the user's. So CI's jobs make it the test user's.

## What the app exports

Measured in Ubuntu 26.04's image, GTK 4.22.4 and GLib 2.88.0, by the UI tests, which read
`org.gtk.Actions` `DescribeAll` at every object path under the app's own that introspection
lists (`the_app_exports_only_its_outside_actions`):

- At the app's path GApplication exports its action group: `notice-clicked` and
  `switch-from-outside`, each taking a string, and `add-account` and `refresh`, taking nothing.
  `org.freedesktop.Application`'s `ActivateAction` reaches these alone.
- With the window an `AdwApplicationWindow`, GTK also exported `org.gtk.Actions` at
  `<app path>/window/1`, empty while the window added no `win.` action: GtkApplication exports
  each application window's own group. The window is an `AdwWindow`, with which no window path
  is exported at all.
- The window's actions, installed on its class (`gtk_widget_class_install_action`) under
  `pitboard.`, are on no path: GTK keeps them in the widget's action muxer. Its keys are bound
  on the class too.
- At `<app path>/WidgetFeed` the app serves its widget feed, which exports no `org.gtk.Actions`
  ([The Plasma widget](#the-plasma-widget)).

## The UI tests

Measured in Ubuntu 26.04's image, GTK 4.22.4, at-spi2-core 2.60.4 and atspi 0.30.0 (crates.io),
the UI tests' client, on zbus 5's own async-io executor:

- GTK names the app's AT-SPI root by the program's name, `pitboard-app`, though
  `g_set_application_name` named it Pitboard before GTK started. A window's name is its title.
- A program holding no window has no node under the registry's root, so `--sign-in` is seen
  to show nothing until a window opens.
- `GetActions` gives an action's localized name, "Click", and `GetName` its own, `click`, which
  the tests press by.
- A `GtkMenuButton` is a `Button` holding a `ToggleButton`, which has the `click`. Its popover
  menu is a `Menu` under it once open. Each item is a `MenuItem` whose `Name` is empty, labelled
  by a label whose `Text` is the item's words: GtkModelButton gives its label the presentation
  role and points `LABELLED_BY` at it (`gtk/gtkmodelbutton.c` at 4.22.4). Pressing the toggle's
  `click` opens the popover, and an item's `click` activates its action
  (`help_in_the_main_menu_opens_the_docs_in_the_browser`), which settles M12's popover
  `DoAction`.
- The header bar's window controls are `Button`s named "Close", "Maximize" and "Minimize".
- Mesa keeps a shader cache in `$XDG_CACHE_HOME/mesa_shader_cache` for any program it draws for
  with GL on llvmpipe, so a launch's fresh cache folder holds it once a window has been drawn.
  The tests that hold every home as a fixture found it leave that folder alone, and nothing
  else.

## The pictures

`tests/pictures` draws each surface in-process, as GTK's reftests do
(`testsuite/reftests/reftest-snapshot.c`): the window's `GtkWidgetPaintable` snapshotted at
twice its size, rendered by the window's own renderer into a texture and saved as PNG, light and
then dark by `AdwStyleManager`. Measured in Ubuntu 26.04's image:

- AdwSpinnerPaintable follows the frame clock of the widget it is given whatever
  `gtk-enable-animations` says, and keeps still at its first angle with no widget
  (`src/adw-spinner-paintable.c` at libadwaita 1.7.6). So the pictures turn animations off and
  hold each spinner still, and say every clock time as one fixed time.
- A failure to read names the path of the account list, so the pictures make each world in a
  folder of the same name every run, `/tmp/pitboard-linux-pictures/<state>`.
- With those, each picture is the same in three runs, with `GSK_RENDERER=opengl`, GskGLRenderer
  on llvmpipe, which `headless.sh` names, and with `cairo` alike. Every picture differs between
  the two, so one renderer is kept for all: GL, which a desktop's GTK draws with where Vulkan has
  no driver, where cairo is GTK's fallback.
- Snapshotted at twice the window's size, 1200 by 1280 pixels for the 600 by 640 window, the
  text and the symbolic icons are drawn at that scale, with edges a pixel wide, not enlarged
  from 1x (looked at in `linux-accounts-noTool-dark.png`).
- Drawn beside another, a picture's window lost the focus to it between its two drawings, which
  dimmed its header bar, so the pictures are drawn one at a time (`.config/nextest.toml`).
- A dialog comes up over the frames after it is mapped, and a window with a frame asked for and
  not drawn yet snapshots as nothing, so each picture is snapshotted right after a frame it asked
  for is drawn. A focused field's blinking cursor would be in one drawing and not the next, so
  the pictures turn the blink off.
- A `GtkLabel` sets its Pango layout's width as it is allocated, so `pango_layout_is_ellipsized`
  says whether a title shows whole. A picture fails on a label of a header bar it shows cut short.

## The accounts page

Read at GTK 4.18.6 and libadwaita 1.7.0, and measured in Ubuntu 26.04's image, GTK 4.22.4 and
libadwaita 1.9, by the tests named:

- A `GtkLevelBar`'s filled block takes, as a style class, the name of the first offset its value
  is at or under (`update_level_style_classes`, `gtk/gtklevelbar.c` at 4.18.6). So a limit's bar
  has one offset, at its whole, named for the step the model says the limit is at, and the app's
  style sheet tints `plenty`, `low` and `out` with libadwaita's accent, warning and error colours.
  GTK's own three offsets, which follow the value, are taken off
  (`each_account_shows_its_limits_with_their_level`).
- `AdwPreferencesPage` adds a group only at its end, and `AdwPreferencesGroup` a row only at
  its end, at 1.7: `adw_preferences_group_bind_model` and `adw_preferences_group_get_row` are
  1.8's (libadwaita-rs 0.9.2 gates both on `v1_8`). So the notices are a `boxed-list`
  `GtkListBox` bound to the store's list model inside their group, and where an account comes in
  between others, the groups after it are added again behind it.
- `AdwPreferencesGroup`'s title and description are markup (`use-markup` is set on both labels
  in `src/adw-preferences-group.ui` at 1.7.0), so the app escapes what it puts in them.
- Over AT-SPI a group is a `Grouping` named by its title, which its label names it by whatever
  label it is given. A `GtkButton` with a label is named by it the same way, so the button a
  group's header shows takes the model's spoken words only once that relation is reset: until
  then it was named "Switch"
  (`the_account_in_use_says_so_and_another_offers_its_button`).
- A view-more `GtkMenuButton`'s popover lists the group's actions over AT-SPI in the menu's order
  once its toggle is clicked (`an_accounts_other_actions_are_in_its_menu`).
- GTK has no label that counts down by itself, as SwiftUI's timer text does, so a notice's row
  asks the model for its countdown's words each second while it is shown, and stops at nought.
- An `AdwDialog`'s default, which Return activates through the `default.activate` action
  (`src/adw-dialog.c` at 1.7.0), is a `GtkButton`, which GTK activates only once it is realized:
  an alert's buttons were realized a frame after the alert was mapped
  (`a_question_answers_with_its_default_and_close_responses`).

## The dialogs

Read at libadwaita 1.7.0 and GTK 4.18.6, and measured in Ubuntu 26.04's image by the tests named:

- `AdwAlertDialog` closes on any response: a response's button calls `adw_dialog_close` and only
  then emits `response` (`button_clicked_cb`, `src/adw-alert-dialog.c`). So the sheets that stay
  up until the model closes them, saying inside what went wrong, are `AdwDialog`s with their
  buttons in the header bar: Add Account, Sign In Again, Name, Rename and Clean Up
  (`a_failure_stays_in_the_dialog`).
- An `AdwDialog` in a window takes Escape through its sheet's shortcut, which calls
  `adw_dialog_close` (`src/adw-floating-sheet.c`, `src/adw-bottom-sheet.c`): with `can-close` off
  that is a `close-attempt`, its header bar showing no close button, which the sign-in dialog
  answers as its Cancel where the model says Escape is that, and Name, Rename and Clean Up as
  their Cancel (`escape_while_signing_in_cancels_it`,
  `rename_asks_in_a_dialog_whose_header_holds_its_buttons_and_escape_is_cancel`).
- `AdwSpinner` always spins: its `AdwSpinnerPaintable` animates on the widget's frame clock with
  `follow-enable-animations` off (`src/adw-spinner-paintable.c`). The sign-in dialog shows the
  same paintable in a `GtkImage`, which libadwaita documents for a spinner where a widget of its
  own does not fit, so the pictures hold it still as they hold a status page's.
- A dropped link reaches a `GtkDropTarget` taking `G_TYPE_STRING` as a string, whatever it was
  dragged from: a browser's address, or text (`a_dropped_link_opens_open_link`). Pasted text
  comes with the white space it was copied with, which the model trims as Foundation trims
  shared text.
- No fixture world's sign-in fails to start, so `linux-signIn-failed` is a sign-in that fails as
  a person's can: Claude Code refusing a code typed back without its state.
- A header bar keeps its buttons' widths and ellipsizes its title: at a `content-width` of 440
  the sign-in dialog showed "Signing In to Claude …" between Cancel and Submit Code. At 500 the
  title is whole and its centre 2 logical pixels left of the dialog's (`linux-signIn-code`).

## Preferences

Read at libadwaita 1.7.6 and GTK 4.18.6, and measured in Ubuntu 26.04's image by the tests named:

- `AdwPreferencesGroup` puts a row in its boxed list and any other widget in the box after it
  (`adw_preferences_group_add`, `src/adw-preferences-group.c:399-414`). So the line that says
  how a setting stands, its note or why its last change failed, is a label under the group's
  rows, and a warm-up account's switch, added later, still goes in the list above it.
- libadwaita 1.7 has no time picker, so each of the warm-up's hours is an `AdwSpinRow` over the
  minutes of the day whose `output` says the time as the person's clock does. A spin button's
  update reads its text back through `input`, and with no handler parses it as a number and,
  under its default policy `GTK_UPDATE_ALWAYS`, keeps what it parsed even where the text did
  not read as one (`gtk_spin_button_update`, `gtk/gtkspinbutton.c:2550-2588`): "6:00 AM" would
  come back as 6 minutes. So the hours are not editable and answer `input` with their own
  value, and the share, which takes typing, is `GTK_UPDATE_IF_VALID` with an `input` that reads
  "95%" as 95. Midnight is 00:00 there, which the core reads as a span's end
  (`duty::warm::Range::new`).
- A control set to the value it shows sends nothing, and the model's values drawn into the
  controls send nothing (`a_setting_sends_only_a_value_that_differs`).

## The keyboard shortcuts

Read at GTK 4.18.6 and libadwaita 1.7.6, and measured in Ubuntu 26.04's image, GTK 4.22.4, by
`every_shortcut_the_window_answers_is_listed_with_its_keys`, which reads every shortcut
controller of the window and of its navigation view:

- The window binds Ctrl+N, Ctrl+R, Ctrl+Comma, Ctrl+?, Ctrl+W and Ctrl+Q on its class to its
  own actions (`window/keys.rs`). Ctrl+? is `question` with Control, which GTK matches with the
  Shift that types it consumed, and `gtk_accelerator_get_label` says "Ctrl+?".
- F10 is GtkWindow's: a capture-phase shortcut controller, `gtk-window-menubar-accel`, calls
  `gtk_window_activate_menubar` (`gtk/gtkwindow.c:1808-1815`), which pops up the first
  `GtkMenuButton` with `primary` set where the window has no menu bar (`:5992-6031`); a primary
  menu button adds itself to its root's list as it is rooted (`gtk/gtkmenubutton.c:1395-1456`).
- Alt+Left, the Back key and Escape go back in `AdwNavigationView`, bound on its class
  (`src/adw-navigation-view.c:2006-2015`; Escape unless `pop-on-escape` is off).
- `AdwShortcutsDialog` is libadwaita 1.8's, and GTK 4.18 deprecates `GtkShortcutsWindow` and
  `GtkShortcutLabel` ([Decisions](#decisions-and-what-they-rest-on)), so Keyboard Shortcuts is
  an `AdwDialog` of `AdwActionRow`s, each with its keys in a label.

## About

`adw_about_dialog_new_from_appdata` (`src/adw-about-dialog.c:1983-2140` at libadwaita 1.7.6)
reads the metainfo the program carries in its GResource through AppStream: the icon from its
`<id>`, the name, the developer's name, the version of its newest release, the website, support
and issue pages from its `homepage`, `help` and `bugtracker` addresses, and the licence from its
`project_license`. Asked for the release notes of a version the metainfo has no release of, it
logs a critical, which a build between releases is, so About asks for none, and its version is
the program's own. Its icon is the build's id, `.Devel` in a debug build. GtkApplication adds
the `icons` folder under its resource base path to the icon theme as the app starts
(`gtk/gtkapplication.c:213-218` at GTK 4.18.6), which a window the pictures draw has no
application to do, so the pictures add it.

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
