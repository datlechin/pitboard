#!/bin/sh
# Checks the Linux repository's public key at a time, in seconds since the epoch as `date -u +%s`
# prints it: one primary that does not sign, one signing subkey that gpg lists neither revoked nor
# expired, with 180 days or more left, its primary's included, and no signature under SHA-256.
# Prints the two fingerprints and the subkey's expiry, in seconds since the epoch as gpg lists it,
# as key=value lines.
set -eu

usage() {
  echo "usage: linux-repository-key.sh <public key> <seconds since the epoch>" >&2
  exit 2
}

[ "$#" -eq 2 ] || usage
key=$1
now=$2
case $now in
  "" | *[!0-9]*) usage ;;
esac

refuse() {
  echo "$1" >&2
  exit 1
}

GNUPGHOME=$(mktemp -d)
export GNUPGHOME
trap 'rm -rf "$GNUPGHOME"' EXIT

packets=$(gpg --batch --quiet --list-packets -- "$key") || refuse "gpg read no key from $key"
# Every signature, and every one embedded in another as a subkey's back signature. RFC 9580's hash
# algorithms 8 to 10 are SHA2-256 and up, 12 and 14 SHA3.
weak=$(printf '%s\n' "$packets" | awk '
  BEGIN { name[1] = "MD5"; name[2] = "SHA-1"; name[3] = "RIPEMD-160"; name[11] = "SHA2-224" }
  function check(class, algo) {
    if (algo !~ /^(8|9|10|12|14)$/)
      print "  class " class ": " (algo in name ? name[algo] : "hash algorithm " algo)
  }
  /^:signature packet:/ { signature = 1; next }
  /^:/ { signature = 0 }
  signature && match($0, /sigclass 0x[0-9a-fA-F]+/) { class = substr($0, RSTART + 9, RLENGTH - 9) }
  signature && match($0, /^[ \t]*digest algo [0-9]+/) {
    split(substr($0, RSTART, RLENGTH), field, " ")
    check(class, field[3])
  }
  match($0, /\(signature: v[0-9]+, class 0x[0-9a-fA-F]+, algo [0-9]+, digest algo [0-9]+\)/) {
    split(substr($0, RSTART + 1, RLENGTH - 2), field, /[ ,]+/)
    check(field[4], field[9])
  }
')
# Debian 13's apt has sqv refuse SHA-1 and SHA2-224 from 2026-02-01 (default-sequoia.config).
if [ -n "$weak" ]; then
  echo "$key holds signatures under SHA-256, which apt's sqv refuses:" >&2
  printf '%s\n' "$weak" >&2
  exit 1
fi

listed=$(gpg --batch --quiet --with-colons --with-subkey-fingerprints --show-keys -- "$key") ||
  refuse "gpg read no key from $key"
# A line per key: pub or sub, its fingerprint, its own capabilities, which are the lowercase
# letters of the twelfth field, and its expiry, - where there is none. A subkey whose validity, the
# second field, is r or e is left out: once revoked or expired it signs nothing, and a key keeps
# the subkey it replaced.
keys=$(printf '%s\n' "$listed" | awk -F: '
  $1 == "sub" && ($2 == "r" || $2 == "e") { kind = ""; next }
  $1 == "pub" || $1 == "sub" { kind = $1; caps = $12; expires = $7; next }
  $1 == "fpr" && kind != "" {
    gsub(/[^a-z]/, "", caps)
    print kind, $10, (caps == "" ? "-" : caps), (expires == "" ? "-" : expires)
    kind = ""
  }
')

primaries=0
signing=0
subkeys=
while read -r kind fingerprint caps expires; do
  case $kind in
    pub)
      primaries=$((primaries + 1))
      primary=$fingerprint
      primary_caps=$caps
      primary_expires=$expires
      ;;
    sub)
      case $caps in
        *s*)
          signing=$((signing + 1))
          subkey=$fingerprint
          subkey_expires=$expires
          subkeys="$subkeys $fingerprint"
          ;;
      esac
      ;;
  esac
done <<EOF
$keys
EOF

if [ "$primaries" -ne 1 ]; then
  refuse "$key holds $primaries primary keys, and the repository's key is one"
fi
case $primary_caps in
  *s*)
    refuse "the primary key $primary signs: the repository's primary only certifies, kept" \
      "offline, and its subkey signs"
    ;;
esac
if [ "$signing" -eq 0 ]; then
  refuse "$key has no signing subkey that is neither revoked nor expired: the repository signs" \
    "with one subkey"
fi
if [ "$signing" -gt 1 ]; then
  refuse "$key has $signing signing subkeys,$subkeys: the repository signs with one"
fi
if [ "$subkey_expires" = - ]; then
  refuse "the signing subkey $subkey never expires: the repository's is extended two years at a" \
    "time"
fi

# A subkey signs no longer than its primary lasts. gpg lists a subkey whose primary has expired by
# its clock with the primary's expiry.
until=$subkey_expires
because=
if [ "$primary_expires" != - ] && [ "$primary_expires" -le "$until" ]; then
  until=$primary_expires
  because=", until its primary $primary expires"
fi
if [ "$until" -lt $((now + 180 * 86400)) ]; then
  days=$(((until - now) / 86400))
  case $days in
    -* | 0) left="no day" ;;
    1) left="1 day" ;;
    *) left="$days days" ;;
  esac
  refuse "the signing subkey $subkey has $left left$because: extend the key 180 days ahead, as" \
    "'The Linux repository key' in RELEASING.md says"
fi

echo "primary=$primary"
echo "signing=$subkey"
echo "expires=$subkey_expires"
