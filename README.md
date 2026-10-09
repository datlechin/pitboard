# Pitboard

Switch between your own Claude Code and Codex logins, and see how much each one has left.

[![CI](https://github.com/datlechin/pitboard/actions/workflows/ci.yml/badge.svg)](https://github.com/datlechin/pitboard/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/pitboard.svg)](https://crates.io/crates/pitboard)
[![Licence](https://img.shields.io/badge/licence-Apache--2.0-blue.svg)](https://github.com/datlechin/pitboard/blob/main/LICENSE)

<img src="https://raw.githubusercontent.com/datlechin/pitboard/main/.github/media/menu.png"
  alt="The Pitboard menu, listing Claude Code and Codex accounts, with a check mark on each
  one in use and, under each account's name, its usage or that it needs signing in again"
  width="300">

With two or more Claude or ChatGPT subscriptions, changing accounts in Claude Code or
OpenAI's Codex CLI means a sign-out and a browser sign-in. Pitboard keeps the login of
each account you are not using, called a parked login. A switch puts the chosen account's
parked login where the tool reads it.

Only the account changes: your history, sessions, settings and projects stay where they are.
A Claude Code session that is already open picks up a switch within about 33 seconds, or,
while `~/.claude/.credentials.json` sits behind the keychain on macOS, when its login is next
renewed. A running `codex` keeps the old account until you restart it.

The menu bar app also gives each account its own
[window on claude.ai or chatgpt.com](https://docs.usepitboard.com/guides/account-windows),
signed in separately. It opens a page from your browser's Share menu as the account you
choose.

Pitboard sends a login only to the service that issued it: Anthropic for Claude Code, OpenAI
for Codex. It has no server of its own and no telemetry, and parked logins stay on your
computer. For every request, including the app's update check on GitHub, see
[What leaves your machine](https://docs.usepitboard.com/security#what-leaves-your-machine).

Pitboard is pre-release. macOS is tested. On Linux, Pitboard builds and its tests pass, but
it has not been run with a real signed-in Claude Code or Codex.

## Install

```sh
brew install datlechin/tap/pitboard            # the command line, macOS and Linux
brew install --cask datlechin/tap/pitboard-app # the menu bar app, macOS 14 or later
```

The app includes the command line and keeps both up to date, so install one or the other.
Neither needs Rust. Without Homebrew, use one of these:

- With [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) installed,
  `cargo binstall pitboard` fetches the release's command line for your machine.
- The [latest release](https://github.com/datlechin/pitboard/releases/latest) has the
  command line for macOS and Linux, on x86_64 and aarch64, and the app. To check a file, use
  `gh attestation verify <file> --repo datlechin/pitboard`, as
  [Verify a download](https://docs.usepitboard.com/install/verify) explains.
- `cargo install --locked pitboard` builds it from source, with Rust 1.91 or later and a C
  compiler. Without `--locked`, Cargo ignores the `Cargo.lock` Pitboard was released with
  and may pick newer dependencies.

If Homebrew installed Pitboard up to 0.3.0, follow
[Upgrade from 0.3.0 or earlier](https://docs.usepitboard.com/install/upgrade-from-0-3-0).
From the old app cask, `brew update` replaces the app with the command line.

Run `pitboard uninstall` before you remove Pitboard, as
[Remove Pitboard](https://docs.usepitboard.com/install/remove) describes. Removing the
program alone leaves its parked logins, the daily renewal schedule and `~/.pitboard` behind.

## Quickstart

First enrol the account Claude Code is signed in to, then the others:

```sh
pitboard enroll personal        # the account Claude Code is signed in to
pitboard enroll work --sign-in  # another one, through Claude Code's own sign-in
pitboard                        # what each account has left
pitboard use work               # switch Claude Code to work
```

Before the switch, `pitboard` shows each account and what it has left. Each limit says how
its use compares with an even pace, and the account in use says which limit runs out first:

```text
● personal  me@example.com  signed in
    5h    ██████░│░░   59%  resets in 1h 10m  18% under pace
    week  █│█████░░░   73%  resets in 5d 18h  55% over pace
          weekly limit runs out in 11h 05m at this pace

○ work      me@company.com  ready · good for 26d 4h
    5h    █░░│░░░░░░   12%  resets in 3h 02m  27% under pace
    week  ████░░│░░░   40%  resets in 2d 4h   29% under pace
```

For Codex, put `codex/` before each label: `pitboard enroll codex/personal`,
`pitboard enroll codex/work --sign-in`, `pitboard use codex/work`.

Do not add an account with Claude Code's `/login` or with `codex login`. Both replace the
login in use without Pitboard parking it, so the account that was in use needs a browser
sign-in again. `codex login` also revokes the login it replaces.

To switch in the menu bar app, click Pitboard's item in the menu bar, then choose the
account. For each step and what it prints, see the
[Quickstart](https://docs.usepitboard.com/quickstart).

## Documentation

Pitboard's documentation is at [docs.usepitboard.com](https://docs.usepitboard.com). It
includes these pages:

- [Quickstart](https://docs.usepitboard.com/quickstart)
- [Use Pitboard with Codex](https://docs.usepitboard.com/guides/codex)
- [Show usage in Claude Code's status line](https://docs.usepitboard.com/guides/status-line)
- [Commands](https://docs.usepitboard.com/reference/commands)
- [JSON output](https://docs.usepitboard.com/reference/json-output)
- [Security and privacy](https://docs.usepitboard.com/security)
- [Troubleshooting](https://docs.usepitboard.com/troubleshooting)

`pitboard --help` lists the commands. After a Homebrew install, `man pitboard` describes
each one.

## Contributing

One person maintains Pitboard, and everyone taking part follows the
[code of conduct](https://github.com/datlechin/pitboard/blob/main/CODE_OF_CONDUCT.md). To
report a bug, use the
[bug report form](https://github.com/datlechin/pitboard/issues/new?template=bug.yml).
[Report a bug](https://github.com/datlechin/pitboard/blob/main/CONTRIBUTING.md#report-a-bug)
says what it asks for and what is safe to paste.

For building Pitboard, working on the app and submitting a change, see
[CONTRIBUTING.md](https://github.com/datlechin/pitboard/blob/main/CONTRIBUTING.md). Report a
security problem privately, as
[SECURITY.md](https://github.com/datlechin/pitboard/blob/main/SECURITY.md) describes, and
not in a public issue.

## Licence

Pitboard is licensed under [Apache-2.0](https://github.com/datlechin/pitboard/blob/main/LICENSE).
It is not affiliated with Anthropic or OpenAI, as
[NOTICE](https://github.com/datlechin/pitboard/blob/main/NOTICE) states.
