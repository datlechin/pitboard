import Foundation
@_exported import PitboardBindings

/// What the app asks of pitboard. A protocol so a test can answer instead of the real
/// core, which would read the real keychain of whoever is running the tests.
public protocol Core: Sendable {
    func status() async throws -> Status
    func doctor() async -> Diagnosis
    func switchTo(_ label: String) async throws -> Switched
    func rename(_ from: String, to: String) async throws -> Changed
}

/// pitboard's core, called off the main thread. Any call may wait on the keychain, a lock or
/// the network, so reads run on one queue and changes on another, one change at a time.
public final class PitboardService: Core, Sendable {
    private let core: Pitboard
    private let reads = DispatchQueue(label: "com.usepitboard.reads")
    private let changes = DispatchQueue(label: "com.usepitboard.changes")

    public init(settings: Settings) {
        core = Pitboard(settings: settings)
    }

    public func status() async throws -> Status {
        try await run(on: reads) { try $0.status() }
    }

    public func doctor() async -> Diagnosis {
        // `doctor` does not throw, so the only failure is the queue's, which cannot happen.
        (try? await run(on: reads) { $0.doctor() }) ?? Diagnosis(checks: [], healthy: false)
    }

    public func switchTo(_ label: String) async throws -> Switched {
        try await run(on: changes) { try $0.switchTo(label: label) }
    }

    public func rename(_ from: String, to: String) async throws -> Changed {
        try await run(on: changes) { try $0.rename(from: from, to: to) }
    }

    private func run<T: Sendable>(
        on queue: DispatchQueue,
        _ work: @escaping @Sendable (Pitboard) throws -> T
    ) async throws -> T {
        let core = self.core
        return try await withCheckedThrowingContinuation { continuation in
            queue.async { continuation.resume(with: Result { try work(core) }) }
        }
    }
}

extension Settings {
    /// Claude Code's defaults for the person running this app. An app started from Finder sees
    /// no shell environment, so `claude` is looked for where its installers put it.
    /// What the core would read from a shell, as far as an app can see it. An app opened
    /// from Finder inherits none of a shell's exports, so these are usually absent and the
    /// defaults apply; when one is set, reading it is what keeps the app and the command
    /// line looking at the same keychain item.
    public static func forCurrentUser() -> Settings {
        let environment = ProcessInfo.processInfo.environment
        let home = environment["HOME"] ?? FileManager.default.homeDirectoryForCurrentUser.path
        let claude =
            environment["PITBOARD_CLAUDE"]
            ?? [
                "\(home)/.local/bin/claude",
                "/opt/homebrew/bin/claude",
                "/usr/local/bin/claude",
            ].first { FileManager.default.isExecutableFile(atPath: $0) }
        return Settings(
            home: home,
            pitboardHome: environment["PITBOARD_HOME"],
            claudeConfigDir: environment["CLAUDE_CONFIG_DIR"],
            secureStorageDir: environment["CLAUDE_SECURESTORAGE_CONFIG_DIR"],
            user: environment["USER"] ?? NSUserName(),
            claudeProgram: claude
        )
    }
}
