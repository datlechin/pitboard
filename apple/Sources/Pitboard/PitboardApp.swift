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
            // A mark as well as words: on a crowded menu bar macOS drops the widest items
            // first, and an item that is only text is the widest thing up there. A Label
            // would render as the icon alone, so both are placed by hand.
            HStack(spacing: 4) {
                Image(systemName: "speedometer")
                if !model.title.isEmpty {
                    Text(model.title)
                }
            }
        }
        .menuBarExtraStyle(.window)

        Window("pitboard", id: DetailWindow.id) {
            DetailWindow(model: model)
                // An app with no Dock icon opens a window behind everything otherwise,
                // because nothing has brought it to the front.
                .onAppear { NSApp.activate(ignoringOtherApps: true) }
        }
        .defaultSize(width: 720, height: 480)
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
