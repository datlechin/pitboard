import AppKit
import PitboardKit
import SwiftUI

struct GeneralSettings: View {
    let openAtLogin: OpenAtLogin
    let notifications: NotificationsAllowed
    let updates: any Updates
    @AppStorage(DefaultsKey.menuBarShows) private var shows = MenuBarShows.nameAndUsage

    var body: some View {
        let said = words.settings
        Form {
            Section {
                Toggle(
                    said.openAtLogin,
                    isOn: Binding(
                        get: { openAtLogin.state != .disabled },
                        set: { openAtLogin.set($0) })
                )
                .accessibilityIdentifier("settings.openAtLogin")
                LoginItemApproval(openAtLogin: openAtLogin)
                if notifications.off {
                    LabeledContent {
                        Button(said.openNotificationSettings) {
                            notifications.openSystemSettings()
                        }
                    } label: {
                        Text(said.notifications)
                        Text(said.notificationsOff)
                    }
                    .accessibilityIdentifier("settings.notificationsOff")
                }
                Picker(macWords.menuBarShows, selection: $shows) {
                    ForEach(MenuBarShows.allCases) { option in
                        Text(option.title).tag(option)
                    }
                }
            }
            Section {
                if updates.available {
                    Toggle(
                        macWords.checkForUpdatesAutomatically,
                        isOn: Binding(
                            get: { updates.checksAutomatically },
                            set: { updates.checksAutomatically = $0 }))
                    Toggle(
                        macWords.downloadAndInstallAutomatically,
                        isOn: Binding(
                            get: { updates.installsAutomatically },
                            set: { updates.installsAutomatically = $0 })
                    )
                    .disabled(!updates.checksAutomatically)
                }
                LabeledContent {
                    HStack {
                        Text(AppVersion.whole() ?? "")
                        if updates.available {
                            Button(macWords.checkNow) { updates.check() }
                        }
                    }
                } label: {
                    Text(said.version)
                    if let waiting = updates.waiting {
                        Text(readyToInstall(version: waiting))
                    }
                }
            } header: {
                Text(macWords.updates)
            } footer: {
                // Without this, a build with no update key would look broken.
                if !updates.available {
                    Text(noUpdateKey()).footnote()
                        .accessibilityIdentifier("settings.noUpdateKey")
                }
            }
        }
        .formStyle(.grouped)
        .task {
            openAtLogin.read()
            await notifications.read()
            // Both are changed in System Settings; coming back reruns no `.task`.
            for await _ in NotificationCenter.default.notifications(
                named: NSApplication.didBecomeActiveNotification)
            {
                openAtLogin.read()
                await notifications.read()
            }
        }
    }
}

// Under the toggle that turns the login item on, in Settings and in the welcome.
struct LoginItemApproval: View {
    let openAtLogin: OpenAtLogin

    var body: some View {
        if openAtLogin.state == .requiresApproval {
            LabeledContent {
                Button(macWords.openLoginItems) {
                    openAtLogin.openSystemSettings()
                }
            } label: {
                Text(macWords.loginItemsWaiting).explanatory()
            }
        }
        if let failed = openAtLogin.failed {
            Text(failed).explanatory()
                .accessibilityIdentifier("settings.openAtLoginFailed")
        }
    }
}
