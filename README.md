# Pitboard

Switch AI coding tools between your accounts.

[![CI](https://github.com/datlechin/pitboard/actions/workflows/ci.yml/badge.svg)](https://github.com/datlechin/pitboard/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/pitboard.svg)](https://crates.io/crates/pitboard)
[![Licence](https://img.shields.io/badge/licence-Apache--2.0-blue.svg)](https://github.com/datlechin/pitboard/blob/main/LICENSE)

<img src="https://raw.githubusercontent.com/datlechin/pitboard/main/.github/media/menu.png"
  alt="The Pitboard menu, offering first to sign in to an account again, then listing Claude
  Code and Codex accounts with a check mark on each one in use and, under each account's name,
  how much of its limits it has used or that its sign-in has expired"
  width="300">

Pitboard switches Claude Code, OpenAI's Codex CLI and, on macOS, Claude Desktop between your
own accounts without signing out, and shows what each Claude Code and Codex account has left.

It keeps the login of each account you are not using, called a parked login. A switch puts
the chosen account's parked login where the tool reads it. Your history, settings and
projects stay where they are.

[Open Claude Code sessions](https://docs.usepitboard.com/concepts/open-sessions) follow a
switch with no restart. Anything that runs Codex keeps the old account until it starts again.
Claude Desktop switches only while Claude is closed. The app offers to quit Claude, and the
command line refuses while it is open.

While the app is open, it notifies you once a limit of the account in use reaches 95%, and
offers another account with room left. Pitboard can also
[switch Claude Code by itself](https://docs.usepitboard.com/guides/automatic-switching), from
the app or from a background service that `pitboard watch install` sets up on macOS, Windows
and Linux.

The app also gives each Claude Code and Codex account its own
[window on claude.ai or chatgpt.com](https://docs.usepitboard.com/guides/account-windows).

Pitboard has no server of its own and no telemetry. It sends each login only to the service
that issued it, and sends nothing for a Claude Desktop account.
[What leaves your machine](https://docs.usepitboard.com/security#what-leaves-your-machine)
lists every request.

The command line runs on macOS, Windows and Linux. The app runs on macOS 14 or later and on
Windows 11 24H2 or later, and includes the command line. Pitboard for Windows is in beta, and
its files are not signed. On Linux, Pitboard builds and passes its tests, but has not been run
with real logins.

## Install

On macOS and Linux, install the command line with the script:

```sh
curl -fsSL https://usepitboard.com/install.sh | sh
```

Or with Homebrew:

```sh
brew install datlechin/tap/pitboard              # the command line
brew install --cask datlechin/tap/pitboard-app   # the app, on macOS
```

On Windows, download the installer from
[Download Pitboard](https://usepitboard.com/download#windows). It installs the app and puts
the command line on your `PATH`, for your Windows account alone.

The app includes the command line, so install one or the other.
[Install Pitboard](https://docs.usepitboard.com/install) covers each system, Cargo, release
files and updates. [Verify a download](https://docs.usepitboard.com/install/verify) checks a
file before you run it. The
[code signing policy](https://docs.usepitboard.com/install/code-signing-policy) says which
files are signed and who approves a signature.

Removing Pitboard without `pitboard uninstall` leaves its parked logins behind. Follow
[Remove Pitboard](https://docs.usepitboard.com/install/remove).

## Quickstart

```sh
pitboard enroll personal        # the account Claude Code is signed in to
pitboard enroll work --sign-in  # another one, through Claude Code's own sign-in
pitboard                        # what each account has left
pitboard use work               # switch Claude Code to work
```

In the app, choose the account in Pitboard's menu, from its item in the menu bar on macOS or
its icon in the notification area on Windows. The
[Quickstart](https://docs.usepitboard.com/quickstart) shows each step and what it prints. For
Codex, put `codex/` before each label, as
[Use Pitboard with Codex](https://docs.usepitboard.com/guides/codex) explains. For Claude
Desktop on macOS, put `desktop/`, as
[Use Pitboard with Claude Desktop](https://docs.usepitboard.com/guides/claude-desktop)
explains.

Add accounts only this way, never with Claude Code's `/login` or with `codex login`. Each
replaces the login in use without Pitboard parking it. The account that was in use then needs
a browser sign-in. Never choose **Log out** in Claude Desktop: it ends the login Claude holds
at claude.ai, and Pitboard cannot bring it back.

`pitboard doctor` checks what Pitboard relies on, and names a setting, such as a managed one,
that limits which organisation or workspace Claude Code or Codex signs in to. Pitboard refuses
to switch to an account such a setting rules out.

## Documentation

The documentation is at [docs.usepitboard.com](https://docs.usepitboard.com). Start with:

- [Commands](https://docs.usepitboard.com/reference/commands)
- [JSON output](https://docs.usepitboard.com/reference/json-output)
- [Security and privacy](https://docs.usepitboard.com/security)
- [Troubleshooting](https://docs.usepitboard.com/troubleshooting)

## Contributing

[CONTRIBUTING.md](https://github.com/datlechin/pitboard/blob/main/CONTRIBUTING.md) says how to
build Pitboard and submit a change. Report a bug with the
[bug report form](https://github.com/datlechin/pitboard/issues/new?template=bug.yml), and a
vulnerability privately, as
[SECURITY.md](https://github.com/datlechin/pitboard/blob/main/SECURITY.md) says. Everyone
taking part follows the
[code of conduct](https://github.com/datlechin/pitboard/blob/main/CODE_OF_CONDUCT.md).

## Licence

Pitboard is licensed under
[Apache-2.0](https://github.com/datlechin/pitboard/blob/main/LICENSE). It is not affiliated
with Anthropic or OpenAI, as [NOTICE](https://github.com/datlechin/pitboard/blob/main/NOTICE)
states.
