import AppKit
import PitboardKit
import SwiftUI

/// Everything configurable, in the place macOS users look for it and open with
/// Command-comma.
struct SettingsView: View {
    let model: AppModel
    let presence: AppPresence
    let updates: any Updates

    enum Tab: String {
        case general
        case commandLine
        case updates
    }

    @AppStorage("settingsTab") private var tab = Tab.general

    var body: some View {
        TabView(selection: $tab) {
            GeneralSettings(model: model, machine: model.machine)
                .tabItem { Label("General", systemImage: Symbol.general) }
                .tag(Tab.general)
            CommandLineSettings(machine: model.machine)
                .tabItem { Label("Command Line", systemImage: Symbol.terminal) }
                .tag(Tab.commandLine)
            UpdatesSettings(updates: updates)
                .tabItem { Label("Updates", systemImage: Symbol.update) }
                .tag(Tab.updates)
        }
        .frame(width: 500)
        .fixedSize(horizontal: false, vertical: true)
        .appWindow(presence)
        // An app with no Dock icon opens its settings behind everything otherwise, because
        // nothing has brought it to the front.
        .onAppear { presence.activate() }
    }
}

private struct GeneralSettings: View {
    let model: AppModel
    let machine: MachineModel
    @AppStorage(DefaultsKey.menuBarShows) private var shows = MenuBarShows.nameAndUsage

    var body: some View {
        Form {
            Section {
                Toggle(
                    "Open Pitboard at login",
                    isOn: Binding(
                        get: { machine.openAtLogin != .disabled },
                        set: { machine.setOpenAtLogin($0) })
                )
                .accessibilityIdentifier("settings.openAtLogin")
                if machine.openAtLogin == .requiresApproval {
                    LabeledContent {
                        Button("Open Login Items Settings…") { machine.openLoginItemSettings() }
                    } label: {
                        Text("macOS is waiting for you to allow Pitboard in Login Items.")
                            .explanatory()
                    }
                }
                if let failed = machine.loginItemFailed {
                    Text(failed).explanatory()
                }
                Picker("Menu bar shows", selection: $shows) {
                    ForEach(MenuBarShows.allCases) { option in
                        Text(option.title).tag(option)
                    }
                }
            }

            Section {
                Toggle(
                    "Renew parked logins daily",
                    isOn: Binding(
                        get: { machine.scheduling ?? machine.renewsDaily },
                        set: { wanted in Task { await machine.setSchedule(on: wanted) } })
                )
                .accessibilityIdentifier("settings.renewDaily")
                // Only turning it on, so a schedule that cannot work can still be taken away.
                .disabled(
                    machine.scheduling != nil
                        || (!machine.renewsDaily && machine.cannotSchedule != nil))
                if case .installed(let path, let every) = machine.schedule {
                    LabeledContent(
                        "Runs",
                        value: every == 86_400 ? "Every day" : "Every \(every / 3600) hours")
                    LabeledContent("Scheduled in") {
                        Text(path)
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                } else if case .unsupported = machine.schedule {
                    Text("This Mac has no scheduler Pitboard knows how to write to.")
                        .explanatory()
                } else if let why = machine.cannotSchedule {
                    Text(why).explanatory()
                }
                if let failed = machine.scheduleFailed {
                    Text(failed).explanatory()
                }
                LabeledContent {
                    Button("Renew Now") { Task { await machine.renewNow() } }
                        .disabled(machine.renewing)
                } label: {
                    Text(
                        machine.renewals.map { renewalNote(renewals: $0) }
                            ?? "Renew every parked login that is due.")
                }
            } header: {
                Text("While you’re away")
            } footer: {
                Text(
                    "A parked login is renewed whenever Pitboard runs, and otherwise not, so "
                        + "one you leave alone for weeks expires and needs a browser sign-in. "
                        + "Daily renewal hands that to your Mac’s own scheduler. It renews "
                        + "your parked logins and does nothing else: it never switches account "
                        + "and never asks for usage."
                )
                .footnote()
            }

            if model.desktopShown {
                DesktopSettings(model: model)
            }
        }
        .formStyle(.grouped)
        .task {
            machine.readLoginItem()
            await machine.readSchedule()
            // Approving Pitboard in Login Items happens in System Settings, and coming back
            // from there makes the app active again without showing this tab anew.
            for await _ in NotificationCenter.default.notifications(
                named: NSApplication.didBecomeActiveNotification)
            {
                machine.readLoginItem()
            }
        }
    }
}

/// Where Claude Desktop's numbers come from. Turning live usage on only shows the sheet that
/// explains the keychain's prompt; its Continue is what turns it on.
private struct DesktopSettings: View {
    let model: AppModel

