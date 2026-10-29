#!/bin/sh
# check-linux-packages.sh <deb|rpm> <dir> <command line tarball> <version>: fails the Debian or
# RPM packages build-linux-packages.sh wrote to <dir> for the release <version> on this machine's
# architecture unless they are that release's: pitboard, pitboard-app and, for Debian,
# pitboard-archive-keyring, at linux-versions.sh's version and nothing else. The command line is
# the tarball's, byte for byte, with its man page and completions; the app depends on that command
# line's own version and on the floor's libraries, and its tree passes check-linux-app.sh; the
# keyring is the committed key; each package holds its layout alone; an RPM package is in RPM's
# format 4; and lintian or rpmlint warns about none. test-linux-packages.py tests each rule.
set -eu

usage() {
  echo "usage: check-linux-packages.sh <deb|rpm> <dir> <command line tarball> <version>" >&2
  exit 2
}

[ "$#" -eq 4 ] || usage
format=$1
dir=$2
tarball=$3
version=$4
case $format in
  deb | rpm) ;;
  *) usage ;;
esac
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
versions=$(sh "$here/linux-versions.sh" "$version")
if [ "$format" = deb ]; then
  expected=$(printf '%s\n' "$versions" | sed -n 's/^deb=//p')
  architecture=$(dpkg --print-architecture)
  packages='pitboard pitboard-app pitboard-archive-keyring'
  zsh=usr/share/zsh/vendor-completions/_pitboard
else
  expected=$(printf '%s\n' "$versions" | sed -n 's/^rpm-version=//p')-$(
    printf '%s\n' "$versions" | sed -n 's/^rpm-release=//p'
  )
  architecture=$(rpm --eval '%{_arch}')
  packages='pitboard pitboard-app'
  zsh=usr/share/zsh/site-functions/_pitboard
fi

failed=0
refuse() {
  echo "$1" >&2
  failed=1
}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# release_file <package>: the release's file name for the package.
release_file() {
  case $1 in
    pitboard-archive-keyring) echo "$1-v$version-all.$format" ;;
    *) echo "$1-v$version-$architecture.$format" ;;
  esac
}

for package in $packages; do
  release_file "$package"
done | LC_ALL=C sort >"$work/listed"
(cd "$dir" && find . -maxdepth 1 -name "*.$format") | sed 's|^\./||' | LC_ALL=C sort \
  >"$work/found"
outside=$(LC_ALL=C comm -13 "$work/listed" "$work/found")
missing=$(LC_ALL=C comm -23 "$work/listed" "$work/found")
if [ -n "$outside" ]; then
  refuse "$(echo "$outside" | sed "s/\$/ is not a package of the release/")"
fi
if [ -n "$missing" ]; then
  refuse "$(echo "$missing" | sed "s/\$/ of the release is missing/")"
fi
[ -z "$missing" ] || exit 1
command_line=$(release_file pitboard)
app=$(release_file pitboard-app)

# field <package file> <field>: the package's control field, or its RPM header tag.
field() {
  if [ "$format" = deb ]; then
    dpkg-deb -f "$dir/$1" "$2"
  else
    rpm -qp --qf "%{$2}" "$dir/$1"
  fi
}

for package in $packages; do
  name=$(release_file "$package")
  if [ "$format" = deb ]; then
    said=$(field "$name" Package)
    stated=$(field "$name" Version)
  else
    said=$(field "$name" NAME)
    stated=$(field "$name" VERSION)-$(field "$name" RELEASE)
    rpm_format=$(field "$name" RPMFORMAT)
    if [ "$rpm_format" != 4 ]; then
      refuse "$name is in RPM's format $rpm_format, not 4, which every RPM of the floor reads"
    fi
  fi
  if [ "$said" != "$package" ]; then
    refuse "$name is the package $said, not $package"
  fi
  if [ "$stated" != "$expected" ]; then
    refuse "$name is version $stated, not $expected"
  fi
  mkdir "$work/$package"
  if [ "$format" = deb ]; then
    dpkg-deb -x "$dir/$name" "$work/$package"
  else
    rpm2archive - <"$dir/$name" | tar -xz -C "$work/$package"
  fi
  (cd "$work/$package" && find . ! -type d) | sed 's|^\./||' >>"$work/paths"
