# pitboard

Switch between your own Claude Code logins, and see how much each one has left.

If you have more than one Claude subscription, changing accounts normally means signing out
and back in through a browser. pitboard keeps a copy of each login and puts the one you ask
for where Claude Code reads it. Your history, sessions, settings and projects stay where
they are. Only the account changes.

Status: pre-release. macOS is tested. On Linux it builds and the tests pass, but how Claude
Code stores its login there has not been checked on a real signed-in install.

## Install

```sh
brew install datlechin/tap/pitboard        # the command line
brew install --cask datlechin/tap/pitboard # the menu bar app, macOS 14 or later
```

Or `cargo install pitboard` for the command line on its own. The app is also a signed and
notarised download on the
[latest release](https://github.com/datlechin/pitboard/releases/latest) if you would rather
not use Homebrew.

## Set up

Add the account you are signed in as, then the others:

```sh
pitboard enroll personal
pitboard enroll work --sign-in
```

`--sign-in` runs Claude Code's own sign-in in a separate directory, so the login you are
using now stays put. Do not use `/login` to add an account. That replaces the login in use,
and pitboard cannot keep a copy of a login it did not see leave.

## Daily use

```sh
pitboard            # who is signed in, and what each account has left
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

Usage comes from Anthropic every time you run it, for all accounts at once. If a parked
login has expired, pitboard renews it first, so every account answers with live numbers. If
Anthropic cannot be reached, you get the last numbers pitboard saw, and when it saw them.

A Claude Code session that is already open picks up the switch within about 33 seconds. You
do not have to restart it.

Each account holds one parked login. Switching to an account uses that login up, and
switching away parks a fresh one. If one expires or goes missing, sign in to that account
again. The rest of what pitboard knows about it stays:

```sh
pitboard enroll work --sign-in
```

Other commands:

- `pitboard rename wrong right` fixes a label without signing in again.
- `pitboard forget work` drops an account.
- `pitboard doctor` checks everything pitboard depends on, including every parked login.

## Status line

`pitboard statusline` prints one line with the account in use and what every account has
left:

```text
personal 59%·73%  work 12%·40%
```

It reads what Claude Code passes on stdin plus pitboard's own files. It does not use the
network and does not touch a login. Add it to `~/.claude/settings.json`:

```json
{ "statusLine": { "type": "command", "command": "pitboard statusline" } }
```

To combine it with a status line of your own, pipe the same input to it:
`echo "$input" | pitboard statusline`.

## Menu bar app

The menu bar shows the account in use and its tightest limit. Open the panel to see every
account's limits and switch with one click. When an account runs out, the app says so once
and offers the account with the most left.

It calls the same core as the command line, so nothing else has to be installed. Usage is
read when you open the panel and every few minutes while the app runs. "Open at login" is
in the menu at the bottom right. A copy from a release keeps itself up to date. One you
build yourself does not, because it carries no update key:

```sh
./apple/scripts/build-app.sh
cp -R apple/build/Pitboard.app /Applications/
```

## Scripting

Every command takes `--json` and prints the same envelope, including on failure and for a
mistyped command line: `{v, command, ok, data, warnings, error}`. Error codes are stable.
Exit codes: 0 done, 1 not done, 2 command line wrong, 3 a login or Claude Code's files are
in a state pitboard will not act on.

Shell completions: `pitboard completions zsh` (or `bash`, `fish`, `elvish`, `powershell`).

## What it will not do

These are rules, not gaps:

- No switching by itself on any server signal.
- No pooling or proxying of requests, and no `ANTHROPIC_BASE_URL` interception.
- No failover when an account is on hold.
- No renewing of the login you are signed in as. That is Claude Code's job. pitboard
  renews only a login it parked itself.
- No export, import or sync of parked logins between machines.

The last one is a safety rule, not a missing feature. Each machine has to sign in to each
account itself. If a machine presents a refresh token another machine has already rotated,
Claude Code drops the login on both.

## What a switch costs you

None of these are pitboard's to fix, so they are listed plainly:

- Remote Control turns off for a conversation that was started under a different account.
  Claude Code does that when it sees the owner change.
- The prompt cache belongs to one account, so the first message after a switch rebuilds it.
  Measured on this project, that costs about the same as leaving a session idle for an
  hour, which a five-hour limit usually means has happened anyway.
- Usage for a parked account is read with its parked login, so work done on other machines
  counts too.

## How it works

pitboard asks Anthropic which account a login belongs to instead of trusting Claude Code's
config file, which can be a day behind the login it describes. It will not move a login it
cannot identify.

On macOS, Claude Code keeps its login in a keychain item that only `/usr/bin/security` is
trusted to read. pitboard reads and writes it with the same tool. Calling the Security
framework from another process changes that item's access list for good, and after that
every read Claude Code makes takes one to three seconds instead of a few milliseconds.
pitboard takes the same lock Claude Code takes around credential writes, and never writes
Claude Code's plaintext fallback file for you.

See [SECURITY.md](SECURITY.md) for where your credentials live and what pitboard protects
against.

## Licence

Apache-2.0. Not affiliated with Anthropic. See [NOTICE](NOTICE).
