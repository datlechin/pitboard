import PitboardKit
import SwiftUI

struct AdvancedSettings: View {
    let model: AppModel
    let link: CommandLineLink

    var body: some View {
        let schedule = model.machine.schedule
        let commandLine = model.machine.commandLine
        Form {
            Section {
                Toggle(
                    "Renew parked logins daily",
                    isOn: Binding(
                        get: { schedule.on }, set: { model.send(.setSchedule(on: $0)) })
                )
                .accessibilityIdentifier("settings.renewDaily")
                .disabled(!schedule.enabled)
                if let note = schedule.note {
                    Text(note).explanatory()
                }
                if let failed = schedule.failed {
                    Text(failed).explanatory()
                }
            } footer: {
                Text(schedule.about).footnote()
            }

            Section {
                LabeledContent("In your terminal") {
                    if let found = commandLine.inTerminal {
                        Text(found).foregroundStyle(.secondary).textSelection(.enabled)
                    } else {
                        ProgressView().controlSize(.small)
                    }
                }
                if commandLine.offersLink {
                    LabeledContent {
                        Button("Install Command Line Tool…") {
                            Task { await link.install() }
                        }
                        .disabled(link.linking)
                        .accessibilityIdentifier("settings.installCommandLine")
                    } label: {
                        Text(
                            "Links \(link.target) to the one inside this app. macOS asks for "
                                + "an administrator’s password."
                        )
                        .explanatory()
                    }
                } else if let cannot = commandLine.cannotLink {
                    Text(cannot).explanatory()
                }
                if let failed = link.failed {
                    Text(failed).explanatory()
                }
            } footer: {
                if let note = commandLine.updateNote {
                    Text(note).footnote()
                }
            }
        }
        .formStyle(.grouped)
        // A terminal can change both while the app runs.
        .task {
            model.send(.readSchedule)
            model.send(.lookForCommandLine)
        }
    }
}
