import AppKit
import SwiftUI

/// Whether Pitboard is a menu bar app or a regular one: in the Dock, in Command-Tab and with
/// its menus in the menu bar while any of its windows is open, and only in the menu bar
/// while none is.
///
/// A window behind another app has no way back without a Dock icon, and a window whose app
/// has no menus has no Edit menu to copy from and no shortcuts. Counted from each window's
/// root view, which appears when its window opens and disappears when it closes: measured on
/// macOS 27, not when it is minimised or becomes a background tab.
@MainActor
final class AppPresence {
    private var open: Set<UUID> = []
    private let apply: @MainActor (NSApplication.ActivationPolicy) -> Void
    private let bringForward: @MainActor () -> Void
    private let reopen: @MainActor () -> Void
    private let isActive: @MainActor () -> Bool
    private let settle: @MainActor () async -> Void

    /// `settle` waits for a window that replaces one closing to open, before the app goes
    /// back to the menu bar, and for an activation to take.
    init(
        apply: @escaping @MainActor (NSApplication.ActivationPolicy) -> Void,
        bringForward: @escaping @MainActor () -> Void,
        reopen: @escaping @MainActor () -> Void = {},
        isActive: @escaping @MainActor () -> Bool = { true },
        settle: @escaping @MainActor () async -> Void
    ) {
        self.apply = apply
        self.bringForward = bringForward
        self.reopen = reopen
        self.isActive = isActive
        self.settle = settle
    }

    static func live() -> AppPresence {
        AppPresence(
            apply: { NSApp.setActivationPolicy($0) },
            bringForward: { NSApp.activate() },
            reopen: {
                let configuration = NSWorkspace.OpenConfiguration()
                configuration.activates = true
                NSWorkspace.shared.openApplication(
                    at: Bundle.main.bundleURL, configuration: configuration)
            },
            isActive: { NSApp.isActive },
            settle: { try? await Task.sleep(for: .milliseconds(500)) })
    }

    var hasWindows: Bool { !open.isEmpty }

    /// For a window about to open. Regular first, so Pitboard comes forward with its Dock
    /// icon and its menus.
    func activate() {
        apply(.regular)
        bringForward()
        // Refused after a notification's button too, where nobody clicked Pitboard itself.
        Task {
            await settle()
            if !isActive() { reopen() }
        }
    }

    /// Brings Pitboard to the front for something that came from another app, a shared link,
    /// while Pitboard may be in the background with nobody's click on it.
    ///
    /// Measured on macOS 27: `NSApp.activate()` from an app in the background is refused, and
    /// a link from the Share extension at a launch leaves Pitboard there. Launch Services
    /// opening the app does bring it forward, so Pitboard asks it to.
    func comeForward() {
        apply(.regular)
        if isActive() {
            bringForward()
        } else {
            reopen()
        }
    }

    func opened(_ window: UUID) {
        open.insert(window)
        apply(.regular)
    }

    /// A window closed. When it was the last, the app goes back to the menu bar, once a
    /// window replacing it has had its moment to open: the account picker closes before the
    /// window it opened appears, and going back in between gives the app's activation away.
    func closed(_ window: UUID) {
        open.remove(window)
        guard open.isEmpty else { return }
        Task {
            await settle()
            if open.isEmpty { apply(.accessory) }
        }
    }
}

extension View {
    /// Counts the window this view is the root of among the app's open windows.
    func appWindow(_ presence: AppPresence) -> some View {
        modifier(AppWindow(presence: presence))
    }
}

private struct AppWindow: ViewModifier {
    let presence: AppPresence
    @State private var id = UUID()

    func body(content: Content) -> some View {
        content
            .onAppear { presence.opened(id) }
            .onDisappear { presence.closed(id) }
    }
}
