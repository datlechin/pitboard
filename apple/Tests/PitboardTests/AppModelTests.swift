import Foundation
import PitboardKit
import Testing

@testable import Pitboard

/// Answers whatever a test wants, so the model can be driven through states a real machine
/// would take a keychain and a network to reach.
private final class Stub: Core, @unchecked Sendable {
    var answer: Result<Status, Error>
    var switched: Result<Switched, Error> = .success(
        Switched(outcome: .alreadyActive(label: "work"), warnings: []))
    private(set) var switchedTo: [String] = []
    private(set) var enrolled: [String] = []
    private(set) var forgot: [String] = []
    private(set) var signedIn: [String] = []
    /// What the next sign-in hands back; nil fails it the way a missing program does.
    var session: SignIn?
    /// The tools whose program the app found.
    var found: [Tool] = [claudeCode]
    private(set) var installedAsks = 0
    var enrolling: Result<Enrolled, Error> = .success(
        Enrolled(email: "a@b.c", enrolled: .current, warnings: []))
    /// What a sign-in that cannot start warns about beside its refusal.
    var signInWarnings: [Warning] = []
    /// The login shell's `PATH`, as far as the app looks in it.
    var path: String?

    init(_ answer: Result<Status, Error>) {
        self.answer = answer
    }

    private(set) var freshAsks = 0
    var offline: Result<Status, Error> = .success(Status(now: 0, accounts: [], warnings: []))
    var changed: Int64 = 0
    private(set) var offlineReads = 0
    var abandoned: Abandoned?

    func status(fresh: Bool) async throws -> Status {
        if fresh { freshAsks += 1 }
        return try answer.get()
    }
    func statusOffline() async throws -> Status {
        offlineReads += 1
        return try offline.get()
    }
    func abandonRecovery() async throws -> Abandoned? { abandoned }
    func log(limit: UInt32) async -> [Change] { [] }
    func renew() async -> [Renewed] { [] }
    /// What the scheduler has installed.
    var scheduled: Schedule = .absent
    private(set) var scheduleReads = 0
    func schedule() async -> Schedule {
        scheduleReads += 1
        return scheduled
    }
    /// What repairing a schedule an older app wrote comes to.
    var repairs: Result<Bool, Error> = .success(false)
    private(set) var repairAsks = 0
    func scheduleRepair() async throws -> Bool {
        repairAsks += 1
        return try repairs.get()
    }
    private(set) var scheduleInstalls = 0
    private(set) var scheduleUninstalls = 0
    func scheduleInstall() async throws -> String {
        scheduleInstalls += 1
        return "/nowhere"
    }
    func scheduleUninstall() async throws -> Bool {
        scheduleUninstalls += 1
        return false
    }
    func changedAt() async -> Int64 { changed }
    func doctor() async -> Diagnosis {
        Diagnosis(
            checks: [
                Check(code: "state", name: "state", level: .ok, detail: "fine", advice: "")
            ],
            healthy: true)
    }
    func switchTo(_ label: String) async throws -> Switched {
        switchedTo.append(label)
        return try switched.get()
    }
    func enrollCurrent(_ label: String) async throws -> Enrolled {
        enrolled.append(label)
        return try enrolling.get()
    }
    func forget(_ label: String) async throws -> Changed {
        forgot.append(label)
        return Changed(email: "\(label)@example.com", warnings: [])
    }
    func rename(_ from: String, to: String) async throws -> Changed {
        Changed(email: "a@b.c", warnings: [])
    }
    func signIn(_ label: String) async throws -> SignIn {
        signedIn.append(label)
        guard let session else {
            throw PitboardError.Failed(
                code: "claude_program_missing", cause: nil,
                message: "`claude` is not on this machine",
                warnings: signInWarnings)
        }
        return session
    }
    func tools() -> [Tool] { bothTools }
    func installed() async -> [Tool] {
        installedAsks += 1
        return found
    }
    func searchPath() async -> String? { path }
}

/// A sign-in that says what it is given to say and then enrols, without a tool behind it.
///
/// `waits` holds it after its last line, the way a tool that has printed its address waits
/// on the browser, until it is cancelled or `done` is signalled.
private final class ScriptedSignIn: SignIn, @unchecked Sendable {
    private var lines: [String]
    private let code: Bool
    private let waits: Bool
    let done = DispatchSemaphore(value: 0)
    private(set) var pasted: [String] = []
    /// Whether each was called on the main thread, which is the app's to keep free.
    private(set) var pastedOnMain: Bool?
    private(set) var cancelledOnMain: Bool?
    private(set) var finished = false
    /// What finishing enrols.
    var enrolls = Enrolled(email: "a@b.c", enrolled: .signedIn, warnings: [])

    init(saying lines: [String], takesACode code: Bool, waits: Bool = false) {
        self.lines = lines
        self.code = code
        self.waits = waits
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) { fatalError("not from the core") }

    override func takesACode() -> Bool { code }
    override func nextLine() -> String? {
        guard lines.isEmpty else { return lines.removeFirst() }
        if waits { done.wait() }
        return nil
    }
    override func paste(line: String) throws {
        pastedOnMain = Thread.isMainThread
        pasted.append(line)
    }
    override func finish() throws -> Enrolled {
        finished = true
        return enrolls
    }
    override func cancel() {
        cancelledOnMain = Thread.isMainThread
        done.signal()
    }
}

