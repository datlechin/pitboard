#!/bin/sh
# Prints the versions a release's Linux packages carry for its tag's version, X.Y.Z, X.Y.Z-rcN or
# X.Y.Z-beta.N, as key=value lines: the Debian version, and the RPM Version and Release.
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: linux-versions.sh <version>" >&2
  exit 2
fi
version=$1

refuse() {
  echo "$version has no package version: it takes X.Y.Z, X.Y.Z-rcN or, from 1.0.0," \
    "X.Y.Z-beta.N with N from 1 to 98" >&2
  exit 1
}

# grep matches line by line, so a newline in the version would get past it.
case $version in
  *[!0-9A-Za-z.-]* | "") refuse ;;
esac
# release-kind.sh's versions.
number='(0|[1-9][0-9]*)'
printf '%s\n' "$version" |
  grep -qxE "$number\.$number\.$number(-rc[1-9][0-9]*|-beta\.([1-9]|[1-8][0-9]|9[0-8]))?" ||
  refuse
case $version in
  0.*-beta.*) refuse ;;
esac

# A hyphen would start a Debian revision and sort the pre-release above its final release, and RPM
# takes none in a Version. A tilde sorts before anything, in both.
package=$(printf '%s\n' "$version" | tr - '~')
echo "deb=$package"
echo "rpm-version=$package"
echo "rpm-release=1"
