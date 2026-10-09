# Releasing

A tag `v<version>` releases Pitboard through `.github/workflows/release.yml`. A tag with a
suffix, such as `v0.6.0-rc1`, makes a GitHub pre-release instead. A pre-release gets an
empty release body, publishes nothing to crates.io and has no update feed, so no installed
copy updates into it. The `feed`, `tap` and `brew` jobs skip it.

The `guard` job runs before anything is built. It refuses any tag whose version, without
the suffix, differs from `Cargo.toml`. For a full release, it also refuses a version with
no section in [CHANGELOG.md](CHANGELOG.md).

## Prepare a release

Pick the version first. While Pitboard is at 0.x, a release that removes or renames
anything public in `pitboard-core` is a new minor version, as Cargo reads one: 0.8.0 after
0.7.0, not 0.7.1. CI's `semver` job reports such a change against the version on crates.io.

- The first release after 0.7.0 is a new minor version at least.
  `pitboard_core::assumptions::Platform::ALL`, which meant macOS and Linux, is gone, and
  `assumptions::read_on` and `assumptions::verified_against` changed shape, now that each
  tool's register says which systems each fact was read on, Windows among them.
  `service::Pitboard::renew`, part of the supported interface, returns a `Result`, so that
  a run refused as root says so, and what changes anything outside `service`, such as
  `app::write_file`, takes a `service::Permit`. `host::Os` has a `Windows` variant, which
  breaks an exhaustive `match` on it, and `host::Os::make_private_command` returns an
  `Option`.
- Every 0.x release from then on compiles on Windows, and its Windows build answers only
  `--version`, `--help`, `completions` and `manpage`, saying "Pitboard for Windows is not
  released yet. This build changes nothing." (`pitboard_core::release`). Its notes say that
  Pitboard for Windows is not released. The crates' keywords and the README keep naming
  macOS and Linux alone until Windows is released. No release builds anything for Windows
  while the version's major is 0, and nothing a release builds, scripts or packages passes
  `--cfg pitboard_unreleased_windows`, `test-support` or `fixture`.
- The first release after 0.9.0 is 0.10.0. In `pitboard-core`, `state::State::active`,
  with `active_for`, `set_active`, `slot_for`, `set_slot` and `used`, is gone, now that
  the account index records whose login each tool has stored. So are
  `usage::Source::ClaudeCodeCache`, `usage::from_config_cache` and
  `usage::Snapshot::account_uuid`, now that Pitboard takes no usage from Claude Code's
  cache. `status::gather`, `autoswitch::Auto::Idle` and `Auto::NoRoom`,
  `autoswitch::Skip::SwitchInterrupted` and `Skip::CustomOauth`, and
  `error::Error::SwitchOvertaken` are gone too. `provider::Adoption`, `autoswitch::Auto`
  and `autoswitch::Skip` have new variants, which break an exhaustive `match` on them, and
  `Adoption` is no longer `Copy`. `usage::Snapshot`, `state::State`, `state::Account`,
  `statusline::StatusLine`, `service::Warning::FallbackLogin` and
  `switch::Outcome::AlreadyActive` have new fields. `doctor::FallbackLogin::fingerprint`
  and `words::not_switching` changed shape. The `--json` contract loses the check
  `usage_cache`, the `usage.source` value `claude_code_cache` and the error code
  `switch_overtaken`, and `pitboard watch` gives `switch_interrupted` and
  `custom_oauth_endpoint` as reasons of `not_watching`, where they were reasons of
  `skipped`. The account index is schema 6, which 0.9.0 refuses, so the release's notes
  say to update the command line and the app together.

1. In CHANGELOG.md, add `## [<version>] - YYYY-MM-DD` directly under `## [Unreleased]`, so
   the entries there fall under the version. The guard looks for a line that starts
   `## [<version>]`.
2. At the bottom of CHANGELOG.md, point `[Unreleased]` at `v<version>...HEAD`. Add a
   `[<version>]` line that compares the previous tag with `v<version>`.
3. In `Cargo.toml`, set `version` under `[workspace.package]` to the version. Set the
   `pitboard-core` line under `[workspace.dependencies]` to `version = "=<version>"`.
