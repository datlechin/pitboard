import PitboardKit
import SwiftUI

// Above the accounts, so the list holds accounts only.
struct NoticeBanner: View {
    let notice: PanelNotice
    let perform: (NoticeAction) -> Void

    var body: some View {
        let buttons = notice.actions.filter { !$0.dismisses }
        Banner(
            symbol: notice.severity.symbol, tint: notice.severity.tint,
            spoken: notice.spokenSeverity
        ) {
            Text(notice.title)
                .fontWeight(.medium)
                .fixedSize(horizontal: false, vertical: true)
            ForEach(Array(notice.lines.enumerated()), id: \.offset) { _, line in
                Text(line).explanatory().textSelection(.enabled)
                    .accessibilityIdentifier("notice.line")
            }
            if let until = notice.until, let label = notice.untilLabel {
                // Counts down by itself, and stops at zero once the moment has passed.
                let follows = Date(timeIntervalSince1970: TimeInterval(until))
                let timer = Text(timerInterval: Date()...max(follows, Date()), countsDown: true)
                Text("\(label) \(timer)")
                    .explanatory()
                    .monospacedDigit()
            }
            if let learn = notice.learn, let link = URL(string: learn.link) {
                Link(learn.title, destination: link).font(.callout)
            }
        } actions: {
            ForEach(buttons, id: \.title) { action in
                Button(action.title) { perform(action) }
                    .disabled(!action.enabled)
                    .accessibilityIdentifier("notice.action.\(caseName(action.intent))")
            }
            if let dismiss = notice.actions.first(where: \.dismisses) {
                Button(dismiss.title, systemImage: "xmark") { perform(dismiss) }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .foregroundStyle(.secondary)
                    .help(dismiss.title)
                    .accessibilityIdentifier("notice.dismiss")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("notice.\(notice.id)")
    }
}

struct TipBanner: View {
    let step: SetupStep
    let footing: Footing
    let perform: (Intent) -> Void

    var body: some View {
        Banner(symbol: symbol, tint: .accentColor, spoken: nil) {
            Text(step.title).fontWeight(.medium)
            Text(step.detail).explanatory()
        } actions: {
            ForEach(Array(step.actions.enumerated()), id: \.offset) { index, choice in
                let identifier = "setup.action.\(caseName(choice.intent))"
                if index == 0 {
                    Button(choice.title) { perform(choice.intent) }
                        .buttonStyle(.borderedProminent)
                        .disabled(!choice.enabled)
                        .accessibilityIdentifier(identifier)
                } else {
                    Button(choice.title) { perform(choice.intent) }
                        .disabled(!choice.enabled)
                        .accessibilityIdentifier(identifier)
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("setup.\(caseName(footing))")
    }

    private var symbol: String {
        switch footing {
        case .atLimitUnchosen: Symbol.limits
        default: Symbol.switchAccount
        }
    }
}

private struct Banner<Content: View, Actions: View>: View {
    let symbol: String
    let tint: Color
    // Where the symbol says something the words do not.
    let spoken: String?
    @ViewBuilder let content: Content
    @ViewBuilder let actions: Actions

    var body: some View {
        GroupBox {
            HStack(alignment: .center, spacing: Design.iconSpacing) {
                Image(systemName: symbol)
                    .font(.title3)
                    .foregroundStyle(tint)
                    .accessibilityLabel(spoken ?? "")
                    .accessibilityHidden(spoken == nil)
                VStack(alignment: .leading, spacing: Design.lineSpacing) {
                    content
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                HStack {
                    actions
                }
                .fixedSize()
            }
            .padding(Design.lineSpacing)
        }
    }
}
