import SwiftUI

/// One command for every window, so Command-R means one item in the menu bar and the key
/// window's own action, and a toolbar's Refresh is in the menu bar as well.
struct RefreshCommand {
    let title: String
    let disabled: Bool
    let perform: @MainActor () -> Void
}

extension FocusedValues {
    @Entry var refresh: RefreshCommand?
}

struct RefreshCommands: Commands {
    @FocusedValue(\.refresh) private var refresh

    var body: some Commands {
        CommandGroup(before: .toolbar) {
            Button(refresh?.title ?? words.menu.refresh) { refresh?.perform() }
                .keyboardShortcut("r")
                .disabled(refresh == nil || refresh?.disabled == true)
            Divider()
        }
    }
}
