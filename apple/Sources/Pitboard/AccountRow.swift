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
                if account.unplaced {
                    // Not an account anybody can name, so not "unenrolled": what is wrong
                    // with the login is the only thing there is to say about it.
                    Text(account.heading)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                } else {
                    Text(account.heading).fontWeight(.medium)
                }
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
            if let note = account.unplaced ? nil : account.staleExplanation ?? parkedNote {
                Text(note).font(.caption2).foregroundStyle(.secondary)
            }
        }
        // One account, read as one thing with a button in it, rather than six separate
        // stops on the way past.
        .accessibilityElement(children: .contain)
        .accessibilityLabel(spokenName)
    }

    /// Once more than one tool is shown, the row says which it is for: VoiceOver reads a
    /// row on its own, apart from the heading above it, and two tools can each have a
    /// `work`.
    private var spokenName: String {
        let tool = model.tool(account.provider)?.name ?? account.provider
        // Only what the row is: its first line, read next, says what is wrong with it, and
        // naming the row with that sentence read it twice.
        if account.unplaced { return "\(tool) login pitboard cannot use" }
        let name = account.label ?? "unenrolled, \(account.email)"
        return model.showsTools ? "\(name), \(tool)" : name
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
        } else if let qualified = account.qualified, model.switching == qualified {
            ProgressView().controlSize(.small)
        } else if account.switchable, let qualified = account.qualified {
            // Named with its tool once there is more than one: a list of buttons and a
            // spoken command have no row around them to say which `work` is meant.
            Button("Use") { Task { await model.use(qualified) } }
                .buttonStyle(.link)
                .disabled(model.switching != nil)
                .accessibilityLabel("Switch to \(model.name(of: account))")
        } else if let name = typed(account) {
            Text("sign in again: pitboard enroll \(name) --sign-in")
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

    /// From the window's length, so a Codex account's limits read the way Claude Code's do.
    private var name: String { windowShortName(window) }

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
        spokenLimit(
            window,
            resettingIn: window.resetsAt.map {
                Date(timeIntervalSince1970: TimeInterval($0)).timeIntervalSinceNow
            })
    }

    private var colour: Color {
        switch window.percent {
        case 90...: .red
        case 70...: .orange
        default: .green
        }
    }
}