/// Asks again every few milliseconds, for as long as a test can reasonably wait, for
/// something that happens on another thread.
@MainActor
private func eventually(_ condition: @MainActor () -> Bool) async -> Bool {
    for _ in 0..<500 {
        if condition() { return true }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return condition()
}

/// Defaults held in memory, so what a test declines is not what the next one reads, and
/// nothing is written to the Mac running the tests.
private final class MemoryDefaults: UserDefaults, @unchecked Sendable {
    private var values: [String: Any] = [:]

    override func object(forKey key: String) -> Any? { values[key] }
    override func set(_ value: Any?, forKey key: String) { values[key] = value }
    override func set(_ value: Bool, forKey key: String) { values[key] = value }
    override func removeObject(forKey key: String) { values[key] = nil }
    override func stringArray(forKey key: String) -> [String]? { values[key] as? [String] }
    override func bool(forKey key: String) -> Bool { values[key] as? Bool ?? false }
}

/// A switch of `provider`'s tool, as the core reports one: `from` and `to` typed the way
/// the core types them, bare for Claude Code and with the tool for any other.
private func switched(
    _ provider: String, from: String, to: String, warnings: [Warning] = []
) -> Result<Switched, Error> {
    let adoption: Adoption =
        provider == "codex" ? .restart(program: "codex") : .follows(withinSeconds: 33)
    return .success(
        Switched(
            outcome: .switched(provider: provider, from: from, to: to, adoption: adoption),
            warnings: warnings))
}

private let stillRunning = Warning(
    code: "sessions_still_running",
    message:
        "2 `codex` sessions started before this switch are still running and still using "
        + "`codex/personal`. Quit them and start again to use the new account. Quit rather "
        + "than signing out inside one: signing out there revokes `codex/personal`'s login, "
        + "which pitboard has just parked.")

private func account(_ label: String, signedIn: Bool, percent: Double) -> Account {
    account(label, signedIn: signedIn, [window("session", percent, resets: nil)])
}

@MainActor
@Test func aReadFillsInTheTitleAndTheRows() async {
    let model = AppModel(
        service: Stub(
            .success(
                Status(
                    now: 0,
                    accounts: [account("work", signedIn: true, percent: 42)],
                    warnings: []))))
    await model.refresh()
    #expect(model.title == "work 42%")
    #expect(model.status?.accounts.count == 1)
    #expect(model.problem == nil)
}

/// pitboard's errors already say what to do, so the panel shows the message as it is.
@MainActor
@Test func aFailedReadIsShownAsItsOwnMessage() async {
    let model = AppModel(
        service: Stub(
            .failure(
                PitboardError.Failed(
                    code: "state_wrong_machine", cause: nil,
                    message: "was written on another computer",
                    warnings: []))))
    await model.refresh()
    #expect(model.problem == "was written on another computer")
    #expect(
        model.status?.accounts.isEmpty == true,
        "the panel falls back to what is already known, which here is nothing")
}

/// A switch that failed must not read as one that worked.
@MainActor
@Test func aFailedSwitchSaysSoAndChangesNothing() async {
    let stub = Stub(.success(Status(now: 0, accounts: [], warnings: [])))
    stub.switched = .failure(
        PitboardError.Failed(
            code: "nothing_parked", cause: nil, message: "nothing parked", warnings: []))
    let model = AppModel(watching: false, service: stub)
    await model.use("work")
    #expect(stub.switchedTo == ["work"])
    #expect(model.problem == "nothing parked")
    #expect(model.lastSwitches.isEmpty)
}

/// After a switch the panel counts down to when open sessions follow.
@MainActor
@Test func aSwitchRecordsWhenOpenSessionsFollow() async {
    let stub = Stub(.success(status([account("work", signedIn: true), account("personal")])))
    stub.switched = switched("claude", from: "personal", to: "work")
    let model = AppModel(watching: false, service: stub)
    await model.use("claude/work")
    let left = model.lastSwitches.first?.adopted?.timeIntervalSinceNow ?? 0
    #expect(left > 30 && left <= 33)
    #expect(model.lastSwitches.first?.restart == nil)
}

/// A running `codex` never picks a switch up, so a countdown there would promise something
/// that does not happen. When the core counted the sessions still running, its warning says
/// so, with the count and with what not to do in them, and it outlives the read after the
/// switch, which would otherwise replace it. The app's own sentence would only say the same
/// thing again.
@MainActor
@Test func aSwitchThatNeedsARestartSaysSoAndCountsNothingDown() async throws {
    let stub = Stub(
        .success(status([account("work", of: "codex", signedIn: true), account("personal")])))
    stub.switched = switched(
        "codex", from: "codex/personal", to: "codex/work", warnings: [stillRunning])
    let model = AppModel(watching: false, service: stub)

    await model.use("codex/work")

    let last = try #require(model.lastSwitches.first)
    #expect(last.adopted == nil, "no countdown")
    #expect(
        last.restart == AppModel.Restart(program: "codex", from: "personal"),
        "the core types the account with its tool; the sentence names the tool already")
    #expect(last.notice == nil, "said once, by the core's warning")
    #expect(model.warnings(after: last) == [stillRunning])

    model.forgetSwitch(of: "codex")
    #expect(model.lastSwitches.isEmpty)
}

/// When the core could not count any sessions, the app's sentence is all there is, and it
/// is said of any session rather than of ones that may not exist.
@MainActor
@Test func aRestartIsExplainedWhenNoSessionsWereCounted() async {
    let stub = Stub(.success(status([account("work", of: "codex", signedIn: true)])))
    stub.switched = switched("codex", from: "codex/personal", to: "codex/work")
    let model = AppModel(watching: false, service: stub)
    await model.use("codex/work")
    #expect(
        model.lastSwitches.first?.notice
            == "Any codex session started before this switch keeps using personal until it is "
            + "quit and started again.")
}

/// A warning the read after a switch also carries is shown once, not twice.
@MainActor
@Test func aSwitchWarningTheReadRepeatsIsSaidOnce() async throws {
    let overridden = Warning(code: "auth_overridden", message: "ANTHROPIC_API_KEY is set")
    let stub = Stub(.success(status([account("b", signedIn: true)], warnings: [overridden])))
    stub.switched = switched("claude", from: "a", to: "b", warnings: [overridden])
    let model = AppModel(watching: false, service: stub)
    await model.use("claude/b")
    #expect(model.warnings == [overridden])
    #expect(model.warnings(after: try #require(model.lastSwitches.first)).isEmpty)
}

/// A switch replaces what its own tool's last switch said, and nothing another tool's said:
/// a Claude Code switch says nothing about Codex sessions still using the account Codex
/// just parked, and used to put away the warning not to sign out inside one.
@MainActor
@Test func eachToolKeepsWhatItsOwnLastSwitchSaid() async {
    let stub = Stub(
        .success(
            status([account("b", signedIn: true), account("b", of: "codex", signedIn: true)]))
    )
    stub.switched = switched("codex", from: "codex/a", to: "codex/b", warnings: [stillRunning])
    let model = AppModel(watching: false, service: stub)
    await model.use("codex/b")

    stub.switched = switched("claude", from: "a", to: "b")
    await model.use("claude/b")
    #expect(model.lastSwitches.map(\.provider) == ["codex", "claude"])
    #expect(model.lastSwitches.first?.warnings == [stillRunning])
    #expect(model.lastSwitches.last?.adopted != nil)

    stub.answer = .success(
        status([account("b", signedIn: true), account("c", of: "codex", signedIn: true)]))
    stub.switched = switched("codex", from: "codex/b", to: "codex/c")
    await model.use("codex/c")
    #expect(model.lastSwitches.map(\.to) == ["codex/c", "b"])
    #expect(
        model.lastSwitches.first?.warnings == [], "the last Codex switch, not the one before")
}

