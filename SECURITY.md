# Security policy

pitboard handles the OAuth refresh tokens of Claude Code and Codex, which grant full access
to a paid account. A Codex parked login is Codex's whole `auth.json`, which can also hold an
OpenAI API key.

To report a vulnerability, follow [Report a vulnerability](#report-a-vulnerability), not a
public issue. For where parked logins are kept and what leaves your machine, see
[Security and privacy](https://docs.usepitboard.com/security).

## Supported versions

Security fixes go into the next release. Earlier releases do not get fixes, so update to
the most recent one.

For how to update, see [Update pitboard](https://docs.usepitboard.com/install#update-pitboard).

## Report a vulnerability

Use
[pitboard's private report form](https://github.com/datlechin/pitboard/security/advisories/new),
not a public issue.

Include:

- pitboard's version, from `pitboard --version`
- your macOS or Linux version
- the steps to reproduce it
- the output of `pitboard doctor --json`, where it helps

`pitboard doctor --json` prints no token, and shows email addresses and account identifiers
as digests.

One person maintains pitboard. Reports are answered as early as possible, and anything
that could expose a token comes before other work. Each fix is listed under Security in
[CHANGELOG.md](CHANGELOG.md).

## What pitboard protects against

A report is in scope when pitboard fails at one of these.

- pitboard does not file a login under the wrong account. It asks Anthropic which account a
  Claude Code login belongs to, because Claude Code's config can be a day out of date. A
  Codex login names its ChatGPT account and user in its ID token, so two people in one Team
  or Business workspace are two accounts. Each parked login is bound to its refresh token's
  fingerprint, and a mismatch is refused.
- A switch interrupted by a crash is finished or undone. It records its intent before it
  moves any login. The next command that changes something settles it first, and changes
  nothing if it cannot tell what happened.
- No parked login is left where nothing can find it. pitboard writes down each name before
  writing a login into it. The next change returns any login a killed run left under such a
  name to its account, or deletes it. An unreadable login is tried again later, and so is a
  failed delete. Temporary files a killed run left, which can hold a whole login, are
  removed by the next write to their directory.
- pitboard reports a locked or unreadable keychain as unreadable, never as empty. Taking it
  as empty could make pitboard park or overwrite the wrong login.
- pitboard never installs a login Claude Code has since renewed, because presenting a
  replaced refresh token makes Claude Code discard the login. It deletes a parked login
  once installed, keeps no copy of a login in use and refuses an expired one. It renews
  only a parked login, which it alone holds, because a second renewer would break a login.
  It stores the renewed login before deleting the old one, under its lock.
- pitboard takes the lock Claude Code takes around each write of its login. As read in
  Claude Code 2.1.284, sessions and the supervisor daemon wait for that lock and read the
  login again inside it. So neither can write an older account back over a switch. A
  `/logout` that gave up waiting deletes the login without the lock, so pitboard reads the
  login back after each switch.
- pitboard never keeps two usable copies of one Codex login. `codex login` and
  `codex logout` ask OpenAI to revoke the stored refresh token, so a copy would end with
  your next sign-in or sign-out. A Codex switch moves the outgoing login into pitboard's
  store and reads it back before writing the incoming one. Recovery, `abandon` and
  `repair` keep no such copy.
- pitboard reads `auth.json` back after a Codex switch, because Codex takes no lock on it.
  A `codex` refreshing its token during a switch writes its old account's tokens under the
  incoming account's id. pitboard refuses a login whose tokens and account id disagree.
- A parked login stays on one machine, because a refresh token presented from a second
  machine can end the login on both. pitboard refuses a state directory in a cloud-synced
  folder, or a state file written on another machine. `pitboard adopt` takes over such a
  file, keeping the accounts and dropping their parked logins.
- pitboard sends a login only to the service that issued it, verifies TLS against your
  operating system's trust store and has no telemetry. `PITBOARD_API_BASE` redirects
  requests for tests, and only to a loopback IP address, because the server that answers
  decides which account a login is filed under.
- You can check that a download is unchanged. Every release attests each file it
  publishes: its tarballs, the app, their bills of materials, `SHA256SUMS` and, for a full
  release, the update feed, `appcast.xml`.
  `gh attestation verify <file> --repo datlechin/pitboard` checks a file against the
  workflow and commit that made it. The macOS files are signed with a Developer ID and
  notarised. Homebrew builds nothing and refuses a file changed on the release page later,
  because the tap's checksums come from the release run.
- An installed app takes an update signed with the key it carries. A copy signed with a
  Developer ID also takes one whose app is signed by the same Developer ID team. A copy
  signed ad hoc learns another key only from a release signed with its key, and otherwise
  stops updating without saying so. So the release workflow refuses a changed key unless
  the repository declares a rotation.
- The app's claude.ai windows keep their hands off claude.ai's sign-in. pitboard never
  reads, copies or changes what WebKit keeps for a window, adds no script to the page,
  poses as no other browser, and never makes a claude.ai sign-in from a Claude Code
  login. The sign-in happens on claude.ai's own page. A window stays on
  `https://claude.ai`; any other page opens in your browser, and Google's sign-in, which
  Google blocks in apps, is stopped rather than sent there. A download that an artifact or
  another site starts asks first, and every download is quarantined, as a browser's is.
- Forgetting a Claude Code account deletes what its claude.ai window keeps, sign-in
  included: at once in the app, and at the app's next read after `pitboard forget`. A
  store that cannot be deleted is said, and tried again at every later read.
- Nothing from outside the app opens a claude.ai window by itself. A `pitboard://` link, a
  Service request and a share each show the account picker, and only a choice there opens
  a window. Only claude.ai links are accepted, and claude.ai's sign-in links are refused,
  since one would sign a window in as whoever it belongs to. No route reads a browser's
  cookies, history or files.
- The Share extension is sandboxed, with no network, file or shared-group access. It reads
  the one link its host hands it and keeps nothing.
- pitboard renews a parked login while its account is enrolled, so an account nobody
  uses keeps a live refresh token. `pitboard doctor` warns about an account last switched
  to 30 days ago or more, a Claude Code refresh token's life. It does not warn about one
  enrolled with `--sign-in` and never switched to. `pitboard forget` deletes the login and
  its record.

## What pitboard does not protect against

- Another process running as your user, which can read what you can read. That includes
  each claude.ai window's sign-in, which WebKit keeps as ordinary files under
  `~/Library/WebKit/com.usepitboard.Pitboard`, as a browser keeps its cookies.
- claude.ai's own page, which runs in WebKit's processes as it would in Safari, with its
  own scripts and whatever it embeds. pitboard decides only where the window goes.
- Another user with administrator access to your computer.
- A compromised Claude Code, Codex or dependency of pitboard. CI checks every Rust
  dependency for known advisories, licences and sources. A weekly job checks Sparkle, the
  app's update framework, only for a newer release. Each release publishes a CycloneDX bill
  of materials for each tarball and for the app, Sparkle included.
- Signing out inside a `codex` started before a switch. It still holds the outgoing
  account's tokens, so `/logout` there revokes the login pitboard has parked. After a Codex
  switch, pitboard counts the running `codex` processes and says to quit them instead.
- Anyone who can already read your files. On Linux, parked logins are files in
  `~/.pitboard/vault/`, each 0600, in a directory held at 0700. Those modes are all that
  keep other users on the machine from reading them. A backup restore, `cp -r` or a umask
  can change them without warning. `pitboard doctor` fails if anyone else has access to
  Claude Code's login file, the vault or a file in it, and warns for Codex's `auth.json`.
- A token still valid after pitboard deletes it. pitboard revokes no login, so a deleted
  parked login stays valid until it expires. Whether Anthropic accepts a revocation, and
  whether it would end the whole grant or only pitboard's refresh chain, has not been
  measured. So pitboard does not try.
- A forged ID token on your disk. pitboard reads a Codex login's account from its ID
  token without checking the signature. The token came from Codex on your disk, which is
  the same trust as reading any other file there.
- A large login on the argument line on macOS. A login too large for standard input goes
  to `security` as an argument, which another process running as you could read while the
  call lasts. Every Codex parked login is that large. A switch or `--sign-in` enrolment
  says so, as does `pitboard doctor` for a Claude Code login; a renewal does not.
  `PITBOARD_NO_ARGV=1` refuses such a write, so on macOS no Codex account can be parked.
  For what the variable does to renewal and to the app, see
  [Large logins on macOS](https://docs.usepitboard.com/security#large-logins-on-macos).
