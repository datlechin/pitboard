import Foundation
import PitboardKit

/// What the menu shows, and the only place that calls the core. Every call runs off the main
/// thread inside `PitboardService`; this only holds the answers.
@MainActor
@Observable
final class AppModel {
    private let service: PitboardService
    private(set) var status: Status?
    private(set) var problem: String?
    /// The account a switch is running for, so its row can say so.
    private(set) var switching: String?
    private(set) var updatedAt: Date?

    /// Usage is asked of Anthropic for every account, so it is asked sparingly: on opening the
    /// menu when the numbers are a minute old, and in the background every five minutes.
    static let staleAfter: TimeInterval = 60
    private static let refreshEvery: Duration = .seconds(300)

    init(service: PitboardService = PitboardService(settings: .forCurrentUser())) {
        self.service = service
        Task { [weak self] in
            while !Task.isCancelled {
                await self?.refresh()
                try? await Task.sleep(for: Self.refreshEvery, tolerance: .seconds(60))
            }
        }
    }

    /// The account in use and its tightest limit, as the menu bar reads it.
    var title: String {
        guard let account = status?.accounts.first(where: \.signedIn) else { return "pitboard" }
        let name = account.label ?? "unenrolled"
        guard let tightest = account.usage?.windows.map(\.percent).max() else { return name }
        return "\(name) \(Int(tightest.rounded()))%"
    }

    var updated: String {
        guard let updatedAt else { return "not read yet" }
        let ago = Int(Date().timeIntervalSince(updatedAt))
        return ago < 60 ? "updated just now" : "updated \(ago / 60)m ago"
    }

    func refresh(ifOlderThan seconds: TimeInterval = 0) async {
        if let updatedAt, Date().timeIntervalSince(updatedAt) < seconds { return }
        do {
            status = try await service.status()
            problem = status?.warnings.first?.message
            updatedAt = Date()
        } catch {
            problem = Self.saying(error)
        }
    }

    func use(_ label: String) async {
        switching = label
        defer { switching = nil }
        do {
            _ = try await service.switchTo(label)
            updatedAt = nil
            await refresh()
        } catch {
            problem = Self.saying(error)
        }
    }

    /// pitboard's errors already say what to do, so they are shown as they are.
    private static func saying(_ error: Error) -> String {
        if case let PitboardError.Failed(_, message, _) = error {
            return message
        }
        return error.localizedDescription
    }
}
