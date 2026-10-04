import Foundation
import PitboardKit

/// Something Pitboard has to tell somebody that is not an account: an account that ran out
/// with another to switch to, what the last switch means for sessions already open, a read
/// that could not reach a service, a warning, an interrupted switch.
///
/// Worked out in one place from the model, so the window's list and the menu's summary of
/// it cannot disagree, and so what each says can be tested without drawing either.
struct Notice: Identifiable, Equatable {
    enum Severity: Int, Comparable {
        case info
        case warning
        case error

        static func < (lhs: Severity, rhs: Severity) -> Bool { lhs.rawValue < rhs.rawValue }
    }

    enum Action: Equatable {
        /// Switch to the account with this label and tool, named by its label.
        case use(qualified: String, label: String)
        /// Put away what a tool's last switch said.
        case dismissSwitch(provider: String)
        /// Give up on an interrupted switch, after asking.
        case giveUp
        /// Put away what giving up on one said.
        case dismissAbandoned
        /// Show the sheet that asks to read Claude's key again, for live usage that macOS
        /// stopped.
        case allowLiveUsage
        /// Show the sheet for a Claude Desktop account whose adding was left halfway.
        case finishDesktopAdd
    }

    let id: String
    let severity: Severity
    /// One line, as the menu and the window's heading for it say it.
    let title: String
    /// Everything there is to say, a paragraph each.
    let lines: [String]
    /// When sessions already open follow a switch, for a live countdown.
    var follows: Date?
    /// Said before the countdown: "Sessions already open follow in".
    var followsLabel: String?
    let actions: [Action]
}

extension AppModel {
    /// Everything to tell somebody, the most pressing first: what stops Pitboard working,
    /// then accounts that ran out, then what the last switch of each tool said, then any
    /// warning the last read or change carried, then what is only worth knowing.
    func notices(at now: Date = Date()) -> [Notice] {
        var notices: [Notice] = []
        if stuck {
            let reason =
                problem
                ?? "An interrupted switch can’t be finished until \(services) answers."
            notices.append(
                Notice(
                    id: "stuck", severity: .error, title: "An interrupted switch is waiting",
                    lines: [
                        reason,
                        "Giving up on it keeps every login. Nothing is deleted.",
                    ],
                    actions: [.giveUp]))
        } else if let problem, footing != .noClaudeCode {
            notices.append(
                Notice(
                    id: "read", severity: .error, title: "Couldn’t read usage",
                    lines: [problem]
                        + (status?.accounts.contains { $0.usage != nil } == true
                            ? ["The numbers shown are the last ones measured."] : []),
                    actions: []))
        }
        for advice in advice {
            let tool = advice.tool.map { "\($0): " } ?? ""
            notices.append(
                Notice(
                    id: "advice/\(advice.key)", severity: .warning,
                    title: "\(tool)\(advice.ran) has no \(advice.limit) limit left",
                    lines: ["\(advice.use) has \(advice.left)% of its own left."],
                    actions: [.use(qualified: advice.switchTo, label: advice.use)]))
        }
        // Said where it can be acted on rather than at every read: the notification about
        // it is sent once, and this stays until live usage works again or is turned off.
        if let liveUsage, liveUsage.enabled, liveUsage.approval == "needs_approval" {
            notices.append(
                Notice(
                    id: "live-usage", severity: .warning,
                    title: "Live usage for Claude is paused",
                    lines: [
                        "macOS stopped letting Pitboard read Claude’s key, which can happen "
                            + "after Claude updates. Until it is allowed again, Claude "
                            + "Desktop’s numbers come from Claude’s own history."
                    ],
                    actions: [.allowLiveUsage]))
        }
        // An add left halfway leaves Claude signed out. The window does not come forward for
        // one started somewhere else, so this is where it is said.
        if let awaiting = desktopAwaiting {
            let back = awaiting.fromLabel.map { ", or put \($0) back" } ?? ""
            notices.append(
                Notice(
                    id: "desktop-awaiting", severity: .warning,
                    title: "Adding a Claude account isn’t finished",
                    lines: [
                        "Claude is signed out while another account is added. Sign in to it "
                            + "in Claude and name it in Pitboard\(back)."
                    ],
                    actions: [.finishDesktopAdd]))
        }
        for last in lastSwitches {
            notices.append(notice(about: last, at: now))
        }
        // What a switch's notice says already, which is only what the read does not repeat:
        // a warning both carry is the read's to say, and leaving it out of both said it
        // nowhere.
        let shown = Set(lastSwitches.flatMap { warnings(after: $0) })
        for warning in otherWarnings where !shown.contains(warning) {
            if stuck, warning.code == "recovery_undetermined" { continue }
            notices.append(
                Notice(
                    id: "warning/\(warning.code)/\(warning.message.hashValue)",
                    severity: .warning, title: warningTitle(warning),
                    lines: [warning.message], actions: []))
        }
        if let abandoned {
            notices.append(
                Notice(
                    id: "abandoned", severity: .info,
                    title: "Gave up on the interrupted switch",
                    lines: [
                        "The switch from \(abandoned.from) to \(abandoned.to) was given up. "
                            + "\(logins(abandoned.loginsKept)) kept, and nothing was deleted."
                    ],
                    actions: [.dismissAbandoned]))
        }
        return notices
    }

