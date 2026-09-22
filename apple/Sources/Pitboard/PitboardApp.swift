import AppKit
import SwiftUI

/// A menu bar app: no Dock icon, `LSUIElement` in Info.plist. The panel the status item
/// opens is the glance, and the window and the settings are where the things that need room
/// go.
@main
struct PitboardApp: App {
    @State private var model = AppModel()
    @State private var updater = Updater()

    var body: some Scene {
        MenuBarExtra {
            MenuView(model: model, updater: updater)
        } label: {
            BarLabel(model: model)
        }
        .menuBarExtraStyle(.window)

        Window("pitboard", id: DetailWindow.id) {
            DetailWindow(model: model)
                // An app with no Dock icon opens a window behind everything otherwise,
                // because nothing has brought it to the front.
                .onAppear { NSApp.activate(ignoringOtherApps: true) }
        }
        .defaultSize(width: 760, height: 520)
        .commands {
            // A menu bar app has no application menu, so the only way to a window is a
            // keyboard shortcut and the panel's own button.
            CommandGroup(replacing: .newItem) {}
        }

        Settings {
            SettingsView(model: model, updater: updater)
                .onAppear { NSApp.activate(ignoringOtherApps: true) }
        }
    }
}

/// What sits in the menu bar, and the only view alive at launch.
private struct BarLabel: View {
    let model: AppModel
    @Environment(\.openWindow) private var openWindow
    /// Whether this app has ever shown anyone anything.
    ///
    /// An app with no Dock icon that launches straight into a status item shows a person
    /// who has just installed it nothing at all, and the cask installs the command line
    /// beside it, so there is not even a leftover window to explain the mark that appeared
    /// in their menu bar. Once, and never again.
    @AppStorage("hasBeenSeen") private var seen = false

    var body: some View {
        // A mark as well as words: on a crowded menu bar macOS drops the widest items
        // first, and an item that is only text is the widest thing up there. A Label would
        // render as the icon alone, so both are placed by hand.
        HStack(spacing: 4) {
            Image(systemName: "speedometer")
            if !model.title.isEmpty {
                Text(model.title)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(model.title.isEmpty ? "pitboard" : "pitboard, \(model.title)")
        .task {
            guard !seen else { return }
            seen = true
            NSApp.activate(ignoringOtherApps: true)
            openWindow(id: DetailWindow.id)
        }
    }
}
