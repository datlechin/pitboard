import PitboardKit

// Settings is a scene of its own, which opens another way.
enum PlaceWindow: String, CaseIterable {
    case accounts
    case activity
    case diagnostics
    case welcome

    // No place is the accounts window as it was left.
    init?(_ place: Place?) {
        switch place {
        case nil, .accounts?: self = .accounts
        case .activity?: self = .activity
        case .checks?: self = .diagnostics
        case .welcome?: self = .welcome
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
