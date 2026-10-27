import Accessibility
import PitboardKit
import SwiftUI

struct AccountSheetView: View {
    let model: AppModel
    let sheet: Sheet

    var body: some View {
        Group {
            switch sheet {
            case .add, .signInAgain:
                SignInSheet(model: model, sheet: sheet)
            case .name, .rename:
                NameSheet(model: model, sheet: sheet)
            case .stow:
                StowSheet(model: model)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("sheet.\(caseName(sheet))")
        // Only while the model keeps this sheet: as it closes, the window's take over.
        .windowAlerts(model, shown: model.sheet?.id == sheet.id)
    }
}

struct SheetHeader: View {
    let title: String
    var message = ""

    var body: some View {
        VStack(alignment: .leading, spacing: Design.rowSpacing) {
            Text(title)
                .font(.headline)
                .foregroundStyle(.primary)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("sheet.title")
            if !message.isEmpty {
                Text(message)
                    .font(.body)
                    .fontWeight(.regular)
                    .foregroundStyle(.primary)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("sheet.message")
            }
        }
        .padding(.bottom, Design.lineSpacing)
    }
}

extension View {
    // A sheet draws its toolbar's cancellation and confirmation items in a bar of its own.
    func sheetForm() -> some View {
        formStyle(.grouped)
            .scrollDisabled(true)
            .fixedSize(horizontal: false, vertical: true)
            .frame(width: 400)
    }
}

struct WaitingRow: View {
    let text: String
    // what VoiceOver calls the spinner, where the words beside it do not say it
    var spoken: String?

    var body: some View {
        Label {
            Text(text).fixedSize(horizontal: false, vertical: true)
        } icon: {
            ProgressView()
                .controlSize(.small)
                .accessibilityLabel(spoken ?? "")
                .accessibilityHidden(spoken == nil)
        }
    }
}

struct Steps: View {
    let steps: [String]

    var body: some View {
        ForEach(Array(steps.enumerated()), id: \.offset) { index, step in
            Label {
                Text(step).fixedSize(horizontal: false, vertical: true)
            } icon: {
                Image(systemName: "\(index + 1).circle")
            }
            .accessibilityIdentifier("sheet.step")
        }
    }
}

struct ToolMissing: View {
    let missing: Missing

    var body: some View {
        Section {
            Label {
                VStack(alignment: .leading, spacing: Design.lineSpacing) {
                    Text(missing.title).fontWeight(.medium)
                    Text(missing.detail).explanatory()
                    if let page = URL(string: missing.link) {
                        Link(missing.linkTitle, destination: page)
                            .accessibilityIdentifier("sheet.missing.link")
                    }
                }
            } icon: {
                Image(systemName: Severity.warning.symbol)
                    .foregroundStyle(Severity.warning.tint)
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("sheet.missing")
        }
        // Picking a tool Pitboard did not find changes the sheet below the field in focus.
        .onAppear(perform: announce)
        .onChange(of: missing.title, announce)
    }

    private func announce() {
        AccessibilityNotification.Announcement("\(missing.title) \(missing.detail)").post()
    }
}

struct NameProblem: View {
    let problem: String

    var body: some View {
        Text(problem)
            .footnote()
            .accessibilityIdentifier("sheet.problem")
            .onChange(of: problem, initial: true) {
                AccessibilityNotification.Announcement(problem).post()
            }
    }
}

// Said inside the sheet that met it, where the name typed is still there to correct.
struct SheetFailure: View {
    let failure: FailureText
    let send: (Intent) -> Void

    var body: some View {
        Section {
            Label {
                VStack(alignment: .leading, spacing: Design.lineSpacing) {
                    Text(failure.title).fontWeight(.medium)
                    Text(failure.message).explanatory().textSelection(.enabled)
                    ForEach(failure.warnings, id: \.self) { warning in
                        Text(warning).explanatory()
                    }
                    NextStepControl(step: failure.next, send: send)
                        .padding(.top, Design.lineSpacing)
                }
            } icon: {
                Image(systemName: Severity.error.symbol)
                    .foregroundStyle(Severity.error.tint)
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("sheet.failure")
        }
        // It appears where nobody's focus is, after the button that was pressed, and
        // VoiceOver does not read what appears by itself.
        .onAppear(perform: announce)
        .onChange(of: failure.id, announce)
    }

    private func announce() {
        AccessibilityNotification.Announcement("\(failure.title). \(failure.message)").post()
    }
}

struct NextStepControl: View {
    let step: NextStep
    let send: (Intent) -> Void

    var body: some View {
        switch step {
        case .act(let choice):
            Button(choice.title) { send(choice.intent) }
                .disabled(!choice.enabled)
                .accessibilityIdentifier("next.\(caseName(choice.intent))")
        case .learn(let link):
            if let page = URL(string: link.link) {
                Link(link.title, destination: page)
                    .accessibilityIdentifier("next.learn")
            }
        }
    }
}