/// Whatever else writes the account index leaves the sessions a switch described as they
/// were: a read that renews a lapsed parked login, a scheduled renewal, an enrolment. Only a
/// switch made somewhere else, which leaves another account signed in, puts it away.
@MainActor
@Test func onlyASwitchMadeElsewherePutsAwayWhatASwitchSaid() async {
    let after = status([
        account("mine", signedIn: true), account("personal", of: "codex"),
        account("work", of: "codex", signedIn: true),
    ])
    let stub = Stub(.success(after))
    stub.switched = switched(
        "codex", from: "codex/personal", to: "codex/work", warnings: [stillRunning])
    let model = AppModel(watching: false, service: stub)
    await model.use("codex/work")

    stub.offline = .success(after)
    stub.changed = 99
    await model.noticeOtherChangesForTesting()
    #expect(model.lastSwitches.map(\.to) == ["codex/work"], "written, and nothing switched")

    // Claude Code switched in a terminal: Codex's sessions are where they were.
    stub.offline = .success(
        status([
            account("mine"), account("theirs", signedIn: true),
            account("personal", of: "codex"),
            account("work", of: "codex", signedIn: true),
        ]))
    stub.changed = 100
    await model.noticeOtherChangesForTesting()
    #expect(model.lastSwitches.map(\.to) == ["codex/work"])

    // Codex switched back in a terminal: what this switch said is no longer true.
    stub.offline = .success(
        status([
            account("mine", signedIn: true), account("personal", of: "codex", signedIn: true),
            account("work", of: "codex"),
        ]))
    stub.changed = 101
    await model.noticeOtherChangesForTesting()
    #expect(model.lastSwitches.isEmpty)
}

/// A read that fails after a switch leaves the change unrecorded, so the poll finds it.
/// That is the app's own switch, and taking it for somebody else's put away what it said
/// within two seconds of it being said.
@MainActor
@Test func aFailedReadAfterASwitchDoesNotPutAwayWhatItSaid() async {
    let before = status([
        account("personal", of: "codex", signedIn: true), account("work", of: "codex"),
    ])
    let after = status([
        account("personal", of: "codex"), account("work", of: "codex", signedIn: true),
    ])
    let stub = Stub(.success(before))
    let model = AppModel(watching: false, service: stub)
    await model.refresh()

    stub.answer = .failure(
        PitboardError.Failed(
            code: "unreachable", cause: nil, message: "OpenAI could not be reached",
            warnings: []))
    stub.offline = .success(after)
    stub.switched = switched(
        "codex", from: "codex/personal", to: "codex/work", warnings: [stillRunning])
    await model.use("codex/work")
    stub.changed = 7
    await model.noticeOtherChangesForTesting()

    #expect(model.lastSwitches.first?.warnings == [stillRunning])
    #expect(model.status == after, "and the panel shows the switch")
}

/// A switch that failed moved nothing, so what the last one said is still true, and the
/// failure is said as a failure with everything it warned about.
@MainActor
@Test func aFailedSwitchLeavesWhatTheLastSwitchSaid() async throws {
    let stub = Stub(
        .success(
            status([
                account("work", signedIn: true), account("work", of: "codex", signedIn: true),
            ]))
    )
    stub.switched = switched(
        "codex", from: "codex/personal", to: "codex/work", warnings: [stillRunning])
    let model = AppModel(watching: false, service: stub)
    await model.use("codex/work")
    let before = try #require(model.lastSwitches.first)

    let overridden = Warning(code: "auth_overridden", message: "ANTHROPIC_API_KEY is set")
    stub.switched = .failure(
        PitboardError.Failed(
            code: "parked_login_expired", cause: nil,
            message: "spare's parked login has expired",
            warnings: [overridden]))
    await model.use("claude/spare")

    #expect(model.lastSwitches == [before])
    #expect(model.problem == "spare's parked login has expired")
    #expect(model.otherWarnings == [overridden])
}

/// A switch to the account already in use moves nothing, so a notification pressed after
/// the switch it advised was made elsewhere leaves what that switch said.
@MainActor
@Test func aSwitchToTheAccountInUseLeavesWhatTheLastOneSaid() async throws {
    let stub = Stub(.success(status([account("work", of: "codex", signedIn: true)])))
    stub.switched = switched(
        "codex", from: "codex/personal", to: "codex/work", warnings: [stillRunning])
    let model = AppModel(watching: false, service: stub)
    await model.use("codex/work")
    let before = try #require(model.lastSwitches.first)

    stub.switched = .success(
        Switched(outcome: .alreadyActive(label: "codex/work"), warnings: []))
    await model.use("codex/work")
    #expect(model.lastSwitches == [before])
}

@MainActor
@Test func doctorIsOnlyReadWhenAskedFor() async {
    let model = AppModel(
        watching: false, service: Stub(.success(Status(now: 0, accounts: [], warnings: []))))
    #expect(model.checks.isEmpty)
    await model.diagnose()
    #expect(model.checks.map(\.code) == ["state"])
    model.forgetDiagnosis()
    #expect(model.checks.isEmpty)
}

/// The account signed in but not enrolled is the one the app can record by itself: no
/// browser, no terminal.
@MainActor
@Test func onlyAnUnenrolledSignedInAccountCanBeNamedHere() async {
    let stub = Stub(
        .success(
            Status(
                now: 0,
                accounts: [account(nil, signedIn: true, uuid: "a")], warnings: [])))
    let model = AppModel(watching: false, service: stub)
    await model.refresh()
    #expect(model.unenrolled)

    model.naming = .theOneInUse("claude")
    await model.enrol("work", for: "claude")
    #expect(stub.enrolled == ["claude/work"])
    #expect(model.naming == nil, "the form closes once it has been used")
}

/// Naming a Codex login enrols it as Codex's. A bare name means Claude Code to the core,
/// and would have enrolled nothing or the wrong tool's login.
@MainActor
@Test func aCodexLoginIsNamedAsCodexs() async {
    let stub = Stub(
        .success(
            status([
                account("work", signedIn: true),
                account(nil, of: "codex", signedIn: true, uuid: "c"),
            ])))
    let model = AppModel(watching: false, service: stub)
    await model.refresh()
    #expect(model.footing == .unnamed(provider: "codex", email: "c@example.com"))
    await model.enrol("job", for: "codex")
    #expect(stub.enrolled == ["codex/job"])
}

/// Forgetting is destructive, so what the model does with a refusal matters.
@MainActor
@Test func aRefusedForgetIsReported() async {
    let stub = Stub(.success(Status(now: 0, accounts: [], warnings: [])))
    let model = AppModel(watching: false, service: stub)
    await model.forget("claude/alpha")
    #expect(stub.forgot == ["claude/alpha"])
    #expect(model.problem == nil)
}

