import PitboardKit
import SwiftUI

struct AdvancedSettings: View {
    let model: AppModel
    let link: CommandLineLink
    @State private var asking: Asking?

    var body: some View {
        let said = words.settings
        let schedule = model.machine.schedule
        let commandLine = model.machine.commandLine
        Form {
            Section {
                Toggle(
                    said.keepSavedSignInsFresh,
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
                LabeledContent(said.path) {
                    if let found = commandLine.inTerminal {
                        Text(found).foregroundStyle(.secondary).textSelection(.enabled)
                    } else {
                        ProgressView().controlSize(.small)
                            .accessibilityLabel(said.lookingForCommandLine ?? "")
                    }
                }
                if commandLine.offersLink {
                    LabeledContent {
                        Button(said.installCommandLine ?? "") {
                            Task { await link.install() }
                        }
                        .disabled(link.linking)
                        .accessibilityIdentifier("settings.installCommandLine")
                    } label: {
                        Text(linksTo(target: link.target)).explanatory()
                    }
                } else if let cannot = commandLine.cannotLink {
                    Text(cannot).explanatory()
                }
                if let failed = link.failed {
                    Text(failed).explanatory()
                }
            } header: {
                Text(said.commandLine)
            } footer: {
                if let note = commandLine.updateNote {
                    Text(note).footnote()
                }
            }

            UninstallSection(model: model) { offer in
                if let question = offer.confirm {
                    asking = Asking(question: question, intent: offer.intent)
                } else {
                    model.send(offer.intent)
                }
            }
        }
        .formStyle(.grouped)
        .asking($asking, of: model)
        // A terminal can change both while the app runs.
        .task {
            model.send(.readSchedule)
            model.send(.lookForCommandLine)
        }
    }
}

// The model's removal of Pitboard's folder; the app's own removal follows (AppRemoval).
private struct UninstallSection: View {
    let model: AppModel
    let ask: (ItemOffer) -> Void

    var body: some View {
        let offer = model.uninstall
        Section {
            LabeledContent(words.settings.uninstall) {
                Button(offer.title) { ask(offer) }
                    .disabled(!offer.enabled)
                    .accessibilityIdentifier("settings.uninstall")
            }
            switch model.removal {
            case .removing(let title)?:
                WaitingRow(text: title)
                    .accessibilityIdentifier("settings.uninstalling")
            case .stopped(let title, let line, let again)?:
                LabeledContent {
                    Button(again.title) { model.send(again.intent) }
                        .disabled(!again.enabled)
                        .accessibilityIdentifier("settings.uninstallAgain")
                } label: {
                    Text(title)
                    Text(line)
                }
                .accessibilityIdentifier("settings.uninstallStopped")
            case .done?, nil:
                EmptyView()
            }
        } footer: {
            Text(words.settings.uninstallNote).footnote()
        }
    }
}
