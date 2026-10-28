# What the Linux packages rest on

Read on 28 October 2026 in the sources named, and measured where a line says so: in Docker on
the owner's Mac, as uid 1000 in
`debian:13@sha256:8c4733a92aacd0bcff3779eadc35a4a5147f36cb20a5fcb56504cbc740212259` (arm64) with
Debian's dpkg 1.22.22 and rpm 4.20.1+dfsg-3 of that day.

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
