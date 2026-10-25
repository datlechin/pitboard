import PitboardKit
import SwiftUI

/// Names the login signed in now, so Pitboard can park it, with no browser since the login
/// is already there; or gives an enrolled account a new name, which keeps its parked login
/// and its place, inside its own tool.
///
/// What it says, its default button's words, whether that can be pressed and what it sends
/// are the model's: the button sends what `checkName` returns, and the model saves by the
/// same rule.
struct NameSheet: View {
    let model: AppModel
    let sheet: Sheet
    @State private var name: String
    @FocusState private var focused: Bool
    @Environment(\.dismiss) private var dismiss

    init(model: AppModel, sheet: Sheet) {
        self.model = model
        self.sheet = sheet
        _name = State(initialValue: model.sheetText?.name ?? "")
    }

    var body: some View {
        let text = model.sheetText
        // A name being saved cannot be withdrawn, so there is nothing to cancel.
        let busy = text?.saving ?? false
        let check = checkName(sheet: sheet, tool: text?.tool ?? "", typed: name)
        SheetLayout(title: text?.title ?? "", message: text?.message ?? "") {
            Section {
                TextField("Name", text: $name, prompt: Text(text?.prompt ?? ""))
                    .accessibilityIdentifier("sheet.name")
                    .focused($focused)
                    .onSubmit(save)
            } footer: {
                if let problem = check.problem {
                    NameProblem(problem: problem)
                }
            }
            if let failure = model.sheetFailure {
                SheetFailure(failure: failure)
            }
        } buttons: {
            Button("Cancel", role: .cancel) { dismiss() }
                .keyboardShortcut(.cancelAction)
                .disabled(busy)
                .accessibilityIdentifier("sheet.cancel")
            Button(text?.confirm ?? "", action: save)
                .keyboardShortcut(.defaultAction)
                .disabled(check.intent == nil || busy)
                .accessibilityIdentifier("sheet.confirm")
        }
        .onAppear { focused = true }
        .onChange(of: name) {
            if model.sheetFailure != nil { model.send(.dismissSheetFailure) }
        }
        .interactiveDismissDisabled(busy)
    }

    private func save() {
        guard let text = model.sheetText, !text.saving,
            let intent = checkName(sheet: sheet, tool: text.tool, typed: name).intent
        else { return }
        model.send(intent)
    }
}
