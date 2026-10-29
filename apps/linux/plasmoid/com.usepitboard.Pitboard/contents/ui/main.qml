// Pitboard's Plasma widget: the accounts the running app feeds it, each limit of every account,
// and a switch the app makes. It reads and asks only through Feed, and every word it shows is the
// feed's. In a panel it is Plasma's own icon, which opens the accounts; while Pitboard is not
// running a system tray keeps it among its hidden items.

pragma ComponentBehavior: Bound

import QtQuick
import org.kde.kirigami as Kirigami
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.plasmoid

PlasmoidItem {
    id: root

    readonly property var glance: feed.glance

    Plasmoid.icon: "com.usepitboard.Pitboard-symbolic"
    Plasmoid.status: glance ? PlasmaCore.Types.ActiveStatus : PlasmaCore.Types.PassiveStatus

    // Each account in use, by its name, with its limits as the app's menu says them.
    toolTipSubText: {
        if (!glance) {
            return "";
        }
        if (glance.accounts.length === 0) {
            return glance.words.no_accounts;
        }
        const inUse = glance.accounts.filter(account => account.in_use);
        if (inUse.length === 0) {
            return glance.words.none_in_use;
        }
        return inUse.map(account => account.name + ": " + account.said).join("\n");
    }

    switchWidth: Kirigami.Units.gridUnit * 16
    switchHeight: Kirigami.Units.gridUnit * 10

    fullRepresentation: FullRepresentation {
        feed: feed
    }

    Feed {
        id: feed
    }
}
