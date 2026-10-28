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
    let removal: AppRemoval
    let defaults: UserDefaults
    let updates: any Updates

    public init(delegate: AppDelegate, updates: any Updates) {
        model = delegate.model
        windows = delegate.windows
        requests = delegate.windowRequests
        openAtLogin = delegate.openAtLogin
        notificationsAllowed = delegate.notificationsAllowed
        commandLineLink = delegate.commandLineLink
        removal = delegate.removal
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
            MenuBarLabel(model: model, windows: windows, requests: requests, removal: removal)
        }
        .menuBarExtraStyle(.menu)

        Window(words.window.accounts, id: PlaceWindow.accounts.id) {
            AccountsWindow(model: model, windows: windows)
        }
        .defaultSize(width: 840, height: 560)
        .commands {
            // The standard item shows a beta's CFBundleShortVersionString, its X.Y.Z alone.
            CommandGroup(replacing: .appInfo) {
                Button(words.menu.about) { AppVersion.showAboutPanel() }
                if updates.available {
                    Button(words.menu.checkForUpdates) { updates.check() }
                }
            }
            // Nothing here makes a document: what is new here is an account, from any window.
            CommandGroup(replacing: .newItem) {
                Button(words.menu.addAccount) {
                    model.send(.presentSheet(sheet: .add(tool: nil)))
                }
                .keyboardShortcut("n")
            }
            AccountWindowCommands(windows: windows)
            AccountCommands(model: model)
            InspectorCommands()
            RefreshCommands()
            CopyReportCommands()
            // Pitboard's help is its documentation site; the app has no help book.
            CommandGroup(replacing: .help) {
                Link(words.menu.help, destination: Links.documentation)
            }
        }

        Window(model.welcome.title, id: PlaceWindow.welcome.id) {
            WelcomeWindow(model: model, openAtLogin: openAtLogin, presence: windows.presence)
        }
        .windowStyle(.hiddenTitleBar)
        .windowResizability(.contentSize)
        .defaultPosition(.center)
        // Shown once, so not listed in the Window menu.
        .commandsRemoved()

        // Each is listed in the Window menu, as Mail lists its Connection Doctor.
        Window(words.window.diagnostics, id: PlaceWindow.diagnostics.id) {
            DiagnosticsWindow(model: model, presence: windows.presence)
        }
        .defaultSize(width: 560, height: 620)

        Window(words.window.activity, id: PlaceWindow.activity.id) {
            ActivityWindow(model: model, presence: windows.presence)
        }
        .defaultSize(width: 720, height: 440)

        AccountWindowScene(windows: windows)

        // A link the Share extension hands over, as a Pitboard link, comes here whatever
        // else is open, and only here: no other scene makes a window for one.
        Window(words.window.openLink, id: AccountPicker.id) {
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
        .defaultPosition(.center)
    }
}
