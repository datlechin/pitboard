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

On macOS, run the keychain latency test on its own — contention from other concurrent
`security` calls can breach its threshold:

```sh
cargo test -p pitboard --test keychain_write_is_harmless -- --test-threads=1
```

Some code compiles only on Linux, so lint for it before pushing, for example with
`cargo zigbuild clippy --target x86_64-unknown-linux-gnu --all-targets`.

A contract snapshot changes only when the `--json` contract changes on purpose. Review the
difference with `cargo insta review`, and say in the change why the contract moved.

## Rules this project learned the hard way

1. **Red before green.** Before a change that alters behaviour, write or find a test that
   fails against the current code. Apply the change and watch that same test pass. A test
   that has never failed has never proved anything — this project shipped one that passed
   identically whether the code under it worked or not.

2. **"It still compiles" is not evidence an edit applied.** An edit that silently did
   nothing leaves the old code in place, and the old code compiles. Re-read the region you
   are about to change immediately before changing it, and after every edit look at the
   diff. An empty or unexpectedly small diff is the symptom.

3. **Never write to a keychain item a real login lives in.** Tests name their items from
   their own identity and call `common::guard_not_live` before the first write.

4. **Measure rather than infer anything about Claude Code.** Its behaviour here is
   undocumented. A claim about it needs an experiment, and the experiment belongs in the
   commit message or a test.

## Releasing

A tag `v<version>` releases: the crates to crates.io, the command line for four targets,
and the app, signed and notarised when these repository secrets are set. Without them the
release still happens and the app is signed ad-hoc, which Gatekeeper warns about.

| Secret | Where it comes from |
| --- | --- |
| `APPLE_SIGN_IDENTITY` | The certificate's name, as `security find-identity -v` prints it |
| `APPLE_CERT_P12` | The Developer ID Application certificate, exported from Keychain Access, `base64` |
| `APPLE_CERT_PASSWORD` | The password given to that export |
| `APPLE_API_KEY_P8` | An App Store Connect API key with the Developer role, `base64` |
| `APPLE_API_KEY_ID`, `APPLE_API_ISSUER` | Shown beside that key |
| `SPARKLE_PUBLIC_KEY`, `SPARKLE_PRIVATE_KEY` | `apple/.build/artifacts/sparkle/Sparkle/bin/generate_keys` once, then `generate_keys -x -` to read the private one |

The update key must never change once a release carries it: an app checks the feed's
signature against the key it was built with, so a new key strands every copy already
installed.

## Dependencies

Every new dependency needs a reason in the pull request. `cargo deny check` must pass.
