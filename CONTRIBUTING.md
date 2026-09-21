# Contributing

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
cargo test --test keychain_write_is_harmless -- --test-threads=1
```

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

## Dependencies

Every new dependency needs a reason in the pull request. `cargo deny check` must pass.
