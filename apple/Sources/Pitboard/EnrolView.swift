import PitboardKit
import SwiftUI

struct NameIt: View {
    let model: AppModel
    let asking: AppModel.Naming
    @State private var typed = ""
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                TextField("a name for this account", text: $typed)
                    .textFieldStyle(.roundedBorder)
                    .focused($focused)
                    .onSubmit { go() }
                Button(asking == .theOneInUse ? "Enrol" : "Sign in", action: go)
                    .disabled(typed.trimmingCharacters(in: .whitespaces).isEmpty)
                Button("Cancel") { model.naming = nil }
            }
            if asking == .another {
                Text(
                    "Opens Claude Code's own sign-in in your browser, and shows what it "
                        + "says here. Sign in as the account you are adding."
                )
                .font(.caption2)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            }
        }
        .onAppear { focused = true }
    }

    private func go() {
        let label = typed.trimmingCharacters(in: .whitespaces)
        guard !label.isEmpty else { return }
        switch asking {
        case .theOneInUse: Task { await model.enrol(as: label) }
        case .another: Task { await model.signIn(as: label) }
        }
    }
}

/// A sign-in in progress. Claude Code opens the browser itself and finishes through its own
/// callback, so this shows what it is doing and offers the address if the browser did not
/// open. The code field appears only when Claude Code asks for one.
struct SigningInView: View {
    let model: AppModel
    let signingIn: SigningIn
    @State private var code = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                ProgressView().controlSize(.small)
                Text("Signing in as \(signingIn.label)…").font(.callout)
                Spacer()
                Button("Cancel") { model.cancelSignIn() }
            }
            if let url = signingIn.url {
                Link("Open the sign-in page", destination: url).font(.caption)
            }
            if signingIn.wantsCode {
                HStack(spacing: 6) {
                    TextField("paste the code from the browser", text: $code)
                        .textFieldStyle(.roundedBorder)
                        .onSubmit { send() }
                    Button("Send", action: send).disabled(code.isEmpty)
                }
                Text("Claude Code asks for this only when the browser could not reach it.")
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
