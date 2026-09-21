import SwiftUI

/// A menu bar app: no Dock icon, no window, `LSUIElement` in Info.plist. Its only scene is
/// the panel the status item opens.
@main
struct PitboardApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        MenuBarExtra {
            MenuView(model: model)
        } label: {
            Text(model.title)
        }
        .menuBarExtraStyle(.window)
    }
}
