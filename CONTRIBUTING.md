# Contributing

## Layout

- `crates/pitboard-core`: the engine. Parking, switching, recovery, the stores, usage. It
  reads no environment variable except in `Context::from_env`, and prints nothing.
- `crates/pitboard-ffi`: the core as UniFFI bindings, for the app. Records and enums only,
  every call synchronous.
- `apple`: the Swift package. `PitboardKit` calls the bindings off the main thread,
  `Pitboard` is the menu bar app. `scripts/build-xcframework.sh` builds the core for both
  architectures, `scripts/build-app.sh` assembles `Pitboard.app` from it.
- `crates/pitboard`: the command line. Arguments, rendering for people, and the `--json`
  contract, pinned by the snapshots in `crates/pitboard/tests/snapshots`.

## Before you change anything

The local loop is the same as CI:

```sh
cargo fmt --check
cargo clippy --all-targets
cargo test
cargo deny check
```

On macOS, run the keychain latency test on its own. Other `security` calls running at the
same time push it over its threshold:

```sh
cargo test -p pitboard --test keychain_write_is_harmless -- --test-threads=1
```

Some code compiles only on Linux, so lint for it before pushing, for example with
`cargo zigbuild clippy --target x86_64-unknown-linux-gnu --all-targets`.

A contract snapshot changes only when the `--json` contract changes on purpose. Review the
difference with `cargo insta review`, and say in the change why the contract moved.

## Rules this project learned the hard way

1. Red before green. Before changing behaviour, write or find a test that fails against
   the current code, then watch that same test pass. A test that has never failed has
   proved nothing. This project once shipped one that passed whether the code under it
   worked or not.

2. Compiling is not evidence that an edit applied. An edit that did nothing leaves the old
   code in place, and the old code compiles. Read the region again right before changing
   it, and look at the diff after. An empty or surprisingly small diff is the symptom.

3. Never write to a keychain item that holds a real login. Tests name their items after
   their own identity and call `common::guard_not_live` before the first write.

4. Measure Claude Code, do not guess at it. Its behaviour here is undocumented, so a claim
   about it needs an experiment, and the experiment belongs in a test or the commit
   message.

## The site

`website/` is where usepitboard.com will be built, with Astro; it is empty until then.
`docs/` holds the documentation source for docs.usepitboard.com. Neither is published by
this repository yet, and the README and the app already link to the documentation, so it
has to be standing before the next release.

## The state file

`state.json` carries a `schema`. The command line and the app hold their own copy of the
core and update by different routes, so on one machine an older pitboard will meet a file a
newer one wrote. Reading forwards is `state::migrate`: each bump adds an arm that rewrites
the document and falls through to the next. Reading backwards is not possible and says
which half to upgrade. A bump needs a test that loads a file the previous version wrote.

## Releasing

A tag `v<version>` releases; a tag like `v0.2.0-rc1` is a pre-release, which skips
crates.io and publishes no update feed, so nobody's installed copy updates into it. The
guard job refuses a tag that disagrees with the manifest or has no CHANGELOG section.

A release publishes the crates to crates.io, the command line for four targets, the app,
and the Homebrew tap, signed and notarised when these repository secrets are set. Without
them the release still happens and the app is signed ad-hoc, which Gatekeeper warns about.

| Secret | Where it comes from |
| --- | --- |
| `CERTIFICATES_P12` | The Developer ID Application certificate, exported from Keychain Access as .p12, `base64` |
| `CERTIFICATES_PASSWORD` | The password given to that export |
| `APPLE_API_KEY_P8` | An App Store Connect team key with the Developer role, `base64` |
| `APPLE_ID` | Only needed if the notarisation route ever goes back to an app-specific password |
| `APPLE_API_KEY_ID`, `APPLE_API_ISSUER` | Shown beside that key |
| `SPARKLE_PUBLIC_KEY`, `SPARKLE_PRIVATE_KEY` | `apple/.build/artifacts/sparkle/Sparkle/bin/generate_keys --account pitboard` once, then the same with `-x -` to read the private one |
| `HOMEBREW_TAP_TOKEN` | In the `homebrew-tap` environment. A fine-grained personal access token, `datlechin/homebrew-tap` as its only repository, Contents read and write as its only permission, and an expiry the maintainer will notice |

