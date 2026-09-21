import AppKit
import PitboardKit
import ServiceManagement
import SwiftUI

struct MenuView: View {
    let model: AppModel
    let updater: Updater

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let advice = model.advice {
                Label(
                    "\(advice.ran) has none of its \(advice.limit) limit left. "
                        + "\(advice.use) has \(advice.left)% of its own left.",
                    systemImage: "exclamationmark.circle"
                )
                .font(.callout)
                .fixedSize(horizontal: false, vertical: true)
            }
            if let adopted = model.adopted, adopted > Date() {
                Text(
                    "Sessions already open follow in ",
                    comment: "followed by a countdown"
                )
                .font(.caption).foregroundStyle(.secondary)
                    + Text(timerInterval: Date()...adopted, countsDown: true)
                    .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
            }
            if updater.available, let version = updater.waiting {
                HStack {
                    Label("Version \(version) is ready", systemImage: "arrow.down.circle")
                        .font(.callout)
                    Spacer()
                    Button("Install") { updater.check() }
                }
            }
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
            if !model.checks.isEmpty {
                Divider()
                Diagnosis(model: model)
            }
            Divider()
            Footer(model: model, updater: updater)
        }
        .padding(14)
        .frame(width: 400)
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
                    .accessibilityHidden(true)
                Text(account.label ?? "unenrolled").fontWeight(.medium)
                if let email = account.email.isEmpty ? nil : account.email {
                    Text(email)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .accessibilityLabel("account \(email)")
                }
                Spacer()
                standing
            }
            ForEach(Array((account.usage?.windows ?? []).enumerated()), id: \.offset) {
                _, window in
                Limit(window: window)
            }
            if let note = account.staleExplanation ?? parkedNote {
                Text(note).font(.caption2).foregroundStyle(.secondary)
            }
        }
    }

    /// When the login held for this account stops being usable. The command line says the
    /// same thing; without it the panel cannot tell you a switch is about to stop working.
    private var parkedNote: String? {
        guard !account.signedIn, let parked = account.parked, let at = parked.refreshExpiresAt
        else { return nil }
        let left = Date(timeIntervalSince1970: TimeInterval(at)).timeIntervalSinceNow
        guard left > 0 else { return "its parked login has expired" }
        let days = Int(left / 86_400)
        return days >= 1
            ? "good for \(days) more day\(days == 1 ? "" : "s")"
            : "good for under a day"
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
                .accessibilityLabel("Switch to \(label)")
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
            Text(name)
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(width: 78, alignment: .leading)
            ProgressView(value: min(window.percent, 100) / 100).tint(colour)
            Text("\(Int(window.percent.rounded()))%")
                .font(.caption.monospacedDigit())
                .frame(width: 34, alignment: .trailing)
            Text(resets ?? "")
                .font(.caption2)
                .foregroundStyle(.secondary)
                .frame(width: 62, alignment: .trailing)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(spoken)
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

    /// "resets in 3h", as the command line says it. Blank once the moment has passed: the
    /// next reading is what says whether it actually reset.
    private var resets: String? {
        guard let at = window.resetsAt else { return nil }
        let left = Date(timeIntervalSince1970: TimeInterval(at)).timeIntervalSinceNow
        guard left > 0 else { return nil }
        let hours = Int(left / 3600)
        if hours >= 24 { return "in \(hours / 24)d \(hours % 24)h" }
        if hours >= 1 { return "in \(hours)h" }
        return "in \(max(1, Int(left / 60)))m"
    }

    private var spoken: String {
        let used = "\(name), \(Int(window.percent.rounded())) percent used"
        return resets.map { "\(used), resets \($0)" } ?? used
    }

    private var colour: Color {
        switch window.percent {
        case 90...: .red
        case 70...: .orange
        default: .green
        }
    }
}

/// What doctor found, in the panel, so a machine-level problem does not have to be chased
/// from a terminal.
private struct Diagnosis: View {
    let model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(model.checks, id: \.code) { check in
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Image(systemName: mark(check.level))
                        .foregroundStyle(colour(check.level))
                        .accessibilityHidden(true)
                    Text(check.name).font(.caption).frame(width: 110, alignment: .leading)
                    Text(check.detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                        .truncationMode(.middle)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("\(check.name): \(check.detail)")
                if check.level != .ok, !check.advice.isEmpty {
                    Text(check.advice)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }

    private func mark(_ level: Level) -> String {
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
                Button(model.checks.isEmpty ? "Check this machine…" : "Hide checks") {
                    if model.checks.isEmpty {
                        Task { await model.diagnose() }
                    } else {
                        model.forgetDiagnosis()
                    }
                }
                Divider()
                Toggle("Open at login", isOn: $openAtLogin)
                    .onChange(of: openAtLogin) { _, wanted in
                        try? wanted
                            ? SMAppService.mainApp.register()
                            : SMAppService.mainApp.unregister()
                        openAtLogin = SMAppService.mainApp.status == .enabled
                    }
                Divider()
                Text(
                    "pitboard \(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "")"
                )
                Link(
                    "Documentation",
                    destination: URL(string: "https://datlechin.github.io/pitboard/")!)
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
