# pitboard

Park and restore your own Claude Code logins, and see what each one has left.

If you hold more than one Claude subscription, switching between them means signing out
and back in through a browser every time. pitboard keeps a copy of each login you give it,
and moves the one you ask for into the place Claude Code reads. Your history, sessions,
settings and projects stay exactly where they are — only the identity changes.

**Status: pre-release.** macOS is the tested platform. Linux is implemented but its
behaviour is inferred from a macOS build of Claude Code and has not yet been confirmed on a
real Linux install.

## Getting started

```sh
cargo install pitboard
```

Enroll the account you are signed in as now:

```sh
pitboard enroll personal
```

Add another account without signing out of this one:

```sh
pitboard enroll work --sign-in
```

That runs Claude Code's own sign-in in a private directory, so the login you are using is
never touched. Do not add accounts by signing in with `/login` instead: that replaces the
login in use, and pitboard cannot keep a copy of a login it did not see leave.

From then on:

```sh
pitboard            # who is signed in, and how much each account has left
pitboard use work   # switch
pitboard doctor     # check that pitboard's model of Claude Code still holds
```

Usage is asked of Anthropic each time, for every account at once. A parked account can be
asked for about twelve hours after it was parked; after that pitboard shows the last number
it measured, and says when.

A Claude Code session that is already running picks up a switch within about 33 seconds,
without restarting. Until then it keeps using the previous account.

Every command accepts `--json` and emits the same versioned envelope, including on
failure: `{v, command, ok, data, warnings, error}`. Error codes are stable.

## What it will not do

These are rules, not gaps:

- No automatic switching on any server signal.
- No request pooling, proxying, or `ANTHROPIC_BASE_URL` interception.
- No failover when an account is on hold.
- No OAuth grant of any kind. Refreshing tokens is Claude Code's job, never pitboard's.
- No export, import or sync of parked logins between machines.

That last one is a safety property, not a missing feature. Each machine must sign in to
each account itself: presenting a refresh token that another machine has since rotated
makes Claude Code discard the login on both.

## What a switch costs you

Honestly listed, because none of these are pitboard's to fix:

- **Remote Control** is switched off for a conversation that was started under a different
  account. Claude Code does this deliberately when it sees the owner change.
- **The prompt cache** is per account, so the first message after a switch rebuilds it. On
  this project's own measurements that costs about the same as leaving a session idle for
  an hour — which a five-hour limit usually means has happened anyway.
- **Usage for a parked account** is live for about twelve hours after it was parked, and
  after that is the last number pitboard measured. Another machine using the same account
  since then is counted once it can be asked again.

## How it works

pitboard asks Anthropic which account a login belongs to, rather than trusting Claude
Code's config, which can be a day behind the login it describes. It will not move a login
it cannot identify.

On macOS, Claude Code keeps its login in a keychain item that only `/usr/bin/security` is
trusted to read. pitboard reads and writes it the same way, deliberately: calling the
Security framework directly from another process permanently alters the item's access
list, after which every read Claude Code makes costs one to three seconds instead of a few
milliseconds. pitboard takes the same lock Claude Code takes around every credential write,
and never writes Claude Code's plaintext fallback file on your behalf.

See [SECURITY.md](SECURITY.md) for where your credentials live and what pitboard defends
against.

## Licence

Apache-2.0. Not affiliated with Anthropic; see [NOTICE](NOTICE).
