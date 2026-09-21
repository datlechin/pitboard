# Security

pitboard handles OAuth refresh tokens that grant full access to a paid account. This file
says where they live, what pitboard defends against, and what it does not.

## Where your credentials are

### macOS

Claude Code's own login stays in the keychain item it created. Parked copies are
keychain items named `pitboard-park-<account>-<time>`, in your login keychain. They are
read and written only through `/usr/bin/security`, the one application the item's access
list trusts. No token is ever passed on a command line, where `ps` could see it; it goes
to `security` on standard input.

### Linux

Claude Code keeps its login in a plaintext file, `.credentials.json`, in its
config directory. That is Claude Code's design and pitboard cannot change it. Parked copies
live in `~/.pitboard/vault/`, one file per copy, each 0600, in a directory held at 0700.
Any process running as your user can read them, which is the exposure Claude Code's own
file already has.

### Everywhere else

Every switch copies Claude Code's config file to `~/.pitboard/backups/claude.json.<time>`
before rewriting the account it names, and the ten newest are kept. That copy holds
whatever Claude Code keeps in its config, which includes the signed-in email address, the
account and organization identifiers, and the path of every project you have used it in. It
holds no token. `pitboard uninstall` removes it along with everything else pitboard wrote.

The audit log is one tab-separated line per change: the time, which front end asked, the
verb, the label, and how it ended. Labels, codes and times only.

pitboard's account list, `~/.pitboard/state.json`, holds
each account's email address and Anthropic account and organization identifiers, but no
token. `~/.pitboard/usage.json` holds the last usage reading per account. The audit log
holds labels, codes and times only.

## What leaves your machine

pitboard makes two read-only requests to `https://api.anthropic.com`, each carrying an
access token: `/api/oauth/profile`, to learn which account a login belongs to, and
`/api/oauth/usage`, for the numbers `pitboard status` shows.

It sends a refresh token in one case only: renewing a parked login whose access
token has expired, through `https://platform.claude.com/v1/oauth/token` with Claude Code's
own client id, the request Claude Code makes to renew its own login. A parked login is held
by pitboard alone, so renewing it puts no second holder on its refresh chain. The new
tokens replace the parked copy before the old one is deleted, and all of this happens under
pitboard's lock, so no switch can install the old copy meanwhile. The login signed in is
never renewed by pitboard: that is Claude Code's job, and a second renewer would break it.

TLS is verified against your operating system's trust store. There is no telemetry.

`PITBOARD_API_BASE` redirects these requests for tests, and is honoured only for a loopback
IP address.

## What pitboard defends against

- Mixing up accounts: which account a login belongs to is asked of Anthropic, not taken
  from Claude Code's config, which can be a day out of date. Every parked copy is also
  bound to a fingerprint of its refresh token, and a copy that does not match is refused.
- Corruption from a crash mid-switch: a switch durably records its intent before acting,
  and the next `use`, `enroll` or `forget` finishes what the interrupted one started before
  doing anything else. When it cannot tell what happened, it changes nothing and keeps the
  record for a later run.
- Leftover copies: a parked login that is no longer needed stays listed until it is
  deleted, so a failed or interrupted delete is retried. Temporary files a killed run left
  behind are removed on the next write to the same directory.
- A locked or unreadable keychain: reported as unreadable, never taken to mean that no
  login is there.
- Restoring a login Claude Code has already moved past: each account keeps one parked
  login, deleted as soon as it is installed, and a copy of a login that is still signed in
  is never kept, because presenting a superseded refresh token makes Claude Code discard
  the login. A parked login past its expiry is refused rather than installed.
- Writing alongside a running Claude Code: pitboard takes the same lock Claude Code takes
  around every credential write.
- A state directory inside a cloud-synced folder: refused, because a parked login belongs
  to exactly one machine.

## What pitboard does not defend against

- Another process running as your user. It can read what you can read.
- Another user with administrative access to your machine.
- A compromised Claude Code binary, or a compromised dependency of pitboard itself. The
  dependency tree is checked for known advisories, licences and sources in CI.

## If something goes wrong

If a switch is interrupted, the next `pitboard use`, `enroll` or `forget` finishes it first
and says what it found. Run `pitboard doctor` if anything still looks wrong.

Finishing needs Anthropic to say who owns the login that is in place. When it cannot be
reached, recovery changes nothing and every command that would change something stops.
`pitboard abandon` is the way out: it throws the record away and keeps every copy it names,
so nothing is lost and `pitboard status` can say who is signed in.

To remove pitboard, run `pitboard uninstall`: it deletes every parked login before
deleting its own directory, in that order, because the account list is the only index of
those keychain items. Deleting the directory first leaves live refresh tokens on the
machine with nothing able to name them.

Never copy `~/.pitboard` to another machine. pitboard refuses to read a state file written
elsewhere, and a parked login presented from a second machine can end the login on both.

## Reporting a vulnerability

Report privately through the "Report a vulnerability" button on this repository's Security
tab rather than in a public issue.

One person maintains this. Reports are answered as soon as possible, and anything that
could expose a credential is handled before other work.
