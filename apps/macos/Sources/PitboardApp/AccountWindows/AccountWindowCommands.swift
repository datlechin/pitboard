import PitboardKit
import SwiftUI

/// Keyed by the account's store, so opening an account's window again brings the open one
/// forward.
struct AccountWindowScene: Scene {
    static let id = "account"

    let windows: AccountWindows

    var body: some Scene {
        WindowGroup(words.menu.account, id: Self.id, for: UUID.self) { $store in
            AccountWindowView(windows: windows, store: store)
        }
        .defaultSize(width: 1100, height: 800)
        // A window opens only for an account somebody chose, never empty from File > New.
        .commandsRemoved()
    }
}

struct AccountWindowCommands: Commands {
    let windows: AccountWindows
    @FocusedValue(WebSession.self) private var session

    var body: some Commands {
        CommandGroup(after: .newItem) {
            Divider()
            SiteMenuItems(windows: windows)
            Divider()
            Button(words.accountWindow.removeWebsiteData) { session?.removalAsked = true }
                .disabled(session == nil)
        }
        // Edit > Find, which a window's find bar answers.
        TextEditingCommands()
        CommandGroup(after: .toolbar) {
            Button(words.accountWindow.actualSize) { session?.page.zoom(.actualSize) }
                .keyboardShortcut("0")
                .disabled(session == nil)
            Button(words.accountWindow.zoomIn) { session?.page.zoom(.zoomIn) }
                .keyboardShortcut("+")
                .disabled(session == nil)
            Button(words.accountWindow.zoomOut) { session?.page.zoom(.zoomOut) }
                .keyboardShortcut("-")
                .disabled(session == nil)
            Divider()
        }
        // Finder's Go menu, with Safari's shortcuts for going back and forward.
        CommandMenu(words.accountWindow.go) {
            Button(words.accountWindow.back) { session?.page.goBack() }
                .keyboardShortcut("[")
                .disabled(session?.page.canGoBack != true)
            Button(words.accountWindow.forward) { session?.page.goForward() }
                .keyboardShortcut("]")
                .disabled(session?.page.canGoForward != true)
        }
    }
}

struct SiteMenuItems: View {
    let windows: AccountWindows
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        ForEach(windows.menus, id: \.self) { menu in
            switch menu {
            case .one(let title, let account):
                Button(title) { open(account) }
            case .several(let title, _, let accounts):
                Menu(title) {
                    ForEach(accounts) { account in
                        Button(account.title) { open(account) }
                    }
                }
            }
        }
    }

    private func open(_ account: WindowAccount) {
        windows.presence.activate()
        openWindow(id: AccountWindowScene.id, value: account.id)
    }
}
