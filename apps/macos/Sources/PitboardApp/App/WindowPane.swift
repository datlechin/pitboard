import PitboardKit

/// The main window's panes, with the `String` raw value `@SceneStorage` needs to keep the
/// pane shown.
enum WindowPane: String, CaseIterable, Identifiable {
    case accounts
    case activity
    case machine

    var id: String { rawValue }

    // The welcome has no window of its own yet, so it opens on the accounts.
    init?(_ place: Place) {
        switch place {
        case .accounts, .welcome: self = .accounts
        case .activity: self = .activity
        case .checks: self = .machine
        case .settings: return nil
        }
    }

    var place: Place {
        switch self {
        case .accounts: .accounts
        case .activity: .activity
        case .machine: .checks
        }
    }

    func after(_ request: WindowRequest) -> WindowPane {
        request.place.flatMap(WindowPane.init) ?? self
    }

    var title: String {
        switch self {
        case .accounts: "Accounts"
        case .activity: "Activity"
        case .machine: "This Mac"
        }
    }

    var symbol: String {
        switch self {
        case .accounts: Symbol.accounts
        case .activity: Symbol.activity
        case .machine: Symbol.machine
        }
    }
}

/// Which of the model's requests for the main window have been answered, so that each is
/// answered once: the window opened for it, and the pane it wants shown.
///
/// The model asks by moving `WindowRequest.serial`, from 0, which asks for nothing. The menu
/// bar item opens the window as it first appears and each time the serial moves, since the
/// first launch's request can come before it is first drawn; the window shows the pane as it
/// appears and each time the serial moves while it is open. Answered by serial, a request
/// seen both ways is answered once, and one answered already is not answered again when the
/// window is opened some other way, from the Window menu or the Dock icon's.
@MainActor
final class WindowRequests {
    private var opened: UInt64 = 0
    private var shown: UInt64 = 0

    func opens(_ request: WindowRequest) -> Bool {
        guard request.serial > opened else { return false }
        opened = request.serial
        return true
    }

    func pane(for request: WindowRequest, from pane: WindowPane) -> WindowPane {
        guard request.serial > shown else { return pane }
        shown = request.serial
        return pane.after(request)
    }
}
