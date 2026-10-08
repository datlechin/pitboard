# AGENTS.md

Pitboard is a Rust command line and a native macOS menu bar app that switch between a
person's own Claude Code and Codex logins. It moves real OAuth logins, so a mistake here
can sign someone out of a paid account. [CONTRIBUTING.md](CONTRIBUTING.md) is the full
guide; this file is what an agent needs before it touches anything.

## Never

- Never write to, overwrite or delete a keychain item that holds a real login, such as
  `Claude Code-credentials`. Tests name their items after themselves and call
  `common::guard_not_live` before the first write.
- Never run `claude`, `codex login`, `codex logout` or `claude auth login` against a real
  home: they replace or revoke the login stored there.
- Never run a `pitboard` built from a branch, or the built app, against your real home.
  Point `HOME`, `CODEX_HOME` and `CLAUDE_CONFIG_DIR` at a fresh directory first, and build
  before you set them, since Cargo and rustup read `HOME` too.
- Never write to `~/.claude`, `~/.claude.json`, `~/.codex` or `~/.pitboard` from a test
  or a script.
- Never print a token. `pitboard doctor --json` hides them; raw files do not.
- Tests never touch launchd, systemd, `/usr/local/bin`, `/Applications` or
  `~/Library/LaunchAgents`.
- Never load claude.ai or chatgpt.com, or sign in to either, from a test, a script or a
  branch build. A fixture's account windows load stand-in pages on `pitboard-fixture://`
  and keep their data in memory. Never read, copy or delete anything under
  `~/Library/WebKit/com.usepitboard.Pitboard`, where the account windows keep real
  sign-ins.
- Never open a `pitboard://` or `pitboard-debug://` link against a real home: it starts
  whichever copy claims the scheme, with the real home. UI tests send `pitboard-debug://`
  links only to a debug build they launched in a fixture.
- Never leave a local build of the app registered with macOS. Its scheme and Share
  extension stay registered after a build with Xcode, `xcodebuild` or `build-app.sh`:
  `pitboard-debug://` and **Pitboard Debug** for a debug build, `pitboard://` and
  **Pitboard** for a release. A link or a share can then start that branch build against
  the real home. Unregister it with `pluginkit -r` on each `PitboardShare.appex`, then
  `lsregister -u` on each `Pitboard.app`; [The app](CONTRIBUTING.md#the-app) has the
  commands.

### On Windows

Pitboard for Windows is being built. [Windows](CONTRIBUTING.md#windows) says how to build
and test it there.

- Never write to, overwrite or delete a Credential Manager item that holds a real login:
  `Claude Code-credentials` and every `Claude Code-credentials-<hash>`, each piece of them
  included, Codex's `cli|*` and `secrets|*` targets, and anything under
  `Codex MCP Credentials`. Never use `cmdkey /delete`, `vaultcmd` or PowerShell's credential
  modules on those names. Tests write only `pitboard-citest-*` items or the slot hashed from
  their own folder, `common::guard_not_live` refuses the rest by pattern, and CI lists those
  names after each Windows run and fails on any left.
- Never read, copy, rewrite or delete `%USERPROFILE%\.claude`, `%USERPROFILE%\.claude.json`,
  `%USERPROFILE%\.codex`, `%ProgramData%\OpenAI\Codex`, `C:\Program Files\ClaudeCode`, or
  the `%LOCALAPPDATA%\Pitboard` of an account that holds real logins.
- Never run a `pitboard.exe` built from a branch on a Windows account that holds real
  logins, and never put one on a `PATH`, in `%LOCALAPPDATA%\Programs`, in WinGet's Links or
  behind a Scoop shim. A test or a script points `USERPROFILE`, `HOME`, `APPDATA`,
  `LOCALAPPDATA`, an existing `CODEX_HOME`, `CLAUDE_CONFIG_DIR` and `PITBOARD_HOME` at fresh
  folders, after building, since Cargo and rustup find their own homes through
  `USERPROFILE`.
- Never weaken or get around the release gate in a build anybody installs: no release,
  script or package passes `--cfg pitboard_unreleased_windows`, `test-support` or
  `fixture`. A Windows build of a 0.x release answers only `--version`, `--help`,
  `completions` and `manpage`, and changes nothing.
- Tests never touch Task Scheduler, the registry, the user `PATH` or DPAPI keys, and never
  run elevated on a machine that holds logins.

## Check a change

```sh
cargo fmt --check
cargo clippy --all-targets --locked
cargo test --locked -- --skip writing_preserves_attributes
cargo deny check
```

For the app, build the core's bindings once, with the fixtures its debug build, its UI tests
and one of its unit tests launch into, then run its unit tests:

```sh
./apps/macos/scripts/build-xcframework.sh --fixture
swift test --package-path apps/macos
```

CI sets `RUSTFLAGS=-D warnings`. [Check a change](CONTRIBUTING.md#check-a-change) lists
everything else CI runs, and [The app](CONTRIBUTING.md#the-app) covers the Xcode project,
fixtures and UI tests.

## Rules

- A behaviour change starts with a test that fails against the current code.
- A claim about Claude Code or Codex needs a reading of a named build or an experiment,
  recorded in the tool's register (`crates/pitboard-core/src/provider/*/assumptions.rs`),
  a test or the commit message. [Tool registers](CONTRIBUTING.md#tool-registers) says how.
- A change people would notice gets an entry under `## [Unreleased]` in
  [CHANGELOG.md](CHANGELOG.md).
- A commit subject is one present-tense sentence saying what Pitboard does after the
  change, with no prefix. The body says why, and what was measured.

## Where things are

- [ARCHITECTURE.md](ARCHITECTURE.md): the code map, invariants and measured facts.
- [RELEASING.md](RELEASING.md): how a release is made. Do not push tags.
- `docs/`: the documentation site; [docs/AGENTS.md](docs/AGENTS.md) has its writing rules.
