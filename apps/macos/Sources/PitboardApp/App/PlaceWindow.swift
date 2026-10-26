import PitboardKit

// Settings is a scene of its own, which opens another way.
enum PlaceWindow: String, CaseIterable {
    case accounts
    case activity
    case diagnostics

    // No place is the accounts window as it was left, and the welcome has no window yet.
    init?(_ place: Place?) {
        switch place {
        case nil, .accounts?, .welcome?: self = .accounts
        case .activity?: self = .activity
        case .checks?: self = .diagnostics
        case .settings?: return nil
        }
    }

    var id: String { rawValue }
}

// The first launch's request can come before the menu bar item is drawn, so it looks twice.
@MainActor
final class WindowRequests {
    private var opened: UInt64 = 0

    func opens(_ request: WindowRequest) -> Bool {
        guard request.serial > opened else { return false }
        opened = request.serial
        return true
    }
}
