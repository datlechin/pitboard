import PitboardLinks
import SwiftUI

/// Opens a claude.ai link, such as an artifact's, in the claude.ai window it is over, signed
/// in as that window's account.
///
/// pitboard does not read the clipboard by itself: the field starts empty, and pasting into
/// it is the person's own act.
struct OpenLinkSheet: View {
    let label: String
    /// The window's own origin, which the link is opened on.
    let home: URL
    let open: (URL) -> Void
    @State private var typed = ""
    @FocusState private var focused: Bool
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        SheetLayout(
            title: "Open a claude.ai Link as \(label)",
            message: "Paste a claude.ai link, such as an artifact’s, to open it signed in as "
                + "\(label)."
        ) {
            Section {
                TextField("Link", text: $typed, prompt: Text("https://claude.ai/…"))
                    .accessibilityIdentifier("sheet.link")
                    .focused($focused)
                    .onSubmit(openLink)
            } footer: {
                if !trimmed(typed).isEmpty, case .failure(let refusal) = checked {
                    Text(refusal.message).footnote()
                }
            }
        } buttons: {
            Button("Cancel", role: .cancel) { dismiss() }
                .keyboardShortcut(.cancelAction)
            Button("Open", action: openLink)
                .keyboardShortcut(.defaultAction)
                .disabled(link == nil)
        }
        .onAppear { focused = true }
    }

    private var checked: Result<URL, LinkRefusal> { claudeLink(from: typed, home: home) }

    private var link: URL? { try? checked.get() }

    private func openLink() {
        guard let link else { return }
        open(link)
        dismiss()
    }
}
