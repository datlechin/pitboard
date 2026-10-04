import Foundation
import PitboardKit
import Testing

@testable import PitboardApp

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
        Enrolled(email: "a@b.c", outcome: .current, warnings: []))
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
    /// When the readings last changed. A read moves it, as the core's does: what it measured
    /// is recorded.
    var readings: Int64 = 0
    private(set) var offlineReads = 0
    var abandoned: Abandoned?

    /// What happens on this machine while a read waits on a service. The read has taken
    /// what it answers by then, as the core's takes who is signed in before it asks anyone.
    var duringRead: (@MainActor () async -> Void)?
    func status(fresh: Bool) async throws -> Status {
        if fresh { freshAsks += 1 }
        readings += 1
        let answer = self.answer
        await duringRead?()
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
    func readingsChangedAt() async -> Int64 { readings }
    func doctor() async -> Diagnosis {
        Diagnosis(
            checks: [
                Check(code: "state", name: "state", level: .ok, detail: "fine", advice: "")
            ],
            healthy: true)
    }
    /// What is running each tool with its login in memory, by the tool's code.
    var held: [String: [Holding]] = [:]
    /// What happens before the core says what holds a login, which a real one asks of the
    /// process list.
    var beforeHolding: (@MainActor () async -> Void)?
    func holding(_ provider: String) async -> [Holding] {
        if let beforeHolding { await beforeHolding() }
        return held[provider] ?? []
    }
    /// What happens on this machine while a switch is under way.
    var duringSwitch: (@MainActor () async -> Void)?
    func switchTo(_ label: String) async throws -> Switched {
        switchedTo.append(label)
        await duringSwitch?()
        return try switched.get()
    }
    func enrollCurrent(_ label: String) async throws -> Enrolled {
        enrolled.append(label)
        return try enrolling.get()
    }
    /// What the next forget is refused with, if anything.
    var forgetting: Error?
    func forget(_ label: String) async throws -> Changed {
        forgot.append(label)
        if let forgetting { throw forgetting }
        return Changed(email: "\(label)@example.com", warnings: [])
    }
    func rename(_ from: String, to: String) async throws -> Changed {
        Changed(email: "a@b.c", warnings: [])
    }
    /// What happens while a sign-in is still starting, before the tool has said anything.
    var duringSignIn: (@MainActor () async -> Void)?
    func signIn(_ label: String) async throws -> SignIn {
        signedIn.append(label)
        await duringSignIn?()
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
    func switchToSignedOut(_ provider: String) async throws -> Switched {
        throw PitboardError.Failed(
            code: "unsupported", cause: nil, message: "not in this test", warnings: [])
    }
    func awaitingSignIn() async -> Awaiting? { nil }
    func liveUsage() async -> LiveUsageState {
        LiveUsageState(enabled: false, approval: "unknown", reason: nil, lastOkAt: nil)
    }
    func enableLiveUsage() async throws -> LiveUsageState { await liveUsage() }
    func disableLiveUsage() async throws -> LiveUsageState { await liveUsage() }
}

/// A sign-in that says what it is given to say and then enrols, without a tool behind it.
///
/// `waits` holds it after its last line, the way a tool that has printed its address waits
/// on the browser, until it is cancelled or `done` is signalled.
private final class ScriptedSignIn: SignIn, @unchecked Sendable {
    private var lines: [String]
    private let waits: Bool
    let done = DispatchSemaphore(value: 0)
    private(set) var pasted: [String] = []
    /// Whether each was called on the main thread, which is the app's to keep free.
    private(set) var pastedOnMain: Bool?
    private(set) var cancelledOnMain: Bool?
    /// Whether finishing has been asked for, whether or not it has returned.
    private(set) var finished = false
    /// What finishing enrols.
    var enrolls = Enrolled(email: "a@b.c", outcome: .signedIn, warnings: [])
    /// What finishing fails with instead, as a tool stopped before it finished does.
    var refusal: Error?
    /// Holds finishing until it is signalled, as a tool still enrolling does.
    var finishing: DispatchSemaphore?

    init(saying lines: [String], waits: Bool = false) {
        self.lines = lines
        self.waits = waits
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) { fatalError("not from the core") }

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
        finishing?.wait()
        if let refusal { throw refusal }
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
        + "`codex/personal`. Quit them and start again to use the new account. Do not sign "
        + "out in any of them: signing out there revokes `codex/personal`'s login, which "
        + "Pitboard has just parked.")

private func account(_ label: String, signedIn: Bool, percent: Double) -> Account {
    account(label, signedIn: signedIn, [window("session", percent, resets: nil)])
}

@MainActor
@Test func aReadFillsInTheTitleAndTheRows() async {
    let model = AppModel(
        testing: Stub(
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

/// Pitboard's errors already say what to do, so the panel shows the message as it is.
@MainActor
@Test func aFailedReadIsShownAsItsOwnMessage() async {
    let model = AppModel(
        testing: Stub(
            .failure(
                PitboardError.Failed(
                    code: "state_wrong_machine", cause: nil,
                    message: "was written on another computer",
                    warnings: []))))
    await model.refresh()
    #expect(model.problem == "was written on another computer")
    #expect(model.problemCode == "state_wrong_machine")
    #expect(
        model.status?.accounts.isEmpty == true,
        "the panel falls back to what is already known, which here is nothing")
}

/// A switch that failed must not read as one that worked. The failure goes back to whoever
/// asked for the switch rather than where a read's is said, where the next read replaced it
/// seconds later, before anyone had looked. Asked for from the menu or a notification, which
/// have nowhere to put a sentence, it is said in the window.
@MainActor
@Test func aFailedSwitchSaysSoAndChangesNothing() async throws {
    let stub = Stub(.success(Status(now: 0, accounts: [], warnings: [])))
    stub.switched = .failure(
        PitboardError.Failed(
            code: "nothing_parked", cause: nil, message: "nothing parked", warnings: []))
    let model = AppModel(testing: stub)
    let failure = try #require(await model.use("work"))
    #expect(stub.switchedTo == ["work"])
    #expect(failure.title == "Couldn’t switch to work")
    #expect(failure.message == "nothing parked")
    #expect(failure.code == "nothing_parked")
    #expect(model.problem == nil, "a switch is not a read")
    #expect(model.lastSwitches.isEmpty)

    await model.switchAsked(to: "claude/work")
    #expect(model.presentedFailure?.message == "nothing parked")
    #expect(model.windowRequests == 1)
}

/// After a switch the panel counts down to when open sessions follow.
@MainActor
@Test func aSwitchRecordsWhenOpenSessionsFollow() async {
    let stub = Stub(.success(status([account("work", signedIn: true), account("personal")])))
    stub.switched = switched("claude", from: "personal", to: "work")
    let model = AppModel(testing: stub)
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
    let model = AppModel(testing: stub)

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
    let model = AppModel(testing: stub)
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
    let model = AppModel(testing: stub)
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
    let model = AppModel(testing: stub)
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
    let model = AppModel(testing: stub)
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
    let model = AppModel(testing: stub)
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
    let model = AppModel(testing: stub)
    await model.use("codex/work")
    let before = try #require(model.lastSwitches.first)

    let overridden = Warning(code: "auth_overridden", message: "ANTHROPIC_API_KEY is set")
    stub.switched = .failure(
        PitboardError.Failed(
            code: "parked_login_expired", cause: nil,
            message: "spare's parked login has expired",
            warnings: [overridden]))
    let failure = try #require(await model.use("claude/spare"))

    #expect(model.lastSwitches == [before])
    #expect(failure.message == "spare's parked login has expired")
    #expect(failure.warnings == [overridden])
    #expect(model.warnings == [overridden], "and the panel says it beside the accounts")
}

/// A switch to the account already in use moves nothing, so a notification pressed after
/// the switch it advised was made elsewhere leaves what that switch said.
@MainActor
@Test func aSwitchToTheAccountInUseLeavesWhatTheLastOneSaid() async throws {
    let stub = Stub(.success(status([account("work", of: "codex", signedIn: true)])))
    stub.switched = switched(
        "codex", from: "codex/personal", to: "codex/work", warnings: [stillRunning])
    let model = AppModel(testing: stub)
    await model.use("codex/work")
    let before = try #require(model.lastSwitches.first)

    stub.switched = .success(
        Switched(outcome: .alreadyActive(label: "codex/work"), warnings: []))
    await model.use("codex/work")
    #expect(model.lastSwitches == [before])
}

/// `pitboard doctor` looks at everything on this Mac, so its checks are made when somebody
/// asks to see them and not with every read, and they say when they were made.
@MainActor
@Test func doctorIsOnlyReadWhenAskedFor() async {
    let model = AppModel(
        testing: Stub(.success(Status(now: 0, accounts: [], warnings: []))))
    await model.refresh()
    #expect(model.machine.checks.isEmpty)
    #expect(model.machine.checkedAt == nil)
    await model.machine.diagnose()
    #expect(model.machine.checks.map(\.code) == ["state"])
    #expect(model.machine.checkedAt != nil)
}

/// The account signed in but not enrolled is the one the app can record by itself: no
/// browser, no terminal. The sheet closes once the name is taken, and stays open with the
/// name in it when it is refused, so it can be corrected rather than typed again.
@MainActor
@Test func onlyAnUnenrolledSignedInAccountCanBeNamedHere() async throws {
    let stub = Stub(
        .success(
            Status(
                now: 0,
                accounts: [account(nil, signedIn: true, uuid: "a")], warnings: [])))
    let model = AppModel(testing: stub)
    await model.refresh()
    #expect(model.unenrolled)

    let naming = AccountSheet.name(provider: "claude", email: "a@example.com")
    model.sheet = naming
    stub.enrolling = .failure(
        PitboardError.Failed(
            code: "label_taken", cause: nil,
            message: "There is already an account called work.",
            warnings: [Warning(code: "auth_overridden", message: "ANTHROPIC_API_KEY is set")]))
    let refused = try #require(await model.enrol("work", for: "claude"))
    #expect(refused.title == "Couldn’t name this account")
    #expect(refused.message == "There is already an account called work.")
    #expect(model.sheet == naming, "the sheet stays open to say why")
    #expect(model.warnings.isEmpty, "said in the sheet, not in the panel")

    stub.enrolling = .success(
        Enrolled(email: "a@example.com", outcome: .current, warnings: []))
    #expect(await model.enrol("work", for: "claude") == nil)
    #expect(stub.enrolled == ["claude/work", "claude/work"])
    #expect(model.sheet == nil, "the sheet closes once it has been used")
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
    let model = AppModel(testing: stub)
    await model.refresh()
    #expect(model.footing == .unnamed(provider: "codex", email: "c@example.com"))
    await model.enrol("job", for: "codex")
    #expect(stub.enrolled == ["codex/job"])
}

/// Forgetting is destructive, so what the model does with a refusal matters. The refusal is
/// said to whoever asked, in the core's own words, and one that went through says nothing.
@MainActor
@Test func aRefusedForgetIsReported() async throws {
    let stub = Stub(.success(Status(now: 0, accounts: [], warnings: [])))
    let model = AppModel(testing: stub)
    #expect(await model.forget("claude/alpha") == nil)

    stub.forgetting = PitboardError.Failed(
        code: "cannot_forget_active_account", cause: nil,
        message: "beta is the account in use, so it cannot be forgotten.",
        warnings: [Warning(code: "auth_overridden", message: "ANTHROPIC_API_KEY is set")])
    let refused = try #require(await model.forget("claude/beta"))
    #expect(stub.forgot == ["claude/alpha", "claude/beta"])
    #expect(refused.title == "Couldn’t forget beta")
    #expect(refused.message == "beta is the account in use, so it cannot be forgotten.")
    #expect(refused.code == "cannot_forget_active_account")
    #expect(refused.warnings.map(\.code) == ["auth_overridden"])
    #expect(model.problem == nil, "a refusal is not a failed read")
    #expect(model.warnings.isEmpty, "its warnings are said with it, not in the panel")
}

/// A sign-in that cannot start says why in the sheet that started it, which stays open to
/// say it, and leaves nothing half-shown.
@MainActor
@Test func aSignInThatCannotStartIsReported() async throws {
    let model = AppModel(
        testing: Stub(.success(Status(now: 0, accounts: [], warnings: []))))
    model.sheet = .add(provider: nil)
    let failure = try #require(await model.signIn("work", for: "claude"))
    #expect(model.signingIn == nil)
    #expect(model.sheet == .add(provider: nil))
    #expect(failure.title == "Couldn’t sign in to work")
    #expect(failure.message == "`claude` is not on this machine")
    #expect(model.problem == nil)
}

/// Codex's sign-in prints an address and reads nothing, so the tool is named in the label
/// the core is given, and the sign-in finishes and enrols.
@MainActor
@Test func aCodexSignInIsForCodex() async {
    let stub = Stub(.success(status([])))
    let session = ScriptedSignIn(saying: ["https://auth.openai.com/oauth\n"])
    stub.session = session
    let model = AppModel(testing: stub)
    model.sheet = .add(provider: "codex")
    #expect(await model.signIn("work", for: "codex") == nil)
    #expect(stub.signedIn == ["codex/work"])
    #expect(session.finished)
    #expect(model.signingIn == nil, "finished and enrolled")
    #expect(model.sheet == nil, "and the sheet that started it is done")
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
    let session = ScriptedSignIn(saying: ["https://auth.openai.com/oauth\n"])
    session.enrolls = Enrolled(
        email: "w@example.com", outcome: .inUse(again: true), warnings: [oldLogin])
    stub.session = session
    let model = AppModel(testing: stub)

    #expect(await model.signIn("work", for: "codex") == nil)

    let last = try #require(model.lastSwitches.first)
    #expect(last.to == "codex/work")
    #expect(last.said == "Signed in to work again. Its new login is the one in use now.")
    #expect(last.notice == nil, "nothing switched away from anything")
    #expect(model.warnings(after: last) == [oldLogin])
}

/// A browser often signs in to the session it already has, so the account signed in now
/// can be enrolled by a sign-in under a new name. It says it was enrolled, not signed in to
/// again.
@MainActor
@Test func aFirstSignInToTheAccountInUseSaysItWasEnrolled() async throws {
    let stub = Stub(.success(status([account("work", of: "codex", signedIn: true)])))
    let session = ScriptedSignIn(saying: [])
    session.enrolls = Enrolled(
        email: "w@example.com", outcome: .inUse(again: false), warnings: [])
    stub.session = session
    let model = AppModel(testing: stub)

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
    let session = ScriptedSignIn(saying: [])
    session.enrolls = Enrolled(
        email: "w@example.com", outcome: .inUse(again: true), warnings: [oldLogin, parked])
    stub.session = session
    let model = AppModel(testing: stub)

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
    let session = ScriptedSignIn(saying: [])
    session.enrolls = Enrolled(email: "w@example.com", outcome: .renewed, warnings: [untold])
    stub.session = session
    let model = AppModel(testing: stub)

    await model.signIn("work", for: "codex")

    #expect(model.warnings == [untold])
    #expect(model.lastSwitches.isEmpty)
}

/// A sign-in refused before it started says what that refusal found on the way, such as a
/// switch interrupted earlier and finished now, not only why it was refused. The panel says
/// it too, since what was found is about the machine and outlives the sheet.
@MainActor
@Test func aRefusedSignInSaysWhatItFoundOnTheWay() async throws {
    let recovered = Warning(
        code: "interrupted_switch_undone", message: "an earlier switch was interrupted")
    let stub = Stub(.success(status([])))
    stub.signInWarnings = [recovered]
    let model = AppModel(testing: stub)

    let failure = try #require(await model.signIn("work", for: "claude"))

    #expect(failure.message == "`claude` is not on this machine")
    #expect(failure.warnings == [recovered])
    #expect(model.warnings == [recovered])
}

/// A sign-in of another account adds a row and says nothing more.
@MainActor
@Test func aSignInOfAnotherAccountSaysNothingMore() async {
    let stub = Stub(.success(status([account("work", of: "codex", signedIn: true)])))
    stub.session = ScriptedSignIn(saying: ["https://auth.openai.com/oauth\n"])
    let model = AppModel(testing: stub)
    await model.signIn("personal", for: "codex")
    #expect(model.lastSwitches.isEmpty)
}

/// Whether a code is asked for is the core's to read, in each tool's own words, and its
/// cases are the core's tests. The sheet asks it about the tool being signed in to, a code
/// typed back is not asked for again, and what is typed goes to the tool off the main thread.
@MainActor
@Test(.timeLimit(.minutes(1)))
func aCodeIsAskedForOnlyWhereTheToolTakesOne() async throws {
    for (provider, takes) in [("claude", true), ("codex", false)] {
        let stub = Stub(.success(status([])))
        let session = ScriptedSignIn(saying: ["Paste code here if prompted > "], waits: true)
        stub.session = session
        let model = AppModel(testing: stub)
        let running = Task { await model.signIn("work", for: provider) }

        #expect(await eventually { model.signingIn?.said.contains("Paste code") == true })
        #expect(model.signingIn?.wantsCode == takes)
        if takes {
            model.paste("abc")
            #expect(model.signingIn?.wantsCode == false, "asked once")
            #expect(await eventually { session.pasted == ["abc"] })
            #expect(session.pastedOnMain == false)
        }

        session.done.signal()
        #expect(await running.value == nil)
    }
}

/// Cancel stops the tool off the main thread: stopping waits for the tool, and a Codex
/// sign-in waiting on the browser held the whole app while it did. What the stopped tool
/// leaves behind is not a failure to report, and nothing is enrolled.
@MainActor
@Test(.timeLimit(.minutes(1)))
func aCancelledSignInStopsTheToolAndReportsNothing() async {
    let stub = Stub(.success(status([])))
    let session = ScriptedSignIn(
        saying: ["https://auth.openai.com/oauth/authorize?state=x\n"], waits: true)
    stub.session = session
    let model = AppModel(testing: stub)
    let running = Task { await model.signIn("work", for: "codex") }
    #expect(await eventually { model.signingIn?.url != nil })

    model.cancelSignIn()
    #expect(model.signingIn == nil)
    #expect(await eventually { session.cancelledOnMain != nil }, "the tool is stopped")
    // Let go by hand only when the cancel never reached the tool, so that fails here rather
    // than waiting for ever on a browser.
    if session.cancelledOnMain == nil { session.done.signal() }
    #expect(await running.value == nil, "somebody asked for it to stop")

    #expect(session.cancelledOnMain == false)
    #expect(!session.finished)
    #expect(model.warnings.isEmpty)
}

/// An account whose parked login can no longer be used is signed in to again from its row,
/// through the sign-in a new account gets, so the address Codex prints shows in the sheet
/// the same way. The sheet has the label alone, and the core is given it with its tool once,
/// as for a new account.
@MainActor
@Test(.timeLimit(.minutes(1)))
func anAccountThatCannotBeSwitchedToIsSignedInToAgainFromThePanel() async throws {
    let stub = Stub(
        .success(
            status([
                account("personal", of: "codex", signedIn: true),
                account("work", of: "codex", switchable: false),
                account("spare", switchable: false),
            ])))
    let session = ScriptedSignIn(
        saying: ["https://auth.openai.com/oauth/authorize?state=x\n"], waits: true)
    stub.session = session
    let model = AppModel(testing: stub)
    await model.refresh()
    let accounts = try #require(model.status?.accounts)

    let work = AccountDescription(accounts[1], switching: nil, busy: false).action
    #expect(work == .signInAgain(provider: "codex", label: "work"))
    model.sheet = .signInAgain(provider: "codex", label: "work")
    let running = Task { await model.signIn("work", for: "codex") }
    #expect(await eventually { model.signingIn?.url != nil })
    #expect(stub.signedIn == ["codex/work"])
    #expect(model.signingIn?.tool == "Codex")
    #expect(model.signingIn?.label == "work")
    session.done.signal()
    #expect(await running.value == nil)
    #expect(session.finished)
    #expect(model.signingIn == nil)
    #expect(model.sheet == nil)

    let spare = AccountDescription(accounts[2], switching: nil, busy: false).action
    #expect(spare == .signInAgain(provider: "claude", label: "spare"))
    stub.session = ScriptedSignIn(saying: [])
    model.sheet = .signInAgain(provider: "claude", label: "spare")
    await model.signIn("spare", for: "claude")
    #expect(stub.signedIn == ["codex/work", "claude/spare"])
}

/// A stand-in `Pitboard.app` in `directory`, with a command line inside it that can be run.
/// Made by the test, so nothing depends on whether the Mac running it has Pitboard
/// installed. The test removes `directory`.
private func standInApp(in directory: URL) throws -> URL {
    let app = directory.appendingPathComponent("Pitboard.app")
    let helper = app.appendingPathComponent("Contents/Helpers/pitboard")
    try FileManager.default.createDirectory(
        at: helper.deletingLastPathComponent(), withIntermediateDirectories: true)
    try Data("#!/bin/sh\n".utf8).write(to: helper)
    try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: helper.path)
    return app
}

/// The settings say which `pitboard` a terminal runs, looking where the login shell's
/// `PATH` says before anywhere else, and whether it is this app's own. Where the shell could
/// not be asked, it is the first one found where a way of installing Pitboard puts it.
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
    let scripts = Scripts()
    let model = AppModel(
        testing: stub,
        commandLineTool: CommandLineTool(
            bundle: root.appendingPathComponent("Pitboard.app"), home: home.path,
            execute: scripts.run))
    let machine = model.machine
    await model.refresh()
    #expect(machine.commandLine == nil, "not looked for until the settings ask")
    await machine.findCommandLine()
    #expect(machine.commandLine == .bundled(linked), "ahead of the one cargo installed")

    let other = AppModel(
        testing: stub,
        commandLineTool: CommandLineTool(bundle: root, home: home.path, execute: scripts.run))
    await other.machine.findCommandLine()
    #expect(other.machine.commandLine == .another(linked), "not run from an app")

    stub.path = nil
    await machine.findCommandLine()
    #expect(machine.commandLine == .another(cargo.path), "the login shell could not be asked")
    #expect(scripts.ran.isEmpty, "looking links nothing")
}

/// Why a link could not be made is said beside the button that tried, and a dismissed
/// password prompt says nothing: it was somebody's answer, and it puts away what an earlier
/// try said.
@MainActor
@Test func aDismissedPasswordPromptIsNotShownAsAFailure() async throws {
    let scratch = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboard-link-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: scratch) }
    let root = scratch.path
    let scripts = Scripts()
    let model = AppModel(
        testing: Stub(.success(status([]))),
        commandLineTool: CommandLineTool(
            bundle: try standInApp(in: scratch), home: root, link: "\(root)/bin/pitboard",
            execute: scripts.run))

    scripts.raise(1, "ln: \(root)/bin/pitboard: Permission denied")
    await model.machine.installCommandLine()
    #expect(model.machine.linkFailed == "ln: \(root)/bin/pitboard: Permission denied")
    #expect(model.problem == nil, "said beside the button, not as a failed read")
    scripts.raise(-128, "User canceled.")
    await model.machine.installCommandLine()
    #expect(model.machine.linkFailed == nil)
    #expect(scripts.ran.count == 2)
}

