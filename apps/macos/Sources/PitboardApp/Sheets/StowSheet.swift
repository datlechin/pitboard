import PitboardKit
import SwiftUI

// Cleans up the file only while it still holds what the model's look found.
struct StowSheet: View {
    let model: AppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let text = model.stowText
        // Cleaning up cannot be withdrawn.
        let busy = text?.saving ?? false
        Form {
            Section {
                if let looking = text?.looking {
                    WaitingRow(text: looking)
                        .accessibilityIdentifier("sheet.stowLooking")
                }
            } header: {
                SheetHeader(title: text?.title ?? "", message: text?.message ?? "")
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
            // Chosen on purpose, so not drawn as destructive.
            ToolbarItem(placement: .confirmationAction) {
                Button(text?.confirm ?? "") { model.send(.stow) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!(text?.canConfirm ?? false))
                    .accessibilityIdentifier("sheet.stow")
            }
        }
        .interactiveDismissDisabled(busy)
    }
}
