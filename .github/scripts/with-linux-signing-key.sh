#!/bin/sh
# with-linux-signing-key.sh <public key> <command> [<argument>...]: runs the command with GNUPGHOME
# at a fresh folder that holds the Linux repository's signing subkey from PACKAGES_SIGNING_KEY,
# its passphrase PACKAGES_SIGNING_PASSPHRASE preset in gpg-agent, so that gpg and rpmsign sign
# with it unasked, then removes the folder. release.yml's linux-sign and linux-repository jobs run
# sign-linux-packages.sh and sign-linux-repository.sh through it, from the linux-packages
# environment.
#
# Before the command runs, it refuses a secret that is not the public key's signing subkey at the
# public key's binding, as linux-repository-key.sh lists the two at the time, one that holds the
# primary's secret, which stays in the owner's offline copy, and a passphrase that does not
# unlock it. Neither variable reaches the command, and the command's exit is this script's.
set -eu

if [ "$#" -lt 2 ]; then
  echo "usage: with-linux-signing-key.sh <public key> <command> [<argument>...]" >&2
  exit 2
fi
key=$1
shift
here=$(cd "$(dirname "$0")" && pwd)

refuse() {
  echo "$@" >&2
  exit 1
}

missing() {
  refuse "$1 is not set in the linux-packages environment, so nothing can be signed with the" \
    "Linux repository's key"
}
[ -n "${PACKAGES_SIGNING_KEY-}" ] || missing PACKAGES_SIGNING_KEY
[ -n "${PACKAGES_SIGNING_PASSPHRASE-}" ] || missing PACKAGES_SIGNING_PASSPHRASE
secret=$PACKAGES_SIGNING_KEY
passphrase=$PACKAGES_SIGNING_PASSPHRASE
unset PACKAGES_SIGNING_KEY PACKAGES_SIGNING_PASSPHRASE

now=$(date -u +%s)
committed=$(sh "$here/linux-repository-key.sh" "$key" "$now")
signing=$(printf '%s\n' "$committed" | sed -n 's/^signing=//p')

# macOS's mktemp takes no TMPDIR without a template.
GNUPGHOME=$(mktemp -d "${TMPDIR:-/tmp}/gnupg.XXXXXX")
export GNUPGHOME
trap 'gpgconf --kill all 2>/dev/null || :; rm -rf "$GNUPGHOME"' EXIT
# gpg-preset-passphrase seeds the agent's cache only where the agent allows it.
echo allow-preset-passphrase >"$GNUPGHOME/gpg-agent.conf"

printf '%s\n' "$secret" | gpg --batch --quiet --import 2>"$GNUPGHOME/import.log" || {
  cat "$GNUPGHOME/import.log" >&2
  refuse "gpg could not import PACKAGES_SIGNING_KEY"
}
gpg --batch --armor --export >"$GNUPGHOME/given.asc"
given=$(sh "$here/linux-repository-key.sh" "$GNUPGHOME/given.asc" "$now") ||
  refuse "PACKAGES_SIGNING_KEY is not a key the repository can sign with, as above"
if [ "$given" != "$committed" ]; then
  refuse "PACKAGES_SIGNING_KEY is not the signing subkey of $key at its binding: it holds
$given
and $key
$committed
Once the key is extended, export it again into PACKAGES_SIGNING_KEY, as 'The Linux repository key'
in RELEASING.md says."
fi

# Each secret key as gpg lists it: sec or ssb, its fingerprint, its keygrip, and in the fifteenth
# field a # where only a stub of its secret is here.
listed=$(gpg --batch --with-colons --with-keygrip --with-subkey-fingerprints --list-secret-keys |
  awk -F: '
    $1 == "sec" || $1 == "ssb" { kind = $1; stub = $15 == "#" ? "stub" : "secret"; next }
    $1 == "fpr" && kind != "" { fingerprint = $10; next }
    $1 == "grp" && kind != "" { print kind, fingerprint, $10, stub; kind = "" }
  ')
grip=
while read -r kind fingerprint keygrip held; do
  case $kind:$held in
    sec:secret)
      refuse "PACKAGES_SIGNING_KEY holds the secret of the primary $fingerprint, which stays in" \
        "the owner's offline copy alone: export the signing subkey alone, as 'The Linux" \
        "repository key' in RELEASING.md says"
      ;;
    ssb:secret) [ "$fingerprint" != "$signing" ] || grip=$keygrip ;;
  esac
done <<EOF
$listed
EOF
[ -n "$grip" ] || refuse "PACKAGES_SIGNING_KEY holds no secret of the signing subkey $signing"

printf '%s' "$passphrase" | "$(gpgconf --list-dirs libexecdir)/gpg-preset-passphrase" --preset \
  "$grip" || refuse "gpg-agent did not take PACKAGES_SIGNING_PASSPHRASE"
unset passphrase secret
echo "with-linux-signing-key.sh" >"$GNUPGHOME/check"
gpg --batch --yes --local-user "$signing!" --output "$GNUPGHOME/check.sig" --detach-sign \
  "$GNUPGHOME/check" 2>"$GNUPGHOME/check.log" || {
  cat "$GNUPGHOME/check.log" >&2
  refuse "PACKAGES_SIGNING_PASSPHRASE does not unlock $signing, the signing subkey in" \
    "PACKAGES_SIGNING_KEY"
}

status=0
"$@" || status=$?
exit "$status"
