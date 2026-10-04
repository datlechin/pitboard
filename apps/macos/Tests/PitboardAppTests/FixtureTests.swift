import Foundation
import PitboardKit
import Testing

@testable import PitboardApp

/// Each account the way a test reads it: its name with its tool, or the email of a login
/// with no name, then whether it is the one in use, or why it cannot be switched to.
private func described(_ status: Status) -> [String] {
    status.accounts.map { account in
        let name = account.qualified ?? account.email
        if account.signedIn { return "\(name), in use" }
        return account.switchable ? name : "\(name), \(account.stale ?? "not switchable")"
    }
}

/// The code a call was refused with, or nil when it was not refused.
private func refusal<T>(_ call: () async throws -> T) async -> String? {
    do {
        _ = try await call()
        return nil
    } catch {
        return AppModel.code(of: error)
    }
}

/// Runs `work`, which blocks, on a thread of its own, and waits for it without holding one of
/// the threads Swift's tasks share. A sign-in's read waits until something else wakes it, and
/// a few such waits on the shared threads can leave none for whatever would wake them.
private func onAThreadOfItsOwn<T: Sendable>(_ work: @escaping @Sendable () -> T) async -> T {
    await withCheckedContinuation { done in
        Thread.detachNewThread { done.resume(returning: work()) }
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

/// Runs a fixture's sign-in to the end the way the sheet does: reads everything the tool
/// says, types a code back once it asks for one, and finishes.
private func signInToTheEnd(
    _ label: String, on core: FixtureCore
) async throws -> Enrolled {
    let session = try await core.signIn(label)
    await onAThreadOfItsOwn {
        while let line = session.nextLine() {
            if line.contains("Paste code") { try? session.paste(line: "fixture-code") }
        }
    }
    return try session.finish()
}

// MARK: - Where each fixture starts

/// The UI tests launch the app into these fixtures and assert on what each starts with, so
/// a change here is a change to what they test: who a read shows, or the code it fails
/// with, who the offline read shows, and which tools were found. A read that fails still
/// has the offline read to fall back on.
@Test(arguments: Fixture.allCases)
func eachFixtureStartsWhereItsTestsExpect(_ fixture: Fixture) async throws {
    let work = "claude/work, in use"
    let expected: (failure: String?, shown: [String]) =
        switch fixture {
        case .twoTools, .chatGPTOpen:
            (
                nil,
                [
                    work, "claude/personal", "claude/old, parked_access_expired",
                    "codex/main, in use", "codex/spare",
                ]
            )
        case .oneTool: (nil, [work, "claude/personal"])
        case .claudeDesktop:
            (nil, [work, "claude/personal", "desktop/personal, in use", "desktop/work"])
        case .onlyOne: (nil, [work])
        case .unnamed: (nil, ["dana@work.example, in use"])
        case .empty, .firstLaunch: (nil, [])
        case .noClaudeCode: (nil, [])
        case .readFailure: ("unreachable", [work, "claude/personal"])
        case .stuck: ("recovery_undetermined", [work, "claude/personal"])
        }
    let core = FixtureCore(fixture)
    let offline = try await core.statusOffline()
    #expect(described(offline) == expected.shown)

    do {
        let read = try await core.status(fresh: true)
        #expect(expected.failure == nil)
        #expect(read.accounts == offline.accounts)
    } catch {
        #expect(AppModel.code(of: error) == expected.failure)
    }
    let desktop = fixture == .claudeDesktop
    #expect(core.tools() == bothTools + (desktop ? [claudeDesktop] : []))
    #expect(
        await core.installed().map(\.code)
            == (fixture == .noClaudeCode
                ? [] : ["claude", "codex"] + (desktop ? ["desktop"] : [])))
}

/// The UI tests launch a fixture by setting this variable to one of these names, from a
/// list of their own, so a name changed here has to change there.
@Test func theUITestsNameEveryFixtureAsTheAppDoes() {
    #expect(Fixture.variable == "PITBOARD_FIXTURE")
    #expect(
        Fixture.allCases.map(\.rawValue) == [
            "twoTools", "oneTool", "empty", "firstLaunch", "noClaudeCode", "unnamed",
            "onlyOne", "readFailure", "stuck", "chatGPTOpen", "claudeDesktop",
        ])
}

// MARK: - Changing accounts

/// The accounts in use, one per tool at most, by their names with their tools.
private func inUse(_ status: Status) -> [String] {
    status.accounts.filter(\.signedIn).compactMap(\.qualified)
}

