import AppKit
import PitboardKit
import SwiftUI

// A label's second Text is its menu item's native subtitle (docs/facts/macos-app.md).
struct MenuBarContent: View {
    let model: AppModel
    let windows: AccountWindows
    let updates: any Updates
    @Environment(\.openURL) private var openURL
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        attention
        accounts
        Section {
            Button(words.menu.addAccount) {
                model.send(.presentSheet(sheet: .add(tool: nil)))
            }
            .keyboardShortcut("n")
            Button {
                model.send(.refresh(asked: true))
            } label: {
                Text(words.menu.refresh)
                Text(model.updatedMenu)
            }
            .keyboardShortcut("r")
            .disabled(model.reading)
        }
        if !windows.menus.isEmpty {
            Section {
                SiteMenuItems(windows: windows)
            }
        }
        Section {
            Button(words.menu.openPitboard) { model.send(.show(place: nil)) }
            Button(words.menu.settings) {
                // Choosing an item of a menu bar item's menu does not make the app active,
                // and settings already open would come forward behind the app in front.
                windows.presence.activate()
                openSettings()
            }
            .keyboardShortcut(",")
            if updates.available {
                Button(words.menu.checkForUpdates) { updates.check() }
            }
        }
        Section {
            Button(words.menu.about) { AppVersion.showAboutPanel() }
            Link(words.menu.help, destination: Links.documentation)
            Button(words.menu.quit) { NSApp.terminate(nil) }
                .keyboardShortcut("q")
        }
    }

    // MARK: - What needs attention

    @ViewBuilder private var attention: some View {
        let notices = model.menuNotices
        let waiting = updates.available ? updates.waiting : nil
        let acting = notices.switches + notices.signIns
        if notices.install != nil || !acting.isEmpty || notices.others != nil || waiting != nil
        {
            Section {
                if let install = notices.install {
                    Button {
                        if let link = install.link.flatMap(URL.init(string:)) { openURL(link) }
                    } label: {
                        entry(install)
                    }
                }
                ForEach(Array(acting.enumerated()), id: \.offset) { _, item in
                    Button {
                        if let intent = item.intent { model.send(intent) }
                    } label: {
                        entry(item)
                    }
                    .disabled(!item.enabled)
                }
                if let others = notices.others {
                    Button {
                        if let intent = others.intent { model.send(intent) }
                    } label: {
                        entry(others)
                    }
                }
                if let waiting {
                    Button(installUpdate(version: waiting)) { updates.check() }
                }
            }
        }
    }

    @ViewBuilder private func entry(_ entry: MenuEntry) -> some View {
        Text(entry.title)
        if let subtitle = entry.subtitle { Text(subtitle) }
    }

    // MARK: - Accounts

    @ViewBuilder private var accounts: some View {
        if let note = model.menuAccountsNote {
            Section {
                Text(note)
            }
        } else {
            ForEach(model.sections, id: \.id) { section in
                if let heading = section.heading {
                    Section(heading) { items(of: section) }
                } else {
                    Section { items(of: section) }
                }
            }
        }
    }

    private func items(of section: AccountSection) -> some View {
        ForEach(section.accounts, id: \.id) { item in
            AccountMenuItem(item: item) { model.send($0) }
        }
    }
}

private struct AccountMenuItem: View {
    let item: AccountItem
    let perform: (Intent) -> Void

    var body: some View {
        Toggle(
            isOn: Binding(
                get: { item.inUse },
                // Choosing it does what pressing it means, whichever way the checkmark would
                // go: the one in use stays in use, and a sign-in not added yet is added.
                set: { _ in
                    if let action = item.action { perform(action.intent) }
                })
        ) {
            Text(item.menuTitle)
            Text(item.summary)
        }
        .disabled(!item.inUse && item.action == nil)
    }
}

enum Links {
    static let documentation = URL(string: "https://docs.usepitboard.com")!
}
