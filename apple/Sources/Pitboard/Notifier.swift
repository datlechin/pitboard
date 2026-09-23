import PitboardKit
import UserNotifications

/// Tells you once when an account in use runs out, and offers the account of the same tool
/// with the most left. Switching is the button's job, never the notification's: pitboard
/// does not switch accounts on its own.
@MainActor
final class Notifier: NSObject, UNUserNotificationCenterDelegate {
    /// Called when the notification's button is pressed, with the label to switch to, tool
    /// and all.
    var onSwitch: ((String) -> Void)?

    /// Notification Center belongs to an app bundle. Asked for anywhere else, including a
    /// test bundle, it stops the process, so everything here is a no-op outside one.
    private static let inAnApp = Bundle.main.bundleURL.pathExtension == "app"
    private let centre: UNUserNotificationCenter? =
        Bundle.main.bundleURL.pathExtension == "app" ? .current() : nil
    fileprivate nonisolated static let category = "limit"
    private nonisolated static let action = "switch"
    /// The reset time of each window last reported, by `Advice.key`, so one exhausted
    /// window is mentioned once rather than every few minutes until it resets.
    private(set) var told: [String: Int64] = [:]

    /// Claiming the delegate is free and asks nothing of anyone, and it has to happen at
    /// launch: a notification left in Notification Center and clicked later, including
    /// right after an update relaunches the app, is delivered the moment there is someone
    /// to deliver it to.
    func start() {
        guard let centre else { return }
        centre.delegate = self
        centre.setNotificationCategories([
            UNNotificationCategory(
                identifier: Self.category,
                actions: [
                    UNNotificationAction(
                        identifier: Self.action, title: "Switch", options: [.foreground])
                ],
                intentIdentifiers: [])
        ])
    }

    /// Permission is asked for when there is finally something to say, not at launch, where
    /// a prompt arrives before the app has shown what it is for.
    func tell(_ advice: Advice) {
        told[advice.key] = advice.window.resetsAt ?? 0
        guard let centre else { return }
        let request = UNNotificationRequest(
            identifier: "\(advice.key)-\(advice.window.resetsAt ?? 0)",
            content: advice.notification, trigger: nil)
        Task {
            // Refused is not an error: the panel carries the same advice either way.
            if (try? await centre.requestAuthorization(options: [.alert])) == true {
                try? await centre.add(request)
            }
        }
    }

    nonisolated func userNotificationCenter(
        _ centre: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse
    ) async {
        guard response.actionIdentifier == Self.action,
            let label = response.notification.request.content.userInfo["label"] as? String
        else { return }
        await MainActor.run { onSwitch?(label) }
    }
}

/// An account has run out, and another account of the same tool has room.
struct Advice {
    /// The tool both accounts are for, as a `Tool`'s code.
    let provider: String
    /// The tool's name, when accounts of more than one tool are shown and a sentence has to
    /// say which one it is about. Nil otherwise, so a machine with one tool reads as before.
    let tool: String?
    /// The account that ran out, and the window it ran out of.
    let ran: String
    let window: Limits
    /// The account offered instead, and what it has left in the same kind of window.
    let use: String
    let left: Int
    /// What to switch to: `use` with its tool, which names one account whatever else is
    /// enrolled. Two tools can each have a `work`.
    let switchTo: String

    /// Nothing to say unless an account in use has exhausted a window that has not been
    /// mentioned yet and an account of the same tool that can be switched to now has room in
    /// the same kind of window. At most one piece of advice per tool, in the order `tools`
    /// lists them.
    ///
    /// Only ever the same tool: a Codex account with room left is no help to somebody whose
    /// Claude Code account has run out, and a switch between them is not a switch at all.
    /// Weekly limits are per account, so the comparison is like for like.
    static func about(
        _ status: Status, tools: [Tool] = [], unless told: [String: Int64]
    ) -> [Advice] {
        let providers = inOrder(status.accounts.map(\.provider), by: tools)
        return providers.compactMap { provider in
            let mine = status.accounts.filter { $0.provider == provider }
            guard let current = mine.first(where: { $0.signedIn && $0.label != nil }),
                let ran = current.label
            else { return nil }
            for window in current.usage?.windows ?? [] where window.percent >= 100 {
                guard told[key(provider, ran, window)] != (window.resetsAt ?? 0) else {
                    continue
                }
                let spare =
                    mine
                    .filter { $0.switchable && $0.qualified != nil }
                    .min { used($0, like: window) < used($1, like: window) }
                guard let spare, let use = spare.label, let switchTo = spare.qualified,
                    used(spare, like: window) < 100
                else { continue }
                return Advice(
                    provider: provider,
                    tool: providers.count > 1
                        ? tools.first { $0.code == provider }?.name ?? provider : nil,
                    ran: ran, window: window, use: use,
                    left: 100 - Int(used(spare, like: window).rounded()),
                    switchTo: switchTo)
            }
            return nil
        }
    }

    /// What an account has used of the same window, counting a window it does not report
    /// as spent: an account whose limits are unknown is not one to recommend.
    private static func used(_ account: Account, like window: Limits) -> Double {
        account.usage?.windows.first { $0.kind == window.kind && $0.scope == window.scope }?
            .percent ?? 100
    }

    /// What `Notifier.told` is keyed by: the tool, the account and the window. Keyed by
    /// window alone, one tool's exhausted five hours would silence another's.
    static func key(_ provider: String, _ label: String, _ window: Limits) -> String {
        [provider, label, window.kind, window.scope ?? ""].joined(separator: "/")
    }

    var key: String { Self.key(provider, ran, window) }

    /// How a person names the window that ran out.
    var limit: String { windowName(window) }

    /// The panel's sentence about it.
    var said: String {
        let about = tool.map { "\($0): " } ?? ""
        return "\(about)\(ran) has none of its \(limit) limit left. "
            + "\(use) has \(left)% of its own left."
    }

    var notification: UNNotificationContent {
        let content = UNMutableNotificationContent()
        content.title = "\(ran) has no \(limit) limit left"
        if let tool { content.subtitle = tool }
        content.body = "\(use) has \(left)% of its own left."
        content.categoryIdentifier = Notifier.category
        content.userInfo = ["label": switchTo]
        return content
    }
}
