#!/bin/sh
# repository-test.sh <step> <argument...>: the steps repository-test.Containerfile runs as uid 1000
# while test-linux-repository.sh builds it, with the repository's own scripts:
#
#   keys <out>                  the test's keys, made in fresh homes, which nothing outside the
#                               build holds
#   rpms <keys> <packages> <beta> <final> <out>
#                               on Fedora 43: the RPM packages of each release, signed
#   trees <keys> <packages> <releases> <suite> <beta> <final> <out>
#                               on Ubuntu 26.04: the Debian packages of each release beside
#                               them, and the trees the clients are served
#
# The releases are `now`, the packages given; `beta`, at the version <beta>, whose keyring is the
# key; and `final`, at <final> above <beta> and the packages given, whose keyring is the key with
# its signing subkey extended, and which that copy signs. The trees are `now` in <suite>;
# `forged`, `final` in <suite> signed with another key while it serves the key; `beta`; and
# `final` in stable and beta.
set -eu
export LC_ALL=C

here=$(cd "$(dirname "$0")" && pwd)
scripts=$(cd "$here/../../.github/scripts" && pwd)

usage() {
  echo "usage: repository-test.sh <keys <out> | rpms <keys> <packages> <beta> <final> <out> |" \
    "trees <keys> <packages> <releases> <suite> <beta> <final> <out>>" >&2
  exit 2
}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# fingerprint <home> <pub|sub>: the home's primary key's fingerprint, or its subkey's.
fingerprint() {
  GNUPGHOME=$1 gpg --batch --with-colons --with-subkey-fingerprints --list-keys |
    awk -F: -v kind="$2" '$1 == "pub" || $1 == "sub" { at = $1 } $1 == "fpr" && at == kind {
      print $10
      exit
    }'
}

# key <home> <name> [<when>]: in a fresh home, a key laid out as the repository's, made at the
# time given or now: an RSA primary that only certifies, for ten years, and a signing subkey for
# two, neither with a passphrase.
key() {
  mkdir -m 0700 "$1"
  set -- "$1" "$2" ${3:+--faked-system-time "$3!"}
  home=$1
  name=$2
  shift 2
  GNUPGHOME=$home gpg --batch --quiet --passphrase '' "$@" \
    --quick-gen-key "$name <test@example.com>" rsa3072 cert 10y
  GNUPGHOME=$home gpg --batch --quiet --passphrase '' "$@" \
    --quick-add-key "$(fingerprint "$home" pub)" rsa3072 sign 2y
}

# exported <home> <out> <name>: the public key as <name>.asc, and the signing subkey alone, as
# PACKAGES_SIGNING_KEY holds it, as <name>-signing.asc.
exported() {
  GNUPGHOME=$1 gpg --batch --armor --export >"$2/$3.asc"
  GNUPGHOME=$1 gpg --batch --armor --export-secret-subkeys "$(fingerprint "$1" sub)!" \
    >"$2/$3-signing.asc"
}

# signing <keys> <name>: a fresh home holding the key's signing subkey alone, as release.yml
# imports PACKAGES_SIGNING_KEY.
signing() {
  home=$(mktemp -d "$work/home.XXXXXX")
  GNUPGHOME=$home gpg --batch --quiet --import "$1/$2-signing.asc"
  echo "$home"
}

# release_version <release>: the release's package version.
release_version() {
  case $1 in
    now) echo "$given" ;;
    beta) echo "$beta" ;;
    final) echo "$final" ;;
  esac
}

# pattern <version>: the version as a sed pattern, its dots only dots.
pattern() {
  printf '%s\n' "$1" | sed 's/[.]/[.]/g'
}

# release_key <release>: the key the release's keyring holds and that signs it.
release_key() {
  case $1 in
    final) echo extended ;;
    *) echo key ;;
  esac
}

step_keys() {
  [ "$#" -eq 1 ] || usage
  out=$1
  mkdir -p "$out"
  # Made 500 days ago, its subkey has 230 days left, which an extension two years from today
  # replaces with a binding newer than its first, as RELEASING.md extends the repository's.
  key "$work/key" "Pitboard test" "$(($(date +%s) - 500 * 86400))"
  exported "$work/key" "$out" key
  GNUPGHOME=$work/key gpg --batch --quiet --quick-set-expire "$(fingerprint "$work/key" pub)" 2y \
    "$(fingerprint "$work/key" sub)"
  exported "$work/key" "$out" extended
  key "$work/other" "Another key"
  exported "$work/other" "$out" other
  GNUPGHOME=$work/key gpgconf --kill all
  GNUPGHOME=$work/other gpgconf --kill all
}