/// A sign-in that cannot start says why, and leaves nothing half-shown in the panel.
@MainActor
@Test func aSignInThatCannotStartIsReported() async {
    let model = AppModel(
        watching: false, service: Stub(.success(Status(now: 0, accounts: [], warnings: []))))
    model.naming = .another(nil)
    await model.signIn("work", for: "claude")
    #expect(model.signingIn == nil)
    #expect(model.naming == nil)
    #expect(model.problem == "`claude` is not on this machine")
}

/// Codex's sign-in prints an address and reads nothing, so the tool is named in the label
/// the core is given, and the sign-in finishes and enrols.
@MainActor
@Test func aCodexSignInIsForCodex() async {
    let stub = Stub(.success(status([])))
    let session = ScriptedSignIn(saying: ["https://auth.openai.com/oauth\n"], takesACode: false)
    stub.session = session
    let model = AppModel(watching: false, service: stub)
    await model.signIn("work", for: "codex")
    #expect(stub.signedIn == ["codex/work"])
    #expect(session.finished)
    #expect(model.signingIn == nil, "finished and enrolled")
    #expect(model.problem == nil)
}

/// Signing in again to the account in use puts its new login in use at once. The panel says
/// so, and what the core warned about sessions still on the old login stays beside it until
/// the tool has another account in use, the way a switch's warning does.
@MainActor
@Test func aSignInToTheAccountInUseSaysItsNewLoginIsInUse() async throws {
    let oldLogin = Warning(
        code: "sessions_keep_old_login",
        message:
            "2 `codex` sessions started before this sign-in are still running and still using "
            + "`codex/work`'s old login.")
    let stub = Stub(.success(status([account("work", of: "codex", signedIn: true)])))
    let session = ScriptedSignIn(saying: ["https://auth.openai.com/oauth\n"], takesACode: false)
    session.enrolls = Enrolled(
        email: "w@example.com", enrolled: .inUse(again: true), warnings: [oldLogin])
    stub.session = session
    let model = AppModel(watching: false, service: stub)

    await model.signIn("work", for: "codex")

    let last = try #require(model.lastSwitches.first)
    #expect(last.to == "codex/work")
    #expect(last.said == "Signed in to work again. Its new login is the one in use now.")
    #expect(last.notice == nil, "nothing switched away from anything")
    #expect(model.warnings(after: last) == [oldLogin])
    #expect(model.problem == nil)
}

/// A browser often signs in to the session it already has, so the account signed in now
/// can be enrolled by a sign-in under a new name. It says it was enrolled, not signed in to
/// again.
@MainActor
@Test func aFirstSignInToTheAccountInUseSaysItWasEnrolled() async throws {
    let stub = Stub(.success(status([account("work", of: "codex", signedIn: true)])))
    let session = ScriptedSignIn(saying: [], takesACode: false)
    session.enrolls = Enrolled(
        email: "w@example.com", enrolled: .inUse(again: false), warnings: [])
    stub.session = session
    let model = AppModel(watching: false, service: stub)

    await model.signIn("work", for: "codex")

    #expect(
        try #require(model.lastSwitches.first).said
            == "Enrolled work, the account signed in now. Its new login is the one in use.")
}

/// The tool did not switch, so what its last switch said about sessions still using the
/// account it left is still true, and stays: the restart it asked for, and the warning not
/// to sign out inside one. The sign-in's own count of the same sessions, naming this
/// account's old login, would contradict it and is not added.
@MainActor
@Test func aSignInToTheAccountInUseKeepsWhatTheLastSwitchSaid() async throws {
    let stub = Stub(
        .success(status([account("work", of: "codex", signedIn: true), account("personal")])))
    stub.switched = switched(
        "codex", from: "codex/personal", to: "codex/work", warnings: [stillRunning])
    let oldLogin = Warning(code: "sessions_keep_old_login", message: "2 sessions")
    let parked = Warning(code: "written_on_the_command_line", message: "on the argument line")
    let session = ScriptedSignIn(saying: [], takesACode: false)
    session.enrolls = Enrolled(
        email: "w@example.com", enrolled: .inUse(again: true), warnings: [oldLogin, parked])
    stub.session = session
    let model = AppModel(watching: false, service: stub)

    await model.use("codex/work")
    await model.signIn("work", for: "codex")

    #expect(model.lastSwitches.count == 1)
    let last = try #require(model.lastSwitches.first)
    #expect(last.restart == AppModel.Restart(program: "codex", from: "personal"))
    #expect(last.said == "Signed in to work again. Its new login is the one in use now.")
    #expect(last.warnings == [stillRunning, parked])
}

/// A sign-in that parked its login rather than put it in use says why, after the read that
/// follows it, which would otherwise put the warning away.
@MainActor
@Test func aSignInThatWasParkedSaysWhatItWarnedAbout() async {
    let untold = Warning(
        code: "sign_in_parked_not_in_use", message: "Codex goes on with the login it has")
    let stub = Stub(.success(status([account("work", of: "codex", signedIn: true)])))
    let session = ScriptedSignIn(saying: [], takesACode: false)
    session.enrolls = Enrolled(email: "w@example.com", enrolled: .renewed, warnings: [untold])
    stub.session = session
    let model = AppModel(watching: false, service: stub)

    await model.signIn("work", for: "codex")

    #expect(model.warnings == [untold])
    #expect(model.lastSwitches.isEmpty)
}

/// A sign-in refused before it started says what that refusal found on the way, such as a
/// switch interrupted earlier and finished now, not only why it was refused.
@MainActor
@Test func aRefusedSignInSaysWhatItFoundOnTheWay() async {
    let recovered = Warning(
        code: "interrupted_switch_undone", message: "an earlier switch was interrupted")
    let stub = Stub(.success(status([])))
    stub.signInWarnings = [recovered]
    let model = AppModel(watching: false, service: stub)

    await model.signIn("work", for: "claude")

    #expect(model.problem == "`claude` is not on this machine")
    #expect(model.warnings == [recovered])
}

/// A sign-in of another account adds a row and says nothing more.
@MainActor
@Test func aSignInOfAnotherAccountSaysNothingMore() async {
    let stub = Stub(.success(status([account("work", of: "codex", signedIn: true)])))
    stub.session = ScriptedSignIn(
        saying: ["https://auth.openai.com/oauth\n"], takesACode: false)
    let model = AppModel(watching: false, service: stub)
    await model.signIn("personal", for: "codex")
    #expect(model.lastSwitches.isEmpty)
}

