#!/bin/sh
# upload-linux-repository.sh <tree>: puts the repository linux-repository.sh built and
# sign-linux-repository.sh signed in <tree> into the R2 bucket pitboard-packages, which
# packages.usepitboard.com serves, with the AWS CLI as Cloudflare documents it for R2: region
# auto and the account's endpoint. release.yml's linux-repository job runs it in amazon/aws-cli,
# with PACKAGES_R2_ACCESS_KEY_ID, PACKAGES_R2_SECRET_ACCESS_KEY and PACKAGES_R2_ENDPOINT from the
# linux-packages environment.
#
# A bucket has no atomic swap, so each file goes up on its own, in an order that keeps a reader
# from an index whose files are not there yet: the packages, then each index named by its hash,
# then the other indices, then each suite's Release, Release.gpg and InRelease and each
# repomd.xml.asc and repomd.xml, and last the key and the setup files. A package or an index named
# by its hash never changes, so its Cache-Control keeps it a year; everything else is no-cache.
# Nothing is deleted: a client that holds an earlier index still finds its files. A tree with an
# index left unsigned is refused before anything goes up.
set -eu
export LC_ALL=C

if [ "$#" -ne 1 ]; then
  echo "usage: upload-linux-repository.sh <tree>" >&2
  exit 2
fi
tree=$1
bucket=pitboard-packages
kept='public, max-age=31536000, immutable'

refuse() {
  echo "$@" >&2
  exit 1
}

missing() {
  refuse "$1 is not set in the linux-packages environment, so the repository cannot be written"
}
[ -n "${PACKAGES_R2_ACCESS_KEY_ID-}" ] || missing PACKAGES_R2_ACCESS_KEY_ID
[ -n "${PACKAGES_R2_SECRET_ACCESS_KEY-}" ] || missing PACKAGES_R2_SECRET_ACCESS_KEY
[ -n "${PACKAGES_R2_ENDPOINT-}" ] || missing PACKAGES_R2_ENDPOINT
# grep matches line by line, so a newline would get past it.
case $PACKAGES_R2_ENDPOINT in
  *[!0-9a-z:/.]*) endpoint= ;;
  *) endpoint=$PACKAGES_R2_ENDPOINT ;;
esac
printf '%s\n' "$endpoint" | grep -qxE 'https://[0-9a-f]{32}\.r2\.cloudflarestorage\.com' ||
  refuse "PACKAGES_R2_ENDPOINT is not an R2 account's endpoint, as" \
    "https://<account id>.r2.cloudflarestorage.com"
[ -d "$tree" ] || refuse "$tree is not a folder"

AWS_ACCESS_KEY_ID=$PACKAGES_R2_ACCESS_KEY_ID
AWS_SECRET_ACCESS_KEY=$PACKAGES_R2_SECRET_ACCESS_KEY
export AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY
unset PACKAGES_R2_ACCESS_KEY_ID PACKAGES_R2_SECRET_ACCESS_KEY PACKAGES_R2_ENDPOINT

work=$(mktemp -d "${TMPDIR:-/tmp}/upload.XXXXXX")
trap 'rm -rf "$work"' EXIT
(cd "$tree" && find . -type f | sed 's|^\./||' | sort) >"$work/files"
grep -q . "$work/files" || refuse "$tree holds nothing to upload"
if grep -q '[[:space:]]' "$work/files"; then
  refuse "$tree holds a file with a space in its name, which no index of the repository names"
fi

# Each file as its step, the folder it is kept with and its place there, then its name. The steps:
# 1 the packages, 2 an index named by its hash, 3 another index, 4 a suite's Release, Release.gpg
# and InRelease, or a repomd.xml.asc and its repomd.xml, a folder's together, and 5 the key and
# the setup files. createrepo_c names each of its files after its SHA-256.
while read -r name; do
  case $name in
    deb/pool/* | rpm/*/*/Packages/*) step=1 ;;
    */by-hash/*) step=2 ;;
    deb/dists/*/Release | rpm/*/*/repodata/repomd.xml.asc) step=4 place=1 ;;
    deb/dists/*/Release.gpg | rpm/*/*/repodata/repomd.xml) step=4 place=2 ;;
    deb/dists/*/InRelease) step=4 place=3 ;;
    rpm/*/*/repodata/*)
      if printf '%s\n' "${name##*/}" | grep -qE '^[0-9a-f]{64}-'; then step=2; else step=3; fi
      ;;
    deb/* | rpm/*) step=3 ;;
    *) step=5 ;;
  esac
  if [ "$step" = 4 ]; then
    echo "$step ${name%/*} $place $name"
  else
    echo "$step - 0 $name"
  fi
done <"$work/files" | sort -k 1,1n -k 2,2 -k 3,3n -k 4,4 >"$work/order"

while read -r _ _ _ name; do
  case $name in
    deb/dists/*/Release) set -- "${name%/Release}/InRelease" "$name.gpg" ;;
    rpm/*/*/repodata/repomd.xml) set -- "$name.asc" ;;
    *) continue ;;
  esac
  for signature in "$@"; do
    [ -f "$tree/$signature" ] || refuse "$tree/$signature is missing: sign-linux-repository.sh" \
      "signs the tree before it goes up"
  done
done <"$work/order"

while read -r step _ _ name; do
  case $step in
    1 | 2) control=$kept ;;
    *) control=no-cache ;;
  esac
  aws s3 cp "$tree/$name" "s3://$bucket/$name" --endpoint-url "$endpoint" --region auto \
    --cache-control "$control" --no-progress
done <"$work/order"
