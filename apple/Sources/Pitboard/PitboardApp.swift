import SwiftUI

/// A menu bar app: no Dock icon, no window, `LSUIElement` in Info.plist. Its only scene is
/// the panel the status item opens.
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
    }
}
