# Contributing

## Layout

- `crates/pitboard-core`: the engine. Parking, switching, recovery, the stores, usage. It
  reads no environment variable except in `Context::from_env`, and prints nothing.
- `crates/pitboard-core/src/provider`: one module per tool, `claude` and `codex`, each
  implementing the `Provider` trait in `mod.rs` (where the tool keeps its login, whose it
  is, how to renew it, what it has left). Each module's `assumptions.rs` is that tool's
  register of facts.
- `crates/pitboard-conformance`: reads a tool's register out of a build of that tool.
- `crates/pitboard-ffi`: the core as UniFFI bindings, for the app. Records and enums only,
  every call synchronous.
- `apple`: the Swift package. `PitboardKit` calls the bindings off the main thread,
  `Pitboard` is the menu bar app. `scripts/build-xcframework.sh` builds the core for both
  architectures, `scripts/build-app.sh` assembles `Pitboard.app` from it, with the command
  line inside at `Contents/Helpers/pitboard`.
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
   their own identity and call `common::guard_not_live` before the first write. A Codex
   test that writes points `CODEX_HOME` at a scratch directory and never writes to
   `~/.codex` (the one ignored test that reads a real `auth.json` only reads it), and
   nothing runs `codex login` or `codex logout` against a real home: both revoke the login
   stored there.

4. Measure the tool, do not guess at it. Claude Code's behaviour here is undocumented,
   Codex's moves with its source, and both ship several times a week. A claim about either
   needs an experiment or a reading of a named build, and it belongs in a test, the tool's
   register or the commit message.

## The site

`website/` is where usepitboard.com will be built, with Astro; it is empty until then.
`docs/` holds the documentation source for docs.usepitboard.com. Neither is published by
this repository yet, and the README and the app already link to the documentation, so it
has to be standing before the next release.

## The state file

`state.json` carries a `schema`. The app updates the command line inside it, but a command
line installed some other way updates by its own route, so on one machine an older
pitboard will meet a file a newer one wrote. Reading forwards is `state::migrate`: each
bump adds an arm that rewrites the document and falls through to the next. Reading
backwards is not possible and says to update the pitboard that is behind. A bump needs a
test that loads a file the previous version wrote.

Schema 4 records each account's tool and keeps which account is signed in per tool. A
schema 3 file is brought forward on its first read, with nothing in the keychain or the
vault touched. A file naming a tool this build does not know is reported as written by a
newer pitboard, not as corrupt.

## Releasing

A tag `v<version>` releases; a tag like `v0.2.0-rc1` is a pre-release, which skips
crates.io and publishes no update feed, so nobody's installed copy updates into it. The
guard job refuses a tag that disagrees with the manifest or has no CHANGELOG section.

A release publishes the crates to crates.io, the command line for four targets, the app
with the command line for both macOS targets inside it, and the Homebrew tap, signed and
notarised when these repository secrets are set. Without them the release still happens
and the app is signed ad-hoc, which Gatekeeper warns about. It publishes no source
tarball: nothing installs from one, and crates.io has the source.

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
5. Change `README.md` in `datlechin/homebrew-tap` to the two commands this repository's
   README gives, `brew install datlechin/tap/pitboard` and
   `brew install --cask datlechin/tap/pitboard-app`. The `tap` job writes the casks and
   not that file, which still gives `--cask datlechin/tap/pitboard` for the app.
6. Push a pre-release tag, `v<next>-rc1`, and watch the publish job. It exchanges the
   crates.io token and uploads nothing, which is where a registration that does not match
   is meant to be found out.

Then `CARGO_REGISTRY_TOKEN` can be deleted from this repository's secrets, and the token it
held revoked on crates.io.

The `tap` job reads `SHA256SUMS` back from the release it has just published and fills the
placeholders in `packaging/pitboard.rb`, the command line, and `packaging/pitboard-app.rb`,
the app. It commits them to the tap as `Casks/pitboard.rb` and `Casks/pitboard-app.rb`
with `packaging/tap_migrations.json`, and removes `Formula/pitboard.rb`, in one commit. It
refuses to push a cask with a placeholder left in it or a line missing from `SHA256SUMS`.
Those files are its alone, and an edit made to them in the tap is gone at the next release.

If the tap push fails, re-run the `tap` job. There is no script for doing it by hand any
more: the checksums come from the `SHA256SUMS` the release computed, and a second download
somewhere else is what this replaced.

