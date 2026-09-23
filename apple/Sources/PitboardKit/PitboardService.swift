import Foundation
@_exported import PitboardBindings

/// What the app asks of pitboard. A protocol so a test can answer instead of the real
/// core, which would read the real keychain of whoever is running the tests.
public protocol Core: Sendable {
    /// Every account of every tool, each asked of its own tool's service.
    func status(fresh: Bool) async throws -> Status
    /// The last numbers pitboard measured, and who each tool's own files say is signed in.
    /// No network and no keychain, so it answers at once and works on a plane.
    func statusOffline() async throws -> Status
    func doctor() async -> Diagnosis
    /// Takes a label with its tool, as `Account.qualified` gives it, which names exactly one
    /// account whatever else is enrolled.
    func switchTo(_ label: String) async throws -> Switched
    /// A label with its tool, `codex/work`, is enrolled for that tool; a bare one means
    /// Claude Code.
    func enrollCurrent(_ label: String) async throws -> Enrolled
    func forget(_ label: String) async throws -> Changed
    func rename(_ from: String, to: String) async throws -> Changed
    /// Starts the tool's own sign-in for a new account, watched rather than handed to a
    /// terminal. The label says which tool, as in `codex/work`; a bare one means Claude Code.
    func signIn(_ label: String) async throws -> SignIn
    /// Give up on an interrupted switch that cannot be finished, keeping every login it
    /// names. Nil when there was none. The way out when recovery cannot reach the tool's
    /// service, which used to send the person to a terminal.
    func abandonRecovery() async throws -> Abandoned?
    /// What pitboard has changed, newest last.
    func log(limit: UInt32) async -> [Change]
    /// Renew every parked login that is due, and nothing else.
    func renew() async -> [Renewed]
    /// Whether anything keeps parked logins alive without a command being run.
    func schedule() async -> Schedule
    func scheduleInstall() async throws -> String
    func scheduleUninstall() async throws -> Bool
    /// When pitboard's account index last changed, in epoch seconds. One stat of one file,
    /// so it can be asked often: it is how this app notices a switch typed in a terminal.
    func changedAt() async -> Int64
    /// Every tool pitboard handles, in the order a listing shows them. Asks nothing of
    /// anyone.
    func tools() -> [Tool]
    /// The tools whose program was found where an app can look for one, in the same order.
    /// A tool missing here may still be on the `PATH`, so this narrows what is offered and
    /// never forbids anything.
    func installed() -> [Tool]
}

/// pitboard's core, called off the main thread. Any call may wait on the keychain, a lock or
/// the network, so reads run on one queue and changes on another, one change at a time.
public final class PitboardService: Core, Sendable {
    private let core: Pitboard
    private let reads = DispatchQueue(label: "com.usepitboard.reads")
    private let changes = DispatchQueue(label: "com.usepitboard.changes")
    /// The codes of the tools a program was given for, which is what `installed` answers.
    private let found: Set<String>

    public init(settings: Settings) {
        core = Pitboard(settings: settings)
        found = Set(
            [("claude", settings.claudeProgram), ("codex", settings.codexProgram)]
                .compactMap { code, program in program == nil ? nil : code })
    }

    public func tools() -> [Tool] {
        PitboardBindings.tools()
    }

    public func installed() -> [Tool] {
        tools().filter { found.contains($0.code) }
    }

    /// `fresh` asks each tool's service about every account even if it was asked moments
    /// ago. Pass
    /// false for a poll: an account is otherwise only asked about again once its tightest
    /// limit could have moved by a percentage point, which is what keeps this app and the
    /// command line to one request between them.
    public func status(fresh: Bool) async throws -> Status {
        try await run(on: reads) { try $0.status(fresh: fresh) }
    }

    public func statusOffline() async throws -> Status {
        try await run(on: reads) { try $0.statusOffline() }
    }

    public func abandonRecovery() async throws -> Abandoned? {
        try await run(on: changes) { try $0.abandonRecovery() }
    }

    public func log(limit: UInt32) async -> [Change] {
        (try? await run(on: reads) { $0.log(limit: limit) }) ?? []
    }

    public func renew() async -> [Renewed] {
        (try? await run(on: changes) { $0.renew() }) ?? []
    }

    public func schedule() async -> Schedule {
        (try? await run(on: reads) { $0.schedule() }) ?? .unsupported
    }

    public func scheduleInstall() async throws -> String {
        try await run(on: changes) { try $0.scheduleInstall() }
    }

    public func scheduleUninstall() async throws -> Bool {
        try await run(on: changes) { try $0.scheduleUninstall() }
    }

    /// One stat of one file. Deliberately not on the `changes` queue: it must answer while
    /// a switch is in flight, which is exactly when something has changed.
    public func changedAt() async -> Int64 {
        (try? await run(on: reads) { $0.changedAt() }) ?? 0
    }

    public func doctor() async -> Diagnosis {
        // `doctor` does not throw, so the only failure is the queue's, which cannot happen.
        (try? await run(on: reads) { $0.doctor() }) ?? Diagnosis(checks: [], healthy: false)
    }

    public func switchTo(_ label: String) async throws -> Switched {
        try await run(on: changes) { try $0.switchTo(label: label) }
    }

    /// Records the account signed in now. The other kind of enrolment opens a browser, and
    /// is `signIn`, which hands back what the tool says rather than printing it.
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
    /// What the core would read from a shell, as far as an app can see it. An app opened
    /// from Finder inherits none of a shell's exports, so these are usually absent and the
    /// defaults apply; when one is set, reading it is what keeps the app and the command
    /// line looking at the same keychain item and the same files.
    public static func forCurrentUser() -> Settings {
        let environment = ProcessInfo.processInfo.environment
        let home = environment["HOME"] ?? FileManager.default.homeDirectoryForCurrentUser.path
        // Finder's PATH has none of the places a tool installs itself, so each is looked for
        // where its installers put it, after the variable that names it outright.
        func find(_ program: String, unless variable: String) -> String? {
            environment[variable]
                ?? [
                    "\(home)/.local/bin/\(program)",
                    "/opt/homebrew/bin/\(program)",
                    "/usr/local/bin/\(program)",
                ].first { FileManager.default.isExecutableFile(atPath: $0) }
        }
        return Settings(
            home: home,
            pitboardHome: environment["PITBOARD_HOME"],
            claudeConfigDir: environment["CLAUDE_CONFIG_DIR"],
            secureStorageDir: environment["CLAUDE_SECURESTORAGE_CONFIG_DIR"],
            user: environment["USER"] ?? NSUserName(),
            claudeProgram: find("claude", unless: "PITBOARD_CLAUDE"),
            codexHome: environment["CODEX_HOME"],
            codexProgram: find("codex", unless: "PITBOARD_CODEX")
        )
    }
}
