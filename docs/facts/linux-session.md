# The Linux session machines

Measured by hand on 30 October 2026 on the Linux session machines, arm64 OSMates VMs on the
owner's Mac. The bundle was `linux-facts.yml` session run 41065936222 at 23fd0dbb, the commit
that holds `Release 1.0.0-beta.8`, its sums checked on the Mac and in each guest. Fedora with
GNOME, Fedora with KDE Plasma and the first half of Ubuntu ran on earlier bundles of the same
release, run 40986051417 and run 41021104026 at db03f566, before #361, #362, #363 and #364;
what those changed was measured again on the last one. Claude Code 2.1.314 and Codex 0.166.0
came from npm on each machine's own Node. Each machine's session account was made for it, uid
1000, signed in at the display manager with a password. The Claude and ChatGPT accounts were
the owner's own, not throwaway ones, and their emails and one-time codes were typed in the
guests for the owner, both by the owner's choice; each machine was deleted after its session.
The notes are kept out of the repository, which names no account.

| Machine | Desktop | GTK, libadwaita, GLib | systemd | Secret Service | Tray |
|---|---|---|---|---|---|
| Fedora 44 | GNOME Shell 50.6, GDM | 4.22.5, 1.9.4, 2.88.0 | 259.5 | gnome-keyring 50.0 | none |
| Fedora 44 | Plasma and KWin 6.7.5, Plasma Login Manager | 4.22.5, 1.9.4, 2.88.0 | 259.5 | ksecretd, kf6-kwallet 6.31.0 | kded6's watcher |
| Ubuntu 26.04.1 | GNOME Shell 50.1, GDM 50.1 | 4.22.4, 1.9.1, 2.88.0 | 259.5 | gnome-keyring 50.0 | `ubuntu-appindicators@ubuntu.com` |
| Debian 13 | GNOME Shell 48.7, GDM 48.0 | 4.18.6, 1.7.6, 2.84.4 | 257.13 | gnome-keyring 48.0 | none |

All ran under Wayland. Session incidents, OSMates' and not Pitboard's: the guests' TLS
ClientHello with X25519MLKEM768 at an MTU of 1500 got no answer, as in the Windows session, so
each machine ran at 1400; a suspend the GNOME guest asked of logind failed, after which GNOME
Shell stopped answering until a restart; and OSMates' updates stopped the KDE machine twice,
which resumed each time from its saved memory.

## The packages

- `dnf install` of the two rpms installed those two and nothing else on Fedora Workstation and
  on Fedora's KDE group: `pitboard` 12 files, `pitboard-app` 27. `apt-get install` of the two
  debs took 12.3 s on Ubuntu and 2.6 s on Debian and pulled nothing else in: 8 and 19 files.
  `pitboard`'s Recommends of `pitboard-archive-keyring` was listed, and not installed from
  local files.

## Sign-in and the Secret Service

- GNOME: `org.freedesktop.secrets` is gnome-keyring-daemon's, its "Login" collection unlocked
  at sign-in under the `default` alias; gkr-pam unlocked a daemon already running 0.94 s after
  Enter. On Debian, in the first session after the account's first sign-in, `Collections` listed
  the login collection and `default` named it, but its path answered "Object does not exist":
  an enrol had nowhere to park, and was refused with `credential_store_unreadable`. After a
  logout and a sign-in it answered. `org.gtk.Notifications` is gnome-shell's. On Fedora nothing owns
  `org.kde.StatusNotifierWatcher`; on Ubuntu gnome-shell does, through the extension.
- Plasma: `org.freedesktop.secrets` is ksecretd's, `default` the `kdewallet` collection,
  unlocked at sign-in. `org.kde.StatusNotifierWatcher` is kded6's and
  `org.freedesktop.Notifications` plasmashell's; nothing owns `org.gtk.Notifications`.
- Claude Code 2.1.314 keeps its login in `$HOME/.claude/.credentials.json` on Linux, 559
  bytes, 0600, with no Secret Service item; Codex in `$HOME/.codex/auth.json`, 0600. Pitboard
  parks each login as one item of schema `com.usepitboard.Pitboard.Park` in the default
  collection, the `kdewallet` one on Plasma, where `doctor` still calls it "the login keyring".

## Starting the app

