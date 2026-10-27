# Contributing

One person maintains Pitboard. Everyone taking part follows the
[code of conduct](CODE_OF_CONDUCT.md). Send a change as a pull request against `main`. Report
a security problem as [SECURITY.md](SECURITY.md) says, never in an issue.
[ARCHITECTURE.md](ARCHITECTURE.md) maps the code, and [RELEASING.md](RELEASING.md) says how a
release is made.

Pitboard moves real logins. Before you run anything, read the
[Never list in AGENTS.md](AGENTS.md#never): it applies to people as well as agents.

## Report a bug

Open an issue with the **Something went wrong** form.
[Report a problem](https://docs.usepitboard.com/troubleshooting#report-a-problem) says what
to paste, and what to read before you paste it.

## Set up

`rust-toolchain.toml` picks stable Rust with rustfmt and clippy. Run
`rustup toolchain install` in the repository to install it. The crates build on Rust 1.91
and later, the `rust-version` in `Cargo.toml`.

| Tool | For |
| --- | --- |
| `cargo-nextest` | The tests |
| `cargo-deny` | Dependencies, licences and advisories |
| `cargo-semver-checks` | `pitboard-core`'s public API |
| `cargo-insta` | Reviewing a changed `--json` snapshot |
| `cargo-hack` | The Rust 1.91 floor |
| `cargo-zigbuild`, Zig and Rust's `x86_64-unknown-linux-gnu` target | Linting the Linux code from a Mac |
| `shellcheck` 0.11.0 or later, Python 3.11 or later | The install script and the release's scripts |
| Xcode with Swift 6.2 or later, XcodeGen | [The macOS app](#the-macos-app) |
| .NET 10 SDK | [The C# bindings](#the-c-bindings) |
| Node 24 or later | [Website](#website) |

## Rules

1. Red before green. Before changing behaviour, write or find a test that fails against the
   current code, then watch that same test pass. A test that has never failed proves
   nothing. Pitboard once shipped one that passed whether the code under it worked or not.
2. Read the diff. An edit that did nothing leaves the old code in place, and the old code
   compiles. Read the region again right before changing it, and the diff after. An empty
   or surprisingly small diff is the symptom.
3. Measure the tool. Claude Code's behaviour here is undocumented, Codex's moves with its
   source, and Claude Desktop documents none of the files Pitboard moves. Claude Code and
   Codex ship several times a week. A claim about any of them needs an experiment or a
   reading of a named build, recorded in a test, the tool's register or the commit message.
   [Tool registers](#tool-registers) says how.
4. Comment what the code cannot say. A `//` line says why, where the code does not show it,
   and no comment repeats the code. A doc comment on an item is one line, and only where its
   name does not say it: UniFFI copies `///` on what the bindings export into Swift and C#,
   and docs.rs shows `pitboard-core`'s. A module's header says the rule it keeps, cut to the
   rule. A comment that rests on a tool names the fact in the tool's register, and one that
   rests on a system, a library or a site names its page in [docs/facts](docs/facts/README.md),
   never a build, a run or "measured" alone. Trim the comments of the code you change to this.

## Check a change

CI runs these on every pull request with `RUSTFLAGS=-D warnings`, so a warning fails it. Run
them before you push:

```sh
cargo fmt --check
cargo clippy --all-targets --locked
cargo clippy -p pitboard-ffi --all-targets --locked --features fixture
cargo nextest run --locked
cargo nextest run --locked -p pitboard-ffi --features fixture -E 'test(/^fixture::/)'
cargo deny --all-features check
cargo semver-checks --package pitboard-core
python3 .github/scripts/windows-pending.py
```

The `fixture` feature compiles the worlds the apps' debug builds launch into, so it gets a
lint and a run of the `fixture::` tests of its own.

CI checks the workspace on Rust 1.91 too:

```sh
cargo hack check --rust-version --workspace-behavior=cargo --workspace --all-targets --locked
```

Only a Mac compiles `rusqlite`, so CI also checks `pitboard-core`, `pitboard` and
`pitboard-ffi` on Rust 1.91 on a Mac.

Some code compiles only on Linux. To lint it from a Mac:

```sh
cargo-zigbuild clippy --target x86_64-unknown-linux-gnu --all-targets
```

Every program a Rust test starts in place of `claude`, `codex` or another tool is one
compiled stand-in, the `pitboard` crate's example `stand-in`. `cargo nextest run` builds
it. A test run alone with `--test`, or `pitboard-ffi`'s tests on their own, does not, so
build it first, with the `--release` or `--target` the tests use:

```sh
cargo build --locked -p pitboard --example stand-in
```

Unit tests run on `MemoryHost` and `Context::for_unit_test`, which reach no real home. A
test that needs a machine of its own with its services scripted builds it with
`pitboard_core::testing::ScratchMachine`, which panics where its context still reaches this
machine's keychain or the network. The command line's tests are one test binary,
`crates/pitboard/tests/cli`: a new one is a module its `main.rs` names, on the harness in its
`common`.

A build for tests finds Claude Desktop only in a scratch folder a test names, with
`Context::with_scratch_claude` or `PITBOARD_TEST_CLAUDE_DESKTOP`. Claude's data folder is
its `data`, and a program counts as Claude only inside its `Claude.app`. A folder that is,
holds or lies in the account's own `Library/Application Support` or a `Claude.app` under
either `Applications` panics. So does one named through `..`. The harness names one in each
test's own folder, and starts the stand-in under its `Claude.app` where a test needs Claude
open. `pitboard_core::testing::desktop` writes a data folder as Claude leaves one.

As root, under `sudo`, from an elevated terminal on Windows, or on a file system without Unix
modes, some tests fail and say why.

The snapshots in `crates/pitboard/tests/cli/snapshots` pin the `--json` contract, which changes
only on purpose. Review a changed one with `cargo insta review`, and say in the pull request
why the contract moved. They hold each message as `[message]`: its code is the contract.

A test of what Pitboard does checks codes and fields, and compares a sentence with the
function that makes it. Each sentence is pinned once, in the tests of the module that makes
it: `pitboard_core`'s `words.rs`, `error.rs` or `doctor/words.rs`, or `pitboard-ffi`'s
`present/words.rs`.

Both apps' UI tests find a control by its accessibility identifier, named for the model's
case it shows or sends: `notice.action.abandonStuckSwitch`, `setup.onlyOne`. A dialog's
buttons are the system's: `PrimaryButton` and `CloseButton` in a WinUI `ContentDialog`,
`action-button-1` and `action-button-2` in an AppKit alert. SwiftUI gives a menu's items no
identifier, so the macOS UI tests find those, a pop-up's choices and the Settings window's
panes by their words. Those words, and any sentence a UI test reads, sit in
`apps/macos/UITests/Words.swift` or `apps/windows/Pitboard.UITests/Words.cs`, the one place a
change to the model's words reaches the UI tests.

For a change to the install script, `packaging/install.sh`, another template in `packaging/`,
or `.github/scripts/fill-template.sh`, which fills them at release time:

```sh
shellcheck -s sh packaging/install.sh .github/scripts/fill-template.sh
python3 .github/scripts/test-install.py
```

The test runs the script under each of `sh`, `dash`, `bash`, `zsh` and `busybox sh` it finds
on `PATH`. `TEST_INSTALL_SHELLS` names the ones that must be found, as CI sets it.

For a change to the release's scripts, `release-kind.sh`, `core-unchanged.sh`,
`check-winget-hashes.py`, `windows-appcast.py`, `windows-versions.sh`,
`windows-update-key.sh`, `windows-update-sign.sh` and `session-gate.sh` in `.github/scripts`,
`apps/macos/scripts/build-number.sh`, the cargo-binstall metadata in
`crates/pitboard/Cargo.toml`, or a workflow's call of a PowerShell script in `.github/scripts`:

```sh
shellcheck -s sh .github/scripts/release-kind.sh .github/scripts/core-unchanged.sh \
  .github/scripts/windows-versions.sh .github/scripts/windows-update-key.sh \
  .github/scripts/windows-update-sign.sh .github/scripts/session-gate.sh \
  apps/macos/scripts/build-number.sh
python3 .github/scripts/test-release.py
```

The macOS app, Windows, the Windows app and the website have checks of their own, in their
sections. CI's `all-green` check fails when any other job failed or was cancelled, and a pull
request needs it to pass.

## The macOS app

Building the app takes a Mac with Xcode whose Swift is 6.2 or later, XcodeGen, and Rust's two
Mac targets. `./apps/macos/scripts/build-app.sh` builds the release bundle the way CI and a
release do.

`build-xcframework.sh` builds the core and `pitboard-share-ffi` for both kinds of Mac, with
their Swift bindings. None of that is committed. Run it before you open the project the first
time, and again after a change to the Rust. The Xcode project is generated from
`apps/macos/project.yml` by [XcodeGen](https://github.com/yonaskolb/XcodeGen), and only its
`Package.resolved` is committed. Change `project.yml`, never the project, and generate it
again:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
brew install xcodegen
./apps/macos/scripts/build-xcframework.sh --fixture
xcodegen generate --spec apps/macos/project.yml
open apps/macos/Pitboard.xcodeproj
```

`--fixture` builds the core with the worlds a debug build launches into. A debug build linked
against a core without them stops at launch and says how to build one.

Everything the app does is in the Swift package, and its tests run without starting the app:

```sh
swift test --package-path apps/macos
```

One of them starts a fixture's model. Against a core built without `--fixture` it is skipped,
and says so. A Swift warning fails the build, in the package and in the project, outside the
generated bindings.

CI checks the format of the Swift written by hand:

```sh
swift format lint --strict --recursive --configuration apps/macos/.swift-format \
  apps/macos/Sources/PitboardApp apps/macos/Sources/PitboardKit apps/macos/Sources/PitboardLinkTarget \
  apps/macos/App apps/macos/ShareExtension apps/macos/UITests apps/macos/Tests apps/macos/scripts .github/scripts
```

### Run the debug build

A fixture is a machine in a known state, where nothing reaches the keychain, the network or
your accounts. To run the debug build in one, start its executable with a fixture's name and
scratch homes:

```sh
scratch=$(mktemp -d)
env PITBOARD_FIXTURE=twoTools HOME="$scratch" PITBOARD_HOME="$scratch/.pitboard" \
  CLAUDE_CONFIG_DIR="$scratch/.claude" CODEX_HOME="$scratch/.codex" \
  <build>/Pitboard.app/Contents/MacOS/Pitboard
```

`<build>` is `Products/Debug` in the folder **Product** > **Show Build Folder in Finder**
opens. To run it from Xcode, add the same variables to the scheme's Run environment in
**Product** > **Scheme** > **Edit Scheme**. `xcodegen generate` writes the scheme again
without them, so check them before each Run: started without `PITBOARD_FIXTURE`, the debug
build quits at once. [AGENTS.md](AGENTS.md#never) lists the ways it must never be started.

`fixture_names()` lists the worlds, and a name that is none of them stops the launch and
names them all. A fixture's Claude Code sign-in takes a code typed back as `<code>#<state>`,
such as `fixture-code#state`. On macOS, `claudeDesktop` and `claudeDesktopOpen` hold Claude
Desktop, with a stand-in Claude that is open in the second. Opened for a sign-in, it signs in
to a third account and quits.

The UI tests start the debug build, each in a fixture of its own. Run them from Xcode with
**Product** > **Test**, or with:

```sh
xcodebuild test -project apps/macos/Pitboard.xcodeproj -scheme Pitboard -destination 'platform=macOS'
```

macOS asks for a password before a UI test can drive the app, unless the Mac allows
Automation Mode without one. `automationmodetool` says which applies to your Mac.

The pictures of the menu and the window in `docs/images` and `.github/media` come from the
UI test `ScreenshotTests`, in the `twoTools` fixture, in Light. Each test runs again in
Dark, and XCTest leaves your Mac in Dark. This takes them again, writes them at the sizes the
docs use and unregisters the build:

```sh
./apps/macos/scripts/docs-screenshots.sh
```

With `--figures <dir>`, it also writes every picture the test kept into `<dir>`, at full size
and under the name the test gave it: one of each surface, `macos-<surface>-<state>`, and in
Dark `macos-<surface>-<state>-dark`, the scenes usepitboard.com's macOS figures are checked
against among them. CI keeps them as the artifact `macos-app-pictures`.

### Unregister a build

A debug build keeps apart from a copy you installed. A release build you made claims what
the installed copy claims:

| | Debug build | Release build |
| --- | --- | --- |
| Bundle identifier | `com.usepitboard.Pitboard.debug` | `com.usepitboard.Pitboard` |
| Link scheme | `pitboard-debug://` | `pitboard://` |
| Share menu entry | **Pitboard Debug** | **Pitboard** |

Building the app registers its link scheme and its Share extension with macOS, and both stay
registered. After a build with Xcode, `xcodebuild` or `build-app.sh`, unregister each copy
it made. `<build>` is the folder that holds the copy, such as `apps/macos/build` for
`build-app.sh`. The first line lists every copy macOS knows; run it again afterwards to
check:

```sh
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
"$lsregister" -dump | grep -E '^path:.*(Pitboard\.app|PitboardShare\.appex)'
pluginkit -r <build>/Pitboard.app/Contents/PlugIns/PitboardShare.appex
"$lsregister" -u <build>/Pitboard.app/Contents/Frameworks/Sparkle.framework/Versions/*/Updater.app
"$lsregister" -u <build>/Pitboard.app
```

`pluginkit -m | grep usepitboard` lists the Share extensions macOS still offers.

## Windows

From 1.0.0-beta.1, a build of `main` runs on Windows. A Windows build of a 0.x release answers
only `--version`, `--help`, `completions` and `manpage`, and changes nothing. The gate is
`pitboard_core::release`. To work on a `release/0.N` branch on Windows, open a build to it with
`--cfg pitboard_unreleased_windows` in `RUSTFLAGS`, and with nothing else. `release.rs` says why
that is a `cfg` and not a Cargo feature.

Working there takes:

- Windows 11 24H2 or Windows Server 2025, build 26100, or later. An older build refuses every
  change with `system_too_old`.
- x64 or ARM64, with Rust's MSVC toolchain. On ARM64, clang as well: ring compiles its C with
  it there.
- Developer Mode, or the right to make symbolic links, for the tests that make links. Only on
  a machine kept for testing, as AGENTS.md says.
- A folder on an exFAT volume, named in `PITBOARD_TEST_VOLUME`, for the tests of how Pitboard
  replaces and removes files where a volume has no POSIX rename. Only on a machine kept for
  testing: an administrator mounts one with `diskpart`, as `test-as-standard-user.ps1`'s
  `-ExFat` does on CI.

Read the [Windows rules in AGENTS.md](AGENTS.md#on-windows) first. Then, as a Windows account
that holds no real login, in PowerShell from a terminal that is not elevated:

```powershell
$env:RUSTFLAGS = "-D warnings -C target-feature=+crt-static"
cargo clippy --workspace --all-targets --locked
cargo clippy -p pitboard-ffi --all-targets --locked --features fixture
cargo nextest run --locked
cargo nextest run --locked -p pitboard-ffi --features fixture -E 'test(/^fixture::/)'
```

`RUSTFLAGS` replaces each target's own rustflags, so it names the static C runtime releases
are built with.

A Windows build of `pitboard` carries a manifest and a version resource. `rc.exe` from the
newest Windows SDK compiles the resource, or `llvm-rc` on `PATH` from a Mac or Linux. Without
either the build warns and goes on. With `PITBOARD_REQUIRE_VERSIONINFO=1`, which CI and the
release set, it fails.

The whole suite runs on Windows, with no filter and no `--skip`. A Rust test Windows leaves
out gives a reason `.github/scripts/windows-pending.py` accepts: in its `ignore` reason, or,
where a `cfg` compiles it out or it returns early, in a `// windows: <reason>` line right above
that `cfg` or that `if`. CI's `windows-pending` job fails on any other. After changing the
script:

```sh
python3 .github/scripts/test-windows-pending.py
```

To lint for Windows from a Mac, with `cargo-xwin` and Homebrew's `llvm@22`:

```sh
export PATH="$(brew --prefix llvm@22)/bin:$PATH"
export RUSTFLAGS="-D warnings -C target-feature=+crt-static --cfg pitboard_unreleased_windows"
cargo-xwin clippy --locked --workspace --all-targets --target x86_64-pc-windows-msvc
cargo-xwin clippy --locked --workspace --all-targets --target aarch64-pc-windows-msvc \
  --cross-compiler clang
```

Run each once more with `-p pitboard-ffi --all-targets --features fixture` in place of
`--workspace --all-targets`, as CI does, and the workspace once without the `--cfg`, as a
release is built.

CI's `windows` job runs the PowerShell commands on `windows-2025` and `windows-11-arm`, the tests as
a fresh standard user through `.github/scripts/test-as-standard-user.ps1`, with an exFAT
volume it mounts for the run. The job fails when a run leaves a login item, a scheduled task, a login
folder, the app's data folder, a link scheme, a notification registration, a start at
sign-in, a `PATH` entry or a package behind. It also builds `pitboard.exe` as a release is
built, with no cfg, and checks that a 0.x build refuses and changes nothing, and that a 1.x
build answers with no argument as a fresh standard user, as winget's validation runs a program.
It holds the opened build to the same. One test reads the app execution aliases in the job
user's own profile, since a fresh standard user's has none.

CI's `windows-release` job builds the release's zips with `.github/actions/windows-release`,
as `release.yml` does from 1.0.0. `.github/scripts/check-windows-build.ps1` checks each
`pitboard.exe` in both jobs, and `test-windows-release.ps1` tests `package-windows.ps1`,
`check-windows-build.ps1`, `verify-windows-signatures.ps1`, which checks a release's
signatures, and `pack-session-bundle.ps1`. They run on CI's runners only.

`gh workflow run windows-facts.yml --ref <branch>` measures both runner images with
`pitboard-probe` and `.github/scripts/runner-facts.ps1`. No pull request runs it on its own:
one that changes the probe, its helpers or the script dispatches it on its branch and names
the run. [Windows runners](docs/facts/windows-runners.md) records what it measured.

With `-f session=true -f age-recipient=<age public key>`, the workflow builds the session
bundle for [the session machine](AGENTS.md#the-session-machine) instead. Only the owner
dispatches it, on the branch that holds `Release <version>`, its draft pull request's branch
once one is open ([Prepare a release](RELEASING.md#prepare-a-release)).
`.github/scripts/session-gate.sh` refuses anything but a branch at 1.0.0-beta.1 or later, the
cfg, and a key that is not an age public key. Each machine's bundle holds its MSI as a beta
with no signer ships it. The ARM64 one adds the zip, a second MSI one higher in
ProductVersion's fourth field, which installs over the first, `pitboard-probe` and
`Pitboard.Probe`. `pack-session-bundle.ps1` lists every file in `SHA256SUMS`, zips the bundle
and encrypts it with age to that key. GitHub keeps the two artifacts a day. Download, decrypt
and unzip them on the Mac, then delete them from GitHub:

```sh
gh run download <run id> -n session-build-aarch64-pc-windows-msvc \
  -n session-build-x86_64-pc-windows-msvc
for name in session-build-aarch64-pc-windows-msvc session-build-x86_64-pc-windows-msvc; do
  age -d -i <age identity file> -o "$name.zip" "$name/$name.zip.age" && rm -r "$name" &&
    unzip -q "$name.zip" && rm "$name.zip"
done
gh api repos/datlechin/pitboard/actions/runs/<run id>/artifacts --jq '.artifacts[].id' |
  xargs -I{} gh api -X DELETE repos/datlechin/pitboard/actions/artifacts/{}
```

Copy both folders, each with its `SHA256SUMS` at the root, to the session machine, and delete
them from the Mac after the session.

## The Windows app

The Windows app is C# in `apps/windows`, over the core's C# bindings. The
[code map](ARCHITECTURE.md#code-map) says what each project holds. `Pitboard.slnx` holds the
bindings and their tests. `Pitboard.Windows.slnx` holds the bindings, the app,
`PitboardShare.exe`, the MSI, the probe, and the app's and the MSI's tests. The WinUI
projects, `PitboardShare.exe` and the MSI build only on Windows, and the UI tests run only on
CI's runners.

Each project restores only the packages its `packages.lock.json` pins, at the versions
`Directory.Packages.props` names, with the exact .NET SDK `global.json` names:
`Pitboard.Core`'s lock file holds that SDK's trimming analyzers. After changing a package or
the SDK, run `dotnet restore --force-evaluate` on both solutions, with
`-p:EnableWindowsTargeting=true` on macOS and Linux, and commit the lock files. The build
fails on the style rules in `apps/windows/.editorconfig`, and CI checks the layout with
`dotnet format whitespace --folder --verify-no-changes apps/windows`.

### The C# bindings

The Windows app reaches the core through C# bindings, generated from `pitboard-ffi` like the
Swift ones and not committed, and `PitboardShare.exe` reaches `pitboard-share-ffi` through
its own. They need the .NET 10 SDK, and they and their tests build and run on macOS and Linux
as well as Windows:

```sh
cargo build --locked -p pitboard-ffi
cargo build --locked -p pitboard-share-ffi
mkdir -p apps/windows/Pitboard.Core/Generated apps/windows/Pitboard.Share.Core/Generated
cargo run --locked -p uniffi-bindgen-csharp -- --library target/debug/libpitboard_ffi.a \
  --out-dir apps/windows/Pitboard.Core/Generated --config crates/pitboard-ffi/uniffi.toml --no-format
cargo run --locked -p uniffi-bindgen-csharp -- --library target/debug/libpitboard_share_ffi.a \
  --out-dir apps/windows/Pitboard.Share.Core/Generated --config crates/pitboard-share-ffi/uniffi.toml --no-format
cd apps/windows
dotnet test --solution Pitboard.slnx -p:PitboardNativeLibrary="$PWD/../../target/debug/libpitboard_ffi.dylib" \
  -p:PitboardShareNativeLibrary="$PWD/../../target/debug/libpitboard_share_ffi.dylib"
```

On Linux the libraries the tests load are `libpitboard_ffi.so` and `libpitboard_share_ffi.so`,
and on Windows `pitboard_ffi.dll` and `pitboard_share_ffi.dll`. There the generator reads
`pitboard_ffi.dll` or `pitboard_ffi.lib`, which give the same file; `pitboard_ffi.dll.lib`
gives none. On a `release/0.N` branch, a Windows library built with
`--cfg pitboard_unreleased_windows` takes `-p:PitboardNativeLibraryOpened=true` as well, or the
test of what a 0.x release refuses fails. C# makes each field of a record a member of it, so a
field may not share its record's name.

On Windows, run the tests as [Windows](#windows) says: as a Windows account that holds no real
login, with the homes [AGENTS.md](AGENTS.md#on-windows) names at fresh folders. A C# test
Windows leaves out gives a reason `windows-pending.py` accepts, as a Rust test does:

```csharp
[OSCondition(ConditionMode.Exclude, OperatingSystems.Windows, IgnoreMessage = "<reason>")]
```

In the projects that build only on Windows, `Pitboard.Windows.Tests` and `Pitboard.UITests`:

```csharp
[Ignore("<reason>")]
```

The tests that start a fixture's model are skipped against a library built without the
`fixture` feature. To run them, build the library with
`cargo build --locked -p pitboard-ffi --features fixture` before `dotnet test`. The bindings
need not be generated again.

CI's `csharp-windows` job builds `pitboard_ffi.dll` twice, as a release is built and opened
with its fixtures, checks that the bindings generated from each are the Linux job's byte for
byte, and runs the C# tests against each the same way.

### The app on CI

CI's `windows-app` job builds and checks the app on `windows-2025` and `windows-11-arm`. It
builds the app in Debug over `pitboard_ffi.dll` opened with its fixtures, and publishes it in
Release over the library a release builds. It sets `PITBOARD_REQUIRE_VERSIONINFO=1`, so a
`pitboard_ffi.dll` or `pitboard_share_ffi.dll` built without its version resource fails the
build. The Release build takes its update key from `WINDOWS_UPDATE_PUBLIC_KEY`, and CI gives
it 32 bytes no private key was made for. Beside it, the job publishes `PitboardShare.exe`, the
share target, with Native AOT. `PitboardShare.exe` names its package's publisher, so its build
takes the signer's subject in `WINDOWS_SIGNER_SUBJECT`.
`.github/scripts/pack-windows-share.ps1` packs its sparse package and signs it with a
throwaway certificate, which nothing trusts and nothing registers.

`.github/scripts/check-windows-app.ps1` fails a Release build that holds:

- a fixture's text;
- a package identity on any program but `PitboardShare.exe`, an identity there other than its
  package's, or a package another publisher than the signer published;
- an update key in `Pitboard.dll` other than the one the build was given, or one WinSparkle
  would not take;
- a feed other than the latest full release's;
- a commit in `Pitboard.exe`'s version;
- a program or library named for Pitboard, as the release hands the signer, whose version
  resource names another product, company or version than Pitboard and `Cargo.toml`'s, or
  another file version than `.github/scripts/windows-versions.sh` gives it;
- a core opened before Windows is released, or one that takes its C runtime from a DLL;
- a `WinSparkle.dll` for another machine.

`test-windows-app.ps1` tests its identity, key, feed and version checks.

The job reads each image's WebView2 Runtime, which the account windows need. As a fresh
standard user, it runs `Pitboard.Windows.Tests`, a WinUI app that hosts its own tests, its
WebView2 tests against a stand-in site on the loopback among them, `Pitboard.UITests`, which
drives the built app with FlaUI, and `Pitboard.Share.Core.Tests` over `pitboard_share_ffi.dll`.
Then, as the job's user, it runs the UI test of an elevated launch. It builds `Pitboard.Probe`,
which the session machine runs to measure the app's notifications, and checks that it refuses
an account without `pitboard-probe`'s throwaway marker.

The job also builds two MSIs of `apps/windows/Pitboard.Installer` with WiX v7, from the
Release build, the `pitboard.exe` a release builds and the PowerShell completions its zip
holds: one with `PitboardShare.exe` and its package, as a signed release carries them, and one
without, as a release with no signer ships.
WiX runs ICE validation as it builds, with ICE03 off, and
`.github/scripts/validate-windows-msi.ps1` runs ICE03 after it and refuses any row but WinUI's
own files. That script first holds the WiX SDK to nuget.org's hash, so raising
`WixToolset.Sdk` in `global.json` needs its hash there too. CI never installs an MSI:
`extract-msi.ps1` extracts each with `msiexec /a`, `check-windows-app.ps1` checks the files
the signed one holds, `check-windows-msi.ps1` each one's version, layout, command line in `bin`,
completions and share target, the UI tests of the release gate start the signed one's
`Pitboard.exe`, which refuses at 0.x and at 1.x takes back what it registered with
`--unregister`, and `Pitboard.Installer.Tests` reads each one's tables and asks its
`pitboard_ffi.dll` whether it opens. `check-msi-components.ps1` fails each one where a key path
has another component GUID than in both MSIs of the newest release that shipped one, or a GUID
another key path, as the release checks its own. `test-windows-app.ps1` tests
`check-windows-msi.ps1`, `validate-windows-msi.ps1` and `check-msi-components.ps1` too, and runs
the release's `verify-windows-signatures.ps1` on the signed one's MSI, on copies of it with a row
changed and on the package the job signed. The MSI takes its version from
`windows-versions.sh`, which `test-release.py` tests.

The pictures of the Windows app come from the UI test `ScreenshotTests`: one of each surface,
`windows-<surface>-<state>`, in the fixtures, the docs' two in `twoTools`. The job keeps the
ones `windows-2025` takes for seven days, as the artifact `windows-app-pictures`:

```sh
gh run download <run id> -n windows-app-pictures
```

### Run the Windows debug build

Read [the rules for the app in AGENTS.md](AGENTS.md#on-windows) first. Build the bindings as
[The C# bindings](#the-c-bindings) says, from a library built with `--features fixture`, and
with `--cfg pitboard_unreleased_windows` too on a `release/0.N` branch. Then build the app in
Debug and start its executable in a fixture, with scratch homes set after building. Build it
into its own folder, without `-o`, and do not publish it: a Debug publish leaves out the XAML
files the app reads beside it, and `-o` names them wrong in its PRI file. From Windows PowerShell or
PowerShell 7:

```powershell
cd apps\windows
dotnet build Pitboard.App -c Debug -r win-arm64 -p:Platform=ARM64 `
  -p:PitboardNativeLibrary="$PWD\..\..\target\debug\pitboard_ffi.dll"
$scratch = (New-Item -ItemType Directory (Join-Path $env:TEMP "pitboard-$(New-Guid)")).FullName
$app = [Diagnostics.ProcessStartInfo]::new(
  (Resolve-Path Pitboard.App\bin\ARM64\Debug\net10.0-windows10.0.26100.0\win-arm64\Pitboard.exe).Path)
$app.UseShellExecute = $false
$app.Environment['PITBOARD_FIXTURE'] = 'firstLaunch'
foreach ($name in 'USERPROFILE', 'HOME', 'APPDATA', 'LOCALAPPDATA', 'CODEX_HOME', 'CLAUDE_CONFIG_DIR', 'PITBOARD_HOME') {
  $app.Environment[$name] = (New-Item -ItemType Directory (Join-Path $scratch $name)).FullName
}
[Diagnostics.Process]::Start($app)
```

Started again, the second copy hands its launch to the first, which brings its window forward,
and quits. Exit in the icon's menu ends it.

To open a link, start a second copy with a Pitboard link of the Debug build:

```powershell
$app.Arguments = 'pitboard-debug://open?url=' + [Uri]::EscapeDataString('https://chatgpt.com/')
[Diagnostics.Process]::Start($app)
$app.Arguments = ''
```

The first copy asks which account opens it, and the last line has `$app` start plain launches
again. Outside a fixture a Debug build refuses a link. Windows PowerShell's .NET Framework
starts a process through the shell unless told not to, and then refuses `Environment`, so
`UseShellExecute` is turned off; it has no `ArgumentList`, so the link goes in `Arguments`.

On x64, build with `-r win-x64 -p:Platform=x64`, which builds into `bin\x64\...\win-x64`. A
Debug build keeps apart from an installed Release:

| | Debug build | Release build |
| --- | --- | --- |
| AppUserModelID | `Pitboard.Pitboard.Debug` | `Pitboard.Pitboard` |
| Data folder | `%LOCALAPPDATA%\Pitboard App Debug`, or the fixture's | `%LOCALAPPDATA%\Pitboard App` |
| Link scheme | `pitboard-debug://`, never registered | `pitboard://` |
| Notifications | never registers, posts none | registers as Pitboard |
| Updates | says it cannot update | WinSparkle, over the feed of full releases, with the key `WINDOWS_UPDATE_PUBLIC_KEY` gave it |

The data folder is the known folder Local AppData, which `LOCALAPPDATA` does not move. Under a
fixture the app keeps its data in the fixture's folder, `pitboard-fixture` in the temporary
directory.

The app's icon is drawn in `apps/windows/Icon`. After changing a drawing, run
`python3 make-ico.py` there, with `rsvg-convert` and Pillow installed, and commit the
`Pitboard.App/Pitboard.ico` it writes.

## Tool registers

Each tool's register is `crates/pitboard-core/src/provider/<tool>/assumptions.rs`: what
Pitboard relies on about that tool, each fact with the build it was read from.
`crates/pitboard-core/src/assumptions.rs` says what each field holds.

`pitboard-conformance` looks in a build of the tool for the literals each fact can be read by,
and says which are still there:

```sh
cargo run -p pitboard-conformance -- <claude binary>
cargo run -p pitboard-conformance -- <codex binary> --provider codex
cargo run -p pitboard-conformance -- <app.asar> --provider desktop
```

Point it at the tool's native binary, not the npm wrapper, which carries none.
`npm pack @anthropic-ai/claude-code-<system>@<version>` holds `claude`, and
`npm pack @openai/codex@<version>-<system>` holds `codex` under `vendor/`. `<system>` is one
of `darwin-arm64`, `linux-x64`, `win32-x64` and `win32-arm64`.

Claude Desktop's register is read by hand from `Contents/Resources/app.asar` in Claude's
bundle, on a Mac where Claude is installed. No conformance job reads it, since where its
builds are published is not recorded. The checker reads the bundle, not the data folder, and
never starts Claude.

The checker exits 1 when a fact has moved: a literal it needs is gone, or one the fact rules
out has turned up. It exits 2 when it cannot make sense of its arguments, read the binary or
tell its system. `--json` prints a report a program can read.

To add a fact:

1. Add an `Assumption` to the tool's register.
2. Give `probe` literals that only this fact explains. A literal already in the build for
   another reason proves nothing. A fact about behaviour has no literal to find, so it gets
   an empty `probe`, and a run reports it as not readable.
3. Give the fact a line in the register's `PER_SYSTEM` table: `Read`, `NotRead` or `Pending`
   for each of macOS, Linux and Windows. `OnSystem` says when each applies.
4. Run the checker against the build you read the fact from. If you can, run it against an
   older build that predates the fact too, and watch it go red.

`cargo test` checks each register's shape and says what a fact lacks.

### Read a fact on Windows

Read a Windows build as bytes, on a Mac or on Linux, from the package `npm pack` fetches.
Claude Code's code is plain JavaScript inside `claude.exe`. Codex's is Rust, in its public
source at the tag `rust-v<version>`: look there for code behind `cfg(windows)`. Read the code
the fact is about, not only its literals, then run the checker on the Windows build. Then:

- If the Windows build does what the fact says, mark the fact `Read` on Windows, with the
  Windows build's version.
- If it does something else, write the Windows behaviour as a fact of its own, named for
  Windows, such as `credman_target`, and mark it `NotRead` on macOS and Linux. Mark the first
  fact `NotRead` on Windows, and name the new fact in the reason. Never add a second reading
  of one fact.
- If the fact can only be seen by running the tool, measure it as
  [AGENTS.md](AGENTS.md#on-windows) allows. Name the build and the date in the fact.

## Documentation

`docs/` is the source of [docs.usepitboard.com](https://docs.usepitboard.com).
[docs/README.md](docs/README.md) says how to preview, check and publish a change, and
[docs/AGENTS.md](docs/AGENTS.md) holds the writing rules.

## Website

`website/` is the source of usepitboard.com: Astro and Tailwind CSS, built to static files
that Cloudflare Workers serves. It needs Node 24 or later. Check a change in `website/` with:

```sh
npm ci
npx prettier --check .
npm run check
npm test
npm run build
```

CI runs these when a change reaches `website/` or a file the site reads, such as
`CHANGELOG.md` or a docs page. `npm run build` reads the latest release and the betas published
above it, with `GITHUB_TOKEN` when it is set. `PITBOARD_SITE_OFFLINE=1` builds from the releases
recorded in `src/data/fixtures/` instead, and takes each beta recorded there as published. The
build ends with `scripts/guard.mjs`, which fails on anything the site must not ship and says
what. `npm run preview` serves the build.

The browser tests in `website/e2e/` check the home page's figures, their markup and axe, in
Chromium, WebKit, Firefox and two phones, against the offline build that wrangler serves with
its CSP enforced:

```sh
npx playwright install chromium webkit firefox
PITBOARD_SITE_OFFLINE=1 npm run build
npm run test:browser
```

CI's `website-browsers` jobs run them, one for each project, and in Chromium also compare each
figure with its baseline in `e2e/static.spec.ts-snapshots/`. The baselines are made in CI's Linux
image: a figure whose look changes on purpose takes its new drawing from the Chromium job's
`website-browsers-chromium` artifact.

## Changelog

Record a change that someone using Pitboard would notice in [CHANGELOG.md](CHANGELOG.md),
under `## [Unreleased]`. The file follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and
[Common Changelog](https://common-changelog.org/) where Keep a Changelog sets no rule.

- Put each entry under one of six types, in this order: `### Added`, `### Changed`,
  `### Deprecated`, `### Removed`, `### Fixed`, `### Security`. A security fix goes under
  `### Security`.
- Write one change per bullet, as one present-tense sentence that says what Pitboard does
  after the change and reads without its heading. How it works and what was measured go in
  the docs and the commit message.
- Start an entry that is not backward compatible with `**Breaking:**`, list it first in its
  section, and say what to do in a second sentence. A removed or renamed command, flag, exit
  code, error or warning code, or `--json` key or event is breaking, and so is a newly refused
  input. A schema that older versions cannot read is not.
- Before you remove a flag, a code, a `--json` key or a check, list it under `### Deprecated`
  in an earlier release where you can.
- A break in `pitboard-core`'s public API is one `**Breaking:**` line under `### Changed`,
  linking the new version on docs.rs. An addition to it gets no entry.
- Leave out tests, CI, the release pipeline, the repository's own documents, and refactors or
  packaging with no visible effect.
- Make links inside an entry inline and absolute. A release's notes are cut from its
  section, without the link definitions at the end of the file, and the site's build refuses
  reference-style links, tables, blockquotes and headings other than the six types.

## Submit a change

A pull request against `main` needs all of these:

- CI's `all-green` check passes.
- A change in behaviour comes with a test that failed before it, as [Rules](#rules) asks.
- A changed snapshot comes with the reason the `--json` contract moved.
- A dependency you add comes with a reason, and `cargo deny` passes against `deny.toml`.
- `CHANGELOG.md` has an entry, if someone using Pitboard would notice the change.
- `docs/` says what the code does, for anything the change makes it say differently.
- A change that removes, renames or reshapes anything public in `pitboard-core` raises the
  core's minor version, which is its own and not Pitboard's: `version` in
  `crates/pitboard-core/Cargo.toml`, and the `=` version of `pitboard-core` under
  `[workspace.dependencies]` in `Cargo.toml`. Then run `cargo update --workspace`. CI's
  `semver` job fails until it is raised.

When `crates/pitboard-core/Cargo.toml` already names a minor version past the core's last
release, it stays.

Each commit subject is one present-tense sentence saying what Pitboard does after the change,
with no prefix, such as "The man page sets out every command instead of naming pages that
are not installed". The body says why, and what was measured.
