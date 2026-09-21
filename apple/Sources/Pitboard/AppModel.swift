import AppKit
import Foundation
import PitboardKit

/// SwiftUI has a `Window` of its own, so a limit's window is named for what it is here.
typealias Limits = PitboardBindings.Window

/// What the menu shows, and the only place that calls the core. Every call runs off the main
/// thread inside `PitboardService`; this only holds the answers.
@MainActor
@Observable
final class AppModel {
    private let service: any Core
    private(set) var status: Status?
    private(set) var problem: String?
    /// The account a switch is running for, so its row can say so.
    private(set) var switching: String?
    private(set) var updatedAt: Date?
    /// An account has run out and another has room. Shown in the panel whether or not
    /// notifications are allowed, so the advice does not depend on a permission.
    private(set) var advice: Advice?
    /// When sessions that were already open will have picked up the last switch.
    private(set) var adopted: Date?
    /// Every check pitboard makes about this machine, once someone asks for them.
    private(set) var checks: [Check] = []

    /// Usage is asked of Anthropic for every account, so it is asked sparingly: on opening the
    /// menu when the numbers are a minute old, and in the background every five minutes.
    static let staleAfter: TimeInterval = 60
    private static let refreshEvery: Duration = .seconds(300)

    private let notifier = Notifier()

    init(service: any Core = PitboardService(settings: .forCurrentUser())) {
        self.service = service
        notifier.start()
        notifier.onSwitch = { [weak self] label in
            Task { await self?.use(label) }
        }
        Task { [weak self] in
            while !Task.isCancelled {
                await self?.refresh()
                try? await Task.sleep(for: Self.refreshEvery, tolerance: .seconds(60))
            }
        }
        // Numbers read before the machine slept say nothing about now.
        Task { [weak self] in
            let woke = NSWorkspace.shared.notificationCenter.notifications(
                named: NSWorkspace.didWakeNotification)
            for await _ in woke {
                await self?.refresh()
            }
        }
    }

    /// The account in use and its tightest limit, as the menu bar reads it.
    var title: String { menuTitle(for: status) }

    var updated: String {
        guard let updatedAt else { return "not read yet" }
        let ago = Int(Date().timeIntervalSince(updatedAt))
        return ago < 60 ? "updated just now" : "updated \(ago / 60)m ago"
    }

    func refresh(ifOlderThan seconds: TimeInterval = 0) async {
        if let updatedAt, Date().timeIntervalSince(updatedAt) < seconds { return }
        do {
            let read = try await service.status()
            status = read
            problem = read.warnings.first?.message
            updatedAt = Date()
            advice = Advice.about(read, unless: notifier.told)
            if let advice { notifier.tell(advice) }
        } catch {
            problem = Self.saying(error)
        }
    }

    func use(_ label: String) async {
        switching = label
        defer { switching = nil }
        do {
            let done = try await service.switchTo(label)
            if case .switched(_, _, let ceiling) = done.outcome {
                adopted = Date().addingTimeInterval(TimeInterval(ceiling))
            }
            advice = nil
            updatedAt = nil
            await refresh()
        } catch {
            problem = Self.saying(error)
        }
    }

    /// What `pitboard doctor` reports, for when something is wrong at machine level and
    /// the one line a failed call carries is not enough to act on.
    func diagnose() async {
        checks = await service.doctor().checks
    }

    func forgetDiagnosis() {
        checks = []
    }

    /// pitboard's errors already say what to do, so they are shown as they are.
    private static func saying(_ error: Error) -> String {
        if case PitboardError.Failed(_, let message, _) = error {
            return message
        }
        return error.localizedDescription
    }
}

/// The limit worth putting in the menu bar: the account's own, not one scoped to a single
/// model, and one the account is working against when the server says which. A scoped row
/// at 98% would otherwise read as though everything had stopped.
func headline(of windows: [Limits]) -> Limits? {
    let ownLimits = windows.filter { $0.scope == nil }
    let candidates = ownLimits.isEmpty ? windows : ownLimits
    let active = candidates.filter(\.isActive)
    return (active.isEmpty ? candidates : active).max { $0.percent < $1.percent }
}

/// What the menu bar says: the account in use and the limit closest to its end. Nothing is
/// known until the first read, and an account signed in but not enrolled has no name here.
func menuTitle(for status: Status?) -> String {
    guard let account = status?.accounts.first(where: \.signedIn) else { return "" }
    // Long labels are bounded, because this sits in a bar someone else also wants space in.
    let full = account.label ?? "unenrolled"
    let name = full.count > 12 ? full.prefix(11) + "…" : full[...]
    guard let tightest = headline(of: account.usage?.windows ?? []) else {
        return String(name)
    }
    return "\(name) \(Int(tightest.percent.rounded()))%"
}
