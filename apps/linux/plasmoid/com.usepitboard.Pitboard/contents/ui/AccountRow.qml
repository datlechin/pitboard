// An account as the feed says it: its name and plan, In Use or Switch, and a row for each limit
// with its bar, its figure and when it resets, or what Pitboard says in their place. Switch is
// offered where Pitboard's other widgets offer it, on an account ready or expiring; each bar takes
// the colour of the step Pitboard says its limit is at, as the app's own bars do.

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components as PlasmaComponents

ColumnLayout {
    id: row

    required property var account
    // The feed's words, none for the moment the feed goes before its accounts do.
    required property var words

    // Switch, for the account the feed calls `id`.
    signal switchAsked(id: string)

    spacing: Kirigami.Units.smallSpacing

    RowLayout {
        spacing: Kirigami.Units.smallSpacing
        Layout.fillWidth: true

        PlasmaComponents.Label {
            text: row.account.name
            font.weight: Font.DemiBold
            elide: Text.ElideRight
            Layout.minimumWidth: 0
        }
        PlasmaComponents.Label {
            visible: text !== ""
            text: row.account.plan ?? ""
            color: Kirigami.Theme.disabledTextColor
            elide: Text.ElideRight
            Layout.minimumWidth: 0
        }
        Item {
            Layout.fillWidth: true
        }
        RowLayout {
            visible: row.account.in_use
            spacing: Kirigami.Units.smallSpacing

            Kirigami.Icon {
                source: "checkmark"
                Accessible.ignored: true
                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                Layout.preferredHeight: Kirigami.Units.iconSizes.small
            }
            PlasmaComponents.Label {
                text: row.words.in_use ?? ""
                color: Kirigami.Theme.disabledTextColor
            }
        }
        PlasmaComponents.Button {
            visible: !row.account.in_use
                && (row.account.standing === "ready" || row.account.standing === "expiring")
            text: row.words.switch_button ?? ""
            Accessible.description: row.account.name
            onClicked: row.switchAsked(row.account.id)
        }
    }

    PlasmaComponents.Label {
        visible: row.account.limits.length === 0
        text: row.account.said
        color: Kirigami.Theme.disabledTextColor
        wrapMode: Text.Wrap
        Layout.fillWidth: true
    }

    Repeater {
        model: row.account.limits

        delegate: RowLayout {
            id: limit

            required property var modelData

            spacing: Kirigami.Units.smallSpacing
            Layout.fillWidth: true

            PlasmaComponents.Label {
                text: limit.modelData.short
                color: Kirigami.Theme.disabledTextColor
                elide: Text.ElideRight
                Layout.preferredWidth: Kirigami.Units.gridUnit * 5
            }
            PlasmaComponents.ProgressBar {
                from: 0
                to: 100
                value: limit.modelData.used
                Accessible.name: limit.modelData.spoken
                Kirigami.Theme.inherit: false
                Kirigami.Theme.highlightColor: {
                    switch (limit.modelData.level) {
                    case "low":
                        return row.Kirigami.Theme.neutralTextColor;
                    case "out":
                        return row.Kirigami.Theme.negativeTextColor;
                    default:
                        return row.Kirigami.Theme.highlightColor;
                    }
                }
                Layout.fillWidth: true
            }
            PlasmaComponents.Label {
                text: limit.modelData.figure
                horizontalAlignment: Text.AlignRight
                Layout.preferredWidth: Kirigami.Units.gridUnit * 2.5
            }
            PlasmaComponents.Label {
                text: limit.modelData.resets
                color: Kirigami.Theme.disabledTextColor
                elide: Text.ElideRight
                Layout.preferredWidth: Kirigami.Units.gridUnit * 7
            }
        }
    }
}
