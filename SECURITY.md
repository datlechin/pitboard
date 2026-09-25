# Security

pitboard handles the OAuth refresh tokens of Claude Code and Codex, which grant full access
to a paid account. This file says where they live, what pitboard defends against, and what
it does not.

## Where your credentials are

### macOS

Claude Code's own login stays in the keychain item it created. Parked copies are
keychain items named `pitboard-park-<account>-<time>`, in your login keychain. What a Claude
Code park holds is the account's whole slice of Claude Code's credential document: its
OAuth block, and whichever of `organizationUuid`, `trustedDeviceToken`, `enterpriseGateway`
and `designOauth` were there, which are the keys Claude Code itself deletes on a logout. That
is what a switch back puts there, so an account comes back as it left. On one real account
the slice is 524 bytes against 506 for the OAuth block alone; an account holding a device
token has not been measured. Whether restoring a device token spares a re-verification is
also not measured, and is not claimed. They are
read and written only through `/usr/bin/security`, the one application the item's access
list trusts. A token goes to `security` on standard input rather than on a command line,
where `ps` could see it, unless the login is too large for that (see below).

Codex keeps its login in a file, `~/.codex/auth.json` (or `$CODEX_HOME/auth.json`), at mode
0600, on macOS as everywhere else. pitboard reads and writes that file and no other store of
Codex's. A Codex park is the whole file: the ID, access and refresh tokens, the ChatGPT
account id, `last_refresh`, and the `OPENAI_API_KEY` field, which can hold an API key Codex
obtained at sign-in. It goes into a `pitboard-park-` keychain item like a Claude Code park.

Codex can instead be set to keep its login in the keychain: `cli_auth_credentials_store` set
to `keyring` or `auto` in its `config.toml`, with or without the `secret_auth_storage`
feature. pitboard refuses that setup. Those items are Codex's own, made through the
Security framework and trusting only `codex`, so every read by another program would bring
up a permission prompt, and choosing Always Allow would change Codex's item. `ephemeral`,
which keeps the login in memory only, is refused too: there is nothing at rest to park.

A login larger than about two kilobytes cannot go through standard input. MCP server tokens
make a Claude Code login that large, and every Codex park is larger: it is the whole
`auth.json`, over four kilobytes. `security` reads at most 4097 bytes of command from
standard input, with no line continuation, and its interactive prompt takes 128 bytes.
Measured on macOS 26 on 22 September 2026, along with the two alternatives:

| How to write a large login | Cost |
| --- | --- |
| `security ... -X <hex>` on the argument line | Visible to `ps` for the length of one call |
| The Security framework, in pitboard's own process | Every later read of that item by `security` takes about a second instead of 0.01, for good |

The second was measured on a scratch item: reads went from 0.01 seconds to 20.55, then
settled around 0.8. Claude Code reads its login on every cache miss, so pitboard would be
making Claude Code permanently slower to save an exposure of a few milliseconds. It also
cannot be undone without `security set-key-partition-list`, which asks for the keychain
password.

So pitboard passes it as an argument and says so in the warnings of that switch or
`--sign-in` enrolment, and in `doctor` for a Claude Code login. A renewal of a parked login
writes the same way without a warning. For a Claude Code login this is what Claude Code
itself does on every token refresh. Codex writes its login to a file, so for a Codex park
the exposure is pitboard's alone, on every Codex switch, `--sign-in` enrolment and renewal
on macOS. `PITBOARD_NO_ARGV=1` refuses the write instead, for anyone who would rather have
neither; on macOS that means no Codex account can be parked. Set it before parking any
Codex account: with Codex parks already in the keychain, the next renewal exchanges the
refresh token and then cannot store the result, and that account's parked login is lost.

### Linux

Claude Code keeps its login in a plaintext file, `.credentials.json`, in its
config directory. That is Claude Code's design and pitboard cannot change it: the shipping
build has no Secret Service, libsecret, gnome-keyring or KWallet backend, only the macOS
keychain and the Windows credential manager. Codex keeps its login in `auth.json` at mode
0600, as it does on macOS. Parked copies of both tools live in `~/.pitboard/vault/`,
one file per copy, each 0600, in a directory held at 0700. Any process running as your user
can read them, which is the exposure each tool's own file already has.

A mode bit is the whole of that protection, and a backup restore, a `cp -r` or a careless
umask changes one quietly. `pitboard doctor` looks at the actual modes of Claude Code's
credential file, the vault and everything in it, and fails if anyone but you can read one.
It does not look at Codex's `auth.json`.

### Everywhere else

