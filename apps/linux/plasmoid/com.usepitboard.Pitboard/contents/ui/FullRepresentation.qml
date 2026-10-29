// Every account the running Pitboard feeds the widget, under each tool where there are two, and
// when Pitboard read them; before Pitboard has said anything, as Pitboard's other widgets do, its
// icon alone.

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components as PlasmaComponents

PlasmaComponents.Page {
    id: full

    required property Feed feed

    readonly property var glance: feed ? feed.glance : null
    readonly property var accounts: glance ? glance.accounts : []
    readonly property var words: glance ? glance.words : ({})
    // Each tool once, in the order the feed gives its accounts.
    readonly property var tools: accounts
        .map(account => account.tool_name)
        .filter((tool, at, all) => all.indexOf(tool) === at)

    implicitWidth: Kirigami.Units.gridUnit * 22
    implicitHeight: Math.min(
        shown.implicitHeight + (footer.visible ? footer.implicitHeight : 0)
            + Kirigami.Units.largeSpacing * 2,
        Kirigami.Units.gridUnit * 26)
    // main.qml's switch size, which holds the icon or the placeholder in place of the accounts.
    Layout.minimumWidth: Kirigami.Units.gridUnit * 16
    Layout.minimumHeight: Kirigami.Units.gridUnit * 10

    Kirigami.Icon {
        visible: !full.glance
        source: "com.usepitboard.Pitboard"
        anchors.centerIn: parent
        width: Kirigami.Units.iconSizes.huge
        height: width
    }

    Kirigami.PlaceholderMessage {
        visible: !!full.glance && full.accounts.length === 0
        text: full.words.no_accounts ?? ""
        anchors.centerIn: parent
        width: parent.width - Kirigami.Units.gridUnit * 4
    }

    PlasmaComponents.ScrollView {
        id: scroll

        visible: full.accounts.length > 0
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing
        contentWidth: availableWidth
        // QTBUG-83890, as Plasma's own applets avoid it: the bar stayed over the last limit once
        // the popup grew.
        PlasmaComponents.ScrollBar.horizontal.policy: PlasmaComponents.ScrollBar.AlwaysOff

        ColumnLayout {
            id: shown

            width: scroll.availableWidth
            spacing: Kirigami.Units.largeSpacing

            Repeater {
                model: full.tools

                delegate: ColumnLayout {
                    id: tool

                    required property string modelData

                    spacing: Kirigami.Units.largeSpacing
                    Layout.fillWidth: true

                    Kirigami.Heading {
                        visible: full.tools.length > 1
                        level: 4
                        text: tool.modelData
                        Layout.fillWidth: true
                    }

                    Repeater {
                        model: full.accounts.filter(account => account.tool_name === tool.modelData)

                        delegate: AccountRow {
                            required property var modelData

                            account: modelData
                            words: full.words
                            Layout.fillWidth: true
                            onSwitchAsked: id => full.feed.switchTo(id)
                        }
                    }
                }
            }
        }
    }

    footer: PlasmaComponents.Label {
        visible: text !== ""
        text: full.words.updated ?? ""
        color: Kirigami.Theme.disabledTextColor
        font: Kirigami.Theme.smallFont
        padding: Kirigami.Units.largeSpacing
        topPadding: 0
    }
}
