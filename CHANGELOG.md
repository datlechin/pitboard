# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `pitboard status`, its `--json` and the app show a Codex account's limits for one model under
  the model's name, as they show Claude Code's.
- `pitboard doctor`, **This Mac** and **This PC** name each Codex setting that limits which logins
  Codex keeps, such as `forced_chatgpt_workspace_id`, and the accounts it rules out.
- A Codex switch on Linux names the ChatGPT app among what still runs Codex, and says to quit it
  and open it again.
- `pitboard watch install` has the system's own service manager run `pitboard watch` from each
  sign-in on, and `pitboard watch status` and `pitboard watch uninstall` check and remove it.
- `pitboard doctor` reports the watch service, and `pitboard uninstall` stops and removes it.
- `pitboard watch --help` and `pitboard watch install` say that Anthropic's terms may cover
  automatic switching and warm-ups, and link to what they say.
- The install script ends with the command to run next and, where your login shell is zsh, the
  lines that make zsh load Pitboard's completions.
- The app's notification that a login was replaced outside Pitboard, and its notice in the
  window, offer **Sign In Again**.
- The app posts a notification once for each account whose parked login can no longer be used,
  with **Sign In Again**.
- The app notifies you once a limit of the account in use reaches the share in **Settings**, 95%
  at first, before it runs out, with **Switch** where another account has room.
- The app's **Settings** link **Is this allowed?** beside switching automatically and warm-up.
- The macOS app's **Settings** says when Pitboard's notifications are turned off, with **Open
  Notification Settings**.
- The app's **Name This Account** starts with a name from the account's email address.
- The macOS menu bar mark has a dot while the menu opens on something that needs attention.
- The Windows app's icon shows how much of its limit the account in use has left, and Windows'
  warning symbol while its menu opens on something that needs attention.
- The app's setup tips go on past a second account: opening Pitboard at login, then what
  happens when a limit reaches 95%, each with **Not Now**.
- The app offers **Use Pitboard with Codex** while Codex is installed and no Codex account is
  enrolled, and names the account Codex is signed in to once you choose it, where a machine with
  Claude Code signed in to nothing said **No Accounts**.
- The Windows app's tip to start when you sign in says where Windows puts its icon, with a link
  to Taskbar settings.
- The macOS app's **Settings** take the share a limit is reached at in a field beside its
  stepper.
- The macOS app's **Pitboard** menu offers **Check for Updates** while a window is open.
- The macOS app has an **Account** menu with what an account's shortcut menu offers, for the
  account selected, and **Forget** on Command-Delete.
- The macOS app's **Edit** > **Copy** copies the email address of the account selected.
- The Windows app copies an account's email address with Ctrl+C on its row, and its shortcut
  menu shows F2, Del and Ctrl+C beside the items they do.

### Changed

