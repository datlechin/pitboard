import PitboardKit
import SwiftUI

/// One account in the window: its name and address, whether it is in use, each of its
/// limits drawn out, and what is worth knowing about it, as the model describes it.
struct AccountRow: View {
    let item: AccountItem
    let perform: (Intent) -> Void
    @ScaledMetric(relativeTo: .title2) private var symbolWidth: CGFloat = 26

    var body: some View {
        let textInset = symbolWidth + Design.iconSpacing
        VStack(alignment: .leading, spacing: Design.rowSpacing) {
            HStack(spacing: Design.iconSpacing) {
                Image(systemName: symbol)
                    .font(.title2)
                    .foregroundStyle(
                        item.inUse ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary)
                    )
                    .frame(width: symbolWidth)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: Design.lineSpacing) {
                    HStack(spacing: Design.iconSpacing) {
                        Text(item.title)
                            .fontWeight(.medium)
                            .lineLimit(1)
                        if let plan = item.plan {
                            PlanTag(plan: plan)
                        }
                    }
                    if !item.email.isEmpty, item.email != item.title {
                        Text(item.email)
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
                if !item.limits.isEmpty {
                    UsageBars(limits: item.limits)
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
        .accessibilityLabel(item.spoken)
        .accessibilityIdentifier("account.\(item.qualified ?? item.id)")
    }

    private var symbol: String {
        if item.unplaced { return "exclamationmark.triangle" }
        if item.needsSignIn { return Symbol.signIn }
        return item.inUse ? "person.crop.circle.fill" : Symbol.account
    }

    /// What is worth knowing beyond the limits: why it cannot be used, why its numbers are
    /// not new, which limit of the account in use runs out first at its pace, and how long a
    /// parked login stays usable.
    private var notes: [String] {
        [item.problem, item.staleNote, item.pace, item.parkedNote].compactMap { $0 }
    }

    @ViewBuilder private var trailing: some View {
        if item.switching {
            ProgressView().controlSize(.small)
                .accessibilityLabel("Switching")
        } else if item.inUse {
            Label("In Use", systemImage: "checkmark")
                .font(.callout)
                .foregroundStyle(.secondary)
        } else if let action = item.action {
            Button(action.title) { perform(action.intent) }
                .accessibilityLabel(action.spoken)
        }
    }
}

/// The plan an account's login says it is on, as a tag beside its name: "Team 5x", "Plus".
private struct PlanTag: View {
    let plan: String

    var body: some View {
        Text(plan)
            .font(.caption)
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .padding(.horizontal, 6)
            .padding(.vertical, 1)
            .background(.quaternary, in: Capsule())
            .fixedSize()
    }
}
