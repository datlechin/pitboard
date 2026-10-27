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
        case advanced

        init(_ section: SettingsSection) {
            switch section {
            case .general: self = .general
            case .limits, .warmUp: self = .limits
            case .advanced: self = .advanced
            }
        }

        // The panes an earlier version kept open the one that holds them now.
        init?(rawValue: String) {
            switch rawValue {
            case "general", "updates": self = .general
            case "limits": self = .limits
            case "advanced", "renewal", "commandLine": self = .advanced
            default: return nil
            }
        }
    }

    // A taller pane scrolls.
    static let tallestPane: CGFloat = 560

    @AppStorage(DefaultsKey.settingsTab) private var tab = Tab.general

    var body: some View {
        TabView(selection: $tab) {
            GeneralSettings(
                openAtLogin: openAtLogin, notifications: notifications, updates: updates
            )
            .tabItem { Label(words.settings.general, systemImage: Symbol.general) }
            .tag(Tab.general)
            LimitsSettings(model: model)
                .tabItem { Label(words.settings.limits, systemImage: Symbol.limits) }
                .tag(Tab.limits)
            AdvancedSettings(model: model, link: commandLineLink)
                .tabItem { Label(words.settings.advanced, systemImage: Symbol.advanced) }
                .tag(Tab.advanced)
        }
        .frame(width: 500)
        .frame(maxHeight: Self.tallestPane)
        .fixedSize(horizontal: false, vertical: true)
        .appWindow(presence)
        // With no Dock icon, Settings would open behind every other app.
        .onAppear { presence.activate() }
    }
}
