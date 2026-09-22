import PitboardKit
import SwiftUI

struct AccountRow: View {
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
        // One account, read as one thing with a button in it, rather than six separate
        // stops on the way past.
        .accessibilityElement(children: .contain)
        .accessibilityLabel(account.label ?? "unenrolled, \(account.email)")
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

struct Limit: View {
    let window: Limits
    /// The columns line up across rows, which needs fixed widths, and a fixed width set at
    /// the default text size clips the moment somebody has text larger than that.
    @ScaledMetric(relativeTo: .caption) private var nameWidth: CGFloat = 78
    @ScaledMetric(relativeTo: .caption) private var percentWidth: CGFloat = 34
    @ScaledMetric(relativeTo: .caption2) private var resetsWidth: CGFloat = 62

    var body: some View {
        HStack(spacing: 8) {
            Text(name)
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(width: nameWidth, alignment: .leading)
            ProgressView(value: min(window.percent, 100) / 100).tint(colour)
            Text("\(Int(window.percent.rounded()))%")
                .font(.caption.monospacedDigit())
                .frame(width: percentWidth, alignment: .trailing)
            Text(resets ?? "")
                .font(.caption2)
                .foregroundStyle(.secondary)
                .frame(width: resetsWidth, alignment: .trailing)
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

/// Asks for the name to record the account in use under. Anything else about adding an
/// account needs a browser, which the command line drives.