done

# relation <package file> <field> <name>: the version relation the field states for the package
# name, such as `>= 4.18.6`, where it names that package alone, not among alternatives.
relation() {
  dpkg-deb -f "$dir/$1" "$2" | tr '\n,' ' \n' | awk -v name="$3" '
    { sub(/^ +/, ""); sub(/ +$/, "") }
    $0 == name { print "any" }
    index($0, name " (") == 1 && /\)$/ {
      print substr($0, length(name) + 3, length($0) - length(name) - 3)
    }'
}

if [ "$format" = deb ]; then
  if [ "$(relation "$app" Depends pitboard)" != "= $expected" ]; then
    refuse "$app does not depend on pitboard (= $expected)"
  fi
  for floor in libadwaita-1-0:1.7 libgtk-4-1:4.18 libglib2.0-0t64:2.84; do
    library=${floor%:*}
    least=${floor#*:}
    stated=$(relation "$app" Depends "$library")
    case $stated in
      ">= "*) dpkg --compare-versions "${stated#>= }" ge "$least" || stated= ;;
      *) stated= ;;
    esac
    if [ -z "$stated" ]; then
      refuse "$app does not depend on $library (>= $least), the floor's"
    fi
  done
  if [ "$(relation "$command_line" Recommends pitboard-archive-keyring)" != any ]; then
    refuse "$command_line does not recommend pitboard-archive-keyring"
  fi
else
  rpm -qp --requires "$dir/$app" >"$work/requires"
  if ! grep -q -x -F "pitboard = $expected" "$work/requires"; then
    refuse "$app does not require pitboard = $expected"
  fi
  for library in libadwaita-1.so.0 libgtk-4.so.1 libglib-2.0.so.0; do
    if ! grep -q -x -F "$library()(64bit)" "$work/requires"; then
      refuse "$app does not require $library()(64bit), the floor's"
    fi
  done
  # No version of GTK's, libadwaita's or GLib's symbols names a floor, so rpm's own requirements
  # hold none. The spec requires the names Fedora's packages and openSUSE's provide at their
  # version, at the floor's or above.
  for floor in libadwaita:1.7 gtk4:4.18 glib2:2.84; do
    library=${floor%:*}
    least=${floor#*:}
    stated=$(sed -n "s/^$library >= //p" "$work/requires")
    case $stated in
      "" | *[!0-9.]*) stated= ;;
      *)
        above=$(rpm --eval "%{lua:print(rpm.vercmp('$stated', '$least'))}")
        [ "$above" -ge 0 ] || stated=
        ;;
    esac
    if [ -z "$stated" ]; then
      refuse "$app does not require $library >= $least, the floor's"
    fi
  done
fi

shared=$(LC_ALL=C sort "$work/paths" | uniq -d)
if [ -n "$shared" ]; then
  refuse "$(echo "$shared" | sed 's/$/ is in more than one package/')"
fi

# documents <package>: the files the packaging tool adds to the package beside its own.
documents() {
  if [ "$format" = deb ]; then
    echo "usr/share/doc/$1/changelog.gz" "usr/share/doc/$1/copyright"
  else
    echo "usr/share/licenses/$1/LICENSE" "usr/share/licenses/$1/NOTICE"
  fi
}

# layout <package> <path...>: fails the package's files unless they are the paths given and its
# documents.
layout() {
  package=$1
  shift
  # shellcheck disable=SC2046 # the documents are words
  printf '%s\n' "$@" $(documents "$package") | LC_ALL=C sort >"$work/listed"
  (cd "$work/$package" && find . ! -type d) | sed 's|^\./||' | LC_ALL=C sort >"$work/found"
  outside=$(LC_ALL=C comm -13 "$work/listed" "$work/found")
  missing=$(LC_ALL=C comm -23 "$work/listed" "$work/found")
  if [ -n "$outside" ]; then
    refuse "$(echo "$outside" | sed "s|\$| is not in $package's layout|")"
  fi
  if [ -n "$missing" ]; then
    refuse "$(echo "$missing" | sed "s|\$| of $package's layout is missing|")"
  fi
}

