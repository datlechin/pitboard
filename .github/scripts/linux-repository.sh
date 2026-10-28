#!/bin/sh
# linux-repository.sh <suites> <packages dir> <public key> <out>: Pitboard's apt and dnf repository
# for one release, as packages.usepitboard.com serves it, from the Debian and RPM packages
# build-linux-packages.sh wrote to <packages dir>, all of one version. <suites> is `stable`,
# `beta` or `stable beta`, and each suite lists those packages and nothing else:
#
#   deb/pool/main/<initial>/<source>/<package>_<version>_<architecture>.deb, from its own fields
#   deb/dists/<suite>/: main/binary-<amd64|arm64>/Packages{,.gz,.xz}, main/Contents-<arch>.gz
#     and main/dep11/, the AppStream catalog Components-<arch>.yml{,.gz,.xz} and its icons
#     icons-<size>.tar{,.gz}, each named in Release by its SHA-256 and copied under by-hash
#   rpm/<suite>/<x86_64|aarch64>/: Packages/<name>-<version>-<release>.<arch>.rpm and repodata/,
#     with the catalog and its icons as the types appstream and appstream-icons, which Fedora's
#     PackageKit reads, and appdata and appdata-icons, which openSUSE's libzypp plugin reads
#   pitboard-archive-keyring.pgp and .asc, the key unarmored and armored, and the setup files:
#     apt's sources for each channel and the preferences file that keeps the repository to its
#     own packages, and dnf's and zypper's repository for each channel
#
# The stable suite never lists a version that holds ~, a beta's or a release candidate's. The
# build stops on an error among appstream-generator's hints, and where the catalog of an
# architecture with the app lacks it. Nothing is signed: sign-linux-repository.sh signs the tree.
# <out> is made once the whole tree is, and must be missing or empty. It runs on Ubuntu 26.04
# with install-deps.sh's repository set, whose appstream-generator is the floor's one build with
# its rpmmd backend.
set -eu
export LC_ALL=C

usage() {
  echo "usage: linux-repository.sh <stable | beta | 'stable beta'> <packages dir> <public key>" \
    "<out>" >&2
  exit 2
}

refuse() {
  echo "$@" >&2
  exit 1
}

[ "$#" -eq 4 ] || usage
suites=$1
dir=$2
key=$3
out=$4
here=$(cd "$(dirname "$0")" && pwd)
config=$(cd "$here/../../packaging/linux" && pwd)

case $suites in
  *[!a-z\ ]* | "") usage ;;
esac
named=
for suite in $suites; do
  case $suite in
    stable | beta) ;;
    *) refuse "$suite is not a suite of the repository, which has stable and beta" ;;
  esac
  case " $named " in
    *" $suite "*) refuse "$suite is named twice" ;;
  esac
  named="$named $suite"
done
[ -d "$dir" ] || refuse "$dir is not a folder"
[ -f "$key" ] || refuse "$key is not a file"
if [ -e "$out" ] && [ -n "$(ls -A "$out")" ]; then
  refuse "$out holds files already, and a repository is written to a folder of its own"
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# rpm reads a package's header only with a database it can lock, and Ubuntu has none.
mkdir "$work/rpmdb"

