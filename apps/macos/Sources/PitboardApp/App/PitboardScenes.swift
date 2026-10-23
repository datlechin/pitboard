import PitboardKit
import SwiftUI

/// The menu bar item comes first, which makes it the scene the app starts with: an app with
/// no Dock icon opens no window at launch.
public struct PitboardScenes: Scene {
    let model: AppModel
    let windows: AccountWindows
    let requests: WindowRequests
    let openAtLogin: OpenAtLogin
    let notificationsAllowed: NotificationsAllowed
    let commandLineLink: CommandLineLink
    let defaults: UserDefaults
    let updates: any Updates

    public init(delegate: AppDelegate, updates: any Updates) {
        model = delegate.model
        windows = delegate.windows
        requests = delegate.windowRequests
        openAtLogin = delegate.openAtLogin
        notificationsAllowed = delegate.notificationsAllowed
        commandLineLink = delegate.commandLineLink
        defaults = delegate.defaults
        self.updates = updates
    }

    public var body: some Scene {
        scenes.defaultAppStorage(defaults)
    }

    @SceneBuilder private var scenes: some Scene {
        MenuBarExtra {
            MenuBarContent(model: model, windows: windows, updates: updates)
        } label: {
            MenuBarLabel(model: model, windows: windows, requests: requests)
        }
        .menuBarExtraStyle(.menu)

        Window("Pitboard", id: MainWindow.id) {
            MainWindow(model: model, windows: windows, requests: requests)
        }
        .defaultSize(width: 760, height: 560)
        .commands {
            // The standard item shows a beta's CFBundleShortVersionString, its X.Y.Z alone.
            CommandGroup(replacing: .appInfo) {
                Button("About Pitboard") { AppVersion.showAboutPanel() }
            }
            // Nothing here makes a document: what is new here is an account, from any pane.
            CommandGroup(replacing: .newItem) {
                Button("Add Account…") { model.send(.presentSheet(sheet: .add(provider: nil))) }
                    .keyboardShortcut("n")
            }
            AccountWindowCommands(windows: windows)
            RefreshCommands()
            // Pitboard's help is its documentation site; the app has no help book.
            CommandGroup(replacing: .help) {
                Link("Pitboard Help", destination: Links.documentation)
            }
        }

        AccountWindowScene(windows: windows)

        // A link the Share extension hands over, as a Pitboard link, comes here whatever
        // else is open, and only here: no other scene makes a window for one.
        Window("Open Link", id: AccountPicker.id) {
            AccountPicker(windows: windows)
                .onOpenURL { windows.receive($0) }
        }
        .handlesExternalEvents(matching: ["*"])
        .windowResizability(.contentSize)
        .defaultPosition(.center)
        .commandsRemoved()

        Settings {
            SettingsView(
                model: model, openAtLogin: openAtLogin, notifications: notificationsAllowed,
                commandLineLink: commandLineLink,
                presence: windows.presence, updates: updates
            )
        }
        // A taller pane grows the window downward, so it opens at the top of the screen,
        // where the tallest pane still clears the Dock.
        .defaultPosition(.top)
    }
}
