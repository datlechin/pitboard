import AppKit
import PitboardKit
import SwiftUI

// Stays over the window while the browser or Claude is in front, which a menu could not.
struct SignInSheet: View {
    let model: AppModel
    let sheet: Sheet
    @State private var provider: String
    @State private var code = ""
    // Kept while the sheet closes after the sign-in, so the form does not flash on the way out.
    @State private var shown: (signIn: RunningSignIn, text: SigningInText)?
    @FocusState private var codeFocused: Bool
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL

    init(model: AppModel, sheet: Sheet) {
        self.model = model
        self.sheet = sheet
        // A picker drawn with a selection none of its items has can draw with none selected.
        _provider = State(initialValue: model.sheetText?.tool ?? "")
    }

    var body: some View {
        Group {
            if let signingIn = model.signingIn, let text = model.signingInText {
                progress(signingIn, text)
            } else if let shown {
                progress(shown.signIn, shown.text)
            } else {
                form
            }
        }
        .onChange(of: model.signingIn, initial: true) { _, running in
            if let running, let text = model.signingInText {
                shown = (running, text)
            } else if model.sheet != nil {
                // It failed or was stopped, and the sheet stays: back to the form.
                shown = nil
            }
        }
        // VoiceOver does not read what appears by itself.
        .onChange(of: model.signingInText?.announce, initial: true) { _, said in
            if let said { AccessibilityNotification.Announcement(said).post() }
        }
    }

    // MARK: - Before it starts

    @ViewBuilder private var form: some View {
        let text = model.sheetText
        let tools = text?.tools ?? []
        let chosen = tools.first { $0.code == provider }
        let busy = text?.saving ?? false
        let start = checkName(sheet: sheet, tool: provider, typed: "").intent
        Form {
            Section {
                if tools.count > 1 {
                    Picker(words.sheet.tool, selection: $provider) {
                        ForEach(tools, id: \.code) { tool in
                            Text(tool.name).tag(tool.code)
                        }
                    }
                    .accessibilityIdentifier("sheet.tool")
                    .disabled(busy)
                }
                if let account = text?.account {
                    LabeledContent(words.sheet.account, value: account)
                }
                if let working = text?.progress {
                    WaitingRow(text: working)
                }
            } header: {
                SheetHeader(
                    title: text?.title ?? "", message: chosen?.message ?? text?.message ?? "")
            }
            if let steps = chosen?.steps, !steps.isEmpty {
                Section { Steps(steps: steps) }
            }
            if let missing = chosen?.missing {
                ToolMissing(missing: missing)
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
                Button(chosen?.confirm ?? text?.confirm ?? "", action: signIn)
                    .keyboardShortcut(.defaultAction)
                    .disabled(
                        start == nil || chosen?.missing != nil || model.signingIn != nil || busy
                    )
                    .accessibilityIdentifier("sheet.confirm")
            }
        }
        .onChange(of: provider) { edited() }
        // Back from installing a tool Pitboard said it could not find.
        .onReceive(
            NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)
        ) { _ in model.send(.lookForTools) }
        .interactiveDismissDisabled(busy)
    }

    private func edited() {
        if model.sheetFailure != nil { model.send(.dismissSheetFailure) }
    }

    private func signIn() {
        let chosen = model.sheetText?.tools.first { $0.code == provider }
        guard let intent = checkName(sheet: sheet, tool: provider, typed: "").intent,
            chosen?.missing == nil, model.signingIn == nil, shown == nil,
            model.sheetText?.saving != true
        else { return }
        model.send(intent)
    }

    // MARK: - While it runs

    private func progress(_ signingIn: RunningSignIn, _ text: SigningInText) -> some View {
        let page = signingIn.url.flatMap(URL.init(string:))
        return Form {
            Section {
                if text.steps.isEmpty {
                    WaitingRow(text: text.message, spoken: text.waiting)
                } else {
                    Steps(steps: text.steps)
                }
                if text.offersLink {
                    LabeledContent(words.sheet.signInPage) {
                        HStack {
                            Button(words.sheet.copyLink) {
                                if let page { NSPasteboard.general.copy(link: page) }
                            }
                            .accessibilityIdentifier("sheet.copyLink")
                            Button(words.sheet.open) {
                                if let page { openURL(page) }
                            }
                            .help(page?.absoluteString ?? "")
                            .accessibilityIdentifier("sheet.openLink")
                        }
                        // Dimmed until the tool prints the address, so the sheet keeps its size.
                        .disabled(page == nil)
                    }
                }
            } header: {
                SheetHeader(title: text.title)
            } footer: {
                // Beside the steps, the message is a caution and not what the spinner waits on.
                if !text.steps.isEmpty {
                    VStack(alignment: .leading, spacing: Design.lineSpacing) {
                        WaitingRow(text: text.waiting)
                        Text(text.message).footnote()
                    }
                }
            }
            if signingIn.wantsCode {
                Section {
                    TextField(
                        words.sheet.code, text: $code, prompt: Text(words.sheet.pasteCode)
                    )
                    .accessibilityIdentifier("sheet.code")
                    .focused($codeFocused)
                    .onSubmit(send)
                    // Appears once the tool asks, with the code on the clipboard to paste.
                    .onAppear { codeFocused = true }
                    if let refused = text.refused {
                        Text(refused).explanatory()
                    }
                } footer: {
                    Text(text.codeNote).footnote()
                }
            }
        }
        .sheetForm()
        .toolbar {
            if let undo = text.undo {
                ToolbarItem(placement: .automatic) {
                    Button(undo.title) { model.send(undo.intent) }
                        .disabled(!undo.enabled)
                        .accessibilityIdentifier("sheet.undo")
                }
            }
            ToolbarItem(placement: .cancellationAction) {
                Button(text.cancel.title, role: text.escapeCancels ? .cancel : nil) {
                    model.send(text.cancel.intent)
                    shown = nil
                    dismiss()
                }
                .keyboardShortcut(text.escapeCancels ? .cancelAction : nil)
                .disabled(!text.cancel.enabled)
                .accessibilityIdentifier("sheet.cancel")
            }
            if signingIn.wantsCode {
                ToolbarItem(placement: .confirmationAction) {
                    Button(words.sheet.submitCode, action: send)
                        .keyboardShortcut(.defaultAction)
                        .disabled(code.isEmpty)
                        .accessibilityIdentifier("sheet.submitCode")
                }
            }
        }
        // Escape is the Cancel button's, which stops the sign-in, where the model says so.
        .interactiveDismissDisabled(true)
    }

    private func send() {
        guard !code.isEmpty else { return }
        model.send(.pasteCode(code: code))
        code = ""
    }
}

extension NSPasteboard {
    // As text too, for a browser's address bar: an `NSURL` alone gives only the address.
    func copy(link: URL) {
        let item = NSPasteboardItem()
        item.setString(link.absoluteString, forType: .URL)
        item.setString(link.absoluteString, forType: .string)
        clearContents()
        writeObjects([item])
    }
}