Every Claude Code switch copies Claude Code's config file to
`~/.pitboard/backups/claude.json.<time>` before rewriting the account it names, and the ten
newest are kept. That copy holds whatever Claude Code keeps in its config, which includes
the signed-in email address, the account and organization identifiers, and the path of
every project you have used it in. It holds no token. `pitboard uninstall` removes it along
with everything else pitboard wrote. A Codex switch changes only `auth.json`, so it makes
no such copy.

The audit log is one tab-separated line per change: the time, which front end asked, the
verb, the label, and how it ended. Labels, codes and times only.

pitboard's account list, `~/.pitboard/state.json`, holds each account's tool, label and
email address; for a Claude Code account its Anthropic account and organization
identifiers, and for a Codex account its ChatGPT account and user ids, workspace id and
plan. It holds no token. `~/.pitboard/usage.json` holds the last usage reading per
account. The audit log holds labels, codes and times only.

## What leaves your machine

For a Claude Code account, pitboard makes two read-only requests to
`https://api.anthropic.com`, each carrying an access token: `/api/oauth/profile`, to learn
which account a login belongs to, and `/api/oauth/usage`, for the numbers `pitboard status`
shows. It sends a refresh token in one case only: renewing a parked login whose access
token has expired, through `https://platform.claude.com/v1/oauth/token` with Claude Code's
own client id, the request Claude Code makes to renew its own login.

pitboard reads nothing of Codex's, and sends OpenAI nothing, until a Codex account is
enrolled. For a Codex account, pitboard makes one read-only request,
`GET https://chatgpt.com/backend-api/wham/usage`, carrying the access token and the ChatGPT
account id. It is the usage read Codex itself makes, and spends no quota. pitboard uses it
for the numbers `pitboard status` shows, and to check that OpenAI still accepts a Codex
login before a switch installs it. Which account a Codex login belongs to is read from its
own ID token with no request. The token's signature is not checked: it came from Codex on
your own disk, which is the same trust as reading any other file there.

pitboard sends a Codex refresh token in one case only: renewing a parked login whose
access token has expired, through `https://auth.openai.com/oauth/token` with Codex's own
public client id, `app_EMoamEEZ73f0CkXaXp7hrann`, the request Codex makes to renew its own
login.

A parked login is held by pitboard alone, so renewing it puts no second holder on its
refresh chain. The new tokens replace the parked copy before the old one is deleted, and all
of this happens under pitboard's lock, so no switch can install the old copy meanwhile. The
login signed in is never renewed by pitboard: that is the tool's own job, and a second
renewer would break it.

TLS is verified against your operating system's trust store. There is no telemetry.

`PITBOARD_API_BASE` redirects these requests for tests, and is honoured only for a loopback
IP address.

## What pitboard defends against

- Mixing up accounts: which account a Claude Code login belongs to is asked of Anthropic,
  not taken from Claude Code's config, which can be a day out of date. A Codex login names
  its account in its ID token, by ChatGPT account and user, so two people in one Team or
  Business workspace are two accounts. Every parked copy is also bound to a fingerprint of
  its refresh token, and a copy that does not match is refused.
- Corruption from a crash mid-switch: a switch durably records its intent before acting,
  and the next `use`, `enroll` or `forget` finishes what the interrupted one started before
  doing anything else. When it cannot tell what happened, it changes nothing and keeps the
  record for a later run.
- Leftover copies: a parked login that is no longer needed stays listed until it is
  deleted, so a failed or interrupted delete is retried. Temporary files a killed run left
  behind are removed on the next write to the same directory.
