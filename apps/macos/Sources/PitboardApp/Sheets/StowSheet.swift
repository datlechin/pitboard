import PitboardKit
import SwiftUI

/// Puts away the login Claude Code left in a file behind the keychain, once the model has
/// looked at the file and said whose the login is.
///
/// What it says, whether Put Away can be pressed and what it puts away are the model's: it
/// puts away the file as the model looked at it, and only while the file still holds that.
struct StowSheet: View {
    let model: AppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let text = model.stowText
        // Putting it away cannot be withdrawn, so there is nothing to cancel.
        let busy = text?.saving ?? false
        SheetLayout(title: text?.title ?? "", message: text?.message ?? "") {
            Section {
                if let looking = text?.looking {
                    ProgressView(looking)
                        .controlSize(.small)
                        .accessibilityIdentifier("sheet.stowLooking")
                }
                ForEach(text?.lines ?? [], id: \.self) { line in
                    Text(line)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            if let failure = model.sheetFailure {
                SheetFailure(failure: failure)
            }
        } buttons: {
            Button("Cancel", role: .cancel) { dismiss() }
                .keyboardShortcut(.cancelAction)
                .disabled(busy)
            Button(text?.confirm ?? "", role: .destructive) { model.send(.stow) }
                .keyboardShortcut(.defaultAction)
                .disabled(!(text?.canConfirm ?? false))
                .accessibilityIdentifier("sheet.stow")
        }
        .interactiveDismissDisabled(busy)
    }
}
