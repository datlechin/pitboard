import PitboardKit
import SwiftUI

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
                    if let line = item.emailLine {
                        Text(line)
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
                ForEach(item.notes, id: \.self) { note in
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
        if item.unplaced { return Symbol.warning }
        if item.needsSignIn { return Symbol.signIn }
        return item.inUse ? "person.crop.circle.fill" : Symbol.account
    }

    @ViewBuilder private var trailing: some View {
        switch item.end {
        case .switching(let spoken)?:
            ProgressView().controlSize(.small)
                .accessibilityLabel(spoken)
        case .inUse(let title)?:
            Label(title, systemImage: "checkmark")
                .font(.callout)
                .foregroundStyle(.secondary)
        case .button(let action)?:
            Button(action.title) { perform(action.intent) }
                .accessibilityLabel(action.spoken)
        case nil:
            EmptyView()
        }
    }
}

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
