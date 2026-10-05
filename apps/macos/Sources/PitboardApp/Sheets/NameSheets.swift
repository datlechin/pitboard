import SwiftUI

/// Names the login signed in now, so Pitboard can park it: no browser, since the login is
/// already there.
struct NameSheet: View {
    let model: AppModel
    let provider: String
    let email: String
    @State private var name = ""
    @State private var saving = false
    @State private var failure: ActionFailure?
    @FocusState private var focused: Bool
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        SheetLayout(
            title: "Name This Account",
            message: "\(whoIsSignedIn(email)) is signed in to "
                + "\(model.tool(provider)?.name ?? provider). "
                + "Pitboard parks its login under this name whenever you switch to another "
                + "account."
        ) {
            Section {
                TextField("Name", text: $name, prompt: Text("work"))
                    .accessibilityIdentifier("sheet.name")
                    .focused($focused)
                    .onSubmit(save)
            }
            if let failure {
                SheetFailure(failure: failure)
            }
        } buttons: {
            // A name being saved cannot be withdrawn, so there is nothing to cancel.
            Button("Cancel", role: .cancel) { dismiss() }
                .keyboardShortcut(.cancelAction)
                .disabled(saving)
            Button("Save", action: save)
                .keyboardShortcut(.defaultAction)
                .disabled(trimmed(name).isEmpty || saving)
        }
        .onAppear { focused = true }
        .interactiveDismissDisabled(saving)
    }

    private func save() {
        let name = trimmed(name)
        guard !name.isEmpty, !saving else { return }
        saving = true
        Task {
            failure = await model.enrol(name, for: provider)
            saving = false
        }
    }
}

/// Gives an enrolled account a new name. It keeps its parked login and its place; only the
/// name changes, and only inside its own tool.
struct RenameSheet: View {
    let model: AppModel
    let provider: String
    let label: String
    @State private var name: String
    @State private var saving = false
    @State private var failure: ActionFailure?
    @FocusState private var focused: Bool
    @Environment(\.dismiss) private var dismiss

    init(model: AppModel, provider: String, label: String) {
        self.model = model
        self.provider = provider
        self.label = label
        _name = State(initialValue: label)
    }

    var body: some View {
        SheetLayout(
            title: "Rename “\(label)”",
            message: "The account keeps its parked login. Only the name you switch to it by "
                + "changes, here and in the command line."
        ) {
            Section {
                TextField("Name", text: $name, prompt: Text(label))
                    .accessibilityIdentifier("sheet.name")
                    .focused($focused)
                    .onSubmit(save)
            }
            if let failure {
                SheetFailure(failure: failure)
            }
        } buttons: {
            Button("Cancel", role: .cancel) { dismiss() }
                .keyboardShortcut(.cancelAction)
                .disabled(saving)
            Button("Rename", action: save)
                .keyboardShortcut(.defaultAction)
                .disabled(trimmed(name).isEmpty || trimmed(name) == label || saving)
        }
        .onAppear { focused = true }
        .interactiveDismissDisabled(saving)
    }

    private func save() {
        let name = trimmed(name)
        guard !name.isEmpty, name != label, !saving else { return }
        saving = true
        Task {
            failure = await model.rename(label, of: provider, to: name)
            saving = false
        }
    }
}
