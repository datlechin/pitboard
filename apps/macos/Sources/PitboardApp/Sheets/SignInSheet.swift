import AppKit
import PitboardKit
import SwiftUI

/// Adds an account, or signs in again to one whose parked login can no longer be used,
/// through the tool's own sign-in, which the model runs.
///
/// Claude Code and Codex open the default browser themselves and finish through their own
/// callback, so the sheet offers the address each prints: to open where no browser opened,
/// or to copy into another browser or browser profile. The code field appears only for a
/// tool that reads one, only once it asks, and again once Claude Code has refused the code
/// typed back. Claude Desktop signs in inside Claude, and the sheet says each step of it.
/// The sheet stays over the window while the browser or Claude is in front, which a menu
/// could not: it closed the moment the browser came forward.
struct SignInSheet: View {
    let model: AppModel
    let sheet: Sheet
    @State private var provider: String
    @State private var name: String
    @State private var code = ""
    /// The sign-in this sheet showed last, and what it said, kept while the sheet closes
    /// after it finishes, when the model has already let it go, so the sheet does not flash
    /// its form on the way out.
    @State private var shown: (signIn: RunningSignIn, text: SigningInText)?
    @FocusState private var nameFocused: Bool
    @FocusState private var codeFocused: Bool
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL

    init(model: AppModel, sheet: Sheet) {
        self.model = model
        self.sheet = sheet
        // Chosen before the first frame, not after it: a picker drawn with a selection none
        // of its items has logs that it is invalid, and can draw with nothing selected.
        _provider = State(initialValue: model.sheetText?.tool ?? "")
        _name = State(initialValue: model.sheetText?.name ?? "")
    }

    /// The model runs one sign-in at a time, so a sign-in sheet shows the one running,
    /// whichever sheet started it.
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
                // Over, and the sheet stays: it failed, or it was stopped. Back to the form,
                // with what went wrong and the name still in it.
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
        let check = checkName(sheet: sheet, tool: provider, typed: name)
        SheetLayout(
            title: text?.title ?? "",
            message: chosen?.message ?? text?.message ?? ""
        ) {
            Section {
                if tools.count > 1 {
                    Picker("Tool", selection: $provider) {
                        ForEach(tools, id: \.code) { tool in
                            Text(tool.name).tag(tool.code)
                        }
                    }
                    .accessibilityIdentifier("sheet.tool")
                    .disabled(busy)
                }
                if let account = text?.account {
                    LabeledContent("Account", value: account)
                } else {
                    TextField("Name", text: $name, prompt: Text(text?.prompt ?? ""))
                        .accessibilityIdentifier("sheet.name")
                        .focused($nameFocused)
                        .onSubmit(start)
                        .disabled(busy)
                }
                if let working = text?.progress {
                    ProgressView(working)
                        .controlSize(.small)
                }
            } footer: {
                if let problem = check.problem {
                    NameProblem(problem: problem)
                }
            }
            if let missing = chosen?.missing {
                ToolMissing(missing: missing)
            }
            if let failure = model.sheetFailure {
                SheetFailure(failure: failure)
            }
        } buttons: {
            Button("Cancel", role: .cancel) { dismiss() }
                .keyboardShortcut(.cancelAction)
                .disabled(busy)
                .accessibilityIdentifier("sheet.cancel")
            Button(chosen?.confirm ?? text?.confirm ?? "", action: start)
                .keyboardShortcut(.defaultAction)
                .disabled(
                    check.intent == nil || chosen?.missing != nil || model.signingIn != nil
                        || busy
                )
                .accessibilityIdentifier("sheet.confirm")
        }
        .onAppear { nameFocused = text?.account == nil }
        .onChange(of: name) { edited() }
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

    private func start() {
        let chosen = model.sheetText?.tools.first { $0.code == provider }
        guard let intent = checkName(sheet: sheet, tool: provider, typed: name).intent,
            chosen?.missing == nil, model.signingIn == nil, shown == nil,
            model.sheetText?.saving != true
        else { return }
        model.send(intent)
    }

    // MARK: - While it runs

    private func progress(_ signingIn: RunningSignIn, _ text: SigningInText) -> some View {
        let page = signingIn.url.flatMap(URL.init(string:))
        return SheetLayout(title: text.title, message: text.message) {
            if text.offersLink {
                Section {
                    LabeledContent("Sign-In Page") {
                        HStack {
                            Button("Copy Link") {
                                if let page { NSPasteboard.general.copy(link: page) }
                            }
                            .accessibilityIdentifier("sheet.copyLink")
                            Button("Open") {
                                if let page { openURL(page) }
                            }
                            .help(page?.absoluteString ?? "")
                            .accessibilityIdentifier("sheet.openLink")
                        }
                        // Dimmed rather than left out until the tool prints the address, so
                        // the sheet does not grow a moment after it opens.
                        .disabled(page == nil)
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
                    if let refused = text.refused {
                        Text(refused).explanatory()
                    }
                } footer: {
                    Text(text.codeNote).footnote()
                }
            }
        } status: {
            // The message says what it waits for, so the spinner goes unlabelled, as the
            // HIG asks of one on a Mac. VoiceOver still names it.
            ProgressView()
                .controlSize(.small)
                .accessibilityLabel(text.waiting)
        } buttons: {
            Button(text.cancel.title, role: text.escapeCancels ? .cancel : nil) {
                model.send(text.cancel.intent)
                shown = nil
                dismiss()
            }
            .keyboardShortcut(text.escapeCancels ? .cancelAction : nil)
            .disabled(!text.cancel.enabled)
            .accessibilityIdentifier("sheet.cancel")
            if let undo = text.undo {
                Button(undo.title) { model.send(undo.intent) }
                    .disabled(!undo.enabled)
                    .accessibilityIdentifier("sheet.undo")
            }
            if signingIn.wantsCode {
                Button("Submit Code", action: send)
                    .keyboardShortcut(.defaultAction)
                    .disabled(code.isEmpty)
                    .accessibilityIdentifier("sheet.submitCode")
            }
        }
        .interactiveDismissDisabled(!text.escapeCancels)
    }

    /// The model takes the code without the white space around it, and types it back only
    /// where the tool asks for one.
    private func send() {
        guard !code.isEmpty else { return }
        model.send(.pasteCode(code: code))
        code = ""
    }
}

extension NSPasteboard {
    /// Puts `link` here in place of whatever was here, as an address for an app that reads
    /// one and as text for a field that reads only text, such as a browser's address bar.
    /// An `NSURL` written alone gives only the address.
    func copy(link: URL) {
        let item = NSPasteboardItem()
        item.setString(link.absoluteString, forType: .URL)
        item.setString(link.absoluteString, forType: .string)
        clearContents()
        writeObjects([item])
    }
}