- From GNOME's overview and from Kickoff the desktop starts the app by D-Bus activation. On
  Fedora, whose dbus-broker activates through systemd, it runs in
  `dbus-:1.2-com.usepitboard.Pitboard@0.service`; on Ubuntu and Debian dbus-daemon forks it
  inside `session.slice/dbus.service`. Its `PATH` was `$HOME/.local/bin:$HOME/bin:/usr/local/bin:/usr/bin`
  on Fedora (and `/bin` on Plasma), and the system's own folders on Ubuntu and Debian, with
  `/snap/bin` on Ubuntu and no `$HOME/.local/bin` on either; `WAYLAND_DISPLAY`, `DISPLAY` and
  `XDG_CURRENT_DESKTOP` set, and no `XDG_ACTIVATION_TOKEN` left in it. Its first window came to
  the front each time. A `pitboard://` link started it the same way when it was not running.
- Claude Code's sign-in, and the browser it opens when none runs, start inside the app's unit or
  cgroup on Fedora's GNOME, where the browser outlived the unit's stop at logout. On Plasma
  Firefox ran in a unit of its own, and on Ubuntu the snap's in a scope of its own.
- `org.freedesktop.Application.Activate` with no token raised the window on Fedora's GNOME 50.6.
  GNOME 50.1 on Ubuntu posted '"Pitboard" is ready' instead, and KWin kept Firefox in front.

## The status item and its menu

- The app registers `org.kde.StatusNotifierItem-<pid>-1` at `/StatusNotifierItem`, category
  ApplicationStatus, `ItemIsMenu` false, menu at `/MenuBar`, tooltip "Pitboard" over a line per
  account in use.
- The bundles before #364 put each entry of `GetLayout`'s children in two variants. The
  AppIndicator extension threw on each read and made a new client at once, 263 `GetLayout`
  in 2 s with nothing clicked, GNOME Shell at about 91% CPU and 1.2 GB after 20 minutes, Firefox
  "Not Responding"; Plasma drew no menu for it either.
- With each entry in one variant both draw the menu: Open Pitboard, each tool's name as a
  disabled header where there are two, its accounts as radio entries with the one in use
  checked, then Add Account…, Refresh, Preferences, Help and Quit. A click on an entry switched,
  in 1.06 s through Plasma, with no notification.
- "Open Pitboard" with Firefox in front: Plasma and the extension each called
  `ProvideXdgActivationToken`, then `Event`, and the window came in front. A left click on the
  item in Plasma's panel did too. The extension opens the menu on a primary click, after the
  double-click time, and calls `Activate` on a double click, which OSMates' pointer did not
  make: `Activate` from the extension is not measured.
- `plasmashell --replace` kept the item in kded6's watcher and drew it again 1.06 s later.
  Turning the extension off and on, the item registered again 0.057 s after the watcher came
  back. With no watcher the app went on running.
- Quit from the menu ends the app and, with it, a Claude Code sign-in under way.

## Search

- GNOME Shell's search gives "Switch to <name> (Claude Code)" with the account's limits, or "In
  Use"; activating one switched it, with no window and no notification.
- KRunner 6.31.0 gives Pitboard's match at category relevance 50, below an application whose
  name the query starts. Run, it calls `Teardown`, then `SetActivationToken` with a token, then
  `Run`; the runner dropped that run until #362, and since then it switches.

## Notifications

- GLib posts through `org.gtk.Notifications.AddNotification` on GNOME and through
  `org.freedesktop.Notifications.Notify` on Plasma, where nothing owns `org.gtk.Notifications`.
- GNOME Shell and plasmashell drew the notices as banners or popups with title, the tool's name
  and body; Plasma drew the limit notice's one button, "Switch to <name>". A click on a banner,
  a popup or a notice in GNOME's list, Firefox in front, brought the window in front. A click on
  the button is not measured: the popup had gone first, and a notice is told once per window.

## Start at Login

- The entry is `$HOME/.config/autostart/com.usepitboard.Pitboard.desktop`, 144 bytes,
  `Exec=/usr/bin/pitboard-app --sign-in`, `TryExec=/usr/bin/pitboard-app`.
- GNOME, on Fedora, Ubuntu and Debian: gnome-session starts it itself in
  `app-gnome-com.usepitboard.Pitboard-<pid>.scope`, about at `graphical-session.target`; the
  generated `app-com.usepitboard.Pitboard@autostart.service` stays inactive. With no
  StatusNotifierWatcher the app ended as designed; with the extension's it stayed, its item
  registered, no window.
