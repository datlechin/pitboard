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
    /// Point a schedule an app up to 0.3.0 wrote, which runs that app and renews nothing, at
    /// the command line inside this one. True when it did; nothing changes otherwise.
    func scheduleRepair() async throws -> Bool
    /// When pitboard's account index last changed, in epoch seconds. One stat of one file,
    /// so it can be asked often: it is how this app notices a switch typed in a terminal.
    func changedAt() async -> Int64
    /// Every tool pitboard handles, in the order a listing shows them. Asks nothing of
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
}

/// pitboard's core, called off the main thread. Any call may wait on the keychain, a lock or
/// the network, so reads run on one queue and changes on another, one change at a time.
///
/// The core itself is made on first use, by whichever of those gets there first: working
/// out where each tool is installed can mean asking the person's login shell, and nothing
/// on the main thread may wait for that.
public final class PitboardService: Core, Sendable {
    private let made: Kept<Made>
    /// How long after a login shell too slow to answer it is asked once more.
    private let askAgainAfter: TimeInterval
    private let reads = DispatchQueue(label: "com.usepitboard.reads")
    private let changes = DispatchQueue(label: "com.usepitboard.changes")

    /// What this service makes once and keeps.
    private struct Made: Sendable {
        let core: Pitboard
        /// The codes of the tools a program was given for, which is what `installed`
        /// answers.
        let found: Set<String>
        /// The login shell's `PATH` as far as it was looked in, which `searchPath` answers.
        let searchPath: String?
    }

    /// `settings` is asked for once, on first use and off the main thread, so it may take
    /// its time.
    public convenience init(settings: @escaping @Sendable () -> Settings) {
        self.init(asking: { (settings(), false) })
    }

    public convenience init(settings: Settings) {
        self.init { settings }
    }

    /// `settings` says as well whether the login shell it asked was too slow to answer, as
    /// `Settings.forCurrentUserAsked` does. What it made then stands, and it is asked once
    /// more when something next asks what is installed or starts a sign-in, `askAgainAfter`
    /// seconds on: startup files are slowest while the machine is busy logging in, which is
    /// when an app that opens at login first asks. Once more and no more, because a shell
    /// that is always that slow would otherwise cost five seconds every time.
    public init(
        asking settings: @escaping @Sendable () -> (settings: Settings, late: Bool),
        askAgainAfter: TimeInterval = 60
    ) {
        self.askAgainAfter = askAgainAfter
        made = Kept {
            let (settings, late) = settings()
            let made = Made(
                core: Pitboard(settings: settings),
                found: Set(
                    [("claude", settings.claudeProgram), ("codex", settings.codexProgram)]
                        .compactMap { code, program in program == nil ? nil : code }),
                searchPath: settings.searchPath)
            return (made, late)
        }
    }

    public func tools() -> [Tool] {
        PitboardBindings.tools()
    }

    public func installed() async -> [Tool] {
        let found = await looked().found
        return tools().filter { found.contains($0.code) }
    }

    public func searchPath() async -> String? {
        await looked().searchPath
    }

    /// What finding the programs came to. Not on `reads`, where a read may be waiting on the
    /// network: what is installed is needed before the first read can say anything useful
    /// about it.
    private func looked() async -> Made {
        let (made, after) = (self.made, askAgainAfter)
        return await withCheckedContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                continuation.resume(returning: made.value(askingAgainAfter: after))
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
            let (made, after) = (self.made, askAgainAfter)
            DispatchQueue(label: "com.usepitboard.signin").async {
                continuation.resume(
                    with: Result {
                        try made.value(askingAgainAfter: after).core.signIn(label: label)
                    })
            }
        }
    }

    private func run<T: Sendable>(
        on queue: DispatchQueue,
        _ work: @escaping @Sendable (Pitboard) throws -> T
    ) async throws -> T {
        let made = self.made
        return try await withCheckedThrowingContinuation { continuation in
            queue.async { continuation.resume(with: Result { try work(made.value().core) }) }
        }
    }
}

/// A value made the first time it is asked for, by whichever thread asks first. Any other
/// thread asking meanwhile waits for that one rather than making a second.
///
/// One made from an answer that came too late is made once more when asked to, later, and
/// kept in its place if that answers: a caller that asks meanwhile gets what was made first
/// rather than waiting on the second ask.
private final class Kept<Value: Sendable>: @unchecked Sendable {
    // Unchecked because `made` and `askedAgain` are written after they are shared; every
    // read and write of them holds `lock`.
    private let lock = NSLock()
    private var made: (value: Value, late: Bool, at: Date)?
    private var askedAgain = false
    private let make: @Sendable () -> (Value, late: Bool)

    init(_ make: @escaping @Sendable () -> (Value, late: Bool)) {
        self.make = make
    }

    func value() -> Value {
        lock.withLock {
            if let made { return made.value }
            let (value, late) = make()
            made = (value, late, Date())
            return value
        }
    }

