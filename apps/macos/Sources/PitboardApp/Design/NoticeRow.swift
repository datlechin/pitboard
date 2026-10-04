import SwiftUI

/// One notice in the window: what it is, everything there is to say about it, and what can
/// be done about it.
struct NoticeRow: View {
    let notice: Notice
    /// Whether a switch is running, which holds back a switch offered here.
    var switching = false
    let perform: (Notice.Action) -> Void

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: Design.iconSpacing) {
            Image(systemName: notice.severity.symbol)
                .foregroundStyle(notice.severity.tint)
                .accessibilityLabel(notice.severity.spoken)
            VStack(alignment: .leading, spacing: Design.rowSpacing) {
                Text(notice.title)
                    .fontWeight(.medium)
                    .fixedSize(horizontal: false, vertical: true)
                ForEach(Array(notice.lines.enumerated()), id: \.offset) { _, line in
                    Text(line).explanatory().textSelection(.enabled)
                }
                if let follows = notice.follows, let label = notice.followsLabel {
                    // Counts down by itself, and stops at zero once the moment has passed.
                    (Text("\(label) ")
                        + Text(timerInterval: Date()...max(follows, Date()), countsDown: true))
                        .explanatory()
                        .monospacedDigit()
                }
                let buttons = notice.actions.filter(\.isButton)
                if !buttons.isEmpty {
                    HStack {
                        ForEach(buttons, id: \.title) { action in
                            Button(action.title) { perform(action) }
                                .disabled(switching && action.switches)
                        }
                    }
                }
            }
            Spacer(minLength: 0)
            if let dismiss = notice.actions.first(where: \.dismisses) {
                Button("Dismiss", systemImage: "xmark") { perform(dismiss) }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .foregroundStyle(.secondary)
                    .help("Dismiss")
            }
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("notice.\(notice.id)")
    }
}

extension Notice.Action {
    /// What its button says.
    var title: String {
        switch self {
        case .use(_, let label): "Switch to \(label)"
        case .giveUp: "Give Up…"
        case .allowLiveUsage: "Allow Again…"
        case .finishDesktopAdd: "Finish Adding…"
        case .dismissSwitch, .dismissAbandoned: "Dismiss"
        }
    }

    /// Whether it puts the notice away, which is an icon at the row's end rather than a
    /// button under it.
    var dismisses: Bool {
        switch self {
        case .dismissSwitch, .dismissAbandoned: true
        case .use, .giveUp, .allowLiveUsage, .finishDesktopAdd: false
        }
    }

    var isButton: Bool { !dismisses }

    /// Whether it switches account.
    var switches: Bool {
        if case .use = self { return true }
        return false
    }
}