    private var state: LiveUsageState {
        model.liveUsage
            ?? LiveUsageState(enabled: false, approval: "unknown", reason: nil, lastOkAt: nil)
    }

    var body: some View {
        Section {
            Toggle(
                "Show live usage",
                isOn: Binding(
                    get: { state.enabled },
                    set: { wanted in
                        if wanted {
                            model.liveUsageAsked()
                        } else {
                            Task { model.present(await model.liveUsageDisable()) }
                        }
                    })
            )
            .accessibilityIdentifier("settings.liveUsage")
            if state.enabled, state.approval == "needs_approval" {
                LabeledContent {
                    Button("Allow Again…") { model.liveUsageAsked() }
                } label: {
                    Text(liveUsageLine(state)).explanatory()
                }
            } else {
                Text(liveUsageLine(state)).explanatory()
            }
        } header: {
            Text("Claude Desktop")
        } footer: {
            Text(
                "Live usage asks claude.ai how much each Claude Desktop account has left, "
                    + "which takes reading a key Claude keeps in your login keychain. "
                    + "Switching accounts never reads it."
            )
            .footnote()
        }
    }
}

/// Where a terminal finds `pitboard`, and a way to put this app's own there when it finds
/// none. Only then: a `pitboard` found is one somebody installed, and a link in front of it
/// would change what their terminal runs without saying so.
private struct CommandLineSettings: View {
    let machine: MachineModel

    var body: some View {
        Form {
            Section {
                switch machine.commandLine {
                case .bundled(let path), .another(let path):
                    LabeledContent("In your terminal") {
                        Text(path).foregroundStyle(.secondary).textSelection(.enabled)
                    }
                case .nowhere:
                    LabeledContent("In your terminal", value: "Not installed")
                case nil:
                    LabeledContent("In your terminal") {
                        ProgressView().controlSize(.small)
                    }
                }
                if case .nowhere = machine.commandLine {
                    if machine.commandLineTool.linkable {
                        LabeledContent {
                            Button("Install Command Line Tool…") {
                                Task { await machine.installCommandLine() }
                            }
                            .disabled(machine.linking)
                        } label: {
                            Text(
                                "Links \(machine.commandLineTool.link) to the one inside this "
                                    + "app. macOS asks for an administrator’s password."
                            )
                            .explanatory()
                        }
                    } else if machine.commandLineTool.translocated {
                        Text(
                            "Move Pitboard to your Applications folder first. Until then macOS "
                                + "runs it from a temporary copy, and a link to that would break."
                        )
                        .explanatory()
                    }
                }
                if let failed = machine.linkFailed {
                    Text(failed).explanatory()
                }
            } footer: {
                VStack(alignment: .leading, spacing: Design.rowSpacing) {
                    switch machine.commandLine {
                    case .bundled: Text(updateNote(bundled: true)).footnote()
                    case .another: Text(updateNote(bundled: false)).footnote()
                    default: EmptyView()
                    }
                    // One literal, so its code spans are drawn as code.
                    Text(
                        """
                        The command line does everything the app does, and more: \
                        `pitboard status` in a script, and `pitboard repair` for a parked \
                        login Pitboard’s records have lost track of.
                        """
                    )
                    .footnote()
                }
            }
        }
        .formStyle(.grouped)
        .task { await machine.findCommandLine() }
    }
}

private struct UpdatesSettings: View {
    let updates: any Updates

    var body: some View {
        Form {
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
                                ?? version)
                    }
                }
            } else {
                Section {
                    LabeledContent("Version", value: version)
                } footer: {
                    // A build from a clone carries no update key and cannot update itself, so
                    // saying nothing would look like a setting that does not work.
                    Text(
                        "This copy of Pitboard can’t update itself: it was built from source "
                            + "and carries no update key. A copy from a release keeps itself up "
                            + "to date."
                    )
                    .footnote()
                }
            }
        }
        .formStyle(.grouped)
    }

    private var version: String {
        let info = Bundle.main.infoDictionary
        let short = info?["CFBundleShortVersionString"] as? String ?? ""
        return short.isEmpty ? "Pitboard" : "Pitboard \(short)"
    }
}