4. Run `cargo update --workspace`, so `Cargo.lock` records the version. CI and the release
   build with `--locked`, which fails on a lock file that needs updating.
5. Commit the three files to `main` as `Release <version>`. Both tags in the next section
   go on this commit.

Nothing in the Xcode project changes. `apps/macos/scripts/build-app.sh` takes the app's version
and build number from `Cargo.toml`.

## Make a release

1. Optional: tag the release commit `v<version>-rc1`, push the tag, and approve the
   `publish` job when it waits in the `release` environment. The run builds and signs
   everything and exchanges the crates.io token, but publishes nothing to crates.io. A
   Trusted Publishing registration that does not match fails here, where it costs nothing.
   Releases 0.3.0 to 0.5.0 each had an `-rc1` tag.

   ```sh
   git tag -s v0.6.0-rc1 -m "Pitboard 0.6.0-rc1"
   git push origin v0.6.0-rc1
   ```

2. Tag the same commit `v<version>` and push the tag.

   ```sh
   git tag -s v0.6.0 -m "Pitboard 0.6.0"
   git push origin v0.6.0
   ```

3. Approve the `publish` job when it waits in the `release` environment.
4. If the `tap` job fails to push, re-run it. It reads the same `SHA256SUMS` from the run.
5. If the `publish` job fails after both crates reached crates.io, re-run it. It skips
   crates.io when that version of `pitboard` is already there.

## What a release publishes

The jobs run in this order: `guard`, then `build` and `app`, then `publish`, then `feed`
and `tap`, then `brew`.

A full release publishes:

- `pitboard-core` and then `pitboard` to crates.io.
- The command line for four targets, `aarch64-apple-darwin`, `x86_64-apple-darwin`,
  `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`. Each tarball holds the
  binary, the man page, completions for bash, zsh and fish, `LICENSE`, `NOTICE` and
  `README.md`.
- The app, a universal build with the command line for both macOS targets inside it.
- A bill of materials beside each tarball and beside the app.
- `SHA256SUMS`.
- `appcast.xml`, the update feed.
- The Homebrew tap.

A pre-release publishes the same files on GitHub, without `appcast.xml`. No release
uploads a source tarball: nothing installs from one, and crates.io has the source. The
release body is the version's section of CHANGELOG.md.

When the Apple secrets are set, the macOS binaries and the app are signed with the
Developer ID certificate and notarised. The command line needs both, because a tarball
opened from a browser arrives quarantined. Gatekeeper stops a quarantined binary with an
ad-hoc signature.

