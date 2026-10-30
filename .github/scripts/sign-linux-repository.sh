#!/bin/sh
# sign-linux-repository.sh <tree> <public key>: signs the repository linux-repository.sh wrote to
# <tree> with the key's signing subkey, from the GNUPGHOME the caller made, which holds its
# secret and can use it unasked: each suite's Release as InRelease and Release.gpg, and each RPM
# repository's repomd.xml as repomd.xml.asc. Each signature is then checked against the key
# alone, unarmored: with sqv under the policy apt gives it, and with gpgv. Unless every one holds,
# none is left in the tree. It runs on Ubuntu 26.04 with install-deps.sh's repository set.
set -eu
export LC_ALL=C

usage() {
  echo "usage: sign-linux-repository.sh <tree> <public key>" >&2
  exit 2
}

refuse() {
  echo "$@" >&2
  exit 1
}

[ "$#" -eq 2 ] || usage
tree=$1
key=$2
here=$(cd "$(dirname "$0")" && pwd)
[ -d "$tree" ] || refuse "$tree is not a folder"

# linux-repository-key.sh refuses a key the repository cannot sign with today.
fields=$(sh "$here/linux-repository-key.sh" "$key" "$(date -u +%s)")
signing=$(printf '%s\n' "$fields" | sed -n 's/^signing=//p')

set --
for file in "$tree"/deb/dists/*/Release "$tree"/rpm/*/*/repodata/repomd.xml; do
  [ -f "$file" ] || continue
  for signature in "${file%/*}/InRelease" "${file%/*}/Release.gpg" "$file.asc"; do
    [ ! -e "$signature" ] || refuse "$signature is there already: a tree is signed once"
  done
  set -- "$@" "$file"
done
[ "$#" -gt 0 ] || refuse "$tree holds no Release and no repomd.xml to sign"

work=$(mktemp -d)
signed=
trap '[ -n "$signed" ] || while IFS= read -r file; do rm -f "$file"; done <"$work/written"
  rm -rf "$work"' EXIT
: >"$work/written"
gpg --dearmor <"$key" >"$work/key.pgp"
if [ -f /usr/share/apt/default-sequoia.config ]; then
  SEQUOIA_CRYPTO_POLICY=/usr/share/apt/default-sequoia.config
  export SEQUOIA_CRYPTO_POLICY
fi

# sign <signature> <option...>: the signature, made with the subkey alone, its name kept so that
# a failure takes it away.
sign() {
  signature=$1
  shift
  printf '%s\n' "$signature" >>"$work/written"
  gpg --batch --yes --local-user "$signing!" --digest-algo SHA512 --output "$signature" "$@" ||
    refuse "gpg could not sign with $signing, the signing subkey of $key, from GNUPGHOME"
}

# verified <signature> [<data>]: sqv's and gpgv's verdicts on a detached signature of the data,
# or on a cleartext one, whose text must be Release's. The line ending before the signature is
# not the text's (RFC 9580, 7.1).
verified() {
  if [ "$#" -eq 2 ]; then
    sqv --keyring "$work/key.pgp" --signature-file "$1" "$2" >/dev/null || return 1
  else
    rm -f "$work/text"
    sqv --keyring "$work/key.pgp" --cleartext --output "$work/text" "$1" >/dev/null || return 1
    echo >>"$work/text"
    cmp -s "$work/text" "${1%/*}/Release" || return 1
  fi
  gpgv --keyring "$work/key.pgp" "$@" 2>"$work/gpgv.log" && return 0
  cat "$work/gpgv.log" >&2
  return 1
}

for file in "$@"; do
  case $file in
    */Release)
      sign "${file%/*}/InRelease" --clearsign "$file"
      sign "${file%/*}/Release.gpg" --armor --detach-sign "$file"
      if ! verified "${file%/*}/InRelease" || ! verified "${file%/*}/Release.gpg" "$file"; then
        refuse "the signatures of $file do not verify against $key, so the tree is left unsigned"
      fi
      ;;
    *)
      sign "$file.asc" --armor --detach-sign "$file"
      verified "$file.asc" "$file" ||
        refuse "the signature of $file does not verify against $key, so the tree is left unsigned"
      ;;
  esac
done
signed=1
