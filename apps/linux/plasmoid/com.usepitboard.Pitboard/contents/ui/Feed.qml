// What the running Pitboard feeds a widget, and the switch a widget asks it for. Both go only to
// the connection that owns Pitboard's name while it runs, by that connection's unique name, which
// the bus never starts: nothing here starts Pitboard or asks one that is not running. The feed
// follows Pitboard's PropertiesChanged, with no poll (docs/facts/linux-app.md, The Plasma widget).

pragma ComponentBehavior: Bound

import QtQuick
import org.kde.plasma.workspace.dbus as DBus

QtObject {
    id: feed

    // The release's name and path. A debug build answers to its own, which no widget reads.
    readonly property string service: "com.usepitboard.Pitboard"
    readonly property string path: "/com/usepitboard/Pitboard"

    // The connection that owns the name, while it does.
    property string owner: ""
    property string text: ""

    // The glance as Pitboard's widget feed says it, or null: while Pitboard is not running, before
    // it has read the accounts, once it is removed, and for a schema of another Pitboard's.
    readonly property var glance: {
        if (!owner || !text) {
            return null;
        }
        try {
            const glance = JSON.parse(text);
            return glance.schema === 1 ? glance : null;
        } catch (refused) {
            return null;
        }
    }

    // As Pitboard's own actions are asked for, which a search and a notification ask too: the
    // account by the feed's id, which Pitboard switches to, or says in a notification why not.
    function switchTo(account: string) {
        if (!owner) {
            return;
        }
        DBus.SessionBus.asyncCall({
            "service": owner,
            "path": path,
            "iface": "org.freedesktop.Application",
            "member": "ActivateAction",
            "signature": "(sava{sv})",
            "arguments": ["switch-from-outside", [account], {}]
        });
    }

    function follow() {
        if (!watcher.registered) {
            owner = "";
            return;
        }
        DBus.SessionBus.asyncCall({
            "service": "org.freedesktop.DBus",
            "path": "/org/freedesktop/DBus",
            "iface": "org.freedesktop.DBus",
            "member": "GetNameOwner",
            "signature": "(s)",
            "arguments": [service]
        }, reply => {
            if (watcher.registered) {
                owner = String(reply.value);
            }
        }, () => {});
    }

    onOwnerChanged: text = ""

    readonly property DBus.DBusServiceWatcher watcher: DBus.DBusServiceWatcher {
        busType: DBus.BusType.Session
        watchedService: feed.service
        onRegisteredChanged: feed.follow()
    }

    // Read whole once the owner is known, then as each PropertiesChanged says, which Properties
    // hands on as the properties that changed.
    readonly property DBus.Properties reading: DBus.Properties {
        busType: DBus.BusType.Session
        service: feed.owner
        path: feed.path + "/WidgetFeed"
        iface: "com.usepitboard.Pitboard.WidgetFeed"
        onPropertiesChanged: (iface, changed, invalidated) => {
            if (invalidated.includes("Feed")) {
                feed.text = "";
            } else if (changed.Feed !== undefined) {
                feed.text = String(changed.Feed);
            }
        }
    }

    Component.onCompleted: follow()
}
