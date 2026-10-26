import AppKit
import PitboardKit
import SwiftUI

// The account selected in the window, as its shortcut menu and the Account menu act on it.
struct AccountCommand {
    let item: AccountItem
    let act: @MainActor (AccountAction) -> Void
}

extension FocusedValues {
    @Entry var account: AccountCommand?
}

// The HIG lists a shortcut menu's items in the menu bar too, dimmed rather than left out.
struct AccountCommands: Commands {
    let model: AppModel
    @FocusedValue(\.account) private var account

    var body: some Commands {
        CommandMenu(words.menu.account) {
            if let account {
                AccountMenuItems(command: account, place: .menuBar)
            } else {
                ForEach(model.accountMenu.offers, id: \.self) { title in
                    Button(title) {}
                        .disabled(true)
                }
                Divider()
                Button(model.accountMenu.forget) {}
                    .keyboardShortcut(.delete)
                    .disabled(true)
            }
        }
    }
}

struct AccountMenuItems: View {
    enum Place {
        case shortcutMenu
        case menuBar
    }

    let command: AccountCommand
    let place: Place

    // The HIG hides what cannot be chosen now in a shortcut menu, and dims it in the menu bar.
    nonisolated static func shown(_ actions: [AccountAction], in place: Place)
        -> [AccountAction]
    {
        place == .shortcutMenu ? actions.filter(\.enabled) : actions
    }

    var body: some View {
        let item = command.item
        let shown = Self.shown(item.actions, in: place)
        let opens = shown.filter { $0.kind == .openWindow }
        ForEach(shown.filter { $0.kind != .openWindow && $0.kind != .remove }, id: \.title) {
            action in
            Button(action.title) { command.act(action) }
                .disabled(!action.enabled)
        }
        if !opens.isEmpty {
            Divider()
            ForEach(opens, id: \.title) { action in
                Button(action.title) { command.act(action) }
            }
        }
        // Edit > Copy copies it from the menu bar.
        if place == .shortcutMenu, !item.email.isEmpty {
            Divider()
            Button(words.menu.copyEmail) {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(item.email, forType: .string)
            }
        }
        if let remove = shown.first(where: { $0.kind == .remove }) {
            Divider()
            // The HIG shows keyboard shortcuts in the main menus, not in a shortcut menu.
            if place == .menuBar {
                Button(remove.title, role: .destructive) { command.act(remove) }
                    .keyboardShortcut(.delete)
                    .disabled(!remove.enabled)
            } else {
                Button(remove.title, role: .destructive) { command.act(remove) }
            }
        }
    }
}
