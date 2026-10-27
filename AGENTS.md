# AGENTS.md

Pitboard is a Rust command line and a native macOS menu bar app that switch Claude Code,
Codex and, on macOS, Claude Desktop between a person's own accounts. It moves real logins, so
a mistake here can sign someone out of a paid account. These rules apply to people too.
[CONTRIBUTING.md](CONTRIBUTING.md) says how to set up, check and submit a change.

## Never

- Never write to, overwrite or delete a keychain item that holds a real login, such as
  `Claude Code-credentials`. A test names its items after itself and calls
  `common::guard_not_live` before its first write.
- Never read `Claude Safe Storage`, the keychain item that holds the key to Claude Desktop's
  cookies.
- Never run `claude`, `claude auth login`, `codex login` or `codex logout` against a real
  home. They replace or revoke the login stored there.
  [The session machine](#the-session-machine) is the exception.
- Never start, quit or signal Claude (`com.anthropic.claudefordesktop`) from a test or a
  script. A test's Claude is the stand-in under its own scratch `Claude.app`.
- Never run a `pitboard` built from a branch against your real home. Build first, then point
  `HOME`, `PITBOARD_HOME`, `CLAUDE_CONFIG_DIR` and `CODEX_HOME` at a fresh directory, since
  Cargo and rustup read `HOME` too.
  [The session machine](#the-session-machine) is the exception.
- Never check a branch build against a real Claude on a macOS account that holds logins.
  Use a user account made for it: with `HOME` elsewhere, a build finds no Claude Desktop.
- Never start an app you built outside a fixture. Run a debug build's executable with
  `PITBOARD_FIXTURE` and those four homes set, as
  [Run the debug build](CONTRIBUTING.md#run-the-debug-build) shows. A release build reads no
  fixture, so never start one you built.
  [The session machine](#the-session-machine) is the exception.
- Never start a built app with `open`, Finder, a link, the Share menu or `launchctl setenv`.
  Launch Services passes on none of the variables, so the app starts on the real home.
- Never place a branch build's widget or control on a macOS account that holds logins. Either
  starts the app through Launch Services, on the real home.
- Never run a branch build's Shortcuts or Spotlight action on a macOS account that holds
  logins. It starts the app through Launch Services, on the real home.
- From a test or a script, never write to `~/.claude`, `~/.claude.json`, `~/.codex` or
  `~/.pitboard`. A test that writes gives every home a scratch directory, `CODEX_HOME`
  included.
- From a test or a script, never read, list, move, write or delete anything under
  `~/Library/Application Support/Claude` or `Claude-3p`. A test names its own data folder
  with `Context::with_scratch_claude` or `PITBOARD_TEST_CLAUDE_DESKTOP`.
- Never print a token or a cookie. `pitboard doctor --json` hides them; raw files do not.
- Never run the tests as root or under `sudo` on a machine that holds logins.
- Tests never touch launchd, systemd, `/usr/local/bin`, `/Applications` or
  `~/Library/LaunchAgents`.
- Never load claude.ai or chatgpt.com, or sign in to either, from a test, a script or a
  branch build. [The session machine](#the-session-machine) is the exception.
- Never read, copy or delete anything under `~/Library/WebKit/com.usepitboard.Pitboard`,
  where the account windows keep real sign-ins.
- Never open a `pitboard://` or `pitboard-debug://` link against a real home. It starts
  whichever copy claims the scheme, with the real home.
- Never leave a local build of the app registered with macOS: a link or a share can then
  start it on the real home. Unregister each copy you built, as
  [Unregister a build](CONTRIBUTING.md#unregister-a-build) shows, and leave the one in
  `/Applications` alone.
- Never put an `install.sh`, a rule naming it or a catch-all fallback in `website/`. A zone
  rule sends `/install.sh` to the newest release; a site file there would change what people
  pipe to `sh`.
- Never put an `install.ps1` in a release or in `website/`, or a zone rule that names one.
  Pitboard for Windows installs from the release's own files, Scoop, winget and
  cargo-binstall.
- Never mark a release latest by hand, or reuse or move an rc or beta tag. The release
  workflow decides latest.
- An agent never dispatches a workflow but `windows-facts.yml`, only on its own branch, and
  never with `session` set.
- Never sign with a signer's release policy or production credentials outside `release.yml`
  on a `v*` tag. Test signing uses the signer's test policy or sandbox.
- Never let a job that runs on a pull request or by hand reach the `release`,
  `windows-signing`, `windows-update`, `homebrew-tap`, `scoop-bucket` or `winget`
  environment. Each must keep a deployment rule for `v*` tags only.
- Never write the Scoop bucket or a `winget-pkgs` branch by hand, past the first commit and
  the first versions RELEASING.md sets out. The release writes them from its own
  `SHA256SUMS`, as it writes the tap.
- Never put a beta in Scoop or winget.
- Never publish a Windows release candidate or final release unsigned, and never unset
  `WINDOWS_SIGNER` once a signer is set.
- Never add a cache, such as the Actions cache, `rust-cache` or sccache, to a job that leads
  to a signing request. SignPath refuses a build that may take files from an earlier one.
- Never let `.github/workflows/rotation.yml` name a repository secret. Its rehearsal ships
  bundles that update themselves, and must never reach the real key.
- Never keep the update key and the Developer ID certificate in one place. With both lost,
  no installed copy accepts another update.
- Never keep the Windows update key with the signer's account, the macOS update key or the
  Developer ID certificate, and keep its offline copy apart from all of them. WinSparkle has
  no second route: with that key lost, every Windows copy is installed again by hand.
- Never ship an MSI or a `Pitboard.exe` whose Windows update key is missing or one WinSparkle
  rejects. WinSparkle then installs updates nobody signed.

### On Windows

[Windows](CONTRIBUTING.md#windows) says how to build and test Pitboard there.

- Never write to, overwrite or delete a Credential Manager item that holds a real login:
  `Claude Code-credentials`, every `Claude Code-credentials-<hash>` and each piece of them,
  Codex's `cli|*` and `secrets|*` targets, and anything under `Codex MCP Credentials`.
  [The session machine](#the-session-machine) is the exception.
- Never use `cmdkey /delete`, `vaultcmd` or PowerShell's credential modules on those names.
  Tests write only `pitboard-citest-*` items or the slot hashed from their own folder.
- A build for tests refuses, before `CredReadW`, `CredWriteW` or `CredDeleteW`, every name of
  those families but the slots hashed from its own scratch `CLAUDE_CONFIG_DIR` and private
  sign-in folder, and their pieces.
- Never print what a Credential Manager item holds, from a test, `doctor` or the probe.
- Never read, copy, rewrite or delete `%USERPROFILE%\.claude`, `%USERPROFILE%\.claude.json`,
  `%USERPROFILE%\.codex`, `%ProgramData%\OpenAI\Codex`, `C:\Program Files\ClaudeCode`, or
  the `%LOCALAPPDATA%\Pitboard` of an account that holds real logins.
  [The session machine](#the-session-machine) is the exception.
- Never read, list, write or delete anything under `%APPDATA%\Claude`, `%APPDATA%\Claude-3p`
  or `%LOCALAPPDATA%\AnthropicClaude` from a test, a script or the probe. Claude Desktop keeps
  its program and its sign-in there, and Pitboard leaves them alone on Windows.
- Never run a `pitboard.exe` built from a branch on a Windows account that holds real
  logins. Never put one on a `PATH`, in `%LOCALAPPDATA%\Programs`, in WinGet's Links or
  behind a Scoop shim, but as `runner-facts.ps1` does on a runner, below, and the session
  bundle's MSI does on [the session machine](#the-session-machine).
- Never use WSL to reach a real Windows login. When the ChatGPT app's WSL mode hands the
  Windows `CODEX_HOME` to a Linux codex, a Linux build run in WSL with that home writes the
  Windows login.
- Never start a `Pitboard.exe` you built on a Windows account that holds real logins. Run the
  Debug build's executable directly with `PITBOARD_FIXTURE`, and `USERPROFILE`, `HOME`,
  `APPDATA`, `LOCALAPPDATA`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and `PITBOARD_HOME` at fresh
  folders, set after building, as [Run the Windows debug build](CONTRIBUTING.md#run-the-windows-debug-build)
  shows. A Release build reads no fixture, and its data folder, icon and notifications ignore
  scratch homes: only CI's UI tests of the release gate start one. At 0.x it refuses. At 1.x
  `TheReleasedBuildTakesBackWhatItRegisteredOnUnregister` starts it, exits it and runs it with
  `--unregister`, only on a GitHub-hosted runner as the fresh standard user
  `test-as-standard-user.ps1` makes. That build reads Task Scheduler and writes no task.
  [The session machine](#the-session-machine) is the exception.
- Never start a built `Pitboard.exe` or `PitboardShare.exe` from Explorer, Start, a shortcut,
  `start`, `Invoke-Item`, a notification, the share sheet or a `pitboard://` or
  `pitboard-debug://` link, on any account but [the session machine's](#the-session-machine)
  throwaway ones. Each passes the account's own variables, and so its real homes.
- Never read, copy or delete `%LOCALAPPDATA%\Pitboard App` or `Pitboard App Debug` on an
  account that holds real logins. Their WebView2 profiles hold real sign-ins.
- Never load claude.ai or chatgpt.com in an account window of a Debug, fixture or session
  build. A fixture serves its own pages.
- Never let a test, a fixture or a Debug build register with Windows: no
  `AppNotificationManager.Register`, no HKCU `Run`, `Environment` or `Software\Classes` value,
  no Jump List, no package and no certificate in a store. Only a Release build installed by an
  MSI registers, and `Pitboard.Probe` on [the session machine](#the-session-machine). The
  exception is the Release build `TheReleasedBuildTakesBackWhatItRegisteredOnUnregister`
  starts at 1.x as a runner's fresh standard user, whose `--unregister` takes back what it
  registered before the leak check.
- Never give `Pitboard.exe`, `pitboard.exe` or a process they start a package identity. Only
  `PitboardShare.exe` carries an `<msix>` manifest element.
- Never install an MSI built from a branch on an account that holds real logins, or on a CI
  runner, which only extracts one with `msiexec /a`. Install it only in
  [the session machine's](#the-session-machine) throwaway accounts, and remove it before the
  session ends.
- Never install a release's MSI on a CI runner, and never install a release's MSI or zip on a
  machine that holds logins from a test or a script.
- Never run the app's UI tests on a machine of your own: they move the mouse and type into
  whatever has focus. They run on CI's runners as a fresh standard user.
- Never run a test or a script there before pointing `USERPROFILE`, `HOME`, `APPDATA`,
  `LOCALAPPDATA`, `CODEX_HOME`, `CLAUDE_CONFIG_DIR` and `PITBOARD_HOME` at fresh folders.
  With `CODEX_HOME` unset, Codex's home is `.codex` in the known folder Profile, whatever
  `USERPROFILE` says. Set them after building: Cargo and rustup find their homes through
  `USERPROFILE`.
- Never weaken or get around the release gate in a build anybody installs. No release,
  script or package passes `--cfg pitboard_unreleased_windows`, `test-support` or
  `fixture`.
- Tests never touch Task Scheduler, but for the read of the Release build
  `TheReleasedBuildTakesBackWhatItRegisteredOnUnregister` starts. They never write the registry
  or the user `PATH`, and never run elevated on a machine that holds logins. They may read the
  registry.
- A test seals and unseals only with `CryptProtectData` and `CryptUnprotectData` under the
  running user's own key, never with `CRYPTPROTECT_LOCAL_MACHINE`, and never backs up, exports
  or deletes a master key. Every Windows test that parks a login seals it.
- Never weaken a machine that holds real logins to make a test pass: no Defender exclusion,
  no UAC change, no access change to a real profile, and no Developer Mode or symbolic-link
  right for the tests' links. Those are for CI's runners and a machine kept for testing.
- Never run `.github/scripts/test-as-standard-user.ps1`, `check-windows-build.ps1`,
  `package-windows.ps1`, `test-windows-release.ps1` or `runner-facts.ps1` on a machine of your
  own. They run the builds they are given, make Windows accounts and change who holds which
  rights.
- `test-as-standard-user.ps1`, also when `check-windows-build.ps1` runs it, makes one test user
  and may grant it the symbolic-link right. With `-ExFat`, which only `ci.yml`'s `windows` job
  passes, it also mounts one exFAT VHD under `RUNNER_TEMP` and detaches it in its `finally`.
  Beside that, only `runner-facts.ps1`, only through `windows-facts.yml` on a GitHub-hosted
  runner, may make Windows accounts, put one in Administrators or OpenSSH Users, change their
  passwords, turn on Developer Mode, grant the symbolic-link right, register tasks, write the
  job user's `HKCU\Environment`, mount volumes, share a folder, install Win32-OpenSSH from
  Microsoft's release where the image ships none and start OpenSSH Server, install Bun and
  Scoop, turn on winget's local manifests, serve files on loopback, put a build it made behind
  a Scoop shim, in WinGet's Links, in `~\.cargo\bin` or on the user `PATH`, trust a test
  certificate in `LocalMachine\TrustedPeople` from the job user's elevated side, register the
  probe's sparse package, PitboardShare's and a probe link scheme for a throwaway user, and set
  Defender's attack surface reduction rules to audit. It undoes each in a `finally`.
  `release.yml`'s `scoop-install` job, on a GitHub-hosted runner after `publish`, may install
  Scoop into the job's temporary folder, which writes the job user's `PATH` and
  `HKCU\Environment`, then install the published `pitboard` from the bucket and run its
  `--version`. Never copy one of those changes, or `test-as-standard-user.ps1`'s, into a
  script meant for a person's machine.
- `windows-facts.yml` stays `workflow_dispatch` only, names no secret, and uploads only the
  probe's redacted reports and the session bundle encrypted to the owner's age key. No workflow
  uploads an opened `pitboard.exe` unencrypted: the repository is public, so anyone signed in
  to GitHub can download an artifact.
- Bun and `proper-lockfile` reach a runner only from Bun's release zip, checked against the
  SHA-256 `windows-facts.yml` pins or is given with the version, and from the committed
  `bun.lock` with install scripts off.
- A row that writes a `pitboard-citest-*` Credential Manager item, through `Bun.secrets` or
  otherwise, runs only on a runner and deletes the item by name in the same row. Each
  throwaway user runs `credman-names --leak-check` before its password changes or it is
  removed.
- Never run `claude.exe` or `codex.exe` to read a fact. Read the build as bytes on a Mac or
  Linux, as `pitboard-conformance` does. [The session machine](#the-session-machine) runs them
  only for what no reading of a build settles.
- Never measure a running tool on a Windows machine that holds logins. Use one kept for it,
  with throwaway accounts and scratch homes, or [the session machine](#the-session-machine).
- Never trust a test certificate on a machine, or register a sparse package signed with one,
  outside a disposable runner or a throwaway account on
  [the session machine](#the-session-machine).

### The session machine

The session machine is a Windows 11 for Arm virtual machine on the owner's Mac that holds
throwaway Windows, Claude and ChatGPT accounts and nothing else. It measures what no runner
can, and is the one exception to the rules that point here. In its throwaway accounts, to be
measured: the session bundle's MSI installs into `%LOCALAPPDATA%\Programs` and puts `bin` on
the user `PATH`, the app starts from Start and at sign-in, Pitboard starts Claude Code's and
Codex's sign-ins, and Pitboard, `claude`, `claude auth login` and `codex` read and write the
accounts' own homes and Credential Manager items.

- The session bundle is built only by `windows-facts.yml`'s `session` job, which only the
  owner dispatches, on the branch that holds `Release <version>`, its draft pull request's
  branch once one is open ([Prepare a release](RELEASING.md#prepare-a-release)). It leaves
  the runner only encrypted with age to the owner's key, and is deleted from GitHub once
  downloaded, a day at most. It reaches the session machine from the owner's Mac, never by a
  download there with a GitHub sign-in, and is deleted from both after the session. Never
  install it anywhere else.
- Never run the session on a machine or an account that has ever held a real login, or over
  RDP or OpenSSH. Only the owner signs in, by hand, at the console. It ends with the revert the
  owner chose.
- Never sign in to the session's throwaway Claude or ChatGPT accounts from a machine or browser
  profile that holds a real login.
- Record names, attributes, byte lengths, timings and error codes only, never a token, a blob,
  a user name, a SID or the MachineGuid. Never copy a Process Monitor log, a debug log, a
  screenshot or `doctor --json` output off the session machine.
- Never install Claude Desktop on the session machine, and never use `cmdkey /delete`,
  `vaultcmd` or PowerShell's credential modules there either.

## Check a change

Before you push, run what [Check a change](CONTRIBUTING.md#check-a-change) lists, and the
checks of each other section of CONTRIBUTING.md the change reaches.

## Rules

- A behaviour change starts with a test that fails against the current code.
- A claim about Claude Code, Codex or Claude Desktop needs a reading of a named build or an
  experiment, recorded in the tool's register, a test or the commit message.
  [Tool registers](CONTRIBUTING.md#tool-registers) says how.
- A change people would notice gets an entry under `## [Unreleased]` in
  [CHANGELOG.md](CHANGELOG.md), as [Changelog](CONTRIBUTING.md#changelog) says.
- A change that breaks `pitboard-core`'s public API raises the core's own minor version in
  the same pull request. When `crates/pitboard-core/Cargo.toml` already names a minor
  version past the core's last release, it stays.
- A commit subject is one present-tense sentence saying what Pitboard does after the change,
  with no prefix. The body says why, and what was measured.

## Where things are

- [ARCHITECTURE.md](ARCHITECTURE.md): the code map and the rules the code keeps.
- [docs/facts](docs/facts/README.md): what the code rests on about the systems, libraries and
  sites around it.
- [RELEASING.md](RELEASING.md): how a release is made.
- `docs/`: the documentation site; [docs/AGENTS.md](docs/AGENTS.md) has its writing rules.
