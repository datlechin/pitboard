# Security policy

Pitboard moves and parks the logins of Claude Code, OpenAI's Codex CLI and, on macOS, Claude
Desktop. Each grants full access to a paid account. A Claude Code or Codex login holds a
refresh token. A Codex parked login is Codex's whole `auth.json`, which can also hold an
OpenAI API key. A Claude Desktop login is Claude's own files, its session cookie among them.
Each of the app's account windows keeps a claude.ai or chatgpt.com sign-in.

## Supported versions

| Version | Supported |
| --- | --- |
| The most recent release, and the newest beta published after it | Yes |
| 0.15, the last 0.x line, for three months after 1.0.0 is published | Security fixes only |
| Earlier releases and betas | No |

Pitboard for Windows is in beta. A fix ships in the next release or beta, and a security fix
for 0.15 in a 0.15 release. To update, follow
[Update Pitboard](https://docs.usepitboard.com/install#update-pitboard).

## Report a vulnerability

Use [Pitboard's private report form](https://github.com/datlechin/pitboard/security/advisories/new).
Never open a public issue for a vulnerability.

Include:

- `pitboard --version`
- your macOS or Linux version, or on Windows the build and the system type, x64 or ARM64
- the steps that reproduce it
- the output of `pitboard doctor --json`, if it helps: it prints no token, and replaces email
  addresses and account identifiers with digests

## After you report

One person maintains Pitboard. A report is answered as early as possible, and one that could
expose a token comes before other work. Each fix is listed under Security in
[CHANGELOG.md](CHANGELOG.md).

## Scope

A report is in scope when Pitboard fails at one of the items under In scope.
[Security and privacy](https://docs.usepitboard.com/security) and
[How switching works](https://docs.usepitboard.com/concepts/switching) say how each part
works.

### In scope

- Pitboard identifies each login before it moves one, and refuses a parked login whose
  fingerprint changed.
- Before it changes a tool's login again, Pitboard finishes or undoes that tool's interrupted
  switch, and changes nothing if it cannot tell how.
- No parked login goes untracked, and the next write deletes temporary files a killed run
  left.
- A locked or unreadable keychain, or a Credential Manager Pitboard cannot reach, reads as
  unreadable, never as empty.
- On Windows, each parked login is sealed to your Windows account, never to the machine, and
  Pitboard's folders and files are open only to you, SYSTEM and Administrators.
- Pitboard keeps no copy of a login in use, and never installs one Claude Code renewed since.
- Pitboard renews a login a tool may be using only in the cases
  [What leaves your machine](https://docs.usepitboard.com/security#what-leaves-your-machine)
  lists.
- A switch takes Claude Code's locks before it writes its login, and reads every login back.
- A Codex login is moved, never copied. One whose tokens and account id disagree is refused.
- On macOS, a Claude Desktop login is moved, never copied, and only while Claude is closed.
  Pitboard reads no keychain item, decrypts no cookie and sends no request for it. On Windows
  and Linux, Pitboard reads and changes none of Claude Desktop's files.
- Pitboard identifies a Claude Desktop login by its session cookie's fingerprint. It takes
  the account Claude names in one case only, and refuses in any other doubt.
- In Claude's data folder, Pitboard deletes nothing of Claude's. Only `pitboard forget` and
  `pitboard uninstall` delete a parked Claude Desktop login or a folder set aside.
- Pitboard refuses a synced directory, and a state file written on another machine.
- Pitboard sends each login only to the service that issued it, verifies TLS, and has no
  telemetry.
- `PITBOARD_API_BASE`, for tests, sends requests only to a loopback IP address.
- Every release file is attested. See
  [Verify a download](https://docs.usepitboard.com/install/verify).
- On macOS and Linux, the install script and casks install only files matching the release's
  checksums.
- On Linux, apt, dnf and zypper install from Pitboard's repository only what its signed indices
  list, and each RPM package is signed with the repository key.
- On macOS, the app installs only an update signed with its update key or, in a Developer ID
  copy, by the same team. On Windows, it installs only an update whose installer is signed
  with its Windows update key.
- The release workflow refuses a changed update key without a declared rotation.
- An account window stays on its site and sign-in pages, and sends other web pages to your
  browser.
- An account window refuses links to other apps, Google's sign-in, the camera and the
  microphone.
- Pitboard never reads or changes a window's website data, and adds no script to a page.
- A download the site's own page did not start asks first. On macOS, WebKit quarantines
  every download.
- Forgetting an account deletes its window's data. Pitboard deletes only the stores its own
  directory recorded.
- A `pitboard://` link opens a window only once you choose an account, and never a site's
  sign-in link.
- On macOS, the Share extension is sandboxed, with no network, file or shared-folder access,
  and keeps nothing.

### Out of scope

- Another process running as you, which can read each account window's data as it can a
  browser's, and a parked Claude Desktop login as it can Claude's own. On Linux and Windows,
  it can read every parked login as Pitboard does.
- Another user with administrator access to your computer.
- Anyone who can already read your files. On Linux, file modes alone keep parked logins
  private.
- The sites' own pages and what they embed, which run in WebKit or WebView2 as they would in a
  browser.
- A page a site opens in your browser, which uses your browser's sign-in.
- A compromised Claude Code, Codex, Claude Desktop or library Pitboard uses. CI checks the
  Rust libraries for known advisories, Sparkle only for a newer release, and none of the
  Windows app's NuGet packages, WinSparkle among them.
- A changed install script piped to `sh`. Only its attestation shows where it came from.
- Signing out in a `codex` started before a switch, which revokes the login Pitboard parked.
- An unused Claude Code or Codex account's parked login, which Pitboard keeps renewing while
  the account is enrolled.
- A deleted parked login, which stays valid until it expires. Pitboard revokes no login:
  whether Anthropic accepts a revocation is not measured.
- A forged ID token on your disk. Pitboard reads a Codex login's account from it without
  checking its signature.
- On macOS, a login too large for `security`'s standard input, which goes on its argument
  line. See [Large logins on macOS](https://docs.usepitboard.com/security#large-logins-on-macos).
