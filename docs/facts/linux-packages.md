# What the Linux packages rest on

Read on 28 October 2026 in the sources named, and measured where a line says so: in Docker on
the owner's Mac, as uid 1000 in
`debian:13@sha256:8c4733a92aacd0bcff3779eadc35a4a5147f36cb20a5fcb56504cbc740212259` (arm64) with
Debian's apt 3.0.3, dpkg 1.22.22, rpm 4.20.1+dfsg-3 and gpg 2.4.7-21+deb13u1+b5 of that day, and
on the Mac with GnuPG 2.5.24 and libgcrypt 1.12.4.

## Versions

- deb-version(7), dpkg-dev 1.22.22: without a revision "the upstream-version may not contain a
  hyphen", and a tilde "sorts before anything, even the end of a part". Measured:
  `dpkg --compare-versions` puts `1.0.0-beta.6` above `1.0.0`, and `1.0.0~beta.6` below it.
- rpm-version(7), RPM 6.1.90's page: "the dash character (-) can NOT be used in VERSION or
  RELEASE as it is the component separator", and "The tilde operator causes a segment to sort
  older, and is used for pre-release versions."
- Measured: dpkg and rpm 4.20.1's `rpm.vercmp`, through `rpm --eval` and its Lua, order
  `linux-versions.sh`'s versions as the releases come, `1.0.0~beta.6` to `1.10.0`
  (`test_package_versions_rise_with_the_release`). rpm 4.20.1 ships no `rpmvercmp` program, and
  its `rpmsort` put `1.0.0` before `1.0.0~beta.6`.
- GitHub's REST reference, "Release assets", API version 2026-03-10: "GitHub renames asset
  filenames that have special characters, non-alphanumeric characters, and leading or trailing
  periods." A tilde is one, so a release's file names keep the tag's version.

## The repository key

- Its packets, as `gpg --list-packets` reads `packaging/linux/pitboard-archive-keyring.asc` on
  2.4.7 and 2.5.24 alike:
  - a v4 RSA 4096 primary made 2026-10-28, and the user ID "Pitboard Packages";
  - the user ID's self-signature, class 0x13, hash 8 (SHA2-256), key flags 01 (certify), the key
    expiring 10 years on, at 2036-10-25 (2108545210);
  - a v4 RSA 4096 subkey made the same day, and its binding, class 0x18, hash 8, key flags 02
    (sign), expiring 2 years on, at 2028-10-27 11:00:12 UTC (1856257212);
  - in the binding, the subkey's back signature, class 0x19, hash 8.
- Debian 13's apt 3.0.3 ships `/usr/share/apt/default-sequoia.config`, its "Default APT Sequoia
  configuration", which sets `sha1.second_preimage_resistance = 2026-02-01`,
  `sha224 = 2026-02-01` and `rsa2048 = 2030-02-01`. So `linux-repository-key.sh` refuses every
  signature under SHA-256.
- GitHub CLI's issue cli/cli#9569, opened 2024-09-05: its repository key expired on 2024-09-06 at
  11:17 UTC, the extended key was published at cli.github.com, and `apt update` said "The
  following signatures were invalid" until people fetched the keyring again.

## What gpg does, as the check and its tests use it

Measured on 2.4.7 and 2.5.24 unless a line names one.

- `--with-colons --show-keys` lists the same fields on both: the expiry in the seventh, the
  capabilities in the twelfth, a key's own in lowercase and the whole key's in uppercase, and a
  `fpr` record after each key with `--with-subkey-fingerprints`.
- A subkey whose primary has expired by gpg's clock is listed with the primary's expiry. While
  both last, it is listed with its own. An expired subkey keeps its `s`.
- The second field is a key's validity, which `--show-keys` gives in a fresh home too: `r` for a
  subkey its primary revoked, as `--edit-key`'s `revkey` revokes one, and `e` for one expired by
  gpg's clock, beside `-` for a subkey that is neither. The check leaves out a subkey listed `r`
  or `e`, so a key that replaced its signing subkey passes; before, it was refused as having two
  signing subkeys.
- `--show-keys` and `--list-packets` start no gpg-agent and no keyboxd, and with `--quiet` print
  nothing on stderr. A fresh home holds `pubring.kbx` and `trustdb.gpg` afterwards.
- Neither version refuses to list a key whose self-signature, binding and back signature are
  SHA-1, so the check reads the packets.
- `--cert-digest-algo SHA1` makes a SHA-1 self-signature on a user ID. Adding a signing subkey
  with it fails, "Invalid digest algorithm", unless `--allow-weak-key-signatures` is given: 2.4.7
  says "third-party key signatures using the SHA1 algorithm are rejected" and "make_keysig_packet
  failed for backsig".
- An expiry given as a date is read at noon in the local time zone: `2026-07-01` became 05:00 UTC
  on the Mac at +0700 (2.5.24). `seconds=N` is exact, which the tests use.
- 2.5.24: `--quick-set-expire <primary> 2y <subkey>` writes a new binding and back signature in
  SHA2-256, 730 days from that day, and the export keeps only the new binding.
  `--export-secret-subkeys <subkey>!` leaves the primary a `gnu-dummy` stub. Signing with a subkey
  whose binding has expired fails with "Unusable secret key", and once its primary has expired,
  though its own binding lasts, gpg finds "no default secret key". RELEASING.md's extension and
  the check's count of the primary's expiry rest on these.