There is no `CARGO_REGISTRY_TOKEN`. crates.io hands the publish job a token made from
GitHub's word about which workflow is running, and revokes it when the job ends.

The signing identity is read from the certificate itself, so there is no secret for it.
The command line binaries are signed and notarised like the app, because a tarball opened
from a browser arrives quarantined and Gatekeeper stops an ad-hoc signature.

Everything published is attested, so a downloader can check what built it with
`gh attestation verify <file> --repo datlechin/pitboard`: the tarballs, the app, the bill
of materials beside each of them, `SHA256SUMS`, and `appcast.xml`. The last two are made in
the same job and published in the same release as the files they describe, so on their own
they say a download arrived whole and nothing about who put it there.

Never change the update key once a release carries it. An app checks the feed's signature
against the key it was built with, so a new key strands every copy already installed.

### What the maintainer has to set up by hand

Once, in this order:

1. On crates.io, for `pitboard-core` and then for `pitboard`: Settings, Trusted Publishing,
   Add, GitHub. Repository owner `datlechin`, repository name `pitboard`, workflow filename
   `release.yml`, environment `release`. Both crates need their own entry; registration is
   per crate. Both are already published, which Trusted Publishing requires.
2. In this repository's settings, an environment named `release` with required reviewers.
   The publish job waits in it, so an unexpected tag stops before the one step of a release
   that cannot be undone. The name has to be the one registered on crates.io above.
3. An environment named `homebrew-tap` holding `HOMEBREW_TAP_TOKEN`. An environment rather
   than a repository secret because it is the only credential here that reaches another
   repository, and a secret in an environment is readable only by a job that asks for that
   environment by name.
4. Delete `.github/workflows/follow-releases.yml` from `datlechin/homebrew-tap`. The
   release writes the tap now; leaving the old poller in place means two writers and a
   version that can come from either.
5. Push a pre-release tag, `v<next>-rc1`, and watch the publish job. It exchanges the
   crates.io token and uploads nothing, which is where a registration that does not match
   is meant to be found out.

Then `CARGO_REGISTRY_TOKEN` can be deleted from this repository's secrets, and the token it
held revoked on crates.io.

If the tap push fails, re-run the `tap` job. There is no script for doing it by hand any
more: the checksums come from the `SHA256SUMS` the release computed, and a second download
somewhere else is what this replaced.

## Measured, not assumed

These decide the design, and each was measured rather than reasoned about:

- `security -i` reads 4097 bytes of command line, no continuation. Its `-w` prompt reads 128.
- Writing a keychain item in process, through the Security framework, makes every later
  read of that item by `security` take about a second instead of 0.01, for good.
- A running Claude Code session picks up a swapped credential within about 33 seconds.

Redo the first two on a scratch item before changing anything that depends on them.

Read against Claude Code 2.1.278's own storage layer, which is where the rest of the
coupling comes from:

- The write lock is proper-lockfile at `<storage dir>/.storage-write`, stale 15000ms, ten
  retries, 100ms to 1000ms of backoff. `lock.rs` carries the same numbers.
- Every write under it drops the read cache, reads the credential again inside the lock,
  and abandons the write when that read fails. A stale account cannot be written back.
- Claude Code treats its own lock going missing as a warning and keeps writing, so pitboard
  cannot expect the other side to stop.
- A write can be marked as already locked without the lock being taken. `/logout` is the
  path that does it.
- The keychain write is `security -i` below 4032 bytes of command and
  `add-generic-password -U -a <account> -s <service> -X <hex>` above it, with a 2 second
  timeout, and only a timeout counts as retryable.
- The keychain read is `find-generic-password -a <account> -w -s <service>`. Exit 0 with
  nothing is absent; 44 is absent; 36 is a locked keychain and means unreadable, not empty.
  `security show-keychain-info` exiting 36 is the same signal for the keychain as a whole.