The tap has two casks and no formula. `pitboard` installs the command line from the
release's tarball for the machine, on macOS and Linux, and `pitboard-app` installs the app
and links the command line inside it onto `PATH`. They conflict, since both link
`bin/pitboard`. `tap_migrations.json` moves anyone still on the old formula to the cask of
the same name; Homebrew does that only when that cask is trusted, and otherwise prints what
to run. Somebody on the old app cask has their app replaced by the command line once. That
cost was accepted, and the CHANGELOG and the `pitboard` cask's caveats say how to get the
app back.

The `brew` job then installs from the public tap the way the README says, on a clean macOS
runner and a clean Linux one: the `pitboard` cask on both, checking the version, the man
page and the completions, and on macOS `pitboard-app` in its place, checking that the
`pitboard` on `PATH` is the one inside the app.

### Rotating the update key

For a plain `.app` zip update, which is what pitboard ships, Sparkle takes an update when
either the archive's EdDSA signature verifies under the public key in the installed bundle
or the new bundle satisfies the installed bundle's designated requirement. One of the two,
not both; its own source says this is so that a key can be rotated without breaking the
chain of trust. When the EdDSA check is the one that failed, the archive must also verify
under the key the new bundle carries, so the copy comes out able to take the release after
this one.

There are therefore two routes, and what is still in hand decides which.

The code signing route is one release. The bundle carries the new public key, the archive
is signed with the new private key, and the app is code signed with the same Developer ID
as the copies already out there, which take it through the designated requirement and come
out trusting the new key. It needs no signature from the old key, so this is the route when
the update key is gone rather than merely suspect. It is also the route for a new Developer
ID certificate with the update key unchanged: a certificate reissued for the same team
still satisfies the designated requirement, a different team does not.

The EdDSA route is two releases and depends on nothing but the update key, so it is the one
to use when the certificate is in doubt as well, and the only one that reaches a copy
installed from an ad-hoc signed build, whose designated requirement is its own cdhash. It
signs with the current key, so it is a way off a key that is suspect and not a way back
from one that is lost.

1. A release signed with the current key whose bundle carries the next public key.
   `generate_appcast` will not sign a bundle carrying a key other than the one it is
   handed, and refuses by writing the feed with no signature and exiting 0, so this rung is
   made by generating the feed with the next key and then replacing the `edSignature` with
   one `sign_update` makes from the current key.
2. A release signed with the next key.

The floor is the version of rung one. A copy older than it never learned the next key and
has nothing to check rung two with, so it stays where it is until somebody installs it
again with `brew install --cask datlechin/tap/pitboard-app`. Say the floor version out
loud in the release notes.

Either route changes the key in the bundle, so set the repository variable
`SPARKLE_KEY_ROTATION` to that version first or the app job refuses the build. Rung two
ships the same key as rung one, so it does not need the variable and should not have it.

`.github/workflows/rotation.yml` runs the EdDSA route every month against two keys it makes
on the runner, a feed on `127.0.0.1`, and bundles under an `invalid.` identifier, so the
procedure is one that has been executed rather than one that has been written. It names no
repository secret, which is what stops it reaching the real key, and CI checks that it
still names none.

If both the update key and the certificate are gone there is no route: nothing an installed
copy will accept can be made. Installing the app again is the only way back, which is the
argument for keeping the two in different places.

## Measured, not assumed

These decide the design, and each was measured rather than reasoned about:

- `security -i` reads 4097 bytes of command line, no continuation. Its `-w` prompt reads 128.
- Writing a keychain item in process, through the Security framework, makes every later
  read of that item by `security` take about a second instead of 0.01, for good.
- A running Claude Code session picks up a swapped credential within about 33 seconds. A
  running Codex never does.

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
- The facts in `provider/claude/assumptions.rs` carry the literals they are readable by, and
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

Read against codex-cli 0.154.0: the binary, its public source at tag `rust-v0.154.0`, and a
real `auth.json` that build wrote. The register is `provider/codex/assumptions.rs`, still
green on 0.156.1:

- The login is `$CODEX_HOME/auth.json`, default `~/.codex/auth.json`, mode 0600. `file` is
  the packaged default store; `keyring`, `auto` and `ephemeral` are the others. `keyring`
  and `auto` use a keychain item, `Codex Auth`, that Codex makes through the Security
  framework and that does not trust `/usr/bin/security`, which is why pitboard refuses them.
