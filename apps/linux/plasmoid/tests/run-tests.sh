#!/bin/sh
# run-tests.sh: the Plasma widget's tests, tst_widget.qml under qmltestrunner on Qt's offscreen
# platform, run by check.sh under headless.sh, never on a desktop's own bus. The private bus
# starts the stand-in for Pitboard by the service file written here, and only once something
# asks for Pitboard's name. The tests run in Breeze Light, then in Breeze Dark, as Plasma applies
# a colour scheme, and keep their pictures in $PITBOARD_LINUX_PICTURES where it is set.
set -eu

if [ -z "${DBUS_SESSION_BUS_ADDRESS-}" ] || [ -z "${XDG_DATA_HOME-}" ] ||
  [ -z "${XDG_CONFIG_HOME-}" ]; then
  echo "run-tests.sh runs on the private bus and in the homes headless.sh gives" >&2
  exit 1
fi
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../../.." && pwd)
bins=$(qtpaths6 --query QT_INSTALL_BINS)

services=$XDG_DATA_HOME/dbus-1/services
mkdir -p "$services"
cat >"$services/com.usepitboard.Pitboard.service" <<EOF
[D-BUS Service]
Name=com.usepitboard.Pitboard
Exec=/usr/bin/python3 -I $here/stand-in.py $repo/apps/linux/data/dbus/com.usepitboard.Pitboard.WidgetFeed.xml $repo/crates/pitboard-model/src/present/glance.json
EOF

# The app's icons, as its package installs them, which the widget names.
mkdir -p "$XDG_DATA_HOME/icons/hicolor"
cp -R "$repo/apps/linux/data/icons/scalable" "$repo/apps/linux/data/icons/symbolic" \
  "$XDG_DATA_HOME/icons/hicolor/"

pictures=${PITBOARD_LINUX_PICTURES:-$(mktemp -d)}
mkdir -p "$pictures"
# KDE's platform theme, which a Plasma session loads, reads the colour scheme from kdeglobals,
# where plasma-apply-colorscheme writes the scheme's groups.
for scheme in BreezeLight BreezeDark; do
  cp "/usr/share/color-schemes/$scheme.colors" "$XDG_CONFIG_HOME/kdeglobals"
  (cd "$pictures" && QT_QPA_PLATFORMTHEME=kde \
    "$bins/qmltestrunner" -platform offscreen -input "$here/tst_widget.qml")
done