/// A switch moves who is in use within the tool it is for and leaves the other tool alone,
/// and the account it left can be switched back to. It says what running sessions do:
/// Claude Code's follow within a minute, and Codex's keep the old account until they are
/// started again. The account already in use is not switched to again, and an account
/// nobody enrolled is refused.
@Test func aSwitchMovesWhoIsInUseWithinItsOwnTool() async throws {
    let core = FixtureCore(.twoTools)

    let claude = try await core.switchTo("claude/personal")
    if case .switched(let provider, _, _, let adoption) = claude.outcome {
        #expect(provider == "claude")
        #expect(adoption == .follows(withinSeconds: 45))
    } else {
        Issue.record("\(claude.outcome) is not a switch")
    }
    let afterClaude = try await core.status(fresh: true)
    #expect(inUse(afterClaude) == ["claude/personal", "codex/main"])
    #expect(afterClaude.accounts.first { $0.qualified == "claude/work" }?.switchable == true)

    let codex = try await core.switchTo("codex/spare")
    #expect(
        codex.outcome
            == .switched(
                provider: "codex", from: "codex/main", to: "codex/spare",
                adoption: .restart(program: "codex")))
    let afterCodex = try await core.status(fresh: true)
    #expect(inUse(afterCodex) == ["claude/personal", "codex/spare"])
    #expect(afterCodex.accounts.first { $0.qualified == "codex/main" }?.switchable == true)

    #expect(
        try await core.switchTo("claude/personal").outcome == .alreadyActive(label: "personal"))
    #expect(await refusal { try await core.switchTo("claude/nobody") } != nil)
    #expect(inUse(try await core.status(fresh: true)) == ["claude/personal", "codex/spare"])
}

/// A parked login that expired stays unusable until somebody signs in to it again. A switch
/// to it is refused and moves nothing, and a switch between two other accounts of its tool
/// leaves it as it was, still offering a sign-in rather than a switch.
@Test
func anExpiredParkedLoginStaysUnusableAcrossSwitches() async throws {
    let core = FixtureCore(.twoTools)
    #expect(await refusal { try await core.switchTo("claude/old") } != nil)
    #expect(inUse(try await core.status(fresh: true)) == ["claude/work", "codex/main"])

    _ = try await core.switchTo("claude/personal")
    #expect(
        described(try await core.status(fresh: true)) == [
            "claude/work", "claude/personal, in use", "claude/old, parked_access_expired",
            "codex/main, in use", "codex/spare",
        ])
}

/// The core names the accounts of a switch as the command line types them: bare for Claude
/// Code, with the tool for any other. The model matches what a switch said against that, so
/// a Claude Code switch named with its tool reads as undone by the read after it, and what
/// it said about running sessions is put away before anyone has seen it.
@MainActor
@Test
func aSwitchNamesTheAccountsAsTheCoreTypesThem() async throws {
    let core = FixtureCore(.twoTools)
    #expect(
        try await core.switchTo("codex/main").outcome == .alreadyActive(label: "codex/main"))
    #expect(
        try await core.switchTo("claude/personal").outcome
            == .switched(
                provider: "claude", from: "work", to: "personal",
                adoption: .follows(withinSeconds: 45)))

    let model = AppModel(testing: FixtureCore(.twoTools))
    await model.refresh()
    await model.use("claude/personal")
    #expect(model.lastSwitches.map(\.provider) == ["claude"])
}

/// The login signed in with no name is named in place, and stays the account in use. A
/// name its tool already has is refused and leaves it unnamed, and once it has a name there
/// is nobody left to name.
@Test(.timeLimit(.minutes(1)))
func theLoginSignedInNowIsNamedWithANameNotTaken() async throws {
    let core = FixtureCore(.unnamed)
    _ = try await signInToTheEnd("claude/personal", on: core)
    #expect(await refusal { try await core.enrollCurrent("personal") } == "label_taken")
    #expect(
        described(try await core.status(fresh: true)) == [
            "dana@work.example, in use", "claude/personal",
        ])

    #expect(
        try await core.enrollCurrent("work")
            == Enrolled(email: "dana@work.example", outcome: .current, warnings: []))
    #expect(
        described(try await core.status(fresh: true)) == [
            "claude/work, in use", "claude/personal",
        ])
    #expect(await refusal { try await core.enrollCurrent("home") } != nil)
}

