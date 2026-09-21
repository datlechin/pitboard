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

    func status() async throws -> Status { try answer.get() }
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
        stale: nil, staleExplanation: nil)
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
                    code: "state_wrong_machine", message: "was written on another computer",
                    warnings: []))))
    await model.refresh()
    #expect(model.problem == "was written on another computer")
    #expect(model.status == nil)
}

/// A switch that failed must not read as one that worked.
@MainActor
@Test func aFailedSwitchSaysSoAndChangesNothing() async {
    let stub = Stub(.success(Status(now: 0, accounts: [], warnings: [])))
    stub.switched = .failure(
        PitboardError.Failed(code: "nothing_parked", message: "nothing parked", warnings: []))
    let model = AppModel(service: stub)
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
    let model = AppModel(service: stub)
    await model.use("work")
    let left = model.adopted?.timeIntervalSinceNow ?? 0
    #expect(left > 30 && left <= 33)
}

@MainActor
@Test func doctorIsOnlyReadWhenAskedFor() async {
    let model = AppModel(service: Stub(.success(Status(now: 0, accounts: [], warnings: []))))
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
                        staleExplanation: nil)
                ], warnings: [])))
    let model = AppModel(service: stub)
    await model.refresh()
    #expect(model.unenrolled)

    model.naming = "work"
    await model.enrol(as: "work")
    #expect(stub.enrolled == ["work"])
    #expect(model.naming == nil, "the form closes once it has been used")
}

/// Forgetting is destructive, so what the model does with a refusal matters.
@MainActor
@Test func aRefusedForgetIsReported() async {
    let stub = Stub(.success(Status(now: 0, accounts: [], warnings: [])))
    let model = AppModel(service: stub)
    await model.forget("alpha")
    #expect(stub.forgot == ["alpha"])
    #expect(model.problem == nil)
}
