# Contributing

One person maintains pitboard. Everyone taking part follows the
[code of conduct](CODE_OF_CONDUCT.md). Send a change as a pull request against `main`. CI
runs on every pull request. To report a security problem, follow [SECURITY.md](SECURITY.md) instead
of opening an issue. [ARCHITECTURE.md](ARCHITECTURE.md) describes how the code is organised
and the measured facts it rests on. [RELEASING.md](RELEASING.md) describes how a release is
made.

## Report a bug

Open an issue with the **Something went wrong** form. It asks for what happened, the output
of `pitboard doctor --json` and the versions you run. The last lines of
`~/.pitboard/audit.log` are optional.

`pitboard doctor --json` prints no token, email address or account identifier: each becomes
a short digest. Paths under your home start with `~`. Plain `pitboard doctor` shows your own
account, so read that one yourself and paste the JSON.

Read audit lines before you paste them. A `reclaim` line with the outcome `discarded` names
a parked login, and that name contains an account id.

## Set up

`rust-toolchain.toml` selects the stable Rust channel, with rustfmt and clippy. Run
`rustup toolchain install` in the repository to install it. The oldest Rust the crates
support is 1.91, the `rust-version` in `Cargo.toml`, and CI runs `cargo check` on the
workspace with it.

Checking a change uses these tools as well:

- `cargo-deny`, for `cargo deny check`.
- `cargo-insta`, for `cargo insta review`.
- `cargo-zigbuild` and Zig, to lint the Linux code from a Mac. Rust also needs the
  `x86_64-unknown-linux-gnu` target for that.

