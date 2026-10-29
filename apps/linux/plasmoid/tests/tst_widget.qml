// The widget over a stand-in for Pitboard on the test's own bus, which the bus starts only once
// something asks for Pitboard's name: what it shows of the feed, the switch it asks for, and that
// with no Pitboard running it asks for nothing and so starts nothing. Its pictures are drawn in
// the colours of the run, which run-tests.sh makes light, then dark.

pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import QtTest
import org.kde.kirigami as Kirigami
import org.kde.plasma.workspace.dbus as DBus
import "../com.usepitboard.Pitboard/contents/ui"

TestCase {
    id: test

    name: "Widget"
    // A TestCase is not drawn otherwise, and the widget is drawn in it.
    visible: true
    when: windowShown
    width: Kirigami.Units.gridUnit * 22
    height: Kirigami.Units.gridUnit * 26

    readonly property string service: "com.usepitboard.Pitboard"

    // Whether Pitboard's name has an owner: the stand-in, once the bus has started it.
    DBus.DBusServiceWatcher {
        id: pitboard

        busType: DBus.BusType.Session
        watchedService: test.service
    }

    // As a Plasma popup draws the widget, over the theme's background, reading its own feed, and
    // sizes it: its preferred size, else its implicit size, within its minimum and maximum
    // (plasma-desktop's CompactApplet.qml, libplasma's AppletPopup).
    component Popup: Rectangle {
        id: background

        readonly property Feed feed: Feed {}
        readonly property alias full: full

        color: Kirigami.Theme.backgroundColor
        width: Math.min(Math.max(full.Layout.preferredWidth > 0 ? full.Layout.preferredWidth
            : full.implicitWidth, full.Layout.minimumWidth), full.Layout.maximumWidth)
        height: Math.min(Math.max(full.Layout.preferredHeight > 0 ? full.Layout.preferredHeight
            : full.implicitHeight, full.Layout.minimumHeight), full.Layout.maximumHeight)

        FullRepresentation {
            id: full

            feed: background.feed
            anchors.fill: parent
        }
    }

    Component {
        id: popup

        Popup {}
    }

    function called(message) {
        const reply = DBus.SessionBus.asyncCall(message);
        tryVerify(() => reply.isFinished, 10000, message.member);
        verify(!reply.isError, `${message.member}: ${reply.error.message}`);
        return reply.value;
    }

    // As the desktop asks, by the service file the bus has for the name.
    function start() {
        called({
            "service": "org.freedesktop.DBus",
            "path": "/org/freedesktop/DBus",
            "iface": "org.freedesktop.DBus",
            "member": "StartServiceByName",
            "signature": "(su)",
            "arguments": [test.service, new DBus.uint32(0)]
        });
        tryVerify(() => pitboard.registered, 10000, "the stand-in on the bus");
    }

    function standIn(member: string): string {
        return String(called({
            "service": test.service,
            "path": "/StandIn",
            "iface": "com.usepitboard.Pitboard.StandIn",
            "member": member
        }) ?? "");
    }

    // What the stand-in was asked for, each action with its parameters.
    function asked(): var {
        return JSON.parse(standIn("Asked"));
    }

    // The stand-in feeds `glance` from now on, as Pitboard does once it reads again.
    function feeds(glance: var) {
        called({
            "service": test.service,
            "path": "/StandIn",
            "iface": "com.usepitboard.Pitboard.StandIn",
            "member": "Feed",
            "signature": "(s)",
            "arguments": [JSON.stringify(glance)]
        });
    }

    // A warning from the widget's own QML fails the test that caused it.
    function init() {
        failOnWarning(/com\.usepitboard\.Pitboard\/contents\/ui/);
    }

    function cleanup() {
        if (pitboard.registered) {
            standIn("Quit");
            tryVerify(() => !pitboard.registered, 10000, "the stand-in gone");
        }
    }

    function shown(): Popup {
        const drawn = createTemporaryObject(popup, test) as Popup;
        verify(drawn);
        return drawn;
    }

    // Every item under `item` drawn on the screen, depth first.
    function drawn(item: Item): list<Item> {
        let found = [];
        for (const child of item.children) {
            if (child.visible && child.opacity > 0) {
                found.push(child, ...drawn(child));
            }
        }
        return found;
    }

    // Whether `item` is drawn whole within `popup`.
    function whole(item: Item, popup: Item): bool {
        const at = item.mapToItem(popup, 0, 0);
        return at.x >= 0 && at.y >= 0 && at.x + item.width <= popup.width
            && at.y + item.height <= popup.height;
    }

    function said(item: Item): list<string> {
        return drawn(item)
            .filter(shown => typeof shown.text === "string" && shown.text !== "")
            .map(shown => shown.text);
    }

    // Every string the feed holds, wherever it holds it.
    function strings(value): list<string> {
        if (typeof value === "string") {
            return [value];
        }
        if (value === null || typeof value !== "object") {
            return [];
        }
        return Object.values(value).reduce((found, inner) => found.concat(strings(inner)), []);
    }

    function picture(item: Item, name: string) {
        waitForRendering(item);
        const dark = Kirigami.ColorUtils.brightnessForColor(Kirigami.Theme.backgroundColor)
            === Kirigami.ColorUtils.Dark;
        grabImage(item).save(`${name}${dark ? "-dark" : ""}.png`);
    }

    function test_shows_the_account_in_use_and_its_limits() {
        start();
        const drawnPopup = shown();
        const feed = drawnPopup.feed;
        tryVerify(() => feed.glance !== null, 10000, "the feed read");
        const glance = feed.glance;
        waitForRendering(drawnPopup);
        const shownWords = said(drawnPopup.full);

        for (const account of glance.accounts.filter(account => account.in_use)) {
            verify(shownWords.includes(account.name), `${account.name} among ${shownWords}`);
            for (const limit of account.limits) {
                for (const word of [limit.short, limit.figure, limit.resets].filter(word => word)) {
                    verify(shownWords.includes(word), `${account.name}: ${word}`);
                }
            }
        }
        verify(shownWords.includes(glance.words.in_use));
        verify(shownWords.includes(glance.words.updated));
        for (const tool of ["Claude Code", "Codex"]) {
            verify(shownWords.includes(tool), tool);
        }
        const fedWords = strings(glance);
        for (const word of shownWords) {
            verify(fedWords.includes(word), `${word} is not the feed's`);
        }
        picture(drawnPopup, "linux-widget");
    }

    function test_switch_asks_the_app_by_the_feeds_id() {
        start();
        const drawnPopup = shown();
        const feed = drawnPopup.feed;
        tryVerify(() => feed.glance !== null, 10000, "the feed read");
        const ready = feed.glance.accounts.find(account => account.standing === "ready");
        verify(ready);
        waitForRendering(drawnPopup);
        const row = drawn(drawnPopup.full)
            .find(item => item.account !== undefined && item.account.id === ready.id);
        verify(row, ready.name);
        const switches = drawn(row)
            .filter(item => item.pressed !== undefined && item.text === feed.glance.words.switch_button);
        compare(switches.length, 1);
        compare(asked(), []);

        mouseClick(switches[0]);

        // Asked after the click, on the same connection, so answered after the switch.
        compare(asked(), [["switch-from-outside", [ready.id]]]);
    }

    function test_without_pitboard_it_asks_nothing() {
        verify(!pitboard.registered);
        const drawnPopup = shown();
        const feed = drawnPopup.feed;
        feed.switchTo("c17c8cd1-64cc-5e37-bb6c-a31d984738ec");
        // The bus takes a connection's messages in order: once it answers, it has had whatever the
        // widget sent, and would be starting Pitboard for it.
        called({
            "service": "org.freedesktop.DBus",
            "path": "/org/freedesktop/DBus",
            "iface": "org.freedesktop.DBus",
            "member": "GetId"
        });
        wait(2000);

        verify(!pitboard.registered, "the widget started Pitboard");
        compare(feed.glance, null);
        compare(said(drawnPopup.full), []);
        const icons = drawn(drawnPopup.full).filter(item => item.source === test.service);
        compare(icons.length, 1);
        verify(whole(icons[0], drawnPopup), "the icon cut short");
        picture(drawnPopup, "linux-widget-not-running");

        // It follows a Pitboard that starts after it, with no poll.
        start();
        tryVerify(() => feed.glance !== null, 10000, "the feed read once Pitboard runs");
    }

    function test_with_no_accounts_it_says_so() {
        start();
        const drawnPopup = shown();
        const feed = drawnPopup.feed;
        tryVerify(() => feed.glance !== null, 10000, "the feed read");
        const words = feed.glance.words;
        feeds(Object.assign({}, feed.glance, {
            "accounts": []
        }));
        tryVerify(() => feed.glance !== null && feed.glance.accounts.length === 0, 10000,
            "the accounts gone");
        waitForRendering(drawnPopup);

        const saying = drawn(drawnPopup.full).filter(item => item.text === words.no_accounts);
        verify(saying.length > 0, words.no_accounts);
        for (const item of saying) {
            verify(whole(item, drawnPopup), `${words.no_accounts} cut short`);
        }
        picture(drawnPopup, "linux-widget-no-accounts");
    }
}