    /// The value, made once more first when what it was made from came too late and at
    /// least `seconds` have passed since.
    func value(askingAgainAfter seconds: TimeInterval) -> Value {
        let first = value()
        let due = lock.withLock {
            guard let made, made.late, !askedAgain,
                Date().timeIntervalSince(made.at) >= seconds
            else { return false }
            askedAgain = true
            return true
        }
        guard due else { return value() }
        let (again, late) = make()
        guard !late else { return first }
        lock.withLock { made = (again, false, Date()) }
        return again
    }
}

extension Settings {
    /// What the core would read from a shell, as far as an app can see it. An app opened
    /// from Finder inherits none of a shell's exports, so these are usually absent and the
    /// defaults apply; when one is set, reading it is what keeps the app and the command
    /// line looking at the same keychain item and the same files.
    ///
    /// Asks the person's login shell for its `PATH`, which can take a second or more, so this
    /// is never called on the main thread: `PitboardService` asks for it on first use.
    public static func forCurrentUser() -> Settings {
        forCurrentUserAsked().settings
    }

    /// `forCurrentUser`, and whether the login shell was too slow to answer, which asking
    /// again later may not be.
    public static func forCurrentUserAsked() -> (settings: Settings, late: Bool) {
        let environment = ProcessInfo.processInfo.environment
        let asked = LoginShell.path(environment: environment)
        let settings = forCurrentUser(
            environment: environment,
            loginPath: asked.path,
            bundle: Bundle.main.bundleURL,
            isExecutable: FileManager.default.isExecutableFile(atPath:))
        return (settings, asked.late)
    }

    /// `forCurrentUser` with what it reads from the machine handed in, so a test can say
    /// what the login shell answered without starting one, and which bundle is running.
    ///
    /// Each tool's program is looked for first where the variable naming it outright says,
    /// then on the login shell's `PATH`, where a version manager or an npm prefix puts it,
    /// and then where each tool's own installer puts it, which is all there is when the
    /// shell could not be asked. The login shell's `PATH`, as far as it is looked in here,
    /// is also where the core looks and what a sign-in is given; without one, the core looks
    /// on this app's own, as it always did.
    ///
    /// The renewal schedule runs the command line inside the app `bundle`, since the app
    /// has no renewal of its own and only hands `renew` on to that. It has no default: a
    /// caller that left it out would still compile, and the app would have nothing to
    /// schedule.
    static func forCurrentUser(
        environment: [String: String],
        loginPath: String?,
        bundle: URL?,
        isExecutable: (String) -> Bool
    ) -> Settings {
        let home = environment["HOME"] ?? FileManager.default.homeDirectoryForCurrentUser.path
        // A relative entry would be looked for wherever this app happens to be running, and
        // looking inside a folder macOS guards asks the person whether pitboard may, for
        // something it never needed to read.
        let shell = (loginPath ?? "").split(separator: ":").map(String.init)
            .filter { $0.hasPrefix("/") && !guarded($0, home: home) }
        let places = shell + ["\(home)/.local/bin", "/opt/homebrew/bin", "/usr/local/bin"]
        func find(_ program: String, unless variable: String) -> String? {
            environment[variable]
                ?? places.lazy.map { "\($0)/\(program)" }.first(where: isExecutable)
        }
        return Settings(
            home: home,
            pitboardHome: environment["PITBOARD_HOME"],
            claudeConfigDir: environment["CLAUDE_CONFIG_DIR"],
            secureStorageDir: environment["CLAUDE_SECURESTORAGE_CONFIG_DIR"],
            user: environment["USER"] ?? NSUserName(),
            claudeProgram: find("claude", unless: "PITBOARD_CLAUDE"),
            codexHome: environment["CODEX_HOME"],
            codexProgram: find("codex", unless: "PITBOARD_CODEX"),
            searchPath: loginPath.map { _ in shell.joined(separator: ":") },
            scheduleProgram: bundle.flatMap(bundledCommandLine(in:))
        )
    }

    /// The command line an app bundle comes with. Nil for anything that is not an app, such
    /// as a test or `swift run` in a build directory, which has none: that app cannot
    /// schedule renewal, since the only other thing to schedule is the app itself, which
    /// renews nothing.
    public static func bundledCommandLine(in bundle: URL) -> String? {
        guard bundle.pathExtension == "app" else { return nil }
        return bundle.appendingPathComponent("Contents/Helpers/pitboard").path
    }

    /// Whether `entry` is inside a folder macOS asks the person about before an app may read
    /// it. A program started from there would have the same question asked on its behalf.
    static func guarded(_ entry: String, home: String) -> Bool {
        [
            "Desktop", "Documents", "Downloads", "Library/Mobile Documents",
            "Library/CloudStorage",
        ]
        .map { "\(home)/\($0)" }
        .contains { entry == $0 || entry.hasPrefix("\($0)/") }
    }
}