/// The account in use cannot be forgotten, since its login is the one the tool is using.
/// Any other can, and goes from the list.
@Test func onlyAnAccountNotInUseIsForgotten() async throws {
    let core = FixtureCore(.twoTools)
    #expect(
        await refusal { try await core.forget("claude/work") }
            == "cannot_forget_active_account")
    #expect(await refusal { try await core.forget("claude/nobody") } != nil)

    #expect(
        try await core.forget("codex/spare")
            == Changed(email: "dana@home.example", warnings: []))
    #expect(
        described(try await core.status(fresh: true)) == [
            "claude/work, in use", "claude/personal", "claude/old, parked_access_expired",
            "codex/main, in use",
        ])
}

/// A rename needs a name its tool has not given another account. Another tool's names do
/// not count, since a name is only ever used with its tool.
@Test func aRenameNeedsANameItsToolHasNotGiven() async throws {
    let core = FixtureCore(.twoTools)
    #expect(
        await refusal { try await core.rename("claude/personal", to: "old") } == "label_taken")

    #expect(
        try await core.rename("claude/personal", to: "main")
            == Changed(email: "dana@home.example", warnings: []))
    #expect(
        described(try await core.status(fresh: true)) == [
            "claude/work, in use", "claude/main", "claude/old, parked_access_expired",
            "codex/main, in use", "codex/spare",
        ])
}

// MARK: - Signing in

/// Claude Code's sign-in prints the address to open and asks for the code from the browser,
/// which the sheet shows as a link and a field, and goes no further until a code is typed
/// back. Finishing then parks the new account beside the one in use.
@MainActor
@Test(.timeLimit(.minutes(1)))
func aClaudeCodeSignInWaitsForTheCodeAndThenParksTheAccount() async throws {
    let core = FixtureCore(.oneTool)
    let session = try await core.signIn("claude/travel")
    let said = await onAThreadOfItsOwn {
        var said = ""
        while let line = session.nextLine() {
            said += line
            if line.contains("Paste code") { break }
        }
        return said
    }
    let shown = SigningIn(label: "travel", provider: "claude", tool: "Claude Code")
    shown.add(said)
    #expect(shown.url == URL(string: "https://claude.ai/oauth/authorize?fixture=1"))
    #expect(shown.wantsCode)

    let clock = ContinuousClock()
    let asked = clock.now
    let pasting = Task.detached {
        try await Task.sleep(for: .milliseconds(300))
        try session.paste(line: "fixture-code")
    }
    #expect(await onAThreadOfItsOwn { session.nextLine() } == nil)
    #expect(clock.now - asked >= .milliseconds(300), "nothing more until the code is typed")
    try await pasting.value

    #expect(
        try session.finish()
            == Enrolled(email: "travel@example.com", outcome: .signedIn, warnings: []))
    #expect(
        described(try await core.status(fresh: true)) == [
            "claude/work, in use", "claude/personal", "claude/travel",
        ])
}

/// Codex's sign-in prints the address to open beside the loopback address the browser
/// comes back to, reads nothing typed, and finishes by itself once the browser is done.
@MainActor
@Test(.timeLimit(.minutes(1)))
func aCodexSignInFinishesByItself() async throws {
    let core = FixtureCore(.twoTools)
    let session = try await core.signIn("codex/travel")
    let said = await onAThreadOfItsOwn {
        var said = ""
        while let line = session.nextLine() { said += line }
        return said
    }
    let shown = SigningIn(label: "travel", provider: "codex", tool: "Codex")
    shown.add(said)
    #expect(shown.url == URL(string: "https://auth.openai.com/oauth/authorize?fixture=1"))
    #expect(!shown.wantsCode)

    #expect(
        try session.finish()
            == Enrolled(email: "travel@example.com", outcome: .signedIn, warnings: []))
    #expect(
        described(try await core.status(fresh: true)).suffix(3) == [
            "codex/main, in use", "codex/spare", "codex/travel",
        ])
}

/// A sign-in stopped part way enrols nothing: it stops waiting for a code, and finishing it
/// fails as stopped, the way the tool's own does.
@Test(.timeLimit(.minutes(1)))
func aStoppedSignInEnrolsNothing() async throws {
    let core = FixtureCore(.oneTool)
    let session = try await core.signIn("claude/travel")
    let reading = Task { await onAThreadOfItsOwn { while session.nextLine() != nil {} } }
    session.cancel()
    await reading.value

    #expect(await refusal { try session.finish() } != nil)
    #expect(
        described(try await core.status(fresh: true)) == [
            "claude/work, in use", "claude/personal",
        ])
}

