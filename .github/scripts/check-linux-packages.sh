#!/bin/sh
# check-linux-packages.sh <dir> <command line tarball> <version>: fails the Debian packages
# build-linux-packages.sh wrote to <dir> for the release <version> on this machine's architecture
# unless they are that release's: pitboard, pitboard-app and pitboard-archive-keyring at
# linux-versions.sh's version and nothing else. The command line is the tarball's, byte for byte,
# with its man page and completions; the app depends on that command line's own version and on
# the floor's libraries, and its tree passes check-linux-app.sh; the keyring is the committed key;
# each package holds its layout alone; and lintian warns about none. test-linux-packages.py tests
# each rule.
set -eu

if [ "$#" -ne 3 ]; then
  echo "usage: check-linux-packages.sh <dir> <command line tarball> <version>" >&2
  exit 2
fi
dir=$1
tarball=$2
version=$3
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
versions=$(sh "$here/linux-versions.sh" "$version")
deb=$(printf '%s\n' "$versions" | sed -n 's/^deb=//p')
architecture=$(dpkg --print-architecture)

failed=0
refuse() {
  echo "$1" >&2
  failed=1
}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

command_line=pitboard-v$version-$architecture.deb
app=pitboard-app-v$version-$architecture.deb
keyring=pitboard-archive-keyring-v$version-all.deb
printf '%s\n' "$command_line" "$app" "$keyring" | LC_ALL=C sort >"$work/listed"
(cd "$dir" && find . -maxdepth 1 -name '*.deb') | sed 's|^\./||' | LC_ALL=C sort >"$work/found"
outside=$(LC_ALL=C comm -13 "$work/listed" "$work/found")
missing=$(LC_ALL=C comm -23 "$work/listed" "$work/found")
if [ -n "$outside" ]; then
  refuse "$(echo "$outside" | sed "s/\$/ is not a package of the release/")"
fi
if [ -n "$missing" ]; then
  refuse "$(echo "$missing" | sed "s/\$/ of the release is missing/")"
fi
[ -z "$missing" ] || exit 1

for file in "$command_line" "$app" "$keyring"; do
  package=${file%-v"$version"-*}
  if [ "$(dpkg-deb -f "$dir/$file" Package)" != "$package" ]; then
    refuse "$file is the package $(dpkg-deb -f "$dir/$file" Package), not $package"
  fi
  if [ "$(dpkg-deb -f "$dir/$file" Version)" != "$deb" ]; then
    refuse "$file is version $(dpkg-deb -f "$dir/$file" Version), not $deb"
  fi
  dpkg-deb -x "$dir/$file" "$work/$package"
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

if [ "$(relation "$app" Depends pitboard)" != "= $deb" ]; then
  refuse "$app does not depend on pitboard (= $deb)"
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

shared=$(LC_ALL=C sort "$work/paths" | uniq -d)
if [ -n "$shared" ]; then
  refuse "$(echo "$shared" | sed 's/$/ is in more than one package/')"
fi

# layout <package> <path...>: fails the package's files unless they are the paths given.
layout() {
  package=$1
  shift
  printf '%s\n' "$@" usr/share/doc/"$package"/copyright usr/share/doc/"$package"/changelog.gz |
    LC_ALL=C sort >"$work/listed"
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
zsh=usr/share/zsh/vendor-completions/_pitboard
fish=usr/share/fish/vendor_completions.d/pitboard.fish
man=usr/share/man/man1/pitboard.1.gz
overrides=usr/share/lintian/overrides/pitboard
layout pitboard usr/bin/pitboard "$man" "$bash" "$zsh" "$fish" "$overrides"
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

keyring_file=usr/share/keyrings/pitboard-archive-keyring.pgp
layout pitboard-archive-keyring "$keyring_file"
gpg --dearmor <"$repo/packaging/linux/pitboard-archive-keyring.asc" >"$work/committed.pgp"
if [ -f "$work/pitboard-archive-keyring/$keyring_file" ] &&
  ! cmp -s "$work/committed.pgp" "$work/pitboard-archive-keyring/$keyring_file"; then
  refuse "$keyring's /$keyring_file is not packaging/linux/pitboard-archive-keyring.asc"
fi

# The app's tree as Meson installed it: debhelper adds the package's documents and compresses
# the manual page.
documents=usr/share/doc/pitboard-app
held=$(cd "$work/pitboard-app" && find "$documents" ! -type d 2>/dev/null | LC_ALL=C sort | xargs)
if [ "$held" != "$documents/changelog.gz $documents/copyright" ]; then
  refuse "$app's $documents holds ${held:-nothing}, not its changelog and copyright alone"
fi
rm -rf "${work:?}/pitboard-app/$documents"
if [ -d "$work/pitboard-app/usr/share/man" ]; then
  find "$work/pitboard-app/usr/share/man" -name '*.gz' -exec gzip -d {} +
fi
if ! sh "$here/check-linux-app.sh" "$work/pitboard-app"; then
  refuse "$app's tree is not the release's install tree"
fi

if ! lintian --fail-on error,warning --tag-display-limit 0 \
  "$dir/$command_line" "$dir/$app" "$dir/$keyring" >&2; then
  refuse "lintian warns about the packages"
fi

exit "$failed"
