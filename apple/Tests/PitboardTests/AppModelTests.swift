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
        throw PitboardError.Failed(
            code: "claude_program_missing", cause: nil,
            message: "`claude` is not on this machine",
            warnings: [])
    }
}

private func account(_ label: String, signedIn: Bool, percent: Double) -> Account {
    Account(
        label: label, email: "\(label)@example.com", accountUuid: label, signedIn: signedIn,
        switchable: !signedIn, parked: nil,
        usage: Usage(
            source: .live, observedAt: 0,
            windows: [
                Limits(
                    kind: "session", scope: nil, percent: percent, resetsAt: nil, severity: nil,
                    isActive: true)
            ]),
        stale: nil, staleExplanation: nil, lastsSeconds: nil, lastsBurning: false)
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
            outcome: .switched(from: "personal", to: "work", adoptionCeilingSeconds: 33),
            warnings: []))
    let model = AppModel(watching: false, service: stub)
    await model.use("work")
    let left = model.adopted?.timeIntervalSinceNow ?? 0
    #expect(left > 30 && left <= 33)
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
                accounts: [
                    Account(
                        label: nil, email: "a@b.c", accountUuid: "a", signedIn: true,
                        switchable: false, parked: nil, usage: nil, stale: nil,
                        staleExplanation: nil, lastsSeconds: nil, lastsBurning: false)
                ], warnings: [])))
    let model = AppModel(watching: false, service: stub)
    await model.refresh()
    #expect(model.unenrolled)

    model.naming = .theOneInUse
    await model.enrol(as: "work")
    #expect(stub.enrolled == ["work"])
    #expect(model.naming == nil, "the form closes once it has been used")
}

/// Forgetting is destructive, so what the model does with a refusal matters.
@MainActor
@Test func aRefusedForgetIsReported() async {
    let stub = Stub(.success(Status(now: 0, accounts: [], warnings: [])))
    let model = AppModel(watching: false, service: stub)
    await model.forget("alpha")
    #expect(stub.forgot == ["alpha"])
    #expect(model.problem == nil)
}

/// A sign-in that cannot start says why, and leaves nothing half-shown in the panel.
@MainActor
@Test func aSignInThatCannotStartIsReported() async {
    let model = AppModel(
        watching: false, service: Stub(.success(Status(now: 0, accounts: [], warnings: []))))
    model.naming = .another
    await model.signIn(as: "work")
    #expect(model.signingIn == nil)
    #expect(model.naming == nil)
    #expect(model.problem == "`claude` is not on this machine")
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
