# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Pitboard switches Claude Desktop, Anthropic's app for macOS, between your own accounts, and
the menu bar app quits Claude around a switch and opens it again.

### Added

- `pitboard enroll desktop/<label>` enrols the account Claude Desktop is signed in to, and
  `pitboard use desktop/<label>` switches it. Claude Desktop keeps its login in its data
  folder, so a switch moves the cookie jar, the storage folders that belong to the account
  and three keys of `config.json` into a park under `~/.pitboard/desktop/parks`, by rename
  and never by copy, and leaves its caches and settings where they are. Experiments on 4
  October 2026 found those three, with the files, enough to move each of two accounts
  whole. Nothing moves while
  Claude is open: quit it with Command-Q first, and the command line never quits it for you.
  Switching never reads your keychain.
  [Use Pitboard with Claude Desktop](https://docs.usepitboard.com/guides/claude-desktop)
  has the steps.
- The menu bar app lists Claude Desktop accounts when it finds `Claude.app`, with the date
  each parked one lapses and where its usage came from. A switch while Claude is open asks
  first, then the app quits Claude, switches and opens it again for you. **Add Account**
  walks through adding a Claude Desktop account in three steps, quitting and opening Claude
  around each change, and **Put personal Back** undoes it halfway. **Settings** > **General**
  has **Show live usage**, which explains the keychain prompt before anything is read. When
  macOS stops letting Pitboard read Claude's key, the app says live usage is paused, once in
  a notification and in a notice with **Allow Again**.
- A Claude Desktop switch counts Claude as open when it runs from a copy of `Claude.app`
  under another name, and is not held up by a `SingletonLock` whose process now runs another
  program. If Claude is opened while a switch moves its files, the switch stops with
  `app_opened_midway`; any switch that stops partway warns with `switch_unfinished`, and the
  app then leaves Claude closed so the next change can finish it. If Pitboard cannot list
  what is running, it stops with `app_state_unknown`. Account keys Claude wrote to
  `config.json` while signed out are set aside in `~/.pitboard/desktop/strays`, not
  overwritten, all in one folder per switch, and enrolling an account again drops its
  lapsed park, as does switching to the account already in use. A session Pitboard has not
  seen, under an account it knows, stops with `desktop_identity_unconfirmed` until you
  enrol that label again, because whether Claude replaces the account that Log out leaves
  behind has not been measured; so does a session Pitboard knows as another account's.
- `pitboard use desktop --signed-out` parks the account in use and leaves Claude Desktop
  signed out, so you can sign in to another account in Claude and enrol it without signing
  the first one out. Never use Log out in Claude to switch: it ends that session at
  claude.ai, and Pitboard cannot bring it back (measured on 4 October 2026).
- `pitboard status` lists Claude Desktop accounts under their own heading, with usage read
  from Claude's own history, which matched claude.ai's own count when measured, and says
  when each parked login lapses. With nothing enrolled and Claude Desktop installed, it says
  how to enrol a Claude Desktop account as well as a Claude Code one. Messages about a
  Claude Desktop account, which has no email, print no empty brackets.
- `pitboard desktop live-usage enable` asks claude.ai how much each Claude Desktop account
  has left, which needs Claude's key from the keychain. claude.ai does not document the
  request; it and how Pitboard decrypts the session were checked against claude.ai on 4
  October 2026. It is off until you turn it on, macOS asks for your login password once,
  and switching never needs it. After **Always Allow**, refreshes need nobody at the Mac.
  A keychain question nobody answers can stay on screen after Pitboard stops waiting, and
  its message says to close it with **Deny**. A refresh that stopped waiting does not ask
  again, but each `enable` that runs out of time, and the app and a command reading at
  once, can each leave one. Deny and a
  wrong password are reported alike, since macOS answers both the same way. `disable` turns it
  off and forgets the key, and `status` says whether it is on. An answer from the keychain
  Pitboard does not expect pauses it until you allow it again, instead of asking on every
  refresh.
- `pitboard renew` lists each parked Claude Desktop login as not renewable, with the date
  its sign-in lapses, and switches and `status` warn with `park_expires_soon` a week before.
  A Claude Desktop session lasts about four weeks and Pitboard cannot renew it.
- `pitboard doctor` has a Claude Desktop section: the app's version against the one
  Pitboard's facts were read from, the cookie jar's format, the parks and their volume,
  interrupted switches, what is running and whether live usage is on. It never reads the
  keychain.
- The `--json` output gains `desktop` as a tool, `signed_out` on `use`, `next_launch` as an
  adoption, `not_renewable` with `expires_at` on `renew`, a `desktop` object on `status`,
  `environment.desktop` on `doctor`, the `desktop` command, and the error, warning and stale
  codes Claude Desktop needs. [Errors and warnings](https://docs.usepitboard.com/reference/errors)
  lists them.

### Changed

- The product is called Pitboard, with a capital P, everywhere it names itself: the app
  (its menus, About, Login Items and the Share menu entry, **Pitboard** and **Pitboard
  Debug**), the command line's messages and help, and the docs. The command is still
  `pitboard`, and nothing it reads, writes or prints as a key changes. The `home` check of
  `pitboard doctor --json` is now named `Pitboard home`; its code is still `home`.
- `pitboard status` names a limit whose length is not a whole number of hours by its
  minutes or seconds, as the app does, such as `90m` or `45s`. It showed the limit's kind
  instead, such as `90_minute`, a name Pitboard builds from the length of a Codex limit.
  `pitboard status --json` still gives the kind.
- The app says when a limit resets the way `pitboard status` does: **resets in 2h 05m**,
  with the minutes in two digits, and **resetting now** once that time has come. It said
  **in 2h 5m**, and nothing once the time had passed, so a limit whose reset was due showed
  its old figure with nothing beside it. VoiceOver says **resetting now** too.
- The app says how long an account lasts with the spans `pitboard status` uses, such as
  **About 1h 05m left at this rate** or **Resets in 3d 2h**, where it said **About 1h 5m
  left at this rate** or **Resets in 3d 2h 5m**. Under a minute, `pitboard status` says
  `about to run out` or `resets any moment`, as the app does, where it said
  `about <1m left at this rate` or `resets in <1m`.
- `pitboard doctor` and the app's **This Mac** pane sum up the checks in the same words:
  `Everything Pitboard checks is in order.` when every check holds,
  `One thing is worth looking at.` or `2 things are worth looking at.` when checks only
  warn, and `1 broken: do not switch accounts until fixed.` when a check fails. `doctor`
  said `Everything Pitboard relies on holds.` or `2 to look at; nothing is broken.`, and the
  app counted a check that fails as one more thing worth looking at.
- `pitboard renew` and the app's **Renew Now** report a run in the same words:
  `No parked login was due.`, `Renewed one.`, `Renewed all 2.` or
  `Renewed 1 of 2; the rest are tried again next time.` `renew` said `Renewed 1.` or
  `Renewed 2.` when it renewed everything due, and the app said `Nothing was due.` or
  `Renewed 1 of 2.`
- The command line runs the `claude` or `codex` that `PITBOARD_CLAUDE` or `PITBOARD_CODEX`
  names, as the app does, to sign in, and `pitboard doctor` reports that program's build.
  Before, only the app read them, and the command line always looked on its `PATH`. Set
  but empty, either names nothing, and the program is looked for on `PATH` as before.
- In the app, an empty `PITBOARD_CLAUDE` or `PITBOARD_CODEX` names nothing too, and the app
  looks for the program as it does when neither is set. Before, it offered the tool, then
  refused its sign-in because the program was not installed.
- Pitboard checks a link shared with it, or carried in a `pitboard://` link, in its core
  now, which changes three rare answers:
  - A site's host followed by `/`, `?` or `#` and then a combining mark or a joiner, such
    as `chat.com/` and an accent, is a link to that site. Pitboard took the mark and the
    character before it as one, and found no link.
  - A link is too long past 8,192 Unicode scalars, where it was past 8,192 characters as
    they show on screen. A link of letters each followed by a combining accent is too long
    at about half the length it was.
  - A host written partly in full-width letters and partly in escapes, such as
    `ｃ%EF%BD%8Caude.ai`, is claude.ai, as the WHATWG URL standard maps it. Pitboard said
    it was another site.
- A state file that names a Claude Desktop account stops an older Pitboard with
  `state_names_unknown_tool`, which says to update it. The file stays at schema 4.
- An interrupted Claude Desktop switch has its own record,
  `~/.pitboard/desktop/journal.json`, finished only while Claude is closed. It never blocks a
  Claude Code or Codex command, and `pitboard abandon` gives it up after any interrupted
  Claude Code or Codex switch.

### Fixed

- `pitboard doctor`'s advice about a login others on the machine can read is one
  sentence again, without a run of spaces in the middle of it.
- The note under the sign-in sheet's **Code** field says what Claude Code does: it takes
  the code shown after you sign in, whether or not your browser came back to it. The note
  said Claude Code asked for the code only when your browser could not reach it.
- The sign-in sheet links the right address when Claude Code prints it as a terminal
  hyperlink. Claude Code does that when the app runs with `FORCE_HYPERLINK` set, or with
  the variables a terminal such as iTerm2 sets. The link's address picked up a stray
  control character, and sometimes a second copy of the address.
- With `HOME` unset, the command line used the folder it was run from as your home: it kept
  its files in a `.pitboard` there, and looked for Claude Code's and Codex's beside it. It
  now uses your account's home directory, as the app does.
- The command line takes a `claude` or `codex` to be the tool's program only when it is a
  regular file you may run, as the system judges it when it starts one, and otherwise looks
  further on `PATH`. It took a file only others may run, and then failed to start it.
- The app reads the environment it was started with as the command line reads its own, by
  the same code. It read only the variables that move where things are kept, and ignored
  these, which it now reads as the command line does:
  - `CLAUDE_CODE_CUSTOM_OAUTH_URL`: while it is set, the app refuses changes to Claude Code
    accounts.
  - The variables that make Claude Code sign in another way, such as `ANTHROPIC_API_KEY`:
    the app's checks and changes warn about one that is set.
  - `CLAUDE_CODE_HOVER_REST`, which switches on Claude Code's successor credential
    backend: the app's checks say it is on, and warn when the login is in the plaintext
    file, where what Pitboard reads may not be what Claude Code reads.
  - `PITBOARD_API_BASE`, a hook for Pitboard's tests: the app sends the requests meant for
    Anthropic and OpenAI to the loopback address it names, and ignores any other address.

  An app opened from Finder has these only when they are set with `launchctl setenv`. Both
  front ends already read the first two from Claude Code's settings files.
- The app takes a `claude` or `codex` it finds to be the tool's program only when it is a
  regular file you may run, as the command line does, and otherwise looks further. It took
  a directory with that name, offered the tool, and failed to start its sign-in.
- The **Open Link** window and the Share sheet name claude.ai and chatgpt.com in English, as
  the rest of what they say is. On a Mac set to another language they joined the two names
  in that language, as in `There is no claude.aiまたはchatgpt.com link in what was shared.`

### Security

- Pitboard refuses a site's sign-in link from outside however its path is written. It took
  `claude.ai/magic-link/%FF` and `chatgpt.com/api/auth/%C3`, whose paths hold a byte that is
  not text, and `claude.ai/magic-link/%CC%81`, whose path has a combining mark after a
  slash, for pages of the site, and offered to open them in an account's window. Somebody
  else's sign-in link opened there would sign the window in as them, under your label. A
  link that names a port or a user is refused whatever they hold: one whose port is too
  long for a number, or whose user name is not text, was opened without them.
- Pitboard refuses a link whose path has a `.` or `..` segment whatever else the path
  holds. It opened `claude.ai/x/../%FF`, whose path holds a byte that is not text, though
  WebKit drops the dot segments and loads another page than the link reads.
- A `pitboard://` link that names a user or a port, or has a path, is one Pitboard cannot
  read whatever they hold. One whose user, port or path is not text, such as
  `pitboard://open/%FF?url=claude.ai`, was read as if it had none.

## [0.7.0] - 2026-10-04

The app gives each enrolled account a window of its own on its tool's site, claude.ai or
chatgpt.com, so every account stays signed in side by side, and a page shared from a
browser opens as the account you choose. The command line and `pitboard-core` are
unchanged since 0.6.0.

### Added

- The app gives each enrolled account a window on its tool's site: claude.ai for a Claude
  Code account, and chatgpt.com for a Codex account. Each window keeps its own website data,
  so every account stays signed in side by side without browser profiles. Open one from the
  menu of Pitboard's item in the menu bar, the **File** menu or an account's shortcut menu
  in the Pitboard window: **Open claude.ai as work** for a site with one account, or an
  **Open claude.ai** submenu for several. Opening it again brings the open window forward,
  and a rename keeps the window and its sign-in. Back, Forward and Reload are in its
  toolbar, and in the **Go** and **View** menus with Safari's shortcuts. **View** also zooms
  the page in Safari's steps. Pitboard never reads, copies or changes what a site keeps,
  adds no script to its pages and makes no web session from a Claude Code or Codex login. A
  login Pitboard has no name for, and a Codex API key, get no window. See [Use each
  account's site](https://docs.usepitboard.com/guides/account-windows).
- When an account's window first opens on this Mac, a note at its top says how to sign in as
  the account's email address. Google's sign-in does not work inside apps, so the note names
  the ways that do. The site's sign-in pages load in the window, and one that a page opens
  in a new window gets a sign-in window of its own. That window shares the account's data,
  so what it signs in is that account. It is titled with the page's title and the host it
  is on, and closes when the sign-in closes it or with the account's window. Google's
  sign-in is stopped and never sent to the browser, where it would sign in the browser
  rather than the account. Other web pages open in your default browser, and a link to
  another app is refused.
- A download in an account's window saves to your Downloads folder and never replaces a file
  there: a second `report.pdf` is saved as `report 2.pdf`. One the site's own page starts
  saves at once. One that a frame inside the page, such as an artifact, or another site
  starts asks first, since an artifact runs code the site did not write. The window's
  **Downloads** button lists its downloads with their progress, and downloads carry on after
  the window closes. Quitting while one runs asks first, because quitting stops it for good
  and deletes what it wrote. A download that fails, or that you cancel, leaves no part of its
  file behind. WebKit quarantines each file, as a browser's are.
- **Edit** > **Find** > **Find** (Command-F) shows the system find bar at the top of an
  account's page. **Find Next** (Command-G) and **Find Previous** (Shift-Command-G) step
  through the matches. WebKit shows no find bar of its own, so without it a page could not
  be searched.
- An account's window opens at the last page of its site it showed, whether it is opened
  from a menu or macOS restores it. A window that has shown none starts at the site's home.
- **File** > **Remove Website Data** removes the cookies and everything else a site keeps in
  an account's window on this Mac, after asking. That signs the window out without telling
  the site, so the account stays signed in on your other devices and browsers. The window
  then starts again at the site's home.
- A claude.ai or chatgpt.com page in your browser opens as the account you choose: choose
  **Pitboard** in the browser's Share menu. Pitboard shows the link in its **Open Link**
  window with that site's accounts, and opens nothing until you click **Open**. The account
  chosen last for the site is selected first, or else the account in use. A link to another
  site is refused, and so is a site's sign-in link, since it would sign the window in as
  whoever the link belongs to. `chat.openai.com`, `www.chatgpt.com` and `chat.com` links
  open as chatgpt.com links, since each of those hosts redirects there. With no account on
  the site, the link waits while you add one, or opens in your browser. The Share extension
  is sandboxed. The `pitboard://` link it hands over only ever shows the **Open Link**
  window, since anything on the Mac can open one.
- While any of its windows is open, Pitboard has a Dock icon, is in Command-Tab and shows
  its menus in the menu bar. Its windows are the Pitboard window, Settings, each account's
  window and **Open Link**. When the last one closes, Pitboard is in the menu bar alone
  again. Without a Dock icon, a window behind another app had no way back, and without the
  menus it had no **Edit** menu or shortcuts. The Dock icon's menu opens each account's
  window and the Pitboard window. Opening Pitboard again from Finder or Spotlight with no
  window open opens the Pitboard window.

### Changed

- **View** > **Refresh** is one command for every window, with Command-R. In the Pitboard
  window it reads the accounts or the log again, or is **Check Again** on the **This Mac**
  pane. In an account's window it is **Reload Page**. Each pane's toolbar button had a
  Command-R of its own, and with Pitboard's menus in the menu bar the shortcut belongs to
  one item there.
- Forgetting an account also deletes everything its window keeps on this Mac, its sign-in
  included, and the forget alert says so before you choose. The deletion follows the next
  read of the accounts that succeeds, whether you forget in the app or with
  `pitboard forget`. Pitboard deletes only what it made for the Pitboard directory it reads.
  So a copy run with another `HOME` or `PITBOARD_HOME` never signs another copy's windows
  out.
- `brew uninstall --zap --cask pitboard-app` also removes the account windows' data, the
  windows macOS saved to restore, and the folders macOS makes for the Share extension.

## [0.6.0] - 2026-10-01

Pitboard tells apart everything that runs Codex with its login in memory, and the app quits
OpenAI's ChatGPT app around a Codex switch. `pitboard-core`'s public API changes, as the
last entry under Changed says, which is why this is 0.6.0.

### Added

- Switching Codex in the app while OpenAI's ChatGPT app is open asks first: **Quit ChatGPT
  and Switch** quits ChatGPT the way Command-Q does, switches, and opens ChatGPT again,
  whether or not the switch worked. ChatGPT runs a `codex` of its own with Codex's login in
  memory, and closing its windows leaves it running, so switched under it, it went on with
  the account left behind, and its own **Log Out** revoked the login Pitboard had just
  parked. If ChatGPT does not quit within 30 seconds, nothing changes. The command line
  never quits an app.

### Changed

- After a Codex switch or a sign-in again, Pitboard names what is still running the old
  login and what makes each take it: a `codex` session is quit and started again, the
  ChatGPT app is quit with Command-Q and opened again, Codex's background app server takes
  `codex app-server daemon restart`, and Codex in an editor takes **Developer: Reload
  Window**. Each of them runs a program called `codex`, and the warning used to count them
  all as sessions to quit and start again. `pitboard doctor`'s `codex_running` check lists
  them the same way, with process IDs, and says what to do in its advice.
- Pitboard counts only the processes of the person running it. A switch counted every
  user's `codex`, while `pitboard doctor` counted only theirs.
- In `pitboard-core`, `provider::Adoption::RestartRequired` names its `holders`,
  `service::Warning::SessionsStillRunning` and `SessionsKeepTheOldLogin` carry what is
  running as `holding` in place of `program` and `count`, and `doctor::CodexFacts::running`
  is by kind. The `holder` module and `service::Pitboard::holding` are new. These change the
  crate's public API.

## [0.5.2] - 2026-09-29

### Fixed

- After a banked reset on claude.ai, Pitboard went on showing the account's weekly limit as
  full until the old window's reset, a day and a half away in the case measured. A banked
  reset lowers the share and keeps the reset time, and Pitboard took the higher share in
  one window as the newer, so every answer with the lower share lost to the 100% recorded
  before. An answer from the service taken after everything recorded for the account now
  replaces it, however low, so the lower share shows the next time Pitboard asks. A plan
  upgraded in the middle of a window is followed the same way; before, the old, higher
  share stood until the window reset. A session's numbers still only move a limit forward,
  since they do not say when they were measured.

## [0.5.1] - 2026-09-29

Fixes that writing the documentation site turned up, and Pitboard's facts about Claude Code
read again, from 2.1.284.

### Added

- In `pitboard-core`, `assumptions::read_on` says which systems' builds a fact is read from,
  as the new `assumptions::Platform`.

### Changed

- Pitboard's facts about Claude Code are read from 2.1.284, and from its macOS build as well
  as its Linux one. The weekly check reported three facts moved from 2.1.281; all three were
  read from the Linux build, which has no keychain code, or from the runtime Claude Code
  ships in. The one real change is that Claude Code no longer moves its login to the
  plaintext file when the keychain is locked, and nothing in Pitboard depends on the old
  behaviour.
- The README introduces Pitboard and links to
  [docs.usepitboard.com](https://docs.usepitboard.com), where the guides and reference are.
- `SECURITY.md` is only the security policy: which versions get fixes, how to report a
  vulnerability, and what Pitboard does and does not protect against. Where parked logins
  are kept and what leaves your machine are on
  [Security and privacy](https://docs.usepitboard.com/security).
- How to make a release, how to replace the update key, and the Sparkle and Homebrew
  measurements a release rests on, moved from `CONTRIBUTING.md` to `RELEASING.md`.
- How the code is laid out, and the measurements of macOS, Claude Code and Codex it rests
  on, moved to `ARCHITECTURE.md`.

### Fixed

- `pitboard status --fresh`, and Refresh in the app, asked Anthropic or OpenAI again during
  a wait the service had asked for. The 0.3.0 notes and `pitboard doctor` said they did not.
  They keep that wait now, and still ask a service again after it could not be reached.
  When a service had never answered for an account and then could not be reached, Pitboard
  said the account was rate limited.
- With `PITBOARD_NO_ARGV=1`, renewing a parked login too large for `security`'s standard
  input spent its refresh token and then could not store the new one. The parked login was
  lost. Pitboard refuses before asking the service now, and the parked login stays as it was.
- A switch whose login could not be read back said to run `pitboard` again, which only
  reads. It names the `pitboard use` that finishes or undoes the switch.
- With Codex's `auth.json` missing, `pitboard doctor` suggested `pitboard use codex/<label>`,
  which refuses while nothing is signed in. It says to sign in with `codex login`.
- The man page listed a page per command, such as `pitboard-status(1)`, and none of them is
  installed. It sets out every command, with its arguments and options, on its one page.
- The app never said that Claude Code is not installed: it waited for a read to fail in a
  way no read does. It says so when it finds neither `claude` nor `codex`, and neither tool
  has a login or an enrolled account.

### Security

- The menu bar app ignored `PITBOARD_NO_ARGV`, and so did daily renewal unless its
  scheduler set it. Either could pass a large login to `security` as an argument, which
  another process running as you could read while the call lasts. The app reads
  `PITBOARD_NO_ARGV` from its own environment, and a schedule installed while it is set
  keeps it.

## [0.5.0] - 2026-09-28

### Added

- Rename an account from the window, which only the command line could do.
- In the window's account list: Use with a double-click or Return, Forget with Delete, and
  Copy Email Address and Sign In Again in each account's menu.
- About Pitboard in the menu.

### Changed

- The menu bar item opens a menu instead of a panel, as macOS asks of a menu bar item. Each
  account is an item under its tool, checked when it is the one in use and subtitled with
  what its limits stand at, and choosing another switches to it. Advice to switch, anything
  else worth a look, and an update that is ready come first. It opens at once, closes the
  way every menu does, and works from the keyboard and with VoiceOver.
- Everything that needs typing or room is in Pitboard's window: every account with its
  limits drawn out, everything Pitboard has to say in full, the activity log, and what it
  finds about this Mac. Adding an account, signing in again, naming the account in use and
  renaming one are sheets over it, and a sheet stays open while your browser is in front. The
  panel closed the moment the browser came forward, and took the sign-in's code field with
  it.
- A switch, a rename or a forget that fails says why in an alert, instead of in a line of
  the panel that the next read could replace before anyone saw it. A sign-in or a name that
  fails says so in its sheet, with what you typed still there.
- Settings has a Command Line tab of its own. "Open Pitboard at login" says when macOS is
  waiting for you to allow Pitboard in Login Items, and opens them. "Menu bar shows" picks
  the account and its usage, the usage alone, or the icon alone, for a crowded menu bar.

### Fixed

- The panel said "updated just now" for as long as nothing else changed. The menu says the
  time of the last read, and the window's reset times move on while it stays open.
- Giving up on an interrupted switch was reported with a warning sign, as though it had
  failed. It is a note now, which you dismiss once read.
- A failure to turn daily renewal on or off was said in the panel rather than beside the
  switch in Settings, and a failure to change "Open at login" was not said at all.
- Switching one tool's account put away the advice about another tool's account that had
  run out, and it was not offered again until that account ran out once more.
- Advice to switch kept offering the account it first named after that account was
  forgotten, expired or ran out itself, and choosing it failed. It now offers the best
  account there is at each read, and goes away when there is none.
- A read that was already waiting on Anthropic or OpenAI when you switched landed after the
  switch, with who was signed in before it, and put away what the switch said, such as the
  reminder to restart running `codex` sessions. A switch made in a terminal during such a
  read was taken as seen, and the menu bar went on naming the account before it until the
  next read.
- Why an account's numbers are not new was said only for an account that could not be
  used. The window says it for every account, the one in use included: that its service
  could not be reached or is rate limiting, or that Claude Code's session has expired.
- The panel and the window described accounts differently: the window had no way to sign in
  again or forget an account and did not say how long a parked login lasts, and the panel
  did not say how long the account in use lasts at its rate. Both now say the same things,
  worked out once.

## [0.4.1] - 2026-09-26

### Changed

- A usage reading only moves forward. Pitboard keeps one reading per account, every front
  end records into it and every front end shows it. A later reset is a newer window, and
  within one window the higher share is the newer, so numbers a session has held since its
  last response can no longer replace newer ones, whoever writes last. Where a window's
  share falls, as it does when a plan is upgraded in the middle of one, the old, higher
  share stands until that window resets.
- The app's Documentation item opens docs.usepitboard.com.

### Fixed

- Sessions on one account, and the menu bar, disagreed about the account in use. Each
  session showed the numbers of its own last response, so busy sessions read 22%·6% while an
  idle one read 20%·5%, and the menu bar showed what it had last asked Anthropic, or Claude
  Code's own cache. The status line now records what its session's responses bring wherever
  it is newer, every session shows the newest numbers any of them has recorded, `pitboard
  status` does the same, and the menu bar follows the readings within seconds without
  asking anyone. The README's status line settings add `"refreshInterval": 10`, so an idle
  session picks them up too.
- After a switch, a session still holding the numbers of the account before could record
  them as the account switched to, until the next read. A session's numbers do not say
  whose they are, so the status line now keeps what each session passed at its last run,
  and which account Claude Code's config named then, in `~/.pitboard/sessions.json`. It
  records only what a session's response moved with the same account named before and
  after, and nothing in the half minute sessions take to follow a switch, so a session left
  idle is never recorded as anyone, whatever has happened to other accounts since: a
  switch, a `/login`, an account forgotten. What a session passes the first time Pitboard
  sees it is left out, and so is what it passes as the account named changes; its next
  response is recorded. A `/login` in Claude Code leaves Pitboard no time to count from,
  and for the half minute after one a session's response can still be the account
  before's. The status line leaves that out where Pitboard's reading of the account before
  has the same window; otherwise, or where the two accounts' windows reset within the same
  minute, the account signed in after can show the higher of their shares until that
  window resets.
- The panel's advice to switch, once the account in use had run out, went away at the
  app's next read, as soon as the panel was opened again, while the account was still out.
  It now comes as soon as the numbers show the account run out, stays for as long as they
  do, and is told once.

## [0.4.0] - 2026-09-25

In the tap, the name `pitboard` now means the command line.

From the old formula, `brew update` warns that it did not install the cask that replaces
it, and Pitboard stays at 0.3.0. The two commands it prints leave the formula in front of
the cask, so install the cask in its place instead:

```sh
brew uninstall --formula pitboard
brew install datlechin/tap/pitboard
```

From the old app cask, `brew update` replaces the app with the command line, once. Your
settings and `~/.pitboard` stay. To get the app back, with the command line inside it, run
these in this order, before or after that `brew update`:

```sh
brew uninstall --cask pitboard
brew uninstall --formula --force pitboard
brew install --cask datlechin/tap/pitboard-app
```

Leave `--zap` out when you remove the old app cask. Its zap moves `~/.pitboard` to the
Trash, and Homebrew runs the zap of a cask as it was installed, whatever the tap says by
then.

A copy of the app from a release updates itself as before and brings the command line with
it.

If you turned on daily renewal in a 0.3.0 app, its schedule ran the app itself and renewed
nothing. Once the app from this release is in its place, the old schedule keeps renewing,
through the command line inside the app, until you open the app. Opening it then points the
schedule at that command line. A schedule that runs a `pitboard` that is not there any more
is left as it is. On Linux that includes one turned on with the formula: it ran the copy
inside the formula's own directory, which goes with the formula. Turn such a schedule off
and on again, in the app's Settings or with `pitboard schedule uninstall` and then
`pitboard schedule install`. `pitboard doctor` from this release says whether yours needs
it, and so does Settings, Advanced, "Check this machine".

### Added

- Settings can put the app's command line on the `PATH`. The Advanced tab says which
  `pitboard` a terminal runs, whether it is the app's own and how that one is updated, and
  when there is none, "Install command line tool…" links `/usr/local/bin/pitboard` to the
  one inside the app, once macOS has asked for an administrator's password. It never
  replaces a `pitboard` somebody installed or a file that is not a link, and never links to
  the temporary copy macOS runs an app from before it is moved to Applications.
- An account whose parked login has expired has a "Sign in again" button in its row, which
  starts the same sign-in as adding an account. The row used to say to run `pitboard enroll
  <label> --sign-in` in a terminal, which somebody with only the app does not use.

### Changed

- Installing Pitboard no longer needs Rust. `brew install datlechin/tap/pitboard` is now a
  cask, on macOS and Linux, that installs the release's own command line for the machine,
  with its man page and completions: signed and notarised on macOS, attested, and checked
  against the checksums the release took of its own files. It was a formula that fetched
  Rust and compiled Pitboard, which took minutes and a toolchain nobody had asked for.
- The menu bar app's cask is `pitboard-app`, and the app carries the command line inside
  it, at `Pitboard.app/Contents/Helpers/pitboard`. The cask links it onto `PATH` with its
  man page and completions, so an update, from Sparkle or from Homebrew, moves the app and
  the command line together. They used to be two installs that moved separately, and the
  app's cask depended on the formula. The two casks conflict, since both link `pitboard`.
  The app's bill of materials lists the command line and the crates only it uses. The
  release writes the tap's own README with the casks, so it names both.
- A release no longer publishes a source tarball. Only the formula installed from it, and
  the source is on crates.io and in the tag.
- `pitboard-core`: the report `uninstall` returns says whether the renewal schedule was
  taken away, and it and `doctor::Facts` are now `#[non_exhaustive]`, so a later field is
  not a breaking change. A breaking change for anyone who built either with a literal,
  declared as such; nothing changes for the command line or the app.

### Fixed

- Daily renewal turned on from the app renewed nothing. The schedule recorded the program
  that asked for it, which from the app was the app itself, so launchd started a second
  menu bar app every day and no parked login was renewed. The app now names the command
  line inside it. A schedule an older app wrote starts the app, which now hands the renewal
  to that command line, so the old schedule keeps renewing until the app is opened. Opening
  it then points the schedule at the command line, which `pitboard log` records. The app
  turns renewal on only where that command line will still be there when the schedule runs:
  not from the temporary copy macOS runs an app from before it is moved to Applications,
  which is gone once the app quits, and not from a build with no command line inside it.
  Settings says why. New codes, from the app's bindings: `schedule_program_missing`,
  `schedule_program_temporary` and `schedule_program_unnamed`. `pitboard doctor` reads the
  installed schedule back and fails when the `pitboard` it runs is gone or is an app, and
  says to turn renewal off and on again.
- On Linux, a renewal schedule turned on from the command line stopped working at the next
  `brew upgrade`. It named the running Pitboard with every link resolved, which from
  Homebrew is inside a directory named after the version, and the upgrade deletes that
  directory, so systemd failed to start it every day after. It now names the path Pitboard
  was started by, such as the link in Homebrew's `bin`, when that leads to the same program.
- Removing Pitboard leaves no renewal schedule behind. `pitboard uninstall` takes it away
  first and says so, as `schedule_removed` in `--json`, where before it was left running
  `pitboard renew` every day. Both casks take it away on `brew uninstall --zap`, and not on
  a plain `brew uninstall`, because Homebrew runs a cask's uninstall steps on every upgrade
  too. Neither touches `~/.pitboard`: it is the only index of the parked logins, so run
  `pitboard uninstall` before removing Pitboard.
- Advice about upgrading and removing Pitboard no longer assumes Homebrew. A state file
  from a newer Pitboard said to run `brew upgrade pitboard`, and `pitboard uninstall` said
  to remove the binary with a package manager. Both now say to update or remove Pitboard
  the way it was installed, and the first adds that the app's Check for Updates moves only
  the app and the command line inside it.
- On Homebrew 6 and later, `brew install --cask datlechin/tap/pitboard` failed with
  `build.rb ... exited with 1` unless the formula was installed first. Homebrew trusts only
  the name it is asked to install, and refused to build the formula the app's cask depended
  on. `pitboard-app` depends on nothing.
- `cargo binstall pitboard` no longer falls back to a third party's build when it cannot
  fetch the release's.

## [0.3.0] - 2026-09-24

### Added

- Codex, beside Claude Code. `pitboard enroll codex/work` records the Codex account signed
  in now, reading its account, email and plan out of its own ID token with no network
  call; `pitboard enroll codex/work --sign-in` runs `codex login` in a private
  `CODEX_HOME` so the account in use stays signed in; `pitboard use codex/work` switches
  it; and `pitboard` shows each Codex account's five-hour and weekly limits from the same
  usage read Codex itself makes, which spends no quota. Until a Codex account is enrolled,
  Pitboard reads nothing of Codex's and asks OpenAI nothing. Everything Pitboard does for Codex
  was read out of codex-cli 0.154.0 and is dated in a register of its own, checked against
  every new build by the conformance run twice a week, and still green on 0.156.1.
  Codex is not Claude Code, and Pitboard says where they differ rather than hiding it:
  - A running `codex` never notices a switch, so a switch says to restart it instead of
    counting down, and names how many sessions are still using the outgoing account.
  - `codex login` and `codex logout` revoke the stored refresh token, so a Codex park is
    never a copy: the outgoing login is moved into the vault and read back before
    anything replaces it. Signing out inside a session still running from before a
    switch would revoke the login just parked, and the switch says so.
  - Codex's keychain stores are items Codex made for itself, which every read by another
    program would put a permission prompt in front of, so Pitboard handles Codex's
    default store, the `auth.json` file, and refuses the others with a reason.
  - Codex takes no lock, so a session still running from before a switch can refresh its
    token in the middle of one. The live login is read again just before it is replaced,
    and nothing is written over a login that moved; a login that ends up naming two
    accounts is refused rather than parked.
- The menu bar app handles both tools. With accounts of both, the panel lists them under a
  heading per tool, and the menu bar follows the signed-in account closest to running out.
  When an account runs out, only another account of the same tool is offered. A Codex
  switch has no countdown: the panel says running `codex` sessions keep the old account,
  and how many Pitboard found, and keeps saying so until that tool switches again rather
  than until anything at all changes. "Add another account" asks which tool when more than
  one is installed, and a Codex sign-in shows the address `codex login` printed. Cancelling
  a sign-in no longer holds the app until the tool says something, which a Codex sign-in
  never does before the browser is done.
- Labels belong to a tool. `work` can be a Claude Code account and a Codex account at
  once, `codex/work` says which, and a bare `work` still means what it always did as long
  as it names one account; where it names two, Pitboard lists both rather than picking.
  A bare name for a new account means Claude Code, so every command written before there
  was a second tool does what it did. New codes: `provider_unknown`, `label_ambiguous`,
  `sign_in_not_isolated`, `live_store_unsupported`, `state_names_unknown_tool`,
  `recovery_elsewhere`, `sessions_still_running`, `codex_program_missing` and
  `codex_not_found`.
- No parked login goes unnamed. Every name Pitboard is about to write a login into is
  written down first, and the next command resolves any that nothing refers to: given back
  to the account whose name it carries when that account holds nothing, deleted when nobody
  wants it, and left alone when the store could not be read. Before this, a run killed
  between writing a login and recording it left a live refresh token that no entry named,
  never renewed, never deleted by `pitboard uninstall`, and on macOS not listable by any
  tool a person has. `pitboard doctor` reports anything still outstanding.
- `pitboard adopt` takes over a `~/.pitboard` that another computer wrote. The machine
  stamp is right and its reason is sound, but the refusal stopped every command including
  `uninstall`, so somebody whose home and keychain arrived through Migration Assistant met
  a tool that would not do anything and an error telling them to enrol again with no
  command that made it possible. Adopting keeps what is a fact about an account, the label,
  the email, the uuids and the remembered numbers, and drops every parked login, because a
  login belongs to the computer that signed in. It deliberately does not ask Anthropic
  whether a parked token still works: finding out means exchanging it, and exchanging it is
  the act that would rotate it past the other machine's copy.
- `pitboard repair` asks the credential store itself what parked logins are on this
  machine, rather than reading Pitboard's own index, and accounts for every one it finds:
  given back to the account whose name it carries, or deleted when no account here wants
  it, or reported and left exactly where it is. Only a name this Pitboard wrote down itself
  is ever deleted: a keychain belongs to a whole login session while Pitboard's records
  belong to one `PITBOARD_HOME`, so a parked login it cannot account for is evidence of
  another Pitboard rather than of an orphan, and deleting it would end that account's
  session for somebody who never ran the command. Giving one back is additive and safe on a
  guess; deleting one is not, so on macOS a login given back that this Pitboard never wrote
  down is deleted only once Pitboard has used it, by switching to it or renewing it, even
  when the renewal is stopped before it records what it got back. Parking over it, `forget`
  and `uninstall` leave it where it is, and `uninstall` says how many it left, as
  `parks_left` in `--json`. Elsewhere the vault is a directory inside Pitboard's own, which
  no other Pitboard parks in, so whatever `repair` finds there is this one's. Measured
  first: `security dump-keychain` without `-d` never prompts, takes 0.06 seconds, emits no
  secret of any item, and does not slow later reads.
- A crash matrix: every durable step of a switch, an enrolment, a renewal and a forget,
  killed where it stands, recovered, and checked against what must be true afterwards
  rather than against a particular outcome. Every case runs recovery twice, because a
  recovery that only works once leaves a machine nobody can fix, and once with Anthropic
  unreachable, because the answer then must be to change nothing. The two windows above
  are what it found on its first run.
- A failure that came from asking Anthropic now carries a cause beside its code, in both
  `--json` and the app's bindings: `unreachable`, `rate_limited`, `server_error`,
  `answer_not_understood`, `login_refused` or `token_expired`, each saying whether asking
  again is worth anything. Before this, everything that was not a 401 arrived as
  `identity_unverifiable` and a sentence of prose, so nothing could tell being offline from
  being rate limited from a login Anthropic had finished with. `status` tells the same three
  apart too, where they used to share one code.
- The app follows a switch made anywhere else on the machine. Three front ends ran on one
  machine and none could tell when another had changed something, so a switch typed in a
  terminal left the menu bar naming the account the person had just stopped using for as
  long as five minutes, with a button offering a switch that had already happened. The app
  now asks every couple of seconds when Pitboard's account index last changed, which is one
  stat of one file, and re-reads what it already knows when it moves: no network, no
  keychain and nothing asked of Anthropic. Deliberately the index alone and not the whole
  directory, because the status line writes usage readings after every message in every
  open session.
- The app has a window and a Settings scene. The panel is 400 points wide and everything
  that needed more than that either expanded inside it or sent the person to a terminal,
  and everything configurable lived in an ellipsis menu where opening at login sat between
  hiding the checks and quitting. The window holds each account with what its limits have
  been doing, the whole log of what Pitboard has changed, and all of what it found about
  this machine. Settings opens with Command-comma where people look for it, and reaches the
  scheduled renewal the core could already do and the app could not.
- A first run for somebody who installed only the app. The cask puts the command line on
  the machine too, but an empty panel used to say "Run `pitboard enroll <label>`" and a
  machine without Claude Code said one line of error, which between them sent every new
  person to a terminal to find out whether the thing they had just installed worked. The
  app now names the state it is in, no Claude Code, nobody signed in, signed in but
  unnamed, or one account with nothing to switch to, and offers the one next step, each of
  which it can do itself. Nothing is asked before the first read. The first launch ever
  opens the window, because a status item is invisible to somebody who has just installed
  it.
- `doctor` reads the modes of everything on the disk that holds a login: the plaintext
  credential file, Pitboard's vault and every parked login in it, and fails when anyone
  but the owner can read one. Where there is no keychain, a mode bit is the whole of that
  protection, and a backup restore, a `cp -r` or a careless umask changes one quietly.
- The app's bindings reach the rest of the core: the offline report, giving up on an
  interrupted switch, the change log, renewing, and the renewal schedule. A read that cannot
  reach Anthropic now falls back to the last numbers measured rather than an empty panel,
  which said the accounts were gone. An interrupted switch that recovery cannot finish has
  a way out in the panel rather than sending somebody to a terminal, which is the one place
  a person who installed only the app has not got. And every warning is shown rather than
  the first: a switch can warn about an overriding environment variable and a config that
  did not update at once, and showing one of them is how somebody fixes the wrong thing.
- Pitboard keeps what each account's limits have been doing, and says how long each account
  lasts. The single decision this tool exists to support is which account to use next, and
  it answered with two instantaneous percentages and left the arithmetic to the person: 73%
  of a weekly limit means nothing without knowing whether it was 40% this morning. It
  already had the data and threw it away, one snapshot per account in a map every write
  replaced. Each account now has a series, and from it the only number that answers the
  question: until the tightest limit fills at the rate it has been filling, or until it
  resets, whichever is sooner. It says nothing until there are three readings a quarter of
  an hour apart, because a wrong runway tells somebody to switch when they need not, which
  is worse than no runway. Sized before it was written: one reading is 319 bytes, a reading
  that says nothing new is not kept, and anything older than a fortnight goes.
- `pitboard doctor` says what is taking up the room in a login that will not fit, largest
  first and named per MCP server. A login too big for `security`'s standard input is almost
  never the login: on one real machine the OAuth block was 506 bytes and eleven MCP server
  tokens were 3679. Saying "8503 of 4032 bytes" left a person to guess which of those to
  sign out of, and the answer was in the document Pitboard had already read.
- `pitboard status` names the credential slot it is speaking for, in `--json` always and in
  the human report when it is not the default. `CLAUDE_CONFIG_DIR` selects a different
  keychain item, so who is signed in is a fact about one slot and not about the machine;
  the report named accounts without ever saying which slot it meant.
- `pitboard renew` renews every parked login that is due and does nothing else, and
  `pitboard schedule install` hands that to the platform's own scheduler, a LaunchAgent on
  macOS and a systemd user timer on Linux. Until now the only two things that renewed a
  parked login were somebody typing `pitboard` and the menu bar app's poll, so the tool was
  safe for a macOS user who leaves the app running and quietly unsafe for everyone else:
  go away for the refresh window and every parked login is dead. It is opt-in and stays
  opt-in, it runs one verb, it never switches and never asks for usage, and `RunAtLoad` is
  off because installing it is not a reason to talk to Anthropic that second.
- `pitboard doctor` says when an account has not been switched to for longer than a refresh
  token's own life. Pitboard renews a parked login for as long as its account is enrolled,
  so one enrolled and forgotten keeps a live, continuously rotated token on the machine
  indefinitely, and nothing said so. Nothing is dropped on a timer Pitboard chose: the
  threshold is the token's own lifetime and all the check does is say it.
- Pitboard owns how often it asks Anthropic anything. `status` asked about every enrolled
  account plus the live login on every run with no memory of having just asked, and the
  menu bar app asked the same questions every five minutes, on every wake and on every
  panel open, from a process that knew nothing about the command line's; nothing honoured
  `Retry-After`, so a 429 became a stale row and the identical request went out on the next
  tick. Two accounts and a running app is on the order of six hundred authenticated
  requests a day nobody asked for, and it is the part of Pitboard's behaviour that reads
  least like a person switching between their own accounts.
  An account is now asked about again once the tightest limit it describes could have moved
  by a percentage point, which is three minutes for a five-hour window and derived from the
  window rather than picked. A refusal is recorded and waited out, with `Retry-After`
  believed over anything Pitboard would choose, and the wait is shared by every front end
  on the machine. `pitboard status --fresh` asks anyway; a wait Anthropic asked for is not
  overridden. `pitboard doctor` says what is being held back and for how long.
- A conformance checker, and a job that runs it. `pitboard-conformance` reads the literals
  each of Pitboard's facts about Claude Code is readable by out of a Claude Code build and
  says which are still there; a scheduled workflow fetches the newest build twice a week
  and runs it. Shallow on purpose, and it says so: a literal being present does not prove
  the behaviour around it is unchanged, while a literal disappearing does prove something
  moved. Measured across six builds before it was trusted at all: the probe set holds from
  2.1.273 through 2.1.278 and correctly goes red on 2.1.124, which predates the credential
  write lock, two of the five account-scoped keys, and the keychain error classification.
- A park holds the account's whole slice of Claude Code's credential document rather than
  its OAuth block alone: the four other keys a logout deletes go with it, and a switch back
  puts them there. Before this, switching away deleted them and switching back could not
  restore them, so an account came back to Claude Code with slightly less than it left.
  Parks written by earlier versions still restore, and still behave exactly as they did.
  Measured on one real account: the slice is 524 bytes against 506, which is nothing against
  the 4032-byte ceiling. Whether restoring a device token spares a re-verification is not
  measured and is not claimed anywhere.
- Pitboard reads what a session here would actually authenticate with, from files rather
  than from three environment variables. Claude Code resolves authentication from layered
  settings, and a managed policy or a line in a person's own `settings.json` can set an
  `env` block, an `apiKeyHelper`, or a third-party provider switch; under any of those a
  session ignores the login Pitboard moves and every switch is a no-op that reported
  success. Worse, the app has no shell environment at all, so the one surface that could not
  warn was the one most likely to be used on a machine that needed the warning. Managed
  settings and the person's own are read; a project's are deliberately not, because an
  answer true only in the directory Pitboard happened to run in is worse than none.
  `pitboard doctor` says which it is, and a custom OAuth endpoint set in a file now refuses
  a switch the way one set in the environment always did.
- Every fact Pitboard stands on about Claude Code is now a list rather than a comment:
  what it is, where in Claude Code it was read, which build it was last verified against,
  and what in this crate stops being true if it moves. `pitboard doctor` says which Claude
  Code is installed here, read off disk and never by running it, beside the build those
  facts came from. It states rather than warns: Claude Code ships several times a week, so
  a mismatch is the ordinary state of the world within days and warning about it on every
  machine would be noise. An assumption that has actually stopped holding is a different
  thing and wants a probe, not a version number.
- An interrupted switch is recovered without a network. Deciding what it did meant asking
  Anthropic who owns the live login, so a switch interrupted on a plane, or while Anthropic
  was having a bad morning, stopped every command that changes anything until it could be
  asked. The record now carries a fingerprint of the refresh token on each side, which is
  the same eight bytes of SHA-256 a park already records, so the common case is a
  comparison. It narrows the dependency rather than removing it: Claude Code refreshing the
  token inside those few seconds leaves a login matching neither side, and that is still a
  question for Anthropic, and still changes nothing when Anthropic cannot be reached.
- Every artefact carries a CycloneDX bill of materials, generated from the lockfile per
  target, published with the release and attested like the artefact it describes. So are
  `SHA256SUMS` and `appcast.xml`, which are made in the same job and published in the same
  release as the files they describe: on their own they said a download had arrived whole
  and nothing about who put it there.
- `CONTRIBUTING.md` has a procedure for replacing the Sparkle update key or the Developer
  ID certificate, and `.github/workflows/rotation.yml` runs the awkward half of it every
  month against keys it makes on the runner, a feed on `127.0.0.1` and bundles under an
  `invalid.` identifier. It names no repository secret, so it cannot reach the real key,
  and CI checks that it still names none. Running it found the trap: `generate_appcast`
  will not sign a bundle carrying a key other than the one it is handed, and says so by
  writing the feed with no signature and exiting 0.

### Changed

- Pitboard's account list is at schema 4: every account says which tool it is for, and
  which account is signed in is kept per tool. A schema 3 file is brought forward on its
  first read with nothing moved and nothing in the keychain or the vault touched. An older
  Pitboard refuses the new file and says to upgrade whichever of the command line and the
  app is behind, rather than calling it corrupt.
- Messages name the tool they are about. An error that used to say "Claude Code",
  "Anthropic" or `claude` whatever the account now names that account's tool, its service
  and the command that signs in to it. Codes are unchanged. A few Claude Code messages
  gained the tool's name where a second tool made them ambiguous: "nothing is signed in
  right now" reads "nothing is signed in to Claude Code right now". A name a message tells
  somebody to type is qualified, `claude/work`, where another tool has an account of the
  same name and a bare one would be refused as ambiguous.
- A Claude Code credential with no account in it, which is what `/logout` leaves beside
  the machine's MCP tokens, is nobody signed in. `use` and `enroll` answered
  `live_credential_shape_unexpected` with exit 3 and now answer `live_credential_absent`
  with exit 1, or `live_credential_elsewhere` where Claude Code's config still names
  somebody, and `doctor` warns rather than fails.
- In `--json`, `use` gains `provider` and an `adoption` object saying whether sessions
  already running follow on their own within a number of seconds or need restarting;
  `adoption_ceiling_seconds` is null for a tool that needs restarting. Enrolments and
  renewals gain `provider`. All additive.
- In `--json`, every `status` account gains `provider` and `qualified`, its name with the
  tool spelled out (`codex/work`), null for a login nothing has enrolled. Two stale codes
  are new: `login_unreadable`, for a tool's login that is there and could not be read, and
  `login_unusable`, for one that was read and is no account Pitboard can park or switch,
  such as an API key. Where no record pins such a login on an account, it gets a row of
  its own with no label, email or account id, and only for a tool with accounts enrolled.
  `doctor` gains `environment.codex` (`home`, `present`, `backend`, `login_present`,
  `version`) and a section about Codex whose codes all start `codex_`: `codex_backend`,
  `codex_auth_file`, `codex_login`, `codex_version`, `codex_running`, and for a Codex
  account `codex_parked_login` and `codex_dormant_account`. What somebody chose is not a
  fault: a keychain store fails only where it puts enrolled Codex accounts out of reach,
  an API key login is a warning only where there are Codex accounts to switch to, and
  either is otherwise stated without a warning. All additive: Claude Code's rows, checks
  and codes are what they were.
- `CLAUDE_CODE_CUSTOM_OAUTH_URL` refuses changes to Claude Code accounts, and no longer
  stops a change to a Codex one.
- `pitboard-core` is reorganised around a provider boundary, and much of what it exposed
  moved or changed shape: errors that name a tool carry it, `switch::settle` and
  `sign_in` take the tool, renewals are keyed by account, and Claude Code's own document
  rules live under `provider::claude`. A breaking change for anyone building on the crate,
  declared as such; nothing changes for the command line's contract beyond the additions
  above.
- `pitboard-core` says what it supports. The interface other programs may build on is
  `service::Pitboard`, `context::Context` and what they return; the rest is reachable for
  this repository's own front ends and may change in any release. The enums a caller reads
  codes out of are now `#[non_exhaustive]`, so adding a code is not a breaking change for a
  consumer, which is what the command line's JSON contract has always promised. Writing
  Pitboard's index is no longer reachable from outside the crate: every change goes through
  `switch`, which records its intent first. Marking those enums and withdrawing `state::save`
  are themselves breaking changes for anyone who built on 0.2.0, so this is the release that
  makes them, while the crate is young enough for that to cost nothing. Nothing changes for
  anyone using the command line or the app.
- Claude Code's supervisor daemon is named as what it is, a second writer of the login that
  runs on a schedule of its own. It takes the same write lock and re-reads the credential
  inside it, so it cannot put an older account back over a switch. `doctor` reports it.
- The storage v5 question is settled rather than open. The successor backend replaces the
  fallback half of Claude Code's chain and only for a caller that hands a backend in, so an
  ordinary `claude` still reads the keychain first. `doctor` now warns only for the
  combination that can mislead, the flag on and the login in the fallback.
- Linux is decided rather than assumed. Claude Code has exactly two guarded credential
  stores, the macOS keychain and the Windows credential manager behind a feature flag;
  searched whole, the shipping build carries no libsecret, no `org.freedesktop.secrets`, no
  gnome-keyring and no Secret Service. So on Linux its login is a plaintext file at mode
  0600 and Pitboard's parked copies are files beside it, which is what Pitboard already
  did. There is no keyring backend to add. The facts Pitboard stands on can now rest on an
  absence: each one may name literals whose arrival would disprove it, and the conformance
  run fails when one turns up, because a fact resting on something not existing is wrong
  the moment it does and nothing disappearing would ever say so.
- README and SECURITY.md say what a downloader can actually check: the archives, the
  source tarball Homebrew builds from, `SHA256SUMS`, a bill of materials beside each
  artefact and `appcast.xml` are all attested, and `gh attestation verify` checks any of
  them against the workflow and commit that produced it. They named one archive.
- A failed read in the app shows that failure's own warnings rather than the last
  successful read's. A fresh network error used to sit above warnings about things that
  may have been fixed since.
- Text in the app grows with the person's own. Every column was a width fixed at the
  default size, so anyone with larger text got a limit name running into its bar and a
  reset time clipped off the right. The checks were read out without saying whether they
  passed, an account row was six separate stops for a screen reader rather than one, and
  the status item announced itself as "speedometer".
- A login too large for `security`'s standard input is now written the only other way
  `security` offers, as an argument, which is what Claude Code does for the same login on
  every token refresh. The switch says so, and `doctor` shows the size. `PITBOARD_NO_ARGV=1`
  refuses instead. Measured first: writing the item in process would have made every later
  read by `security` take about a second instead of 0.01, for good.
- The release publishes the Homebrew tap itself, from the checksums it has already computed
  for `SHA256SUMS`, and then installs the formula and the cask from the public tap on a
  clean runner and fails if what it serves is not the version just released. The tap used
  to be written by a workflow of its own inside the tap repository, waking every six hours
  and taking the checksum of whatever it downloaded, so `brew install` could be a version
  behind for most of a day with the release green and nothing anywhere saying so. The
  formula now builds from a source tarball the release publishes and attests, rather than
  from the archive GitHub generates for a tag, whose bytes GitHub has changed before now.
  `packaging/update-tap.sh` is gone with the second download it did.

### Fixed

- The menu bar app finds a tool installed through a Node version manager or an npm prefix,
  and can start its sign-in. An app opened from Finder has none of a shell's `PATH`, so it
  looked for `claude` and `codex` only where their own installers put them, and did not
  offer one installed through nvm, volta, fnm, asdf, mise, pnpm, bun or a custom npm
  prefix. One it did find could not start if npm had installed it: an npm install is a
  script run by `node`, which was not on the app's `PATH` either. The app now asks the
  person's login shell for its `PATH`, off the main thread, which runs its startup files as
  a terminal does, and looks there after `PITBOARD_CLAUDE` or `PITBOARD_CODEX` and before
  the installers' places, passing over a folder macOS asks permission for, such as Documents
  or iCloud Drive. A shell that does not answer within five seconds is stopped with
  everything it started, and the app looks where it did before; it asks once more a minute
  or more later, when the form for another account opens or a sign-in starts, because
  startup files are slowest while the machine is still logging in.
  Every sign-in, from the app and the command line, now starts the program by the path it
  was found at: the first file on the `PATH` that can be run, in a directory named from the
  root, so what is found is what starts. A program found where that `PATH` does not reach,
  as the app finds one where its installer put it, has its own directory put first, which
  is where npm puts the `node` that installed it; one found on the `PATH` runs with it as it
  is, so it finds the `node` the terminal would.
- Signing in again to the account in use puts its new login in use. `pitboard enroll
  <label> --sign-in` for the account signed in now parked the new login and left the tool
  on the old one, which is the login somebody signs in again to replace, and the next
  switch away parked the old login over the new one and deleted it. For Codex, whose
  outgoing account is read from its ID token without asking OpenAI, the login kept could be
  one whose refresh chain was already revoked. The new login now goes in place of the old
  under the same lock, checks and read-back as a switch, nothing is parked, and `--json`
  says `in_use`; a label this enrols for the first time says it was enrolled. Where Pitboard
  cannot tell that the login in use is that account's, the new one is parked as before,
  because writing over a login nobody can name could lose it, and when that is the account
  Pitboard last saw in use it says so and why, with the new code
  `sign_in_parked_not_in_use`. A running `codex` keeps the old login and can write it back
  when it refreshes, so the sessions are counted and warned about, with the new code
  `sessions_keep_old_login`. A new login that could not be written was not kept, and says to
  sign in again, with the new code `sign_in_not_kept`. A new login Pitboard could not
  confirm is in use, where the old one may be gone too, is parked rather than lost, with the
  new code `sign_in_not_installed`, and still says what writing and parking it warned about.
  Where it was in use after all, that parked copy holds the refresh token the tool is using:
  no renewal spends it, and the next change or renewal that can read the tool's login drops
  it. The menu bar app says the new login is the one in use, keeps what that tool's last
  switch said beside it, and shows what a sign-in of any account warned about.
- A renewal killed after writing the fresh login and before recording it could lose the
  login: the next change deleted the fresh copy as unrecorded and kept the old one, whose
  refresh token the service had already spent. A copy Pitboard wrote down itself now
  replaces an older copy of a different chain. A renewal whose record cannot be saved keeps
  what it wrote for the next run instead of deleting it.
- An interrupted switch is recovered only where its tool's login was when it started. Read
  from another `CODEX_HOME` or `CLAUDE_CONFIG_DIR`, recovery compared the switch with a
  login that had nothing to do with it; it now stops with `recovery_elsewhere`.
- A change refused over the name it was given still settles an interrupted switch, like
  every other change. `pitboard use nosuchlabel` after an interrupted switch refused the
  name and left the switch for a later run without a word about it; it now finishes or
  undoes the switch, records that in `pitboard log`, and reports it beside the refusal, as
  a warning in `--json`, and so does a name `enroll --sign-in` refuses before it opens a
  browser, from the command line or the app. With nothing interrupted, a mistyped name
  still takes no lock and changes nothing.
- Labels written by 0.1.x that contain a slash can be switched to, forgotten, renamed and
  signed in to again; they read as a tool prefix and were refused.
- The floor between two questions about one account was the minute Pitboard uses for a
  window it cannot time, not the three minutes it promises. It worked a window's length out
  from its kind and knew `five_hour` and `seven_day`, and Anthropic has been answering
  `session`, `weekly_all` and `weekly_scoped`. A window now carries its length where it is
  known: stated outright by OpenAI, derived from the kind for Anthropic. In `--json` a
  window gains `length_seconds`.
- `pitboard doctor --json` is now what the bug template says it is. The template asks people
  to paste it and promises labels, codes, paths and times with no tokens, no email addresses
  and no account identifiers; it printed the signed-in email address and organisation uuid,
  the login name, a value derived from the refresh token, and home paths carrying the
  username, and the contract snapshot could not catch it because it redacted the whole
  checks array. Identifiers are now salted digests, so two mentions of one account line up
  inside a report and two reports do not line up with each other, and paths under the home
  are shortened. `pitboard doctor` without `--json` is untouched: a person reading their own
  machine should see their own account. The contract test pins the promise rather than a
  snapshot of one machine.
- Changing Claude Code's config no longer loses what Claude Code wrote meanwhile. It was
  read, edited in memory, and a whole new file renamed over it, so anything written in
  between was silently gone from the file that holds a person's project history and MCP
  configuration. Measured on 22 September 2026 against a running session: it is rewritten
  about every forty seconds and every rewrite changes something. Claude Code takes no lock
  on it, so Pitboard checks that the bytes it parsed are still the bytes on disk and starts
  again from the new ones when they are not, and after four tries writes nothing rather than
  writing over what Claude Code just put there. What Pitboard removed from the file is
  written into `pitboard log` rather than left to be inferred from a backup.
- A renewed login's expiry is measured from Anthropic's clock rather than from this
  machine's. The lifetimes a renewal answers with are relative, so what they are added to
  decides when the login expires: on a machine running ahead, a freshly renewed park read
  as already lapsed and every `pitboard` renewed it again, rotating the refresh chain on a
  loop; on one running behind, a lapsed park looked restorable and a switch installed a
  login that could not work. Measured first, which is why there is no skew estimate here:
  this machine sat within 0.75 seconds of Anthropic across eight requests, and the `Date`
  header has a granularity of one second, so the whole spread was noise. Anchoring is the
  correction; estimating would have been machinery with nothing to correct.
- "Nothing is signed in" is no longer said when something is. If Claude Code's config names
  somebody as signed in and no store Pitboard reads holds that login, Pitboard is looking in
  the wrong place, and writing a login there would put it where nobody reads it. That is now
  its own refusal and its own failing check, with the code `live_credential_elsewhere`,
  rather than advice to sign in again. It is the failure that would follow Claude Code
  moving where it keeps a login, and the one the Linux platform has been most at risk of.
- A switch asks about the login going in, not only about the one coming out. It used to ask
  Anthropic twice about the login it was throwing away and never once about the login it was
  installing, so an account whose refresh chain had been revoked or signed out elsewhere
  installed cleanly, read back cleanly and reported a switch; the person found out the next
  time they ran `claude`, by which point the login they had left was parked and the one they
  had arrived at did not work. A park Anthropic refuses is dropped and the switch does not
  happen; one filed under the wrong account is refused by name; one whose access token has
  lapsed is renewed and the renewed login is what gets installed. Costs one round trip,
  asked before Claude Code's write lock is taken, so it does not hold up its writes.
- A switch no longer reports success it did not have. Claude Code's `/logout` deletes the
  credential with no write lock held once it has given up waiting, which is the one write
  Pitboard cannot exclude; landing just after the install, it left the incoming account
  signed out while `pitboard use` printed "Switched to work" and exited 0. The slot is now
  read back before the incoming copy is discarded, so a switch that did not hold leaves
  both logins parked and says what happened, instead of leaving neither and saying nothing.
  The new code is `switch_did_not_hold`.
- The credential write lock now notices when it stops being Pitboard's. A machine that
  sleeps mid-switch lets the lock age past its staleness window, and Claude Code reclaims
  it and writes underneath a switch that believes it still holds it. The heartbeat compares
  the directory's mtime against what it last stored and stops, marking the lock lost, and
  releasing it then leaves the directory alone rather than taking away a lock that now
  belongs to somebody else. Claude Code treats the same event as a warning and keeps
  writing, so Pitboard cannot expect the other side to stop. Measured first: APFS returns a
  mtime 18 to 60 nanoseconds from the one it was given, so a check against the value asked
  for would abandon every switch.
- A locked keychain no longer reads as a lost login. On a machine whose keychain is locked
  the write fails, the read-back that decides whether anything changed fails too, and that
  second failure was taken to mean the slot had changed: Pitboard attempted a rollback,
  that failed as well, and the person was told their login could not be put back and they
  should sign in again. Nothing had been written and it had never moved. The read-back now
  has three answers rather than two, and not knowing is one of them: nothing further is
  written, every copy is kept, and the record of intent stays so a later run with a store
  that answers finishes or undoes the switch. The new code is `switch_unverified`.

### Security

- There is no `CARGO_REGISTRY_TOKEN` any more. crates.io issues the publish job a token
  from its GitHub identity and revokes it when the job ends, so there is no standing
  credential to leak, and the exchange runs on a pre-release tag too, where a registration
  that does not match is found out before a release reaches the one step nobody can undo.
  That step now waits in a GitHub environment with required reviewers.
- The release checks the published feed against the public key in the published app bundle,
  which is the key an installed copy checks it against. It used to check it with the
  private key that signed it, in the job that signed it, so a wrong key verified against
  itself. Nothing in that job reads a secret now. A release whose update key differs from
  the last one's is refused unless a repository variable says that is what it means to do.

## [0.2.0] - 2026-09-22

Everything a stranger hits in the first ten minutes, every state a person could be stuck in,
and what the app was missing to stand on its own.

### Added

- `pitboard log` shows what Pitboard has changed and when, from the record it was already
  keeping. The log now names which front end asked.
- `pitboard uninstall` deletes every parked login and then Pitboard's own files, in that
  order, because the account list is the only index of those keychain items.
- `pitboard abandon` gives up on an interrupted switch that cannot be finished, keeping
  every login. The way out when recovery cannot reach Anthropic.
- `pitboard status --offline` answers from what was last measured, without asking Anthropic
  or touching a login. Milliseconds instead of seconds, and it works with no network.
- `forget` asks before deleting a parked login, unless `--yes` or `--json`.
- Each usage row in `--json` carries `severity`, Anthropic's own grade for that limit. A
  field added to the v1 envelope; nothing was removed or renamed.
- The app: each account's email, when each limit resets, when a parked login stops working,
  doctor's checks on demand, its own version, a mark in the menu bar, and VoiceOver labels.
  It can record the account in use, drop an account, and run Claude Code's own sign-in for
  a new one, showing what that sign-in says rather than borrowing a terminal.
- `cargo binstall pitboard` fetches the built binary instead of compiling the tree.
- The state file can be read forwards, and says which half to upgrade when it cannot.
- A release is guarded, re-runnable, and carries build provenance; the macOS command line
  binaries are signed and notarised like the app. A tag like `v0.2.0-rc1` is a pre-release:
  no crates.io, no update feed.

### Changed

- MSRV is 1.91, measured by building it, and CI builds at whatever the manifest claims.
- The app can be tested without a keychain, and is.

### Fixed

- `pitboard statusline` typed at a prompt waited for input that was never coming. It reads
  stdin only when something is piping into it.
- Offline, the account in use rendered as one with nothing parked, advising a sign-in it did
  not need.
- A switch that failed before installing anything left its journal behind, so the next
  command announced a recovery for something that never happened.
- `enroll --sign-in` checked what could refuse the enrolment only after the browser sign-in.
- The status line showed `?·?` for every account but the one in use until someone ran
  `pitboard` by hand. It now keeps the numbers Claude Code hands it.
- doctor and forget read Pitboard's record of its last switch rather than who is signed in,
  so a sign-in made with Claude Code's own `/login` made both wrong.
- Three ways a parked login could be left in the keychain with nothing naming it.
- A renewal that could not be written left the account with a login already spent.
- The keychain ceiling has its own error, saying the size, the limit, and what to do. A
  login with MCP server tokens in it is past that limit, which is not theory.
- `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN` and `CLAUDE_CODE_OAUTH_TOKEN` raise a warning
  on every change: Claude Code signs in with those, not with the login Pitboard moved.
- Columns line up in what a terminal draws, so a label in Chinese or Japanese no longer
  pushes everything after it out of line.
- One state file serves every credential slot, and what was switched to in one slot is no
  longer claimed in another.
- The menu bar showed the largest percentage of any limit, so a row scoped to one model
  read as though everything had stopped.
- Windows gets one sentence instead of a screen of type errors.

## [0.1.4] - 2026-09-22

### Fixed

- A switch no longer leaves the outgoing account's `trustedDeviceToken`, `organizationUuid`,
  `enterpriseGateway` or `designOauth` behind for the incoming account to present as its
  own. Claude Code deletes all of them with the login on logout; Pitboard now does the same,
  which is the state a logout and a fresh sign-in leave.
- Renewing a parked login whose answer carries no refresh-token lifetime keeps the deadline
  it had, as Claude Code does. Dropping it made a park that was about to lapse look as
  though it never expires, so Pitboard went on offering and renewing it.
- Pitboard refuses to act when `CLAUDE_CODE_CUSTOM_OAUTH_URL` is set. Claude Code then keeps
  its login under a different name, so Pitboard would park nothing and restore into an item
  nobody reads.

## [0.1.3] - 2026-09-22

### Added

- The menu bar panel says when an account has run out and which account has the most left,
  whether or not notifications are allowed, and asks for permission only when there is
  something to say. After a switch it counts down the time until sessions that were already
  open follow.
- A Homebrew tap: `brew install datlechin/tap/pitboard`, and `--cask` for the app.
- A release now fails if the update feed is missing or unsigned, and a job after the release
  reads the feed back the way an installed copy will.

### Changed

- Every assumption Pitboard makes about Claude Code re-checked against 2.1.278. Three
  comments described behaviour that has changed: the credential cache is a rolling window
  rather than one anchored at process start, `/logout` gives up on the write lock after 7.5
  seconds and deletes without it, and the organization fields in the config come from
  separate fetches and are usually absent.

### Removed

- Dead code, duplicated constants and a thrice-written test fixture removed.

### Fixed

- The keychain account name now falls back to the passwd entry when `USER` is not in the
  environment, which is what Claude Code does. Without it, Pitboard run from a launchd
  agent, a cron job or an app opened from Finder read a different keychain item than the
  one Claude Code writes, and reported no login where there was one.
- Renewing a parked login issued to another client now renews it as that client, instead of
  as the first-party one.
- The app's build number now counts up with its version. 0.1.2 shipped with the build
  number the template carried, which Sparkle would have read as newer than the release
  after it.

## [0.1.2] - 2026-09-22

### Added

- A macOS menu bar app: the account in use and its tightest limit in the menu bar, every
  account's limits in the panel, one click to switch, a notification when an account runs
  out, and launch at login. It calls the same core the command line does, directly.
- `status` renews a parked login whose access token has expired, so every account's usage
  is live and a parked login no longer lapses after its refresh token's lifetime. Only
  parked logins are renewed; the one signed in stays Claude Code's to renew. A login
  Anthropic refuses is dropped, with the command that signs in to it again.

## [0.1.1] - 2026-09-21

### Fixed

- `enroll --sign-in` no longer holds Pitboard's lock while the browser sign-in waits, so
  `use`, `forget` and `rename` go ahead meanwhile; a second sign-in is refused, not queued.
- What Claude Code's sign-in prints goes to stderr, so `enroll --sign-in --json` prints
  exactly one JSON line.

## [0.1.0] - 2026-09-21

First release.

### Added

- `enroll`, `use`, `forget`, `status`, `doctor` and `statusline`.
- Live usage in `status`, asked of Anthropic for every enrolled account at once, with
  which accounts can be switched to and until when.
- `enroll <label> --sign-in` on an enrolled label renews its parked login.
- `rename` changes an account's label; its parked login stays as it is.
- A switch refuses a parked login that has expired instead of installing it.
- `doctor` checks every parked login, and reports an interrupted switch.
- `--json` on every command, emitting a versioned envelope with stable error codes, also
  for a mistyped command line.
- Shell completions and a man page, generated from the command definition.
- Linux support, using a file vault for parked logins. Not yet confirmed against a
  signed-in Claude Code on Linux.
- An audit log of every change Pitboard makes.
- Schema 3: one parked login per account, with when it expires. Earlier files are refused
  rather than migrated; nothing was ever released that wrote them.

[Unreleased]: https://github.com/datlechin/pitboard/compare/v0.7.0...HEAD
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