/// Daily renewal is turned on only from an app with a command line inside it that stays
/// where it is. The schedule runs it long after the app has quit: a copy macOS runs from a
/// temporary place is gone by then, and a build with none inside it would schedule the app
/// itself, which renews nothing. Why it was refused is said beside the switch that tried,
/// not as a read that failed. Turning it off is always possible, so a schedule that cannot
/// work can be taken away.
@MainActor
@Test func dailyRenewalIsTurnedOnOnlyFromAnAppThatStaysWhereItIs() async throws {
    func model(_ bundle: String) -> (AppModel, Stub) {
        let stub = Stub(.success(status([])))
        let model = AppModel(
            testing: stub,
            commandLineTool: CommandLineTool(
                bundle: URL(fileURLWithPath: bundle), execute: Scripts().run))
        return (model, stub)
    }

    let scratch = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboard-schedule-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: scratch) }
    let (installed, stub) = model(try standInApp(in: scratch).path)
    #expect(installed.machine.cannotSchedule == nil)
    await installed.machine.setSchedule(on: true)
    #expect(stub.scheduleInstalls == 1)
    #expect(installed.machine.scheduleFailed == nil)

    let (downloaded, temporary) = model(
        "/private/var/folders/xy/abc/T/AppTranslocation/0A1B2C/d/Pitboard.app")
    #expect(
        downloaded.machine.cannotSchedule?.hasPrefix("Move Pitboard to your Applications")
            == true)
    let (built, unbundled) = model("/Users/x/pitboard/apple/.build/debug")
    #expect(built.machine.cannotSchedule?.contains("no command line inside it") == true)
    for (refused, stub) in [(downloaded, temporary), (built, unbundled)] {
        await refused.machine.setSchedule(on: true)
        #expect(stub.scheduleInstalls == 0)
        #expect(refused.machine.scheduleFailed == refused.machine.cannotSchedule)
        #expect(refused.problem == nil)
        await refused.machine.setSchedule(on: false)
        #expect(stub.scheduleUninstalls == 1)
        #expect(refused.machine.scheduleFailed == nil)
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
    let model = AppModel(testing: stub, watching: true)
    #expect(await eventually { model.machine.schedule == installed })
    #expect(stub.repairAsks == 1)

    let refused = PitboardError.Failed(
        code: "schedule_refused", cause: nil, message: "the scheduler refused: no",
        warnings: [])
    for answer: Result<Bool, Error> in [.success(false), .failure(refused)] {
        let quiet = Stub(.success(status([])))
        quiet.scheduled = installed
        quiet.repairs = answer
        let model = AppModel(testing: quiet)
        #expect(quiet.repairAsks == 0, "a test drives it itself")
        await model.machine.repairSchedule()
        #expect(quiet.repairAsks == 1)
        #expect(quiet.scheduleReads == 0)
        #expect(model.machine.schedule == .absent)
        #expect(model.machine.scheduleFailed == nil)
        #expect(model.problem == nil)
    }
}

