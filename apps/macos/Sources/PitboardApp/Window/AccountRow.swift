import PitboardKit
import SwiftUI

// Its email, sign-in and warm-up are the inspector's.
struct AccountRow: View {
    let item: AccountItem
    let perform: (Intent) -> Void
    @ScaledMetric(relativeTo: .title2) private var symbolWidth: CGFloat = 26

    var body: some View {
        let textInset = symbolWidth + Design.iconSpacing
        VStack(alignment: .leading, spacing: Design.rowSpacing) {
            HStack(spacing: Design.iconSpacing) {
                Image(systemName: item.standing.symbol)
                    .font(.title2)
                    .foregroundStyle(
                        item.inUse ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary)
                    )
                    .frame(width: symbolWidth)
                    .accessibilityHidden(true)
                HStack(alignment: .firstTextBaseline, spacing: Design.iconSpacing) {
                    Text(item.title)
                        .fontWeight(.medium)
                        .lineLimit(1)
                    if let plan = item.plan {
                        Text(plan)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
                Spacer(minLength: Design.iconSpacing)
                trailing
            }
            if !item.limits.isEmpty {
                UsageBars(limits: item.limits)
                    .padding(.leading, textInset)
            }
            // The most pressing; the inspector says the rest.
            if let note = item.notes.first {
                Text(note.text)
                    .explanatory()
                    .padding(.leading, textInset)
                    .accessibilityIdentifier("account.note")
            }
        }
        .padding(.vertical, 4)
        // Under the name, where a list would line it up with the row's last label.
        .alignmentGuide(.listRowSeparatorLeading) { _ in textInset }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(item.spoken)
        .accessibilityIdentifier("account.\(item.qualified ?? item.id)")
    }

    @ViewBuilder private var trailing: some View {
        switch item.end {
        case .switching(let spoken)?:
            ProgressView().controlSize(.small)
                .accessibilityLabel(spoken)
                .accessibilityIdentifier("account.switching")
        case .inUse(let title)?:
            Label(title, systemImage: "checkmark")
                .font(.callout)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("account.inUse")
        case .button(let action)?:
            Button(action.title) { perform(action.intent) }
                .accessibilityLabel(action.spoken)
                .accessibilityIdentifier("account.action")
        case nil:
            EmptyView()
        }
    }
}

extension AccountStanding {
    var symbol: String {
        switch self {
        case .inUse: "person.crop.circle.fill"
        case .ready, .expiring, .switching: Symbol.account
        case .needsSignIn: Symbol.signIn
        case .notAdded: "person.crop.circle.badge.plus"
        case .unusable: Symbol.warning
        }
    }
}
