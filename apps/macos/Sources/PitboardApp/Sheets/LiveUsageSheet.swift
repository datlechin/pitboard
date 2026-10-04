import PitboardKit
import SwiftUI

/// Says what turning on live usage for Claude Desktop reads, and why macOS asks for a
/// password, before anything is read. Its Continue is the one thing in the app that turns
/// live usage on: reading Claude's key is what raises the keychain's prompt, and a prompt
/// nobody asked for is one somebody answers without reading.
struct LiveUsageSheet: View {
    let model: AppModel
    @State private var failure: ActionFailure?
    @State private var working = false
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        SheetLayout(
            title: "Show live usage for Claude Desktop?",
            message: "Claude encrypts its sign-in with a key in your login keychain. To ask "
                + "claude.ai how much each account has left, Pitboard reads that key. macOS "
                + "asks for your login password once: choose Always Allow so it never asks "
                + "again. Always Allow lets any program that runs /usr/bin/security read this "
                + "key. Pitboard only reads it, keeps it in memory, and never writes it. "
                + "Switching accounts never needs it."
        ) {
            if let failure {
                SheetFailure(failure: failure)
            }
        } buttons: {
            Button("Not Now", role: .cancel) { dismiss() }
                .keyboardShortcut(.cancelAction)
            Button("Continue", action: enable)
                .keyboardShortcut(.defaultAction)
                .disabled(working)
        }
    }

    private func enable() {
        guard !working else { return }
        working = true
        failure = nil
        Task {
            // Closes the sheet itself when it works.
            failure = await model.liveUsageEnableAsked()
            working = false
        }
    }
}