/// A code field is offered only by a sign-in whose tool reads one, whatever the tool prints,
/// and what is typed goes to the tool off the main thread.
@MainActor
@Test func aCodeIsAskedForOnlyWhereTheToolTakesOne() async throws {
    for (provider, takes) in [("claude", true), ("codex", false)] {
        let stub = Stub(.success(status([])))
        let session = ScriptedSignIn(
            saying: ["Paste code here if prompted > "], takesACode: takes, waits: true)
        stub.session = session
        let model = AppModel(watching: false, service: stub)
        let running = Task { await model.signIn("work", for: provider) }

        #expect(await eventually { model.signingIn?.said.contains("Paste code") == true })
        #expect(model.signingIn?.takesACode == takes)
        #expect(model.signingIn?.wantsCode == takes)
        if takes {
            model.paste("abc")
            #expect(model.signingIn?.wantsCode == false, "asked once")
            #expect(await eventually { session.pasted == ["abc"] })
            #expect(session.pastedOnMain == false)
        }

        session.done.signal()
        await running.value
    }
}

/// Cancel stops the tool off the main thread: stopping waits for the tool, and a Codex
/// sign-in waiting on the browser held the whole app while it did. What the stopped tool
/// leaves behind is not a failure to report, and nothing is enrolled.
@MainActor
@Test func aCancelledSignInStopsTheToolAndReportsNothing() async {
    let stub = Stub(.success(status([])))
    let session = ScriptedSignIn(
        saying: ["https://auth.openai.com/oauth/authorize?state=x\n"], takesACode: false,
        waits: true)
    stub.session = session
    let model = AppModel(watching: false, service: stub)
    let running = Task { await model.signIn("work", for: "codex") }
    #expect(await eventually { model.signingIn?.url != nil })

    model.cancelSignIn()
    #expect(model.signingIn == nil)
    await running.value

    #expect(session.cancelledOnMain == false)
    #expect(!session.finished)
    #expect(model.problem == nil)
}

/// An account whose parked login can no longer be used is signed in to again from its row,
/// through the sign-in a new account gets, so the address Codex prints shows in the panel
/// the same way. The core is given the label with its tool once, as for a new account.
@MainActor
@Test func anAccountThatCannotBeSwitchedToIsSignedInToAgainFromThePanel() async throws {
    let stub = Stub(
        .success(
            status([
                account("personal", of: "codex", signedIn: true),
                account("work", of: "codex", switchable: false),
                account("spare", switchable: false),
            ])))
    let session = ScriptedSignIn(
        saying: ["https://auth.openai.com/oauth/authorize?state=x\n"], takesACode: false,
        waits: true)
    stub.session = session
    let model = AppModel(watching: false, service: stub)
    await model.refresh()
    let accounts = try #require(model.status?.accounts)

    let running = Task { await model.signInAgain(to: accounts[1]) }
    #expect(await eventually { model.signingIn?.url != nil })
    #expect(stub.signedIn == ["codex/work"])
    #expect(model.signingIn?.tool == "Codex")
    #expect(model.signingIn?.label == "work")
    session.done.signal()
    await running.value
    #expect(session.finished)
    #expect(model.signingIn == nil)

    stub.session = ScriptedSignIn(saying: [], takesACode: true)
    await model.signInAgain(to: accounts[2])
    #expect(stub.signedIn == ["codex/work", "claude/spare"])
}

/// The settings say which `pitboard` a terminal runs, looking where the login shell's
/// `PATH` says before anywhere else, and whether it is this app's own. Where the shell could
/// not be asked, it is the first one found where a way of installing pitboard puts it.
@MainActor
@Test func theSettingsLookForTheCommandLineWhereTheLoginShellSays() async throws {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboard-path-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: root) }
    let helper = root.appendingPathComponent("Pitboard.app/Contents/Helpers/pitboard")
    let bin = root.appendingPathComponent("bin")
    let home = root.appendingPathComponent("home")
    let cargo = home.appendingPathComponent(".cargo/bin/pitboard")
    for directory in [
        helper.deletingLastPathComponent(), bin, cargo.deletingLastPathComponent(),
    ] {
        try FileManager.default.createDirectory(
            at: directory, withIntermediateDirectories: true)
    }
    for program in [helper, cargo] {
        try Data("#!/bin/sh\n".utf8).write(to: program)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755], ofItemAtPath: program.path)
    }
    let linked = bin.appendingPathComponent("pitboard").path
    try FileManager.default.createSymbolicLink(
        atPath: linked, withDestinationPath: helper.path)

    let stub = Stub(.success(status([])))
    stub.path = "/nowhere/bin:\(bin.path)"
    let model = AppModel(
        watching: false, service: stub,
        commandLineTool: CommandLineTool(
            bundle: root.appendingPathComponent("Pitboard.app"), home: home.path))
    #expect(model.commandLine == nil, "not looked for until the settings ask")
    await model.findCommandLine()
    #expect(model.commandLine == .bundled(linked), "ahead of the one cargo installed")

    let other = AppModel(
        watching: false, service: stub, commandLineTool: CommandLineTool(home: home.path))
    await other.findCommandLine()
    #expect(other.commandLine == .another(linked), "these tests are not an app")

    stub.path = nil
    await model.findCommandLine()
    #expect(model.commandLine == .another(cargo.path), "the login shell could not be asked")
}

/// Daily renewal is turned on only from an app with a command line inside it that stays
/// where it is. The schedule runs it long after the app has quit: a copy macOS runs from a
/// temporary place is gone by then, and a build with none inside it would schedule the app
/// itself, which renews nothing. Turning it off is always possible, so a schedule that cannot
/// work can be taken away.
@MainActor
@Test func dailyRenewalIsTurnedOnOnlyFromAnAppThatStaysWhereItIs() async {
    func model(_ bundle: String) -> (AppModel, Stub) {
        let stub = Stub(.success(status([])))
        let model = AppModel(
            watching: false, service: stub,
            commandLineTool: CommandLineTool(bundle: URL(fileURLWithPath: bundle)))
        return (model, stub)
    }

    let (installed, stub) = model("/Applications/Pitboard.app")
    #expect(installed.cannotSchedule == nil)
    await installed.setSchedule(on: true)
    #expect(stub.scheduleInstalls == 1)
    #expect(installed.problem == nil)

    let (downloaded, temporary) = model(
        "/private/var/folders/xy/abc/T/AppTranslocation/0A1B2C/d/Pitboard.app")
    #expect(downloaded.cannotSchedule?.hasPrefix("Move pitboard to your Applications") == true)
    let (built, unbundled) = model("/Users/x/pitboard/apple/.build/debug")
    #expect(built.cannotSchedule?.contains("no command line inside it") == true)
    for (refused, stub) in [(downloaded, temporary), (built, unbundled)] {
        await refused.setSchedule(on: true)
        #expect(stub.scheduleInstalls == 0)
        #expect(refused.problem == refused.cannotSchedule)
        await refused.setSchedule(on: false)
        #expect(stub.scheduleUninstalls == 1)
    }
}