/// Signing in again to an account whose parked login expired renews that account rather
/// than adding a second one, and it can be switched to again.
@Test(.timeLimit(.minutes(1)))
func signingInAgainToAnExpiredAccountMakesItSwitchable() async throws {
    let core = FixtureCore(.twoTools)
    #expect(
        try await signInToTheEnd("claude/old", on: core)
            == Enrolled(email: "dana@old.example", outcome: .renewed, warnings: []))
    #expect(
        described(try await core.status(fresh: true)) == [
            "claude/work, in use", "claude/personal", "claude/old", "codex/main, in use",
            "codex/spare",
        ])
}

/// Once its parked login is renewed nothing is wrong with the account any more, so a read
/// no longer says the login expired.
@Test(.timeLimit(.minutes(1)))
func signingInAgainPutsAwayWhatWasWrongWithTheParkedLogin() async throws {
    let core = FixtureCore(.twoTools)
    _ = try await signInToTheEnd("claude/old", on: core)
    let old = try #require(
        try await core.status(fresh: true).accounts.first { $0.qualified == "claude/old" })
    #expect(old.stale == nil)
    #expect(old.staleExplanation == nil)
}

/// Signing in again to the account in use puts its new login in use at once, as the core
/// does, and parks nothing: it stays the account in use and is not one to switch to. It says
/// so, rather than that a parked login was renewed, which is what has the app say what the
/// new login means for sessions already running.
@Test(.timeLimit(.minutes(1)))
func signingInAgainToTheAccountInUseKeepsItInUse() async throws {
    let core = FixtureCore(.oneTool)
    #expect(
        try await signInToTheEnd("claude/work", on: core)
            == Enrolled(
                email: "dana@work.example", outcome: .inUse(again: true), warnings: []))
    let read = try await core.status(fresh: true)
    #expect(described(read) == ["claude/work, in use", "claude/personal"])
    let work = try #require(read.accounts.first { $0.qualified == "claude/work" })
    #expect(!work.switchable)
}

// MARK: - The rest of the machine

/// An interrupted switch is given up on once. The read that failed on it then works, and
/// asking again has nothing to give up on.
@Test func givingUpOnTheInterruptedSwitchHappensOnce() async throws {
    let core = FixtureCore(.stuck)
    #expect(await refusal { try await core.status(fresh: true) } == "recovery_undetermined")

    #expect(
        try await core.abandonRecovery()
            == Abandoned(from: "work", to: "personal", loginsKept: 2))
    #expect(try await core.abandonRecovery() == nil)
    #expect(
        described(try await core.status(fresh: true)) == [
            "claude/work, in use", "claude/personal",
        ])
}

/// The log keeps what changed newest last, and names Claude Code's accounts bare and any
/// other tool's with the tool, as the command line types them. Each change moves when the
/// account index last changed, which is how an app watching would notice.
@Test func theLogRecordsChangesNewestLastAsTheyAreTyped() async throws {
    let core = FixtureCore(.twoTools)
    let history = await core.log(limit: 500)
    #expect(
        history.map { "\($0.verb) \($0.subject)" } == [
            "enroll work", "enroll personal", "switch personal", "switch work",
        ])
    let changedAt = await core.changedAt()

    _ = try await core.switchTo("claude/personal")
    _ = try await core.switchTo("codex/spare")
    _ = try await core.rename("claude/work", to: "office")
    let log = await core.log(limit: 500)
    #expect(
        log.dropFirst(history.count).map { "\($0.verb) \($0.subject)" } == [
            "switch personal", "switch codex/spare", "rename work -> office",
        ])
    #expect(log.suffix(3).allSatisfy { $0.caller == "app" && $0.outcome == "ok" })
    let dates = log.compactMap { changeDate($0.at) }
    #expect(dates.count == log.count)
    #expect(dates == dates.sorted())
    #expect(await core.log(limit: 2) == Array(log.suffix(2)))
    #expect(await core.changedAt() > changedAt)
}

/// Daily renewal can be turned on and off, and taking away a schedule that is not there
/// says there was nothing to take away. Nothing is ever repaired.
@Test func theScheduleTurnsOnAndOff() async throws {
    let core = FixtureCore(.oneTool)
    #expect(await core.schedule() == .absent)
    #expect(try await core.scheduleUninstall() == false)

    let path = try await core.scheduleInstall()
    #expect(await core.schedule() == .installed(path: path, everySeconds: 86_400))
    #expect(try await core.scheduleUninstall())
    #expect(await core.schedule() == .absent)
    #expect(try await core.scheduleRepair() == false)
}

