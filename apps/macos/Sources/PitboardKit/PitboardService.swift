import Foundation
@_exported import PitboardBindings

/// What the app asks of Pitboard. A protocol so a test can answer instead of the real
/// core, which would read the real keychain of whoever is running the tests.
public protocol Core: Sendable {
    /// Every account of every tool, each asked of its own tool's service.
    func status(fresh: Bool) async throws -> Status
    /// The last numbers Pitboard measured, and who each tool's own files say is signed in.
    /// No network and no keychain, so it answers at once and works on a plane.
    func statusOffline() async throws -> Status
    func doctor() async -> Diagnosis
    /// What is running a tool with a login a switch would leave it on, by kind, from the
    /// process list alone. The tool is a `Tool`'s code; empty for a tool that follows a
    /// switch by itself, and wherever nothing is running.
    func holding(_ provider: String) async -> [Holding]
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
    /// What Pitboard has changed, newest last.
    func log(limit: UInt32) async -> [Change]
    /// Renew every parked login that is due, and nothing else.
    func renew() async -> [Renewed]
    /// Whether anything keeps parked logins alive without a command being run.
    func schedule() async -> Schedule
    func scheduleInstall() async throws -> String
    func scheduleUninstall() async throws -> Bool
    /// Point a schedule an app up to 0.3.0 wrote, which runs that app and renews nothing, at
    /// the command line inside this one. True when it did; nothing changes otherwise.
    func scheduleRepair() async throws -> Bool
    /// When Pitboard's account index last changed, in epoch seconds. One stat of one file,
    /// so it can be asked often: it is how this app notices a switch typed in a terminal.
    func changedAt() async -> Int64
    /// When Pitboard's usage readings last changed, in epoch milliseconds. One stat of one
    /// file, like `changedAt`: it is how this app follows the numbers every session's status
    /// line records.
    func readingsChangedAt() async -> Int64
    /// Every tool Pitboard handles, in the order a listing shows them. Asks nothing of
    /// anyone.
    func tools() -> [Tool]
    /// The tools whose program was found where an app can look for one, in the same order.
    /// A tool missing here may still be on the `PATH`, so this narrows what is offered and
    /// never forbids anything. Finding them can mean asking the person's login shell, which
    /// takes a moment, so nothing waits on it on the main thread.
    func installed() async -> [Tool]
    /// Where programs are looked for, in `PATH`'s form: the person's login shell's, as far
    /// as the app looks in it. Nil when the shell could not be asked. Asked the same way and
    /// as rarely as `installed`, and for the same reason.
    func searchPath() async -> String?
    /// Puts the account the tool is signed in to aside, as a switch would, and leaves the
    /// tool signed out so the person can sign in to another one there. Only Claude Desktop
    /// does this, and only while it is not running; the tool is a `Tool`'s code.
    func switchToSignedOut(_ provider: String) async throws -> Switched
    /// The Claude Desktop account put aside for a sign-in that has not been enrolled yet,
    /// or nil. Read from Pitboard's own files, so it answers at once.
    func awaitingSignIn() async -> Awaiting?
    /// Whether live usage for Claude Desktop is on, and whether macOS lets Pitboard read
    /// what it needs. Never asks the keychain: it reports what the last reading found.
    func liveUsage() async -> LiveUsageState
    /// Turns live usage on. The only call that may lead macOS to ask the person about the
    /// keychain, so the app makes it only from the sheet that says so.
    func enableLiveUsage() async throws -> LiveUsageState
    func disableLiveUsage() async throws -> LiveUsageState
}

