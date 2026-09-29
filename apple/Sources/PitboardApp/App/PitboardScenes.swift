import SwiftUI

/// Every scene the app has, for the app target's `App` to return: the menu bar item, the
/// main window, a claude.ai window per account, the account picker, and the settings.
///
/// The menu bar item comes first, which makes it the scene the app starts with: an app with
/// no Dock icon opens no window at launch.
///
/// No scene handles an outside event. SwiftUI would otherwise route a pitboard link to a scene
/// of its choosing, and make a window for it: the main window, or an empty claude.ai window.
/// `PitboardDelegate` receives every link instead, and only a person's choice in the picker
/// opens a window.
public struct PitboardScenes: Scene {
    let model: AppModel
    let updates: any Updates

    public init(model: AppModel, updates: any Updates) {
        self.model = model
        self.updates = updates
    }

    public var body: some Scene {
        MenuBarExtra {
            MenuBarContent(model: model, updates: updates)
                .defaultAppStorage(model.defaults)
        } label: {
            MenuBarLabel(model: model)
                .defaultAppStorage(model.defaults)
        }
        .menuBarExtraStyle(.menu)
        .handlesExternalEvents(matching: [])

        Window("pitboard", id: MainWindow.id) {
            MainWindow(model: model)
                .defaultAppStorage(model.defaults)
        }
        .defaultSize(width: 760, height: 560)
        .commands {
            // Nothing here makes a document: what is new here is an account, from any pane.
            // That also keeps SwiftUI from adding New Window for the claude.ai windows.
            CommandGroup(replacing: .newItem) {
                Button("Add Account…") { model.present(.add(provider: nil)) }
                    .keyboardShortcut("n")
            }
            ClaudeCommands(model: model)
        }
        .handlesExternalEvents(matching: [])

        // One window per account, keyed by the account's store: opening it again brings it
        // forward. What SwiftUI saves to restore it is that derived identifier, never the
        // account's own id.
        WindowGroup("claude.ai", id: ClaudeWindow.id, for: UUID.self) { $store in
            ClaudeWindow(model: model, store: store)
                .defaultAppStorage(model.defaults)
        }
        .defaultSize(width: 1100, height: 800)
        .handlesExternalEvents(matching: [])

        // Opened only for a request. Its title is for the Window menu and VoiceOver: its
        // headline says it, as a sheet's does, and the title bar keeps the space below the
        // close button. The Window menu does not list it, since opening it from there, with
        // nothing to open, would close it at once.
        Window("Open claude.ai Link", id: LinkPicker.id) {
            LinkPicker(model: model)
                .defaultAppStorage(model.defaults)
        }
        .windowStyle(.hiddenTitleBar)
        .windowResizability(.contentSize)
        .defaultPosition(.center)
        .handlesExternalEvents(matching: [])
        .commandsRemoved()

        Settings {
            SettingsView(model: model, updates: updates)
                .defaultAppStorage(model.defaults)
        }
        .handlesExternalEvents(matching: [])
    }
}
