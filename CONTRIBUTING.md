# Contributing

One person maintains Pitboard. Everyone taking part follows the
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
`apps/macos/Package.swift` asks. Rust needs both Mac targets, as [The app](#the-app) shows.

## Rules

1. Red before green. Before changing behaviour, write or find a test that fails against
   the current code. Then watch that same test pass. A test that has never failed has
   proved nothing. Pitboard once shipped one that passed whether the code under it worked
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

The fixtures, the worlds the apps' debug builds launch into, compile only with
`pitboard-ffi`'s `fixture` feature, so their tests and their lints are run with it:

```sh
cargo test --locked -p pitboard-ffi --features fixture
cargo clippy -p pitboard-ffi --all-targets --locked --features fixture
```

Every program a test starts in place of `claude`, `codex` or any other tool is one compiled
stand-in, the `pitboard` crate's example `stand-in`, which plays a script written beside it,
the same on every system. `cargo test` with no target named, and `cargo test -p pitboard`,
build it. Neither a test run alone with `--test` nor a run of `pitboard-ffi`'s tests does, so
build it first, with the `--release` or `--target` the tests use:

```sh
cargo build --locked -p pitboard --example stand-in
cargo test --locked -p pitboard --test switch_round_trip
```

A test that does not find it fails, and its message names that command.

The readFailure world makes its account index unreadable with file modes, so its tests fail,
saying why, where the user can read a file of mode `000`: as root, in a container running as
root, or on a file system without Unix modes. So do two of the core's tests that make a
folder read-only for a write to fail:
`a_renewal_that_cannot_record_its_answer_keeps_it_for_the_next_run` and
`a_schedule_that_cannot_be_taken_away_leaves_every_login_where_it_was`.

Pitboard changes nothing as root or under `sudo`, and a test that runs the real machine runs
as whoever runs the tests. So `cargo test` run as root, as in a development container whose
only user is root, fails most of the integration tests in `crates/pitboard/tests`, with
`elevated`. Most set up their accounts by running `pitboard enroll`, and the output of
`status`, `doctor` and the status line gains the `read_only` warning or the failing
`elevated` check. On macOS `keychain_write_is_harmless` fails too. Three tests in
`crates/pitboard-ffi/src/launch.rs`, which make the app's core on the real machine as the
app does, fail with `elevated`. The other unit tests and the fixtures run on `MemoryHost`,
which runs as the person unless a test says otherwise. Run the tests as a user of your own,
never as root on a machine that holds logins. On Windows the same goes for an elevated
terminal: [Windows](#windows) says how the tests run there.

A unit test reaches no real home. `Context::for_unit_test` points every home, `HOME`,
Pitboard's directory, Claude Code's config directory and Codex's, at one folder of that
test's own under the temporary directory, which nothing makes; a test that writes makes
a scratch home of its own. A unit test that asks the passwd database for the account's
home panics, unless it tests that lookup and keeps what
`host::user::testing::reaching_the_real_home` returns while it does. Any other build for
tests, such as the `pitboard` the integration tests run, gets no home from there, so a
command run there without `HOME` is refused with `home_not_absolute`.

The snapshots in `crates/pitboard/tests/snapshots` pin the `--json` contract. A snapshot
changes only when the contract changes on purpose. Review the difference with
`cargo insta review`, and say in the pull request why the contract moved.

CI also runs:

- clippy and the tests on both macOS and Linux, and both again for `pitboard-ffi` with its
  fixtures
- `cargo check --workspace --all-targets --locked` on Rust 1.91, and again for `pitboard-ffi`
  with its fixtures, on Linux and on Windows
- the Windows job, on x64 and ARM64: the release gate proved on a build made as a release is,
  then clippy and the tests [Windows](#windows) lists
- the app job: `swift format lint --strict`, `./apps/macos/scripts/build-app.sh`, a check that
  the command line inside the app runs and holds both architectures, then
  `./apps/macos/scripts/build-xcframework.sh --fixture`, `swift test` and the UI tests
- the C# job: the core's C# bindings generated, compiled and called against the core on
  Linux, as the Windows app will call them, then against the core built with its fixtures,
  whose bindings must be the same and whose model a test starts
- `cargo semver-checks --package pitboard-core`, which reports and does not block
- a guard that fails when `.github/workflows/rotation.yml` names a repository secret

## The app

The Swift package in `apps/macos/` links the core as `apps/macos/PitboardFFI.xcframework`, with
bindings generated into `apps/macos/Sources/PitboardBindings`. The Share extension links
`pitboard-share-ffi` instead, the check of a shared link and nothing of the core, as
`apps/macos/PitboardShareFFI.xcframework` with bindings in
`apps/macos/Sources/PitboardShareBindings`. None of these is committed. Build them before you
open the project the first time, and again whenever the core or `pitboard-sites` changes. They
are built for both kinds of Mac, so Rust needs both targets.

The Xcode project is generated from `apps/macos/project.yml` by
[XcodeGen](https://github.com/yonaskolb/XcodeGen), and only its `Package.resolved` is
committed. Change `project.yml`, never the project, and generate it again after you do:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
brew install xcodegen
./apps/macos/scripts/build-xcframework.sh --fixture
xcodegen generate --spec apps/macos/project.yml
open apps/macos/Pitboard.xcodeproj
```

The app shows what the core's model says and sends it what was asked: every rule is Rust's,
in `crates/pitboard-ffi`, and the Swift is views and what only macOS can do. Everything the
app does is in the package, and its tests run without starting the app:

```sh
swift test --package-path apps/macos
```

They never make the model over this Mac. Most hand the app's model snapshots from a
stand-in, and one starts a fixture's model to see the bindings carry a snapshot end to end;
against a library built without fixtures it is skipped, and says so.

The UI tests start the debug build, each in a fixture. A fixture is a machine in a known
state, where nothing reaches the keychain, the network or your accounts. Run the UI tests
from Xcode with **Product** > **Test**, or with:

```sh
xcodebuild test -project apps/macos/Pitboard.xcodeproj -scheme Pitboard -destination 'platform=macOS'
```

macOS asks for a password before a UI test can drive the app, unless the Mac allows
Automation Mode without one. GitHub's macOS runners allow it. `automationmodetool` prints
which applies to your Mac.

The debug build's bundle identifier is `com.usepitboard.Pitboard.debug`, so it never shares
preferences, a login item or notification permission with a copy you have installed. Run
from Xcode, it reads this Mac's accounts, as that copy does. To run it in a fixture instead,
add `PITBOARD_FIXTURE=twoTools` to the scheme's environment variables.

The fixtures are Rust's, one set of worlds either app can launch into, in
`crates/pitboard-ffi/src/fixture`: `twoTools`, `oneTool`, `empty`, `firstLaunch`,
`noClaudeCode`, `unnamed`, `onlyOne`, `readFailure`, `stuck` and `chatGPTOpen`, as
`fixture_names()` gives them. Each is the real core over a machine of its own: a home in the
folder `pitboard-fixture` in the temporary directory, `TMPDIR` where it is set, the
keychain, the process list and the scheduler in memory, and Anthropic and OpenAI answering
from a script. Its accounts were put there by the core, signed in, enrolled, parked and
switched, so a world shows what the core makes of it: `readFailure`'s reads fail because
its account index is a file nobody may read, and `stuck`'s read says an interrupted switch
is waiting. Each tool's sign-in is played as its register says it behaves, so the fixture's
Claude Code, like the real one, refuses a code typed back that is not `<code>#<state>`:
type one such as `fixture-code#state`. The app adds only what is macOS's own: a login item
that registers nothing, and a link to the command line inside the fixture's stand-in app,
made in the folder's `bin` with no password. Only a library built with the `fixture`
feature has the worlds, which `build-xcframework.sh --fixture` builds; the bindings are the
same either way. A debug build linked against a library without it stops at launch, saying
how to build one with it. `build-app.sh` never passes it, and fails a build whose library
or app holds a fixture.

Only the UI tests launch into that folder. The unit tests leave it alone: the C# and Swift
tests of the bindings launch with `PitboardModel::fixture_in` into a temporary directory of
their own, which they remove, and the Rust test of `PitboardModel::fixture` runs again in a
child given one, and launches into the folder there. So a debug build launched into a
fixture keeps its world while `cargo test`, `swift test` or `dotnet test` runs, and several
runs of each can go at once.

In a fixture, an account's window loads a stand-in page for its site, such as
`pitboard-fixture://claude.ai`, and each sign-in host has a stand-in on the same scheme. Its
data stays in memory, its records are in the fixture's folder, and a link it would hand to
macOS is recorded instead, so nothing reaches either site. Run without a fixture, the debug
build loads the real sites, into stores of its own under
`~/Library/WebKit/com.usepitboard.Pitboard.debug`, and keeps their records in
`~/Library/Application Support/com.usepitboard.Pitboard.debug`.

The unit tests load no page. What a window decides, its navigation policy, its stores and
their records, its menus, the account picker and its downloads, is the core's, tested in
Rust; the Swift tests cover what the app does with it. The UI tests cover what its pages
do, in a fixture: sign-in windows, Google's sign-in being stopped, downloads, Find,
**Remove Website Data** and links shared to the **Open Link** window.

The debug build claims `pitboard-debug://` rather than `pitboard://`, and its Share
extension shows as **Pitboard Debug**. So a debug build never answers a link or a share
meant for an installed copy. A release build from `build-app.sh` claims `pitboard://` and
shows as **Pitboard**, like the installed copy. Building the app registers its scheme and
its Share extension with macOS, and they stay registered until you unregister them. After
building the app locally, from Xcode, with `xcodebuild` or with `build-app.sh`, unregister
each copy it built, and leave the one in `/Applications` alone:

```sh
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
"$lsregister" -dump | grep -E '^path:.*(Pitboard\.app|PitboardShare\.appex)'
pluginkit -r <build>/Pitboard.app/Contents/PlugIns/PitboardShare.appex
"$lsregister" -u <build>/Pitboard.app
```

The `-dump` line lists the copies macOS knows. `pluginkit -m | grep usepitboard` lists the
Share extensions it still offers.

`./apps/macos/scripts/build-app.sh` builds the release bundle the way CI and a release do.

CI checks the format of the Swift written by hand, leaving out the generated bindings:

```sh
swift format lint --strict --recursive --configuration apps/macos/.swift-format \
  apps/macos/Sources/PitboardApp apps/macos/Sources/PitboardKit apps/macos/Sources/PitboardLinkTarget \
  apps/macos/App apps/macos/ShareExtension apps/macos/UITests apps/macos/Tests apps/macos/scripts .github/scripts
```

### The C# bindings

The Windows app reaches the core through C# bindings, generated from `pitboard-ffi` like the
Swift ones and not committed. They need the [.NET 10 SDK](https://dotnet.microsoft.com/download),
and run on macOS and Linux as well as Windows, so they can be checked on any machine:

```sh
cargo build --locked -p pitboard-ffi
mkdir -p apps/windows/Pitboard.Core/Generated
cargo run --locked -p uniffi-bindgen-csharp -- --library target/debug/libpitboard_ffi.a \
  --out-dir apps/windows/Pitboard.Core/Generated --config crates/pitboard-ffi/uniffi.toml --no-format
cd apps/windows
dotnet test --solution Pitboard.slnx -p:PitboardNativeLibrary="$PWD/../../target/debug/libpitboard_ffi.dylib"
```

The bindings are read from the static library, which keeps what the generator reads on
every system; a release build strips it from the shared library on Linux. The shared
library the tests load is `libpitboard_ffi.so` on Linux and `pitboard_ffi.dll` on Windows. A record
field may not share its record's name, because C# makes each field a member of the record.

One test starts a fixture's model and waits for it to tell a listener written in C# of its
accounts. Against a library built without the `fixture` feature it is skipped, and says so.
To run it, build the library with `cargo build --locked -p pitboard-ffi --features fixture`
before `dotnet test`; the bindings need not be generated again.

## Windows

Pitboard for Windows is being built, a pull request at a time, and reaches people as 1.0.0.
Until then a Windows build of a 0.x release compiles, answers only `--version`, `--help`,
`completions` and `manpage`, refuses everything else with `windows_not_released`, and
changes nothing. The gate is `pitboard_core::release`: the core asks it at the gate every
change passes and wherever it reads the account list, the app's core before every read the
model makes, and the command line before every other command. Only `x86_64-pc-windows-msvc`
and `aarch64-pc-windows-msvc` compile.

A build for working on Pitboard on Windows is opened to it before its release with
`--cfg pitboard_unreleased_windows` in `RUSTFLAGS`, and with nothing else. It is not a Cargo
feature, because crates.io lists a published crate's features and anybody could turn one on
with `cargo install`. No release, script or package ever passes it. The core's own unit
tests are opened by `cfg(test)`. An opened build reaches the Windows face,
`crates/pitboard-core/src/host/windows`. It reads this process's token and the build of
Windows, and refuses every change from an elevated terminal (`elevated`) or on a build older
than Windows 11 24H2's 26100 (`system_too_old`), which Windows Server 2025 shares. As the
person, on 24H2 or later, a change passes that gate and meets the parts not built yet, which
refuse through errors their callers already have: no file is made, no process listed and no
store read.

On Windows 11 24H2 or later, x64 or ARM64, with Rust's MSVC toolchain, and on ARM64 the
clang ring compiles its C with there, follow [AGENTS.md](AGENTS.md#on-windows), then run, in
PowerShell from a terminal that is not elevated (not Run as administrator), as a Windows
account that holds no real login:

```powershell
$env:RUSTFLAGS = "-D warnings -C target-feature=+crt-static --cfg pitboard_unreleased_windows"
cargo clippy --workspace --all-targets --locked
cargo clippy -p pitboard-ffi --all-targets --locked --features fixture
cargo test --locked -p pitboard-core --lib -- host::windows host::token host::tests::the_floor release
cargo test --locked -p pitboard --test windows_refuses
cargo test --locked -p pitboard-sites -p pitboard-share-ffi
```

`RUSTFLAGS` replaces every target's own rustflags, so it names the static C runtime the
releases are built with. Until the integration tests run on Windows, these are the tests
that need no part of the Windows face still to be built. They run as the person: from an
elevated terminal, or as an account whose every program runs elevated, as with User Account
Control off, the face's `as_the_person_the_gate_lets_a_change_through` and `windows_refuses`
fail, saying so. A test that cannot pass on Windows until a later pull request says which,
with `#[cfg_attr(windows, ignore = "W<n>: <what it waits on>")]`, and in no other way.

From a Mac or Linux, `cargo check --target x86_64-pc-windows-msvc` checks only
`pitboard-sites` and `pitboard-share-ffi`: ring's build script compiles C against MSVC's
headers. CI's `windows` job runs the commands above on `windows-2025` and `windows-11-arm`.
First it checks that Rust there builds for that leg's own triple. Then it builds
`pitboard.exe` the way a release is built, without the cfg, and checks that it is a program
for that machine, that `--version` answers, that the program carries its C runtime, and that
`status`, `use`, `renew` and `schedule install` each refuse with `windows_not_released` and
leave every folder they are pointed at empty. `windows_refuses` runs against a build without
the cfg as well, which refuses every one of its commands that way. `windows-msrv` checks the
workspace there with Rust 1.91.

The job's own user is elevated on both images, in every program it starts, so CI runs every
Windows test as a fresh standard user instead, through
`.github/scripts/test-as-standard-user.ps1`. The script builds the tests as the job's user,
makes a local standard user, grants it read and run on the workspace and the target folder,
and starts each test program as it with its profile loaded and its own profile's variables,
from a folder of its own, with the variables Cargo gives a test. It takes the grants back and
removes the user afterwards. Give it what `cargo test` takes, and what the test programs take:

```powershell
./.github/scripts/test-as-standard-user.ps1 -Cargo '--locked -p pitboard-core --lib' -Pass 'host::windows host::token host::tests::the_floor release'
```

It makes and removes a Windows account, grants it rights on folders and, where Developer
Mode is off, changes who holds the symbolic-link right, so it is for CI's disposable runners
only, never a machine of your own: it refuses to run anywhere but a GitHub-hosted runner, as
`runner-facts.ps1` does. One step runs as the job's user on purpose: the opened `pitboard.exe`
refuses `use` and `renew` there with `elevated`, in the words for an account whose every
program runs elevated, and writes nothing.

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
checker reads the binary's header to tell a macOS, Linux or Windows build apart, and reads
each fact only from the systems its line in the register's `PER_SYSTEM` table reads it on.
Claude Code's Linux and Windows builds have no keychain backend, so the keychain facts are
read from its macOS build alone. `.github/workflows/conformance.yml` takes Claude Code's
builds from the packages `@anthropic-ai/claude-code-<system>`, and Codex's from `vendor/`
in `@openai/codex@<version>-<system>`, for `linux-x64`, `darwin-arm64`, `win32-x64` and
`win32-arm64`.

To add a fact, add an `Assumption` to the tool's register:

| Field | What it holds |
| --- | --- |
| `name` | A stable snake_case code |
| `fact` | What Pitboard believes |
| `read_from` | Where in the tool the fact was read, so it can be read again |
| `verified_against` | The build it was read from, such as `2.1.284` |
| `depends` | What in Pitboard stops being true if the fact moves |
| `probe` | Literals that must be in a build for the fact to still be readable there |
| `absent` | Literals whose arrival would disprove the fact |

Then give it a line in the register's `PER_SYSTEM` table. The line says one of these for
each of macOS, Linux and Windows:

| On a system | When |
| --- | --- |
| `Read("<version>")` | You read the fact from that system's build of that version. The macOS and Linux readings name the build in `verified_against` |
| `NotRead("<why>")` | The system has nothing the fact is about, such as a keychain, or its build carries a literal the fact rules out for another reason |
| `Pending { by: &["W<n>"], reads: "<what>" }` | A later pull request of the Windows work reads it there. Say what that pull request reads |

Pick literals specific to the fact. A literal already in the build for another reason
proves nothing. A fact about behaviour has no literal to find. It gets an empty `probe`,
and the run reports it as not readable.

Run the checker against the build you read the fact from. If you can, run it against an
older build that predates the fact too, and watch it go red.

`cargo test` checks the registers themselves. Every name must be unique and snake_case.
Every fact must say what it is, where it was read, which version and what depends on it.
Every fact must have one line in its table, and a reading on macOS or Linux must name the
build in `verified_against`. A pending reading must name a pull request from `W2` to `W27`.
No fact that rules out `Bun.secrets` may be read on Windows.

### Read a fact on Windows

Read a Windows build as bytes, on a Mac or on Linux, as the checker does. Never run
`claude.exe` or `codex.exe` to read one. Fetch the package with `npm pack`, as
`conformance.yml` does, and find `claude.exe`, or `codex.exe` under `vendor/`. Claude
Code's code is plain JavaScript inside its binary. Codex's is Rust, in its public source at
the tag `rust-v<version>`; look there for code behind `cfg(windows)`.

Read the code the fact is about, not only its literals, then run the checker on the
Windows build. Then record what you found:

- If the Windows build does what the fact says, mark the fact `Read` on Windows, with the
  Windows build's version.
- If it does something else, write the Windows behaviour as a fact of its own, named for
  Windows, such as `credman_target`, and mark it `NotRead` on macOS and Linux. Mark the
  first fact `NotRead` on Windows, and name the new fact in the reason. Never add a second
  reading of the same fact.
- If the fact can only be seen by running the tool, measure it on a Windows machine kept
  for that, with throwaway accounts and scratch homes. Name the build and the date in the
  fact.

## Documentation

`docs/` is the source of [docs.usepitboard.com](https://docs.usepitboard.com).
[docs/README.md](docs/README.md) says how to preview, check and publish a change, and
[docs/AGENTS.md](docs/AGENTS.md) holds the writing rules.

## Changelog

Record a change that someone using Pitboard would notice in [CHANGELOG.md](CHANGELOG.md),
under `## [Unreleased]`. The file follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).

- Put each entry under one of six types, in this order: `### Added`, `### Changed`,
  `### Deprecated`, `### Removed`, `### Fixed`, `### Security`.
- Write one change per bullet: what changed for the person using Pitboard, and why.
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
- `CHANGELOG.md` has an entry, if someone using Pitboard would notice the change.

Since 0.3.0, most commit subjects are one present-tense sentence saying what Pitboard does
after the change, with no prefix. An example is "The man page sets out every command instead
of naming pages that are not installed". The body says why, and what was measured, if
anything was.
