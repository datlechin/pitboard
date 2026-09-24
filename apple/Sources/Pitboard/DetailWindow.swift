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
                // The window is what a new install opens into, so the one thing to do
                // belongs here as much as in the panel.
                FirstRun(model: model)
                ForEach(model.status?.accounts ?? [], id: \.id) { account in
                    GroupBox {
                        VStack(alignment: .leading, spacing: 8) {
                            HStack {
                                Text(account.heading)
                                    .font(.headline)
                                if model.showsTools, let tool = model.tool(account.provider) {
                                    Text(tool.name)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                        .padding(.horizontal, 6)
                                        .padding(.vertical, 1)
                                        .background(.quaternary, in: .capsule)
                                }
                                if account.signedIn {
                                    Text("signed in")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                Spacer()
                                if account.switchable, let qualified = account.qualified {
                                    Button("Use") { Task { await model.use(qualified) } }
                                        .accessibilityLabel(
                                            "Switch to \(model.name(of: account))")
                                }
                            }
                            if !account.email.isEmpty {
                                Text(account.email).font(.caption).foregroundStyle(.secondary)
                            }
                            ForEach(
                                Array((account.usage?.windows ?? []).enumerated()), id: \.offset
                            ) {
                                _, window in
                                Limit(window: window)
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
                        Image(systemName: check.level.symbol)
                            .foregroundStyle(check.level.tint)
                            .accessibilityLabel(check.level.spoken)
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
}