name=$(basename "$tarball" .tar.gz)
mkdir "$work/tarball"
tar -xzf "$tarball" -C "$work/tarball" --strip-components=1 "$name"
bash=usr/share/bash-completion/completions/pitboard
fish=usr/share/fish/vendor_completions.d/pitboard.fish
man=usr/share/man/man1/pitboard.1.gz
if [ "$format" = deb ]; then
  layout pitboard usr/bin/pitboard "$man" "$bash" "$zsh" "$fish" \
    usr/share/lintian/overrides/pitboard
else
  layout pitboard usr/bin/pitboard "$man" "$bash" "$zsh" "$fish"
fi
same() {
  if [ -f "$work/pitboard/$2" ] && ! cmp -s "$work/tarball/$1" "$3"; then
    refuse "$command_line's /$2 is not the command line tarball's $1"
  fi
}
same pitboard usr/bin/pitboard "$work/pitboard/usr/bin/pitboard"
same completions/pitboard.bash "$bash" "$work/pitboard/$bash"
same completions/pitboard.zsh "$zsh" "$work/pitboard/$zsh"
same completions/pitboard.fish "$fish" "$work/pitboard/$fish"
if [ -f "$work/pitboard/$man" ]; then
  gzip -dc "$work/pitboard/$man" >"$work/pitboard.1"
  same pitboard.1 "$man" "$work/pitboard.1"
fi

if [ "$format" = deb ]; then
  keyring=$(release_file pitboard-archive-keyring)
  keyring_file=usr/share/keyrings/pitboard-archive-keyring.pgp
  layout pitboard-archive-keyring "$keyring_file"
  gpg --dearmor <"$repo/packaging/linux/pitboard-archive-keyring.asc" >"$work/committed.pgp"
  if [ -f "$work/pitboard-archive-keyring/$keyring_file" ] &&
    ! cmp -s "$work/committed.pgp" "$work/pitboard-archive-keyring/$keyring_file"; then
    refuse "$keyring's /$keyring_file is not packaging/linux/pitboard-archive-keyring.asc"
  fi
fi

# The app's tree as Meson installed it: the packaging tool adds the package's documents and
# compresses the manual page.
held=$(cd "$work/pitboard-app" && find usr/share/doc usr/share/licenses ! -type d 2>/dev/null |
  LC_ALL=C sort | xargs)
if [ "$held" != "$(documents pitboard-app)" ]; then
  refuse "$app's documents are ${held:-none}, not $(documents pitboard-app)"
fi
rm -rf "$work/pitboard-app/usr/share/doc" "$work/pitboard-app/usr/share/licenses"
if [ -d "$work/pitboard-app/usr/share/man" ]; then
  find "$work/pitboard-app/usr/share/man" -name '*.gz' -exec gzip -d {} +
fi
if ! sh "$here/check-linux-app.sh" "$work/pitboard-app"; then
  refuse "$app's tree is not the release's install tree"
fi

if [ "$format" = deb ]; then
  if ! lintian --fail-on error,warning --tag-display-limit 0 \
    "$dir/$command_line" "$dir/$app" "$dir/$keyring" >&2; then
    refuse "lintian warns about the packages"
  fi
else
  # rpmlint refuses a file not named as rpmbuild names it, which GitHub would rename.
  mkdir "$work/rpmlint"
  for package in "$command_line" "$app"; do
    cp "$dir/$package" "$work/rpmlint/$(rpm -qp --qf '%{NVRA}.rpm' "$dir/$package")"
  done
  if ! rpmlint --strict -r "$repo/packaging/linux/pitboard.rpmlintrc" "$work"/rpmlint/*.rpm >&2
  then
    refuse "rpmlint warns about the packages"
  fi
fi

exit "$failed"
