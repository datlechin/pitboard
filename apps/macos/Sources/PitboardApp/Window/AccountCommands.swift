import AppKit
import PitboardKit
import SwiftUI

// The account selected in the window, as its shortcut menu and the Account menu act on it.
struct AccountCommand {
    let item: AccountItem
    let choose: @MainActor (ItemOffer) -> Void
    let open: @MainActor (WindowOffer) -> Void
}

extension FocusedValues {
    @Entry var account: AccountCommand?
}

// The HIG lists a shortcut menu's items in the menu bar too, dimmed rather than left out.
struct AccountCommands: Commands {
    let model: AppModel
    @FocusedValue(\.account) private var account

    var body: some Commands {
        CommandMenu("Account") {
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
    nonisolated static func shown(_ offers: [ItemOffer], in place: Place) -> [ItemOffer] {
        place == .shortcutMenu ? offers.filter(\.enabled) : offers
    }

    var body: some View {
        let item = command.item
        ForEach(Self.shown(item.offers, in: place), id: \.title) { offer in
            Button(offer.title) { command.choose(offer) }
                .disabled(!offer.enabled)
        }
        if !item.windows.isEmpty {
            Divider()
            ForEach(item.windows, id: \.window.store) { offer in
                Button(offer.title) { command.open(offer) }
            }
        }
        // Edit > Copy copies it from the menu bar.
        if place == .shortcutMenu, !item.email.isEmpty {
            Divider()
            Button("Copy Email Address") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(item.email, forType: .string)
            }
        }
        if let forget = item.forget, place == .menuBar || forget.enabled {
            Divider()
            // The HIG shows keyboard shortcuts in the main menus, not in a shortcut menu.
            if place == .menuBar {
                Button(forget.title, role: .destructive) { command.choose(forget) }
                    .keyboardShortcut(.delete)
                    .disabled(!forget.enabled)
            } else {
                Button(forget.title, role: .destructive) { command.choose(forget) }
            }
        }
    }
}
