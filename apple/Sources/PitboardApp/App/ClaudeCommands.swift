import SwiftUI

/// The main menu's commands for claude.ai windows: opening one, in the File menu after
/// Add Account…, and what acts on the claude.ai window in front, in the File and View menus.
///
/// Every command of a window's toolbar is here as well, with its shortcut, since a toolbar
/// can be hidden. Each acts on the focused claude.ai window through the actions it publishes,
/// and is disabled when none is. A key window's own control sees a shortcut before the main
/// menu does, so the pitboard window's Command-R still refreshes it.
struct ClaudeCommands: Commands {
    let model: AppModel
    @FocusedValue(\.claudeWindow) private var focused

    var body: some Commands {
        CommandGroup(after: .newItem) {
            ClaudeMenuItems(model: model)
            Button("Open Link…") { focused?.openLink() }
                .keyboardShortcut("l")
                .disabled(focused == nil)
            Button("Sign Out of This Window…") { focused?.signOut() }
                .disabled(focused == nil)
        }
        CommandGroup(after: .toolbar) {
            Button("Reload Page") { focused?.reload() }
                .keyboardShortcut("r")
                .disabled(focused == nil)
            Button("Back") { focused?.goBack() }
                .keyboardShortcut("[")
                .disabled(focused?.canGoBack != true)
            Button("Forward") { focused?.goForward() }
                .keyboardShortcut("]")
                .disabled(focused?.canGoForward != true)
            Divider()
            Button("Actual Size") { focused?.zoom(.actualSize) }
                .keyboardShortcut("0")
                .disabled(focused == nil)
            Button("Zoom In") { focused?.zoom(.zoomIn) }
                .keyboardShortcut("+")
                .disabled(focused == nil)
            Button("Zoom Out") { focused?.zoom(.zoomOut) }
                .keyboardShortcut("-")
                .disabled(focused == nil)
        }
    }
}

/// **Open claude.ai as work** with one enrolled Claude Code account, **Open claude.ai** with
/// a submenu of labels with several, and nothing with none: the same listing in pitboard's
/// menu and the File menu. A view, so the menus follow the model as a read changes it.
struct ClaudeMenuItems: View {
    let model: AppModel

    var body: some View {
        let menu = ClaudeMenu(model.claudeWindows)
        switch menu {
        case .none:
            EmptyView()
        case .one(let account):
            Button(menu.title ?? "") { model.openClaude(account.store) }
        case .several(let accounts):
            Menu(menu.title ?? "") {
                ForEach(accounts) { account in
                    Button(account.label) { model.openClaude(account.store) }
                }
            }
        }
    }
}
