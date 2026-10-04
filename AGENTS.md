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
  Point `HOME`, `CODEX_HOME`, `CLAUDE_CONFIG_DIR` and `PITBOARD_CLAUDE_DESKTOP_DIR` at a
  fresh directory first, and build before you set them, since Cargo and rustup read `HOME`
  too.
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
- Never move, write or delete anything in `~/Library/Application Support/Claude`, and never
  write the `Claude Safe Storage` keychain item. Tests point `PITBOARD_CLAUDE_DESKTOP_DIR`
  at a scratch directory and call `common::guard_not_live_dir` before the first write.
- Never read `Claude Safe Storage` from a test or a script; tests stand in a scripted key
  in its place.
- Never start, quit or sign out of the Claude app from a test.

## Check a change

```sh
cargo fmt --check
cargo clippy --all-targets --locked
cargo test --locked -- --skip writing_preserves_attributes
cargo deny check
```

For the app, build the core's bindings once, then run its unit tests:

```sh
./apps/macos/scripts/build-xcframework.sh
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
