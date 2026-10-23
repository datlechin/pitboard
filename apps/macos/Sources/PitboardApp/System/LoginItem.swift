import Foundation
import Observation
import ServiceManagement

/// A protocol so a UI test can turn the setting on and off without registering the test
/// build as a login item of whoever runs the tests.
@MainActor
protocol LoginItem: AnyObject {
    var state: LoginItemState { get }
    func register() throws
    func unregister() throws
    func openSystemSettings()
}

enum LoginItemState: Equatable, Sendable {
    case enabled
    case disabled
    /// Registered, and waiting for its person to allow it in System Settings. macOS asks
    /// this of an app whose login item it has not seen before, and until then it does not
    /// open at login.
    case requiresApproval
}

@MainActor
final class MainAppLoginItem: LoginItem {
    init() {}

    var state: LoginItemState {
        switch SMAppService.mainApp.status {
        case .enabled: .enabled
        case .requiresApproval: .requiresApproval
        default: .disabled
        }
    }

    func register() throws { try SMAppService.mainApp.register() }
    func unregister() throws { try SMAppService.mainApp.unregister() }
    func openSystemSettings() { SMAppService.openSystemSettingsLoginItems() }
}

// The settings' Open Pitboard at login: what macOS has, and why a change it refused did not
// happen. macOS's own: the login item belongs to this app's bundle, and the model is only told.
@MainActor
@Observable
final class OpenAtLogin {
    @ObservationIgnored private let item: any LoginItem
    @ObservationIgnored private let said: @MainActor (Bool) -> Void
    private(set) var state: LoginItemState
    private(set) var failed: String?

    init(_ item: any LoginItem, said: @escaping @MainActor (Bool) -> Void = { _ in }) {
        self.item = item
        self.said = said
        state = item.state
    }

    // The person can change it in System Settings at any time.
    func read() {
        state = item.state
        said(state != .disabled)
    }

    func set(_ wanted: Bool) {
        failed = nil
        do {
            if wanted {
                try item.register()
            } else {
                try item.unregister()
            }
        } catch {
            failed = error.localizedDescription
        }
        read()
    }

    func openSystemSettings() {
        item.openSystemSettings()
    }
}
