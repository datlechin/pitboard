#!/bin/sh
# check-linux-app.sh <destdir>: fails the Linux app's install tree, as Meson installs it under
# /usr for a package, unless it is the release's and claims com.usepitboard.Pitboard and
# pitboard:// alone. Its program holds no fixture, no file names the debug build, and it holds
# the layout docs/facts/linux-app.md names and nothing else, the Plasma widget's files included
# where the tree has the widget. The launcher and KRunner's runner pass desktop-file-validate and
# the metainfo appstreamcli, the D-Bus service starts the program as a service, neither GNOME's
# search nor KRunner starts Pitboard, and the widget is com.usepitboard.Pitboard's.
# test-linux-app.py tests each rule.
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: check-linux-app.sh <destdir>" >&2
  exit 2
fi
tree=$1
if [ ! -d "$tree/usr" ]; then
  echo "there is no install tree under $tree" >&2
  exit 1
fi
here=$(cd "$(dirname "$0")" && pwd)
id=com.usepitboard.Pitboard
program=usr/bin/pitboard-app
launcher=usr/share/applications/$id.desktop
service=usr/share/dbus-1/services/$id.service
metainfo=usr/share/metainfo/$id.metainfo.xml
search=usr/share/gnome-shell/search-providers/$id.search-provider.ini
runner=usr/share/krunner/dbusplugins/$id.desktop
manual=usr/share/man/man1/pitboard-app.1
icons=usr/share/icons/hicolor
widget=usr/share/plasma/plasmoids/$id

failed=0
refuse() {
  echo "$1" >&2
  failed=1
}

# value <file> <group> <key>: the key's value in the group, where the file writes it as
# key=value with no space around the sign, as the tree's own files do.
value() {
  awk -v group="[$2]" -v key="$3=" '
    /^\[/ { inside = ($0 == group); next }
    inside && index($0, key) == 1 { print substr($0, length(key) + 1); exit }
  ' "$tree/$1"
}

layout() {
  for path in "$program" "$launcher" "$service" "$metainfo" "$search" "$runner" "$manual" \
    "$icons/scalable/apps/$id.svg" "$icons/symbolic/apps/$id-symbolic.svg"; do
    echo "$path"
  done
  for level in plenty low out none; do
    for attention in '' -attention; do
      echo "$icons/symbolic/status/$id-$level$attention-symbolic.svg"
    done
  done
  # Meson's plasma_widget, on unless a package leaves the widget to a package of its own.
  if [ -d "$tree/$widget" ]; then
    echo "$widget/metadata.json"
    for view in main Feed FullRepresentation AccountRow; do
      echo "$widget/contents/ui/$view.qml"
    done
  fi
}

listed=$(mktemp)
found=$(mktemp)
trap 'rm -f "$listed" "$found"' EXIT
layout | LC_ALL=C sort >"$listed"
(cd "$tree" && find . ! -type d) | sed 's|^\./||' | LC_ALL=C sort >"$found"
outside=$(LC_ALL=C comm -13 "$listed" "$found")
missing=$(LC_ALL=C comm -23 "$listed" "$found")
if [ -n "$outside" ]; then
  refuse "$(echo "$outside" | sed "s/\$/ is not in the release's layout/")"
fi
if [ -n "$missing" ]; then
  refuse "$(echo "$missing" | sed "s/\$/ of the release's layout is missing/")"
fi

if [ -f "$tree/$program" ] && ! sh "$here/no-fixture-text.sh" "$tree/$program"; then
  failed=1
fi
named=$(cd "$tree" &&
  find . -type f -exec grep -q -a -F -e "$id.Devel" -e pitboard-debug {} \; -print |
  sed 's|^\./||')
if [ -n "$named" ]; then
  refuse "$(echo "$named" | sed 's/$/ names the debug build/')"
fi

for path in "$launcher" "$runner"; do
  if [ -f "$tree/$path" ] && ! desktop-file-validate "$tree/$path" >&2; then
    refuse "desktop-file-validate refuses $path"
  fi
done
if [ -f "$tree/$metainfo" ] && ! appstreamcli validate --no-net "$tree/$metainfo" >&2; then
  refuse "appstreamcli refuses $metainfo"
fi

if [ -f "$tree/$service" ]; then
  if [ "$(value "$service" 'D-BUS Service' Exec)" != "/$program --gapplication-service" ]; then
    refuse "$service does not start /$program --gapplication-service"
  fi
  name=$(value "$service" 'D-BUS Service' Name)
  if [ "$name" != "$id" ]; then
    refuse "$service is for $name, not $id"
  fi
fi
if [ -f "$tree/$launcher" ]; then
  handles=$(value "$launcher" 'Desktop Entry' MimeType)
  if [ "$handles" != 'x-scheme-handler/pitboard;' ]; then
    refuse "$launcher handles $handles, not pitboard:// alone"
  fi
fi
if [ -f "$tree/$search" ] &&
  [ "$(value "$search" 'Shell Search Provider' AutoStart)" != false ]; then
  refuse "$search has GNOME Shell start Pitboard: its AutoStart is not false"
fi
# Plasma loads a widget only from the folder its id names.
if [ -f "$tree/$widget/metadata.json" ]; then
  named=$(python3 -I -c 'import json, sys; print(json.load(sys.stdin)["KPlugin"]["Id"])' \
    <"$tree/$widget/metadata.json") || named=
  if [ "$named" != "$id" ]; then
    refuse "$widget/metadata.json is the widget ${named:-of no id}, not $id"
  fi
fi
# KRunner starts a service named in full, and asks only the running ones a trailing * matches.
if [ -f "$tree/$runner" ] &&
  [ "$(value "$runner" 'Desktop Entry' X-Plasma-DBusRunner-Service)" != "$id*" ]; then
  refuse "$runner has KRunner start Pitboard: its service is not $id*"
fi

exit "$failed"