# Each package goes where its own fields say. The release's version is the first package's, which
# every other must carry, so that a suite lists one release.
version=
app_debs=
app_rpms=
debs=0
rpms=0
for file in "$dir"/*.deb; do
  [ -f "$file" ] || continue
  package=$(dpkg-deb -f "$file" Package)
  stated=$(dpkg-deb -f "$file" Version)
  architecture=$(dpkg-deb -f "$file" Architecture)
  source=$(dpkg-deb -f "$file" Source)
  # Source names the version too where it differs from the package's.
  source=${source%% *}
  : "${source:=$package}"
  case $architecture in
    amd64 | arm64 | all) ;;
    *) refuse "$file is for $architecture, and the repository serves amd64 and arm64" ;;
  esac
  : "${version:=$stated}"
  if [ "$stated" != "$version" ]; then
    refuse "$file is version $stated, and the release's packages $version: a suite lists one" \
      "release"
  fi
  # Debian's pool puts a source under its first letter, and a library under four.
  case $source in
    lib?*) letter=$(printf '%.4s' "$source") ;;
    *) letter=$(printf '%.1s' "$source") ;;
  esac
  pool=$work/deb/pool/main/$letter/$source
  mkdir -p "$pool"
  [ ! -e "$pool/${package}_${stated}_$architecture.deb" ] ||
    refuse "$dir holds two packages $package for $architecture"
  cp "$file" "$pool/${package}_${stated}_$architecture.deb"
  [ "$package" != pitboard-app ] || app_debs="$app_debs $architecture"
  debs=$((debs + 1))
done

# rpm_field <file> <tag>: the package's header tag. Its signature is sign-linux-packages.sh's to
# check.
rpm_field() {
  rpm --dbpath "$work/rpmdb" --nosignature -qp --qf "%{$2}" "$1"
}

for file in "$dir"/*.rpm; do
  [ -f "$file" ] || continue
  name=$(rpm_field "$file" NAME)
  stated=$(rpm_field "$file" VERSION)
  release=$(rpm_field "$file" RELEASE)
  architecture=$(rpm_field "$file" ARCH)
  case $architecture in
    x86_64 | aarch64) machines=$architecture ;;
    noarch) machines='x86_64 aarch64' ;;
    *) refuse "$file is for $architecture, and the repository serves x86_64 and aarch64" ;;
  esac
  : "${version:=$stated}"
  if [ "$stated" != "$version" ]; then
    refuse "$file is version $stated, and the release's packages $version: a suite lists one" \
      "release"
  fi
  for suite in $suites; do
    for machine in $machines; do
      packages=$work/rpm/$suite/$machine/Packages
      mkdir -p "$packages"
      [ ! -e "$packages/$name-$stated-$release.$architecture.rpm" ] ||
        refuse "$dir holds two packages $name for $architecture"
      cp "$file" "$packages/$name-$stated-$release.$architecture.rpm"
    done
  done
  [ "$name" != pitboard-app ] || app_rpms="$app_rpms $machines"
  rpms=$((rpms + 1))
done

[ "$debs" -gt 0 ] || refuse "$dir holds no Debian package, and a release has both formats"
[ "$rpms" -gt 0 ] || refuse "$dir holds no RPM package, and a release has both formats"
case " $named :$version" in
  *" stable "*:*~*)
    refuse "the stable suite never lists a beta or a release candidate, and the release's" \
      "packages are version $version"
    ;;
esac

# no_errors <hints folder>: stops the build on an error among appstream-generator's hints for a
# suite, by the severity its own definitions give each tag, which it writes only where an
# architecture's packages hold metadata.
no_errors() {
  set -- "$1" "$1"/main/Hints-*.json.gz
  [ -f "$2" ] || return 0
  definitions=$1/hint-definitions.json
  shift
  errors=$(gzip -dc "$@" | jq -r --slurpfile tags "$definitions" '
    .[] | .package as $package | .hints | to_entries[] | .key as $component | .value[]
    | select($tags[0][.tag].severity == "error")
    | "\($package) \($component): \(.tag) \(.vars | tostring)"')
  [ -z "$errors" ] || refuse "appstream-generator found errors in the app's metadata:
$errors"
}

# holds_the_app <data folder> <architecture>: stops the build unless the architecture's catalog
# names the app, as the component index appstream-generator writes beside it says.
holds_the_app() {
  index=$1/CID-Index-$2.json.gz
  if [ ! -f "$index" ] ||
    ! gzip -dc "$index" | jq -e 'has("com.usepitboard.Pitboard")' >/dev/null; then
    refuse "the $2 catalog does not hold com.usepitboard.Pitboard, which pitboard-app's" \
      "metainfo describes"
  fi
}

# generate <backend> <suite> <archive>: appstream-generator's catalog of the suite, from the
# archive at the folder its configuration's ArchiveRoot names.
generate() {
  (cd "$3" && appstream-generator run "$2" --workspace "$work/asgen-$1" \
    --config "$config/repository/asgen-$1.json") >&2 ||
    refuse "appstream-generator could not make the $2 catalog"
  no_errors "$work/asgen-$1/export/hints/$2"
}

# The apt tree: each suite's indices, its catalog, then its Release over both.
set --
for suite in $suites; do
  mkdir -p "$work/deb/dists/$suite/main/binary-amd64" "$work/deb/dists/$suite/main/binary-arm64"
  set -- "$@" "dists/$suite/.*"
done
mkdir "$work/cache"
# apt-ftparchive goes on past an error, so the check is that each index is there.
(cd "$work" && apt-ftparchive -q=2 generate "$config/repository/apt-ftparchive.conf" "$@")
for suite in $suites; do
  for architecture in amd64 arm64; do
    for index in binary-$architecture/Packages binary-$architecture/Packages.gz \
      binary-$architecture/Packages.xz Contents-$architecture.gz; do
      [ -f "$work/deb/dists/$suite/main/$index" ] ||
        refuse "apt-ftparchive wrote no $suite/main/$index"
    done
  done
  generate deb "$suite" "$work/deb"
  data=$work/asgen-deb/export/data/$suite/main
  for architecture in $app_debs; do
    holds_the_app "$data" "$architecture"
  done
  # apt fetches a catalog or icons only where Release names them uncompressed, as Debian's does.
  dep11=$work/deb/dists/$suite/main/dep11
  mkdir "$dep11"
  for file in "$data"/Components-*.yml.gz "$data"/icons-*.tar.gz; do
    [ -f "$file" ] || continue
    cp "$file" "$dep11/"
    gzip -dc "$file" >"$dep11/$(basename "$file" .gz)"
  done
  for file in "$data"/Components-*.yml.xz; do
    [ ! -f "$file" ] || cp "$file" "$dep11/"
  done
  (cd "$work/deb" && apt-ftparchive -c "$config/repository/apt-ftparchive.conf" \
    -o "APT::FTPArchive::Release::Suite=$suite" -o "APT::FTPArchive::Release::Codename=$suite" \
    release "dists/$suite") >"$work/Release"
  mv "$work/Release" "$work/deb/dists/$suite/Release"
done

# The dnf and zypper tree: each suite's repository for each machine, its catalog after.
# appstream-generator's rpmmd backend reads <suite>/<section>/<arch>/os, a view of the tree.
for suite in $suites; do
  for machine in x86_64 aarch64; do
    mkdir -p "$work/rpm/$suite/$machine/Packages" "$work/rpm-view/$suite/main/$machine"
    createrepo_c --quiet --no-database "$work/rpm/$suite/$machine"
    ln -s "$work/rpm/$suite/$machine" "$work/rpm-view/$suite/main/$machine/os"
  done
  generate rpm "$suite" "$work/rpm-view"
  data=$work/asgen-rpm/export/data/$suite/main
  for machine in $app_rpms; do
    holds_the_app "$data" "$machine"
  done
  # The icons of every size in one tarball, each size in its folder, which the clients take a
  # tarball whose name names no size to hold.
  icons=$work/icons-$suite
  for machine in x86_64 aarch64; do
    [ -f "$data/Components-$machine.xml.gz" ] || continue
    if [ ! -d "$icons" ]; then
      for size in 48x48 64x64 128x128; do
        mkdir -p "$icons/$size"
        tar -xzf "$data/icons-$size.tar.gz" -C "$icons/$size"
      done
      tar -cf "$icons.tar" -C "$icons" 48x48 64x64 128x128
    fi
    # modifyrepo_c reads a compressed file as its content and compresses what it stores again.
    repodata=$work/rpm/$suite/$machine/repodata
    for type in appstream appdata; do
      modifyrepo_c --compress-type gz --mdtype "$type" --new-name "$type.xml" \
        "$data/Components-$machine.xml.gz" "$repodata"
      modifyrepo_c --compress-type gz --mdtype "$type-icons" --new-name "$type-icons.tar" \
        "$icons.tar" "$repodata"
    done
  done
done

mkdir -p "$out"
cp -R "$work/deb" "$work/rpm" "$out/"
gpg --dearmor <"$key" >"$out/pitboard-archive-keyring.pgp"
cp "$key" "$out/pitboard-archive-keyring.asc"
for file in pitboard.sources pitboard-beta.sources pitboard.pref pitboard.repo pitboard-beta.repo; do
  cp "$config/$file" "$out/"
done