- `codex login` and `codex logout` both POST the stored refresh token to
  `https://auth.openai.com/oauth/revoke` before clearing it. This is why a Codex park is a
  move and never a copy (`ParkSemantics::MoveOnly`).
- A running Codex holds its login in memory for the life of the process, watches no file,
  and refuses a reload whose account id has changed (`Adoption::RestartRequired`). A refresh
  already under way when the file changes writes its own account's tokens under whatever
  account id it finds there.
- Codex writes `auth.json` with no lock of any kind, so there is none for pitboard to share.
- The ID token names the account: `email`, and under `https://api.openai.com/auth`,
  `chatgpt_account_id`, which a Team or Business workspace shares, and `chatgpt_user_id`,
  the person. pitboard identifies an account by the pair, with no network call.
- Renewal is `POST https://auth.openai.com/oauth/token` with a JSON body
  `{client_id, grant_type, refresh_token}` and client id `app_EMoamEEZ73f0CkXaXp7hrann`.
  Each token in the answer is written only if present. `last_refresh` must be there, as an
  RFC 3339 string, or Codex reads the login as having no token data. A spent or revoked
  refresh token answers 400 `invalid_grant` (or one of the older `refresh_token_expired`,
  `refresh_token_reused`, `refresh_token_invalidated`), or 401; any other 400 is not a dead
  login.
- Usage is `GET https://chatgpt.com/backend-api/wham/usage` with `Authorization: Bearer` and
  `ChatGPT-Account-ID`, no quota spent. Its shape was read from a live answer, not the
  source: a parser written from the source found the windows in the wrong place and returned
  nothing while the request succeeded.
- `CODEX_HOME` moves everything Codex keeps, and an empty one means unset, so the private
  sign-in always sets it to a directory that exists and runs `codex login` from inside that
  directory. `codex login` revokes what is stored in that home before signing in, opens the
  browser itself, and reads nothing from stdin.

Read on 2026-09-22, against Sparkle 2.10.0, Homebrew 7.0.6 and the tap as it then stood.
These decide how a release is allowed to move:

- `generate_appcast` cross-checks the private key it is given against the bundle's
  `SUPublicEDKey`, and when they disagree it writes the feed with no `sparkle:edSignature`
  at all and exits 0, saying "Wrote 1 new update". Tried with a key that was not base64 and
  again with a valid key that was simply a different one: no signature either time. Two
  consequences. A release whose `SPARKLE_PUBLIC_KEY` and `SPARKLE_PRIVATE_KEY` drift apart
  would publish a feed nobody can install, so the app job greps for the attribute. And the
  first rung of an EdDSA-only rotation cannot be made by `generate_appcast` at all.
- `sign_update --verify` takes the private key and derives the public one from it, so
  the job that signed a feed could only ever agree with its own arithmetic. CryptoKit's
  `Curve25519.Signing` verifies the same signature from the public key alone: exit 0 with
  the right key, exit 1 with a different one, over a signature `sign_update` had just made.
  The feed job checks the published feed against the key in the published bundle now, and
  reads no secret.
- `sign_update --ed-key-file` accepts a bare base64 of 32 random bytes, so the rehearsal
  makes keys with `openssl rand -base64 32` and never touches a keychain.
- Sparkle's `SUUpdateValidator.m` takes a plain `.app` zip update when either the
  archive's EdDSA signature verifies under the installed bundle's public key or the new
  bundle satisfies the installed bundle's designated requirement, and says in a comment that
  this is what allows key rotation. This was read, not run. What was run: an ad-hoc signed
  bundle's designated requirement is a list of cdhashes, so for a copy installed from an
  ad-hoc build there is no code signing route and the EdDSA one is all there is.
- `pitboard doctor` exits 3 where Claude Code has never run, which is every clean runner, so
  the job that installs from the tap treats 0 and 3 as the binary having run its checks and
  anything else as it having failed to.
- `cargo cyclonedx` writes a bill of materials beside every `Cargo.toml` in the
  workspace whatever `--manifest-path` says, so the release keeps the one belonging to the
  crate in the artefact and deletes the rest. A target that is not installed still resolves.
  The two macOS targets resolve to the same 83 components, which is why the app has one bill
  of materials and not two; macOS and musl differ by 7, which is why each target has its own.
  The app's is read from the bindings crate, and the command line it carries adds 4 more.
