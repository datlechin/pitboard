# Not measured

What nobody has measured about Claude Code, Codex and Claude Desktop, and about watching
Pitboard's directory, and what Pitboard does where it cannot know. What was read or measured
is in each tool's register, `crates/pitboard-core/src/provider/<tool>/assumptions.rs`, and in
[The app model](app-model.md#watching-pitboards-directory).

## Claude Code

Switching by itself rests on the session cache's 33 seconds, measured on 2.1.278
(`credential_cache`), and, while a file sits behind the keychain, on a reading of 2.1.294 that
no running session has been measured against (`fallback_file_pins_session_login`). These were
not measured, and the register cannot hold them, since every fact in it is read from a build:

- Whether a request a session has under way at the moment of a switch finishes, on either
  account.
- What Claude Code does when an account reaches a limit: whether a session stops, waits or
  tries again, and whether it would take a switch made after that. Pitboard switches at a
  share under 100% so that a session following within the 33 seconds need not meet it,
  and the docs say neither is known.
- A busy session renewing its login while an automatic switch is under way, against a
  running build. The refresh lock a switch holds (`refresh_lock`) keeps a renewal from
  starting while the outgoing login is parked, and the read again under the write lock
  (`write_lock`) finds one that finished before it, as for any switch.
  `a_session_renewing_the_outgoing_login_during_a_switch_waits_for_it` in `switch/mod.rs`
  plays the first on the stores in memory, with a session that takes the refresh lock as
  2.1.294 does. `a_login_claude_code_renews_partway_through_is_not_written_over` in
  `switch/auto.rs` plays a login that changed under the switch all the same: the switch
  stops with `signed_in_account_changed`, nothing is written over the renewed login, the
  account it would have switched to keeps its parked login, and the next attempt, a minute
  on, switches.
- Whether exchanging a parked refresh token, to see whether it still works, revokes the copy
  another machine holds. So `pitboard adopt` drops each login parked in a store, and asks
  Anthropic nothing.
- Whether a switch stops a Remote Control session. Claude Code's changelog for 2.1.234 says
  signing the computer in to another claude.ai account or organisation stops one within
  seconds. Pitboard says nothing about Remote Control.
- Whether the skills and plugins a session syncs from claude.ai, from 2.1.275, change in an
  open session after a switch. Pitboard moves the login alone.
- Whether a renewal's answer names `refresh_token_expires_in`, and so moves the end of a parked
  login (`refresh_token_end`). Pitboard keeps the date a login has until an answer names one,
  as Claude Code does, and asks for a sign-in within 3 days of it.
- The 33 seconds on a build after 2.1.278, and on Linux at all. `credential_cache` is read on
  both systems and was measured on macOS with 2.1.278 alone: no Linux session with real
  logins has been run. Pitboard gives 33 seconds on every system.

## Codex

- Where an editor's Codex extension runs Codex from. Pitboard takes a folder named
  `openai.chatgpt-<version>`, the extension's packaged layout, for one: no editor with the
  extension was running on the Mac measured on 1 October 2026.
- When a Codex limit's period starts. Pitboard warms up Claude Code accounts alone.
- Whether an `OPENAI_API_KEY` in the environment wins over a ChatGPT login. Pitboard names no
  override for Codex.
- Whether the ChatGPT app's WSL mode hands its Linux codex the Windows Codex home. So doctor
  on Windows says a switch changes the login of a Codex in WSL that uses that home, of no
  distribution in particular.

## Claude Desktop

These were not measured. Each row says what Pitboard does where it cannot know.

| Not measured | What Pitboard does |
| --- | --- |
| Whether a sign-in over a stale `lastKnownAccountUuid`, followed by a quit within seconds, leaves the new session under the old name | A session it has not seen is taken as named in one case and is never deleted. Beside token caches that all hold the emptied value it is a sign-in not finished, for every reader |
| How often Claude replaces a session in place after a login is put back (seen once) | It parks the session under the fingerprint found and says so. Nothing is deleted |
| What the names a build Pitboard has not read makes hold | They stay, as Claude keeps them when another account signs in. Only a part of a store the move carries that no item names stops the start of a move, and names itself |
| Whether each account keeps its own previews, connector sign-ins and session windows across a switch, and Claude makes a missing `Partitions` or `Service Worker` again (read in 2.42304.0, not run) | They move like the site's storage. An item in neither place is gone on without, and said |
| Whether 2.19675.0 and the builds people run write `config.json.journal` in 2.7032.0's format | A journal valid by its own hashes is the settings. A stale one is ignored. A header that does not parse refuses every change |
| Whether a stale journal Pitboard left in place can become valid again | Only where a later write makes `config.json` the file it was written over, and then its body is that run's settings |
| Whether the builds people run keep the five cookie names, `meta.version` 24 and `v10` | Signed out needs none of the five names and emptied caches. Another shape refuses |
| Whether Claude ever keeps a `sessionKey` for both `.claude.ai` and `claude.ai` | Refused, whatever the values: one session on both hosts is not told from two |
| Whether the emptied token cache is 28 characters on the builds people run | A cache of another length is not emptied, so the folder is never read as signed out |
| Whether the token caches change while an account is in use | Keys are never written over keys. A parked copy that differs goes aside |
| Whether a session with none of its three keys opens as its account and writes them again (read in 2.7032.0, not run) | Reached only by a kill between the keys and the jar. The next change reads again |
| What Claude does on a folder holding parts of two accounts | No step leaves one. A change stops at any session it did not record |
| Whether Claude makes a missing `Local Storage`, `Session Storage`, `IndexedDB` or `WebStorage` again | An item in neither place is gone on without, and said. The jar never is |
| Whether a session cookie's expiry slides, and how long a parked session lives | `session_expires_at` is a note in days, never a refusal |
| Whether an update, a reinstall or a keychain reset keeps `Claude Safe Storage` | Not checked: it would be a keychain call. A park from another Mac is refused by its manifest |
| How Claude's own account switcher keeps accounts on disk | Two rows of one session name for one host refuse. The recorded session under another name is doubt |
| Whether helpers outlive a crashed main program | Every program inside Claude's bundle counts as Claude open, whatever the bundle is named |
| Whether the updater's helper runs from outside the bundle between a quit and a relaunch | A program inside Claude's bundle or the data folder counts. So does one inside a `Claude.app` whose identifier does not read, and Claude's main program or a `Claude Helper` inside any bundle whose identifier does not read. One copied elsewhere does not |
| Whether the Claude Code under `claude-code/`, MCP servers or the Cowork VM outlive the main program | A program inside the data folder counts. The others do not |
| How long a quit takes, and a quit Claude's own question refuses | The core never waits for a quit. It refuses while Claude is listed |
| Whether a lock on a leveldb `LOCK` names the app, and whether inodes survive a launch | Neither is used |
| Whether Chromium tolerates a second reader of `Cookies` while it runs | The jar is opened only under `state.lock`, with Claude seen closed |
| Whether a read-only SQLite open of the jar writes anything beside it | Tested on a scratch jar on macOS: the folder's listing is the same after the read |
| Where the data folder is when `HOME` is not the passwd home | The tool is absent there |
| Whether the HTTP `Cache` can serve one account's answer to another | Not moved, not claimed |
| Whether `renamex_np(RENAME_EXCL)` is refused on a volume a home can be on | A refusal, with nothing changed. Never a rename that replaces |
| Two Pitboard homes changing one data folder at the same moment | Every change holds an advisory lock on the data folder, never waited for. The second is refused |
| Where each macOS build is published | No scheduled conformance run. `doctor` reports the build and gates nothing |
| Anything on Linux or Windows | Refused. Every register line is `NotRead` there |
| How many `in_the_way` folders a person gathers | Each is kept until `uninstall`. None holds a login |
| Whether Claude takes an advisory lock on its own data folder | Pitboard's lock is taken only once Claude is seen closed, and a lock held by anything refuses |
| What **Log out** does to other sessions of the same claude.ai account | No sentence claims it |

A person makes each hand measurement, M1 to M11, on a macOS user account made for it, which
holds no real login, with two throwaway claude.ai accounts. None is automated: no test or
script starts, quits or writes Claude. Each result goes into the register with its build
and date, or stays in the table of what was not measured, with what was seen.

| | Measures | Due |
| --- | --- | --- |
| M1 | The first-level names of the build people run, listed after Chat, the Code tab and Cowork were each used once, and again while Claude runs | Before a Claude Desktop login is moved |
| M2 | Whether `config.json.journal` is there after a quit and after a force quit, and which field each hash equals | Before a Claude Desktop login is moved |
| M3 | The cookie names and hosts, the first three bytes of each value, `meta`'s versions, and the token caches' lengths after a **Log out** | Before a Claude Desktop login is moved |
| M4 | The session fingerprint and the three keys' digests before and after ten put-backs across two days | Before a Claude Desktop login is moved |
| M5 | A sign-in over a stale name with a quit at 5, 10, 20, 40 and 70 seconds: whose name the session is under | Before a release |
| M6 | A quit with a task running: whether Claude asks, and how long each process stays listed | Before a release |
| M7 | Which processes are listed, each 200 milliseconds for 30 seconds after a quit, after a staged update, and for 35 minutes after a force quit | Before a release |
| M8 | An update, and a reinstall, between a park and its put-back: whether the account opens signed in | Before a release |
| M9 | Whether Claude makes a missing `Local Storage` again, and whether a login without its three keys opens signed in and writes them | Before a release |
| M10 | Claude's own switcher, where the account shows it: the session rows and `lastKnownAccountUuid` after an add and after a switch | Before a release |
| M11 | A switch each way with a preview signed in to a site and, under a Microsoft 365 policy, the connector signed in: whether each account finds its own, and whether Claude makes a missing `Partitions` and `Service Worker` again | Before a release |

## Watching Pitboard's directory

| Not measured | What Pitboard does |
| --- | --- |
| ReadDirectoryChangesW's events for the core's replace of `state.json`, and how soon they come | Takes a change by the file's name, as on macOS and Linux. `a_write_to_the_index_is_seen_as_it_lands` measures both on CI's Windows legs |
| Whether `pitboard uninstall` removes a folder ReadDirectoryChangesW watches whole, by the time it returns, on NTFS and on the exFAT volume CI's `windows` job mounts | The app stops its watch, and has let go of the folder, before it uninstalls. `a_watched_pitboard_directory_is_removed_whole_by_uninstall` measures the command line's on CI's Windows legs |
| What an overflow of ReadDirectoryChangesW's buffer comes to through notify 8.2.1 | Where the watch ends, it watches again and its caller looks. Where nothing is said, a write in it goes unheard until the next write, the next read, or, for `pitboard watch`, the next decision |
| inotify on btrfs, which needs a mount Docker gives only a privileged container | It watches a folder on btrfs as any other |
| Whether FSEvents hears a write another machine makes on a share, or on a volume autofs mounts | It watches only a volume statfs marks `MNT_LOCAL`, and looks every 2 seconds on any other |

## Linux

These were not measured. Each says what Pitboard does where it cannot know.

- A running user manager. `host/linux/systemd.rs` was tested against a stand-in served on
  a private socket in a test's own folder, never against systemd: whether systemd's private
  socket takes zbus 5.19's peer to peer handshake, `UnitPath` and `Environment` as a running
  manager answers them, a refused start and its restore, and `uninstall`. Until a run on a machine with systemd measures them, Pitboard does what systemd
  262's source says ([linux.md](linux.md#the-users-systemd)).
- The watch service's `PATH`. A user manager gives its services `default_user_PATH()`, the
  system's own `bin` folders (`src/basic/path-util.h` at `v262`), and `environment.d` can add
  to it. Whether that `PATH` finds `claude` and `codex` where people install them, such as
  `~/.local/bin`, is not measured. The watch service looks for each tool only on the `PATH` it
  was given.