- Plasma: systemd started it as `app-com.usepitboard.Pitboard@autostart.service`, 2.70 s after
  `plasma-kded6.service` and 0.34 s after `graphical-session.target`; it registered its item and
  showed no window. systemd's generator writes no unit for the entry when `TryExec` names a
  missing file.

## Logout

- Plasma: KWin closes each window and waits on it. With only the status item, the main window,
  or since #363 Preferences over it, the login screen came within 14 s. Before #363, with
  Preferences open, KWin's close closed only the dialog, the window stayed, and Plasma said "The
  following applications did not close: • Pitboard", logging out after 2 minutes.
- GNOME on Ubuntu with Preferences open, and on Debian: GDM within 7 s.

## Claude Code and Codex

- `claude auth login` runs as comm `claude`, Codex as `codex`; the browser's localhost callback
  finishes a sign-in.
- Switching to a login added through Pitboard, with nothing live, works; the Windows session's
  refusal is gone.
- An interactive Claude Code session statx'es `.credentials.json` at each request, with no
  inotify watch, and opens it only once it changed: `/status` named the new account 2.3 s after
  `pitboard use` began, well inside the 33 seconds Pitboard says.
- One ChatGPT sign-in with two workspaces is two Codex accounts, the second named with `-2`.
  The business workspace's plan shows raw, `Self_serve_business_usage_based`.

## Renewal and the watch service

- Keep Saved Sign-Ins Fresh writes `pitboard-renew.service` (283 bytes, 0600, a oneshot of
  `/usr/bin/pitboard renew --scheduled` after and wanted by `graphical-session.target`) and
  `pitboard-renew.timer` (154 bytes, 0600, daily, persistent, up to an hour late) in
  `$XDG_CONFIG_HOME/systemd/user`, both enabled, the same on each desktop and on Debian's
  systemd 257. Runs by hand took 0.03 to 0.12 s.
- Watch in the Background, offered only where the status item cannot be shown, writes
  `pitboard-watch.service` (311 bytes, 0600, `pitboard watch --scheduled --no-switch`,
  restarted on failure after 60 s, wanted by `default.target`), started at once and again at
  each sign-in.
- `pitboard schedule install` writes the path it was found at: `/usr/sbin/pitboard` under a
  `PATH` with `/usr/sbin` first, where Fedora 44's `/usr/sbin` links to `bin`.
- Under linger, nobody signed in: the user manager ran 6.5 s into the boot, the watch service
  started with gnome-keyring-daemon on the session bus and the collection locked; a renewal
  said "No parked login was due." and `doctor` said the keyring was locked, with no prompt.
  Signing in unlocked that daemon's collection; the watch service kept its process.

## The clock

- No desktop set `org.gnome.desktop.interface` `clock-format`. GSettings' default follows
  `LC_TIME`: '12h' under en_US, '24h' under en_GB and C.UTF-8, which Ubuntu's and Debian's
  cloud images set. Plasma's Region writes `LC_TIME`
  to `plasma-localerc`, which the next session's environment carries, so the app showed 24 hour
  times with Plasma's clock.

## Links

- `xdg-open 'pitboard://open?url=https://claude.ai/chat/…'`, the default handler
  `com.usepitboard.Pitboard.desktop`, reached the running app: Open Link, "Open in Your
  Browser".

## The Plasma widget

- Installed from `apps/linux/plasmoid` with `kpackagetool6`, plasmashell 6.7.5 loaded it with
  no QML error and drew it in a panel and on the desktop from the running app's feed: every
  account in a scroll view, a Switch beside each one ready, which switched. With no app, the
  desktop widget draws nothing and the panel's tooltip says "Pitboard".

## Uninstall and removal

- Uninstall from the app took 2 to 6 s. It deleted `$HOME/.pitboard`, the app's data folder,
  the autostart entry, the units and their links, and the parked items, and kept
  `.credentials.json` and `auth.json`. Linger stayed on. It then named the package command:
  `sudo dnf remove pitboard-app pitboard`, `sudo apt remove pitboard-app pitboard`.
- After that command nothing named for Pitboard was left under `/usr`, `/etc`, dpkg's records or
  `$HOME`, and no `x-scheme-handler/pitboard` default.
- Where nothing shows the status item, closing the window ends the app.

## Not measured here

- `PrepareForSleep` across a real suspend: the guests' suspends failed.
- The 5 s wait for `org.freedesktop.systemd1` on the session bus: the user manager owned it each
  time it was asked.
- The status icon's colour in either panel, a notification button's click, `Activate` from the
  AppIndicator extension, and the widget in Plasma's system tray.
