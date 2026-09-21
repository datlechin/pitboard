import Foundation
@_exported import PitboardBindings

/// pitboard's core, called off the main thread. Any call may wait on the keychain, a lock or
/// the network, so reads run on one queue and changes on another, one change at a time.
public final class PitboardService: Sendable {
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

    public func enrollCurrent(_ label: String) async throws -> Enrolled {
        try await run(on: changes) { try $0.enrollCurrent(label: label) }
    }

    public func forget(_ label: String) async throws -> Changed {
        try await run(on: changes) { try $0.forget(label: label) }
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
    public static func forCurrentUser() -> Settings {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let claude = [
            "\(home)/.local/bin/claude",
            "/opt/homebrew/bin/claude",
            "/usr/local/bin/claude",
        ].first { FileManager.default.isExecutableFile(atPath: $0) }
        return Settings(
            home: home,
            pitboardHome: nil,
            claudeConfigDir: nil,
            secureStorageDir: nil,
            user: NSUserName(),
            claudeProgram: claude
        )
    }
}
