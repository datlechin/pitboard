import Foundation
@_exported import PitboardBindings

/// What the app asks of pitboard. A protocol so a test can answer instead of the real
/// core, which would read the real keychain of whoever is running the tests.
public protocol Core: Sendable {
    func status(fresh: Bool) async throws -> Status
    func doctor() async -> Diagnosis
    func switchTo(_ label: String) async throws -> Switched
    func enrollCurrent(_ label: String) async throws -> Enrolled
    func forget(_ label: String) async throws -> Changed
    func rename(_ from: String, to: String) async throws -> Changed
    /// Starts Claude Code's own sign-in for a new account, watched rather than handed to a
    /// terminal. Returns nil where a front end cannot run one.
    func signIn(_ label: String) async throws -> SignIn
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

    /// `fresh` asks Anthropic about every account even if it was asked moments ago. Pass
    /// false for a poll: an account is otherwise only asked about again once its tightest
    /// limit could have moved by a percentage point, which is what keeps this app and the
    /// command line to one request between them.
    public func status(fresh: Bool) async throws -> Status {
        try await run(on: reads) { try $0.status(fresh: fresh) }
    }

    public func doctor() async -> Diagnosis {
        // `doctor` does not throw, so the only failure is the queue's, which cannot happen.
        (try? await run(on: reads) { $0.doctor() }) ?? Diagnosis(checks: [], healthy: false)
    }

    public func switchTo(_ label: String) async throws -> Switched {
        try await run(on: changes) { try $0.switchTo(label: label) }
    }

    /// Records the account signed in now. The other kind of enrolment opens a browser and
    /// belongs to the command line, which has somewhere to print what Claude Code says.
    public func enrollCurrent(_ label: String) async throws -> Enrolled {
        try await run(on: changes) { try $0.enrollCurrent(label: label) }
    }

    public func forget(_ label: String) async throws -> Changed {
        try await run(on: changes) { try $0.forget(label: label) }
    }

    public func rename(_ from: String, to: String) async throws -> Changed {
        try await run(on: changes) { try $0.rename(from: from, to: to) }
    }

    public func signIn(_ label: String) async throws -> SignIn {
        // Its own queue: this waits on a person in a browser, and a read or a change must
        // not queue behind that.
        try await withCheckedThrowingContinuation { continuation in
            let core = self.core
            DispatchQueue(label: "com.usepitboard.signin").async {
                continuation.resume(with: Result { try core.signIn(label: label) })
            }
        }
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
