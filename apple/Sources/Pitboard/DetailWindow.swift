import PitboardKit
import SwiftUI

/// A window, beside the panel rather than instead of it.
///
/// The panel is the glance: who is in use, what each account has left, and the one thing
/// wrong right now. It is 400 points wide and every state that needed more than that
/// expanded inline inside it, or sent the person to a terminal. This is where the things
/// that need room go: each account with what its limits have been doing, everything
/// pitboard has changed, and the whole of what it found about this machine.
struct DetailWindow: View {
    /// The scene's id, named once so the panel's button and the scene cannot drift apart.
    static let id = "pitboard"

    let model: AppModel
    @State private var showing: Pane = .accounts

    enum Pane: String, CaseIterable, Identifiable {
        case accounts = "Accounts"
        case changes = "Changes"
        case checks = "This machine"
        var id: String { rawValue }

        var symbol: String {
            switch self {
            case .accounts: "person.2"
            case .changes: "clock.arrow.circlepath"
            case .checks: "stethoscope"
            }
        }
    }

    var body: some View {
        NavigationSplitView {
            List(Pane.allCases, selection: $showing) { pane in
                Label(pane.rawValue, systemImage: pane.symbol).tag(pane)
            }
            .navigationSplitViewColumnWidth(min: 160, ideal: 180, max: 240)
        } detail: {
            switch showing {
            case .accounts: Accounts(model: model)
            case .changes: Changes(model: model)
            case .checks: Checks(model: model)
            }
        }
        .navigationTitle("pitboard")
        .frame(minWidth: 620, minHeight: 420)
    }
}

private struct Accounts: View {
    let model: AppModel

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 16) {
                ForEach(model.status?.accounts ?? [], id: \.accountUuid) { account in
                    GroupBox {
                        VStack(alignment: .leading, spacing: 8) {
                            HStack {
                                Text(account.label ?? account.email)
                                    .font(.headline)
                                if account.signedIn {
                                    Text("signed in")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                Spacer()
                                if account.switchable, let label = account.label {
                                    Button("Use") { Task { await model.use(label) } }
                                }
                            }
                            Text(account.email).font(.caption).foregroundStyle(.secondary)
                            ForEach(account.usage?.windows ?? [], id: \.kind) { limit in
                                LimitRow(limit: limit, now: model.status?.now ?? 0)
                            }
                            if let lasts = account.lastsSeconds {
                                Text(lasting(lasts, burning: account.lastsBurning))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
            .scenePadding()
        }
        .task { await model.refresh(ifOlderThan: AppModel.staleAfter) }
    }
}

private struct LimitRow: View {
    let limit: Limits
    let now: Int64

    var body: some View {
        HStack(spacing: 12) {
            Text(limit.kind == "five_hour" ? "5h" : "week")
                .font(.caption.monospaced())
                .frame(width: 40, alignment: .leading)
            ProgressView(value: min(limit.percent, 100), total: 100)
            Text("\(Int(limit.percent.rounded()))%")
                .font(.caption.monospacedDigit())
                .frame(width: 44, alignment: .trailing)
            if let resets = limit.resetsAt, resets > now {
                Text(
                    "resets in "
                        + Duration.seconds(resets - now)
                        .formatted(.units(allowed: [.days, .hours, .minutes], width: .narrow))
                )
                .font(.caption)
                .foregroundStyle(.secondary)
            }
        }
    }
}

private struct Changes: View {
    let model: AppModel

    /// A line of the log, numbered. The log itself has no id: two changes can share a
    /// timestamp, a verb and a subject, and rows that claim the same identity make a Table
    /// drop all but one of them.
    private struct Line: Identifiable {
        let id: Int
        let change: Change
    }

    private var lines: [Line] {
        model.changes.reversed().enumerated().map { Line(id: $0.offset, change: $0.element) }
    }

    var body: some View {
        Table(lines) {
            TableColumn("When") { Text($0.change.at).monospacedDigit() }
            TableColumn("What") { line in
                Text("\(line.change.verb) \(line.change.subject)").lineLimit(1)
            }
            TableColumn("How it ended") { Text($0.change.outcome) }
            TableColumn("Asked by") { Text($0.change.caller) }
        }
        .task { await model.readChanges() }
        .overlay {
            if model.changes.isEmpty {
                ContentUnavailableView(
                    "Nothing yet",
                    systemImage: "clock",
                    description: Text("pitboard records every change it makes here."))
            }
        }
    }
}

private struct Checks: View {
    let model: AppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(model.checks, id: \.code) { check in
                    HStack(alignment: .firstTextBaseline, spacing: 10) {
                        Image(systemName: symbol(check.level))
                            .foregroundStyle(colour(check.level))
                            .accessibilityLabel(spoken(check.level))
                        VStack(alignment: .leading, spacing: 2) {
                            Text(check.name).font(.headline)
                            Text(check.detail)
                                .font(.callout)
                                .foregroundStyle(.secondary)
                                .textSelection(.enabled)
                            if !check.advice.isEmpty {
                                Text(check.advice).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                        Spacer()
                    }
                }
                Button(model.checks.isEmpty ? "Check this machine" : "Check again") {
                    Task { await model.diagnose() }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .scenePadding()
        }
        .task { await model.diagnose() }
    }

    private func symbol(_ level: Level) -> String {
        switch level {
        case .ok: "checkmark.circle"
        case .warn: "exclamationmark.triangle"
        case .fail: "xmark.octagon"
        }
    }

    private func colour(_ level: Level) -> Color {
        switch level {
        case .ok: .green
        case .warn: .orange
        case .fail: .red
        }
    }

    /// What VoiceOver says instead of naming a shape or a colour.
    private func spoken(_ level: Level) -> String {
        switch level {
        case .ok: "fine"
        case .warn: "worth looking at"
        case .fail: "broken"
        }
    }
}
