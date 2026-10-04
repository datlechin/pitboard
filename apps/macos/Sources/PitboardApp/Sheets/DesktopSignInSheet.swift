import PitboardKit
import SwiftUI

/// Adds a Claude Desktop account, or signs in again to one whose sign-in has lapsed.
///
/// Claude signs in inside Claude, not through a sign-in Pitboard can run, so this takes
/// somebody through it a step at a time: Pitboard puts the account in use aside and leaves
/// Claude signed out, somebody signs in to the other account in Claude, and Pitboard names
/// what they signed in to. Claude is quit around each change to its files and opened again
/// after, and nothing is signed out on claude.ai, so the account put aside comes back with
/// one click.
struct DesktopSignInSheet: View {
    let model: AppModel
    /// The account being signed in to again, or nil for a new one.
    let again: String?
    @State private var step: Step
    @State private var name: String
    @State private var failure: ActionFailure?
    @State private var working = false
    @FocusState private var nameFocused: Bool
    @Environment(\.dismiss) private var dismiss

    enum Step {
        /// What is about to happen, before anything has.
        case explain
        /// Claude is signed out, waiting for somebody to sign in to it.
        case signIn
        /// Signed in, waiting for a name.
        case name
    }

    init(model: AppModel, again: String?) {
        self.model = model
        self.again = again
        // An add left halfway, by this app or one that quit, carries on where it stopped.
        _step = State(initialValue: model.desktopAwaiting == nil ? .explain : .signIn)
        _name = State(initialValue: again ?? "")
    }

    var body: some View {
        switch step {
        case .explain: explain
        case .signIn: signIn
        case .name: naming
        }
    }

    /// The account put aside, by its name: the one waiting to be put back, or the one in use
    /// now.
    private var from: String {
        model.desktopAwaiting?.fromLabel
            ?? model.status?.accounts.first {
                $0.provider == desktopProvider && $0.signedIn
            }?.label
            ?? "the account in use"
    }

    // MARK: - Before anything happens

    private var explain: some View {
        SheetLayout(
            title: again.map { "Sign In to \($0) Again" } ?? "Add another Claude account",
            message: "Pitboard puts \(from) aside and leaves Claude signed out. Nothing is "
                + "signed out on claude.ai, so \(from) comes back with one click."
        ) {
            if again == nil, model.addable.count > 1 {
                Section {
                    Picker(
                        "Tool",
                        selection: Binding(
                            get: { desktopProvider },
                            set: { model.present(.add(provider: $0)) })
                    ) {
                        ForEach(model.addable, id: \.code) { tool in
                            Text(tool.name).tag(tool.code)
                        }
                    }
                    .accessibilityIdentifier("sheet.tool")
                }
            }
            if let failure {
                SheetFailure(failure: failure)
            }
        } buttons: {
            Button("Cancel", role: .cancel) { dismiss() }
                .keyboardShortcut(.cancelAction)
            Button("Continue", action: putAside)
                .keyboardShortcut(.defaultAction)
                .disabled(working)
        }
    }

    // MARK: - While Claude is signed out

    /// Claude was not open when the account was put aside, so Pitboard left it closed and
    /// this says to open it rather than taking it to be open.
    private var signIn: some View {
        SheetLayout(
            title: again.map { "Sign In to \($0) Again" } ?? "Add another Claude account",
            message: (model.claudeLeftClosed ? "Open Claude and sign in" : "Sign in")
                + " to \(again ?? "the other account") in Claude. Come back here when you "
                + "see its chats."
        ) {
            if let failure {
                SheetFailure(failure: failure)
            }
        } buttons: {
            // Closing it leaves the add waiting, and the menu offers to finish it.
            Button("Later", role: .cancel) { dismiss() }
                .keyboardShortcut(.cancelAction)
            if model.claudeLeftClosed {
                Button("Open Claude") { failure = model.openClaudeAsked() }
                    .accessibilityIdentifier("sheet.openClaude")
                    .disabled(working)
            }
            if model.desktopAwaiting?.fromLabel != nil {
                Button("Put \(from) Back", action: putBack)
                    .disabled(working)
            }
            Button("I’m Signed In") {
                failure = nil
                step = .name
            }
            .keyboardShortcut(.defaultAction)
            .disabled(working)
        }
    }

    // MARK: - Naming it

    private var naming: some View {
        SheetLayout(
            title: "Name this account",
            message: "Pitboard quits Claude, records the account it is signed in to under this "
                + "name, and opens Claude again."
        ) {
            Section {
                if let again {
                    LabeledContent("Account", value: again)
                } else {
                    TextField("Name", text: $name, prompt: Text("work"))
                        .accessibilityIdentifier("sheet.name")
                        .focused($nameFocused)
                        .onSubmit(enrol)
                }
            }
            if let failure {
                SheetFailure(failure: failure)
            }
        } buttons: {
            Button("Back", role: .cancel) {
                failure = nil
                step = .signIn
            }
            .keyboardShortcut(.cancelAction)
            Button(again == nil ? "Add Account" : "Sign In Again", action: enrol)
                .keyboardShortcut(.defaultAction)
                .disabled(trimmed(name).isEmpty || working)
        }
        .onAppear { nameFocused = again == nil }
    }

    private func enrol() {
        let label = trimmed(name)
        guard !label.isEmpty else { return }
        run {
            await model.desktopEnrollAsked(label: label)
        }
    }

    private func putAside() {
        run {
            await model.desktopAddAsked()
        } then: {
            step = .signIn
        }
    }

    private func putBack() {
        run {
            await model.desktopPutBack()
        }
    }

    /// Runs one step, says its failure here, and moves on when it worked. A step that worked
    /// and finished the add has closed the sheet already.
    private func run(
        _ work: @escaping @MainActor () async -> ActionFailure?,
        then next: @escaping @MainActor () -> Void = {}
    ) {
        guard !working else { return }
        working = true
        failure = nil
        Task {
            let failed = await work()
            working = false
            if let failed {
                failure = failed
            } else {
                next()
            }
        }
    }
}
