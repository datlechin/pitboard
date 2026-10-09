import AppKit
import PitboardKit
import SwiftUI

/// Everything configurable, in the place macOS users look for it and open with
/// Command-comma.
struct SettingsView: View {
    let model: AppModel
    let openAtLogin: OpenAtLogin
    let commandLineLink: CommandLineLink
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
            GeneralSettings(model: model, openAtLogin: openAtLogin)
                .tabItem { Label("General", systemImage: Symbol.general) }
                .tag(Tab.general)
            CommandLineSettings(model: model, link: commandLineLink)
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
    let openAtLogin: OpenAtLogin
    @AppStorage(DefaultsKey.menuBarShows) private var shows = MenuBarShows.nameAndUsage

    var body: some View {
        let schedule = model.machine.schedule
        let renewal = model.machine.renewal
        let auto = model.machine.autoSwitch
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
                Picker("Menu bar shows", selection: $shows) {
                    ForEach(MenuBarShows.allCases) { option in
                        Text(option.title).tag(option)
                    }
                }
            }

            Section {
                // Off unless somebody turns it on, and held back until the app's preferences
                // are read, where a change would be lost under what they say.
                Toggle(
                    "Switch Claude Code automatically",
                    isOn: Binding(
                        get: { auto.on },
                        set: { model.send(.setAutoSwitch(on: $0, at: auto.at)) })
                )
                .accessibilityIdentifier("settings.autoSwitch")
                .disabled(!auto.enabled)
                Stepper(
                    auto.atLabel,
                    value: Binding(
                        get: { Int(auto.at) },
                        set: { model.send(.setAutoSwitch(on: auto.on, at: UInt8($0))) }),
                    in: Int(auto.lowest)...Int(auto.highest)
                )
                .accessibilityIdentifier("settings.autoSwitchAt")
                .disabled(!auto.enabled || !auto.on)
                if let standing = auto.standing {
                    Text(standing)
                        .explanatory()
                        .accessibilityIdentifier("settings.autoSwitchStanding")
                }
            } header: {
                Text("Before an account runs out")
            } footer: {
                Text(auto.note).footnote()
            }

            Section {
                // Shows what was asked for while the scheduler answers, and cannot be pressed
                // meanwhile; only turning it on is held back where this copy cannot, so a
                // schedule that cannot work can still be taken away.
                Toggle(
                    "Renew parked logins daily",
                    isOn: Binding(
                        get: { schedule.on }, set: { model.send(.setSchedule(on: $0)) })
                )
                .accessibilityIdentifier("settings.renewDaily")
                .disabled(!schedule.enabled)
                if let runs = schedule.runs {
                    LabeledContent("Runs", value: runs)
                }
                if let path = schedule.scheduledIn {
                    LabeledContent("Scheduled in") {
                        Text(path)
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                }
                if let note = schedule.note {
                    Text(note).explanatory()
                }
                if let failed = schedule.failed {
                    Text(failed).explanatory()
                }
                LabeledContent {
                    Button("Renew Now") { model.send(.renewNow) }
                        .disabled(renewal.renewing)
                } label: {
                    Text(renewal.note)
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
        }
        .formStyle(.grouped)
        .task {
            openAtLogin.read()
            // A terminal can change the schedule while the app runs.
            model.send(.readSchedule)
            // Approving Pitboard in Login Items happens in System Settings, and coming back
            // from there makes the app active again without showing this tab anew.
            for await _ in NotificationCenter.default.notifications(
                named: NSApplication.didBecomeActiveNotification)
            {
                openAtLogin.read()
            }
        }
    }
}

/// Where a terminal finds `pitboard`, and a way to put this app's own there when it finds
/// none. Only then: a `pitboard` found is one somebody installed, and a link in front of it
/// would change what their terminal runs without saying so.
private struct CommandLineSettings: View {
    let model: AppModel
    let link: CommandLineLink

    var body: some View {
        let shown = model.machine.commandLine
        Form {
            Section {
                LabeledContent("In your terminal") {
                    if let found = shown.inTerminal {
                        Text(found).foregroundStyle(.secondary).textSelection(.enabled)
                    } else {
                        ProgressView().controlSize(.small)
                    }
                }
                if shown.offersLink {
                    LabeledContent {
                        Button("Install Command Line Tool…") {
                            Task { await link.install() }
                        }
                        .disabled(link.linking)
                    } label: {
                        Text(
                            "Links \(link.target) to the one inside this app. macOS asks for "
                                + "an administrator’s password."
                        )
                        .explanatory()
                    }
                } else if let cannot = shown.cannotLink {
                    Text(cannot).explanatory()
                }
                if let failed = link.failed {
                    Text(failed).explanatory()
                }
            } footer: {
                VStack(alignment: .leading, spacing: Design.rowSpacing) {
                    if let note = shown.updateNote {
                        Text(note).footnote()
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
        .task { model.send(.lookForCommandLine) }
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
