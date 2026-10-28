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

## The Debian packages

Measured on 28 and 29 October 2026 in Docker on the owner's Mac (arm64), as uid 1000 in Debian 13
from `debian:13@sha256:8c4733a92aacd0bcff3779eadc35a4a5147f36cb20a5fcb56504cbc740212259` with
debhelper 13.24.2, dpkg-dev 1.22.22, lintian 2.122.0, dh-shell-completions 0.1.0, meson 1.7.0,
binutils 2.44 and rustup's stable, Rust 1.99.1.

- `dh_auto_configure` for Meson runs `meson setup ../apps/linux --wrap-mode=nodownload
  --buildtype=plain --prefix=/usr --sysconfdir=/etc --localstatedir=/var
  --libdir=lib/aarch64-linux-gnu -Dpython.bytecompile=-1` and then rules' `-Dpackage=deb
  -Dplasma_widget=false`, in `obj-aarch64-linux-gnu`. Cargo's release build inside it ran with
  no network, and the 14 Meson tests passed under `dh_auto_test`.
- `ninja install` under `dh_auto_install` runs the Cargo step again, which `build_always_stale`
  asks for, and Cargo compiles `pitboard-linux` once more: the app's build script writes the
  metainfo in `OUT_DIR` after Cargo's `invoked.timestamp`, and `glib_build_tools::compile_resources`
  names that file `rerun-if-changed`. A third run compiles nothing.
- The command line from the tarball, built with musl-gcc from musl-tools 1.2.5-3.1~deb13u1 for
  aarch64-unknown-linux-musl, is static and stripped, with a `.comment` section. `strip
  --remove-section=.comment --remove-section=.note --strip-unneeded`, what dh_strip runs, takes
  168 bytes out of it, and `dwz` finds no `.debug_info`. With `dh_strip -Npitboard` and
  `dh_dwz -Npitboard`, the packaged `/usr/bin/pitboard` is the tarball's byte for byte (`cmp`),
  and dh_shlibdeps leaves it too.
- dpkg-shlibdeps(1): where a library's symbols file names a `Build-Depends-Package`, it takes
  "the minimal version required by the corresponding package in the Build-Depends field" when
  that is higher than the symbols' own. Debian 13's symbols files for libgtk-4-1,
  libadwaita-1-0 and libglib2.0-0t64 name libgtk-4-dev, libadwaita-1-dev and libglib2.0-dev.
  Measured on a C program calling `adw_init`, `g_application_get_default` and
  `gtk_get_major_version`: with those unversioned in Build-Depends it wrote `libadwaita-1-0
  (>= 1.0.0)`, `libglib2.0-0t64 (>= 2.32.0)` and `libgtk-4-1 (>= 4.0.0)`; with
  `debian/control`'s floor, `(>= 1.7)`, `(>= 2.84)` and `(>= 4.18)`.
- The app's program needs `libadwaita-1.so.0`, `libgtk-4.so.1`, `libpango-1.0.so.0`,
  `libgio-2.0.so.0`, `libgobject-2.0.so.0` and `libglib-2.0.so.0`, beside `libgcc_s.so.1`,
  `libc.so.6` and `ld-linux-aarch64.so.1` (`readelf -d`). dpkg-shlibdeps gives `pitboard-app`
  `libadwaita-1-0 (>= 1.7), libc6 (>= 2.39), libgcc-s1 (>= 4.2), libglib2.0-0t64 (>= 2.84),
  libgtk-4-1 (>= 4.18), libpango-1.0-0 (>= 1.14.0)`, the floor's three from Build-Depends, and
  warns "diversions involved - output may be incorrect" over libc6's diversion of
  `/lib/ld-linux-aarch64.so.1`.
- lintian, `--pedantic --display-info`: an error `statically-linked-binary` on
  `/usr/bin/pitboard`, which `pitboard.lintian-overrides` overrides; information tags
  `binary-has-unneeded-section .comment` on the command line and `hardening-no-fortify-functions`
  on the app; nothing else. On a test's package, `non-standard-file-perm` is a warning, and a file
  that begins as an ELF file does but is not one is `binary-from-other-architecture`, an error.
- `gpg --dearmor` of the committed key writes the same 2874 bytes each time, with `HOME` at a
  folder that does not exist, and makes no home.
- dh_shell_completions installs `debian/pitboard.shell-completions`' three files as
  `/usr/share/bash-completion/completions/pitboard`, `/usr/share/fish/vendor_completions.d/pitboard.fish`
  and `/usr/share/zsh/vendor-completions/_pitboard`.
- dpkg-buildpackage stops on `build-essential:native` where build-essential is not installed,
  though Build-Depends never names it.

### Installed

Measured the same day by `test-linux-packages.sh`, arm64, with apt fetching from each
distribution's archive while the image built:

- On Debian 13, `debian:13@sha256:8c4733a9…`, and Ubuntu 26.04,
  `ubuntu:26.04@sha256:0e73b71479e7bc5f006bca872e25c3cd680dc052a0116f90d63ae363537db0dd`, apt
  installed the three packages, `pitboard --version` ran as uid 1000 with no network,
  desktop-file-validate and `appstreamcli validate --no-net` passed the installed launcher and
  metainfo, and `apt-get purge` left none of the packages' files.
- Both resolve the floor's libraries by the names dpkg-shlibdeps writes. On 29 October apt
  installed the app with libgtk-4-1 4.22.4+ds-0ubuntu0.1, libadwaita-1-0 1.9.1-0ubuntu0.2 and
  libglib2.0-0t64 2.88.0-1ubuntu0.1 on Ubuntu 26.04, and with Debian 13.7's libgtk-4-1
  4.18.6+ds-2 and libadwaita-1-0 1.7.6-1~deb13u1 beside the libglib2.0-0t64 2.84.4-3~deb13u5 the
  validators had brought.

## The RPM packages

Measured on 29 October 2026 in Docker on the owner's Mac (arm64), as uid 1000 in Fedora 43 from
`fedora:43@sha256:a651ddf48ea28a06ed4e1e6519f51c9f47e7a5a138722ade87369b8fbb7e5b42` with
rpm-build 6.0.2-1.fc43, redhat-rpm-config 343-14.fc43, rpmlint 2.8.0, meson 1.8.5, gcc 15.3.1
and rustup's stable, from `fedora` and `updates` of that day.

- Fedora 43's and Fedora 44's RPM, 6.0.2, evaluates `%_rpmformat` to 4, which Fedora 43's
  `/usr/lib/rpm/macros` sets. Its changelog says "Revert to v4 package format by default for now
  (upstream switched to v6)".
  With `--define '_rpmformat 6'` rpmbuild wrote a package whose `%{RPMFORMAT}` is 6 and whose
  lead `file` reads as "RPM v4.0", where format 4's reads "RPM v3.0".
- redhat-rpm-config's `%_binary_payload` is `w19.zstdio` in format 4 too, so the packages need
  `rpmlib(PayloadIsZstd)`.
- `%__os_install_post` runs `brp-strip` and `brp-strip-comment-note` only where there is no
  debuginfo package, which `%global debug_package %{nil}` gives. `brp-strip` runs `strip -g`
  only on what `file` calls "not stripped", which neither program is. `brp-strip-comment-note`
  runs `strip -R .comment` on every stripped ELF file, and took the musl command line's 168 bytes
  of `.comment` out. With `%global __brp_strip_comment_note %{nil}` the packaged
  `/usr/bin/pitboard` is the tarball's (`cmp`). `add-determinism` and the other steps leave it.
- The musl command line carries a GNU build ID, `.note.gnu.build-id` (`readelf -n`), from Debian's
  gcc. Fedora sets `%_build_id_links compat` and `%_missing_build_ids_terminate_build 1`, but with
  no debuginfo package rpmbuild made no `/usr/lib/.build-id` link, and packaged a copy of the
  binary whose build ID `objcopy` had removed without a word.
- `%meson` begins with `%set_build_flags`, which exports
  `RUSTFLAGS="${RUSTFLAGS:-%{build_rustflags}}"` wherever rust-srpm-macros' `macros.rust`
  defines `build_rustflags`:
  `-Copt-level=3 -Cdebuginfo=2 -Ccodegen-units=1 -Cstrip=none -Cforce-frame-pointers=yes`, with
  `-Clink-arg=-specs=/usr/lib/rpm/redhat/redhat-package-notes` in a build. `%undefine
  build_rustflags` leaves `RUSTFLAGS` as the caller set it. Its `CFLAGS` and `LDFLAGS` are
  Fedora's hardened ones.
- `%meson` sets up `%{_vpath_srcdir}` in `redhat-linux-build` with `--buildtype=plain
  --wrap-mode=nodownload --auto-features=enabled`, and C1's Cargo step built the app in it with
  no network. `%meson_test` ran the 14 Meson tests in `%check`.
- redhat-rpm-config sets `%source_date_epoch_from_changelog 1` and
  `%clamp_mtime_to_source_date_epoch 1`. With `SOURCE_DATE_EPOCH` in the environment, rpmbuild
  takes it over the changelog's date for every file's time, and with
  `use_source_date_epoch_as_buildtime` for the build time. Macros expand in `%changelog`.
- `%{bash_completions_dir}`, `%{zsh_completions_dir}` and `%{fish_completions_dir}` are
  `/usr/share/bash-completion/completions`, `/usr/share/zsh/site-functions` and
  `/usr/share/fish/vendor_completions.d`.
- Fedora's `filesystem` holds the completions' folders, `applications`, `metainfo`,
  `licenses` and `man/man1`; `hicolor-icon-theme` 0.18 holds `hicolor`, `scalable/apps` and
  `symbolic/apps` but not `symbolic/status`; `dbus-common` holds `dbus-1/services`; in the build
  image nothing holds `gnome-shell/search-providers` or `krunner/dbusplugins`. Tumbleweed's
  `filesystem` holds `applications`, `metainfo`, `licenses` and `man/man1`, and util-linux
  `bash-completion/completions`, but none holds zsh's or fish's folder.
- GTK's and GLib's libraries version no symbol, and libadwaita one, `LIBADWAITA_1_0`, for all of
  1.x (`readelf -V`), so rpm's ELF generator requires `libgtk-4.so.1()(64bit)`,
  `libadwaita-1.so.0()(64bit)` and `libglib-2.0.so.0()(64bit)` with no version. Fedora's gtk4,
  libadwaita and glib2 provide those. Only glibc's requirements carry versions, so the sonames
  alone take any GTK 4, libadwaita 1 and GLib 2, below the floor too.
- The names the spec requires at the floor's versions, read on 29 October with
  `dnf repoquery --provides` in fedora:43 and fedora:44 and `zypper info --provides` in
  Tumbleweed 20261020: Fedora's gtk4, libadwaita and glib2 provide their own names at their
  versions, such as `gtk4 = 4.20.4-1.fc43`; Tumbleweed's libgtk-4-1 4.22.5-1.1 provides
  `gtk4 = 4.22.5`, libadwaita-1-0 1.9.4-1.1 `libadwaita = 1.9.4` and libglib-2_0-0 2.88.4-1.1
  `glib2 = 2.88.4`. So `gtk4 >= 4.18`, `libadwaita >= 1.7` and `glib2 >= 2.84` hold the floor on
  every rpm system of it, and need no rich dependency.
- The app requires `libgtk-4.so.1()(64bit)`, `libadwaita-1.so.0()(64bit)` with
  `libadwaita-1.so.0(LIBADWAITA_1_0)(64bit)`, `libglib-2.0.so.0()(64bit)`,
  `libgio-2.0.so.0()(64bit)`, `libgobject-2.0.so.0()(64bit)` and `libpango-1.0.so.0()(64bit)`,
  beside glibc's and libgcc_s's, and `check-linux-packages.sh` passes it.
- rpmlint 2.8.0 with Fedora's configuration, `--strict`: on the command line,
  `statically-linked-binary`, and on aarch64 `position-independent-executable-suggested`, which
  it gives where `file` does not say "pie executable". The released v1.0.0-beta.6's
  aarch64-unknown-linux-musl `pitboard` is "statically linked", its x86_64-unknown-linux-musl one
  "static-pie linked": an rpm of each, made with `rpmbuild --target`, drew the first error on both
  and the second on aarch64 alone. On a file named other than `%{NVRA}.rpm`,
  `non-coherent-filename`. On the app, nothing. A filter that matches nothing is an
  error, `unused-rpmlintrc-filter`, and `-r` takes one file, as its `lint.py` reads it.
- Tumbleweed's own packages, 118 of the 123 in its image, say their build host is
  `reproducible`.