/// An app up to 0.3.0 scheduled itself, so launchd has been starting a second app every day
/// and renewing nothing. This one asks the core to point that schedule at the command line
/// inside it once, when it starts, and the settings then show the schedule as it is now.
/// Where nothing was repaired, or the repair failed, nothing more is read or said.
@MainActor
@Test func anOldScheduleIsRepairedOnceTheAppStarts() async {
    let installed = Schedule.installed(
        path: "/Users/x/Library/LaunchAgents/com.usepitboard.renew.plist", everySeconds: 86_400)
    let stub = Stub(.success(status([])))
    stub.scheduled = installed
    stub.repairs = .success(true)
    let model = AppModel(service: stub, defaults: MemoryDefaults())
    #expect(await eventually { model.schedule == installed })
    #expect(stub.repairAsks == 1)

    let refused = PitboardError.Failed(
        code: "schedule_refused", cause: nil, message: "the scheduler refused: no",
        warnings: [])
    for answer: Result<Bool, Error> in [.success(false), .failure(refused)] {
        let quiet = Stub(.success(status([])))
        quiet.scheduled = installed
        quiet.repairs = answer
        let model = AppModel(watching: false, service: quiet)
        #expect(quiet.repairAsks == 0, "a test drives it itself")
        await model.repairSchedule()
        #expect(quiet.repairAsks == 1)
        #expect(quiet.scheduleReads == 0)
        #expect(model.schedule == .absent)
        #expect(model.problem == nil)
    }
}

/// The address Codex prints is the one to open; the loopback address it also prints is
/// where the browser comes back to.
@MainActor
@Test func theAddressToOpenIsTheOneThePersonGoesTo() {
    let codexSaid = SigningIn(label: "work", tool: "Codex")
    codexSaid.add("Starting local login server on http://localhost:1455.\n")
    codexSaid.add(
        "If your browser did not open, navigate to this URL to authenticate:\n\n"
            + "https://auth.openai.com/oauth/authorize?response_type=code&state=x\u{1B}[0m\n")
    #expect(
        codexSaid.url?.absoluteString
            == "https://auth.openai.com/oauth/authorize?response_type=code&state=x")
    codexSaid.add("Paste code here if prompted > ")
    #expect(!codexSaid.wantsCode, "Codex reads nothing, whatever it prints")

    let claudeSaid = SigningIn(label: "work", tool: "Claude Code")
    claudeSaid.takesACode = true
    claudeSaid.add("Paste code here if prompted > ")
    #expect(claudeSaid.wantsCode)
}

/// A timer producing a reading is not somebody asking for one, and the core decides whether
/// to go to Anthropic from that. The panel's own Refresh is asking; everything else is not.
@MainActor
@Test func onlyAskingForAReadingAsksAnthropicAgain() async {
    let stub = Stub(.success(Status(now: 0, accounts: [], warnings: [])))
    let model = AppModel(watching: false, service: stub)

    await model.refresh()
    #expect(stub.freshAsks == 0, "a poll takes whatever the core already knows")

    await model.refresh(asked: true)
    #expect(stub.freshAsks == 1)
}

/// Three front ends run on one machine and none of them could tell when another had changed
/// anything. A switch typed in a terminal left the menu bar naming the account the person
/// had just stopped using, for as long as five minutes.
@MainActor
@Test func aChangeMadeSomewhereElseIsNoticedWithoutAskingAnthropic() async {
    let stub = Stub(.success(Status(now: 0, accounts: [], warnings: [])))
    stub.offline = .success(
        Status(now: 0, accounts: [account("work", signedIn: true, percent: 5)], warnings: []))
    let model = AppModel(watching: false, service: stub)

    // A read establishes where things stand, and costs one offline read at most.
    await model.refresh()
    let before = stub.offlineReads

    // Something else changes the account index.
    stub.changed = 42
    await model.noticeOtherChangesForTesting()

    #expect(stub.offlineReads == before + 1, "it read what is already known")
    #expect(stub.freshAsks == 0, "and asked Anthropic nothing")
    #expect(model.status?.accounts.first?.label == "work")
}

/// A read that could not reach Anthropic still has something true to show. An empty panel
/// says the accounts are gone, which is not what happened.
@MainActor
@Test func aFailedFirstReadFallsBackToWhatIsAlreadyKnown() async {
    let stub = Stub(
        .failure(
            PitboardError.Failed(
                code: "unreachable", cause: nil, message: "could not reach Anthropic",
                warnings: [])))
    stub.offline = .success(
        Status(now: 0, accounts: [account("work", signedIn: true, percent: 5)], warnings: []))
    let model = AppModel(watching: false, service: stub)

    await model.refresh()

    #expect(model.problem == "could not reach Anthropic")
    #expect(model.status?.accounts.count == 1, "the last numbers measured are still true")
}

/// Every warning, not only the first. A switch can warn about an overriding environment
/// variable and a config that did not update, and showing one is how somebody fixes the
/// wrong thing.
@MainActor
@Test func everyWarningIsKeptNotOnlyTheFirst() async {
    let model = AppModel(
        service: Stub(
            .success(
                Status(
                    now: 0, accounts: [],
                    warnings: [
                        Warning(code: "auth_overridden", message: "ANTHROPIC_API_KEY is set"),
                        Warning(
                            code: "config_write_failed", message: "the config did not update"),
                    ]))))
    await model.refresh()
    #expect(model.warnings.count == 2)
    #expect(model.warnings.map(\.code) == ["auth_overridden", "config_write_failed"])
}

// MARK: - What a machine that is not set up yet is told to do

/// A new install of the app. Before the first read there is nothing true to say, and a
/// setup step shown to somebody who finished it years ago is worse than silence.
@MainActor
@Test func nothingIsAskedOfAnyoneBeforeTheFirstRead() {
    let model = AppModel(
        watching: false, service: Stub(.success(Status(now: 0, accounts: [], warnings: []))))
    #expect(model.footing == .ready)
}

/// The one state pitboard cannot do anything about. It has to say so rather than show an
/// empty panel, which reads as an app that does not work.
@MainActor
@Test func aMachineWithoutClaudeCodeIsToldThatFirst() async {
    let model = AppModel(
        watching: false,
        service: Stub(
            .failure(
                PitboardError.Failed(
                    code: "claude_program_missing", cause: nil,
                    message: "`claude` is not on this machine", warnings: []))))
    await model.refresh()
    #expect(model.footing == .noClaudeCode)
}

