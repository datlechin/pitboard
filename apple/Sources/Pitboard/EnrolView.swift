import PitboardKit
import SwiftUI

/// Asks for the name to enrol an account under, and for a new one, which tool it is for.
struct NameIt: View {
    let model: AppModel
    let asking: AppModel.Naming
    @State private var typed = ""
    /// The tool a new account is for, as a `Tool`'s code.
    @State private var provider: String
    @FocusState private var focused: Bool

    /// The tool is chosen before the first frame, not after it: a segmented picker drawn
    /// with a selection none of its segments has logs that it is invalid, and can draw with
    /// nothing selected.
    init(model: AppModel, asking: AppModel.Naming) {
        self.model = model
        self.asking = asking
        _provider = State(initialValue: model.provider(for: asking))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            // Only where there is a choice: with one tool the form is what it always was.
            if case .another = asking, model.addable.count > 1 {
                Picker("Tool", selection: $provider) {
                    ForEach(model.addable, id: \.code) { tool in
                        Text(tool.name).tag(tool.code)
                    }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            }
            HStack(spacing: 6) {
                TextField("a name for this account", text: $typed)
                    .textFieldStyle(.roundedBorder)
                    .focused($focused)
                    .onSubmit { go() }
                Button(isNew ? "Sign in" : "Enrol", action: go)
                    .disabled(typed.trimmingCharacters(in: .whitespaces).isEmpty)
                Button("Cancel") { model.naming = nil }
            }
            if isNew {
                Text(
                    "Opens \(toolName)'s own sign-in in your browser, and shows what it "
                        + "says here. Sign in as the account you are adding."
                )
                .font(.caption2)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                // Said rather than left out without a word, which read as pitboard not
                // handling the tool at all.
                if let missing = model.notOffered {
                    Text(missing)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .onAppear { focused = true }
        // Again whenever the question changes under a form already showing: a card's button
        // pressed while the form is open asks about a different tool.
        .onChange(of: asking) { provider = model.provider(for: asking) }
    }

    private var isNew: Bool {
        if case .another = asking { return true }
        return false
    }

    private var toolName: String { model.tool(provider)?.name ?? provider }

    private func go() {
        let name = typed.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty, !provider.isEmpty else { return }
        if isNew {
            Task { await model.signIn(name, for: provider) }
        } else {
            Task { await model.enrol(name, for: provider) }
        }
    }
}

/// A sign-in in progress. Both tools open the browser themselves and finish through their
/// own callback, so this shows what the tool is doing and offers the address it printed in
/// case the browser did not open. The code field appears only for a tool that reads one,
/// and only once it asks.
struct SigningInView: View {
    let model: AppModel
    let signingIn: SigningIn
    @State private var code = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                ProgressView().controlSize(.small)
                // Wraps rather than truncates: what a long label loses is the name being
                // signed in.
                Text("Signing in to \(signingIn.tool) as \(signingIn.label)…")
                    .font(.callout)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer()
                Button("Cancel") { model.cancelSignIn() }
            }
            if let url = signingIn.url {
                Link("Open the sign-in page", destination: url)
                    .font(.caption)
                    .help(url.absoluteString)
                Text("\(signingIn.tool) opens it in your browser; this is for when it did not.")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if signingIn.wantsCode {
                HStack(spacing: 6) {
                    TextField("paste the code from the browser", text: $code)
                        .textFieldStyle(.roundedBorder)
                        .onSubmit { send() }
                    Button("Send", action: send).disabled(code.isEmpty)
                }
                Text(
                    "\(signingIn.tool) asks for this only when the browser could not reach it."
                )
                .font(.caption2)
                .foregroundStyle(.secondary)
            }
        }
    }

    private func send() {
        model.paste(code.trimmingCharacters(in: .whitespaces))
        code = ""
    }
}

/// What doctor found, in the panel, so a machine-level problem does not have to be chased
/// from a terminal.