To work on the app, you need a Mac with Xcode, and its Swift must be 6.2 or later, as
`apple/Package.swift` asks. Rust needs both Mac targets, as [The app](#the-app) shows.

## Rules

1. Red before green. Before changing behaviour, write or find a test that fails against
   the current code. Then watch that same test pass. A test that has never failed has
   proved nothing. pitboard once shipped one that passed whether the code under it worked
   or not.

2. Compiling is not evidence that an edit applied. An edit that did nothing leaves the old
   code in place, and the old code compiles. Read the region again right before changing
   it, and look at the diff after. An empty or surprisingly small diff is the symptom.

3. Never write to a keychain item that holds a real login. Each test names its items after
   itself and calls `common::guard_not_live` before the first write. A Codex test that
   writes points `CODEX_HOME` at a scratch directory and never writes to `~/.codex`. The
   one ignored test that reads a real `auth.json` only reads it. Nothing runs `codex login`
   or `codex logout` against a real home, because both revoke the login stored there.

4. Measure the tool, do not guess at it. Claude Code's behaviour here is undocumented,
   Codex's moves with its source, and both ship several times a week. A claim about either
   needs an experiment or a reading of a named build. It belongs in a test, the tool's
   register or the commit message. [Tool registers](#tool-registers) says how to add a
   fact, and [ARCHITECTURE.md](ARCHITECTURE.md#tool-registers) says what a register is.

## Check a change

CI runs these on every pull request. Run them before you push:

```sh
cargo fmt --check
cargo clippy --all-targets --locked
cargo test --locked -- --skip writing_preserves_attributes
cargo deny check
```

CI sets `RUSTFLAGS=-D warnings`, so any compiler or Clippy warning fails it.

The skipped test, `writing_preserves_attributes_and_does_not_slow_later_reads`, exists only
on macOS. It asserts a read-latency threshold, and other `security` calls running at the
same time push reads over it. So on macOS, run it on its own:

```sh
cargo test --locked -p pitboard --test keychain_write_is_harmless writing_preserves_attributes
```

Some code compiles only on Linux. CI lints it on Linux. To lint it from a Mac before you
push, run:

```sh
cargo-zigbuild clippy --target x86_64-unknown-linux-gnu --all-targets
```

The snapshots in `crates/pitboard/tests/snapshots` pin the `--json` contract. A snapshot
changes only when the contract changes on purpose. Review the difference with
`cargo insta review`, and say in the pull request why the contract moved.

CI also runs:

- clippy and the tests on both macOS and Linux
- `cargo check --workspace --all-targets --locked` on Rust 1.91
- the app job: `swift format lint --strict`, `./apple/scripts/build-app.sh`, a check that
  the command line inside the app runs and holds both architectures, `swift test` and the
  UI tests
- `cargo semver-checks --package pitboard-core`, which reports and does not block
- a guard that fails when `.github/workflows/rotation.yml` names a repository secret

## The app

The Swift package in `apple/` links the core as `apple/PitboardFFI.xcframework`, with
bindings generated into `apple/Sources/PitboardBindings`. Neither is committed. Build them
before you open the project the first time, and again whenever the core changes. They are
built for both kinds of Mac, so Rust needs both targets:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
./apple/scripts/build-xcframework.sh
open apple/Pitboard.xcodeproj
```

Everything the app does is in the package, and its tests run without starting the app:

```sh
swift test --package-path apple
```

The UI tests start the debug build, each in a fixture. A fixture is a machine in a known
state, where nothing reaches the keychain, the network or your accounts. Run the UI tests
from Xcode with **Product** > **Test**, or with:

```sh
xcodebuild test -project apple/Pitboard.xcodeproj -scheme Pitboard -destination 'platform=macOS'
```

macOS asks for a password before a UI test can drive the app, unless the Mac allows
Automation Mode without one. GitHub's macOS runners allow it. `automationmodetool` prints
which applies to your Mac.

The debug build's bundle identifier is `com.usepitboard.Pitboard.debug`, so it never shares
preferences, a login item or notification permission with a copy you have installed. Run
from Xcode, it reads this Mac's accounts, as that copy does. To run it in a fixture instead,
add `PITBOARD_FIXTURE=twoTools` to the scheme's environment variables. The fixtures are the
cases of `Fixture` in `apple/Sources/PitboardApp/Fixture/Fixture.swift`.

In a fixture, a claude.ai window loads a stand-in page served under
`pitboard-fixture://claude.ai`, keeps its data in memory and opens no link outside it, so
nothing reaches claude.ai. A debug build run without a fixture loads the real claude.ai,
into stores of its own under `~/Library/WebKit/com.usepitboard.Pitboard.debug`.

The debug build claims `pitboard-debug://` rather than `pitboard://`, shows as
**pitboard Debug** in Share menus and as **Open in pitboard Debug** in the Services menu,
once it has been built. A link or share meant for an installed copy never reaches a build
from a branch. The Share extension is the `PitboardShare` target, from
`apple/ShareExtension`; it links `PitboardLinks` and nothing else of the app's.

`./apple/scripts/build-app.sh` builds the release bundle the way CI and a release do.

CI checks the format of the Swift written by hand, leaving out the generated bindings:

```sh
swift format lint --strict --recursive --configuration apple/.swift-format \
  apple/Sources/PitboardApp apple/Sources/PitboardKit apple/Sources/PitboardLinks \
  apple/App apple/ShareExtension apple/UITests apple/Tests apple/scripts .github/scripts
```

## Tool registers

Each tool's register is `crates/pitboard-core/src/provider/<tool>/assumptions.rs`. What a
register is for, when CI checks each one and what adding a tool takes are in
[Tool registers in ARCHITECTURE.md](ARCHITECTURE.md#tool-registers).

Each fact names the literals it can be read by. `pitboard-conformance` looks for them in a
build of the tool and says which are still there:

```sh
cargo run -p pitboard-conformance -- <claude binary>
cargo run -p pitboard-conformance -- <codex binary> --provider codex
```

Add `--json` for a report a program can read. The checker exits 1 when a fact has moved: a
literal it needs is gone, or one it rules out has turned up. It exits 2 when it cannot make
sense of its arguments or read the binary.

Point it at the tool's native binary, not the npm wrapper, which carries no binary. The
checker reads the binary's header to tell a macOS build from a Linux one, and reads each
fact only from the builds the register's `read_on` names for it: Claude Code's Linux build
has no keychain code, so the keychain facts are read from its macOS build.
`.github/workflows/conformance.yml` takes Claude Code's builds from the packages
`@anthropic-ai/claude-code-linux-x64` and `@anthropic-ai/claude-code-darwin-arm64`. It takes
Codex's from `vendor/` in `@openai/codex@<version>-linux-x64`.

To add a fact, add an `Assumption` to the tool's register:

| Field | What it holds |
| --- | --- |
| `name` | A stable snake_case code |
| `fact` | What pitboard believes |
| `read_from` | Where in the tool the fact was read, so it can be read again |
| `verified_against` | The build it was read from, such as `2.1.284` |
| `depends` | What in pitboard stops being true if the fact moves |
| `probe` | Literals that must be in a build for the fact to still be readable there |
| `absent` | Literals whose arrival would disprove the fact |

A fact that only one system's build can be read for, such as one about the macOS keychain,
goes in that register's `MACOS_ONLY` or `LINUX_ONLY` list; every other fact is read from
both.

Pick literals specific to the fact. A literal already in the build for another reason
proves nothing. A fact about behaviour has no literal to find. It gets an empty `probe`,
and the run reports it as not readable.

Run the checker against the build you read the fact from. If you can, run it against an
older build that predates the fact too, and watch it go red.

`cargo test` checks the registers themselves. Every name must be unique and snake_case.
Every fact must say what it is, where it was read, which version and what depends on it.

## Documentation

`docs/` is the source of [docs.usepitboard.com](https://docs.usepitboard.com).
[docs/README.md](docs/README.md) says how to preview, check and publish a change, and
[docs/AGENTS.md](docs/AGENTS.md) holds the writing rules.

## Changelog

Record a change that someone using pitboard would notice in [CHANGELOG.md](CHANGELOG.md),
under `## [Unreleased]`. The file follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).

- Put each entry under one of six types, in this order: `### Added`, `### Changed`,
  `### Deprecated`, `### Removed`, `### Fixed`, `### Security`.
- Write one change per bullet: what changed for the person using pitboard, and why.
- Say in words when a change breaks something that worked before.
- Put a security fix under `### Security`.
- Make links inside an entry inline and absolute. A release's notes are cut from its
  section, without the link definitions at the end of the file.

The maintainer adds the version heading when making a release, as
[RELEASING.md](RELEASING.md) describes.

## Submit a change

A pull request against `main` needs all of the following:

- CI passes.
- A change in behaviour comes with a test that failed before the change, as
  [Rules](#rules) asks.
- A changed snapshot comes with the reason the `--json` contract moved.
- A dependency you add comes with a reason, and `cargo deny check` passes. It checks
  advisories, licences, bans and sources against `deny.toml`.
- `CHANGELOG.md` has an entry, if someone using pitboard would notice the change.

Since 0.3.0, most commit subjects are one present-tense sentence saying what pitboard does
after the change, with no prefix. An example is "The man page sets out every command instead
of naming pages that are not installed". The body says why, and what was measured, if
anything was.