- crates.io issues a Trusted Publishing token that lasts 30 minutes, and matches on
  repository owner, repository name, workflow filename and, when it is given one, the
  environment. Registration is per crate, so `pitboard` and `pitboard-core` each need it.
- The tap was being written by `follow-releases.yml` inside `datlechin/homebrew-tap`, on
  `17 */6 * * *`. A release was therefore finished and green up to six hours before anyone
  could install what it published, and `packaging/pitboard.rb` in this repository still said
  v0.1.2 while the tap served 0.2.0 and the workspace was at 0.2.0. Nothing anywhere
  compared the three.

Measured on 2026-09-24 in a Homebrew 7.0.6 of its own, against a copy of the tap in each
shape it could take, on macOS and on Linux. These decide what the casks may do:

- A cask's `uninstall` directives run on every upgrade and reinstall, not only on removal,
  so a cask that took the renewal schedule away there would take it away at every release.
  Both casks take it away in `zap`, which only `brew uninstall --zap` runs. Homebrew's
  source has `zap launchctl:` look in the system domain too, with `sudo`, so it can ask for
  an administrator's password; that was read, not run.
- Neither cask's `zap` touches `~/.pitboard`. `state.json` is the only index of the parked
  logins in the keychain, and deleting it without `pitboard uninstall` leaves live refresh
  tokens nothing can name.
- From Homebrew 6, installing a full name trusts that one cask or formula and nothing else.
  The old app cask depended on the formula, which Homebrew then refused to build, so
  `brew install --cask datlechin/tap/pitboard` failed with `build.rb ... exited with 1`
  unless the formula was installed first.
- `tap_migrations.json` moves an installed formula to the cask of the same name only when
  that cask is trusted and some cask has been installed before. Otherwise `brew update`
  prints two commands, which leave the formula linked in front of the cask, so the
  CHANGELOG says to uninstall the formula first. Trust goes by name and type, so somebody on
  the old app cask already trusts the cask `pitboard`, and `brew update` replaces their app
  with the command line.
- Nothing moves an app from the cask `pitboard` to `pitboard-app` while `pitboard` is still
  a cask: `cask_renames.json` wins every lookup of the old name, hiding the command line,
  and `brew audit` rejects it; `old_tokens` redirects nothing. `conflicts_with formula:` no
  longer exists, only `cask:`.
- `binary`, `manpage` and the three completion stanzas work on Linux, and `zap launchctl:`
  does nothing there.

## A tool's register and the conformance run

Each tool keeps its own register, `crates/pitboard-core/src/provider/<tool>/assumptions.rs`,
dated against the build it was read from. `pitboard-conformance` reads the literals each
fact is readable by out of a build and says which are still there:

```sh
cargo run -p pitboard-conformance -- <a claude binary>
cargo run -p pitboard-conformance -- <a codex binary> --provider codex
```

Add `--json` for a report a program can read. It exits 1 when a fact has moved: a literal
it needs is gone, or one it rules out has turned up. Point it at the native `codex` binary,
not the npm wrapper `@openai/codex`. The binary is under `vendor/` in the platform package,
such as `@openai/codex@<version>-linux-x64`, which is where
`.github/workflows/conformance.yml` gets it. That workflow checks the newest build of
each tool against its own register twice a week, and can be run by hand for a given
version.

To add a fact, add an `Assumption` to that tool's register: what pitboard believes
(`fact`), where in the tool it was read (`read_from`), the build (`verified_against`), and
what in pitboard stops being true if it moves (`depends`). `probe` lists literals that must
be in a build for the fact to still be readable there; `absent` lists literals whose
arrival would disprove it. Pick literals specific to the fact: one already in the build for
another reason proves nothing. A fact about behaviour with no literal to find gets an empty
`probe`, and the run reports it as not readable rather than as holding. Run the checker
against the build you read the fact from, and against an older build that predates it if
you can, to see it go red. `cargo test` checks that every name is unique and that every
fact says what it is, where it was read, which version, and what depends on it.

Adding a tool takes three things: a register read out of a named build of it, a module
under `provider/` that implements `Provider`, and a conformance job for it. `ProviderId`,
`ProviderId::ALL` and the matches in `provider::of` and `assumptions::of` name every tool,
so the compiler and the tests point at what a new one has to fill in.

## Dependencies

Every new dependency needs a reason in the pull request. `cargo deny check` must pass.
