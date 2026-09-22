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

A release the crates to crates.io, the command line for four targets,
and the app, signed and notarised when these repository secrets are set. Without them the
release still happens and the app is signed ad-hoc, which Gatekeeper warns about.

| Secret | Where it comes from |
| --- | --- |
| `CERTIFICATES_P12` | The Developer ID Application certificate, exported from Keychain Access as .p12, `base64` |
| `CERTIFICATES_PASSWORD` | The password given to that export |
| `APPLE_API_KEY_P8` | An App Store Connect team key with the Developer role, `base64` |
| `APPLE_ID` | Only needed if the notarisation route ever goes back to an app-specific password |
| `APPLE_API_KEY_ID`, `APPLE_API_ISSUER` | Shown beside that key |
| `SPARKLE_PUBLIC_KEY`, `SPARKLE_PRIVATE_KEY` | `apple/.build/artifacts/sparkle/Sparkle/bin/generate_keys --account pitboard` once, then the same with `-x -` to read the private one |

The signing identity is read from the certificate itself, so there is no secret for it.
Every archive is attested, so a downloader can check what built it with
`gh attestation verify <file> --repo datlechin/pitboard`. The command line binaries are
signed and notarised like the app, because a tarball opened from a browser arrives
quarantined and Gatekeeper stops an ad-hoc signature.

Never change the update key once a release carries it. An app checks the feed's signature
against the key it was built with, so a new key strands every copy already installed.

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
- `security dump-keychain` without `-d` never prompts, exits 0 in 0.06 seconds against a
  keychain of 362 items, and emits attributes only: no secret of any item. Reads afterwards
  take the usual 0.016 seconds, so listing carries none of the access-list side effect an
  in-process read does. Service names appear as `    "svce"<blob>="<name>"`.

## Dependencies

Every new dependency needs a reason in the pull request. `cargo deny check` must pass.
