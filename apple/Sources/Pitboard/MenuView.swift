import AppKit
import PitboardKit
import ServiceManagement
import SwiftUI

struct MenuView: View {
    let model: AppModel
    let updater: Updater

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let problem = model.problem {
                Label(problem, systemImage: "exclamationmark.triangle")
                    .font(.callout)
                    .foregroundStyle(.orange)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if let status = model.status {
                if status.accounts.isEmpty {
                    Text("No account is enrolled yet. Run `pitboard enroll <label>`.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                ForEach(status.accounts, id: \.accountUuid) { account in
                    AccountRow(account: account, model: model)
                }
            } else if model.problem == nil {
                ProgressView().controlSize(.small)
            }
            Divider()
            Footer(model: model, updater: updater)
        }
        .padding(14)
        .frame(width: 340)
        .task { await model.refresh(ifOlderThan: AppModel.staleAfter) }
    }
}

private struct AccountRow: View {
    let account: Account
    let model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack(spacing: 6) {
                Circle()
                    .fill(account.signedIn ? Color.green : Color.secondary.opacity(0.4))
                    .frame(width: 7, height: 7)
                Text(account.label ?? "unenrolled").fontWeight(.medium)
                Spacer()
                standing
            }
            ForEach(Array(account.usage?.windows.enumerated() ?? [].enumerated()), id: \.offset)
            {
                _, window in
                Limit(window: window)
            }
            if let note = account.staleExplanation {
                Text(note).font(.caption2).foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder private var standing: some View {
        if account.signedIn {
            Text("signed in").font(.caption).foregroundStyle(.secondary)
        } else if model.switching == account.label {
            ProgressView().controlSize(.small)
        } else if account.switchable, let label = account.label {
            Button("Use") { Task { await model.use(label) } }
                .buttonStyle(.link)
                .disabled(model.switching != nil)
        } else if let label = account.label {
            Text("sign in again: pitboard enroll \(label) --sign-in")
                .font(.caption2)
                .foregroundStyle(.orange)
        }
    }
}

private struct Limit: View {
    let window: Limits

    var body: some View {
        HStack(spacing: 8) {
            Text(name).font(.caption).foregroundStyle(.secondary).frame(
                width: 78, alignment: .leading)
            ProgressView(value: min(window.percent, 100) / 100).tint(colour)
            Text("\(Int(window.percent.rounded()))%")
                .font(.caption.monospacedDigit())
                .frame(width: 34, alignment: .trailing)
        }
    }

    private var name: String {
        let base =
            switch window.kind {
            case "session", "five_hour": "5h"
            case "weekly_all", "seven_day", "weekly_scoped": "week"
            default: window.kind
            }
        return window.scope.map { "\(base) · \($0)" } ?? base
    }

    private var colour: Color {
        switch window.percent {
        case 90...: .red
        case 70...: .orange
        default: .green
        }
    }
}

private struct Footer: View {
    let model: AppModel
    let updater: Updater
    @State private var openAtLogin = SMAppService.mainApp.status == .enabled

    var body: some View {
        HStack {
            Text(model.updated).font(.caption).foregroundStyle(.secondary)
            Spacer()
            Button("Refresh") { Task { await model.refresh() } }
            Menu {
                if updater.available {
                    Button("Check for Updates…") { updater.check() }
                }
                Toggle("Open at login", isOn: $openAtLogin)
                    .onChange(of: openAtLogin) { _, wanted in
                        try? wanted
                            ? SMAppService.mainApp.register()
                            : SMAppService.mainApp.unregister()
                        openAtLogin = SMAppService.mainApp.status == .enabled
                    }
                Divider()
                Button("Quit pitboard") { NSApp.terminate(nil) }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
        }
    }
}
