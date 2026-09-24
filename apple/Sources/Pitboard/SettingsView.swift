import PitboardKit
import ServiceManagement
import SwiftUI

/// Everything configurable, in the place macOS users look for it.
///
/// It all lived in an ellipsis menu at the bottom right of a 400-point panel: opening at
/// login sat between hiding the checks and quitting. People look for preferences in one
/// place and open them with Command-comma, and there was nowhere to put anything that
/// needed more than a line.
struct SettingsView: View {
    let model: AppModel
    let updater: Updater

    var body: some View {
        TabView {
            General(model: model)
                .tabItem { Label("General", systemImage: "gear") }
            Updates(updater: updater)
                .tabItem { Label("Updates", systemImage: "arrow.down.circle") }
            Advanced(model: model)
                .tabItem { Label("Advanced", systemImage: "wrench.and.screwdriver") }
        }
        .frame(width: 460)
        .scenePadding()
    }
}

private struct General: View {
    let model: AppModel
    @State private var openAtLogin = SMAppService.mainApp.status == .enabled

    var body: some View {
        Form {
            Toggle("Open at login", isOn: $openAtLogin)
                .onChange(of: openAtLogin) { _, wanted in
                    try? wanted
                        ? SMAppService.mainApp.register()
                        : SMAppService.mainApp.unregister()
                    openAtLogin = SMAppService.mainApp.status == .enabled
                }

            Section {
                Toggle("Renew parked logins daily", isOn: renewing)
                Text(
                    """
                    A parked login is renewed whenever pitboard runs, and otherwise not, so \
                    one you leave alone for weeks expires and needs a browser sign-in. This \
                    hands that to your computer's own scheduler.

                    It renews your own parked logins and does nothing else: it never \
                    switches account and never asks \(model.services) for usage.
                    """
                )
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

                if case .installed(let path, let every) = model.schedule {
                    LabeledContent("Runs") {
                        Text("every \(hours(every))")
                            .foregroundStyle(.secondary)
                    }
                    LabeledContent("Installed at") {
                        Text(path).font(.caption).textSelection(.enabled)
                    }
                } else if case .unsupported = model.schedule {
                    Text("This computer has no scheduler pitboard knows how to write.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                HStack {
                    Button("Renew now") { Task { await model.renewNow() } }
                    if let renewals = model.renewals {
                        Text(renewalNote(renewals))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            } header: {
                Text("While you are away")
            }
        }
        .formStyle(.grouped)
        .task { await model.readSchedule() }
    }

    private var renewing: Binding<Bool> {
        Binding(
            get: {
                if case .installed = model.schedule { return true }
                return false
            },
            set: { wanted in Task { await model.setSchedule(on: wanted) } }
        )
    }

    private func hours(_ seconds: UInt32) -> String {
        let hours = Int(seconds) / 3600
        return hours == 24 ? "day" : "\(hours) hours"
    }
}

private struct Updates: View {
    let updater: Updater

    var body: some View {
        Form {
            if updater.available {
                Toggle("Check for updates automatically", isOn: updater.checksAutomatically)
                Toggle("Install them when pitboard quits", isOn: updater.installsAutomatically)
                HStack {
                    Button("Check now") { updater.check() }
                    if let waiting = updater.waiting {
                        Text("\(waiting) is ready").font(.caption).foregroundStyle(.secondary)
                    }
                }
            } else {
                // A build from a clone carries no update key and Sparkle will not run
                // without one, so saying nothing would look like a setting that does not work.
                Text(
                    """
                    This copy of pitboard cannot update itself: it was built from source and \
                    carries no update key. A copy from a release keeps itself up to date.
                    """
                )
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            }
        }
        .formStyle(.grouped)
    }
}

private struct Advanced: View {
    let model: AppModel

    var body: some View {
        Form {
            Section {
                Text(
                    """
                    The panel adds and drops accounts. The command line, which the cask \
                    installs too, also renames them with `pitboard rename`, and `pitboard \
                    repair` accounts for any parked login pitboard's own records have lost \
                    track of.
                    """
                )
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            } header: {
                Text("The command line")
            }

            Section {
                ForEach(model.checks, id: \.code) { check in
                    LabeledContent(check.name) {
                        Text(check.detail)
                            .foregroundStyle(check.level == .ok ? .secondary : .primary)
                            .multilineTextAlignment(.trailing)
                    }
                }
                Button(model.checks.isEmpty ? "Check this machine" : "Check again") {
                    Task { await model.diagnose() }
                }
            } header: {
                Text("What pitboard found")
            }
        }
        .formStyle(.grouped)
    }
}
