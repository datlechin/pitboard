import PitboardKit
import SwiftUI

struct SettingsView: View {
    let model: AppModel
    let openAtLogin: OpenAtLogin
    let notifications: NotificationsAllowed
    let commandLineLink: CommandLineLink
    let presence: AppPresence
    let updates: any Updates

    // `DefaultsKey.settingsTab` keeps the pane last shown by these names, so none may change.
    enum Tab: String, CaseIterable {
        case general
        case limits
        case renewal
        case commandLine
        case updates
    }

    // A taller pane scrolls. With the window at the top of a 1024 x 768 screen, 590 points
    // are left between the toolbar and the Dock.
    static let tallestPane: CGFloat = 560

    @AppStorage(DefaultsKey.settingsTab) private var tab = Tab.general

    var body: some View {
        TabView(selection: $tab) {
            GeneralSettings(openAtLogin: openAtLogin, notifications: notifications)
                .tabItem { Label("General", systemImage: Symbol.general) }
                .tag(Tab.general)
            LimitsSettings(model: model)
                .tabItem { Label("Limits", systemImage: Symbol.limits) }
                .tag(Tab.limits)
            RenewalSettings(model: model)
                .tabItem { Label("Renewal", systemImage: Symbol.renewal) }
                .tag(Tab.renewal)
            CommandLineSettings(model: model, link: commandLineLink)
                .tabItem { Label("Command Line", systemImage: Symbol.terminal) }
                .tag(Tab.commandLine)
            UpdatesSettings(updates: updates)
                .tabItem { Label("Updates", systemImage: Symbol.update) }
                .tag(Tab.updates)
        }
        .frame(width: 500)
        .frame(maxHeight: Self.tallestPane)
        .fixedSize(horizontal: false, vertical: true)
        .appWindow(presence)
        // With no Dock icon, Settings would open behind every other app.
        .onAppear { presence.activate() }
    }
}
