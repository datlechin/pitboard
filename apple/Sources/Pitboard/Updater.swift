import AppKit
import Foundation
import Observation
import Sparkle
import SwiftUI

/// Updates, when the build was made to receive them. A build from a clone carries no update
/// key, and Sparkle will not run without one, so such a build has no updater and says
/// nothing about updates.
///
/// An app with no Dock icon has nowhere to put a window nobody asked for, so Sparkle is told
/// this one handles scheduled updates gently: the panel says a version is ready, and the
/// person decides when to stop what they are doing.
@MainActor
@Observable
final class Updater: NSObject, SPUStandardUserDriverDelegate {
    /// The version waiting, once one is, for the panel to offer.
    private(set) var waiting: String?

    @ObservationIgnored private var controller: SPUStandardUpdaterController?

    override init() {
        super.init()
        guard Bundle.main.object(forInfoDictionaryKey: "SUPublicEDKey") != nil else { return }
        controller = SPUStandardUpdaterController(
            startingUpdater: true, updaterDelegate: nil, userDriverDelegate: self)
    }

    var available: Bool { controller != nil }

    /// Sparkle's own settings, bound so a Settings pane can show them. An app with no Dock
    /// icon has no menu bar to put Sparkle's own checkbox in, so these are the only place
    /// they can be.
    var checksAutomatically: Binding<Bool> {
        Binding(
            get: { self.controller?.updater.automaticallyChecksForUpdates ?? false },
            set: { self.controller?.updater.automaticallyChecksForUpdates = $0 }
        )
    }

    var installsAutomatically: Binding<Bool> {
        Binding(
            get: { self.controller?.updater.automaticallyDownloadsUpdates ?? false },
            set: { self.controller?.updater.automaticallyDownloadsUpdates = $0 }
        )
    }

    /// Shows Sparkle's own window: what it finds, what changed, and the install button.
    func check() {
        waiting = nil
        controller?.updater.checkForUpdates()
    }

    nonisolated var supportsGentleScheduledUpdateReminders: Bool { true }

    nonisolated func standardUserDriver(
        _ driver: SPUStandardUserDriver,
        willShowModalAlert alert: NSAlert
    ) {
        // A modal alert from an app with no Dock icon arrives behind everything otherwise.
        Task { @MainActor in NSApp.activate(ignoringOtherApps: true) }
    }

    nonisolated func standardUserDriverWillHandleShowingUpdate(
        _ handleShowingUpdate: Bool,
        forUpdate update: SUAppcastItem,
        state: SPUUserUpdateState
    ) {
        guard !state.userInitiated else { return }
        let version = update.displayVersionString
        Task { @MainActor in self.waiting = version }
    }
}
