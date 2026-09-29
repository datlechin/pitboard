import AppKit
import SwiftUI

/// What sits in the menu bar: pitboard's mark, and the account in use with its tightest
/// limit unless the settings say to show less.
///
/// The one view alive from launch to quit, so it is also what opens the main window when
/// the model asks for it: from the menu, from a notification's button, or on the first
/// launch ever. It opens an account's claude.ai window, and the account picker for a link
/// from outside, the same way, and gives the app a Dock icon while a claude.ai window is
/// open.
struct MenuBarLabel: View {
    let model: AppModel
    @Environment(\.openWindow) private var openWindow
    /// Whether this app has ever shown anyone anything.
    ///
    /// An app with no Dock icon that launches straight into a menu bar item shows a person
    /// who has just installed it nothing at all: no window, and nothing to explain the mark
    /// that appeared in their menu bar. Once, and never again.
    @AppStorage(DefaultsKey.hasBeenSeen) private var seen = false
    @AppStorage(DefaultsKey.menuBarShows) private var shows = MenuBarShows.nameAndUsage

    var body: some View {
        let title = menuTitle(for: model.status, order: model.tools, showing: shows)
        // A mark as well as words: on a crowded menu bar macOS drops the widest items
        // first, and an item that is only text is the widest thing up there. A Label would
        // render as the icon alone, so both are placed by hand.
        HStack(spacing: 4) {
            Image(systemName: Symbol.menuBar)
            if !title.isEmpty {
                Text(title).monospacedDigit()
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(model.spokenTitle)
        .onChange(of: model.windowRequests) {
            // An app with no Dock icon has nothing to bring forward but itself, and the
            // window opens behind whatever is in front otherwise.
            NSApp.activate()
            openWindow(id: MainWindow.id)
        }
        .onChange(of: model.web.requests) {
            guard let store = model.web.requested else { return }
            // Active, so the main menu shows and its Edit commands reach the page.
            NSApp.activate()
            openWindow(id: ClaudeWindow.id, value: store)
        }
        // A claude.ai window lives as long as a browser's, and behind another app a window
        // of an app with no Dock icon has no way back but pitboard's menu. So the app is in
        // the Dock and the Command-Tab switcher, with its menus in the menu bar, while one is
        // open, and leaves them when the last one closes. The Info.plist keeps it out at
        // launch.
        .onChange(of: model.web.wantsDockIcon) { _, wants in
            NSApp.setActivationPolicy(wants ? .regular : .accessory)
            // Only an app already in front: a window restored at login takes no focus.
            if wants, NSApp.isActive { NSApp.activate() }
        }
        // From the start as well: a link that launched the app can arrive before this
        // appears.
        .onChange(of: model.links.requests, initial: true) {
            guard model.links.pending != nil else { return }
            NSApp.activate()
            openWindow(id: LinkPicker.id)
        }
        .task {
            guard !seen else { return }
            seen = true
            model.showWindow()
        }
    }
}