step_rpms() {
  [ "$#" -eq 5 ] || usage
  keys=$1
  packages=$2
  beta=$3
  final=$4
  out=$5
  set -- "$packages"/pitboard-v*.rpm
  given=$(rpm -qp --qf '%{VERSION}' "$1")
  for release in now beta final; do
    mkdir -p "$out/$release"
    version=$(release_version "$release")
    tag=$(printf '%s\n' "$version" | tr '~' -)
    for package in "$packages"/*.rpm; do
      if [ "$release" = now ]; then
        cp "$package" "$out/$release/"
        continue
      fi
      # The package again at the release's version, what it requires and provides with it.
      rebuilt=$work/rebuilt
      rpmrebuild --package --notest-install --batch --directory "$rebuilt" \
        --change-spec-whole "sed -e 's/$(pattern "$given")/$version/g'" "$package" >&2
      set -- "$rebuilt"/*/*.rpm
      cp "$1" "$out/$release/$(rpm -qp --qf "%{NAME}-v$tag-%{ARCH}" "$1").rpm"
      rm -r "$rebuilt"
    done
    key=$(release_key "$release")
    GNUPGHOME=$(signing "$keys" "$key") sh "$scripts/sign-linux-packages.sh" "$out/$release" \
      "$keys/$key.asc"
  done
}

step_trees() {
  [ "$#" -eq 7 ] || usage
  keys=$1
  packages=$2
  releases=$3
  suite=$4
  beta=$5
  final=$6
  out=$7
  set -- "$packages"/pitboard-v*.deb
  given=$(dpkg-deb -f "$1" Version)
  for release in now beta final; do
    mkdir -p "$releases/$release"
    version=$(release_version "$release")
    tag=$(printf '%s\n' "$version" | tr '~' -)
    for package in "$packages"/*.deb; do
      if [ "$release" = now ]; then
        cp "$package" "$releases/$release/"
        continue
      fi
      # The package again at the release's version, its dependency on the command line with it,
      # and a keyring of the release's key.
      tree=$work/tree
      dpkg-deb --raw-extract "$package" "$tree"
      sed -i -e "s/^Version: .*/Version: $version/" -e "s/(= $(pattern "$given"))/(= $version)/" \
        "$tree/DEBIAN/control"
      keyring=$tree/usr/share/keyrings/pitboard-archive-keyring.pgp
      if [ -f "$keyring" ]; then
        gpg --dearmor <"$keys/$(release_key "$release").asc" >"$keyring"
        (cd "$tree" && find usr -type f | sort | xargs md5sum) >"$tree/DEBIAN/md5sums"
      fi
      name=$(dpkg-deb -f "$package" Package)-v$tag-$(dpkg-deb -f "$package" Architecture)
      dpkg-deb --root-owner-group --build "$tree" "$releases/$release/$name.deb" >&2
      rm -r "$tree"
    done
  done

  # tree <name> <suites> <release> <key>: the tree built from the release and signed with the key.
  tree() {
    sh "$scripts/linux-repository.sh" "$2" "$releases/$3" "$keys/$4.asc" "$out/$1"
    GNUPGHOME=$(signing "$keys" "$4") sh "$scripts/sign-linux-repository.sh" "$out/$1" \
      "$keys/$4.asc"
  }
  tree now "$suite" now key
  tree forged "$suite" final other
  cp "$out/now/pitboard-archive-keyring.pgp" "$out/now/pitboard-archive-keyring.asc" "$out/forged/"
  tree beta beta beta key
  tree final "stable beta" final extended
  # The setup files as the docs give them, with only the address of the test's server.
  sed -i -e 's|https://packages.usepitboard.com|http://127.0.0.1:8080|' \
    -e 's|^Pin: origin packages.usepitboard.com$|Pin: origin 127.0.0.1|' \
    "$out"/*/pitboard*.sources "$out"/*/pitboard.pref "$out"/*/pitboard*.repo
}

[ "$#" -ge 1 ] || usage
step=$1
shift
case $step in
  keys | rpms | trees) "step_$step" "$@" ;;
  *) usage ;;
esac