- The live chain is the keychain with the plaintext file behind it. The successor backend
  (`tengu_hover_rest`) replaces the fallback half and only for a caller that hands a backend
  in, so an ordinary `claude` still reads the keychain first.
- Claude Code demotes to the plaintext file when a keychain write fails for good, and
  deletes the keychain item when it does. pitboard does not, on purpose.
- The supervisor daemon records itself in `<config dir>/daemon.lock` with its pid and the
  Claude Code version that launched it, and leaves the file behind when it dies.
- This machine sat within 0.75 seconds of api.anthropic.com's `Date` header across eight
  requests, and that header has a granularity of one second, so the whole spread was inside
  the noise. There is therefore no skew estimate anywhere: a renewal's expiries are
  anchored to the `Date` of the answer that carried them, which is the correction, and on a
  machine whose clock works there is nothing left to correct.
- The facts in `pitboard-core::assumptions` carry the literals they are readable by, and
  `cargo run -p pitboard-conformance -- <a claude binary>` checks them. Measured across six
  builds: the set holds from 2.1.273 through 2.1.278 and correctly goes red on 2.1.124,
  which predates the credential write lock, two of the five account-scoped keys and the
  keychain error classification. It is shallow on purpose, and the tool says so: a literal
  being present does not prove the behaviour around it, and a literal disappearing does
  prove something moved.
- APFS stores a directory's mtime to the nanosecond and stores it approximately: setting
  one and reading it straight back gives a value 18 to 60 nanoseconds away. A lock that
  remembered the value it asked for would find a mismatch every time; the value read back
  is the only one worth keeping.
- `security dump-keychain` without `-d` never prompts, exits 0 in 0.06 seconds against a
  keychain of 362 items, and emits attributes only: no secret of any item. Reads afterwards
  take the usual 0.016 seconds, so listing carries none of the access-list side effect an
  in-process read does. Service names appear as `    "svce"<blob>="<name>"`.
- Claude Code has two guarded credential stores and no others: the macOS keychain, and the
  Windows credential manager behind the `tengu_windows_credman` GrowthBook flag. Searching
  the whole 2.1.278 bundle finds no `libsecret`, no `org.freedesktop.secrets`, no
  `gnome-keyring` and no `SecretService`. `secret-tool` and `kwallet-query` do appear, in
  the list of credential helpers its Bash sandbox keeps out of a shell, which is why
  neither is used as a needle. So on Linux the keychain backend's `security` call simply
  fails and the plaintext file is what holds the login. It is written and then chmod'd to
  0600. pitboard's `PlainUnix` platform matches that, and `assumptions.rs` carries the
  absence under `no_keyring_off_macos`, checked on every build by the conformance job:
  a fact resting on something not existing is wrong the moment it does, and nothing
  disappearing would ever say so.

Read on 2026-09-22, against Homebrew 7.0.6 and the tap as it then stood:

- `cargo cyclonedx` writes a bill of materials beside every `Cargo.toml` in the workspace
  whatever `--manifest-path` says, so the release keeps the one belonging to the crate in
  the artefact and deletes the rest. A target that is not installed still resolves. The two
  macOS targets resolve to the same 83 components, which is why the app has one bill of
  materials and not two; macOS and musl differ by 7, which is why each target has its own.
- crates.io issues a Trusted Publishing token that lasts 30 minutes, and matches on
  repository owner, repository name, workflow filename and, when it is given one, the
  environment. Registration is per crate, so `pitboard` and `pitboard-core` each need it.
- The tap was being written by `follow-releases.yml` inside `datlechin/homebrew-tap`, on
  `17 */6 * * *`. A release was therefore finished and green up to six hours before anyone
  could install what it published, and `packaging/pitboard.rb` in this repository still said
  v0.1.2 while the tap served 0.2.0 and the workspace was at 0.2.0. Nothing anywhere
  compared the three.
- `pitboard doctor` exits 3 where Claude Code has never run, which is every clean runner, so
  the job that installs from the tap treats 0 and 3 as the binary having run its checks and
  anything else as it having failed to.

## Dependencies

Every new dependency needs a reason in the pull request. `cargo deny check` must pass.
