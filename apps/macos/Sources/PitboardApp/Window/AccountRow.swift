import PitboardKit
import SwiftUI

/// One account in the window: its name and address, whether it is in use, each of its
/// limits drawn out, and what is worth knowing about it.
struct AccountRow: View {
    let account: Account
    let description: AccountDescription
    /// Its name as said where nothing around it says which tool it is for: VoiceOver reads a
    /// row apart from the heading above it, and Voice Control's "Use work" has to name one
    /// account when two tools each have a `work`.
    let spokenName: String
    let perform: (AccountAction) -> Void
    @ScaledMetric(relativeTo: .title2) private var symbolWidth: CGFloat = 26

    var body: some View {
        let textInset = symbolWidth + Design.iconSpacing
        VStack(alignment: .leading, spacing: Design.rowSpacing) {
            HStack(spacing: Design.iconSpacing) {
                Image(systemName: symbol)
                    .font(.title2)
                    .foregroundStyle(
                        description.inUse ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary)
                    )
                    .frame(width: symbolWidth)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: Design.lineSpacing) {
                    Text(description.title)
                        .fontWeight(.medium)
                        .lineLimit(1)
                    if !description.email.isEmpty, description.email != description.title {
                        Text(description.email)
                            .font(.callout)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                }
                Spacer(minLength: Design.iconSpacing)
                trailing
            }
            VStack(alignment: .leading, spacing: Design.rowSpacing) {
                if !description.limits.isEmpty {
                    UsageBars(limits: description.limits)
                }
                ForEach(notes, id: \.self) { note in
                    Text(note).explanatory()
                }
            }
            .padding(.leading, textInset)
        }
        .padding(.vertical, 4)
        // The line between rows starts under the name, where the text starts, whatever the
        // row ends with: a list lines it up with a row's last label otherwise, which for the
        // account in use is "In Use" at the far end.
        .alignmentGuide(.listRowSeparatorLeading) { _ in textInset }
        // One account, read as one thing with its controls in it, rather than a stop for
        // every line on the way past.
        .accessibilityElement(children: .contain)
        .accessibilityLabel(spokenLabel)
        .accessibilityIdentifier("account.\(account.qualified ?? account.id)")
    }

    private var symbol: String {
        if account.unplaced { return "exclamationmark.triangle" }
        if description.needsSignIn { return Symbol.signIn }
        return description.inUse ? "person.crop.circle.fill" : Symbol.account
    }

    /// What is worth knowing beyond the limits: why it cannot be used, why its numbers are
    /// not new, how long the account in use lasts at this rate, and how long a parked login
    /// stays usable.
    private var notes: [String] {
        [
            description.problem, description.staleNote, description.pace,
            description.parkedNote, description.sourceNote,
        ]
        .compactMap { $0 }
    }

    @ViewBuilder private var trailing: some View {
        if description.switching {
            ProgressView().controlSize(.small)
                .accessibilityLabel("Switching")
        } else if description.inUse {
            Label("In Use", systemImage: "checkmark")
                .font(.callout)
                .foregroundStyle(.secondary)
        } else {
            switch description.action {
            case .use:
                Button("Use") { perform(description.action) }
                    .accessibilityLabel("Use \(spokenName)")
            case .signInAgain:
                Button("Sign In Again…") { perform(description.action) }
                    .accessibilityLabel("Sign In to \(spokenName) Again…")
            case .name:
                Button("Name…") { perform(description.action) }
                    .accessibilityLabel("Name \(spokenName)…")
            case .none:
                EmptyView()
            }
        }
    }

    private var spokenLabel: String {
        var parts = [spokenName]
        if description.inUse { parts.append("in use") }
        if description.needsSignIn { parts.append("needs signing in again") }
        return parts.joined(separator: ", ")
    }
}
