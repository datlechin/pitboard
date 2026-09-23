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
    var enrolling: Result<Enrolled, Error> = .success(
        Enrolled(email: "a@b.c", enrolled: .current, warnings: []))

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
    func schedule() async -> Schedule { .absent }
    func scheduleInstall() async throws -> String { "/nowhere" }
    func scheduleUninstall() async throws -> Bool { false }
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
                warnings: [])
        }
        return session
    }
    func tools() -> [Tool] { bothTools }
    func installed() -> [Tool] { found }
}

/// A sign-in that says what it is given to say and then enrols, without a tool behind it.
private final class ScriptedSignIn: SignIn, @unchecked Sendable {
    private var lines: [String]
    private let code: Bool
    private(set) var pasted: [String] = []

    init(saying lines: [String], takesACode code: Bool) {
        self.lines = lines
        self.code = code
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) { fatalError("not from the core") }

    override func takesACode() -> Bool { code }
    override func nextLine() -> String? { lines.isEmpty ? nil : lines.removeFirst() }
    override func paste(line: String) throws { pasted.append(line) }
    override func finish() throws -> Enrolled {
        Enrolled(email: "a@b.c", enrolled: .signedIn, warnings: [])
    }
    override func cancel() {}
}

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
    #expect(model.adopted == nil)
}

/// After a switch the panel counts down to when open sessions follow.
@MainActor
@Test func aSwitchRecordsWhenOpenSessionsFollow() async {
    let stub = Stub(.success(Status(now: 0, accounts: [], warnings: [])))
    stub.switched = .success(
        Switched(
            outcome: .switched(
                provider: "claude", from: "personal", to: "work",
                adoption: .follows(withinSeconds: 33)),
            warnings: []))
    let model = AppModel(watching: false, service: stub)
    await model.use("claude/work")
    let left = model.adopted?.timeIntervalSinceNow ?? 0
    #expect(left > 30 && left <= 33)
    #expect(model.restart == nil)
}

/// A running `codex` never picks a switch up, so a countdown there would promise something
/// that does not happen. The panel says what does instead, and keeps the core's own warning
/// about the sessions it counted, which the read after the switch would otherwise replace.
@MainActor
@Test func aSwitchThatNeedsARestartSaysSoAndCountsNothingDown() async {
    let stub = Stub(.success(status([])))
    let running = Warning(
        code: "sessions_still_running",
        message: "2 `codex` sessions started before this switch are still running")
    stub.switched = .success(
        Switched(
            outcome: .switched(
                provider: "codex", from: "personal", to: "work",
                adoption: .restart(program: "codex")),
            warnings: [running]))
    let model = AppModel(watching: false, service: stub)

    await model.use("codex/work")

    #expect(model.adopted == nil, "no countdown")
    #expect(
        model.restart == AppModel.Restart(provider: "codex", program: "codex", from: "personal")
    )
    #expect(
        restartNotice(program: "codex", from: "personal")
            == "Running codex sessions keep using personal until they are quit and started again."
    )
    #expect(model.afterSwitch == [running], "the switch's warning outlives the read after it")

    model.forgetSwitch()
    #expect(model.restart == nil)
    #expect(model.afterSwitch.isEmpty)
}

/// A warning the read after a switch also carries is shown once, not twice.
@MainActor
@Test func aSwitchWarningTheReadRepeatsIsSaidOnce() async {
    let overridden = Warning(code: "auth_overridden", message: "ANTHROPIC_API_KEY is set")
    let stub = Stub(.success(status([], warnings: [overridden])))
    stub.switched = .success(
        Switched(
            outcome: .switched(
                provider: "claude", from: "a", to: "b", adoption: .follows(withinSeconds: 5)),
            warnings: [overridden]))
    let model = AppModel(watching: false, service: stub)
    await model.use("claude/b")
    #expect(model.warnings == [overridden])
    #expect(model.afterSwitch.isEmpty)
}

/// Each switch says what it means and nothing about the one before: a countdown from a
/// Claude Code switch next to a Codex restart notice would read as contradicting it.
@MainActor
@Test func aSwitchReplacesWhatTheLastOneSaid() async {
    let stub = Stub(.success(status([])))
    stub.switched = .success(
        Switched(
            outcome: .switched(
                provider: "codex", from: "a", to: "b", adoption: .restart(program: "codex")),
            warnings: [Warning(code: "sessions_still_running", message: "1 still running")]))
    let model = AppModel(watching: false, service: stub)
    await model.use("codex/b")
    stub.switched = .success(
        Switched(
            outcome: .switched(
                provider: "claude", from: "a", to: "b", adoption: .follows(withinSeconds: 33)),
            warnings: []))
    await model.use("claude/b")
    #expect(model.restart == nil)
    #expect(model.afterSwitch.isEmpty)
    #expect(model.adopted != nil)
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
/// the core is given and no code is ever asked for, whatever the tool prints.
@MainActor
@Test func aCodexSignInIsForCodexAndTakesNoCode() async {
    let stub = Stub(.success(status([])))
    stub.session = ScriptedSignIn(
        saying: ["Paste code here if prompted\n"], takesACode: false)
    let model = AppModel(watching: false, service: stub)
    await model.signIn("work", for: "codex")
    #expect(stub.signedIn == ["codex/work"])
    #expect(model.signingIn == nil, "finished and enrolled")
    #expect(model.problem == nil)
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
    #expect(!codexSaid.wantsCode)

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

/// An app from the cask and nothing else. Before the first read there is nothing true to
/// say, and a setup step shown to somebody who finished it years ago is worse than silence.
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

/// Only tools the app found a program for are offered for a new account, and every tool is
/// when it found none: a program elsewhere on the PATH is still a program.
@MainActor
@Test func aNewAccountIsOfferedForToolsThatAreHere() async {
    let stub = Stub(.success(status([])))
    let model = AppModel(watching: false, service: stub)
    await model.refresh()
    #expect(model.addable == [claudeCode])

    stub.found = []
    #expect(model.addable == bothTools)

    // A tool with an account here is here, wherever its program is.
    stub.answer = .success(status([account("work", of: "codex", signedIn: true)]))
    stub.found = [claudeCode]
    await model.refresh()
    #expect(model.addable == bothTools)
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
