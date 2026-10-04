import PitboardKit
import UserNotifications

/// Tells you once when an account in use runs out, and offers the account of the same tool
/// with the most left. Switching is the button's job, never the notification's: Pitboard
/// does not switch accounts on its own.
@MainActor
final class Notifier: NSObject, UNUserNotificationCenterDelegate {
    /// Called when the notification's button is pressed, with the label to switch to, tool
    /// and all.
    var onSwitch: ((String) -> Void)?
    /// Called when the notification that live usage is paused is clicked, to show the
    /// sheet that asks to read Claude's key again.
    var onLiveUsage: (() -> Void)?

    /// Nil where nothing is posted. Notification Center belongs to an app bundle: asked for
    /// anywhere else, including a test bundle, it stops the process. A fixture posts
    /// nothing either, because asking for permission would put a system prompt in front of
    /// the UI test that launched it.
    private let centre: UNUserNotificationCenter?
    nonisolated static let category = "limit"
    private nonisolated static let action = "switch"
    /// The reset time of each window last reported, by `Advice.key`, so one exhausted
    /// window is mentioned once rather than every few minutes until it resets.
    private(set) var told: [String: Int64] = [:]
    /// How many times this app has said live usage is paused, for a test that must see it
    /// said once. Counted whether or not anything is posted.
    private(set) var liveUsagePausedTold = 0
    private nonisolated static let liveUsagePaused = "live-usage-paused"

    /// Claiming the delegate is free and asks nothing of anyone, and it has to happen at
    /// launch: a notification left in Notification Center and clicked later, including
    /// right after an update relaunches the app, is delivered the moment there is someone
    /// to deliver it to.
    init(delivering: Bool) {
        centre = delivering && Bundle.main.bundleURL.pathExtension == "app" ? .current() : nil
    }

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

    /// Says macOS stopped Pitboard reading Claude's key, which an update to Claude can do.
    /// Clicking it opens Pitboard, where one button asks again; nothing is read from here.
    func tellLiveUsagePaused() {
        liveUsagePausedTold += 1
        guard let centre else { return }
        let content = UNMutableNotificationContent()
        content.title = "Live usage for Claude is paused"
        content.body =
            "macOS stopped letting Pitboard read Claude’s key, which can happen after Claude "
            + "updates. Open Pitboard to allow it again."
        let request = UNNotificationRequest(
            identifier: Self.liveUsagePaused, content: content, trigger: nil)
        Task {
            if (try? await centre.requestAuthorization(options: [.alert])) == true {
                try? await centre.add(request)
            }
        }
    }

    /// What was told about `label` of `provider`, kept as told about it under `name`, so a
    /// rename does not make an account that ran out read as one that has just run out.
    func rename(_ label: String, of provider: String, to name: String) {
        let old = "\(provider)/\(label)/"
        for (key, at) in told where key.hasPrefix(old) {
            told[key] = nil
            told["\(provider)/\(name)/" + key.dropFirst(old.count)] = at
        }
    }

    nonisolated func userNotificationCenter(
        _ centre: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse
    ) async {
        if response.notification.request.identifier == Self.liveUsagePaused {
            await MainActor.run { onLiveUsage?() }
            return
        }
        guard response.actionIdentifier == Self.action,
            let label = response.notification.request.content.userInfo["label"] as? String
        else { return }
        await MainActor.run { onSwitch?(label) }
    }
}
