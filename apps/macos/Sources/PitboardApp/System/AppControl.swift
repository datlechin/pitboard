import AppKit

/// Another app on this Mac, by its bundle id, that Pitboard may quit and open again around a
/// switch: one that runs a tool for itself and keeps the tool's login in memory while it is
/// open, as ChatGPT does with Codex's.
///
/// A protocol so a test drives the whole quit, switch and reopen without touching a real
/// app. Only quitting the way Command-Q does and opening are offered: never a forced quit,
/// which would lose whatever the app had not saved.
@MainActor
public protocol AppControl: AnyObject {
    /// Where the running copy of the app was opened from, or nil when it is not running. An
    /// app is a bundle, so a running one always says where it is.
    func running(_ bundleID: String) -> URL?
    /// Where macOS would open the app from, running or not, or nil when it knows of no copy.
    func installed(_ bundleID: String) -> URL?
    /// Asks the app to quit the way Command-Q does, which lets it ask about work in
    /// progress. Returns at once; the app may take a while, or decline.
    func requestQuit(_ bundleID: String)
    /// Opens the app at `url` the way Finder would. Brought to the front only when `inFront`:
    /// otherwise whatever Pitboard has to say about the switch stays in front of it.
    func open(_ url: URL, inFront: Bool)
}

/// What came of asking an app to quit.
enum QuitOutcome: Equatable {
    /// It quit. The copy that was running is the one to open again.
    case quit(URL)
    /// It was not running, so there was nothing to quit and there is nothing to open.
    case notRunning
    /// It is still running, as it was.
    case stillRunning
}

extension AppControl {
    /// Asks the app to quit and waits until it has, for as long as `limit`.
    func quit(
        _ bundleID: String, within limit: Duration,
        checkingEvery interval: Duration = .milliseconds(200)
    ) async -> QuitOutcome {
        guard let copy = running(bundleID) else { return .notRunning }
        requestQuit(bundleID)
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: limit)
        while running(bundleID) != nil {
            guard clock.now < deadline else { return .stillRunning }
            try? await Task.sleep(for: interval)
        }
        return .quit(copy)
    }
}

/// The apps running on this Mac, as macOS's own list of them has it.
@MainActor
public final class WorkspaceAppControl: AppControl {
    public init() {}

    private func apps(_ bundleID: String) -> [NSRunningApplication] {
        NSRunningApplication.runningApplications(withBundleIdentifier: bundleID)
            .filter { !$0.isTerminated }
    }

    public func running(_ bundleID: String) -> URL? {
        apps(bundleID).lazy.compactMap(\.bundleURL).first
    }

    public func installed(_ bundleID: String) -> URL? {
        NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleID)
    }

    public func requestQuit(_ bundleID: String) {
        for app in apps(bundleID) { app.terminate() }
    }

    public func open(_ url: URL, inFront: Bool) {
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.activates = inFront
        NSWorkspace.shared.openApplication(at: url, configuration: configuration)
    }
}
