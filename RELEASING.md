# Releasing

A tag releases Pitboard through `.github/workflows/release.yml`. Its `guard` job reads the
tag with `version` under `[workspace.package]` in `Cargo.toml`, as
`.github/scripts/release-kind.sh` does, and refuses any other pair:

| Tag | `Cargo.toml` | Kind | crates.io | Latest | Windows files |
| --- | --- | --- | --- | --- | --- |
| `v1.2.0` | `1.2.0` | Full release | Yes | When no published full release is higher | Signed |
| `v1.2.0-rc1` | `1.2.0` | Release candidate, a GitHub pre-release | No | No | Signed |
| `v1.3.0-beta.1` | `1.3.0-beta.1` | Beta, a GitHub pre-release | Yes | No | Signed once a signer is set |

An rc's number starts at 1, a beta's runs from 1 to 98, and a beta's major version is 1 or
more. `usepitboard.com/install.sh` and the apps' feeds lead to GitHub's latest release, so a
0.x release made after 1.0.0 never moves them back. A release ships Windows files from major
version 1, and the guard refuses an rc or a full release while no signer is set
([Windows signing](#windows-signing)).

## Prepare a release

The first release with Claude Desktop waits until the hand measurements M1 to M10 in
[ARCHITECTURE.md](ARCHITECTURE.md#claude-desktop) are recorded. The first release that signs
Pitboard for Windows, and 1.0.0, also rewrite what
[Site and docs tied to Windows' status](#site-and-docs-tied-to-windows-status) lists.

1. Pick the version: minor for anything people would notice, patch for fixes alone.
2. In `CHANGELOG.md`, add `## [<version>] - YYYY-MM-DD` directly under `## [Unreleased]`.
   The entries under it become the release notes.
3. At the bottom of `CHANGELOG.md`, point `[Unreleased]` at `v<version>...HEAD`. Add a
   `[<version>]` line that compares the previous tag with `v<version>`.
4. For every release, betas included, add a line for the version in
   `website/src/data/highlights.ts`, 60 characters at most. The site's build fails without it.
5. In `Cargo.toml`, set `version` under `[workspace.package]`. `pitboard-core` has a
   version of its own: if `crates/pitboard-core`, or what it takes from the root
   `Cargo.toml`, changed since that version reached crates.io, raise it in
   `crates/pitboard-core/Cargo.toml`, as a patch, or as the minor an API break has already
   raised there ([Submit a change](CONTRIBUTING.md#submit-a-change)).
   Put it on the `pitboard-core` line under `[workspace.dependencies]` as
   `version = "=<core version>"`. Run `cargo update --workspace`: CI and the release build
   with `--locked`.
6. Commit as `Release <version>` and land it on `main`. Nothing else names the version:
   `build-app.sh` takes the app's version and build number from `Cargo.toml`.

## Make a release

7. Optional: tag the `main` commit that holds `Release <version>` as `v<version>-rc1`, push
   the tag, and approve the `publish` job.

   ```sh
   git tag -s v<version>-rc1 -m "Pitboard <version>-rc1"
   git push origin v<version>-rc1
   ```

   A release candidate builds, signs and attests everything, and runs its own install script
   on each target. It also exchanges the crates.io token, so a wrong Trusted Publishing
   registration fails here, where it costs nothing. It uploads nothing to crates.io, has no
   release notes or update feeds, and skips the `feed`, `windows-feed-key`, `windows-feed`,
   `tap`, `brew`, `scoop` and `winget` jobs.

8. Tag the same commit `v<version>` and push the tag.

   ```sh
   git tag -s v<version> -m "Pitboard <version>"
   git push origin v<version>
   ```

   The `guard` job refuses a tag the table above does not allow, a full release or a beta
   with no section in `CHANGELOG.md`, and a `pitboard-core` that changed while its version
   stayed. Releases run one at a time, so this run waits until the candidate's has ended.

9. Approve the `publish` job when it waits in the `release` environment.
10. Watch the run to the end. A re-run fixes nine failures:

| Job | Failed | Do |
| --- | --- | --- |
| `windows-sign` | Anywhere | Re-run it. SignPath evaluates at most three re-runs of a run, and fails the fourth |
| `windows-sign-msi` | Anywhere | Re-run it, as `windows-sign` |
| `windows-update-sign` | Anywhere | Re-run it. It signs the MSIs `windows-msi` checked again and, for the latest full release, replaces the feed it handed on |
| `publish` | After the crates reached crates.io | Re-run it. It skips each crate whose version is there |
| `tap` | At the push | Re-run it. It reads the run's own `SHA256SUMS`. Never write the tap by hand: its checksums would come from a second download |
| `scoop` | At the push | Re-run it, as `tap`. Never write the bucket by hand |
| `winget` | Before "Submit" | Re-run it |
| `install-script` | At "Install the way the README says to" | Check [the redirect](#the-redirect), then re-run it |
| `site` | Anywhere | Re-run it. Workers Builds answers a second request with the build already queued |

Never re-run `winget` once its "Submit" step has started: komac may have opened a pull request
on `microsoft/winget-pkgs`. Answer in that pull request.

An rc or beta number is used once. A fix that needs another commit takes the next number,
and a published tag never moves. The workflow decides which release is latest: never mark
one by hand.

## 0.x after 1.0.0

A 0.x release made after 1.0.0 is never the latest. It ships from a `release/0.N` branch,
protected like `main` and cut from the last 0.x tag. A release runs the `release.yml` of the
commit it tags, so that tag must hold the release kinds above: release a 0.x with them
before 1.0.0-beta.1.

On `release/0.15`, `pitboard-core` takes the next patch crates.io does not hold, past the
0.15.1 to 0.15.3 the 1.0 betas published, and the `semver` job checks it against 0.15.0.

0.15 gets security fixes for three months after 1.0.0 is published. Then take its row out of
Supported versions in `SECURITY.md`, with the sentence on 0.15 releases under it.

## Before 1.0.0-beta.1

Tag the first release with Windows files only once each of these holds:

- No Rust or C# test waits on a later Windows pull request: none is ignored or excluded on
  Windows with a `W<id>` reason.
- The session on [the session machine](AGENTS.md#the-session-machine) is done, on the bundle
  built from `Release 1.0.0-beta.1`, and `ARCHITECTURE.md` records what it measured, with the
  build and the date.
- The last 0.x release is published with the release kinds above, and `release/0.N` is cut
  from its tag.
- `windows-release` passed on the pull request that holds `Release 1.0.0-beta.1`.
- `WINDOWS_UPDATE_PUBLIC_KEY` and the `windows-update` environment's
  `WINDOWS_UPDATE_PRIVATE_KEY` are set, as [the Windows update key](#the-windows-update-key)
  says, and `WINDOWS_SIGNER` is unset.

## What a release publishes

Each release has a tarball of the command line for each target of the `build` job, the app
as a zip, a bill of materials beside each, `install.sh` and `SHA256SUMS`. The latest release
also has `appcast.xml`, the update feed, and writes the Homebrew tap. A full release or a
beta uploads `pitboard-core`, when its version moved, and then `pitboard` to crates.io.
Every file on the GitHub release has a GitHub attestation. From major version 1, each
release also has a zip of the command line for Windows on x64 and on ARM64, built and checked
by `.github/actions/windows-release`, which CI runs on every pull request, and signed as
[Windows signing](#windows-signing) says. The `windows-install` job downloads each published
zip on its machine, checks it against `SHA256SUMS`, its attestation and its signatures, and
runs its command line as a fresh standard user. For a full release or a beta it also installs
the zip with cargo-binstall from crates.io, with `compile` disabled, so a `pkg-url` in
`crates/pitboard/Cargo.toml` that names no zip fails the job instead of building from source.

From major version 1, each release also has the app for Windows, an MSI for x64 and one for
ARM64, `Pitboard-v<version>-windows-<arch>.msi`, each with a bill of materials. `app-windows-build`
builds the app on each machine, `windows-sign` signs Pitboard's own programs in it with the zips,
and `app-windows-package` builds the MSI from them with the signed zip's `pitboard.exe` in `bin`
and its completions, so the zip and the MSI carry one build. It fails where a key path has
another component GUID than in the newest earlier release's MSIs, or a GUID another key path:
an upgrade removes the old product after the new one's files are in, and would take that file
away. CI builds and checks an MSI the same way on every pull request. `windows-sign-msi` signs
both MSIs, and `windows-msi` checks each on its own machine.
The latest full release also has `appcast-windows.xml`, the feed the Windows app updates from,
which `windows-update-sign` signs with [the Windows update key](#the-windows-update-key). The
`windows-install` job downloads each MSI, checks it against `SHA256SUMS`, its attestation and its
signatures, and extracts it with `msiexec /a`. No job installs an MSI. `windows-feed-key` and
`windows-feed` fetch the Windows feed as an installed copy does and check each MSI in it against
the key in the published MSIs. `publish` has 15 minutes, for the MSIs' upload, a limit
1.0.0-beta.1 is the first release to try.

The MSIs and their numbers follow the version in `Cargo.toml`, as
`.github/scripts/windows-versions.sh` prints them:

| `Cargo.toml` | MSI | Version resource and package | Windows feed |
| --- | --- | --- | --- |
| `1.0.0-beta.1` | `1.0.0.1` | `1.0.0.1` | None |
| `1.0.0` | `1.0.0` | `1.0.0.1000` | `1.0.0` |
| `1.1.0-beta.2` | `1.1.0.2` | `1.1.0.2` | None |

Windows Installer compares three fields, so a beta and its final are one version there, and
each MSI replaces the other. Settings > Apps shows the MSI's version.

The MSI is built with WiX v7. The owner accepted its Open Source Maintenance Fee EULA on
15 October 2026 (`AcceptEula` in `apps/windows/Pitboard.Installer/Pitboard.Installer.wixproj`).
Nothing is owed while Pitboard's revenue stays under US$10,000 a year and sponsorships stay
donations: WiX v7.0.0's `OSMFEULA.txt`, lines 12 to 23, and `.github/FUNDING.yml`.

Once a full release or a beta is published, the `site` job asks Workers Builds to build
usepitboard.com from `main` again, through `SITE_DEPLOY_HOOK`. The site lists the releases it
found when it was built, and Workers Builds builds only on a push to `main`.

Immutable releases are on, and only the owner changes that setting. A published release's
files and tag cannot change; its title, notes and marks can. Only the next version fixes a
mistake in a published file. A deleted release's tag name cannot be used again.

## Windows signing

Two repository variables, not secrets, name the signer:

| Variable | Value |
| --- | --- |
| `WINDOWS_SIGNER` | `signpath` or `sslcom`, unset until Pitboard for Windows has a signer |
| `WINDOWS_SIGNER_SUBJECT` | The subject the signer signs as, as `Get-AuthenticodeSignature` prints it |

From major version 1, the guard refuses an rc or a full release while `WINDOWS_SIGNER` is
unset, and any release that sets it without `WINDOWS_SIGNER_SUBJECT`. A beta with no signer
ships its Windows files unsigned. Once a signer is set, every Windows release is signed,
betas included: never unset it to ship one unsigned. When you set `WINDOWS_SIGNER`, set
`WINDOWS_SIGNED_FROM` in `website/src/data/site.ts` to the first version it signs:
usepitboard.com calls a beta's Windows files unsigned below that version.

The `windows-sign` job takes the zips `build-windows` made and the app's own programs and
sparse package from `app-windows-build`, in one request, and hands the zips to `publish` and the
app to `app-windows-package`. `windows-sign-msi` signs both MSIs in a second request. Both run
in the `windows-signing` environment, which takes deployments from `v*` tags only and holds the
signer's credentials. Make it before the first 1.x tag: a job that names a missing environment
makes one with no rule. `.github/scripts/verify-windows-signatures.ps1` checks every program in
the zips, the app's programs and package, and each MSI and the programs named for Pitboard in
it: signed by `WINDOWS_SIGNER_SUBJECT`, valid and timestamped, or, with no signer, not signed at
all. The MSI's other programs ship as their makers signed them, and the script holds each to
its maker: the .NET runtime's and the Windows App SDK's valid and signed by Microsoft
Corporation, and the Windows Community Toolkit's by the .NET Foundation. WinSparkle.dll ships
unsigned, as NuGet's WinSparkle 0.9.4 holds it, and the script refuses it signed until it names
the signer. An MSI is checked on its own machine,
`windows-msi`, since `msiexec /a` extracts one only where it could install it, and must hold
the files and tables it was built with. No signer has a signing step yet, so a set
`WINDOWS_SIGNER` fails the job.
Once one has, a signed release waits for two approvals at the signer, then one at `release`.

A release with no signer builds no sparse package, so its MSI carries no share target: only a
signature the machine trusts registers one, and Pitboard never asks anyone to trust a test
certificate.

Keep the signer's account apart from the update key and the Developer ID certificate.

## Secrets and environments

| Secret | How to make it |
| --- | --- |
| `CERTIFICATES_P12` | Export the Developer ID Application certificate from Keychain Access as .p12, then `base64` it |
| `CERTIFICATES_PASSWORD` | The password given to that export |
| `APPLE_API_KEY_P8` | An App Store Connect team key with the Developer role, `base64` |
| `APPLE_API_KEY_ID`, `APPLE_API_ISSUER` | Shown beside that key |
| `SPARKLE_PUBLIC_KEY`, `SPARKLE_PRIVATE_KEY` | `bin/generate_keys --account pitboard`, from Sparkle's release archive at the version `apps/macos/project.yml` pins, prints the public key. With `-x <file>` it writes the private key to `<file>`, whose contents are the secret. Delete the file afterwards |
| `HOMEBREW_TAP_TOKEN` | A fine-grained token with `datlechin/homebrew-tap` as its only repository, Contents read and write as its only permission, and an expiry you will notice |
| `SCOOP_BUCKET_TOKEN` | The same, for `datlechin/scoop-bucket` |
| `WINDOWS_UPDATE_PUBLIC_KEY`, `WINDOWS_UPDATE_PRIVATE_KEY` | As [the Windows update key](#the-windows-update-key) says. The private key is a secret of the `windows-update` environment alone |
| `SITE_DEPLOY_HOOK` | In Cloudflare, the `pitboard` Worker's **Settings** > **Builds** > **Deploy Hooks**: a hook that builds `main`. The secret is the whole URL. Anyone with it can start a build |
| `WINGET_TOKEN` | A classic token with the `public_repo` scope, of a GitHub account kept for it alone, which holds the fork of `microsoft/winget-pkgs` and can write to no `datlechin` repository. A classic token writes to every public repository its account can, the tap and the bucket among them. komac 2.16.0's README says a fine-grained token fails to open the pull request |

Without the Apple secrets, everything is signed ad hoc and not notarised, and Gatekeeper
warns about the app on first launch. The `app` job fails without the Sparkle secrets. The
`tap` job fails without `HOMEBREW_TAP_TOKEN`, and the `site` job without `SITE_DEPLOY_HOOK`. Without `SCOOP_BUCKET_TOKEN` or the bucket, the
`scoop` job writes nothing and says which is missing.

There is no `CARGO_REGISTRY_TOKEN`: the `publish` job gets a crates.io token through Trusted
Publishing. Both crates are registered with owner `datlechin`, repository `pitboard`,
workflow `release.yml` and environment `release`. crates.io registers only a crate it
already has. A crate added without `publish = false` needs a first upload by hand with an
API token, then its own registration under **Settings** > **Trusted Publishing** > **Add** >
**GitHub** on its crates.io page.

The `release` environment has required reviewers, and the `publish` job waits in it.
Renaming it breaks the crates.io registration. The `homebrew-tap` environment holds
`HOMEBREW_TAP_TOKEN`, `scoop-bucket` holds `SCOOP_BUCKET_TOKEN`, `winget` holds
`WINGET_TOKEN` and `windows-update` holds `WINDOWS_UPDATE_PRIVATE_KEY`. Give `release`,
`homebrew-tap`, `scoop-bucket`, `winget`, `windows-signing` and `windows-update` each a
deployment rule for `v*` tags, under **Settings** > **Environments** > **Deployment branches
and tags**, so a job run from a branch never reaches them.

[Rotate the update key](#rotate-the-update-key) covers the repository variable
`SPARKLE_KEY_ROTATION`, [the Windows update key](#the-windows-update-key)
`WINDOWS_UPDATE_KEY_ROTATION`, and [Windows signing](#windows-signing) `WINDOWS_SIGNER`,
`WINDOWS_SIGNER_SUBJECT` and the `windows-signing` environment.

## The Homebrew tap

For the latest release, the `tap` job writes `datlechin/homebrew-tap` from `packaging/`: the
two casks with the version and the run's checksums filled in, `tap_migrations.json`, and
`tap-README.md` as `README.md`. Change those files here. An edit made in the tap is replaced
at the next release.

`docs/install/upgrade-from-0-3-0.mdx` gives the commands to move off 0.3.0.
`packaging/tap-README.md` repeats them for readers of the tap. Change both together. The
`pitboard` cask's caveats link the docs page.

The `brew` job runs both `brew install` lines on clean runners. Change a line and the job
together: `rg -F datlechin/tap/pitboard` finds every copy.

## The Scoop bucket

From major version 1, the `scoop` job writes `datlechin/scoop-bucket` for the latest release,
as `tap` writes the tap: `packaging/scoop/pitboard.json` as `bucket/pitboard.json`, with the
version and the Windows zips' checksums filled in, and `packaging/scoop/README.md` as
`README.md`. A beta or an rc never reaches it. The `scoop-install` job then installs Scoop in
a folder of its own on both Windows machines, adds the bucket and installs `pitboard` from it.

Before the first 1.x release:

1. Make `datlechin/scoop-bucket`, public, with default branch `main` and one commit holding
   `packaging/scoop/README.md` as `README.md`. Not from Scoop's BucketTemplate: its Excavator
   would write the bucket too.
2. Make `SCOOP_BUCKET_TOKEN` as the secrets table says, in an environment `scoop-bucket` with
   a deployment rule for `v*` tags.

The bucket's page and the `scoop-install` job hold the same install lines. Change them
together: `rg -F datlechin/scoop-bucket` finds every copy.

## winget

Pitboard is two winget packages: `Pitboard.CLI`, the zips, each holding a portable
`pitboard.exe` with the command alias `pitboard`, and `Pitboard.Pitboard`, the app's per-user
MSIs once the release builds them. Since winget 1.29 an MSI wins over a portable under one
identifier, hence two. A beta never reaches winget.

For the latest release from major version 1, the `winget` job takes each package
`microsoft/winget-pkgs` already holds. komac writes its next version from the published files
with the run's read-only token, `.github/scripts/check-winget-hashes.py` checks each
installer's hash against the run's `SHA256SUMS`, and `winget validate` checks the manifests.
Then komac opens the pull request from the fork with `WINGET_TOKEN`. A package that is not in
`microsoft/winget-pkgs` yet, or has a pull request for this version, is skipped with a notice.
The validate step warns on every manifest komac writes: windows-2025's winget knows manifests
up to 1.10.0, and komac writes 1.12.0.

komac stays at 2.16.0, pinned by SHA-256 in `release.yml`: 2.17.0 and 2.17.1 write nothing to
`--output` with `--dry-run`. Before the first 1.x release, make the `winget` environment with a
deployment rule for `v*` tags, so the job never makes one with no rule. `komac new` asks
questions at a terminal, so the first version of each package goes in by hand, after 1.0.0 is
published:

1. Make a GitHub account for winget alone, with write access to no `datlechin` repository.
   Fork `microsoft/winget-pkgs` from it: komac opens the pull request from the fork of the
   account `WINGET_TOKEN` belongs to. Put `WINGET_TOKEN` in the `winget` environment as the
   secrets table says.
2. Run `komac new Pitboard.CLI --version 1.0.0 --publisher Pitboard --urls <x64 zip> <arm64
   zip> --dry-run --output <folder>`, and the same for `Pitboard.Pitboard` with the MSIs. Give
   the zips the command alias `pitboard`. Every later version keeps the publisher this one
   names.
3. Run `python3 .github/scripts/check-winget-hashes.py <manifests> <the release's SHA256SUMS>`
   and `winget validate --manifest <manifests>` on each, and check `Publisher: Pitboard` in
   the locale manifests and `Scope: user` on the MSIs.
4. Submit with `komac submit <manifests>`. Sign the Microsoft CLA in the first pull request, and
   answer a `Needs-Author-Feedback` label within 5 days: 3 days of silence after that closes
   the pull request.

## Site and docs tied to Windows' status

usepitboard.com reads Windows' status from the releases it builds from and from
`WINDOWS_SIGNED_FROM`: the `(beta)` on its download button, the beta and signing sentences on
`/download`, the version in Cargo's commands, the Scoop lines and **Check for updates** in the
menu figure follow by themselves. What follows is written by hand, so rewrite it at its step.

At the first signed Windows release, measure on
[the session machine](AGENTS.md#the-session-machine) what Microsoft Edge and Windows show as its
MSI downloads and opens, whether Smart App Control lets the app start, and whether a browser's
share sheet lists Pitboard once it is installed. Then rewrite, from what you measured:

- "and its files are not signed" in `README.md` and in `docs/install.mdx` under What you need,
  and the step of `docs/install.mdx` that opens the installer.
- The Windows tab of Check the signature in `docs/install/verify.mdx`, and "SignPath has signed
  no Pitboard file." in `docs/install/code-signing-policy.mdx`.
- The Smart App Control section of `docs/troubleshooting.mdx`.
- What `docs/guides/account-windows.mdx` and `docs/reference/account-windows.mdx` say of the
  share sheet on Windows, and the Share extension's line in `SECURITY.md`.
- The Share sentence of `#account-windows` in `website/src/pages/index.astro`, and the
  SmartScreen sentence in `website/src/components/InstallTabs.astro` and
  `website/src/pages/download.astro`.

At 1.0.0, rewrite:

- "Pitboard for Windows is in beta" in `README.md`, `SECURITY.md`, `docs/install.mdx` and the
  Systems section of `docs/AGENTS.md`.
- The paragraph of `docs/install/other-ways.mdx` that names the version for Cargo on Windows,
  and the cargo-binstall sentence of Install the command line on Windows in
  `docs/install.mdx`: from 1.0.0, `cargo binstall pitboard` installs a build that runs there.
- Add Scoop to `docs/install/other-ways.mdx`, with the lines of the bucket's page. Scoop checks
  each zip against the checksum the release wrote into the bucket, so add it to the Homebrew
  sentence and the Windows sentence of Check the checksums in `docs/install/verify.mdx`, and
  to the checksum bullet of `SECURITY.md`.

Once `microsoft/winget-pkgs` holds both packages, add their `winget install` lines to
`docs/install/other-ways.mdx`, and to `windowsWays` in `website/src/data/commands.ts`, which
`/download` shows under Other ways. winget checks each installer against the hash its manifest
holds, which `.github/scripts/check-winget-hashes.py` ties to `SHA256SUMS`, so add winget to the
same sentences of `docs/install/verify.mdx` and `SECURITY.md`.

## The install script

`packaging/install.sh` is a template. The `publish` job fills in the version and the
tarballs' checksums, so each release carries its own copy. People run:

```sh
curl -fsSL https://usepitboard.com/install.sh | sh
```

That reaches the copy in the release GitHub marks latest. For the latest release, the
`install-script` job runs the line on each target. Change the line and the job together:
`rg -F usepitboard.com/install.sh` finds every copy.

### The redirect

`usepitboard.com/install.sh` is a Cloudflare Single Redirect rule on the `usepitboard.com`
zone, in the owner's dashboard. Nothing in this repository deploys it, and no workflow holds
a Cloudflare token: `SITE_DEPLOY_HOOK` only starts a build of `main`. Whoever can edit the rule picks the script every reader runs. Guard that
Cloudflare account as closely as the `release` environment.

| Setting | Value |
| --- | --- |
| Rule name | `install.sh to latest release` |
| When incoming requests match | Custom filter expression `(http.host eq "usepitboard.com" and http.request.uri.path eq "/install.sh")` |
| Then | Static redirect to `https://github.com/datlechin/pitboard/releases/latest/download/install.sh` |
| Status code | 302 |
| Preserve query string | Off |
| Order | First among the Single Redirects |

It is a 302 because a client may cache a 301 past a change to the rule (RFC 9110, section
15.4.2). To check it, run this and look for a 302 to the GitHub address:

```sh
curl -sSI https://usepitboard.com/install.sh | grep -i -E '^HTTP|^location'
```

## Rotate the update key

Sparkle takes a plain `.app` zip update when either check passes, as read in Sparkle
2.10.0's `SUUpdateValidator.m`:

- The archive's EdDSA signature verifies under the public key in the installed bundle.
- The update's bundle satisfies the installed bundle's designated requirement.

When the EdDSA check fails, the archive must still verify under the key in the update's
bundle, so the updated copy can take the next release.

| Route | Releases | Needs | Use it when |
| --- | --- | --- | --- |
| Code signing | One | A Developer ID certificate of the same team | The update key is lost |
| EdDSA | Two | The current update key | The certificate is in doubt too, or copies from an ad-hoc build must follow |

On the code signing route, the bundle carries the next public key and the archive is signed
with the next private key: replace both Sparkle secrets with the next pair. A certificate
reissued for the same team satisfies the designated requirement; one from another team does
not.

On the EdDSA route:

1. Rung one is signed with the current key, and its bundle carries the next public key.
   `generate_appcast` will not sign that bundle with the current key. Generate the feed with
   the next key, then replace its `edSignature` with one `sign_update` makes with the
   current key.
2. Rung two is signed with the next key.

The release workflow cannot make rung one. Its `app` job signs the feed with
`generate_appcast` alone, and its `feed` job checks it against the next key in the bundle.
The step "The two feeds" in `rotation.yml` makes rung one's feed with keys made on the
runner. No workflow makes it with the real keys.

The floor is rung one's version. A copy older than it takes rung two only through its
designated requirement. A copy from an ad-hoc build never meets it, and stays on its version
until it is installed again with `brew install --cask datlechin/tap/pitboard-app`. State the
floor in the version's section of `CHANGELOG.md`.

The `app` job compares the bundle's key with the one the last full release shipped. It lets
a change through only when `SPARKLE_KEY_ROTATION` names this version, the tag without its
`v`. Set it before you push each tag of the release that changes the key: `<version>-rc1`,
then `<version>`. Rung two ships rung one's key, so never set the variable to its version: a
mistaken key would then pass.

Keep the update key and the certificate in different places. If both are lost, no update
reaches an installed copy, and every copy has to be installed again.

## The Windows update key

The Windows app updates itself through WinSparkle from `appcast-windows.xml`, which lists full
releases only: a beta updates itself into the next full release, never into another beta. Each
Release build carries the Windows update public key in `Pitboard.dll`, and WinSparkle installs
an MSI only when that key verifies it. WinSparkle checks no Authenticode signature, so there is
no second route: with the key lost, every Windows copy has to be installed again by hand. With
no key, or one it does not take, WinSparkle installs updates nobody signed, so the release
refuses to build one.

| Name | Kind | Read by |
| --- | --- | --- |
| `WINDOWS_UPDATE_PUBLIC_KEY` | Repository secret | `app-windows-build`, `app-windows-package` and `windows-update-sign` |
| `WINDOWS_UPDATE_PRIVATE_KEY` | Secret of the `windows-update` environment | `windows-update-sign` alone |
| `WINDOWS_UPDATE_KEY_ROTATION` | Repository variable | `app-windows-package` |

Before the first 1.x tag:

1. Run `bin/generate_keys --account pitboard-windows` from Sparkle's release archive at the
   version `apps/macos/project.yml` pins. Put the public key it prints in
   `WINDOWS_UPDATE_PUBLIC_KEY`.
2. `bin/generate_keys --account pitboard-windows -x <file>` writes the private key. Make the
   `windows-update` environment with a deployment rule for `v*` tags and put the file's
   contents in its `WINDOWS_UPDATE_PRIVATE_KEY`.
3. Keep an offline copy of the private key apart from the signer's account, the macOS update
   key and the Developer ID certificate.
4. Delete the file, and the key from the login keychain with `security
   delete-generic-password -s https://sparkle-project.org -a pitboard-windows`.

WinSparkle's keys and signatures are Sparkle's Ed25519 in base64. For every Windows release,
betas and release candidates included, `windows-update-sign` signs each MSI with `sign_update`
from that release archive, pinned by its SHA-256 in `release.yml`, and checks each signature
with `WINDOWS_UPDATE_PUBLIC_KEY`, so a private key that is not its pair fails the first beta.
For the latest full release it then writes the feed with `.github/scripts/windows-appcast.py`.

### Rotate the Windows update key

Only the EdDSA route exists. Rung one carries the next public key and its MSIs are signed with
the current key; rung two is signed with the next key. The feed offers only the latest release,
so a copy that has not taken rung one when rung two ships stays on its version until it is
installed again.

`app-windows-package` compares the key the MSI carries with the one the newest earlier release
with an MSI shipped, betas included. It lets a change through only when
`WINDOWS_UPDATE_KEY_ROTATION` names this version, the tag without its `v`: set it before you push
each tag of the release that changes the key. Rung two ships rung one's key, so never set the
variable to its version.

The release workflow cannot make rung one: `windows-update-sign` checks each signature with the
key the release carries, and `windows-feed` with the key in the published MSIs, which rung one
has moved on from.
