# pitboard

Switch between your own Claude Code and Codex logins, and see how much each one has left.

[![CI](https://github.com/datlechin/pitboard/actions/workflows/ci.yml/badge.svg)](https://github.com/datlechin/pitboard/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/pitboard.svg)](https://crates.io/crates/pitboard)
[![Licence](https://img.shields.io/badge/licence-Apache--2.0-blue.svg)](https://github.com/datlechin/pitboard/blob/main/LICENSE)

<img src="https://raw.githubusercontent.com/datlechin/pitboard/main/.github/media/menu.png"
  alt="The pitboard menu, listing Claude Code and Codex accounts, with a check mark on each
  one in use and, under each account's name, its usage or that it needs signing in again"
  width="300">

With two or more Claude or ChatGPT subscriptions, changing accounts in Claude Code or
OpenAI's Codex CLI means a sign-out and a browser sign-in. pitboard keeps the login of
each account you are not using, called a parked login. A switch puts the chosen account's
parked login where the tool reads it.

Only the account changes: your history, sessions, settings and projects stay where they are.
A Claude Code session that is already open picks up a switch within about 33 seconds. A
running `codex` keeps the old account until you restart it.

The menu bar app also gives each Claude Code account a
[claude.ai window](https://docs.usepitboard.com/guides/claude-web) of its own, all signed in
side by side, and opens a claude.ai link from any browser as the account you choose.

pitboard sends a login only to the service that issued it: Anthropic for Claude Code, OpenAI
for Codex. It has no server of its own and no telemetry, and parked logins stay on your
computer. For every request, including the app's update check on GitHub, see
[What leaves your machine](https://docs.usepitboard.com/security#what-leaves-your-machine).

pitboard is pre-release. macOS is tested. On Linux, pitboard builds and its tests pass, but
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
  compiler. Without `--locked`, Cargo ignores the `Cargo.lock` pitboard was released with
  and may pick newer dependencies.

If Homebrew installed pitboard up to 0.3.0, follow
[Upgrade from 0.3.0 or earlier](https://docs.usepitboard.com/install/upgrade-from-0-3-0).
From the old app cask, `brew update` replaces the app with the command line.

Run `pitboard uninstall` before you remove pitboard, as
[Remove pitboard](https://docs.usepitboard.com/install/remove) describes. Removing the
program alone leaves its parked logins, the daily renewal schedule and `~/.pitboard` behind.

## Quickstart

First enrol the account Claude Code is signed in to, then the others:

```sh
pitboard enroll personal        # the account Claude Code is signed in to
pitboard enroll work --sign-in  # another one, through Claude Code's own sign-in
pitboard                        # what each account has left
pitboard use work               # switch Claude Code to work
```

Before the switch, `pitboard` shows each account and what it has left. Once pitboard has
read an account a few times, a last line under it says how long the account lasts:

```text
● personal  me@example.com  signed in
    5h    ██████░░░░   59%  resets in 1h 10m
    week  ███████░░░   73%  resets in 5d 18h
          about 48m left at this rate

○ work      me@company.com  ready · good for 26d 4h
    5h    █░░░░░░░░░   12%  resets in 3h 02m
    week  ████░░░░░░   40%  resets in 2d 4h
          resets in 3h 02m
```

For Codex, put `codex/` before each label: `pitboard enroll codex/personal`,
`pitboard enroll codex/work --sign-in`, `pitboard use codex/work`.

Do not add an account with Claude Code's `/login` or with `codex login`. Both replace the
login in use without pitboard parking it, so the account that was in use needs a browser
sign-in again. `codex login` also revokes the login it replaces.

To switch in the menu bar app, click pitboard's item in the menu bar, then choose the
account. For each step and what it prints, see the
[Quickstart](https://docs.usepitboard.com/quickstart).

## Documentation

pitboard's documentation is at [docs.usepitboard.com](https://docs.usepitboard.com). It
includes these pages:

- [Quickstart](https://docs.usepitboard.com/quickstart)
- [Use pitboard with Codex](https://docs.usepitboard.com/guides/codex)
- [Show usage in Claude Code's status line](https://docs.usepitboard.com/guides/status-line)
- [Commands](https://docs.usepitboard.com/reference/commands)
- [JSON output](https://docs.usepitboard.com/reference/json-output)
- [Security and privacy](https://docs.usepitboard.com/security)
- [Troubleshooting](https://docs.usepitboard.com/troubleshooting)

`pitboard --help` lists the commands. After a Homebrew install, `man pitboard` describes
each one.

## Contributing

One person maintains pitboard, and everyone taking part follows the
[code of conduct](https://github.com/datlechin/pitboard/blob/main/CODE_OF_CONDUCT.md). To
report a bug, use the
[bug report form](https://github.com/datlechin/pitboard/issues/new?template=bug.yml).
[Report a bug](https://github.com/datlechin/pitboard/blob/main/CONTRIBUTING.md#report-a-bug)
says what it asks for and what is safe to paste.

For building pitboard, working on the app and submitting a change, see
[CONTRIBUTING.md](https://github.com/datlechin/pitboard/blob/main/CONTRIBUTING.md). Report a
security problem privately, as
[SECURITY.md](https://github.com/datlechin/pitboard/blob/main/SECURITY.md) describes, and
not in a public issue.

## Licence

pitboard is licensed under [Apache-2.0](https://github.com/datlechin/pitboard/blob/main/LICENSE).
It is not affiliated with Anthropic or OpenAI, as
[NOTICE](https://github.com/datlechin/pitboard/blob/main/NOTICE) states.
