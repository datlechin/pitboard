# Security policy

Pitboard handles the OAuth refresh tokens of Claude Code and Codex, which grant full access
to a paid account. A Codex parked login is Codex's whole `auth.json`, which can also hold an
OpenAI API key. The menu bar app also gives each account a window on claude.ai or
chatgpt.com, where WebKit keeps that site's sign-in. Its Share extension hands it a page
from your browser.

To report a vulnerability, follow [Report a vulnerability](#report-a-vulnerability), not a
public issue. For where parked logins are kept and what leaves your machine, see
[Security and privacy](https://docs.usepitboard.com/security).

## Supported versions

Security fixes go into the next release. Earlier releases do not get fixes, so update to
the most recent one.

For how to update, see [Update Pitboard](https://docs.usepitboard.com/install#update-pitboard).

## Report a vulnerability

Use
[Pitboard's private report form](https://github.com/datlechin/pitboard/security/advisories/new),
not a public issue.

Include:

- Pitboard's version, from `pitboard --version`
- your macOS or Linux version
- the steps to reproduce it
- the output of `pitboard doctor --json`, where it helps

`pitboard doctor --json` prints no token, and shows email addresses and account identifiers
as digests.

One person maintains Pitboard. Reports are answered as early as possible, and anything
that could expose a token comes before other work. Each fix is listed under Security in
[CHANGELOG.md](CHANGELOG.md).

## What Pitboard protects against

A report is in scope when Pitboard fails at one of these.

- Pitboard does not file a login under the wrong account. It asks Anthropic which account a
  Claude Code login belongs to, because Claude Code's config can be a day out of date. A
  Codex login names its ChatGPT account and user in its ID token, so two people in one Team
  or Business workspace are two accounts. Each parked login is bound to its refresh token's
  fingerprint, and a mismatch is refused.
- A switch interrupted by a crash is finished or undone. It records its intent before it
  moves any login. The next command that changes something settles it first, and changes
  nothing if it cannot tell what happened.
- No parked login is left where nothing can find it. Pitboard writes down each name before
  writing a login into it. The next change returns any login a killed run left under such a
  name to its account, or deletes it. An unreadable login is tried again later, and so is a
  failed delete. Temporary files a killed run left, which can hold a whole login, are
  removed by the next write to their directory.
- Pitboard reports a locked or unreadable keychain as unreadable, never as empty. Taking it
  as empty could make Pitboard park or overwrite the wrong login.
- Pitboard never installs a login Claude Code has since renewed, because presenting a
  replaced refresh token makes Claude Code discard the login. It deletes a parked login
  once installed, keeps no copy of a login in use and refuses an expired one. It renews a
  parked login, which it alone holds, because a second renewer would break a login. It
  stores the renewed login before deleting the old one, under its lock.
- The one other login Pitboard renews is one `pitboard stow` puts away from
  `.credentials.json` whose access token has expired, which a session may be using. It holds
  the lock every Claude Code renewal takes before it sends a refresh token, as read in Claude
  Code 2.1.294, so no session renews that login meanwhile. It writes the renewed login back
  to the file before anything else.
- Pitboard takes the lock Claude Code takes around each write of its login. As read in
  Claude Code 2.1.284, sessions and the supervisor daemon wait for that lock and read the
  login again inside it. So neither can write an older account back over a switch. A
  `/logout` that gave up waiting deletes the login without the lock, so Pitboard reads the
  login back after each switch.
- Pitboard never keeps two usable copies of one Codex login. `codex login` and
  `codex logout` ask OpenAI to revoke the stored refresh token, so a copy would end with
  your next sign-in or sign-out. A Codex switch moves the outgoing login into Pitboard's
  store and reads it back before writing the incoming one. Recovery, `abandon` and
  `repair` keep no such copy.
- Pitboard reads `auth.json` back after a Codex switch, because Codex takes no lock on it.
  A `codex` refreshing its token during a switch writes its old account's tokens under the
  incoming account's id. Pitboard refuses a login whose tokens and account id disagree.
- A parked login stays on one machine, because a refresh token presented from a second
  machine can end the login on both. Pitboard refuses a state directory in a cloud-synced
  folder, or a state file written on another machine. `pitboard adopt` takes over such a
  file, keeping the accounts and dropping their parked logins.
- Pitboard sends a login only to the service that issued it, verifies TLS against your
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
- An account's window keeps its hands off the site's sign-in. The sign-in happens on the
  site's own pages, claude.ai's or chatgpt.com's, and WebKit keeps it in that account's
  store. Pitboard never reads, copies or changes what a site keeps there, adds no script
  to a page, sets no user agent of its own and makes no web session from a Claude Code or
  Codex login.
- A window stays on its site and the hosts its sign-in goes to. Any other web page opens in
  your default browser, and a link to another app is refused. Google's sign-in, which
  Google does not allow inside apps, is stopped rather than sent to the browser, where it
  would sign in the browser. A sign-in window shares its account's store, loads nothing
  else, saves nothing and closes with the account's window. Every page is refused the
  camera and the microphone.
- A download that a frame inside the page, such as an artifact, or another site starts
  asks first. WebKit quarantines every downloaded file, as a browser's are, and a download
  never replaces a file in your Downloads folder.
- Forgetting an account deletes what its window keeps, sign-in included, at the app's next
  read of the accounts that succeeds. A store something still holds is tried again at
  later reads. Pitboard deletes only the stores it recorded making for the Pitboard
  directory it reads. So a copy run with another `HOME` or `PITBOARD_HOME` never signs
  another copy's windows out. A store another Pitboard directory also recorded is kept while
  that directory exists, and only the record of the directory Pitboard reads is removed.
- Nothing from outside the app opens an account's window by itself. Anything on the Mac can
  open a `pitboard://` link, so one only shows the **Open Link** window. Only a choice
  there opens a window, and only on the link's own site. Only claude.ai and chatgpt.com
  links are accepted, and their sign-in links are refused, since one would sign a window
  in as whoever it belongs to.
- The Share extension is sandboxed, with no network, file or shared-group access. It reads
  the one link the browser shares, keeps nothing, and hands the link only to the app it
  came in.
- Pitboard renews a parked login while its account is enrolled, so an account nobody
  uses keeps a live refresh token. `pitboard doctor` warns about an account last switched
  to 30 days ago or more, a Claude Code refresh token's life. It does not warn about one
  enrolled with `--sign-in` and never switched to. `pitboard forget` deletes the login and
  its record.

## What Pitboard does not protect against

- Another process running as your user, which can read what you can read. That includes
  each account window's sign-in, which WebKit keeps as ordinary files under
  `~/Library/WebKit/com.usepitboard.Pitboard`, as a browser keeps its cookies.
- The sites' own pages, claude.ai's and chatgpt.com's and their sign-in pages. They run in
  WebKit as they would in Safari, with their own scripts and whatever they embed, such as
  an artifact. Pitboard decides only where a window goes.
- A page a site opens in your browser. Anything you connect there goes to the account your
  browser is signed in to, which may not be the window's account. The window says so when
  the site opened the page without a click.
- Another user with administrator access to your computer.
- A compromised Claude Code, Codex or dependency of Pitboard. CI checks every Rust
  dependency for known advisories, licences and sources. A weekly job checks Sparkle, the
  app's update framework, only for a newer release. Each release publishes a CycloneDX bill
  of materials for each tarball and for the app, Sparkle included.
- Signing out inside a `codex` started before a switch. It still holds the outgoing
  account's tokens, so `/logout` there revokes the login Pitboard has parked. After a Codex
  switch, Pitboard counts the running `codex` processes and says to quit them instead.
- Anyone who can already read your files. On Linux, parked logins are files in
  `~/.pitboard/vault/`, each 0600, in a directory held at 0700. Those modes are all that
  keep other users on the machine from reading them. A backup restore, `cp -r` or a umask
  can change them without warning. `pitboard doctor` fails if anyone else has access to
  Claude Code's login file, the vault or a file in it, and warns for Codex's `auth.json`.
- A token still valid after Pitboard deletes it. Pitboard revokes no login, so a deleted
  parked login stays valid until it expires. Whether Anthropic accepts a revocation, and
  whether it would end the whole grant or only Pitboard's refresh chain, has not been
  measured. So Pitboard does not try.
- A forged ID token on your disk. Pitboard reads a Codex login's account from its ID
  token without checking the signature. The token came from Codex on your disk, which is
  the same trust as reading any other file there.
- A large login on the argument line on macOS. A login too large for standard input goes
  to `security` as an argument, which another process running as you could read while the
  call lasts. Every Codex parked login is that large. A switch or `--sign-in` enrolment
  says so, as does `pitboard doctor` for a Claude Code login; a renewal does not.
  `PITBOARD_NO_ARGV=1` refuses such a write, so on macOS no Codex account can be parked.
  For what the variable does to renewal and to the app, see
  [Large logins on macOS](https://docs.usepitboard.com/security#large-logins-on-macos).
