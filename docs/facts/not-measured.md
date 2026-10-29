# Not measured

What nobody has measured about Claude Code, Codex, Claude Desktop, watching Pitboard's
directory, the Linux app, its checks and its packages, and what Pitboard does where it cannot
know. What was read or measured is in each tool's register,
`crates/pitboard-core/src/provider/<tool>/assumptions.rs`, and in
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
- The 33 seconds on macOS on a build after 2.1.278: `credential_cache` was measured there with
  2.1.278 alone. On Linux 2.1.314 opens its login file again at the next request once it
  changed ([The Linux session machines](linux-session.md#claude-code-and-codex)). Pitboard gives
  33 seconds on every system.

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

- Parts of a running user manager on the session bus. The Linux session machines installed,
  started and removed the units against systemd 257 and 259
  ([The Linux session machines](linux-session.md#renewal-and-the-watch-service)). Tested only
  against a stand-in on a bus a test started: how soon the manager owns
  `org.freedesktop.systemd1` on a session bus socket activation has just started, a refused start
  and its restore, a mask made before an install, and a service skipped by its condition. `linux-facts.yml`'s `user-manager` job measures each on
  GitHub's Ubuntu 26.04 runners, with a throwaway user's lingering manager. Until a run is read,
  Pitboard does what systemd 262's source says, and waits 5 seconds for the manager's name.
- A timer across suspend: whether a calendar timer that came due while the computer slept runs
  once it wakes, as systemd.timer(5) says, and how late `RandomizedDelaySec=` makes that run.
  Renewal renews only what is due, so a run late or one more changes nothing else.
- The watch service's `PATH`. A user manager gives its services `default_user_PATH()`, the
  system's own `bin` folders (`src/basic/path-util.h` at `v262`), and `environment.d` can add
  to it. `linux-facts.yml`'s `user-manager` job reads the `PATH` a lingering manager gives the
  watch service on Ubuntu 26.04. Whether that `PATH` finds `claude` and `codex` where people
  install them, such as `~/.local/bin`, is not measured. The watch service looks for each tool
  only on the `PATH` it was given.
- The Secret Service of oo7-daemon and KeePassXC's: whether each names a default collection and
  replaces an item by its attributes, and what each answers locked. gnome-keyring 48 and 50 and
  KDE's ksecretd were measured ([The Linux session machines](linux-session.md#sign-in-and-the-secret-service)). Pitboard asks each what the specification says,
  and parks only where `default` names a collection
  ([linux.md](linux.md#the-secret-service)).
- A desktop session's keyring reached from a terminal, and over SSH while the same person is
  signed in to the desktop: whether the session's bus is found there and the login keyring is
  unlocked. A home made where none answers keeps files.
- A real desktop's unlock prompt: whether gcr's prompter, GNOME Shell's or KDE's shows it to a
  run a person started from a terminal in the desktop session. A run over SSH or with no display
  never asks ([linux.md](linux.md#the-secret-service)), and one with a display it is not in front
  of, as in a terminal multiplexer started at the desktop and attached over SSH, waits until the
  prompt is answered or the run is stopped.
- A renewal of a lingering user manager, before the first sign-in after boot, with a parked
  login due. The keyring was locked there and `doctor` said so with no prompt, and with nothing
  due no run was refused ([The Linux session machines](linux-session.md#renewal-and-the-watch-service)).
  A run the timer started never asks, and is refused with `credential_store_locked`, which it
  keeps in `renewal.json` for doctor to say.
- Whether the login collection is unlocked when `graphical-session.target` starts on GNOME 51
  with oo7-daemon, and on Plasma with ksecretd and `plasma-kwallet-pam.service`, which nothing
  orders before the target ([linux.md](linux.md#the-graphical-sessions-target)). On GNOME
  gnome-keyring's was unlocked 3.3 s before it
  ([The Linux session machines](linux-session.md#sign-in-and-the-secret-service)). Renewal runs
  then as any scheduled run does: refused, it keeps the refusal, and the next run tries again.
- How `/proc` names npm's `claude` and `codex` on x86_64 and on GitHub's runners:
  `linux-facts.yml`'s `installs` job starts the stand-in in the place of each program, as Docker
  on the Mac did on arm64 ([linux.md](linux.md#the-tools-processes)). Running, they were
  `claude` and `codex` on the Linux session machines' arm64. Pitboard finds a tool's process by
  its `comm` alone.
- The process names of the programs Homebrew's Linux casks install, which no script installs
  ([linux.md](linux.md#homebrew-on-linux)). Each cask links its program into
  `$HOMEBREW_PREFIX/bin` under the tool's own name.
- inotify on ext4, btrfs and NFS. `linux-facts.yml`'s `inotify` job watches Pitboard's save of
  `state.json` on the runner's own disk, a loop-mounted btrfs and an NFS export mounted on the
  loopback, where a save in the exported folder stands for another machine's. Only a container's
  tmpfs was measured ([linux.md](linux.md#inotify)). 9p, which WSL's Windows drives use, is
  measured nowhere: no runner mounts one.
- Headless mutter on GitHub's runners, x86_64 and arm64, in Ubuntu 26.04's image:
  `linux-facts.yml`'s `display` job measures what Docker on the Mac measured
  ([linux.md](linux.md#the-headless-display)).

## The Linux app's checks

Every reading in [the checks' images](linux-app.md#the-checks-images) is arm64, in Docker on a
Mac. None of CI's Linux app jobs has run yet. Not measured:

- A container job on `ubuntu-26.04-arm`, and headless mutter as a fresh user in a container
  on GitHub's runners. Each job fails where either does not start.
- Any x86_64 image.
- How long each job takes with no cache, and how fast snapshot.ubuntu.com answers a runner,
  whose stalls from the Mac lasted minutes. Only the jobs' timeouts bound them.
- `in-container.sh` under podman: only docker has run it.

## The Linux app on a desktop

[The Linux session machines](linux-session.md) measured the app in Fedora's GNOME and Plasma,
Ubuntu's GNOME with the AppIndicator extension and Debian's GNOME. Not measured:

- Where Plasma starts an autostart entry without systemd. The app writes `TryExec` either way.
- The environment an autostart entry starts the app with, and so whether a debug build started
  from one would run on the real homes (M10's autostart half). A debug build has no launcher,
  D-Bus service or autostart entry, and quits outside a fixture.
- Whether the startup notification ends when the app opens its window from a snapshot after
  GApplication's `activate` returned. The window came to the front each time.
- A click on a notice's button, in GNOME Shell or in Plasma. A click on the notice itself
  reached the app with its argument.
- `PrepareForSleep` from a real logind across a suspend: the guests' suspends failed. The app
  tells the model a wake only on `false` from the owner of `org.freedesktop.login1`.
- `Activate` from the AppIndicator extension, which it calls on a double click. Plasma's was
  measured.
- Whether Plasma and the AppIndicator extension draw the status icons in the panel's text
  colour, through KDE's `ColorScheme-Text` class and GTK's symbolic recolouring.
- Account windows on Linux, which the app has none of until WebKitGTK 2.56: whether claude.ai
  and chatgpt.com load and sign in there, whether stores stay apart and let go of their files,
  and in which order `decide-policy` and `create` come (M8); and whether Ubuntu's AppArmor rule
  on user namespaces lets WebKit's bwrap sandbox run for a packaged app, or the package needs a
  profile of its own (M13). [Account windows](linux-app.md#account-windows) has what was read.
- The Plasma widget in the system tray, and its package's install: the session machines ran it
  from `apps/linux/plasmoid` in a panel and on the desktop
  ([The Plasma widget](linux-app.md#the-plasma-widget)).
- Whether the app runs on openSUSE Tumbleweed, which no session machine is. Its rpm installs
  there, zypper removes it ([The RPM packages](linux-packages.md#the-rpm-packages)), and
  Uninstall names zypper there ([GNOME's words](linux-app.md#gnomes-words)).

## The Linux packages

Every build and install in [the packages' facts](linux-packages.md) is arm64, in Docker on a
Mac. Not measured:

- An x86_64 build or install of either format. rpmlint's verdict on the x86_64 command line was
  read from an rpm `rpmbuild --target x86_64` made of the released binary.
## The Linux repository

Every build, signature and setup in [the repository's facts](linux-packages.md#the-repository) is
arm64, in Docker on a Mac, with keys a test made. Not measured:

- `test-linux-repository.sh` on x86_64, under podman, which it prefers and GitHub's Ubuntu
  images carry, or as CI's `linux-packaging-repository` job, which runs only once
  `linux-packaging` passes.
- Whether GNOME Software and Plasma Discover list Pitboard, with its icon, from the catalog: on
  Debian 13 and Ubuntu 26.04, where gnome-software does not bring apt-config-icons and Discover
  does, on Fedora through PackageKit's dnf backend, and on Tumbleweed. The Linux session machines
  measure it once a release has written the repository.
- What dnf and zypper do once the first binding of an extended signing subkey expires, which no
  test reaches before its date. Both kept the copy they first imported once the extended key
  signed, and `test-linux-repository.sh` prints its expiry. libdnf5 5.2.18.0's expired-pgp-keys
  plugin offers to remove an expired key before a transaction, so that `gpgkey=` imports it
  again, and libzypp's changelog for 17.25.4 says "Fix update of gpg keys with elongated expire
  date (bsc#1179222)"; neither has run on this key.
- What apt, dnf5 and zypper do with indices and rpms a new signing subkey signs, once
  RELEASING.md's Replace the signing subkey revokes the old one, and how each takes the key with
  the new subkey when it is fetched again. `test-linux-repository.sh` measured only indices
  another key signed: apt and dnf5 kept the ones they had, and zypper refused them.
- The real key's signatures, and `packages.usepitboard.com` serving the tree: no release has
  written it. Until the first one does:
  - which `Cache-Control` the zone honours for R2 behind `packages.usepitboard.com`, whether it
    caches a file without an extension, `InRelease` and `Packages` among them, and how soon a
    `no-cache` index that changed is served;
  - whether R2 takes AWS CLI 2.38.1's upload, `aws-chunked` with its
    `x-amz-checksum-crc64nvme` trailer ([the release](linux-packages.md#r2-and-the-aws-cli)),
    which R2's S3 page neither lists nor refuses, and whether it then serves each object
    without that `Content-Encoding`.
- The install docs' commands on x86_64, on a desktop install rather than a container image, and
  against `packages.usepitboard.com` with the real key's signatures
  ([the install docs](linux-packages.md#the-install-docs)). Among them whether `curl` and `gpg`
  come with a desktop install of Debian 13 and Ubuntu 26.04, and how long zypper keeps indices
  before `zypper dup` fetches them again.
- Every Linux job of `release.yml`: none has run, and their steps ran on the Mac alone
  ([the release](linux-packages.md#the-release)). Among them a `docker://` step in a job that
  runs in a container, which the upload is, and the rpm build of an rc.