- pitboard renews a parked login for as long as its account is enrolled, so an account you
  enrol once and never come back to keeps a live, continuously rotated refresh token on the
  machine. `pitboard doctor` says so once an account has not been switched to for thirty
  days, a Claude Code refresh token's life (Codex states none), and `pitboard forget
  <label>` deletes the login and the record. Deleting is not revoking: a token pitboard
  deletes stays valid at Anthropic or OpenAI until it expires on its own. For Anthropic,
  whether the endpoint pitboard uses accepts a revocation, and whether one would end only
  the chain pitboard holds or the whole grant, has not been measured, so pitboard does not
  try. It does not revoke a Codex login either.
- Copies nothing names: every name pitboard is about to write a login into is written down
  before the login is, so a run killed between the two leaves a name the next command
  resolves rather than a login nothing on the machine can see. An item whose account is
  enrolled and holds nothing goes back to that account; one nobody wants is deleted; one
  that cannot be read is left alone and tried again, because a store that could not answer
  says nothing about what is in it.
- A locked or unreadable keychain: reported as unreadable, never taken to mean that no
  login is there.
- Restoring a login Claude Code has already moved past: each account keeps one parked
  login, deleted as soon as it is installed, and a copy of a login that is still signed in
  is never kept, because presenting a superseded refresh token makes Claude Code discard
  the login. A parked login past its expiry is refused rather than installed.
- Writing alongside a running Claude Code: pitboard takes the same lock Claude Code takes,
  with the same staleness and the same heartbeat. Two writers come through that lock, a
  session and the supervisor daemon Claude Code leaves running behind it, which refreshes
  the login on a schedule of its own. Both wait for pitboard, and both re-read the
  credential inside the lock before changing it, so neither can write an older account back
  over a switch. One path does not take the lock at all: a `/logout` that has given up
  waiting deletes the credential with nothing held. pitboard cannot exclude that, so it
  reads the slot back after a switch rather than trusting that its own write stood.
- Two usable copies of one Codex login: `codex login` and `codex logout` send the stored
  refresh token to OpenAI to be revoked before clearing it, so if pitboard kept a copy
  beside the live login, your next sign-in or sign-out would end both. A Codex park is
  therefore never a copy: the outgoing login is moved into pitboard's store and read back
  before the incoming one is written, and recovery, `abandon` and `repair` keep no park
  that copies the login signed in now.
- Writing alongside a running Codex: Codex takes no lock on `auth.json`, so there is none to
  share, and pitboard reads the file back after a switch. A `codex` whose token refresh is
  under way when the switch happens writes its old account's tokens under the new
  account's id. A login whose tokens and account id name different accounts is refused,
  never parked or taken as proof that a switch held.
- A state directory inside a cloud-synced folder: refused, because a parked login belongs
  to exactly one machine.
- A changed download. Every release attests each command line tarball, the app,
  `SHA256SUMS`, a bill of materials beside each artefact, and `appcast.xml`, which is the
  file that decides what an installed copy runs next. `gh attestation verify <file> --repo
  datlechin/pitboard` checks any of them against the workflow and the commit that produced
  it. On macOS the command line and the app are signed with a Developer ID and notarised,
  and the app carries its own signed copy of the command line, so an update replaces both.
  Homebrew installs these same files and builds nothing: the command line's tarball for the
  machine, or the app. The checksums it checks are the lines the release itself wrote in
  `SHA256SUMS` over the files it published, not ones taken later somewhere else.
- A changed update key. An installed copy takes an update signed by the key in the bundle
  it came from. A release whose key differs from the one the previous release shipped is
  refused unless the repository says that release means to rotate, because an ad-hoc
  installed copy has no other route to accept a new key and would silently stop updating.

## What pitboard does not defend against

- Another process running as your user. It can read what you can read.
- Another user with administrative access to your machine.
- A compromised Claude Code or Codex binary, or a compromised dependency of pitboard itself.
  The dependency tree is checked for known advisories, licences and sources in CI, and each
  release publishes what was in it as a CycloneDX bill of materials beside the artefact it
  describes.
- Signing out inside a `codex` that was running before a switch. It still holds the
  outgoing account's tokens in memory, so `/logout` there revokes the login pitboard has
  just parked. After a Codex switch made on the command line, pitboard counts the running
  `codex` processes and says to quit them rather than sign out; the menu bar app does not
  yet say so.
- Anyone who can already read your files. Where there is no keychain, a parked login is a
  file and a mode bit is the whole of what keeps it private. `pitboard doctor` checks the
  modes and fails when one is wrong, which is all it can do.

## If something goes wrong

If a switch is interrupted, the next `pitboard use`, `enroll` or `forget` finishes it first
and says what it found. Run `pitboard doctor` if anything still looks wrong.

When the login in place matches neither side of the record, finishing needs to know whose
it is. A Codex login names its own account. For Claude Code, Anthropic has to say, and when
it cannot be reached, recovery changes nothing and every command that would change
something stops. `pitboard abandon` is the way out: it throws the record away and keeps
every copy it names, so nothing is lost and `pitboard status` can say who is signed in. The
one exception is a Codex park that copies the login signed in now: it is dropped, since that
login is still in place and a sign-out would revoke both.

To remove pitboard, run `pitboard uninstall` before removing pitboard itself: it takes the
daily renewal schedule away, then deletes every parked login before deleting its own
directory, in that order, because the account list is the only index of those keychain
items. Deleting the directory first leaves live refresh tokens on the machine with nothing
able to name them, which is why the casks in the tap leave `~/.pitboard` where it is, even
on `brew uninstall --zap`. The app cask 0.3.0 installed as `pitboard` did not: its zap
moves `~/.pitboard` to the Trash, and Homebrew runs a cask's zap as it was installed, so
leave `--zap` out when removing that one.

Never copy `~/.pitboard` to another machine. pitboard refuses to read a state file written
elsewhere, and a parked login presented from a second machine can end the login on both.

## Reporting a vulnerability

Report privately through the "Report a vulnerability" button on this repository's Security
tab rather than in a public issue.

One person maintains this. Reports are answered as soon as possible, and anything that
could expose a credential is handled before other work.