@MainActor
@Test func anEmptyMachineIsAskedToSignInOnce() async {
    let model = AppModel(
        watching: false, service: Stub(.success(Status(now: 0, accounts: [], warnings: []))))
    await model.refresh()
    #expect(model.footing == .noOneSignedIn)
}

/// A login with no name cannot be parked, so this is the step between signing in and
/// pitboard being able to do anything at all.
@MainActor
@Test func anAccountSignedInWithoutANameIsAskedForOne() async {
    let model = AppModel(
        watching: false,
        service: Stub(
            .success(
                Status(
                    now: 0,
                    accounts: [account(nil, signedIn: true, uuid: "a")], warnings: []))))
    await model.refresh()
    #expect(model.footing == .unnamed(provider: "claude", email: "a@example.com"))
}

@MainActor
@Test func oneEnrolledAccountIsToldThereIsNothingToSwitchTo() async {
    let model = AppModel(
        watching: false,
        service: Stub(.success(status([account("work", signedIn: true, percent: 10)]))))
    await model.refresh()
    #expect(model.footing == .onlyOne(provider: "claude", label: "work"))
}

@MainActor
@Test func twoAccountsAreAskedNothing() async {
    let model = AppModel(
        watching: false,
        service: Stub(
            .success(
                status([
                    account("work", signedIn: true, percent: 10),
                    account("personal", signedIn: false, percent: 4),
                ]))))
    await model.refresh()
    #expect(model.footing == .ready)
}

/// Mid-switch, and a login signed out from somewhere else, both leave accounts enrolled
/// with nobody signed in. Neither is a machine that needs setting up, and asking somebody
/// to sign in again there would have them sign in over an account pitboard already holds.
@MainActor
@Test func enrolledAccountsWithNobodySignedInAreNotAskedToStartOver() async {
    let model = AppModel(
        watching: false,
        service: Stub(.success(status([account("work", signedIn: false, percent: 10)]))))
    await model.refresh()
    #expect(model.footing == .ready)
}

/// A failure carries its own warnings. Leaving the last successful read's in place put a
/// fresh network error above warnings about things that may have been fixed since.
@MainActor
@Test func aFailedReadShowsItsOwnWarningsRatherThanTheLastOnes() async {
    let stub = Stub(
        .success(
            Status(
                now: 0, accounts: [],
                warnings: [
                    Warning(
                        code: "state_on_synced_drive", message: "~/.pitboard is on iCloud Drive"
                    )
                ])))
    let model = AppModel(watching: false, service: stub)
    await model.refresh()
    #expect(model.warnings.map(\.code) == ["state_on_synced_drive"])

    stub.answer = .failure(
        PitboardError.Failed(
            code: "identity_unverifiable",
            cause: Cause(code: "unreachable", worthRetrying: true),
            message: "Anthropic could not be reached",
            warnings: [Warning(code: "overriding_env", message: "ANTHROPIC_API_KEY is set")]))
    await model.refresh()
    #expect(model.warnings.map(\.code) == ["overriding_env"])
    #expect(model.problem == "Anthropic could not be reached")
    #expect(
        model.otherWarnings.map(\.code) == ["overriding_env"],
        "the problem is not one of them, so none is left unsaid")
}

// MARK: - More than one tool

/// Somebody signed in to Codex is not somebody nobody is signed in to.
@MainActor
@Test func aCodexLoginIsNotAnEmptyMachine() async {
    let model = AppModel(
        watching: false,
        service: Stub(.success(status([account("work", of: "codex", signedIn: true)]))))
    await model.refresh()
    #expect(model.footing == .onlyOne(provider: "codex", label: "work"))
}

/// A Claude Code account and a Codex account are two accounts and nothing to switch to:
/// an account is only ever switched to another of its own tool.
@MainActor
@Test func onlyOneAccountIsCountedPerTool() async {
    let model = AppModel(
        watching: false,
        service: Stub(
            .success(
                status([
                    account("work", signedIn: true),
                    account("spare"),
                    account("job", of: "codex", signedIn: true),
                ]))))
    await model.refresh()
    #expect(model.footing == .onlyOne(provider: "codex", label: "job"))
}

/// A login pitboard could not read, or cannot switch, has no account behind it. Offering
/// to name it would enrol something that can never be switched to.
@MainActor
@Test func anUnplacedLoginIsNeitherUnenrolledNorOfferedAName() async {
    for row in [unplaced(of: "codex"), unplaced(of: "codex", signedIn: true)] {
        let model = AppModel(
            watching: false,
            service: Stub(
                .success(
                    status([
                        account("work", signedIn: true),
                        account("spare"),
                        account("job", of: "codex"),
                        account("side", of: "codex"),
                        row,
                    ]))))
        await model.refresh()
        #expect(!model.unenrolled)
        #expect(model.unnamed.isEmpty)
        #expect(model.footing == .ready)
    }
}

/// Two tools can each have a `work`. Every call names the one meant, with its tool, and the
/// rows are told apart by their id rather than by a label they share.
@MainActor
@Test func twoToolsWorkAccountsAreSwitchedAndForgottenByTheirOwnName() async {
    let stub = Stub(
        .success(
            status([
                account("work", signedIn: true, uuid: "same"),
                account("personal"),
                account("work", of: "codex", uuid: "same"),
                account("spare", of: "codex", signedIn: true),
            ])))
    let model = AppModel(watching: false, service: stub)
    await model.refresh()

    let rows = model.groups.flatMap(\.accounts)
    #expect(Set(rows.map(\.id)).count == rows.count, "one uuid, two tools, two rows")
    let codexWork = rows.first { $0.provider == "codex" && $0.label == "work" }
    let claudeWork = rows.first { $0.provider == "claude" && $0.label == "work" }
    #expect(codexWork?.id != claudeWork?.id)

    await model.use(codexWork?.qualified ?? "")
    await model.forget(codexWork?.qualified ?? "")
    await model.forget(claudeWork?.qualified ?? "")
    #expect(stub.switchedTo == ["codex/work"])
    #expect(stub.forgot == ["codex/work", "claude/work"])
    #expect(model.name(of: codexWork!) == "work (Codex)")
}