- **Breaking:** The public API of `pitboard-core` changes; [its
  documentation](https://docs.rs/pitboard-core/0.17.0/pitboard_core/) shows the new one.
- **Breaking:** Pitboard refuses with `login_restricted` to switch Codex to a login that Codex's
  `forced_login_method`, `forced_chatgpt_workspace_id`, `allowed_login_methods` or
  `allowed_chatgpt_workspaces` rules out, since Codex would sign out of it and delete it. Switch
  to an account those settings allow, or change them.
- **Breaking:** Pitboard refuses with `login_restricted` to switch Claude Code to an account of an
  organisation that a managed `forceLoginOrgUUID` leaves out, since Claude Code would refuse its
  login. Switch to an account of an organisation the setting names.
- Clicking one of the app's notifications on macOS opens **Accounts**, or **Settings** on
  **Limits** for what switching or warming up by itself came to, and the Windows app opens the
  same.
- The app's **Switch** button on a notification no longer brings Pitboard to the front on
  macOS.
- The app's **Switch Claude Code automatically** is now **Switch Automatically**, one of three
  choices in **When a limit reaches** beside **Do Nothing** and **Notify Me**, and the app keeps
  switching where it switched before.
- Under **Switch Automatically**, the app notifies you of Codex at the share, since it never
  switches Codex.
- Under **Switch Automatically**, the app's notifications about Claude Code say only what
  switching came to, without the one at 100% that offered **Switch**.
- The app's **Switched to work** notice with nothing under its title goes once open Claude Code
  sessions have followed the switch.
- The Windows app's icon tooltip lists each account in use with its limits, after **Needs
  attention** while its menu opens on something to look at.
- The app's **Settings** say what switching, warm-up and daily renewal do in one or two
  sentences.
- An account's shortcut menu in the macOS app leaves out **Warm Up Now** and **Sign In Again**
  while they cannot be chosen.
- **Open Pitboard** in the macOS app's menu no longer takes Command-0, which is **Actual Size**
  for an account's window.
- The macOS app's **Settings** has three panes, **General**, **Limits** and **Advanced**, and
  the Windows app's **Settings** the same three sections before **About**.

### Removed

- The app's account menu no longer offers **Warm Up Automatically**: **Settings** > **Limits**
  turns each account's warm-up on and off.
- The app's **Settings** no longer show **Renew Now**, **Runs** or **Scheduled in**: the app
  renews parked logins as it reads usage, and `pitboard renew` and `pitboard schedule status`
  keep both.

### Fixed

- Pitboard renews a parked Codex login at the issuer and as the client Codex 0.164.0 records in
  it, instead of always as Codex's own client.
- Pitboard takes no numbers from an OpenAI usage answer that names another account than the
  Codex login it asked about.
- Pitboard keeps a parked Codex login that OpenAI refuses to renew as another client than its
  own, and says why, instead of dropping it.
- A Codex switch names Codex's background app server where it runs from the standalone package an
  older Codex left, and says to restart it, instead of counting it as a `codex` session.
- Pitboard warns with `auth_overridden`, and the automatic switch holds back, while
  `CLAUDE_CODE_USE_MANTLE`, an Anthropic profile or managed settings that require the Cloud
  gateway make Claude Code sign in another way.
- On Linux, Pitboard and its install script work as root in a home root owns, as on a server
  whose only user is root, and refuse only under `sudo` or as root in another user's home.
- `pitboard --help` wraps to the terminal, lists the commands in the order you use them and
  ends with three examples.
- Where a login file behind the keychain or Credential Manager keeps running Claude Code
  sessions on their account, `pitboard use` says that `pitboard stow` puts it away, instead of
  sending you to `pitboard doctor`.
- On a Linux that systemd did not start, as in many containers, `pitboard schedule install`
  refuses with `schedule_unsupported` and says so, instead of a `home_unwritable` error that
  named `systemctl`.
- Shell completions leave out `pitboard renew --scheduled`, which only the daily renewal
  schedule runs.
- On Windows, `pitboard doctor` says where Codex keeps its home among Codex's checks, instead
  of under a second Codex heading above Pitboard's own.
- The app marks an account whose parked login Anthropic or OpenAI refused **Needs signing in
  again** and offers **Sign In Again**, in place of a switch to a login that no longer works.
- The app lines up the usage bars of every account, where a row that said no pace had wider
  bars.

## [1.0.0-beta.5] - 2026-10-23

### Changed

- Pitboard says to open Claude when a Claude Desktop change finds Claude's Reset App Data
  waiting to delete its data folder, instead of asking you to update Pitboard.

### Fixed

- Pitboard adds and switches Claude Desktop accounts with Claude 3.1623.0 and later, and
  leaves the names it does not move where Claude keeps them, such as `SharedStorage` and
  `design`, instead of refusing each name it has not read.
- Pitboard switches Claude Desktop accounts beside a `Claude-3p` folder that Claude does not
  run from, such as one an earlier build left holding only `claude_desktop_config.json`.

### Security

- Pitboard's HTTPS uses rustls-webpki 0.103.16, which fixes RUSTSEC-2026-0382.

## [1.0.0-beta.4] - 2026-10-20

### Added

- Pitboard warms up Claude Code accounts on Windows, from `pitboard warm`, `pitboard watch` and
  the app.
- Where Windows' Startup settings turned Pitboard off, the Windows app's **Start when you sign in
  to Windows** offers **Open Startup settings**, and reads the setting again as you come back to
  the window.
- The Windows app's **Settings** says when Windows turned Pitboard's notifications off, or every
  app's, and offers **Open notification settings**.
- The Windows app's window opens Pitboard's help with F1, and its **About** links to it.
- The Windows app's menu shows each account's most used limit after its name, as in
  **work · Max 20x · 42%**, where the menu bar app shows the account's limits under it.
- On Windows, an account's window closes with Ctrl+W, and its toolbar takes Alt+Left, Alt+Right
  and F5 or Ctrl+R for **Back**, **Forward** and **Reload**, as its page does.
- The Windows app installs the PowerShell completions, as `completions\pitboard.ps1` in its
  folder.
- On Windows, **This PC** names the last notification Windows refused to show, with when and
  the error it gave, until the next one is posted.

### Changed

- **Breaking:** The public API of `pitboard-core` changes; [its
  documentation](https://docs.rs/pitboard-core/0.16.0/pitboard_core/) shows the new one.
- On Windows, an account's window shows its page's title before the account's label.
- On Windows, a download goes on after you close its account's window, as on macOS, and
  opening that window again before the download ends brings back the one you closed.
- Pitboard says, as it does for `ANTHROPIC_API_KEY`, when a token in
  `/home/claude/.claude/remote/.oauth_token` or `.api_key` beside it signs Claude Code sessions
  in instead of the stored login, looking at the root of each fixed drive on Windows, and warms
  up no account while `.api_key` holds a key.

### Removed

- **Breaking:** On Windows, `pitboard doctor` has no `warm_up` check, and nothing refuses with
  `warm_up_unbuilt`, since warm-ups run there. A script that looks for either can stop.

### Fixed

- On Windows, Narrator reads a note that takes another's place in an account's window.
- Under a Windows contrast theme, the Pitboard window's minimize, maximize and close buttons
  take the theme's colours, where they stayed black or white.
- After a change of display scale, Pitboard for Windows loads its notification area icon again
  at the size the new scale asks for.
- On Windows, a beta of the app checks for updates only once a full release of Pitboard for
  Windows is out, and dims **Check now** until then, instead of ending each check in an error.
- Uninstalling the Windows app turns off daily renewal where its task runs the app's own
  `pitboard.exe`, and leaves a task that runs another copy, such as Scoop's.
- On Windows, Pitboard removes the temporary files a killed run left behind, each of which can
  hold a whole login, as it does on macOS and Linux.
- On Windows, Pitboard says to run `claude.cmd` or `codex.cmd` where npm installed the tool,
  since Windows PowerShell refuses a bare `claude` or `codex` there by default.
- On Windows, `pitboard doctor` names the Claude Code and Codex builds Pitboard's facts for
  Windows were read from, not the macOS ones.
- On Windows, Pitboard says to restart Codex's background app server from a terminal that is
  not elevated, since Codex refuses to start it from an elevated one.
- On Windows, `pitboard doctor` says a switch changes the login of a Codex in WSL that uses
  the Windows Codex home, where it said every tool in WSL keeps a login of its own.

### Security

- A switch parks Claude Code's `cloudSessionsOauth` and `trustedDeviceEnrolledAfterRefusalAtMs`
  with the outgoing account's login, and no longer leaves them for the incoming account.

## [1.0.0-beta.3] - 2026-10-19

### Changed

- A Claude Desktop switch moves Claude's preview browsers, the Microsoft 365 connector's
  sign-ins and the session windows with the account, since Claude clears them only when it
  sees another account sign in.

### Fixed

- Pitboard adds and switches Claude Desktop accounts again with Claude 2.42304.0, whose data
  folder holds names earlier builds did not make, such as `GPUPersistentCache`.

## [1.0.0-beta.2] - 2026-10-19

### Changed

- Inside WSL, `pitboard doctor` says Pitboard for Windows switches the Claude Code and Codex
  installed on Windows.

### Fixed

- The Windows app finds the command line its installer puts beside it, so **Renew parked logins
  daily** turns on, and **In your terminal** says that `pitboard` updates with the app.
- `pitboard doctor`, **This Mac** and **This PC** leave out the `Claude Code daemon` and
  `what a session authenticates with` checks where Claude Code is absent, as they leave out its
  other checks.
- On Windows, the help of `pitboard warm`, its commands and `pitboard watch` no longer promises
  warm-ups, which are not available there.
- On Windows, the help of `pitboard stow` names Credential Manager instead of the keychain.
- On Linux and Windows, `pitboard repair` says why it left a parked login alone without naming a
  keychain, which holds no parked login there.

## [1.0.0-beta.1] - 2026-10-18

### Added

- The command line runs on Windows 11 24H2 and later, on x64 and ARM64, and switches Claude Code
  and Codex accounts there.
- The Windows app switches Claude Code and Codex accounts from the notification area, and installs
  per user from an MSI for x64 or ARM64.

### Changed

- **Breaking:** `pitboard uninstall` refuses with `pitboard_open` while the Pitboard app is open,
  instead of deleting a directory the app makes again. Quit the app, then run it again.

### Fixed

- `pitboard use` and **Use** put an account's login in place where nothing is signed in to
  Claude Code or Codex yet, as on a new computer after adding the first account, instead of refusing
  with `live_credential_absent`.
- The app's notice that a Claude Code or Codex login was replaced outside Pitboard points at the
  account's **Sign In Again…** button instead of naming a command.
- `pitboard doctor`, **This Mac** and **This PC** warn instead of failing where Claude Code has
  never run, so a new computer with nothing enrolled no longer reads as broken.
- `pitboard doctor`, **This Mac** and **This PC** show Codex's checks once Pitboard finds `codex`,
  before Codex has run for the first time.

## [0.15.1] - 2026-10-23

### Changed

- Pitboard says to open Claude when a Claude Desktop change finds Claude's Reset App Data
  waiting to delete its data folder, instead of asking you to update Pitboard.
- A Claude Desktop switch moves Claude's preview browsers, the Microsoft 365 connector's
  sign-ins and the session windows with the account, since Claude clears them only when it
  sees another account sign in.

### Fixed

- Pitboard adds and switches Claude Desktop accounts with Claude 2.42304.0 and later, Claude 3
  included, and leaves the names it does not move where Claude keeps them, such as
  `GPUPersistentCache`, `SharedStorage` and `design`, instead of refusing each name it has not
  read.
- Pitboard switches Claude Desktop accounts beside a `Claude-3p` folder that Claude does not
  run from, such as one an earlier build left holding only `claude_desktop_config.json`.

### Security

- Pitboard's HTTPS uses rustls-webpki 0.103.16, which fixes RUSTSEC-2026-0382.
- A switch parks Claude Code's `cloudSessionsOauth` and `trustedDeviceEnrolledAfterRefusalAtMs`
  with the outgoing account's login, and no longer leaves them for the incoming account.

## [0.15.0] - 2026-10-17

### Added

- Inside WSL, `pitboard doctor` says that Pitboard switches the Claude Code and Codex installed
  in that Linux, and Codex on Windows too where both use one home on a Windows drive.
- A switch from a Claude Code login whose refresh token Anthropic refuses for good asks to put the
  account's login in its place, with **Replace** in the app and `pitboard use --yes`.

### Changed

- **Breaking:** The public API of `pitboard-core` changes; [its
  documentation](https://docs.rs/pitboard-core/0.15.0/pitboard_core/) shows the new one.
- **Breaking:** Some failures get a code of their own: `live_login_refused` where `pitboard use`
  said `session_expired` for a login Anthropic no longer accepts, `account_on_hold` where it said
  `identity_unverifiable` or `renewal_failed`, and `refused` where `error.cause` and `stale` said
  `server_error`. Branch on the new codes where a script reads the old ones.
- **Add Account** offers every tool Pitboard handles, and says under a tool it cannot find what
  to install, with a link, instead of failing once you click **Sign In**.
- The app looks for Claude Code and Codex each time **Add Account** or **Sign In Again** opens
  and when Pitboard comes to the front, so a tool installed while it runs is found.

### Fixed

- **This Mac** says the times its checks name on your Mac's clock, as **Checked at** does,
  instead of always in 24 hours.
- Pitboard reads and writes Claude Code's `~/.claude.json` where an empty `.config.json` sits
  in Claude Code's config directory, as Claude Code does.
- `pitboard uninstall` and `pitboard doctor` name the Pitboard directory in use, such as one
  `PITBOARD_HOME` names, instead of always `~/.pitboard`.
- `pitboard uninstall` keeps Pitboard's directory while a warm-up runs in it, as `warming_up` in
  `--json`, and names what it could not remove, as `home_kept_by`.
- The app refuses a name with a space or a `/` as you enter it, as the command line does, instead
  of enrolling `my work` or failing on `codex/codex/work`.
- A failure in an account dialog goes once you change the name or the tool.
- Pitboard reads a Codex login again for a moment when it catches Codex rewriting it, instead
  of calling it unreadable.
- `pitboard uninstall` deletes a login a stopped sign-in left in the keychain, instead of
  leaving it there with nothing to name it.
- On Linux, Pitboard finds Claude Code's `.credentials.json` and its locks where
  `CLAUDE_CONFIG_DIR` spells an accent decomposed, as Claude Code does.
- `pitboard doctor` quotes the path in the `rm` it gives for a login file it could not read, so
  the command deletes that file where its path has a space.
- When Anthropic or OpenAI refuses a request, Pitboard says the reason and request id they gave,
  instead of only the status code.
- A refusal that asking again would not change says to sign in again, with `refused` as its
  cause, instead of telling you to check the connection.
- A switch that cannot confirm whose login is parked for the account switched to says so, instead
  of saying it could not tell which account is signed in.
- Pitboard keeps the parked login of an account Anthropic has put on hold, instead of dropping it
  as no longer accepted.
- The automatic switch passes over an account whose parked login Anthropic will not name or renew,
  or whose account is on hold, as it passes over one whose login Anthropic refuses.
- A sign-in Pitboard cannot identify says to sign in again through Pitboard, instead of with
  `claude`.
- Pitboard tells whose a Claude Code login is by asking Anthropic which account and organisation it
  is, as Claude Code does, so a switch between accounts you added works when Anthropic refuses the
  login's profile.
- A Claude Code login Pitboard cannot identify gets its own row saying why, instead of the window
  saying **No accounts** while Claude Code is signed in.
- An account you add while Pitboard cannot identify the login in use says it was parked, not put
  in use, on a first run too.
- **This Mac** names each check in sentence case, starts what a check says with a capital and
  shows commands without backticks.
- The app shows a failure or warning it has no words of its own for as a sentence, with a
  capital, a full stop and no backticks.
- `pitboard doctor` and **This Mac** say to install Claude Code where it is missing, not to run
  `claude`.
- On Linux, `pitboard doctor` notes the Claude Desktop accounts in a Pitboard directory brought
  from a Mac, which nothing there changes, instead of failing its Claude data folder check.

### Security

- Pitboard's HTTPS uses rustls 0.23.46, which fixes RUSTSEC-2026-0346, -0347, -0348 and -0349.

## [0.14.0] - 2026-10-14

### Added

- Pitboard switches [Claude Desktop](https://docs.usepitboard.com/guides/claude-desktop)
  accounts on macOS with `pitboard use desktop/<label>`, while Claude is closed.
- `pitboard enroll desktop/<label>` adds the account Claude is signed in to.
- `pitboard enroll desktop/<label> --sign-in` parks the account Claude is signed in to and adds
  the one you sign in to inside Claude.
- The app adds, switches and signs in again to Claude Desktop accounts, and asks before it quits
  Claude.
- `pitboard status` says when Pitboard last read a Claude Desktop account in use, and when a
  parked one's sign-in may lapse.
- `pitboard doctor` checks Claude Desktop under a heading of its own.
- `pitboard abandon` gives up an interrupted Claude Desktop move.
- `pitboard repair` gives a Claude Desktop account back its parked login.
- `pitboard adopt` sets aside a Claude Desktop login parked on another Mac.
- **Copy Link** in the sign-in dialog copies the sign-in page's address, to paste into another
  browser or browser profile.

### Changed

- **Breaking:** The public API of `pitboard-core` changes; [its
  documentation](https://docs.rs/pitboard-core/0.14.0/pitboard_core/) shows the new one.
- The account index is schema 8, which earlier versions cannot read, so update the command line
  and the app together.
- `pitboard uninstall` refuses while Claude is signed out and a Claude Desktop login is parked
  or set aside.
- With no account enrolled, `pitboard status` says it found no Claude Code login, not that
  nothing is signed in.
- With no account enrolled, `pitboard status` says how to enrol a Codex account and, on macOS, a
  Claude Desktop one.
- A switch that quits the ChatGPT app waits until the `codex` running inside it has quit too.
- **Open Sign-In Page** in the sign-in dialog is renamed **Open**.
- The `pitboard` cask's caveats link [Upgrade from 0.3.0 or
  earlier](https://docs.usepitboard.com/install/upgrade-from-0-3-0) in place of the commands
  that remove 0.3.0's formula.

### Fixed

- The [status line](https://docs.usepitboard.com/guides/status-line) shows a Team seat's
  five-hour limit alone, where it printed `?` for a weekly limit the seat does not have.
- `pitboard uninstall`'s question says the other accounts need signing in again, not a browser
  sign-in.
- A refused change says the interrupted switch it finished or undid before the refusal, not
  after it.
- While `CLAUDE_CODE_CUSTOM_OAUTH_URL` is set, `pitboard abandon` refuses only where a Claude
  Code switch is interrupted.

## [0.13.0] - 2026-10-13

### Added

- Pitboard can [warm up](https://docs.usepitboard.com/guides/warm-up) a Claude Code account:
  once its five-hour limit resets, Claude Code sends one short message so the next window starts
  at once.
- `pitboard warm on <account>` and **Settings** > **Limits** turn warm-ups on.

### Changed

- **Breaking:** The public API of `pitboard-core` changes; [its
  documentation](https://docs.rs/pitboard-core/0.13.0/pitboard_core/) shows the new one.
- The automatic switch and the offer when an account runs out prefer an account with no weekly
  limit for all models, such as a Team seat.
- **Settings** has a **Limits** pane, with the automatic switch and warm-ups that were under
  **General**.
- **Settings** has a **Renewal** pane, with daily renewal and **Renew Now**.
- The app's icon and menu bar mark are a pit board with its top row lit, replacing three bars
  and a speedometer.

### Fixed

- **Settings** fits on screens as small as 1024 x 768: it opens at the top of the screen, and a
  long pane scrolls.

## [0.12.0] - 2026-10-12

### Added

- Each account shows its plan, such as `Max 20x` or `Pro`, in the app, the menu and `pitboard
  status`, and as `plan` in `--json`.
- An [install script](https://docs.usepitboard.com/install), `curl -fsSL
  https://usepitboard.com/install.sh | sh`, puts `pitboard` in `~/.local/bin` on macOS and Linux
  and edits no shell profile.

## [0.11.0] - 2026-10-10

### Changed

- **Breaking:** `pitboard forget`, `stow` and `uninstall` stop with `confirmation_needed` where
  they cannot ask, such as from a script, unless given `--yes`.
- **Breaking:** `--json` no longer implies `--yes`.
- The account index is schema 7, which earlier versions cannot read, so update the command line
  and the app together.
- When Anthropic no longer accepts the Claude Code login in use, Pitboard marks the account
  `login_refused` and says to sign in again with `claude`.

### Fixed

- A switch, `pitboard stow` and the automatic switch no longer stop on an expired Claude Code
  access token, and renew it as Claude Code would.
- An interrupted `pitboard stow` no longer loses a login it renewed.

## [0.10.0] - 2026-10-10

### Added

- On macOS, `pitboard stow` and **Put Away** put away a Claude Code login left in
  `~/.claude/.credentials.json`, parking it if Pitboard has none.
- Pitboard warns with `config_names_another` when Claude Code's config names another account
  than the one whose login is stored.
- Pitboard warns with `in_use_unconfirmed` when Claude Code's config has named another account
  since Anthropic last identified the stored login.
- `pitboard doctor` has an `in use` check.
- `pitboard use` on the account in use, and **Update Claude Code's Config** in the app, write
  that account into Claude Code's config.
- `pitboard log` records why the automatic switch did not switch, as `auto-stay`, which
  **Activity** shows as **Automatic switch not made**.
- `pitboard watch --json` has the event `waiting`, and more fields on `idle`, `no_room` and
  `skipped`.

### Changed

- **Breaking:** The public API of `pitboard-core` changes; [its
  documentation](https://docs.rs/pitboard-core/0.10.0/pitboard_core/) shows the new one.
- **Breaking:** `pitboard watch --json` reports `switch_interrupted` and `custom_oauth_endpoint`
  with the new event `not_watching`, not with `skipped`.
- The account index is schema 6, which earlier versions cannot read, so update the command line
  and the app together.
- While `~/.claude/.credentials.json` is behind the keychain on macOS, every read warns with
  `fallback_login`.
- While `~/.claude/.credentials.json` is behind the keychain on macOS, a switch says running
  sessions keep their account until they renew their login or restart.
- The app and `pitboard watch` say once why Pitboard is not switching.
- The app decides whether to switch every 30 seconds, as `pitboard watch` does.
- **Switch when a limit reaches** shows which account the automatic switch watches.
- After a failed switch, the automatic switch waits a minute, doubling up to 15 minutes, before
  it tries again.
- The status line labels a session's usage `work?` or `?` while it cannot be sure whose it is.

### Removed

- **Breaking:** `pitboard status --json` no longer gives `"source": "claude_code_cache"`, since
  Pitboard no longer reads Claude Code's usage cache.
- **Breaking:** `pitboard doctor` no longer has the `usage_cache` check.
- Pitboard no longer has the error code `switch_overtaken`, which it never printed.

### Fixed

- A sign-in outside Pitboard that replaces the login of the account in use shows as `login
  replaced outside Pitboard`, not `nothing parked`.
- An account just switched to no longer shows the previous account's usage.
- Numbers filed under the wrong Claude Code account go at that account's next answer from
  Anthropic.
- The status line files a session's usage under the account the numbers belong to.
- The app no longer offers an account whose reading may leave out a limit the account in use
  has.
- The automatic switch and the app's offer can choose an account with fewer limits, such as a
  Team seat.
- The automatic switch switches at the share the menu shows, so 95% shown counts as 95%.
- Every front end rounds a half up and changes colour at 70% and 90%.
- Pitboard takes the account in use from what Anthropic last said of the stored login, not from
  Claude Code's config.
- The automatic switch identifies the stored login before it decides, and when it cannot tell,
  it decides nothing and says so.
- The automatic switch judges every limit of the account in use.
- The automatic switch says and records each reason it does not switch once.
- A Claude Code switch no longer leaves the account switched from with a refused login when a
  session renews it at the same moment.
- The app and `pitboard watch` see a change made in the same second as the last one, such as a
  `pitboard use` in a terminal.
- The app and `pitboard watch` no longer report a switch that another `pitboard` is making as
  interrupted.
- A login too large to pass without the argument line, under `PITBOARD_NO_ARGV`, counts as one
  of the automatic switch's three attempts.

## [0.9.0] - 2026-10-08

### Added

- On macOS, `pitboard doctor` and each Claude Code change warn with `fallback_login` when
  `~/.claude/.credentials.json` holds a login besides the keychain's, which [Claude Code over
  SSH](https://docs.usepitboard.com/troubleshooting#claude-code-over-ssh-uses-another-account)
  uses.
- `pitboard status`, the app and, as `▲` or `▼`, the status line compare each limit's use with
  an even [pace](https://docs.usepitboard.com/concepts/usage#pace) to its reset, such as `27%
  over pace`.
- `--json` gives each limit's `pace`.
- `pitboard status --json` gives each account's `organization_uuid`, the Claude organisation its
  login belongs to.

### Changed

- **Breaking:** A keychain locked where it cannot ask for its password stops a command with
  `credential_store_locked`, not `credential_store_unreadable`. A script that checks for the old
  code should check for both.
- The account index is schema 5, which earlier versions cannot read, so update the command line
  and the app together.
- How long the account in use lasts follows its limits' paces, as in `weekly limit runs out in
  11h 05m at this pace`.
- Accounts not in use no longer say how long they last.

### Removed

- Pitboard no longer keeps `readings/` in its directory, and deletes the files it wrote there.

### Fixed

- `pitboard doctor` and **This Mac** report a locked keychain once, as a failed `credential`
  check, not as parked logins that need a new sign-in.
- While the login in use cannot be read, the account Pitboard last switched to shows `login
  could not be read`, not **Needs signing in again**.
- Pitboard enrols one person's Claude accounts in two organisations, such as a claude.ai plan
  and a Team plan, as two accounts, where it refused the second.

## [0.8.0] - 2026-10-08

### Added

- Pitboard can [switch Claude Code
  automatically](https://docs.usepitboard.com/guides/automatic-switching) once any limit of the
  account in use reaches a share you choose.
- `pitboard watch` switches Claude Code automatically while it runs in a terminal, whatever the
  app's setting.
- `pitboard watch` takes `--at` for the share it switches at, `--once` to decide once and exit,
  and `--json` for one event per line.
- **Settings** > **General** has **Switch Claude Code automatically**, off by default, with the
  share it switches at; the app switches only while open.
- The app posts a notification for each automatic switch and for what stops one.
- `pitboard doctor` and **This Mac** have a `network` check that names the proxy Pitboard's
  requests go through, and warns about a SOCKS proxy or a proxy variable it cannot read.
- The daily renewal schedule goes through the proxy of the Pitboard that installed it.
- `pitboard doctor`'s `schedule_proxy` check warns when the schedule's proxy differs from yours.

### Changed

- **Breaking:** The public API of `pitboard-core` changes; [its
  documentation](https://docs.rs/pitboard-core/0.8.0/pitboard_core/) shows the new one.
- **Breaking:** On macOS and Linux, Pitboard [changes nothing as
  root](https://docs.usepitboard.com/troubleshooting/refused-changes#pitboard-changes-nothing-as-root)
  or with `sudo` and stops with `elevated`, though `pitboard status` still answers as with
  `--offline`. Run it as yourself.
- **Breaking:** Pitboard refuses SOCKS proxies: while `ALL_PROXY`, `HTTPS_PROXY` or `HTTP_PROXY`
  names one, requests fail as `unreachable` unless `NO_PROXY` names their host.
- **Breaking:** `pitboard schedule install` and **Renew parked logins daily** refuse with
  `schedule_not_default_home` while `PITBOARD_HOME` names a directory other than `~/.pitboard`.
  Unset `PITBOARD_HOME` to turn it on.
- On Windows, `pitboard` builds and refuses every command but `--version`, `--help`,
  `completions` and `manpage` with `windows_not_released`.
- The product is called Pitboard, with a capital P, and the `pitboard` command and every key it
  reads or prints stay the same.
- `pitboard status` names a limit that is not a whole number of hours by its minutes or seconds,
  such as `90m`.
- The app writes spans as `pitboard status` does, such as **resets in 2h 05m**.
- The app says **resetting now** once a reset is due.
- Under a minute, `pitboard status` says `about to run out` or `resets any moment`, as the app
  does.
- `pitboard doctor` sums up in the same words as **This Mac**, and `pitboard renew` as **Renew
  Now**.
- The command line runs the `claude` or `codex` that `PITBOARD_CLAUDE` or `PITBOARD_CODEX`
  names, as the app does.
- `pitboard doctor` reports the build of the `claude` or `codex` that `PITBOARD_CLAUDE` or
  `PITBOARD_CODEX` names.
- An empty `PITBOARD_CLAUDE` or `PITBOARD_CODEX` names nothing.
- Pitboard reads a few rare shared links differently, such as `ｃ%EF%BD%8Caude.ai`, which it now
  takes for claude.ai.
- The app's preferences, such as the tools you said **Not Now** to, follow `PITBOARD_HOME`.
- The app posts that the account in use has run out once per reset, even across relaunches.
- The app updates text that depends only on the time, such as **used up until 14:05**, once a
  minute.
- The app's settings update what they offer each time they are shown.
- The **Activity** list names a switch **Switch**, where it said **Use**.
- When the app cannot read its record of the account windows, it deletes no window's data and
  never writes over the record.
- `--json` output is ASCII only, with other characters as `\u` escapes that a JSON parser reads
  as before.
- Pitboard's own Codex sign-ins keep the new login in `auth.json`, even when a Codex config
  chooses the keychain.
- `pitboard doctor` and **This Mac** say which setting chose Codex's login store, such as `file
  (Codex's default)`.

### Fixed

- **Breaking:** With `HOME`, `PITBOARD_HOME` or `CLAUDE_CONFIG_DIR` empty or relative, or
  `CODEX_HOME` relative, Pitboard refuses every command but `completions`, `manpage` and
  `doctor` with `home_not_absolute`. Set a full path or unset the variable.
- When the account in use runs out, the app offers only an account with room in every limit.
- Where Pitboard cannot list running processes, a Codex switch, or a sign-in that puts a new
  login in use, warns with `sessions_unknown`.
- Pitboard reads Codex's login store from every config layer Codex reads, such as `/etc/codex`
  and managed preferences.
- Codex changes stop with `live_store_unsupported` where a Codex config layer chooses a store
  other than the file.
- Codex changes stop with `live_store_unsupported` where a Codex config layer cannot be read,
  names an unknown store or has an unmatched `profile`.
- Pitboard no longer refuses a Codex account where `config.toml` chooses another store but a
  managed setting or a requirement chooses the file.
- An account's window opens again on the page before a sign-in page, or on the site's home, not
  on the sign-in page.
- After Claude Code refuses a pasted code, the sign-in sheet asks for the code again.
- **Sign In** pressed right after **Cancel** no longer says another sign-in is already waiting.
- The app reads again right after you add, rename or forget an account, give up on a switch or
  press **Renew Now**.
- The window says when a sign-in fails, or a name cannot be saved, after its sheet has closed.
- Quitting Pitboard stops a sign-in under way, and the tool's own sign-in with it.
- The window shows two warnings with the same words as two notices.
- **This Mac** gives each account's parked login check its own row.
- **This Mac** says it is checking until the last check is in, not **Checked at** beside checks
  still running.
- After an interrupted switch Pitboard cannot finish, the app shows **An interrupted switch is
  waiting**, with **Give Up**.
- After an interrupted switch Pitboard cannot finish, `pitboard status` warns with
  `recovery_undetermined`.
- `pitboard abandon` and **Give Up** give up on an interrupted Codex switch while
  `CLAUDE_CODE_CUSTOM_OAUTH_URL` is set.
- `pitboard doctor` and the app's checks no longer warn that a parked login could not be read
  after a switch, an enrolment or a renewal.
- `pitboard doctor`'s advice about a login others can read is one sentence again.
- The note under the sign-in sheet's **Code** field says Claude Code takes the code shown after
  you sign in.
- The sign-in sheet links the right address when Claude Code prints it as a terminal hyperlink.
- With `HOME` unset, the command line uses your account's home directory, as the app does.
- The daily renewal schedule renews `~/.pitboard`, even when `PITBOARD_HOME` reaches it through
  `launchctl setenv` or systemd's user environment.
- On Linux, the daily renewal timer runs `pitboard renew --scheduled` once `pitboard schedule
  install` rewrites it.
- The command line and the app take a `claude` or `codex` only when it is a regular file you may
  run.
- The app honours the environment it was started with, such as `ANTHROPIC_API_KEY`, though
  opened from Finder it sees only what `launchctl setenv` sets.
- The **Open Link** window and the Share sheet name claude.ai and chatgpt.com in English,
  whatever the Mac's language.

### Security

- **Breaking:** Pitboard refuses any folder in `~/Library/CloudStorage` or `~/Library/Mobile
  Documents` as its directory, with `state_on_synced_drive`.
- **Breaking:** Pitboard refuses a `~/.pitboard` with a sync client's folder name, such as
  `Dropbox` or `Sync`, anywhere in its path, in any case. Set `PITBOARD_HOME` to a folder
  outside it.
- Pitboard writes the daily renewal schedule's files so only you can read them, since they can
  hold a proxy's password.
- Pitboard refuses a site's sign-in link, or a link with a `.` or `..` path segment, however the
  rest of its path is written.
- Pitboard refuses a link that names a user or a port, and a `pitboard://` link with a user, a
  port or a path.

## [0.7.0] - 2026-10-04

### Added

- The app gives each account a [window on its tool's
  site](https://docs.usepitboard.com/guides/account-windows), claude.ai or chatgpt.com, so every
  account stays signed in side by side.
- An account's window has Back, Forward, Reload, zoom and **Find**, with Safari's shortcuts.
- An account's window opens at the last page it showed.
- When an account's window first opens, a note says how to sign in, since Google's sign-in does
  not work inside apps.
- An account's window opens other web pages in your default browser.
- An account's window saves downloads to your Downloads folder without replacing a file.
- An account's window asks before a download that an artifact or another site starts.
- The **Downloads** button lists an account window's downloads.
- **File** > **Remove Website Data** signs an account's window out on this Mac only.
- **Pitboard** in a browser's Share menu opens a claude.ai or chatgpt.com page as the account
  you pick.
- While any of its windows is open, Pitboard has a Dock icon, is in Command-Tab and shows its
  menus in the menu bar.

### Changed

- **View** > **Refresh** (Command-R) works in every window, and reloads the page in an account's
  window.
- Forgetting an account also deletes everything its window keeps on this Mac, its sign-in
  included.
- `brew uninstall --zap --cask pitboard-app` also removes the account windows' data.

## [0.6.0] - 2026-10-01

### Added

- Switching Codex in the app while the ChatGPT app is open offers **Quit ChatGPT and Switch**,
  which quits ChatGPT, switches and opens it again; the command line never quits an app.

### Changed

- **Breaking:** The public API of `pitboard-core` changes; [its
  documentation](https://docs.rs/pitboard-core/0.6.0/pitboard_core/) shows the new one.
- After a Codex switch, Pitboard names each program still running the old login and how to make
  it take the new one.
- `pitboard doctor`'s `codex_running` check lists the programs still running the old Codex
  login.

### Fixed

- A switch counts only your own `codex` processes, not every user's.

## [0.5.2] - 2026-09-29

### Fixed

- After a banked reset on claude.ai, Pitboard shows the account's lower weekly share at its next
  read, not 100% until the old reset.
- Pitboard shows the right weekly share after a plan upgrade mid-window.

## [0.5.1] - 2026-09-29

### Fixed

- `pitboard status --fresh` and the app's **Refresh** no longer ask again during a wait the
  service asked for with `Retry-After`.
- An account whose service never answered and then could not be reached no longer shows as rate
  limited.
- With `PITBOARD_NO_ARGV=1`, renewing a parked login too large for `security`'s standard input
  no longer loses it.
- A switch whose login could not be read back names the `pitboard use` that finishes or undoes
  it.
- With Codex's `auth.json` missing, `pitboard doctor` says to run `codex login`.
- The man page covers every command on its one page, instead of naming pages that were never
  installed.
- The app says Claude Code is not installed when it finds neither `claude` nor `codex`, and no
  login or enrolled account.

### Security

- The app, and a renewal schedule installed while `PITBOARD_NO_ARGV` is set, honour it, so a
  large login never reaches `security` as an argument another process running as you could read.

## [0.5.0] - 2026-09-28

### Added

- The window's account list can rename an account.
- Double-click or Return in the window's account list switches to that account.
- Delete in the window's account list forgets the account.
- Each account's menu in the window has **Copy Email Address** and **Sign In Again**.
- The menu has **About Pitboard**.
- **Settings** has a **Command Line** tab.
- **Menu bar shows** in **Settings** picks the account and usage, the usage alone or the icon
  alone.

### Changed

- The menu bar item opens a menu, not a panel, with each account under its tool, and choosing
  one switches to it.
- Adding an account, signing in again and renaming are sheets over Pitboard's window.
- A sheet stays open while your browser is in front.
- A failed switch, rename or forget says why in an alert, and a failed sign-in says so in its
  sheet.
- **Open Pitboard at login** says when macOS waits for your approval in Login Items.

### Fixed

- The menu shows when it last read, not "updated just now".
- The window's reset times stay current while it is open.
- Giving up on an interrupted switch shows as a note, not as a failure.
- A failure to change daily renewal or **Open Pitboard at login** shows in Settings.
- Switching one tool's account no longer hides advice about another tool's account that ran out.
- Advice to switch no longer offers an account that was forgotten, expired or ran out since.
- A read in flight when you switch no longer lands after the switch and shows the account before
  it.
- The window says why any account's numbers are not new, the one in use included.
- The menu and the window describe each account the same way.

## [0.4.1] - 2026-09-26

### Changed

- Pitboard keeps one usage reading per account that only moves forward, so a session's older
  numbers no longer replace newer ones.
- The app's **Documentation** item opens docs.usepitboard.com.

### Fixed

- Every session on one account, `pitboard status` and the menu bar show the same, newest
  numbers, and idle sessions follow with `"refreshInterval": 10` in the status line settings.
- A session still holding the previous account's numbers after a switch or a `/login` no longer
  records them as the new account's.
- The advice to switch stays for as long as the account in use is out, not only until the app's
  next read.

## [0.4.0] - 2026-09-25

### Added

- **Settings** > **Advanced** shows which `pitboard` a terminal runs.
- **Install command line tool…** in **Settings** > **Advanced** links `/usr/local/bin/pitboard`
  to the app's own, and never replaces a `pitboard` someone else installed.
- An account whose parked login has expired has a **Sign in again** button in its row.

### Changed

- **Breaking:** In the tap, `pitboard` is the command line's cask and the app's cask is
  `pitboard-app`. [Upgrade from 0.3.0 or
  earlier](https://docs.usepitboard.com/install/upgrade-from-0-3-0) has the commands to move.
- **Breaking:** The public API of `pitboard-core` changes; [its
  documentation](https://docs.rs/pitboard-core/0.4.0/pitboard_core/) shows the new one.
- The app carries the command line inside it, so an update moves both.
- `brew install datlechin/tap/pitboard` installs the release's own command line as a cask, on
  macOS and Linux, without compiling.
- From the old formula, `brew update` leaves Pitboard at 0.3.0, so uninstall the formula before
  you install the cask.
- From the old app cask, `brew update` replaces the app with the command line once, and keeps
  your settings and `~/.pitboard`.
- Removing the old app cask with `--zap` moves `~/.pitboard` to the Trash, so leave `--zap` out.

### Removed

- Releases no longer publish a source tarball; the source is on crates.io and in the tag.

### Fixed

- Daily renewal turned on from the app runs the command line inside the app, instead of starting
  a second copy of the app every day.
- Opening the app points a 0.3.0 renewal schedule at the command line inside it.
- The app no longer turns daily renewal on from the temporary copy macOS runs before the app is
  in Applications.
- `pitboard doctor` and **Check this machine** fail when the renewal schedule runs a `pitboard`
  that is gone or is an app.
- On Linux, a renewal schedule turned on from the command line keeps working after `brew
  upgrade`.
- `pitboard uninstall` removes the renewal schedule first, as `schedule_removed` in `--json`.
- Both casks remove the renewal schedule only on `brew uninstall --zap`, and neither touches
  `~/.pitboard`, so run `pitboard uninstall` before removing Pitboard.
- Advice about upgrading or removing Pitboard no longer assumes Homebrew.
- On Homebrew 6 and later, the app's cask installs without the formula installed first.
- `cargo binstall pitboard` no longer falls back to a third party's build when it cannot fetch
  the release's.

## [0.3.0] - 2026-09-24

### Added

- Pitboard switches Codex accounts too, with `pitboard enroll codex/<label>` and `pitboard use
  codex/<label>`.
- `pitboard enroll codex/<label> --sign-in` runs `codex login` in a private `CODEX_HOME`.
- `pitboard` shows each Codex account's five-hour and weekly limits.
- Pitboard handles Codex's `auth.json` store, and refuses Codex's keychain stores with a reason.
- Until a Codex account is enrolled, Pitboard reads nothing of Codex's and asks OpenAI nothing.
- A Codex switch says that running `codex` sessions keep the old account until restarted, and
  how many it found.
- A Codex switch warns that signing out in a running `codex` would revoke the login just parked.
- A Codex switch writes nothing over a login that a running `codex` renewed meanwhile.
- A Codex switch refuses to park a login that names two accounts.
- The app's panel groups accounts by tool.
- The menu bar follows the signed-in account closest to running out, of either tool.
- When an account runs out, the app offers only an account of the same tool.
- **Add another account** asks which tool when both are installed.
- A Codex sign-in in the app shows the address `codex login` printed.
- Labels belong to a tool: `work` can name a Claude Code and a Codex account, and `codex/work`
  says which.
- A bare label means Claude Code for a new account, and is refused as `label_ambiguous` where it
  matches both tools.
- `pitboard adopt` takes over a `~/.pitboard` written on another computer, keeping each
  account's label, email and usage and dropping every parked login.
- `pitboard repair` finds every parked login in the keychain or vault, and gives it back to its
  account, deletes it or reports it.
- `pitboard repair` deletes only logins this Pitboard recorded.
- On macOS, a login `pitboard repair` gave back that this Pitboard never recorded is deleted
  only once Pitboard has used it.
- `pitboard uninstall` says how many parked logins it left that this Pitboard never recorded, as
  `parks_left` in `--json`.
- A failed request to Anthropic carries a cause beside its code, such as `unreachable`,
  `rate_limited` or `login_refused`, in `--json` and the app.
- A failed request to Anthropic says whether asking again can help, and `pitboard status` tells
  the causes apart.
- The app has a window with each account's usage history, the full change log and what `doctor`
  found.
- The app has a **Settings** window, opened with Command-comma, that can turn on scheduled
  renewal.
- On a first run, the app says what state the machine is in and offers the next step.
- The app's first launch opens its window.
- The app can give up on an interrupted switch that recovery cannot finish.
- `pitboard doctor` fails when anyone but the owner can read a stored login.
- `pitboard doctor` lists what fills a login too large to store, largest first.
- `pitboard doctor` says when an account has gone unused longer than its refresh token lasts.
- `pitboard doctor` reports Claude Code's supervisor daemon, which also writes the login.
- `pitboard doctor` shows the installed Claude Code version, beside the version Pitboard's facts
  about it were checked against.
- `pitboard doctor` has `environment.codex`, and Codex checks whose codes start `codex_`.
- `pitboard doctor` fails or warns about a Codex keychain store or API key login only where it
  stands in the way of enrolled Codex accounts.
- `pitboard doctor` says what Pitboard holds back from asking Anthropic.
- Pitboard keeps two weeks of each account's usage, and says how long the account lasts at its
  recent rate.
- `pitboard status` names the credential slot it reports on, which `CLAUDE_CONFIG_DIR` selects.
- `pitboard renew` renews every parked login that is due.
- `pitboard status --fresh` and the app's **Refresh** ask at once, but never during a wait the
  service asked for with `Retry-After`.
- `pitboard schedule install` runs `pitboard renew` on a schedule, through a LaunchAgent on
  macOS or a systemd user timer on Linux.
- Pitboard warns when Claude Code's settings set an `env` block, an `apiKeyHelper` or a
  third-party provider that makes sessions ignore the login it switches.
- A custom OAuth endpoint set in Claude Code's settings refuses a switch.
- `PITBOARD_NO_ARGV=1` refuses a login too large for `security`'s standard input, instead of
  passing it as an argument.
- What Pitboard removes from Claude Code's config goes into `pitboard log`.
- In `--json`, `use`, enrolments and renewals give `provider`.
- `pitboard use --json` gives `adoption`, which says whether running sessions follow on their
  own or need restarting.
- `pitboard status --json` gives each account `provider` and `qualified`, such as `codex/work`.
- `pitboard status --json` gives `login_unreadable` for a login Pitboard cannot read, and
  `login_unusable` for one it cannot switch, such as an API key.
- Each limit in `--json` gives `length_seconds`.
- Each release artefact comes with a CycloneDX bill of materials.
- The bills of materials, `SHA256SUMS` and `appcast.xml` are attested like the artefacts.

### Changed

- **Breaking:** A Claude Code credential with no account, as `/logout` leaves, makes `use` and
  `enroll` exit 1 with `live_credential_absent`, not 3 with `live_credential_shape_unexpected`.
- **Breaking:** The public API of `pitboard-core` changes; [its
  documentation](https://docs.rs/pitboard-core/0.3.0/pitboard_core/) shows the new one.
- The account index is schema 4, which earlier versions cannot read; they say which of the
  command line and the app to update.
- Pitboard brings a schema 3 account index forward on first read.
- `pitboard doctor` warns, instead of failing, when Claude Code's credential holds no account.
- Messages name the tool, service and sign-in command of the account they are about.
- Messages qualify a label to type, such as `claude/work`, where a bare one would be ambiguous.
- `CLAUDE_CODE_CUSTOM_OAUTH_URL` refuses changes to Claude Code accounts only, not to Codex
  ones.
- `pitboard doctor` warns about the storage v5 flag only when it is on and the login is in the
  fallback store.
- Pitboard asks Anthropic about an account again only once its tightest limit could have moved
  by a percentage point.
- Every front end waits out a refusal for as long as `Retry-After` says.
- An interrupted switch is recovered without a network in most cases.
- The app shows the last measured numbers when Anthropic cannot be reached, not an empty panel.
- Text in the app grows with the system text size without clipping.
- VoiceOver reads each account row as one item, and says whether each check passed.
- A login too large for `security`'s standard input is passed as an argument, as Claude Code
  does, and the switch says so.
- Homebrew serves each release as it is published, not up to a day later.

### Fixed

- The app follows a switch made elsewhere on the machine, such as in a terminal, within seconds,
  not up to five minutes later.
- A run killed while parking a login no longer leaves a live login that nothing names, and
  `pitboard doctor` reports any left.
- A parked Claude Code login keeps the account's other credential keys, such as its device
  token, and a switch back restores them.
- Cancelling a sign-in no longer holds the app.
- The app shows every warning, not only the first.
- A failed read in the app shows its own warnings, not those of the last read that worked.
- VoiceOver no longer calls the status item "speedometer".
- The app finds `claude` and `codex` installed through a Node version manager such as nvm or
  volta, or a custom npm prefix.
- `pitboard enroll <label> --sign-in` for the account in use puts the new login in use, where it
  parked it.
- When Pitboard cannot confirm a new sign-in is in use, it parks the login rather than lose it,
  and says why.
- A renewal interrupted after writing the fresh login no longer loses that login on the next
  run.
- Recovering an interrupted switch from another `CODEX_HOME` or `CLAUDE_CONFIG_DIR` stops with
  `recovery_elsewhere`.
- A command refused over a mistyped name still finishes or undoes an interrupted switch, and
  reports it.
- Labels written by 0.1.x that contain a slash can be switched to, forgotten, renamed and signed
  in to again.
- Pitboard no longer asks about an account as often as once a minute over limit kinds it did not
  know, such as `session`.
- Changing Claude Code's config no longer loses what Claude Code wrote to it meanwhile, such as
  project history.
- A renewed login's expiry follows Anthropic's clock, not this machine's.
- When Claude Code's config names a signed-in account whose login no store Pitboard reads holds,
  Pitboard refuses with `live_credential_elsewhere`.
- A switch checks the incoming login with Anthropic first, and drops a refused login and stops.
- A switch refuses an incoming login filed under the wrong account.
- A switch renews an incoming login's expired access token.
- A switch undone by a Claude Code `/logout` at the same moment fails with
  `switch_did_not_hold`, not with success.
- A switch notices when the machine slept long enough for Claude Code to take back the
  credential lock, and stops.
- A locked keychain no longer reads as a lost login: the switch keeps every copy and fails with
  `switch_unverified`.

### Security

- `pitboard doctor --json` no longer prints the signed-in email, organisation uuid, login name,
  a value derived from the refresh token or home paths with the username.

## [0.2.0] - 2026-09-22

### Added

- `pitboard log` shows what Pitboard has changed and when, and which front end asked.
- `pitboard uninstall` deletes every parked login, then Pitboard's own files.
- `pitboard abandon` gives up on an interrupted switch that cannot be finished, keeping every
  login.
- `pitboard status --offline` answers from the last measured numbers, without asking Anthropic
  or touching a login.
- Each usage row in `--json` carries `severity`, Anthropic's own grade for that limit.
- `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN` and `CLAUDE_CODE_OAUTH_TOKEN` raise a warning on
  every change, since Claude Code signs in with them instead.
- The app shows each account's email, and when each limit resets.
- The app shows when a parked login stops working.
- The app runs `doctor`'s checks on demand.
- The app shows its own version, and a mark in the menu bar.
- The app has VoiceOver labels.
- The app can record the account in use.
- The app can drop an account.
- The app can add an account through Claude Code's sign-in, showing what that sign-in says.
- `cargo binstall pitboard` fetches the built binary instead of compiling.
- The macOS command line binaries are signed and notarised like the app.
- Each release carries build provenance.

### Changed

- `pitboard forget` asks before deleting a parked login, unless given `--yes` or `--json`.
- The minimum supported Rust version is 1.91.

### Fixed

- A state file from a newer version makes Pitboard say whether to update the command line or the
  app, not offer to delete the accounts.
- `pitboard statusline` typed at a prompt no longer waits for input.
- Offline, the account in use no longer shows as one with nothing parked.
- A switch that failed before installing anything no longer makes the next command announce a
  recovery.
- `enroll --sign-in` checks what could refuse the enrolment before the browser sign-in, not
  after.
- The status line shows numbers for every account, not question marks for all but the account in
  use.
- `doctor` and `forget` go by who is signed in, not by Pitboard's record of its last switch.
- Three ways a parked login could be left in the keychain with nothing naming it are closed.
- A renewal that could not be written no longer leaves the account with a login already spent.
- A login past the keychain's size limit gets its own error, giving the size, the limit and what
  to do.
- Columns line up in a terminal when a label is in Chinese or Japanese.
- One state file serves every credential slot, and an account switched to in one slot is no
  longer claimed in another.
- A limit scoped to one model no longer makes the menu bar read as though everything had
  stopped.
- Building on Windows stops with one sentence instead of a screen of type errors.

## [0.1.4] - 2026-09-22

### Fixed

- Renewing a parked login whose answer has no refresh-token lifetime keeps the deadline it had,
  as Claude Code does.
- Pitboard refuses to act while `CLAUDE_CODE_CUSTOM_OAUTH_URL` is set, where a switch would
  restore into an item nobody reads.

### Security

- A switch no longer leaves the outgoing account's `trustedDeviceToken`, `organizationUuid`,
  `enterpriseGateway` or `designOauth` for the incoming account to present as its own.

## [0.1.3] - 2026-09-22

### Added

- The menu bar panel says when an account has run out and which has the most left, whether or
  not notifications are allowed.
- After a switch, the panel counts down until sessions already open follow.
- A Homebrew tap installs the command line with `brew install datlechin/tap/pitboard`, and the
  app with `--cask`.

### Changed

- The app asks for permission to post notifications only when it has something to say.

### Fixed

- Pitboard run without `USER` set, as from launchd, cron or an app opened from Finder, reads the
  same keychain item as Claude Code.
- A parked login issued to another client is renewed as that client, not as the first-party one.
- The app's build number counts up with its version, where 0.1.2 shipped a number Sparkle would
  read as newer than the next release.

## [0.1.2] - 2026-09-22

### Added

- A macOS menu bar app shows the account in use and its tightest limit in the menu bar, and
  every account's limits in its panel.
- The app switches accounts in one click.
- The app posts a notification when an account runs out.
- The app can open at login.
- `status` renews a parked login whose access token has expired, so parked logins no longer
  lapse.
- `status` drops a login Anthropic refuses, and names the command that signs in to it again.

## [0.1.1] - 2026-09-21

### Fixed

- `enroll --sign-in` no longer holds Pitboard's lock while the browser sign-in waits, so `use`,
  `forget` and `rename` work meanwhile.
- A second `enroll --sign-in` is refused while one waits.
- `enroll --sign-in --json` prints exactly one JSON line, with Claude Code's sign-in output on
  stderr.

## [0.1.0] - 2026-09-21

### Added

- Pitboard has the commands `enroll`, `use`, `forget`, `status`, `doctor` and `statusline`.
- `status` shows live usage for every enrolled account, which accounts can be switched to, and
  until when.
- `enroll <label> --sign-in` on an enrolled label renews its parked login.
- `rename` changes an account's label.
- A switch refuses a parked login that has expired.
- `doctor` checks every parked login and reports an interrupted switch.
- Every command takes `--json`, with a versioned envelope and stable error codes, even for a
  mistyped command line.
- Pitboard has shell completions and a man page.
- Pitboard runs on Linux, with parked logins in a file vault, not yet tried against a signed-in
  Claude Code there.
- Pitboard keeps an audit log of every change it makes.
- The account index is schema 3, with one parked login per account and its expiry.

[Unreleased]: https://github.com/datlechin/pitboard/compare/v1.0.0-beta.5...HEAD
[1.0.0-beta.5]: https://github.com/datlechin/pitboard/compare/v1.0.0-beta.4...v1.0.0-beta.5
[1.0.0-beta.4]: https://github.com/datlechin/pitboard/compare/v1.0.0-beta.3...v1.0.0-beta.4
[1.0.0-beta.3]: https://github.com/datlechin/pitboard/compare/v1.0.0-beta.2...v1.0.0-beta.3
[1.0.0-beta.2]: https://github.com/datlechin/pitboard/compare/v1.0.0-beta.1...v1.0.0-beta.2
[1.0.0-beta.1]: https://github.com/datlechin/pitboard/compare/v0.15.0...v1.0.0-beta.1
[0.15.1]: https://github.com/datlechin/pitboard/compare/v0.15.0...v0.15.1
[0.15.0]: https://github.com/datlechin/pitboard/compare/v0.14.0...v0.15.0
[0.14.0]: https://github.com/datlechin/pitboard/compare/v0.13.0...v0.14.0
[0.13.0]: https://github.com/datlechin/pitboard/compare/v0.12.0...v0.13.0
[0.12.0]: https://github.com/datlechin/pitboard/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/datlechin/pitboard/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/datlechin/pitboard/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/datlechin/pitboard/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/datlechin/pitboard/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/datlechin/pitboard/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/datlechin/pitboard/compare/v0.5.2...v0.6.0
[0.5.2]: https://github.com/datlechin/pitboard/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/datlechin/pitboard/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/datlechin/pitboard/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/datlechin/pitboard/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/datlechin/pitboard/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/datlechin/pitboard/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/datlechin/pitboard/compare/v0.1.4...v0.2.0
[0.1.4]: https://github.com/datlechin/pitboard/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/datlechin/pitboard/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/datlechin/pitboard/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/datlechin/pitboard/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/datlechin/pitboard/releases/tag/v0.1.0
