import PitboardKit
import SwiftUI

/// Adds an account, or signs in again to one whose parked login can no longer be used,
/// through the tool's own sign-in.
///
/// Both tools open the browser themselves and finish through their own callback, so the
/// sheet shows what the tool is doing and offers the address it printed in case the browser
/// did not open. The code field appears only for a tool that reads one, and only once it
/// asks. The sheet stays over the window while the browser is in front, which a menu could
/// not: it closed the moment the browser came forward.
struct SignInSheet: View {
    let model: AppModel
    /// The account being signed in to again, or nil for a new one.
    let again: String?
    @State private var provider: String
    @State private var name: String
    @State private var code = ""
    @State private var failure: ActionFailure?
    /// The sign-in this sheet showed last, kept while the sheet closes after it finishes,
    /// when the model has already let it go, so the sheet does not flash its form on the way
    /// out.
    @State private var shown: SigningIn?
    @FocusState private var nameFocused: Bool
    @FocusState private var codeFocused: Bool
    @Environment(\.dismiss) private var dismiss

    init(model: AppModel, provider: String?, again: String?) {
        self.model = model
        self.again = again
        // Chosen before the first frame, not after it: a picker drawn with a selection none
        // of its items has logs that it is invalid, and can draw with nothing selected.
        _provider = State(initialValue: model.provider(for: .add(provider: provider)))
        _name = State(initialValue: again ?? "")
    }

    /// The model runs one sign-in at a time, so a sign-in sheet shows the one running,
    /// whichever sheet started it.
    var body: some View {
        Group {
            if let signingIn = model.signingIn ?? shown {
                progress(signingIn)
            } else {
                form
            }
        }
        .onChange(of: model.signingIn.map(ObjectIdentifier.init), initial: true) {
            if let running = model.signingIn { shown = running }
        }
    }

    // MARK: - Before it starts

    private var form: some View {
        SheetLayout(
            title: again.map { "Sign In to \($0) Again" } ?? "Add Account",
            message: again == nil
                ? "Pitboard opens \(toolName)’s own sign-in in your browser. Sign in as the "
                    + "account you’re adding, and Pitboard parks its login beside the one "
                    + "in use."
                : "Pitboard opens \(toolName)’s own sign-in in your browser. Sign in as "
                    + "\(again ?? "") to give Pitboard a new login for it."
        ) {
            Section {
                if again == nil, model.addable.count > 1 {
                    Picker("Tool", selection: $provider) {
                        ForEach(model.addable, id: \.code) { tool in
                            Text(tool.name).tag(tool.code)
                        }
                    }
                    .accessibilityIdentifier("sheet.tool")
                    // Claude Desktop signs in inside Claude, which its own sheet walks
                    // through.
                    .onChange(of: provider) {
                        if provider == desktopProvider {
                            model.present(.add(provider: desktopProvider))
                        }
                    }
                }
                if let again {
                    LabeledContent("Account", value: again)
                } else {
                    TextField("Name", text: $name, prompt: Text("work"))
                        .accessibilityIdentifier("sheet.name")
                        .focused($nameFocused)
                        .onSubmit(start)
                }
            } footer: {
                if again == nil, let missing = model.notOffered {
                    // Said rather than left out without a word, which read as Pitboard not
                    // handling the tool at all.
                    Text(missing).footnote()
                }
            }
            if let failure {
                SheetFailure(failure: failure)
            }
        } buttons: {
            Button("Cancel", role: .cancel) { dismiss() }
                .keyboardShortcut(.cancelAction)
            Button("Sign In", action: start)
                .keyboardShortcut(.defaultAction)
                .disabled(trimmed(name).isEmpty || model.signingIn != nil)
        }
        .onAppear { nameFocused = again == nil }
    }

    private func start() {
        let name = trimmed(name)
        guard !name.isEmpty, model.signingIn == nil, shown == nil else { return }
        failure = nil
        Task {
            let failed = await model.signIn(name, for: provider)
            // Back to the form, with what went wrong and the name still in it. A sign-in
            // that finished closes the sheet, which keeps showing it on the way out.
            if let failed {
                failure = failed
                shown = nil
            }
        }
    }

    // MARK: - While it runs

    private func progress(_ signingIn: SigningIn) -> some View {
        SheetLayout(
            title: "Signing In to \(signingIn.tool)",
            message: "Finish signing in as \(signingIn.label) in your browser. This closes "
                + "once \(signingIn.tool) says you’re in."
        ) {
            Section {
                LabeledContent {
                    ProgressView().controlSize(.small)
                } label: {
                    Text("Waiting for your browser")
                }
                if let url = signingIn.url {
                    LabeledContent("Browser didn’t open?") {
                        Link("Open Sign-In Page", destination: url)
                            .help(url.absoluteString)
                    }
                }
            }
            if signingIn.wantsCode {
                Section {
                    TextField(
                        "Code", text: $code, prompt: Text("Paste the code from your browser")
                    )
                    .accessibilityIdentifier("sheet.code")
                    .focused($codeFocused)
                    .onSubmit(send)
                    // Appears once the tool asks, with the code on the clipboard to paste.
                    .onAppear { codeFocused = true }
                } footer: {
                    Text(
                        "\(signingIn.tool) takes the code shown after you sign in, whether or "
                            + "not your browser came back to it."
                    )
                    .footnote()
                }
            }
        } buttons: {
            Button("Cancel", role: .cancel) {
                model.cancelSignIn()
                shown = nil
                dismiss()
            }
            .keyboardShortcut(.cancelAction)
            if signingIn.wantsCode {
                Button("Submit Code", action: send)
                    .keyboardShortcut(.defaultAction)
                    .disabled(trimmed(code).isEmpty)
            }
        }
    }

    private func send() {
        let typed = trimmed(code)
        guard !typed.isEmpty else { return }
        model.paste(typed)
        code = ""
    }

    private var toolName: String { model.tool(provider)?.name ?? provider }
}
