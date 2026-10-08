# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- The automatic switch switches at the share as shown: a limit the menu shows at 95% counts
  as 95%, on the account in use and on an account it could go to. It compared the unrounded
  share, so a limit at 94.5% showed 95% and was not switched from. `pitboard status`,
  `pitboard watch` and the status line round a half up, as the app does, and a limit changes
  colour at 70% and at 90% as its figure shows them.

## [0.9.0] - 2026-10-08

### Added

- `pitboard doctor` warns with `fallback login` on macOS when `~/.claude/.credentials.json`
  holds a Claude Code login besides the one in the keychain, and each change to a Claude
  Code account warns with `fallback_login`, which the app shows as **Another login is left
  in a file**. A `/login` run where the keychain is locked, as over SSH, leaves that file,
  and Claude Code 2.1.294 keeps it through every later keychain write. A session that
  cannot read the keychain signs in with it, so it stayed on another account than the one
  Pitboard showed, whatever Pitboard switched to. See [Claude Code over SSH uses another
  account](https://docs.usepitboard.com/troubleshooting#claude-code-over-ssh-uses-another-account).
- Each limit says how its use compares with an even pace: what using it evenly from the
  start of its window to its reset would have used by now. `pitboard status` marks that
  place in the limit's bar, red where the limit is used faster and green where slower, and
  says `27% over pace`, `13% under pace` or `on pace` after its reset. The app's window
  marks it on the bar the same way, with the same words beside it, and the bar's help says
  what an even pace would have used. The status line puts a red `▲` or a green `▼` after the
  account in use's shares. In `--json`, each limit has a `pace` object. Within 5 points of
  even counts as on pace, and nothing is said in the first 3% of a window. See
  [Pace](https://docs.usepitboard.com/concepts/usage#pace).

### Changed

- A keychain that is locked where it cannot ask for its password stops a command with the
  error `credential_store_locked`, in place of `credential_store_unreadable`. The message is
  the same.
- How long the account in use lasts comes from its limits' paces. `weekly limit runs out in
  11h 05m at this pace` names the limit that runs out first at the rate since its window
  began, and nothing is said while every limit lasts until it resets. It was a rate taken
  across 14 days of readings, through every reset in them: on one machine it gave a weekly
  limit at 99% a day and nine hours when it had about one, and said nothing of a week used
  five times faster than even. The app says it in its window and beside the limit in its
  menu, as **weekly 73% (runs out in 11h 05m)**. An account not in use no longer says how
  long it lasts. In `--json`, `lasts` keeps its fields and is worked out this way, and for
  an account not in use it is the first reset.
- `pitboard status --json` gives each account `organization_uuid`, the Claude organisation
  its login belongs to. `account_uuid` stays the Anthropic account UUID.
- The account index is schema 5. An earlier Pitboard cannot read it once this one has, so
  update the command line and the app together. Accounts already enrolled keep their parked
  logins, readings and claude.ai windows.

### Removed

- Pitboard no longer keeps `readings/` in its directory, the readings the old estimate came
  from. The next time it reads usage it deletes the files it wrote there, and the folder
  where nothing else is in it.

### Fixed

- `pitboard doctor` and the app's **This Mac** pane say a locked keychain once, as a failed
  `credential` check, with what Claude Code sessions do until it is unlocked. Each parked
  login behind it warns `not read: the keychain is locked`. Before, every one failed and
  said to sign in to its account again, which nothing needed.
- While the login in use cannot be read, the account Pitboard last switched to, which has
  nothing parked because its login was put in use, says `login could not be read` in
  `pitboard status` and the stale code `login_unreadable`, and the app no longer marks it
  **Needs signing in again**. After a `/login` where the keychain was locked, Claude Code's
  config named another account, and this one was told to sign in again although its login
  was still in the keychain.
- One person's Claude accounts in two organisations, such as a claude.ai plan and a Team
  plan, are two accounts, each with its own label and limits. Pitboard told them apart by
  the Anthropic account alone, so the second was refused as already enrolled, a switch to it
  said the first was in use, and both showed one account's usage. Claude Code 2.1.294 tells
  logins apart by account and organisation together.

## [0.8.0] - 2026-10-08

### Added

- Pitboard can switch Claude Code by itself before the account in use runs out, so an agent
  working in a running `claude` session can go on under another of your accounts. Once any
  limit of the account in use reaches a share you choose, 95% by default, Pitboard
  switches to the enrolled account with the most room in that limit, among those that have
  used less than the share of every limit. With no such account, nothing moves. It
  switches before the limit, not at it, because sessions already running follow a switch
  within about 33 seconds. It is off until you turn it on. It never switches Codex, whose
  running sessions keep their account until restarted, and never switches back by itself.
  Each switch is the one `pitboard use` makes, and `pitboard log` records it as
  `auto-switch`. What was tried for each limit is kept in `autoswitch.json` in Pitboard's
  directory. See [Switch before an account runs
  out](https://docs.usepitboard.com/guides/automatic-switching).
- `pitboard watch` switches Claude Code that way for as long as it runs in a terminal,
  whatever the app's setting says. Control-C stops it. Nothing installs it, and it starts
  nothing that outlives it. `--at` sets the share, from 50 to 99. It reads every account
  every 5 minutes, as the app does, and decides again within 2 seconds of a status line
  recording usage. It prints a line, starting with the local time, for each switch, and
  once for each thing that stops one. `--once` decides once from the numbers Pitboard last
  read and exits. With `--json`, each event is an envelope on a line of its own, whose
  `data.event` is `watching`, `switched`, `no_room`, `skipped` or `idle`.
- The app's **Settings** > **General** has a **Before an account runs out** section, with
  **Switch Claude Code automatically**, off by default, and the share it switches at, from
  50% to 99%. The app keeps both in `app.json`, as `auto_switch` and `auto_switch_at`, and
  switches only while it is open. After a switch it posts a notification such as
  **Switched Claude Code to personal**, and the window shows the switch as one made by
  hand. When something stops a switch, or a switch it tries fails, it says why in a
  notification, once until it next switches. The **Activity** pane names such a switch
  **Automatic switch**.
- `pitboard doctor` has a `network` check, which the app's **This Mac** pane shows too. It
  says whether Pitboard's requests go out directly or through a proxy, names the proxy and
  the variable it came from, such as
  `through http://***:***@proxy.example.com:3128, from HTTPS_PROXY`, and lists the hosts
  Pitboard calls that `NO_PROXY` exempts. Neither report shows the user name or password
  in a proxy's address, and `--json` shows a digest in place of its host and port, as it
  does an organisation's name, unless the proxy is on loopback. It warns when the proxy is
  a SOCKS one, which Pitboard does not go through, in the words a request then fails with,
  unless `NO_PROXY` exempts every host Pitboard calls. It also warns when a variable holds
  no proxy address Pitboard can read, which Pitboard passes over.
- The daily renewal schedule goes through the proxy of the Pitboard that installs it.
  `pitboard schedule install`, and turning on **Renew parked logins daily** in the app,
  write `ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY` and `NO_PROXY`, in whichever case each is
  set, into the LaunchAgent's `EnvironmentVariables` or the systemd service's
  `Environment=` lines, as they write `PITBOARD_NO_ARGV`. launchd and systemd start the
  schedule without your shell's environment, so its requests went out directly, unless
  they gave every job a proxy. Install it again after your proxy changes. On macOS,
  `pitboard schedule install` refuses a variable holding a character a LaunchAgent cannot
  hold, such as the control character U+0001, and names it. `pitboard doctor` has a
  `schedule_proxy` check, which warns when the schedule's proxy differs from the one the
  run sees, with each user name and password as `***`. Where the run's proxy is a SOCKS
  one that fails its requests, it says to change that variable before installing the
  schedule again, which would give the schedule that proxy too. In the app's **This Mac**
  pane the same line is not a warning: the app runs with the variables macOS gives it, not
  your shell's, and it says that the app's switch would give the schedule the app's.

### Changed

- A Windows build of Pitboard, from crates.io or from source, now compiles, for x64 and
  ARM64 with Microsoft's toolchain. It says `Pitboard for Windows is not released yet. This
  build changes nothing.`, with the code `windows_not_released` in `--json`, and reads and
  changes nothing: only `--version`, `--help`, `completions` and `manpage` answer. Until now
  it stopped compiling with the first of those sentences. On Windows,
  `cargo binstall pitboard` finds no release to download and stops, rather than build this
  one from source. macOS and Linux are unchanged.
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
- The app keeps its own preferences, the tools you said **Not Now** to a second account for
  and whether it has opened its window on a first launch, in `app.json` in Pitboard's
  directory, so they follow `PITBOARD_HOME` as your accounts do. It kept one set in its
  macOS preferences for every Pitboard directory, so a **Not Now** said with one
  `PITBOARD_HOME` held with another. The first time it opens a directory with no `app.json`,
  it takes what its macOS preferences held, once.
- A notification that an account in use has run out is posted once for each reset of that
  limit, however often the app is quit and opened again meanwhile: what was posted is kept
  in `told.json` in Pitboard's directory. The app posted it again after every relaunch. The
  window still says it after a relaunch, as before.
- What the app says that depends only on the time, such as **used up until 14:05** in the
  menu, is worked out once a minute, so a menu opened between two minutes can say it for up
  to a minute after that time, as its bars already could. It was worked out as the menu
  opened.
- The **Activity** list names a switch **Switch**, from either front end. It named one
  **Use**, after `pitboard use`, the command the log records a switch by.
- The settings learn whether this copy of the app can turn on daily renewal, or link its
  command line, when they read the schedule or look for the command line, which they do
  each time they are shown. They asked the file system again each time they drew, so a copy
  moved meanwhile showed the new answer at once.
- The app keeps the account windows' records, the data folders it made for each Pitboard
  directory and the page each window was last on, in `windows.json` in
  `~/Library/Application Support/com.usepitboard.Pitboard`, private to you. It kept them in
  its macOS preferences, as `webStores` and `windowPages`. The first time it opens without
  `windows.json`, it takes what those held, once, and removes them once the file is there.
  Where `windows.json` is there and the app cannot read it, or cannot read records in it, as
  after a hand edit, the app deletes no window's data and never writes over the file. Each
  window then opens at its site's home with the sign-in note at each launch, and keeps its
  data.
- Building Pitboard on Windows, as `cargo install pitboard` does there, stops with
  `Pitboard for Windows is not released yet.` It said that Pitboard runs on macOS and
  Linux, and that another system needs a host of its own. Pitboard still does not build on
  Windows.
- Pitboard's facts about Claude Code and Codex say, for each of macOS, Linux and Windows,
  whether they were read there and from which build. Read from the Windows builds, x64 and
  ARM64, seven of Claude Code's 17 facts hold on 2.1.289, and nine of Codex's 16 on
  0.160.0. Three of Claude Code's are not read on Windows: two are about the keychain, and
  the Windows build's Credential Manager store calls the `Bun.secrets` the third rules out.
  Seven of each tool's facts wait on the Windows work. Where Claude Code finds its config
  file was read again, from 2.1.289 on every system. The conformance check reads both
  tools' Windows builds, and Codex's macOS build as well as its Linux one. What Pitboard
  does is unchanged.
- In `pitboard-core`, `assumptions::Platform` gains `Windows`, and `Platform::ALL`, which
  meant macOS and Linux, is gone. `assumptions::read_on` returns a `Vec<Platform>` in place
  of a `&'static [Platform]`, and `assumptions::verified_against` takes the system as well
  as the tool. `assumptions::OnSystem`, `PerSystem`, `per_system`, `on`, `verified_on` and
  `pending` are new. These change the crate's public API.
- The JSON that `--json` prints is ASCII alone, on every system. Each character outside
  ASCII, in a label or an email address for example, is written as a `\u` escape: the label
  `Đạt` is written `\u0110\u1ea1t`, and a character beyond U+FFFF as its UTF-16 surrogate
  pair. A JSON parser reads the same values as before. Pitboard wrote such characters as
  UTF-8, which a program that decodes the output in another code page read as other
  characters. This reaches every output that carries such a character, whatever your labels
  are: each `pitboard doctor --json` detail joins its parts with ` · `, and
  `pitboard statusline --json`'s `line` does too, so both now carry `\u00b7`. What Pitboard
  prints without `--json`, and the JSON files Pitboard keeps, are unchanged.
- On macOS and Linux, Pitboard refuses every command that changes something when it runs
  as root or with `sudo`, with the error code `elevated`:
  `Pitboard changes nothing when it runs as root or with sudo. Run it as yourself.` That
  includes a container or a WSL distribution whose only user is root. Pitboard tells
  `sudo` by `SUDO_UID` being set, as it is under `sudo -u` another user too, and in a
  shell, tmux or editor server started with `sudo -iu` and your own name, which passes
  `SUDO_UID` to everything it starts; `env -u SUDO_UID` runs a command without it. Pitboard
  changed things as root before: a file it made then was root's, and a keychain item might
  have been, where your own runs might not read or replace it. A refused command writes
  nothing, not even its line in `pitboard log`. `pitboard schedule uninstall` and
  `pitboard uninstall` are refused too, so a schedule or a Pitboard directory made as root
  is removed by hand, as
  [Pitboard changes nothing as root](https://docs.usepitboard.com/troubleshooting#pitboard-changes-nothing-as-root)
  says.
  - `pitboard status` still answers, with what `pitboard status --offline` shows and the
    warning `read_only`, which says why. It renews no parked login, asks Anthropic and
    OpenAI nothing and writes nothing. The status line still answers too, and writes
    nothing.
  - `pitboard doctor` fails its `elevated` check, first, with how Pitboard runs.
  - `pitboard renew --json` reports a refused run as the error `elevated`, where it gave
    an empty list, and **Renew Now** in the app says why, where it said
    `No parked login was due.`
  - `forget` and `uninstall` ask no question whose answer would be refused, and
    `enroll --sign-in` does not say it is opening a sign-in.
- In `pitboard-core`, `service::Pitboard::renew` returns a `Result`, and
  `service::Permit` and `Pitboard::permit` are new. Everything outside `service` that
  writes, removes, changes a store of logins or the scheduler, starts a tool's sign-in or
  renews a parked login takes a `Permit`, which only `Pitboard::permit` makes: among them
  `app::write_file`, `audit::record`, `schedule::install`, `status::gather`,
  `switch::settle` and `statusline::read`, which takes an `Option<Permit>` and writes
  nothing without one. `host::Elevation` is new. These change the crate's public API.
- `pitboard schedule install`, and turning on **Renew parked logins daily** in the app's
  settings, refuse while `PITBOARD_HOME` names a directory other than `~/.pitboard`, with
  the error code `schedule_not_default_home`, and install nothing. The schedule renews the
  parked logins in `~/.pitboard` alone, so it would leave those in that directory to run
  out. Both installed it all the same, and it renewed `~/.pitboard`. To turn on daily
  renewal, unset `PITBOARD_HOME`. To renew the parked logins in another directory, run
  `pitboard renew` with `PITBOARD_HOME` set. `pitboard schedule uninstall`, and turning
  the switch off in the app, still remove the schedule whatever `PITBOARD_HOME` says.
  Pitboard compares the two as paths, so a `PITBOARD_HOME` that is a link to
  `~/.pitboard` is refused too. While `PITBOARD_HOME` names another directory,
  `pitboard schedule status` adds that the schedule renews only the parked logins in
  `~/.pitboard`, and with nothing installed it says to unset `PITBOARD_HOME` before
  `pitboard schedule install`. It said to run `pitboard schedule install`, which is
  refused there.
- Pitboard refuses every SOCKS proxy: `socks4://`, `socks4a://`, `socks5://`, `socks5h://`
  and `socks://`, which is `socks5://`. Pitboard takes its proxy from the first of
  `ALL_PROXY`, `HTTPS_PROXY` and `HTTP_PROXY`, each read before its lower-case spelling,
  that holds an address it can read. While that variable names a SOCKS proxy, each request
  to Anthropic or OpenAI fails before anything is sent, with the cause `unreachable` and
  the reason `Pitboard does not use SOCKS proxies yet, and ALL_PROXY names one`, naming
  that variable. Requests with a `socks4://`, `socks5://` or `socks://` proxy no longer go
  out directly, around the proxy you set. Requests with a `socks4a://` or `socks5h://` one
  failed with `Connection refused`, and fail with that reason instead. A host `NO_PROXY`
  names is still reached directly, without the proxy's own name being looked up, and a
  redirect from it is not followed while a SOCKS proxy is named. Pitboard does not go
  through SOCKS proxies because ureq 3.4.2, the library it sends requests with, waits past
  the request's 5 seconds on one that never answers, and looks the proxy up before it asks
  `NO_PROXY`.
- Pitboard writes the daily renewal schedule's files, the LaunchAgent and the systemd
  service and timer, so that only you can read them, mode 600, whatever mode a file
  already there had. They can hold a proxy's user name and password. A file Pitboard wrote
  where none was there was already 600, and one already there kept its own mode, such as
  644.
- In `pitboard-core`, `service::Pitboard::check_homes` and
  `service::Pitboard::schedule_renews_this_home`, the errors `Error::HomeNotAbsolute` and
  `Error::ScheduleNotDefaultHome`, `Context::started_by_the_schedule` and
  `schedule::SCHEDULED_RUN` are new, and `Context::with_claude_config_dir` keeps an empty
  value, which it dropped as unset.
- Pitboard's own Codex sign-ins, the ones `pitboard enroll codex/<label> --sign-in` and the
  app's **Add Account** and **Sign In Again** run, start Codex as
  `codex -c cli_auth_credentials_store="file" login`, so Codex keeps the new login in
  `auth.json` in the private folder Pitboard reads it back from. `/etc/codex/config.toml`,
  or a trusted project's `.codex/config.toml`, could have sent it to the keychain instead.
  Only `/etc/codex/managed_config.toml`, a managed preference or a requirement is over the
  `-c`. Where one of them chooses a store other than the file, Pitboard refuses the sign-in
  before it starts, as the first entry under Fixed says; where one chooses the file, the
  sign-in goes ahead.
- `pitboard doctor`'s `Codex login store` line, and the app's **This Mac** pane, say which
  setting chose Codex's store, such as `file (Codex's default)` or
  ``keyring (pinned to `keyring` by /etc/codex/requirements.toml)``. Where the store is the
  file, `doctor` adds that Pitboard does not read a project's own `.codex/config.toml`. It
  said `file`, or, whichever file chose another store,
  `keyring (cli_auth_credentials_store in config.toml)`. `pitboard doctor --json` gives
  `unknown` as Codex's `backend` where nobody can tell, as the second entry under Fixed
  says.
- In `pitboard-core`, `doctor::CodexFacts` has two more fields, `backend_setting` and
  `backend_remedy`. This changes the crate's public API.
- In `pitboard-core`, `holder::Remedy::ReopenApp` names its app by `app`, a
  `holder::AppId`, in place of `bundle_id`. `AppId::MacBundle` holds a Mac app's bundle id,
  and `AppId::as_str` gives the id as its system writes it. `holder::AppId`,
  `service::Warning::SessionsUnknown` and `service::Warning::SessionsUnknownAfterSignIn` are
  new. These change the crate's public API.
- Pitboard reads `ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY` and `NO_PROXY`, and their
  lower-case spellings, from the environment it was started with, by the rules ureq 3.4.2
  reads them with, and gives ureq the proxy. ureq read them itself, from the process, so
  each request goes through the same HTTP or HTTPS proxy as before. In `pitboard-core`, a
  context made with `Context::new` names no proxy, where its requests went through the one
  the process's environment named; a context read from an environment, as the command
  line's and the app's are, goes through the one that environment names. `doctor::Facts`
  has the fields `network` and `in_the_app`, `doctor::ScheduleFact` has the fields
  `network` and `network_differs`, and `doctor::NetworkFact` and `doctor::ProxyFact` are
  new. These change the crate's public API.
- In `pitboard-core`, the `autoswitch` module, `service::Pitboard::auto_switch`,
  `words::share_of_limit` and `words::not_switching` are new, and `error::Error` has the
  variant `SwitchOvertaken`. These change the crate's public API.

### Fixed

- When the account in use runs out of a limit, the app's notification and window offer to
  switch only to an account with room in every limit it reports, counting a limit whose
  reset time has passed as reset. They offered the account with the most room in the limit
  that ran out, even one that had run out of another limit. An account with five-hour room
  and its weekly limit spent stopped at once when switched to.
- Where Pitboard cannot read the list of running processes, a Codex switch warns, with the
  code `sessions_unknown`, that Pitboard could not tell whether Codex sessions started
  before it are still running, and says not to sign out in one: that revokes the login
  Pitboard has just parked. It said nothing, as when nothing runs Codex. In the app, the
  switch's notice says it.
- Where Pitboard cannot read the list of running processes, a sign-in that puts a new login
  of a Codex account in use warns with `sessions_unknown` too, and says to quit and start
  again any Codex session still on the old login. Otherwise one of them can put the old
  login back when it refreshes. It said nothing. In the app, the notice that the account
  has a new login says it, and says it once where the last switch already could not tell.
- On macOS and Linux, Pitboard reads which store Codex keeps its login in from every layer
  Codex 0.160.0 reads outside a project, in Codex's order: its default, the file store;
  `/etc/codex/config.toml`; `config.toml` in Codex's home; `/etc/codex/managed_config.toml`;
  and on macOS the managed preference `config_toml_base64` of `com.openai.codex`, which a
  configuration profile forces. `/etc/codex/requirements.toml`, and on macOS the managed
  preference `requirements_toml_base64`, pin the store over all of them. Pitboard read only
  `config.toml` in Codex's home, so on a machine where another of them put Codex's login in
  the keychain or in memory, it looked for the login in an `auth.json` Codex does not keep:
  it said nothing was signed in to Codex, and a sign-in it ran left its login where Pitboard
  could not read it back. A Codex switch, enrolment or sign-in on such a machine is refused
  with `live_store_unsupported`, as one already was where Codex's own `config.toml` named
  such a store, and the message names the setting and what would choose the file store: the line
  `cli_auth_credentials_store = "file"` in your own `config.toml`, or, where a requirement,
  `managed_config.toml` or a managed preference chose it, that only an administrator can
  change it.
- Where one of those layers is there and Codex cannot read it, because it cannot be read,
  is not TOML or names a store Codex does not have, Pitboard refuses with
  `live_store_unsupported` and says which, and `pitboard doctor` says it cannot tell where
  the login is: Codex 0.160.0 does not start then. Pitboard took such a store to be the
  file. `cli_auth_credentials_store = "nonsense"` in your own `config.toml` was read as the
  file store, and is refused the same way.
- Where one of those layers has a `profile = "<name>"` line, Pitboard reads it as choosing
  the profile that a `[profiles.<name>]` table in any of them defines, as an older Codex,
  such as 0.99.0, does. A profile's table does not change the store, since neither Codex
  0.99.0 nor 0.160.0 reads `cli_auth_credentials_store` there, so the store is still the
  one the layers choose. A line that names a profile no table defines is refused with
  `live_store_unsupported`, as 0.99.0 refuses it; Pitboard ignored the line. Codex 0.160.0
  does not start with such a line at all, whatever it names: it chooses a profile only
  with `--profile`.
- On macOS and Linux, Pitboard no longer refuses a Codex account where your own
  `config.toml` sets `cli_auth_credentials_store` to `keyring`, `auto` or `ephemeral` and
  `/etc/codex/managed_config.toml`, `/etc/codex/requirements.toml` or, on macOS, a managed
  preference sets it to `file`. Codex 0.160.0 takes their setting over yours and keeps its
  login in `auth.json`, so Pitboard parks and switches it there. Pitboard read only your
  own `config.toml`, and refused such an account with `live_store_unsupported`.
- An account's window no longer opens again on a sign-in page of its site, such as
  chatgpt.com's `/api/auth`, which would repeat a sign-in that has ended: it opens on the
  page it showed before, or at the site's home. A window closed, or Pitboard quit, while it
  was on such a page opened there again.
- After Claude Code refuses a code you paste, the sign-in sheet says **Claude Code didn’t
  take that code. Copy the whole code your browser shows, and paste it again.** and offers
  the code field again, and the same sign-in takes the next code. The sheet asked for a code
  once, so a refused one left a sign-in that only **Cancel** could end.
- In the app, **Sign In** pressed right after **Cancel** no longer says another
  `pitboard enroll --sign-in` is already waiting: the new sign-in starts once the one
  cancelled has stopped. 0.7.0 stopped the cancelled sign-in in the background and started
  the new one at once, which could find the old one still holding the lock that allows one
  sign-in at a time.
- After you add, name, rename or forget an account, give up on an interrupted switch or
  press **Renew Now**, the app shows what it reads of your accounts right after. When its
  look for changes made in a terminal found the account index just as the app had written
  it, the app took its own change for one made elsewhere and threw that read away: in an
  open window, an account just added showed no numbers, and nothing said when the accounts
  were read, until you opened the menu or went back to **Accounts**, either of which reads
  at once, or the read the app makes every five minutes came. 0.7.0 did the same. A look
  that found a rename before the rename had finished also put away what that account's
  last switch said, and could notify you again that the account had run out.
- A sign-in that fails after its sheet has closed is said in the window. It was said
  nowhere.
- A name the **Name** or **Rename** sheet could not save, once the sheet had closed or
  another had taken its place, is said in the window. It was said nowhere.
- Quitting Pitboard stops a sign-in under way, and the tool's own sign-in with it, before the
  app goes. Pitboard left a `claude` or `codex` that was signing in running after it quit.
- Two warnings with the same words are two notices in the window. They shared one id, which
  a SwiftUI list does not allow, so what the window drew of them was not defined.
- **This Mac** lists the check of each account's parked login as a row of its own. Every
  Claude Code account's check shared one id, and so did every Codex account's, which a
  SwiftUI list does not allow, so what the pane drew of them was not defined.
- **This Mac** says it is checking until the last check asked for is in. With two under
  way, after you left **This Mac** and came back while a check ran, it showed **Checked
  at** once the first was in, beside checks still to come.
- After an interrupted switch Pitboard cannot finish, the app shows **An interrupted switch
  is waiting**, with its **Give Up** button, each time it reads your accounts, and
  `pitboard status` warns with `recovery_undetermined` in the words a change stops with.
  Neither said so before. The app looks for it in what a read says, and no read said it, so
  the notice never appeared and a switch failed with nothing offered but **OK**. Reading
  finishes and changes nothing. `pitboard status --offline` and `pitboard doctor` give the
  same message unless only Anthropic can tell whose login Claude Code is using. The app
  puts the notice away once the switch is given up on or finished in a terminal.
- `pitboard abandon`, and the app's **Give Up**, give up on an interrupted Codex switch
  while `CLAUDE_CODE_CUSTOM_OAUTH_URL` is set, as a change to a Codex account goes ahead
  with it set. Both refused, so a Codex switch nothing could finish could not be given up
  on either.
- `pitboard doctor`, and the app's checks, no longer warn that a parked login "could not be
  read this time" after a switch, a sign-in that enrolled an account, or a renewal of a
  parked login. Pitboard writes a park's name down before it writes the login, and the name
  stayed on that list until the next change, so doctor counted a login it had already
  recorded.
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
- With `HOME` or `PITBOARD_HOME` empty, or with it, `CLAUDE_CONFIG_DIR`,
  `CLAUDE_SECURESTORAGE_CONFIG_DIR` or `CODEX_HOME` set to a path that does not start at
  the root, such as `pitboard`, Pitboard refuses every command but `completions`,
  `manpage` and `doctor` with the error code `home_not_absolute`, which names the variable:
  `HOME is empty, so the folder it names would depend on where each program runs. Set it
  to a full path, or unset it; Pitboard reads and changes nothing until then.` Pitboard
  took such a path to be under the folder it was run from: an empty `HOME` or
  `PITBOARD_HOME` kept its files there, and an empty `HOME` had
  `pitboard schedule install` write the schedule's file there. Claude Code and Codex take a
  relative one from the folder each of them runs in, so the login Pitboard read and
  switched was the one under the folder Pitboard ran in. `pitboard doctor` fails a `homes`
  check instead, and checks nothing else; the app's **This Mac** pane shows the same. The
  app reads nothing under such a home either. It shows the error where it shows your
  accounts, finds no tool and no `pitboard`, and leaves its own files there unread and
  unwritten.
- A run of the daily renewal schedule renews the parked logins in `~/.pitboard` whatever
  `PITBOARD_HOME` says, empty or relative included, which other commands refuse as above.
  The schedule renews `~/.pitboard` alone, and its job is written without
  `PITBOARD_HOME`, but a `PITBOARD_HOME` set with `launchctl setenv` on macOS, or in
  systemd's user environment on Linux, through `systemctl --user set-environment` or
  `environment.d`, reached the job. The job then renewed that directory's parked logins
  and left those in `~/.pitboard` to run out. On macOS Pitboard tells a run of the
  schedule by the job's name, which launchd gives it in `XPC_SERVICE_NAME`. On Linux it
  does not rely on what systemd passes a job, which systemd itself calls best effort, so
  the timer's service runs `pitboard renew --scheduled`. A timer installed by an earlier
  Pitboard runs `pitboard renew`, which renews the directory `PITBOARD_HOME` names as
  before, until you run `pitboard schedule install` again, which rewrites it. If you go
  back to an earlier Pitboard on Linux, run its `pitboard schedule install` as well: an
  earlier Pitboard does not take `--scheduled`, so until then the timer fails every day.
- With `CLAUDE_CONFIG_DIR` set but empty, Pitboard refuses as above, with
  `home_not_absolute`: `CLAUDE_CONFIG_DIR is empty, so the folder it names would depend on
  where each program runs.` Claude Code reads an empty one as unset for `~/.claude.json`
  and for which keychain item holds its login, but as the folder it runs in for its
  config directory, so its settings, the lock around its login and, on Linux, the login in
  `.credentials.json` are in whatever folder each session started in. Pitboard read an
  empty one as unset, so it took that lock, and on Linux switched that login, in
  `~/.claude`, where no Claude Code started with it looks. On macOS such a switch did move
  the login, which is in the default keychain item, and took only the lock in the wrong
  folder; it is refused all the same. To keep switching, unset `CLAUDE_CONFIG_DIR`.
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

- Pitboard refuses more folders that sync to other machines as its directory, with
  `state_on_synced_drive`: any folder inside `~/Library/CloudStorage`, where macOS keeps
  the folders of cloud storage apps such as Box and Google Drive, or inside
  `~/Library/Mobile Documents`, where iCloud Drive keeps each app's folder; and any folder
  named `Dropbox`, `Dropbox (<team>)`, `Google Drive`, `GoogleDrive-<account>`,
  `OneDrive`, `OneDrive - <organisation>`, `OneDrive-<kind>`, `com~apple~CloudDocs` or
  `Sync`. A name counts in any case, so `~/dropbox` is refused as `~/Dropbox` was, and
  wherever it is in the path, your home's own folders included: with a home of
  `/home/sync` or `/home/dropbox`, `~/.pitboard` is refused though nothing syncs it, and
  `PITBOARD_HOME` can name a folder outside it. On Linux the parked logins are files in
  that directory, which such a folder would copy to every machine it syncs to. Every
  folder Pitboard refused before is still refused, `SyncAdmin` and `OneDriveTools` among
  them, though they sync nothing.
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

[Unreleased]: https://github.com/datlechin/pitboard/compare/v0.9.0...HEAD
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
