import PitboardKit
import SwiftUI

// The account Name names is added already under the name shown: Cancel keeps that name.
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
        // A name being saved cannot be withdrawn.
        let busy = text?.saving ?? false
        let check = text.map { checkName(sheet: sheet, tool: $0.tool, typed: name) }
        Form {
            Section {
                TextField(words.sheet.name, text: $name, prompt: Text(text?.prompt ?? ""))
                    .accessibilityIdentifier("sheet.name")
                    .focused($focused)
                    .onSubmit(save)
            } header: {
                SheetHeader(title: text?.title ?? "", message: text?.message ?? "")
            } footer: {
                if let problem = check?.problem {
                    NameProblem(problem: problem)
                }
            }
            if let failure = model.sheetFailure {
                SheetFailure(failure: failure) { model.send($0) }
            }
        }
        .sheetForm()
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button(words.window.cancel, role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                    .disabled(busy)
                    .accessibilityIdentifier("sheet.cancel")
            }
            ToolbarItem(placement: .confirmationAction) {
                Button(text?.confirm ?? "", action: save)
                    .keyboardShortcut(.defaultAction)
                    .disabled(check?.intent == nil || busy)
                    .accessibilityIdentifier("sheet.confirm")
            }
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