Every file on the GitHub release has a GitHub attestation. `SHA256SUMS` is made in the job
that publishes the files it lists, so on its own it shows only that a download arrived
whole. The attestation also names the workflow and commit that made it.
[Verify a download](https://docs.usepitboard.com/install/verify) tells users how to check.

## Secrets and environments

| Secret | Where it comes from |
| --- | --- |
| `CERTIFICATES_P12` | The Developer ID Application certificate, exported from Keychain Access as .p12, `base64` |
| `CERTIFICATES_PASSWORD` | The password given to that export |
| `APPLE_API_KEY_P8` | An App Store Connect team key with the Developer role, `base64` |
| `APPLE_API_KEY_ID`, `APPLE_API_ISSUER` | Shown beside that key |
| `APPLE_ID` | Only needed if notarisation goes back to an app-specific password |
| `SPARKLE_PUBLIC_KEY`, `SPARKLE_PRIVATE_KEY` | `apps/macos/build/SourcePackages/artifacts/sparkle/Sparkle/bin/generate_keys --account pitboard` once, after `./apps/macos/scripts/build-app.sh` has fetched Sparkle there, then the same with `-x <file>`, which writes the private key to that file. The secret is that file's contents. Delete the file afterwards |
| `HOMEBREW_TAP_TOKEN` | In the `homebrew-tap` environment. A fine-grained personal access token, with `datlechin/homebrew-tap` as its only repository, Contents read and write as its only permission beyond the Metadata read access GitHub requires, and an expiry the maintainer will notice |

Without the Apple secrets, the command line and the app are signed ad hoc and not
notarised, and Gatekeeper warns about the app on first launch. The other secrets are
required. The `app` job fails without `SPARKLE_PUBLIC_KEY` or `SPARKLE_PRIVATE_KEY`, and
the `tap` job fails without `HOMEBREW_TAP_TOKEN`.

The signing identity is read from the certificate, so it needs no secret.

There is no `CARGO_REGISTRY_TOKEN` secret. crates.io gives the `publish` job a token in
exchange for GitHub's OIDC token, which names the workflow that is running. The token is
revoked when the job ends. This is crates.io's Trusted Publishing. It is registered for
`pitboard-core` and for `pitboard`, with owner `datlechin`, repository `pitboard`, workflow
`release.yml` and environment `release`.

Registration is per crate, and crates.io accepts one only for a crate it already has. A
crate added to the workspace without `publish = false` therefore needs a first upload by
hand, with an API token, and then its own registration. It is made on the crate's page on
crates.io, under **Settings** > **Trusted Publishing** > **Add** > **GitHub**, with the
same owner, repository, workflow and environment.

The `release` environment has required reviewers, and the `publish` job waits in it. An
unexpected tag stops there, before the crates.io upload, which is the one step of a
release that cannot be undone. The environment's name must match the one registered on
crates.io.

`HOMEBREW_TAP_TOKEN` is the only credential here that reaches another repository, so it is
kept in the `homebrew-tap` environment. An environment's secret is readable only by a job
that names that environment.

The repository variable `SPARKLE_KEY_ROTATION` is set only for a release that changes the
update key (see [Rotate the update key](#rotate-the-update-key)).

## The Homebrew tap

The `tap` job writes `datlechin/homebrew-tap` for every full release. Within the same run,
the `publish` job hands the `tap` job the `SHA256SUMS` it made of the files it published.
The `tap` job fills the version and checksum placeholders in `packaging/pitboard.rb` and
`packaging/pitboard-app.rb`, taking the checksums from that file.

It never reads the checksums back from the release. Anyone able to change the release
could change a file and its checksum line together.

In one commit, it writes the casks as `Casks/pitboard.rb` and `Casks/pitboard-app.rb`,
copies `packaging/tap_migrations.json` and `packaging/tap-README.md` (as `README.md`), and
removes `Formula/pitboard.rb`.

It refuses to push a cask with a placeholder left in it. It also refuses when `SHA256SUMS`
does not have exactly one line for a file a cask downloads.

An edit made in the tap is replaced at the next release, so change these files in
`packaging/`. There is no script to write the tap by hand: one would take its checksums
from a second download.

The tap has two casks and no formula. `pitboard` installs the command line from the
release's tarball for the machine, on macOS and Linux. `pitboard-app` installs the app and
links the command line inside it onto `PATH`. The two casks conflict, because both link
`bin/pitboard`.

`tap_migrations.json` moves anyone still on the formula of 0.3.0 and earlier to the cask
of the same name. When Homebrew does that is under [Homebrew](#homebrew).

Before 0.4.0, the cask `pitboard` installed the app. Anyone still on that old app cask has
their app replaced by the command line, once. That cost was accepted.

Four places give the same three commands to get the app back, in the same order. Change all
four together:

- The 0.4.0 entry in CHANGELOG.md.
- `packaging/tap-README.md`.
- The caveats in `packaging/pitboard.rb`.
- `docs/install/upgrade-from-0-3-0.mdx`.

After the `tap` job, the `brew` job installs from the public tap on a clean macOS runner and
a clean Linux one. It runs the two `brew install` lines that `README.md`,
`packaging/tap-README.md`, `docs/install.mdx` and `docs/quickstart.mdx` give, so change all
four and the job together. It installs the `pitboard` cask on both and checks the version,
`pitboard doctor`, the man page and the completions. On macOS, it then installs
`pitboard-app` in its place and checks that the `pitboard` on `PATH` is the app's.

## Rotate the update key

Pitboard ships a plain `.app` zip update. Sparkle takes one when either of two checks
passes, as read in Sparkle 2.10.0's source:

- The archive's EdDSA signature verifies under the public key in the installed bundle.
- The update's bundle satisfies the installed bundle's designated requirement.

When the EdDSA check fails, the archive must also verify under the key in the update's
bundle. Sparkle requires this so that the updated copy can take the next release. There
are two routes, and what is still in hand decides which.

The code signing route is one release. Its bundle carries the next public key, and its
archive is signed with the next private key. The app is signed by the same Developer ID
team as the installed copies. They take it through the designated requirement and come out
trusting the next key.

It needs no signature from the current key, so it is the route when the update key is
lost. A certificate reissued for the same team satisfies the designated requirement; one
from a different team does not. In the release workflow, this route means replacing
`SPARKLE_PUBLIC_KEY` and `SPARKLE_PRIVATE_KEY` with the next pair.

The EdDSA route is two releases and needs nothing but the update key. Use it when the
certificate is in doubt as well as the key. It is also the only route that reaches a copy
installed from an ad-hoc build, whose designated requirement is its own cdhash. It signs
with the current key, so it moves off a key that is suspect, not one that is lost.

1. A release signed with the current key, whose bundle carries the next public key.
   `generate_appcast` will not sign that bundle with the current key, so the feed is
   generated with the next key. Its `edSignature` is then replaced with one that
   `sign_update` makes with the current key.
2. A release signed with the next key.

The release workflow cannot make rung one. Its `app` job signs the feed with
`generate_appcast` alone. Its `feed` job checks the feed against the key in the bundle,
which in rung one is the next key. The step named "The two feeds" in `rotation.yml` shows
how rung one's feed is made. No workflow makes it with the real keys.

The floor is the version of rung one. A copy older than it never learned the next key, so
it can take rung two only through its designated requirement. A copy installed from an
ad-hoc build never meets that, and stays on its version until it is installed again with
`brew install --cask datlechin/tap/pitboard-app`. State the floor version in the release
notes, the version's section of CHANGELOG.md.

Either route changes the key in the bundle. The `app` job compares it with the key the
last full release shipped, and refuses a change unless the repository variable
`SPARKLE_KEY_ROTATION` names this version. Set it before you push each tag. The version is
the tag without its `v`, so `v0.6.0-rc1` needs `0.6.0-rc1` and `v0.6.0` needs `0.6.0`.
Rung two ships the same key as rung one, so it needs no variable, and one would let a
mistaken key change through.

`.github/workflows/rotation.yml` rehearses the EdDSA route on the first of each month
(`41 6 1 * *`), and can be run by hand. It uses two keys made on the runner, a feed on
`127.0.0.1` and bundles with the identifier `invalid.rehearsal.Pitboard`. It checks that
rung one reaches a copy on the current key, and that rung two reaches only a copy that took
rung one. It names no repository secret, so it cannot be given the real key; the workflow
and CI's `rehearsal` job both fail if it names one.

Keep the update key and the certificate in different places. If both are lost, no update
can be made that an installed copy accepts, and every copy has to be installed again.

## Measured facts

These decide what a release and the casks may do. Each was read or run on the date
given, against the version named.

### Sparkle and the release jobs

Read on 22 September 2026, against Sparkle 2.10.0.

- `generate_appcast` checks the private key it is given against the bundle's
  `SUPublicEDKey`. When they differ, it writes the feed with no `sparkle:edSignature` and
  exits 0, printing "Wrote 1 new update". It was tried with a key that was not base64 and
  with a different valid key; neither feed had a signature.
- The `app` job therefore greps the feed for the attribute. Without that, a release whose
  `SPARKLE_PUBLIC_KEY` and `SPARKLE_PRIVATE_KEY` drift apart would publish a feed nobody
  can install.
- `sign_update --verify` takes the private key and derives the public key from it, so a
  job that signed a feed can only agree with itself. CryptoKit's `Curve25519.Signing`
  verifies the same signature from the public key alone. A check written with it exited 0
  with the right key and 1 with a different one, over a signature from `sign_update`.
- The `feed` job therefore checks the published feed against the key in the published
  bundle, with `.github/scripts/eddsa-verify.swift`. It reads no secret.
- `sign_update --ed-key-file` accepts the base64 of 32 random bytes. So the rehearsal
  makes its keys with `openssl rand -base64 32` and never touches a keychain.
- Sparkle's `SUUpdateValidator.m` takes a plain `.app` zip update when either the EdDSA
  check or the designated requirement passes, and a comment says this allows key rotation.
  This was read, not run.
- An ad-hoc signed bundle was checked, and its designated requirement is a list of
  cdhashes. A copy installed from an ad-hoc build therefore has no code signing route.
- `pitboard doctor` exits 3 where Claude Code has never run, which is every clean runner.
  So the `brew` job accepts 0 and 3, and fails on anything else.
- `cargo cyclonedx` writes a bill of materials beside every `Cargo.toml` in the workspace,
  whatever `--manifest-path` says. The release keeps the one for the crate in each
  artefact and deletes the rest. A target that is not installed still resolves.
- The two macOS targets resolved to the same 83 components, which is why the app has one
  bill of materials. macOS and musl differed by 7, which is why each command line target
  has its own.
- The app's bill of materials is read from the bindings crate, `pitboard-ffi`.
  `.github/scripts/sbom-add-crate.py` folds in the command line inside the app, which added
  4 components, and the Share extension's `pitboard-share-ffi`, which added 1, itself, since
  everything it is built from is in `pitboard-ffi`'s too. That was measured on 5 October
  2026 with cargo-cyclonedx 0.5.9. `.github/scripts/sbom-add-sparkle.py` adds Sparkle,
  which `Cargo.lock` does not list, at the version the Xcode project's `Package.resolved`
  pins.
- crates.io issues a Trusted Publishing token that lasts 30 minutes. It matches on
  repository owner, repository name, workflow file name and, when one is given, the
  environment.

### Homebrew

Measured on 24 September 2026, on macOS and on Linux, in a separate installation of
Homebrew 7.0.6. A copy of the tap was tried in each shape it could take.

- A cask's `uninstall` directives run on removal, and also on every upgrade and reinstall.
  A cask that removed the renewal schedule there would remove it at every release. Both
  casks remove it in `zap`, which only `brew uninstall --zap` runs.
- Homebrew's source has `zap launchctl:` look in the system domain too, with `sudo`, so it
  can ask for an administrator's password. That was read, not run.
- Neither cask's `zap` touches `~/.pitboard`. `state.json` is the only index of the parked
  logins. Deleting it without `pitboard uninstall` leaves live refresh tokens in the
  keychain with nothing that records whose they are.
- `brew uninstall --zap` runs the zap of the cask as it was installed, not the tap's copy:
  `Cask::Installer#zap` loads the installed cask file first. On 25 September 2026, a cask
  was installed, its zap changed in the tap and `brew update` run. The zap that ran was the
  installed one.
- A machine still on the old app cask therefore runs that cask's zap, which moves
  `~/.pitboard` to the Trash. That is why CHANGELOG.md and the tap's README say to leave
  `--zap` out.
- From Homebrew 6, installing by full name trusts that one cask or formula and nothing
  else. The 0.4.0 entry in CHANGELOG.md records what this broke.
- `tap_migrations.json` moves an installed formula to the cask of the same name only when
  that cask is trusted and some cask has been installed before. Otherwise `brew update`
  prints two commands, which leave the formula linked in front of the cask. That is why
  CHANGELOG.md says to uninstall the formula first.
- Trust goes by name and type. Someone on the old app cask already trusts the cask
  `pitboard`, so `brew update` replaces their app with the command line.
- Nothing moves an app from the cask `pitboard` to `pitboard-app` while `pitboard` is still
  a cask. `cask_renames.json` wins every lookup of the old name, which hides the command
  line, and `brew audit` rejects it. `old_tokens` redirects nothing.
- Homebrew 7.0.6 has no `conflicts_with formula:`, only `conflicts_with cask:`.
- `binary`, `manpage` and the three completion stanzas work on Linux. `zap launchctl:` does
  nothing there.