/// Pitboard's core, called off the main thread. Any call may wait on the keychain, a lock or
/// the network, so reads run on one queue and changes on another, one change at a time.
///
/// The core makes what it reads from its environment on first use, by whichever call gets
/// there first: working out where each tool is installed can mean asking the person's login
/// shell, and nothing on the main thread may wait for that.
public final class PitboardService: Core, Sendable {
    private let core: Pitboard
    private let reads = DispatchQueue(label: "com.usepitboard.reads")
    private let changes = DispatchQueue(label: "com.usepitboard.changes")
    /// Lists processes and nothing else, so it never waits behind a read on the network.
    private let processes = DispatchQueue(label: "com.usepitboard.processes")

    /// Making `core` asks nothing of anyone, so this may be called on the main thread.
    public init(core: Pitboard) {
        self.core = core
    }

    public convenience init(settings: Settings) {
        self.init(core: Pitboard(settings: settings))
    }

    /// This app's core: what it was started with, read the way the command line reads its
    /// own environment, and where it is, which names the command line its schedule runs. The
    /// login shell is asked for its `PATH` on first use, off the main thread, and once more
    /// a minute or more later if it was too slow to answer.
    public static func forThisApp(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        bundle: URL = Bundle.main.bundleURL
    ) -> PitboardService {
        PitboardService(core: .forApp(environment: environment, app: bundle.path))
    }

    public func tools() -> [Tool] {
        PitboardBindings.tools()
    }

    public func installed() async -> [Tool] {
        await looked { $0.installed() }
    }

    public func searchPath() async -> String? {
        await looked { $0.searchPath() }
    }

    /// What finding the programs came to. Not on `reads`, where a read may be waiting on the
    /// network: what is installed is needed before the first read can say anything useful
    /// about it.
    private func looked<T: Sendable>(_ work: @escaping @Sendable (Pitboard) -> T) async -> T {
        let core = self.core
        return await withCheckedContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                continuation.resume(returning: work(core))
            }
        }
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

    public func scheduleRepair() async throws -> Bool {
        try await run(on: changes) { try $0.scheduleRepair() }
    }

    /// One stat of one file. Deliberately not on the `changes` queue: it must answer while
    /// a switch is in flight, which is exactly when something has changed.
    public func changedAt() async -> Int64 {
        (try? await run(on: reads) { $0.changedAt() }) ?? 0
    }

    public func readingsChangedAt() async -> Int64 {
        (try? await run(on: reads) { $0.readingsChangedAt() }) ?? 0
    }

    public func doctor() async -> Diagnosis {
        // `doctor` does not throw, so the only failure is the queue's, which cannot happen.
        (try? await run(on: reads) { $0.doctor() }) ?? Diagnosis(checks: [], healthy: false)
    }

    public func holding(_ provider: String) async -> [Holding] {
        (try? await run(on: processes) { $0.holding(provider: provider) }) ?? []
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
        try await run(on: DispatchQueue(label: "com.usepitboard.signin")) {
            try $0.signIn(label: label)
        }
    }

    public func switchToSignedOut(_ provider: String) async throws -> Switched {
        try await run(on: changes) { try $0.switchToSignedOut(tool: provider) }
    }

    public func awaitingSignIn() async -> Awaiting? {
        (try? await run(on: reads) { $0.awaitingSignIn() }) ?? nil
    }

    public func liveUsage() async -> LiveUsageState {
        (try? await run(on: reads) { $0.liveUsage() })
            ?? LiveUsageState(enabled: false, approval: "unknown", reason: nil, lastOkAt: nil)
    }

    public func enableLiveUsage() async throws -> LiveUsageState {
        try await run(on: changes) { try $0.enableLiveUsage() }
    }

    public func disableLiveUsage() async throws -> LiveUsageState {
        try await run(on: changes) { try $0.disableLiveUsage() }
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
    /// The command line an app bundle comes with, as the core says where it is. Nil for
    /// anything that is not an app, such as a test or `swift run` in a build directory, which
    /// has none: that app cannot schedule renewal, since the only other thing to schedule is
    /// the app itself, which renews nothing.
    public static func bundledCommandLine(in bundle: URL) -> String? {
        appCommandLine(app: bundle.path)
    }
}
