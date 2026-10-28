#!/bin/sh
# Whether a release ships the Linux packages, and which suites of the repository it writes, for
# release.yml's guard: key=value lines for $GITHUB_OUTPUT. <version>, <kind> and <latest> are what
# release-kind.sh printed. <session record> is docs/facts/linux-session.md, and <published> has a
# line for each published release: its tag, whether GitHub marks it a pre-release, and whether it
# has a .deb, each true or false.
#
# A release ships them from 1.0.0 once a Linux session has measured the bundle of a version at or
# below it, which the record's first paragraph names. Without the record nothing ships, and once a
# release has shipped them its absence is refused: the repository would stop following.
#
# A full release marked latest writes stable, and beta too unless a beta with packages is above
# it. A beta above every full release and beta with packages writes beta. Anything else, a
# release candidate among them, writes no suite.
set -eu

if [ "$#" -ne 5 ]; then
  echo "usage: linux-release.sh <version> <kind> <latest> <session record> <published releases" \
    "file>" >&2
  exit 2
fi
version=$1
kind=$2
latest=$3
record=$4
published=$5

refuse() {
  echo "$@" >&2
  exit 1
}

# SemVer's numbers: no leading zero, as release-kind.sh takes them.
number='(0|[1-9][0-9]*)'
release="$number\.$number\.$number"
beta='-beta\.([1-9]|[1-8][0-9]|9[0-8])'

matches() {
  printf '%s\n' "$1" | grep -qxE "$2"
}

# kind_of <version>: full, rc or beta, or nothing for a version no tag carries. grep matches line
# by line, so a newline would get past it.
kind_of() {
  case $1 in
    *[!0-9A-Za-z.-]* | "") return 0 ;;
  esac
  if matches "$1" "$release"; then
    echo full
  elif matches "$1" "$release-rc[1-9][0-9]*"; then
    echo rc
  elif matches "$1" "$release$beta"; then
    echo beta
  fi
}

# above <a> <b>: whether a comes after b, each X.Y.Z or X.Y.Z-beta.N, a full release after each
# of its betas.
above() {
  awk -v a="$1" -v b="$2" '
    function key(version, k,   n, part) {
      n = split(version, part, /[.-]/)
      k[1] = part[1]; k[2] = part[2]; k[3] = part[3]
      k[4] = n > 3 ? part[5] : 1000
    }
    BEGIN {
      key(a, x)
      key(b, y)
      for (i = 1; i <= 4; i++) {
        if (x[i] + 0 > y[i] + 0) exit 0
        if (x[i] + 0 < y[i] + 0) exit 1
      }
      exit 1
    }'
}

case $latest in
  true | false) ;;
  *) latest= ;;
esac
if [ "$(kind_of "$version")" != "$kind" ] || [ -z "$kind" ] || [ -z "$latest" ]; then
  refuse "$version $kind $3 is not a release as release-kind.sh prints it"
fi
base=${version%-rc*}

if [ "${base%%.*}" = 0 ]; then
  echo "linux=false"
  echo "suites="
  exit 0
fi

# The published releases but this one, run again: the first that shipped packages, and every full
# release and beta that did, since only those can be in a suite.
unreadable() {
  refuse "linux-release.sh cannot read the published release '$line': it takes a tag, then true" \
    "or false for a pre-release, which only a beta or a release candidate is, then true or false" \
    "for a .deb"
}
shipped=
with_packages=
while IFS= read -r line || [ -n "$line" ]; do
  [ -n "$line" ] || continue
  set -f
  # shellcheck disable=SC2086 # the line's fields
  set -- $line
  set +f
  [ "$#" -eq 3 ] || unreadable
  tag=$1
  other=${tag#v}
  other_kind=$(kind_of "$other")
  case $tag:$other_kind:$2 in
    v*:full:false | v*:rc:true | v*:beta:true) ;;
    *) unreadable ;;
  esac
  case $3 in
    true) ;;
    false) continue ;;
    *) unreadable ;;
  esac
  [ "$tag" != "v$version" ] || continue
  : "${shipped:=$tag}"
  [ "$other_kind" = rc ] || with_packages="$with_packages $other"
done <"$published"

if [ ! -e "$record" ]; then
  if [ -n "$shipped" ]; then
    refuse "$record is gone, and $shipped shipped the Linux packages: a release without the Linux" \
      "session's record would leave the repository behind it"
  fi
  echo "linux=false"
  echo "suites="
  exit 0
fi

# The record's first paragraph after its heading, on one line.
paragraph=$(awk '
  /^#/ && !started { next }
  /^[[:space:]]*$/ { if (started) exit; next }
  { started = 1; printf "%s ", $0 }
' "$record" | tr -s ' \t' '  ')
# shellcheck disable=SC2016 # the record's own backticks
named='The bundle was `linux-facts\.yml` session run [0-9][0-9]* at [0-9a-f]\{7,40\}, the commit'
# shellcheck disable=SC2016
bundle=$(printf '%s\n' "$paragraph" | sed -n "s/.*$named"' that holds `Release \([^`]*\)`.*/\1/p')
if [ -z "$bundle" ]; then
  refuse "$record names no bundle as 'The bundle was \`linux-facts.yml\` session run <run id> at" \
    "<commit>, the commit that holds \`Release <version>\`.' in its first paragraph, as" \
    "RELEASING.md says"
fi
case $(kind_of "$bundle") in
  full | beta) ;;
  *)
    refuse "$record names the bundle of $bundle, which no Release commit is: Cargo.toml holds" \
      "X.Y.Z or X.Y.Z-beta.N"
    ;;
esac
if [ "${bundle%%.*}" = 0 ]; then
  refuse "$record names the bundle of $bundle, before 1.0.0-beta.1, the first version a Linux" \
    "session measures"
fi
if above "$bundle" "$base"; then
  refuse "$record names the bundle of $bundle, above this release's $base: a session measures" \
    "the release it ships or an earlier one"
fi

# above_all <version>: whether the version is above every full release and beta with packages.
above_all() {
  for other in $with_packages; do
    above "$1" "$other" || return 1
  done
}

beta_above=false
for other in $with_packages; do
  case $other in
    *-beta.*) if above "$other" "$version"; then beta_above=true; fi ;;
  esac
done
suites=
case $kind:$latest in
  full:true)
    suites=stable
    [ "$beta_above" = true ] || suites="stable beta"
    ;;
  beta:*)
    if above_all "$version"; then suites=beta; fi
    ;;
esac
echo "linux=true"
echo "suites=$suites"