/// A timer producing a reading is not somebody asking for one, and the core decides whether
/// to go to Anthropic from that. The panel's own Refresh is asking; everything else is not.
@MainActor
@Test func onlyAskingForAReadingAsksAnthropicAgain() async {
    let stub = Stub(.success(Status(now: 0, accounts: [], warnings: [])))
    let model = AppModel(testing: stub)

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
    let model = AppModel(testing: stub)

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

/// Every session's status line records what its session has seen, and a reading only moves
/// forward. The menu bar read 20% while every status line said 22%, because it only ever
/// showed what it had asked Anthropic itself. It follows the readings the way it follows a
/// switch made elsewhere: from what is already known, asking nobody.
@MainActor
@Test func numbersASessionRecordedReachTheMenuBarWithoutAskingAnyone() async {
    let stub = Stub(.success(status([account("work", signedIn: true, percent: 20)])))
    let model = AppModel(testing: stub)
    await model.refresh()
    #expect(model.title == "work 20%")

    stub.offline = .success(status([account("work", signedIn: true, percent: 22)]))
    stub.readings += 1
    await model.noticeOtherChangesForTesting()

    #expect(model.title == "work 22%")
    #expect(stub.offlineReads == 1, "it read what is already known")
    #expect(stub.freshAsks == 0, "and asked Anthropic nothing")
}

/// A reading moving says nothing about who is signed in. Taken for a change to the account
/// index, it would have who is signed in read again from Claude Code's config, which a
/// switch that could not update it leaves naming the account before, and so put away the one
/// warning saying so. Numbers move several times a minute, so within seconds of the switch.
@MainActor
@Test func numbersMovingLeaveWhoIsSignedInAndWhatASwitchSaid() async {
    let lagging = Warning(code: "config_write_failed", message: "the config did not update")
    let stub = Stub(
        .success(
            status([
                account("a", signedIn: false, percent: 10),
                account("b", signedIn: true, percent: 10),
            ])))
    stub.switched = switched("claude", from: "a", to: "b", warnings: [lagging])
    let model = AppModel(testing: stub)
    await model.use("claude/b")
    #expect(model.lastSwitches.first?.warnings == [lagging])

    stub.offline = .success(
        status([
            account("a", signedIn: true, percent: 10),
            account("b", signedIn: false, percent: 30),
        ]))
    stub.readings += 1
    await model.noticeOtherChangesForTesting()

    #expect(model.status?.accounts.last?.usage?.windows.first?.percent == 30)
    #expect(model.status?.accounts.map(\.signedIn) == [false, true], "only the numbers moved")
    #expect(model.lastSwitches.first?.warnings == [lagging], "and what the switch said stands")
}

/// The app's own read records what it measured, which moves the readings, and a session can
/// record something newer while the app is asking. Either costs one look at what is
/// recorded, which is a file, and never a second read of anyone.
@MainActor
@Test func theAppsOwnReadCostsOneLookAtTheReadingsAtMost() async {
    let stub = Stub(.success(status([account("work", signedIn: true, percent: 20)])))
    stub.offline = stub.answer
    let model = AppModel(testing: stub)
    await model.noticeOtherChangesForTesting()
    await model.refresh()
    await model.noticeOtherChangesForTesting()
    await model.noticeOtherChangesForTesting()
    #expect(stub.offlineReads == 1)
    #expect(stub.freshAsks == 0)
    #expect(model.title == "work 20%")
}

/// Anthropic's answer can be behind what a busy session records while the app is waiting
/// on it. The file then holds the session's newer numbers, and the app's own read writes
/// nothing over them. Noted as seen when the read was done, they reached the menu bar only
/// once some session wrote again.
@MainActor
@Test func numbersASessionRecordedDuringTheAppsOwnReadAreShownOnTheNextLook() async {
    let stub = Stub(.success(status([account("work", signedIn: true, percent: 21)])))
    let model = AppModel(testing: stub)
    await model.noticeOtherChangesForTesting()
    stub.offline = .success(status([account("work", signedIn: true, percent: 22)]))
    await model.refresh()
    #expect(model.title == "work 21%")

    await model.noticeOtherChangesForTesting()
    #expect(model.title == "work 22%")
}

/// A switch this app has in flight is its own change, so the poll leaves it alone. Numbers a
/// session records meanwhile are somebody else's, and a switch that fails reads nothing
/// after it: seen then, they were never shown.
@MainActor
@Test func numbersRecordedDuringASwitchAreTakenOnceItIsOver() async {
    let stub = Stub(.success(status([account("work", signedIn: true, percent: 20)])))
    stub.switched = .failure(
        PitboardError.Failed(
            code: "nothing_parked", cause: nil, message: "nothing parked", warnings: []))
    let model = AppModel(testing: stub)
    await model.refresh()
    await model.noticeOtherChangesForTesting()
    stub.duringSwitch = {
        stub.offline = .success(status([account("work", signedIn: true, percent: 22)]))
        stub.readings += 1
        await model.noticeOtherChangesForTesting()
    }
    await model.use("claude/personal")
    #expect(model.title == "work 20%")

    await model.noticeOtherChangesForTesting()
    #expect(model.title == "work 22%")
}

/// A session's status line records that the account in use has run out, and the menu bar
/// shows it within seconds. The advice to switch, and its notification, came only with the
/// app's next read of its own, minutes later.
@MainActor
@Test func numbersThatRunAnAccountOutAdviseAtOnceAndTellItOnce() async {
    let work = { (percent: Double, resets: Int64) in
        account("work", signedIn: true, [window("session", percent, resets: resets)])
    }
    let personal = account("personal", [window("session", 10, resets: 9_000)])
    let stub = Stub(.success(status([work(90, 7_200), personal])))
    let model = AppModel(testing: stub)
    await model.refresh()
    #expect(model.advice.isEmpty)

    stub.offline = .success(status([work(100, 7_200), personal]))
    stub.readings += 1
    await model.noticeOtherChangesForTesting()
    #expect(model.advice.map(\.switchTo) == ["claude/personal"])
    let told = Advice.key("claude", "work", window("session", 100))
    #expect(model.toldForTesting == [told: 7_200])

    // The same window again, as another source rounds its reset. Numbers move with every
    // session's response, and advice put away seconds after it was told would be gone
    // before anybody opened the panel.
    stub.offline = .success(status([work(100, 7_201), personal]))
    stub.readings += 1
    await model.noticeOtherChangesForTesting()
    #expect(model.advice.map(\.switchTo) == ["claude/personal"], "still true, so still said")
    #expect(model.toldForTesting == [told: 7_200], "and told once")
}

/// Advice says the account in use has none of a limit left. Once what is recorded shows the
/// window after it, with room, that is no longer true.
@MainActor
@Test func adviceTheNumbersNoLongerBearOutIsPutAway() async {
    let personal = account("personal", [window("session", 10, resets: 9_000)])
    let stub = Stub(
        .success(
            status([
                account("work", signedIn: true, [window("session", 100, resets: 7_200)]),
                personal,
            ])))
    let model = AppModel(testing: stub)
    await model.refresh()
    #expect(model.advice.count == 1)

    stub.offline = .success(
        status([
            account("work", signedIn: true, [window("session", 3, resets: 25_200)]), personal,
        ]))
    stub.readings += 1
    await model.noticeOtherChangesForTesting()
    #expect(model.advice.isEmpty)
}

/// Advice told when what sessions recorded ran the account out stays while the numbers bear
/// it out, whatever read comes next. The app's own read worked advice out afresh, leaving out
/// what had been told, so opening the panel put it away with the account still at 100%.
@MainActor
@Test func adviceToldFromWhatSessionsRecordedOutlastsTheAppsNextRead() async {
    let work = { (percent: Double) in
        account("work", signedIn: true, [window("session", percent, resets: 7_200)])
    }
    let personal = account("personal", [window("session", 10, resets: 9_000)])
    let stub = Stub(.success(status([work(90), personal])))
    let model = AppModel(testing: stub)
    await model.refresh()
    stub.offline = .success(status([work(100), personal]))
    stub.readings += 1
    await model.noticeOtherChangesForTesting()
    #expect(model.advice.map(\.switchTo) == ["claude/personal"])

    stub.answer = .success(status([work(100), personal]))
    await model.refresh()
    #expect(model.title == "work 100%")
    #expect(model.advice.map(\.switchTo) == ["claude/personal"])
}

/// The same for advice a read told: the next read found the window already told about and
/// left it out, so it was gone a minute later, or at once if the panel was opened again.
@MainActor
@Test func adviceToldByAReadOutlastsTheNextReadAndIsToldOnce() async {
    let work = account("work", signedIn: true, [window("session", 100, resets: 7_200)])
    let personal = account("personal", [window("session", 10, resets: 9_000)])
    let stub = Stub(.success(status([work, personal])))
    let model = AppModel(testing: stub)
    await model.refresh()
    #expect(model.advice.count == 1)

    await model.refresh()
    #expect(model.advice.map(\.switchTo) == ["claude/personal"])
    let told = Advice.key("claude", "work", window("session", 100))
    #expect(model.toldForTesting == [told: 7_200])
}

/// A change to the account index can leave the account in use run out, and the poll that
/// notices it reads who is signed in and the numbers again. What they show is advised then,
/// not at the app's next read of its own.
@MainActor
@Test func theAccountIndexChangingElsewhereIsAdvisedOnAtOnce() async {
    let work = { (percent: Double) in
        account("work", signedIn: true, [window("session", percent, resets: 7_200)])
    }
    let personal = account("personal", [window("session", 10, resets: 9_000)])
    let stub = Stub(.success(status([work(20), personal])))
    let model = AppModel(testing: stub)
    await model.refresh()
    #expect(model.advice.isEmpty)

    stub.offline = .success(status([work(100), personal]))
    stub.changed += 1
    await model.noticeOtherChangesForTesting()
    #expect(model.advice.map(\.switchTo) == ["claude/personal"])
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
    let model = AppModel(testing: stub)

    await model.refresh()

    #expect(model.problem == "could not reach Anthropic")
    #expect(model.status?.accounts.count == 1, "the last numbers measured are still true")
}

/// Every warning, not only the first. A switch can warn about an overriding environment
/// variable and a config that did not update, and showing one is how somebody fixes the
/// wrong thing. A read that answered did not fail, however much it warns about, so none of
/// them is said as a read that could not be done.
@MainActor
@Test func everyWarningIsKeptNotOnlyTheFirst() async {
    let model = AppModel(
        testing: Stub(
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
    #expect(model.problem == nil)
    #expect(model.problemCode == nil)
    #expect(model.otherWarnings == model.warnings)
}

// MARK: - What a machine that is not set up yet is told to do

/// A new install of the app. Before the first read there is nothing true to say, and a
/// setup step shown to somebody who finished it years ago is worse than silence.
@MainActor
@Test func nothingIsAskedOfAnyoneBeforeTheFirstRead() {
    let model = AppModel(
        testing: Stub(.success(Status(now: 0, accounts: [], warnings: []))))
    #expect(model.footing == .ready)
}

/// The one state Pitboard cannot do anything about. It has to say so rather than show an
/// empty panel, which reads as an app that does not work. The core's read succeeds with
/// nothing to list on such a machine, so what says it is that no tool was found. It was told
/// by a read failing for a missing program, which no read does, and was never shown.
@MainActor
@Test func aMachineWithoutClaudeCodeIsToldThatFirst() async {
    let stub = Stub(.success(status([])))
    stub.found = []
    let model = AppModel(testing: stub)
    #expect(model.footing == .ready, "nothing is said before the first read")
    await model.refresh()
    #expect(model.footing == .noClaudeCode)
}

/// A tool that was found, or a login or an account of any tool, means a tool is here, and
/// the machine is one to set up rather than one to install on.
@MainActor
@Test func aToolFoundOrSignedInIsNotAMachineWithoutOne() async {
    let codexOnly = Stub(.success(status([])))
    codexOnly.found = [codex]
    let found = AppModel(testing: codexOnly)
    await found.refresh()
    #expect(found.footing == .noOneSignedIn)

    let signedIn = Stub(.success(status([account("job", of: "codex", signedIn: true)])))
    signedIn.found = []
    let unfound = AppModel(testing: signedIn)
    await unfound.refresh()
    #expect(unfound.footing != .noClaudeCode)
}

/// A login shell too slow to answer finds nothing the first time, and the service asks it
/// once more later, so a read asks again while nothing has been found. Once a tool is found
/// the question stops.
@MainActor
@Test func whatIsInstalledIsAskedAgainWhileNothingIsFound() async {
    let stub = Stub(.success(status([])))
    stub.found = []
    let model = AppModel(testing: stub)
    await model.refresh()
    #expect(model.footing == .noClaudeCode)

    stub.found = [claudeCode]
    await model.refresh()
    #expect(stub.installedAsks == 2)
    #expect(model.footing == .noOneSignedIn)
    await model.refresh()
    #expect(stub.installedAsks == 2)
}

@MainActor
@Test func anEmptyMachineIsAskedToSignInOnce() async {
    let model = AppModel(
        testing: Stub(.success(Status(now: 0, accounts: [], warnings: []))))
    await model.refresh()
    #expect(model.footing == .noOneSignedIn)
}

/// A login with no name cannot be parked, so this is the step between signing in and
/// Pitboard being able to do anything at all. Until it has one it is called by its email
/// address, which is how the person knows it.
@MainActor
@Test func anAccountSignedInWithoutANameIsAskedForOne() async throws {
    let model = AppModel(
        testing: Stub(
            .success(
                Status(
                    now: 0,
                    accounts: [account(nil, signedIn: true, uuid: "a")], warnings: []))))
    await model.refresh()
    #expect(model.footing == .unnamed(provider: "claude", email: "a@example.com"))
    let login = try #require(model.unnamed.first)
    #expect(model.name(of: login) == "a@example.com")
}

@MainActor
@Test func oneEnrolledAccountIsToldThereIsNothingToSwitchTo() async {
    let model = AppModel(
        testing: Stub(.success(status([account("work", signedIn: true, percent: 10)]))))
    await model.refresh()
    #expect(model.footing == .onlyOne(provider: "claude", label: "work"))
}

@MainActor
@Test func twoAccountsAreAskedNothing() async {
    let model = AppModel(
        testing: Stub(
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
/// to sign in again there would have them sign in over an account Pitboard already holds.
@MainActor
@Test func enrolledAccountsWithNobodySignedInAreNotAskedToStartOver() async {
    let model = AppModel(
        testing: Stub(.success(status([account("work", signedIn: false, percent: 10)]))))
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
    let model = AppModel(testing: stub)
    await model.refresh()
    #expect(model.warnings.map(\.code) == ["state_on_synced_drive"])
    #expect(model.problem == nil, "a warning is not a failed read")

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
        testing: Stub(.success(status([account("work", of: "codex", signedIn: true)]))))
    await model.refresh()
    #expect(model.footing == .onlyOne(provider: "codex", label: "work"))
}

/// A Claude Code account and a Codex account are two accounts and nothing to switch to:
/// an account is only ever switched to another of its own tool.
@MainActor
@Test func onlyOneAccountIsCountedPerTool() async {
    let model = AppModel(
        testing: Stub(
            .success(
                status([
                    account("work", signedIn: true),
                    account("spare"),
                    account("job", of: "codex", signedIn: true),
                ]))))
    await model.refresh()
    #expect(model.footing == .onlyOne(provider: "codex", label: "job"))
}

/// A login Pitboard could not read, or cannot switch, has no account behind it. Offering
/// to name it would enrol something that can never be switched to.
@MainActor
@Test func anUnplacedLoginIsNeitherUnenrolledNorOfferedAName() async {
    for row in [unplaced(of: "codex"), unplaced(of: "codex", signedIn: true)] {
        let model = AppModel(
            testing: Stub(
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
    let model = AppModel(testing: stub)
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
    let model = AppModel(testing: stub)
    await model.refresh()
    #expect(model.addable == [claudeCode])
    #expect(
        model.notOffered == "Codex is not offered: Pitboard did not find codex on this Mac.")

    stub.found = [codex]
    let codexOnly = AppModel(testing: stub)
    await codexOnly.refresh()
    #expect(codexOnly.addable == [codex])
    #expect(codexOnly.provider(for: .add(provider: nil)) == "codex")

    stub.found = bothTools
    let both = AppModel(testing: stub)
    await both.refresh()
    #expect(both.notOffered == nil)

    // A tool with an account here is here, wherever its program is.
    stub.answer = .success(status([account("work", of: "codex", signedIn: true)]))
    stub.found = [claudeCode]
    let known = AppModel(testing: stub)
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
    let model = AppModel(testing: stub)
    await model.refresh()
    #expect(model.addable == [claudeCode])
    #expect(model.services == "Anthropic")
    #expect(model.provider(for: .add(provider: nil)) == "claude")
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
    let model = AppModel(testing: stub)
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
    let model = AppModel(testing: stub)
    await model.refresh()
    #expect(model.addable == [claudeCode])

    stub.found = bothTools
    model.sheet = .add(provider: nil)
    #expect(await eventually { model.addable == bothTools })
    #expect(stub.installedAsks == 2)

    model.sheet = nil
    model.sheet = .name(provider: "claude", email: "a@example.com")
    await model.refresh()
    #expect(stub.installedAsks == 2, "not for naming the account in use, nor for a read")
}

/// The form starts on the tool it was asked about, and otherwise on the first it offers.
@MainActor
@Test func theFormStartsOnTheToolItIsAbout() {
    let model = AppModel(testing: Stub(.success(status([]))))
    #expect(model.provider(for: .add(provider: nil)) == "claude")
    #expect(model.provider(for: .add(provider: "codex")) == "codex")
    #expect(model.provider(for: .name(provider: "codex", email: "c@example.com")) == "codex")
    #expect(model.provider(for: .signInAgain(provider: "codex", label: "work")) == "codex")
    #expect(model.provider(for: .rename(provider: "codex", label: "work")) == "codex")
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
    let defaults = TestDefaults()
    let model = AppModel(testing: stub, defaults: defaults)
    await model.refresh()
    #expect(model.footing == .onlyOne(provider: "claude", label: "work"))

    model.declineSecondAccount(for: "claude")
    #expect(model.footing == .onlyOne(provider: "codex", label: "job"))

    let later = AppModel(testing: stub, defaults: defaults)
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
    let defaults = TestDefaults()
    defaults.set(true, forKey: "hideSecondAccountNudge")
    let model = AppModel(testing: stub, defaults: defaults)
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
    let model = AppModel(testing: stub)
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
    let model = AppModel(testing: stub)
    #expect(model.spokenTitle == "Pitboard")
    await model.refresh()
    #expect(model.spokenTitle == "Pitboard, work 42%")

    stub.answer = .success(
        status([
            account("work", signedIn: true, percent: 60),
            account(
                "work", of: "codex", signedIn: true, [window("five_hour", 80, resets: nil)]),
        ]))
    await model.refresh()
    #expect(model.spokenTitle == "Pitboard, work 80%, Codex")
}

// MARK: - A read that lands after a change

/// What this app changes, or sees changed, while one of its reads waits on a service.
enum ChangeMidRead: CaseIterable {
    case switched, enrolled, renamed, forgotten, signedIn, gaveUp, noticed
}

/// A read waits on a service with who was signed in when it started, and whatever is changed
/// meanwhile, by this app or somewhere else the poll noticed, is changed before the read
/// lands. Taken as it was, the read showed the accounts as they had been and put away what
/// the change had said, a Codex switch's warning that sessions keep the account it left among
/// it. It is dropped: the read the change starts itself says what is true now.
@MainActor
@Test(arguments: ChangeMidRead.allCases)
func aReadThatStartedBeforeAChangeIsDroppedWhenItLands(_ change: ChangeMidRead) async {
    let before = [
        account("personal", of: "codex", signedIn: true), account("work", of: "codex"),
        account("spare", of: "codex"),
    ]
    let after = status([
        account("personal", of: "codex"), account("work", of: "codex", signedIn: true),
    ])
    let stub = Stub(.success(status(before)))
    let model = AppModel(testing: stub)
    await model.refresh()

    // What the held read finds when it starts, with a warning nothing after it carries.
    let overridden = Warning(code: "auth_overridden", message: "OPENAI_API_KEY is set")
    stub.answer = .success(status(before, warnings: [overridden]))
    let gate = Gate()
    stub.duringRead = { await gate.pass() }
    let held = Task { await model.refresh(asked: true) }
    #expect(await eventually { gate.arrivals == 1 })
    stub.duringRead = nil

    stub.answer = .success(after)
    stub.offline = .success(after)
    switch change {
    case .switched:
        stub.switched = switched(
            "codex", from: "codex/personal", to: "codex/work", warnings: [stillRunning])
        await model.use("codex/work")
        #expect(model.lastSwitches.map(\.to) == ["codex/work"])
    case .enrolled:
        await model.enrol("job", for: "codex")
    case .renamed:
        await model.rename("spare", of: "codex", to: "home")
    case .forgotten:
        await model.forget("codex/spare")
    case .signedIn:
        stub.session = ScriptedSignIn(saying: [])
        await model.signIn("travel", for: "codex")
    case .gaveUp:
        stub.abandoned = Abandoned(from: "personal", to: "work", loginsKept: 2)
        await model.abandonStuckSwitch()
    case .noticed:
        stub.changed += 1
        await model.noticeOtherChangesForTesting()
    }
    #expect(model.status == after)
    let said = (switches: model.lastSwitches, warnings: model.warnings, at: model.updatedAt)

    gate.open()
    await held.value
    #expect(model.status == after, "not the accounts as they were before the change")
    #expect(model.lastSwitches == said.switches)
    #expect(model.warnings == said.warnings)
    #expect(model.updatedAt == said.at)
    #expect(!model.reading)
}

/// The same for a read that fails: started before a switch and landing after it, what it
/// failed on is the machine as it was, and it is not said over the read the switch started.
@MainActor
@Test func aFailedReadThatStartedBeforeASwitchSaysNothingOnceItLands() async {
    let stub = Stub(
        .success(
            status([
                account("personal", of: "codex", signedIn: true), account("work", of: "codex"),
            ])))
    let model = AppModel(testing: stub)
    await model.refresh()

    let unreachable = Warning(code: "unreachable", message: "OpenAI could not be reached")
    stub.answer = .failure(
        PitboardError.Failed(
            code: "unreachable", cause: nil, message: unreachable.message,
            warnings: [unreachable]))
    let gate = Gate()
    stub.duringRead = { await gate.pass() }
    let held = Task { await model.refresh(asked: true) }
    #expect(await eventually { gate.arrivals == 1 })
    stub.duringRead = nil

    let after = status([
        account("personal", of: "codex"), account("work", of: "codex", signedIn: true),
    ])
    stub.answer = .success(after)
    stub.switched = switched(
        "codex", from: "codex/personal", to: "codex/work", warnings: [stillRunning])
    await model.use("codex/work")

    gate.open()
    await held.value
    #expect(model.problem == nil)
    #expect(model.problemCode == nil)
    #expect(model.warnings.isEmpty)
    #expect(model.status == after)
    #expect(model.lastSwitches.first?.warnings == [stillRunning])
}

/// A switch typed in a terminal while the app's read waits on a service changes the account
/// index after the read has taken who is signed in. Counted as seen once the read landed, it
/// was never shown; counted as things stood when the read started, the next look finds it.
@MainActor
@Test func aChangeMadeDuringAReadIsNoticedOnTheNextLook() async {
    let before = status([account("work", signedIn: true), account("personal")])
    let elsewhere = status([account("work"), account("personal", signedIn: true)])
    let stub = Stub(.success(before))
    stub.duringRead = {
        stub.changed += 1
        stub.offline = .success(elsewhere)
    }
    let model = AppModel(testing: stub)
    await model.refresh()
    #expect(model.status == before)

    stub.duringRead = nil
    await model.noticeOtherChangesForTesting()
    #expect(model.status == elsewhere)
    #expect(stub.offlineReads == 1)
    #expect(stub.freshAsks == 0)
}

/// A switch that fails can still have moved who is signed in, by finishing a switch that was
/// interrupted before it. The poll leaves the account index alone while a switch runs, since
/// the change could be the switch's own, and a failed switch reads nothing after it, so the
/// poll finds the change on its next look rather than counting it as seen.
@MainActor
@Test func aChangeThePollSawDuringASwitchIsNoticedOnceTheSwitchIsOver() async {
    let before = status([account("work", signedIn: true), account("personal")])
    let finished = status([account("work"), account("personal", signedIn: true)])
    let stub = Stub(.success(before))
    stub.switched = .failure(
        PitboardError.Failed(
            code: "parked_login_expired", cause: nil,
            message: "spare's parked login has expired",
            warnings: [
                Warning(
                    code: "interrupted_switch_finished",
                    message: "An interrupted switch to personal was finished.")
            ]))
    let model = AppModel(testing: stub)
    await model.refresh()
    stub.duringSwitch = {
        stub.changed += 1
        stub.offline = .success(finished)
        await model.noticeOtherChangesForTesting()
    }
    await model.use("claude/spare")
    #expect(model.status == before, "left alone while the switch ran")
    #expect(stub.offlineReads == 0)

    await model.noticeOtherChangesForTesting()
    #expect(model.status == finished)
    #expect(stub.offlineReads == 1)
}

/// Reads overlap: the timer's with one somebody asked for, a menu opening with the read after
/// a switch. The refresh buttons say a read is running while either is, and the first to end
/// used to say none was while the other still waited on a service.
@MainActor
@Test func aReadIsRunningWhileEitherOfTwoIs() async {
    let stub = Stub(.success(status([account("work", signedIn: true)])))
    let gate = Gate()
    stub.duringRead = { await gate.pass() }
    let model = AppModel(testing: stub)
    #expect(!model.reading)
    let first = Task { await model.refresh() }
    let second = Task { await model.refresh(asked: true) }
    #expect(await eventually { gate.arrivals == 2 })
    #expect(model.reading)

    gate.letOneThrough()
    #expect(await eventually { model.updatedAt != nil })
    #expect(model.reading, "the other is still waiting")

    gate.letOneThrough()
    await first.value
    await second.value
    #expect(!model.reading)
}

/// One switch at a time. Another chosen from the menu or a notification while one runs was
/// chosen from a menu that did not yet show the first, and would switch again behind it.
@MainActor
@Test func aSwitchAskedForWhileOneRunsDoesNothing() async {
    let stub = Stub(
        .success(
            status([account("personal", signedIn: true), account("work"), account("spare")])))
    stub.switched = switched("claude", from: "work", to: "personal")
    let model = AppModel(testing: stub)
    stub.duringSwitch = {
        stub.duringSwitch = nil
        await model.switchAsked(to: "claude/spare")
    }
    await model.switchAsked(to: "claude/personal")
    #expect(stub.switchedTo == ["claude/personal"])
    #expect(model.presentedFailure == nil)
    #expect(model.windowRequests == 0)
}

/// Opening a menu reads the accounts again only once the numbers shown are a minute old:
/// usage is asked of each tool's service for every account, and a menu is opened far more
/// often than the numbers change. Before anything has been read there is nothing to keep.
@MainActor
@Test func openingAMenuReadsOnlyNumbersAMinuteOld() async throws {
    let stub = Stub(.success(status([account("work", signedIn: true, percent: 10)])))
    let model = AppModel(testing: stub)
    #expect(AppModel.staleAfter == 60)
    await model.refresh(ifOlderThan: AppModel.staleAfter)
    #expect(stub.readings == 1, "nothing was read yet")

    await model.refresh(ifOlderThan: AppModel.staleAfter)
    #expect(stub.readings == 1, "read under a minute ago")

    try await Task.sleep(for: .milliseconds(20))
    await model.refresh(ifOlderThan: 0.01)
    #expect(stub.readings == 2, "older than asked for")
    #expect(stub.freshAsks == 0, "and nobody asked for it")
}

// MARK: - Advice and renames

/// Advice offers the account of the same tool with the most left, as a menu item and a button
/// that switch to it, and stays while the account in use is still out. What it offers is
/// worked out again from every read: the account offered may have been forgotten, may need
/// signing in again or may have run out itself since, and choosing it then failed. What it
/// says is left follows the numbers, and with nothing left to offer the advice goes.
@MainActor
@Test func adviceOffersWhatCanStillBeUsedAfterEveryRead() async {
    let work = account("work", signedIn: true, [window("session", 100, resets: 7_200)])
    let other = { (label: String, percent: Double, switchable: Bool) in
        account(label, switchable: switchable, [window("session", percent, resets: 9_000)])
    }
    let stub = Stub(
        .success(
            status([
                work, other("personal", 10, true), other("side", 40, true),
                other("extra", 50, true),
            ])))
    let model = AppModel(testing: stub)
    let offered = { model.advice.map { "\($0.switchTo) \($0.left)" } }
    await model.refresh()
    #expect(offered() == ["claude/personal 90"])

    stub.answer = .success(
        status([
            work, other("personal", 30, true), other("side", 40, true),
            other("extra", 50, true),
        ]))
    await model.refresh()
    #expect(offered() == ["claude/personal 70"], "what is left follows the numbers")

    stub.answer = .success(status([work, other("side", 40, true), other("extra", 50, true)]))
    await model.refresh()
    #expect(offered() == ["claude/side 60"], "personal was forgotten")

    stub.answer = .success(status([work, other("side", 40, false), other("extra", 50, true)]))
    await model.refresh()
    #expect(offered() == ["claude/extra 50"], "side needs signing in again")

    stub.answer = .success(status([work, other("side", 40, false), other("extra", 100, true)]))
    await model.refresh()
    #expect(model.advice.isEmpty, "extra has run out too")
    #expect(model.toldForTesting.count == 1, "told once, when work ran out")
}

/// A rename changes what an account is called and nothing else about it. What its tool's last
/// switch said is still true of it, and so is advice about it running out, so both are said
/// under the new name, and one tool's rename says nothing about another tool's accounts.
/// Keyed by the old name, the read after a rename took the switch for undone and the advice
/// for new, and told it again.
@MainActor
@Test func aRenameCarriesWhatWasSaidAboutTheAccount() async throws {
    let spent = window("session", 100, resets: 7_200)
    let room = window("session", 10, resets: 9_000)
    let stub = Stub(
        .success(
            status([
                account("work", signedIn: true, [spent]), account("personal", [room]),
                account("work", of: "codex", signedIn: true), account("personal", of: "codex"),
            ])))
    let model = AppModel(testing: stub)
    stub.switched = switched("claude", from: "personal", to: "work")
    await model.use("claude/work")
    stub.switched = switched("codex", from: "codex/personal", to: "codex/work")
    await model.use("codex/work")
    #expect(model.lastSwitches.map(\.to) == ["work", "codex/work"])
    #expect(model.lastSwitches.last?.restart?.from == "personal")
    #expect(model.advice.map(\.switchTo) == ["claude/personal"])

    // Each read after a rename fails here, so what is said is what the rename carried.
    stub.answer = .failure(
        PitboardError.Failed(
            code: "unreachable", cause: nil, message: "could not be reached", warnings: []))

    await model.rename("personal", of: "claude", to: "spare")
    let offered = try #require(model.advice.first)
    #expect(offered.use == "spare")
    #expect(offered.switchTo == "claude/spare")
    #expect(model.lastSwitches.last?.restart?.from == "personal", "Codex's is another account")

    await model.rename("personal", of: "codex", to: "home")
    #expect(model.lastSwitches.last?.restart?.from == "home")
    #expect(model.advice.first?.use == "spare", "Claude Code's is another account")

    await model.rename("work", of: "codex", to: "job")
    #expect(model.lastSwitches.map(\.to) == ["work", "codex/job"])
    #expect(model.advice.first?.ran == "work")

    await model.rename("work", of: "claude", to: "office")
    #expect(model.lastSwitches.map(\.to) == ["office", "codex/job"])
    #expect(model.advice.first?.ran == "office")
    let told = [Advice.key("claude", "office", spent): Int64(7_200)]
    #expect(model.toldForTesting == told)

    stub.answer = .success(
        status([
            account("office", signedIn: true, [spent]), account("spare", [room]),
            account("job", of: "codex", signedIn: true), account("home", of: "codex"),
        ]))
    await model.refresh()
    #expect(model.lastSwitches.map(\.to) == ["office", "codex/job"], "still in use")
    #expect(model.lastSwitches.last?.notice?.contains("keeps using home") == true)
    #expect(model.advice.map(\.switchTo) == ["claude/spare"])
    #expect(model.toldForTesting == told, "and not told again")
}

/// What has been told is kept per tool and account. A rename moves what was told about that
/// one account, every window of it, and nothing else: not the same name in another tool, and
/// not a name that only starts with it.
@MainActor
@Test func aRenameMovesWhatWasToldAboutThatAccountAlone() {
    let notifier = Notifier(delivering: false)
    for (provider, ran, kind, resets) in [
        ("claude", "work", "session", Int64(1)), ("claude", "work", "weekly_all", 2),
        ("codex", "work", "primary", 3), ("claude", "workshop", "session", 4),
    ] {
        notifier.tell(
            Advice(
                provider: provider, tool: nil, ran: ran,
                window: window(kind, 100, resets: resets), use: "spare", left: 50,
                switchTo: "\(provider)/spare"))
    }
    notifier.rename("work", of: "claude", to: "office")
    #expect(
        notifier.told == [
            "claude/office/session/": 1, "claude/office/weekly_all/": 2,
            "codex/work/primary/": 3, "claude/workshop/session/": 4,
        ])
}

// MARK: - Sign-ins and sheets

/// One sign-in at a time. A second asked for while one runs would take the place of the one
/// on screen and leave the first tool waiting on a browser with nothing to stop it.
@MainActor
@Test(.timeLimit(.minutes(1)))
func aSecondSignInWhileOneRunsStartsNothing() async {
    let stub = Stub(.success(status([])))
    let session = ScriptedSignIn(saying: ["https://auth.openai.com/oauth\n"], waits: true)
    stub.session = session
    let model = AppModel(testing: stub)
    let running = Task { await model.signIn("work", for: "codex") }
    #expect(await eventually { model.signingIn?.url != nil })
    let first = model.signingIn

    // Were a second one started, it would be refused here the way a missing program is,
    // rather than wait on a browser that never comes.
    stub.session = nil
    #expect(await model.signIn("home", for: "codex") == nil)
    #expect(stub.signedIn == ["codex/work"])
    #expect(model.signingIn === first)

    session.done.signal()
    #expect(await running.value == nil)
    #expect(model.signingIn == nil)
}

/// Cancel can be pressed before the tool has started. What then starts is stopped rather than
/// watched, off the main thread, and nothing is enrolled or said.
@MainActor
@Test(.timeLimit(.minutes(1)))
func aSignInCancelledWhileItStartsStopsWhatStarted() async {
    let stub = Stub(.success(status([])))
    let session = ScriptedSignIn(saying: ["https://auth.openai.com/oauth\n"])
    stub.session = session
    let gate = Gate()
    stub.duringSignIn = { await gate.pass() }
    let model = AppModel(testing: stub)
    let running = Task { await model.signIn("work", for: "codex") }
    #expect(await eventually { gate.arrivals == 1 })
    #expect(model.signingIn != nil)

    model.cancelSignIn()
    #expect(model.signingIn == nil)
    gate.open()
    #expect(await running.value == nil, "somebody asked for it to stop")
    #expect(await eventually { session.cancelledOnMain != nil })
    #expect(session.cancelledOnMain == false)
    #expect(!session.finished)
    #expect(model.signingIn == nil)
    #expect(model.warnings.isEmpty)
    #expect(model.lastSwitches.isEmpty)
}

/// A sign-in that finishes closes the sheet it was started from. A sheet up by then is
/// somebody else's, such as a name half typed, and closing it threw that away.
@MainActor
@Test(.timeLimit(.minutes(1)))
func aFinishedSignInClosesOnlyTheSheetItStartedFrom() async {
    let stub = Stub(.success(status([])))
    let session = ScriptedSignIn(saying: ["https://auth.openai.com/oauth\n"], waits: true)
    stub.session = session
    let model = AppModel(testing: stub)
    model.sheet = .add(provider: "codex")
    let running = Task { await model.signIn("work", for: "codex") }
    #expect(await eventually { model.signingIn?.url != nil })

    // `present` keeps a running sign-in's sheet, so this stands in for one put up some other
    // way.
    model.sheet = .rename(provider: "claude", label: "home")
    session.done.signal()
    #expect(await running.value == nil)
    #expect(session.finished)
    #expect(model.signingIn == nil)
    #expect(model.sheet == .rename(provider: "claude", label: "home"))
}

/// Cancel pressed while the tool is already being finished comes too late to stop it. What it
/// signed in to is enrolled all the same, so the accounts are read again, and nothing else is
/// touched: the sign-in and the sheet on screen by then are somebody else's, a sign-in started
/// since among them. Where the cancel reached the tool first and finishing fails as stopped,
/// that was somebody's answer and says nothing.
@MainActor
@Test(.timeLimit(.minutes(1)), arguments: [false, true])
func aSignInCancelledWhileItFinishesLeavesWhatCameAfterIt(stoppedFirst: Bool) async {
    let travel = account("travel", of: "codex")
    let stub = Stub(.success(status([account("work", of: "codex", signedIn: true)])))
    let late = ScriptedSignIn(saying: [])
    let finishing = DispatchSemaphore(value: 0)
    late.finishing = finishing
    if stoppedFirst {
        late.refusal = PitboardError.Failed(
            code: "sign_in_gone", cause: nil, message: "The sign-in is no longer running.",
            warnings: [
                Warning(code: "interrupted_switch_undone", message: "A switch was undone.")
            ])
    }
    stub.session = late
    let model = AppModel(testing: stub)
    model.sheet = .add(provider: "codex")
    let cancelled = Task { await model.signIn("travel", for: "codex") }
    #expect(await eventually { late.finished })
    model.cancelSignIn()

    let next = ScriptedSignIn(saying: ["https://claude.ai/oauth/authorize\n"], waits: true)
    stub.session = next
    model.sheet = .add(provider: "claude")
    let running = Task { await model.signIn("other", for: "claude") }
    #expect(await eventually { model.signingIn?.label == "other" })

    let reads = stub.readings
    stub.answer = .success(status([account("work", of: "codex", signedIn: true), travel]))
    finishing.signal()
    #expect(await cancelled.value == nil)
    #expect(model.signingIn?.label == "other")
    #expect(model.sheet == .add(provider: "claude"))
    #expect(model.warnings.isEmpty)
    if stoppedFirst {
        #expect(stub.readings == reads, "nothing was enrolled, so nothing is read")
    } else {
        #expect(model.status?.accounts.contains(travel) == true, "enrolled all the same")
    }

    next.done.signal()
    #expect(await running.value == nil)
}

/// Naming or renaming closes its own sheet once it is done, and only that. The name is saved
/// even when somebody has since put up another sheet, and closing that one threw away
/// whatever was in it.
@MainActor
@Test func namingAndRenamingCloseOnlyTheirOwnSheet() async {
    let stub = Stub(
        .success(status([account(nil, signedIn: true, uuid: "a"), account("personal")])))
    let model = AppModel(testing: stub)
    await model.refresh()

    let others: [AccountSheet] = [
        .add(provider: nil), .name(provider: "codex", email: "c@example.com"),
        .rename(provider: "claude", label: "other"),
    ]
    for other in others {
        model.sheet = other
        #expect(await model.enrol("work", for: "claude") == nil)
        #expect(model.sheet == other)
    }
    model.sheet = .name(provider: "claude", email: "a@example.com")
    await model.enrol("work", for: "claude")
    #expect(model.sheet == nil)

    for other in others + [.rename(provider: "codex", label: "personal")] {
        model.sheet = other
        #expect(await model.rename("personal", of: "claude", to: "home") == nil)
        #expect(model.sheet == other)
    }
    model.sheet = .rename(provider: "claude", label: "personal")
    await model.rename("personal", of: "claude", to: "home")
    #expect(model.sheet == nil)
}

// MARK: - An app that holds a tool's login

private let chatGPT = "com.openai.codex"

private let chatGPTHolding = Holding(
    kind: "chatgpt_app", phrase: "the ChatGPT app", pids: [4242, 4243],
    remedy: .reopenApp(bundleId: chatGPT, name: "ChatGPT"))

/// A Codex machine with ChatGPT open and running Codex's login, about to switch to `spare`.
@MainActor
private func chatGPTOpen(quits: Bool = true) -> (AppModel, Stub, StandInAppControl) {
    let stub = Stub(.success(Status(now: 0, accounts: [], warnings: [])))
    stub.held["codex"] = [chatGPTHolding]
    stub.switched = .success(
        Switched(
            outcome: .switched(
                provider: "codex", from: "codex/main", to: "codex/spare",
                adoption: .restart(program: "codex")),
            warnings: []))
    let apps = StandInAppControl(running: [chatGPT], quits: quits)
    let model = AppModel(testing: stub, appControl: apps)
    model.quitWithin = .milliseconds(50)
    return (model, stub, apps)
}

/// Switched under it, ChatGPT goes on with the account left behind, and its own Log Out
/// would revoke the login Pitboard has just parked. So the switch waits to be told.
@MainActor
@Test func aSwitchWaitsForTheAppHoldingTheLoginToBeQuit() async {
    let (model, stub, apps) = chatGPTOpen()
    await model.switchAsked(to: "codex/spare")
    #expect(model.quitting?.name == "ChatGPT")
    #expect(model.quitting?.bundleID == chatGPT)
    #expect(stub.switchedTo.isEmpty)
    #expect(apps.asked.isEmpty, "nothing is quit before the person says so")
    #expect(model.requestedPane == .accounts, "asked in the window")
}

@MainActor
@Test func quittingTheAppSwitchesAndOpensItAgain() async throws {
    let (model, stub, apps) = chatGPTOpen()
    await model.switchAsked(to: "codex/spare")
    await model.quitAndSwitch(try #require(model.quitting))
    #expect(apps.asked == ["quit \(chatGPT)", "open \(chatGPT)"])
    #expect(stub.switchedTo == ["codex/spare"])
    #expect(model.quitting == nil)
    #expect(model.switching == nil)
    #expect(model.presentedFailure == nil)
}

/// Pitboard closed it, so Pitboard opens it, whether or not the switch worked.
@MainActor
@Test func theAppIsOpenedAgainWhenTheSwitchFails() async throws {
    let (model, stub, apps) = chatGPTOpen()
    stub.switched = .failure(
        PitboardError.Failed(
            code: "parked_login_expired", cause: nil, message: "spare's login has expired.",
            warnings: []))
    await model.switchAsked(to: "codex/spare")
    await model.quitAndSwitch(try #require(model.quitting))
    #expect(apps.asked == ["quit \(chatGPT)", "open \(chatGPT)"])
    #expect(model.presentedFailure?.message == "spare's login has expired.")
}

/// An app busy with work, or whose person said no, stays open, and nothing changes.
@MainActor
@Test func anAppThatDoesNotQuitStopsTheSwitchBeforeAnythingChanges() async throws {
    let (model, stub, apps) = chatGPTOpen(quits: false)
    await model.switchAsked(to: "codex/spare")
    await model.quitAndSwitch(try #require(model.quitting))
    #expect(apps.asked == ["quit \(chatGPT)"], "and it is not opened: it never closed")
    #expect(stub.switchedTo.isEmpty)
    #expect(model.switching == nil)
    let failure = try #require(model.presentedFailure)
    #expect(failure.title == "Couldn’t switch to spare")
    #expect(failure.message.contains("ChatGPT is still open, so nothing has changed"))
}

@MainActor
@Test func keepingTheAppOpenSwitchesNothing() async {
    let (model, stub, apps) = chatGPTOpen()
    await model.switchAsked(to: "codex/spare")
    model.closeQuitQuestion()
    #expect(model.quitting == nil)
    #expect(stub.switchedTo.isEmpty)
    #expect(apps.asked.isEmpty)
}

/// What is left of an app that has gone cannot be quit, and a tool nothing holds is
/// switched at once.
@MainActor
@Test func onlyARunningAppHoldingTheToolIsAskedAbout() async {
    let (model, stub, apps) = chatGPTOpen()
    apps.running = []
    await model.switchAsked(to: "codex/spare")
    #expect(model.quitting == nil)
    #expect(stub.switchedTo == ["codex/spare"])

    apps.running = [chatGPT]
    await model.switchAsked(to: "claude/personal")
    #expect(model.quitting == nil, "ChatGPT holds Codex's login, not Claude Code's")
    #expect(stub.switchedTo == ["codex/spare", "claude/personal"])
}

@MainActor
@Test func anAppIsGivenItsTimeToQuitAndNoMore() async {
    let apps = StandInAppControl(running: [chatGPT])
    #expect(
        await apps.quit(chatGPT, within: .milliseconds(50))
            == .quit(StandInAppControl.copy(of: chatGPT)))
    #expect(await apps.quit(chatGPT, within: .milliseconds(50)) == .notRunning)
    #expect(apps.asked == ["quit \(chatGPT)"], "one not running is not asked")

    let busy = StandInAppControl(running: [chatGPT], quits: false)
    let clock = ContinuousClock()
    let started = clock.now
    #expect(await busy.quit(chatGPT, within: .milliseconds(100)) == .stillRunning)
    #expect(clock.now - started >= .milliseconds(100))
    #expect(busy.running(chatGPT) != nil)
}

/// Somebody who quit ChatGPT themselves before answering did not ask for it back: Pitboard
/// opens only what it closed.
@MainActor
@Test func anAppAlreadyGoneIsNotOpenedAgain() async throws {
    let (model, stub, apps) = chatGPTOpen()
    await model.switchAsked(to: "codex/spare")
    apps.running = []
    await model.quitAndSwitch(try #require(model.quitting))
    #expect(apps.asked.isEmpty)
    #expect(stub.switchedTo == ["codex/spare"])
}

/// A switch is claimed before the core is asked what holds the login, so a second one asked
/// for meanwhile waits. While the question about quitting waits, another switch brings it
/// back to the front instead of being dropped unseen, and every account holds back.
@MainActor
@Test func oneSwitchAtATimeWhileTheAppQuestionWaits() async {
    let (model, stub, _) = chatGPTOpen()
    let gate = AsyncGate()
    stub.beforeHolding = { await gate.wait() }
    let first = Task { await model.switchAsked(to: "codex/spare") }
    while model.switching == nil { await Task.yield() }
    await model.switchAsked(to: "claude/personal")
    #expect(stub.switchedTo.isEmpty, "the second waited for the first")
    gate.open()
    await first.value

    #expect(model.switchUnderWay == "codex/spare")
    model.showWindow(.machine)
    await model.switchAsked(to: "claude/personal")
    #expect(stub.switchedTo.isEmpty)
    #expect(model.requestedPane == .accounts, "the question came back to the front")
}

/// Opens once, for a test to hold something back until it says so.
@MainActor
private final class AsyncGate {
    private var opened = false
    private var waiting: [CheckedContinuation<Void, Never>] = []

    func wait() async {
        if opened { return }
        await withCheckedContinuation { waiting.append($0) }
    }

    func open() {
        opened = true
        for continuation in waiting { continuation.resume() }
        waiting = []
    }
}