// MARK: - Launching into a fixture

/// Leaves nothing of a fixture launch behind: the stand-in app and link it made in a
/// temporary directory. Its preferences are the test's own, kept in memory.
private func forgetLaunches() {
    try? FileManager.default.removeItem(
        at: FileManager.default.temporaryDirectory
            .appendingPathComponent("pitboard-fixture"))
}

/// A launch into a fixture empties and makes again one temporary directory, so the tests
/// that launch one take turns.
@MainActor
@Suite(.serialized)
struct FixtureLaunchTests {
    /// A UI test launches the app into a fixture many times, and each launch starts where
    /// the last one did: seen before unless it is the first launch, reading, noticing
    /// changes and reading when a menu opens as the app does on a real machine, and asking
    /// the person running the tests for nothing, a notification's permission included.
    ///
    /// Each launch here is handed preferences kept in memory. That a real launch empties the
    /// fixture's own suite is not checked: the suite is a file in the preferences of whoever
    /// runs the tests, and emptying it still leaves the file there.
    @Test func everyLaunchStartsWhereTheLastOneDid() async throws {
        defer { forgetLaunches() }
        #expect(Fixture.suite == "com.usepitboard.Pitboard.fixture", "never the app's own")
        for fixture in Fixture.allCases {
            let defaults = TestDefaults()
            let launch = fixture.dependencies(defaults: defaults)
            #expect(launch.defaults === defaults)
            #expect(
                launch.defaults.bool(forKey: DefaultsKey.hasBeenSeen)
                    == (fixture != .firstLaunch))
            #expect(!launch.notifies)
            #expect(launch.watching)
            #expect(launch.loginItem is FixtureLoginItem)
            #expect(launch.loginItem.state == .disabled)

            let core = try #require(launch.core as? FixtureCore)
            #expect(
                described(try await core.statusOffline())
                    == described(try await FixtureCore(fixture).statusOffline()))
        }
    }

    /// The UI tests open the menu before the window, and the menu shows only what a read has
    /// found. The app launched into a fixture reads by itself, as it does on a real machine,
    /// with nothing opened and nothing pressed.
    @Test(.timeLimit(.minutes(1)))
    func aLaunchReadsWithoutBeingAsked() async throws {
        defer { forgetLaunches() }
        let model = AppModel(
            dependencies: Fixture.twoTools.dependencies(defaults: TestDefaults()))
        #expect(await eventually { model.status != nil })
        #expect(
            described(try #require(model.status))
                == described(try await FixtureCore(.twoTools).statusOffline()))
    }

    /// A fixture's command line is inside a stand-in app in a temporary directory, and
    /// linking it makes the link there without a password prompt, so a UI test can press the
    /// settings' button without writing to `/usr/local/bin`.
    @Test func aLaunchLinksItsCommandLineInATemporaryDirectory() async throws {
        defer { forgetLaunches() }
        let tool = Fixture.oneTool.dependencies(defaults: TestDefaults()).commandLineTool
        let temporary = FileManager.default.temporaryDirectory.path
        #expect(tool.linkable)
        #expect(tool.helper?.hasPrefix(temporary) == true)
        #expect(tool.link.hasPrefix(temporary))
        #expect(tool.find(onPath: nil) == .nowhere)

        #expect(await tool.install() == .linked)
        #expect(tool.find(onPath: nil) == .bundled(tool.link))
    }
}

// MARK: - An app that holds Codex's login

/// The fixture's ChatGPT runs Codex's login while it is open, as the process list shows it,
/// and its app control quits and opens it: a switch through the fixture quits it, switches
/// and opens it again, which is what the UI test drives.
@MainActor
@Test func theChatGPTFixtureIsQuitForACodexSwitchAndOpenedAgain() async throws {
    let apps = FixtureApps(running: [FixtureApps.chatGPT])
    let core = FixtureCore(.chatGPTOpen, apps: apps)
    let model = AppModel(testing: core, appControl: FixtureAppControl(apps))
    await model.refresh()
    #expect(await core.holding("codex").map(\.kind) == ["chatgpt_app"])
    #expect(await core.holding("claude").isEmpty)

    await model.switchAsked(to: "codex/spare")
    let quitting = try #require(model.quitting)
    #expect(quitting.name == "ChatGPT")
    await model.quitAndSwitch(quitting)
    #expect(apps.asked == ["quit \(FixtureApps.chatGPT)", "open \(FixtureApps.chatGPT)"])
    #expect(inUse(try await core.statusOffline()).contains("codex/spare"))
}