/// Only tools the app found a program for are offered for a new account, and the form says
/// which were left out rather than leaving them out without a word.
@MainActor
@Test func aNewAccountIsOfferedForToolsThatAreHere() async {
    let stub = Stub(.success(status([])))
    let model = AppModel(watching: false, service: stub)
    await model.refresh()
    #expect(model.addable == [claudeCode])
    #expect(
        model.notOffered == "Codex is not offered: pitboard did not find codex on this Mac.")

    stub.found = [codex]
    let codexOnly = AppModel(watching: false, service: stub)
    await codexOnly.refresh()
    #expect(codexOnly.addable == [codex])
    #expect(codexOnly.provider(for: .another(nil)) == "codex")

    stub.found = bothTools
    let both = AppModel(watching: false, service: stub)
    await both.refresh()
    #expect(both.notOffered == nil)

    // A tool with an account here is here, wherever its program is.
    stub.answer = .success(status([account("work", of: "codex", signedIn: true)]))
    stub.found = [claudeCode]
    let known = AppModel(watching: false, service: stub)
    await known.refresh()
    #expect(known.addable == bothTools)
}

/// A machine where the app found neither program reads as one with Claude Code alone, as it
/// always did: whatever it did not find where it looks is not on the PATH of an app opened
/// from Finder either, so offering every tool offered sign-ins that could not start.
@MainActor
@Test func nothingFoundOffersClaudeCodeAlone() async {
    let stub = Stub(.success(status([])))
    stub.found = []
    let model = AppModel(watching: false, service: stub)
    await model.refresh()
    #expect(model.addable == [claudeCode])
    #expect(model.services == "Anthropic")
    #expect(model.provider(for: .another(nil)) == "claude")
}

/// What the app found does not change while it runs, so it is asked once and not on every
/// keystroke in the form that reads it, nor on every read.
///
/// Not when the model is made either: finding a tool can mean waiting on the person's login
/// shell, and the model is made on the main thread.
@MainActor
@Test func whatIsInstalledIsAskedOnce() async {
    let stub = Stub(.success(status([])))
    stub.found = bothTools
    let model = AppModel(watching: false, service: stub)
    #expect(stub.installedAsks == 0)
    #expect(model.addable == [claudeCode], "nothing is known to be here until a read asks")
    await model.refresh()
    for _ in 0..<3 { _ = model.addable }
    _ = model.services
    await model.refresh()
    #expect(stub.installedAsks == 1)
    #expect(model.addable == bothTools)
}

/// Opening the form for another account asks again what is installed: the first answer may
/// have come while the person's login shell was too slow to say, and the service asks it
/// once more when that was so.
@MainActor
@Test func whatIsInstalledIsAskedAgainWhenTheFormForAnotherAccountOpens() async {
    let stub = Stub(.success(status([])))
    stub.found = [claudeCode]
    let model = AppModel(watching: false, service: stub)
    await model.refresh()
    #expect(model.addable == [claudeCode])

    stub.found = bothTools
    model.naming = .another(nil)
    #expect(await eventually { model.addable == bothTools })
    #expect(stub.installedAsks == 2)

    model.naming = nil
    model.naming = .theOneInUse("claude")
    await model.refresh()
    #expect(stub.installedAsks == 2, "not for naming the account in use, nor for a read")
}

/// The form starts on the tool it was asked about, and otherwise on the first it offers.
@MainActor
@Test func theFormStartsOnTheToolItIsAbout() {
    let model = AppModel(watching: false, service: Stub(.success(status([]))))
    #expect(model.provider(for: .another(nil)) == "claude")
    #expect(model.provider(for: .another("codex")) == "codex")
    #expect(model.provider(for: .theOneInUse("codex")) == "codex")
}

/// Keeping one Claude Code account on purpose says nothing about Codex. The prompt for a
/// second account is declined per tool, and a declined one does not stand in front of the
/// next tool's.
@MainActor
@Test func aSecondAccountIsDeclinedPerTool() async {
    let stub = Stub(
        .success(
            status([
                account("work", signedIn: true), account("job", of: "codex", signedIn: true),
            ])
        ))
    let defaults = MemoryDefaults(suiteName: nil)!
    let model = AppModel(watching: false, service: stub, defaults: defaults)
    await model.refresh()
    #expect(model.footing == .onlyOne(provider: "claude", label: "work"))

    model.declineSecondAccount(for: "claude")
    #expect(model.footing == .onlyOne(provider: "codex", label: "job"))

    let later = AppModel(watching: false, service: stub, defaults: defaults)
    await later.refresh()
    #expect(later.footing == .onlyOne(provider: "codex", label: "job"), "and it is remembered")
    later.declineSecondAccount(for: "codex")
    #expect(later.footing == .ready)
}

/// "Not now" said before there was a second tool was said about Claude Code, the only tool
/// there was, and does not hide the prompt for a first Codex account.
@MainActor
@Test func aNudgeDeclinedBeforeCodexWasAboutClaudeCode() async {
    let stub = Stub(
        .success(
            status([
                account("work", signedIn: true), account("job", of: "codex", signedIn: true),
            ])
        ))
    let defaults = MemoryDefaults(suiteName: nil)!
    defaults.set(true, forKey: "hideSecondAccountNudge")
    let model = AppModel(watching: false, service: stub, defaults: defaults)
    await model.refresh()
    #expect(model.footing == .onlyOne(provider: "codex", label: "job"))
    #expect(model.secondAccountDeclined == ["claude"])
    #expect(defaults.object(forKey: "hideSecondAccountNudge") == nil, "moved, not kept twice")
}

/// The words for who is asked follow the tools shown, so a machine with Claude Code alone
/// reads exactly as it did.
@MainActor
@Test func theServiceAskedIsNamedForTheToolsShown() async {
    let stub = Stub(.success(status([account("work", signedIn: true)])))
    let model = AppModel(watching: false, service: stub)
    await model.refresh()
    #expect(model.services == "Anthropic")
    #expect(!model.showsTools)

    stub.answer = .success(
        status([account("work", signedIn: true), account("job", of: "codex", signedIn: true)]))
    await model.refresh()
    #expect(model.services == "Anthropic or OpenAI")
    #expect(model.showsTools)
}

/// The menu bar follows whichever account is closest to running out, and two tools can
/// each have a `work`, so once there is more than one tool VoiceOver says which.
@MainActor
@Test func theMenuBarSaysWhichToolItIsAboutOnceThereAreTwo() async {
    let stub = Stub(.success(status([account("work", signedIn: true, percent: 42)])))
    let model = AppModel(watching: false, service: stub)
    #expect(model.spokenTitle == "pitboard")
    await model.refresh()
    #expect(model.spokenTitle == "pitboard, work 42%")

    stub.answer = .success(
        status([
            account("work", signedIn: true, percent: 60),
            account(
                "work", of: "codex", signedIn: true, [window("five_hour", 80, resets: nil)]),
        ]))
    await model.refresh()
    #expect(model.spokenTitle == "pitboard, work 80%, Codex")
}