    /// What one tool's last switch still has to say.
    private func notice(about last: LastSwitch, at now: Date) -> Notice {
        let label = split(last.to).label
        let tool = tool(last.provider)?.name ?? last.provider
        let warned = warnings(after: last)
        let lines = [last.said, last.notice].compactMap { $0 } + warned.map(\.message)
        let pending = last.adopted.flatMap { $0 > now ? $0 : nil }
        return Notice(
            id: "switch/\(last.provider)",
            severity: warned.isEmpty && last.notice == nil ? .info : .warning,
            title: last.said == nil
                ? "Switched \(showsTools ? "\(tool) " : "")to \(label)"
                : "\(label)\(showsTools ? " in \(tool)" : "") has a new login",
            lines: lines,
            follows: pending,
            followsLabel: pending == nil
                ? nil
                : showsTools
                    ? "\(tool) sessions already open follow in"
                    : "Sessions already open follow in",
            actions: [.dismissSwitch(provider: last.provider)])
    }
}

/// "1 login", "2 logins".
private func logins(_ count: UInt32) -> String {
    count == 1 ? "1 login" : "\(count) logins"
}

/// A warning's heading, from its code, so the menu can name it in a line. The message
/// under it is the core's own, which says what to do.
func warningTitle(_ warning: Warning) -> String {
    switch warning.code {
    case "sessions_still_running": "Open sessions still use the previous account"
    case "sessions_keep_old_login": "Open sessions still use the old login"
    case "auth_overridden": "An environment variable overrides the login"
    case "parked_login_refused": "A parked login was refused"
    case "lock_compromised": "The login may have been written twice"
    case "parks_pending_removal": "Old parked logins are still there"
    case "written_on_the_command_line": "A login was passed on the command line"
    case "sign_in_parked_not_in_use": "The new login was parked, not put in use"
    case "interrupted_switch_finished": "An interrupted switch was finished"
    case "interrupted_switch_undone": "An interrupted switch was undone"
    case "recovery_undetermined": "An interrupted switch is waiting"
    case "recovery_waiting": "An interrupted Claude Desktop switch is waiting"
    case "switch_unfinished": "A Claude Desktop switch stopped partway"
    case "park_expires_soon": "A Claude Desktop sign-in lapses soon"
    case "strays_kept": "Claude Desktop files were set aside"
    case "replaced_outside_pitboard": "Claude was signed in outside Pitboard"
    default: "Pitboard has a warning"
    }
}
