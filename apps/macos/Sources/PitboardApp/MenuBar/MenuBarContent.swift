import AppKit
import PitboardKit
import SwiftUI

/// The menu the menu bar item opens.
///
/// A menu and not a popover, as the platform asks of a menu bar item: it opens at once,
/// closes predictably, reads well to VoiceOver, and works from the keyboard. It is the
/// glance and the switch: which account is in use in each tool, what each has left, and
/// anything that needs attention. Everything that needs typing or room happens in the
/// window, which every item here that needs one opens.
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
            Button("Add Account…") { model.send(.presentSheet(sheet: .add(provider: nil))) }
                .keyboardShortcut("n")
            Button {
                model.send(.refresh(asked: true))
            } label: {
                Text("Refresh")
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
            Button("Open Pitboard") { model.send(.showWindow(pane: nil)) }
                .keyboardShortcut("0")
            Button("Settings…") {
                // Choosing an item of a menu bar item's menu does not make the app active,
                // and settings already open would come forward behind the app in front.
                windows.presence.activate()
                openSettings()
            }
            .keyboardShortcut(",")
            if updates.available {
                Button("Check for Updates…") { updates.check() }
            }
        }
        Section {
            Button("About Pitboard") {
                NSApp.activate()
                NSApp.orderFrontStandardAboutPanel(nil)
            }
            Link("Pitboard Help", destination: Links.documentation)
            Button("Quit Pitboard") { NSApp.terminate(nil) }
                .keyboardShortcut("q")
        }
    }

    // MARK: - What needs attention

    /// How to install Claude Code where no tool is here; advice to switch, as items that
    /// switch; anything else to know about, as one item that opens the window where it is
    /// said in full; and an update that is ready.
    @ViewBuilder private var attention: some View {
        let notices = model.menuNotices
        let waiting = updates.available ? updates.waiting : nil
        if notices.install != nil || !notices.switches.isEmpty || notices.others != nil
            || waiting != nil
        {
            Section {
                if let install = notices.install {
                    Button {
                        if let link = install.link.flatMap(URL.init(string:)) { openURL(link) }
                    } label: {
                        Image(systemName: "questionmark.circle")
                        entry(install)
                    }
                }
                ForEach(Array(notices.switches.enumerated()), id: \.offset) { _, advice in
                    Button {
                        if let intent = advice.intent { model.send(intent) }
                    } label: {
                        Image(systemName: Symbol.switchAccount)
                        entry(advice)
                    }
                    .disabled(!advice.enabled)
                }
                if let others = notices.others {
                    Button {
                        if let intent = others.intent { model.send(intent) }
                    } label: {
                        Image(systemName: (others.severity ?? .warning).symbol)
                        entry(others)
                    }
                    .help(others.help ?? "")
                }
                if let waiting {
                    Button {
                        updates.check()
                    } label: {
                        Image(systemName: Symbol.update)
                        Text("Install Pitboard \(waiting)…")
                    }
                }
            }
        }
    }

    @ViewBuilder private func entry(_ entry: MenuEntry) -> some View {
        Text(entry.title)
        if let subtitle = entry.subtitle { Text(subtitle) }
    }

    // MARK: - Accounts

    /// A section per tool once there is more than one, headed with its name. The account in
    /// use in each is checked, and choosing another does what pressing it means.
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

/// One account in the menu: its name, what its limits stand at, and a check mark when it is
/// the one in use. Choosing it does what pressing it means: switch, sign in again, or name.
private struct AccountMenuItem: View {
    let item: AccountItem
    let perform: (Intent) -> Void

    var body: some View {
        Toggle(
            isOn: Binding(
                get: { item.inUse },
                // Choosing it does what pressing it means, whichever way the check mark
                // would go: the one in use stays in use until another is chosen, and a
                // login in use with no name yet is named from its own checked item.
                set: { _ in
                    if let action = item.action { perform(action.intent) }
                })
        ) {
            // A menu item draws plain text, so the plan's tag is said beside the name.
            Text(item.plan.map { "\(item.title) · \($0)" } ?? item.title)
            Text(item.summary)
        }
        .disabled(!item.inUse && item.action == nil)
        .help(item.help ?? "")
    }
}

/// Addresses the app links to, named once.
enum Links {
    static let documentation = URL(string: "https://docs.usepitboard.com")!
}
