# pitboard

Park and restore your own Claude Code logins, and see what each one has left.

If you hold more than one Claude subscription, switching between them means signing out
and back in through a browser every time. pitboard keeps a copy of each login you give it,
and moves the one you ask for into the place Claude Code reads. Your history, sessions,
settings and projects stay exactly where they are — only the identity changes.

**Status: pre-release.** macOS is the tested platform. On Linux pitboard builds and its
test suite passes, but how Claude Code stores its login there is inferred from its macOS
build and has not yet been confirmed on a signed-in Linux install.

## Getting started

```sh
cargo install pitboard
```

Enroll the account you are signed in as now, then add the others without signing out of
it:

```sh
pitboard enroll personal
pitboard enroll work --sign-in
```

`--sign-in` runs Claude Code's own sign-in in a private directory, so the login in use is
never touched. Do not add accounts with `/login` instead: that replaces the login in use,
and pitboard cannot keep a copy of a login it did not see leave.

## Every day

```sh
pitboard            # who is signed in, what each account has left, which can be used
pitboard use work   # switch
```

```text
● personal  me@example.com  signed in
    5h    ██████░░░░   59%  resets in 1h 10m
    week  ███████░░░   73%  resets in 5d 18h

○ work      me@company.com  ready · good for 26d 4h
    5h    █░░░░░░░░░   12%  resets in 3h 02m
    week  ████░░░░░░   40%  resets in 2d 4h
```

Usage is asked of Anthropic each time, for every account at once. A parked login whose
access token has expired is renewed first, the way Claude Code renews its own, so every
account answers live and none lapses while parked. If Anthropic cannot be reached, pitboard
shows the last number it measured, and when.

A Claude Code session that is already running picks up a switch within about 33 seconds,
without restarting. Until then it keeps using the previous account.

Each account keeps one parked login. It is used up when you switch to that account, and a
fresh one is parked when you switch away. If one expires or goes missing, sign in to that
account again; the rest of pitboard's record of it stays as it is:

```sh
pitboard enroll work --sign-in
```

A label typed wrong is changed without signing in again: `pitboard rename wrong right`.

`pitboard doctor` checks everything pitboard relies on, including every parked login.

## In Claude Code's status bar

`pitboard statusline` prints one line naming the account in use and what every enrolled
account has left:

```text
personal 59%·73%  work 12%·40%
```

It reads the session Claude Code passes on stdin, and pitboard's own files; it never calls
the network or touches a login. Add it to `~/.claude/settings.json`:

```json
{ "statusLine": { "type": "command", "command": "pitboard statusline" } }
```

To combine it with a status line of your own, pipe the same input to it:
`echo "$input" | pitboard statusline`.

## In the menu bar

pitboard also comes as a small macOS app: the account in use and its tightest limit in the
menu bar, every account's limits in the panel, and one click to switch.

It is not released as a signed download yet. To build and install it from a clone:

```sh
./apple/scripts/build-app.sh
cp -R apple/build/Pitboard.app /Applications/
```

It is the same core the CLI uses, called directly — not the `pitboard` binary in a
subprocess — so it needs nothing else installed. Usage is read when you open the panel and
every few minutes while it runs; "Open at login" is under the menu at the bottom right.

A copy downloaded from a release keeps itself up to date; one built from a clone carries no
update key and so never checks.

## Scripting

Every command accepts `--json` and emits the same versioned envelope, including on
failure and for a mistyped command line: `{v, command, ok, data, warnings, error}`. Error
codes are stable. Exit codes: 0 done, 1 not done, 2 command line wrong, 3 a login or
Claude Code's files are in a state pitboard will not act on.

Shell completions: `pitboard completions zsh` (or `bash`, `fish`, `elvish`, `powershell`).

## What it will not do

These are rules, not gaps:

- No automatic switching on any server signal.
- No request pooling, proxying, or `ANTHROPIC_BASE_URL` interception.
- No failover when an account is on hold.
- No renewing of the login signed in: that is Claude Code's. pitboard renews only a parked
  login, which it alone holds.
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
- **Usage for a parked account** is asked with its parked login, so what other machines
  used on that account is counted too.

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
