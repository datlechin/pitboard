import PitboardKit
import SwiftUI

struct AccountInspector: View {
    let item: AccountItem?
    let act: (AccountAction) -> Void
    let choose: (Intent) -> Void

    var body: some View {
        if let item {
            Form {
                Section {
                    ForEach(Array(item.limits.enumerated()), id: \.offset) { _, limit in
                        LimitDetail(limit: limit)
                    }
                } header: {
                    Header(item: item)
                } footer: {
                    footnotes(of: item)
                }
                Section {
                    values(of: item)
                } footer: {
                    buttons(of: item)
                }
            }
            .formStyle(.grouped)
            .accessibilityIdentifier("inspector")
        } else {
            ContentUnavailableView(words.window.noSelection ?? "", systemImage: Symbol.account)
                .accessibilityIdentifier("inspector")
        }
    }

    // Only what the values below do not say: old numbers, or a login Pitboard cannot use.
    @ViewBuilder private func footnotes(of item: AccountItem) -> some View {
        let notes = item.notes.filter {
            $0.kind == .stale || ($0.kind == .problem && !item.needsSignIn)
        }
        if !notes.isEmpty || item.detail.read != nil {
            VStack(alignment: .leading, spacing: Design.lineSpacing) {
                ForEach(notes, id: \.text) { note in
                    Text(note.text).explanatory()
                    if let learn = note.learn, let link = URL(string: learn.link) {
                        Link(learn.title, destination: link).font(.callout)
                    }
                }
                if let read = item.detail.read {
                    Text(read).explanatory()
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    @ViewBuilder private func values(of item: AccountItem) -> some View {
        let detail = item.detail
        if let email = detail.email {
            LabeledContent(words.window.email) {
                Text(email)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
                    .accessibilityIdentifier("inspector.email")
            }
        }
        if let signIn = detail.signIn {
            LabeledContent(words.window.signIn, value: signIn)
        }
        if let warm = detail.warm {
            LabeledContent {
                Button(warm.settings.title) { choose(warm.settings.intent) }
                    .disabled(!warm.settings.enabled)
                    .accessibilityIdentifier("inspector.warmUpSettings")
            } label: {
                Text(words.window.warmUp)
                Text(warm.said)
            }
        }
    }

    // What its shortcut menu offers, in its order.
    private func buttons(of item: AccountItem) -> some View {
        VStack(spacing: Design.rowSpacing) {
            ForEach(AccountMenuItems.shown(item.actions, in: .shortcutMenu), id: \.title) {
                action in
                Button {
                    act(action)
                } label: {
                    Text(action.title).frame(maxWidth: .infinity)
                }
                .accessibilityIdentifier("inspector.action.\(caseName(action.kind))")
            }
        }
        .frame(maxWidth: .infinity)
    }
}

private struct Header: View {
    let item: AccountItem

    var body: some View {
        HStack(spacing: Design.iconSpacing) {
            Image(systemName: item.standing.symbol)
                .font(.largeTitle)
                .foregroundStyle(item.inUse ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: Design.lineSpacing) {
                Text(item.title)
                    .font(.title3)
                    .fontWeight(.semibold)
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                    .accessibilityAddTraits(.isHeader)
                if let plan = item.plan {
                    Text(plan)
                        .font(.body)
                        .fontWeight(.regular)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .padding(.bottom, Design.rowSpacing)
    }
}

private struct LimitDetail: View {
    let limit: LimitRow

    var body: some View {
        VStack(alignment: .leading, spacing: Design.rowSpacing) {
            HStack(alignment: .firstTextBaseline) {
                Text(limit.name)
                Spacer()
                Text(limit.used)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            Gauge(value: min(max(limit.percent, 0), 100), in: 0...100) {
                EmptyView()
            }
            .gaugeStyle(.linearCapacity)
            .tint(limit.level.tint)
            HStack(alignment: .firstTextBaseline) {
                Text(limit.resets)
                Spacer()
                Text(limit.resetsIn)
            }
            .font(.callout)
            .foregroundStyle(.secondary)
            .monospacedDigit()
        }
        .padding(.vertical, Design.lineSpacing)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(limit.spoken)
    }
}
