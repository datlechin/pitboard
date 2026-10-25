import AppKit
import PitboardKit
import SwiftUI

struct GeneralSettings: View {
    let openAtLogin: OpenAtLogin
    let notifications: NotificationsAllowed
    let updates: any Updates
    @AppStorage(DefaultsKey.menuBarShows) private var shows = MenuBarShows.nameAndUsage

    var body: some View {
        Form {
            Section {
                Toggle(
                    "Open Pitboard at login",
                    isOn: Binding(
                        get: { openAtLogin.state != .disabled },
                        set: { openAtLogin.set($0) })
                )
                .accessibilityIdentifier("settings.openAtLogin")
                if openAtLogin.state == .requiresApproval {
                    LabeledContent {
                        Button("Open Login Items Settings…") {
                            openAtLogin.openSystemSettings()
                        }
                    } label: {
                        Text("macOS is waiting for you to allow Pitboard in Login Items.")
                            .explanatory()
                    }
                }
                if let failed = openAtLogin.failed {
                    Text(failed).explanatory()
                }
                if notifications.off {
                    LabeledContent {
                        Button("Open Notification Settings…") {
                            notifications.openSystemSettings()
                        }
                    } label: {
                        Text("Notifications from Pitboard are turned off in System Settings.")
                            .explanatory()
                    }
                    .accessibilityIdentifier("settings.notificationsOff")
                }
                Picker("Menu bar shows", selection: $shows) {
                    ForEach(MenuBarShows.allCases) { option in
                        Text(option.title).tag(option)
                    }
                }
            }
            if updates.available {
                Section {
                    Toggle(
                        "Check for updates automatically",
                        isOn: Binding(
                            get: { updates.checksAutomatically },
                            set: { updates.checksAutomatically = $0 }))
                    Toggle(
                        "Download and install updates automatically",
                        isOn: Binding(
                            get: { updates.installsAutomatically },
                            set: { updates.installsAutomatically = $0 })
                    )
                    .disabled(!updates.checksAutomatically)
                    LabeledContent {
                        Button("Check Now") { updates.check() }
                    } label: {
                        Text(
                            updates.waiting.map { "Pitboard \($0) is ready to install." }
                                ?? AppVersion.named())
                    }
                }
            } else {
                Section {
                    LabeledContent("Version", value: AppVersion.named())
                } footer: {
                    // Without this, a build with no update key would look broken.
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
