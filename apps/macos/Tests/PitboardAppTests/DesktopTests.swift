import Foundation
import PitboardKit
import Testing

@testable import PitboardApp

// MARK: - Stand-ins

/// Claude Desktop as the core lists it.
let claudeDesktop = Tool(
    code: "desktop", name: "Claude Desktop", program: "Claude", service: "Anthropic")

private let claudeApp = "com.anthropic.claudefordesktop"

/// What happened, in the order it happened, across the core and the apps: the order is the
/// whole point, since Claude has to be gone before its files are touched and back after.
final class Trail: @unchecked Sendable {
    // Unchecked because the core is called from whatever thread the model likes; every read
    // and write holds `lock`.
    private let lock = NSLock()
    private var kept: [String] = []

    var steps: [String] { lock.withLock { kept } }

    func add(_ step: String) {
        lock.withLock { kept.append(step) }
    }
}

/// Claude Desktop's side of a machine, in memory: who is signed in, what a sign-out left
/// waiting, and whether live usage is on. Every call that changes something is written to
/// the trail; every call to turn live usage on is counted, since only a button may make it.
private final class DesktopCore: Core, @unchecked Sendable {
    let trail: Trail
    var accounts: [Account]
    var awaiting: Awaiting?
    var live = LiveUsageState(enabled: false, approval: "unknown", reason: nil, lastOkAt: nil)
    private(set) var enableCalls = 0
    private(set) var disableCalls = 0
    var changed: Int64 = 0
    /// Whether Claude's app is open, as the app control has it: on a real Mac the process
    /// list and macOS's list of apps agree.
    var appOpen: @MainActor () -> Bool = { true }
    /// How many more times the process list still shows Claude's helpers once the app has
    /// quit: on a real Mac they outlive it for a moment.
    var lingering = 0
    /// What live usage becomes during the next read, which is when the real core reads
    /// Claude's key and can find macOS no longer lets it.
    var liveOnRead: LiveUsageState?
    /// Why the next request to turn live usage on is refused, and whether the core records
    /// it: it records what macOS answered, not a question it could not ask.
    var refusing: (reason: String, recorded: Bool)?
    /// Why the next enrol fails, after the quiet gate has let it through.
    var enrolFailing: PitboardError?
    /// Why the next switch or sign-out fails, after the quiet gate has let it through.
    var switchFailing: PitboardError?

    init(trail: Trail, accounts: [Account]) {
        self.trail = trail
        self.accounts = accounts
    }

    func status(fresh: Bool) async throws -> Status {
        if let liveOnRead {
            live = liveOnRead
            self.liveOnRead = nil
        }
        return Status(now: 0, accounts: accounts, warnings: [])
    }
    func statusOffline() async throws -> Status {
        Status(now: 0, accounts: accounts, warnings: [])
    }
    func doctor() async -> Diagnosis { Diagnosis(checks: [], healthy: true) }
    /// Claude Desktop always names the app it runs as; whether the app is open is for the
    /// app control to say, as it is on a real Mac.
    func holding(_ provider: String) async -> [Holding] {
        guard provider == "desktop", await claudeRuns() else { return [] }
        return [
            Holding(
                kind: "claude_desktop_app", phrase: "the Claude app", pids: [77],
                remedy: .reopenApp(bundleId: claudeApp, name: "Claude"))
        ]
    }
    /// Whether anything runs from Claude's bundle: the app, or helpers it left behind. Each
    /// look at the helpers counts one down.
    private func claudeRuns() async -> Bool {
        if await appOpen() { return true }
        guard lingering > 0 else { return false }
        lingering -= 1
        return true
    }
    /// The core's quiet gate: nothing of Claude's files is touched while anything runs from
    /// its bundle.
    private func quiet() async throws {
        guard await appOpen() || lingering > 0 else { return }
        throw PitboardError.Failed(
            code: "app_still_open", cause: nil, message: "the Claude app is still open",
            warnings: [])
    }
    func switchTo(_ label: String) async throws -> Switched {
        try await quiet()
        if let switchFailing {
            self.switchFailing = nil
            throw switchFailing
        }
        trail.add("switch \(label)")
        let from = accounts.first { $0.provider == "desktop" && $0.signedIn }?.qualified ?? ""
        accounts = accounts.map { account in
            guard account.provider == "desktop" else { return account }
            return desktop(
                account.label, signedIn: account.qualified == label,
                uuid: account.accountUuid)
        }
        awaiting = nil
        changed += 1
        return Switched(
            outcome: .switched(
                provider: "desktop", from: from, to: label,
                adoption: .nextLaunch(program: "Claude")),
            warnings: [])
    }
    func switchToSignedOut(_ tool: String) async throws -> Switched {
        try await quiet()
        if let switchFailing {
            self.switchFailing = nil
            throw switchFailing
        }
        trail.add("signOut \(tool)")
        let from = accounts.first { $0.provider == tool && $0.signedIn }
        accounts = accounts.map { account in
            guard account.provider == tool else { return account }
            return desktop(account.label, signedIn: false, uuid: account.accountUuid)
        }
        awaiting = Awaiting(fromLabel: from?.label, startedAt: 1_000)
        changed += 1
        return Switched(
            outcome: .switched(
                provider: tool, from: from?.qualified ?? "", to: "",
                adoption: .nextLaunch(program: "Claude")),
            warnings: [])
    }
    func enrollCurrent(_ label: String) async throws -> Enrolled {
        try await quiet()
        if let enrolFailing {
            self.enrolFailing = nil
            throw enrolFailing
        }
        trail.add("enroll \(label)")
        let name = String(label.dropFirst("desktop/".count))
        let kept = accounts.filter { !($0.provider == "desktop" && $0.signedIn) }
        accounts = kept + [desktop(name, signedIn: true, uuid: name)]
        awaiting = nil
        changed += 1
        return Enrolled(email: "", outcome: .current, warnings: [])
    }
    func awaitingSignIn() async -> Awaiting? { awaiting }
    func liveUsage() async -> LiveUsageState { live }
    func enableLiveUsage() async throws -> LiveUsageState {
        enableCalls += 1
        if let refusing {
            if refusing.recorded {
                live = LiveUsageState(
                    enabled: live.enabled, approval: "needs_approval",
                    reason: refusing.reason, lastOkAt: live.lastOkAt)
            }
            throw PitboardError.Failed(
                code: "live_usage_not_allowed", cause: nil,
                message: "the core's own words for \(refusing.reason)", warnings: [])
        }
        live = LiveUsageState(enabled: true, approval: "granted", reason: nil, lastOkAt: 1)
        return live
    }
    func disableLiveUsage() async throws -> LiveUsageState {
        disableCalls += 1
        live = LiveUsageState(enabled: false, approval: "unknown", reason: nil, lastOkAt: nil)
        return live
    }
    func forget(_ label: String) async throws -> Changed { Changed(email: "", warnings: []) }
    func rename(_ from: String, to: String) async throws -> Changed {
        Changed(email: "", warnings: [])
    }
    func signIn(_ label: String) async throws -> SignIn {
        throw PitboardError.Failed(
            code: "sign_in_unsupported", cause: nil, message: "not here", warnings: [])
    }
    func abandonRecovery() async throws -> Abandoned? { nil }
    func log(limit: UInt32) async -> [Change] { [] }
    func renew() async -> [Renewed] { [] }
    func schedule() async -> Schedule { .absent }
    func scheduleInstall() async throws -> String { "/nowhere" }
    func scheduleUninstall() async throws -> Bool { false }
    func scheduleRepair() async throws -> Bool { false }
    func changedAt() async -> Int64 { changed }
    func readingsChangedAt() async -> Int64 { 0 }
    func tools() -> [Tool] { bothTools + [claudeDesktop] }
    func installed() async -> [Tool] { [claudeCode, claudeDesktop] }
    func searchPath() async -> String? { nil }
}

/// Quits and opens apps by writing it on the trail, and nothing on the machine running the
/// tests: this Mac may have Claude open, and no test may quit it.
@MainActor
private final class TrailAppControl: AppControl {
    let trail: Trail
    var running: Set<String>

    init(trail: Trail, running: Set<String>) {
        self.trail = trail
        self.running = running
    }

    func running(_ bundleID: String) -> URL? {
        running.contains(bundleID) ? StandInAppControl.copy(of: bundleID) : nil
    }

    func installed(_ bundleID: String) -> URL? {
        StandInAppControl.copy(of: bundleID)
    }

    func requestQuit(_ bundleID: String) {
        trail.add("quit \(bundleID)")
        running.remove(bundleID)
    }

    func open(_ copy: URL) {
        let bundleID = copy.deletingPathExtension().lastPathComponent
        trail.add("open \(bundleID)")
        running.insert(bundleID)
    }
}

/// A Claude Desktop account as the core reports one, its numbers from Claude's history.
private func desktop(_ label: String?, signedIn: Bool, uuid: String? = nil) -> Account {
    let uuid = uuid ?? label ?? "someone"
    return Account(
        id: "desktop:\(uuid)", provider: "desktop", label: label,
        qualified: label.map { "desktop/\($0)" }, unplaced: false, email: "",
        accountUuid: uuid, signedIn: signedIn, switchable: !signedIn && label != nil,
        parked: nil,
        usage: Usage(
            source: .desktopHistory, observedAt: 0,
            windows: [window("five_hour", 12, resets: nil)], verified: false),
        stale: nil, staleExplanation: nil, lastsSeconds: nil, lastsBurning: false)
}

/// A Mac with Claude Desktop signed in to `personal` with `work` parked, and Claude open
/// unless `running` says otherwise.
@MainActor
private func desktopMachine(
    running: Bool = true, defaults: UserDefaults = TestDefaults()
) -> (AppModel, DesktopCore, TrailAppControl, Trail) {
    let trail = Trail()
    let core = DesktopCore(
        trail: trail,
        accounts: [desktop("personal", signedIn: true), desktop("work", signedIn: false)])
    let apps = TrailAppControl(trail: trail, running: running ? [claudeApp] : [])
    core.appOpen = { [apps] in apps.running.contains(claudeApp) }
    let model = AppModel(testing: core, defaults: defaults, appControl: apps)
    model.quitWithin = .milliseconds(50)
    model.closeCheckEvery = .milliseconds(5)
    return (model, core, apps, trail)
}

// MARK: - Switching

/// Claude keeps its sign-in in its own files while it runs, so it is quit the way
/// Command-Q quits it, switched, and opened again, in that order.
@MainActor
@Test func testDesktopSwitchQuitsClaudeAndOpensItAgain() async throws {
    let (model, _, apps, trail) = desktopMachine()
    await model.switchAsked(to: "desktop/work")
    let question = try #require(model.quitting)
    #expect(question.name == "Claude")
    #expect(question.bundleID == claudeApp)
    #expect(trail.steps.isEmpty, "nothing is quit before the person says so")

    await model.quitAndSwitch(question)
    #expect(trail.steps == ["quit \(claudeApp)", "switch desktop/work", "open \(claudeApp)"])
    #expect(apps.running == [claudeApp])
    #expect(model.presentedFailure == nil)
    #expect(model.switching == nil)
}

/// Pitboard opened Claude again itself, and Claude reads the new sign-in as it starts:
/// there is nothing to restart and nothing to open.
@MainActor
@Test func testDesktopSwitchShowsNoRestartNoticeAfterReopening() async throws {
    let (model, _, _, _) = desktopMachine()
    await model.switchAsked(to: "desktop/work")
    await model.quitAndSwitch(try #require(model.quitting))
    let last = try #require(model.lastSwitches.first)
    #expect(last.provider == "desktop")
    #expect(last.notice == nil)
    let notice = try #require(model.notices().first { $0.id == "switch/desktop" })
    #expect(notice.lines.isEmpty)
    #expect(notice.severity == .info)
}

/// Claude that is not open holds nothing, so the switch is made at once, nothing is quit
/// or opened, and the notice says to open Claude.
@MainActor
@Test func testClaudeNotRunningSwitchesWithoutAsking() async throws {
    let (model, _, _, trail) = desktopMachine(running: false)
    await model.switchAsked(to: "desktop/work")
    #expect(model.quitting == nil)
    #expect(trail.steps == ["switch desktop/work"])
    #expect(model.lastSwitches.first?.notice == "Open Claude to use work.")
}

/// Claude's helpers outlive the app for a moment once it quits, and the core touches none
/// of Claude's files while any runs, so the switch waits for them to go.
@MainActor
@Test func aSwitchWaitsForClaudesHelpersToClose() async throws {
    let (model, core, apps, trail) = desktopMachine()
    await model.switchAsked(to: "desktop/work")
    let question = try #require(model.quitting)
    core.lingering = 2
    await model.quitAndSwitch(question)
    #expect(model.presentedFailure == nil)
    #expect(trail.steps == ["quit \(claudeApp)", "switch desktop/work", "open \(claudeApp)"])
    #expect(apps.running == [claudeApp])
}

/// Helpers that never go stop the switch before anything changes, and Claude is opened
/// again all the same: Pitboard quit it.
@MainActor
@Test func helpersThatStayStopTheSwitchAndClaudeIsOpenedAgain() async throws {
    let (model, core, apps, trail) = desktopMachine()
    model.closeWithin = .milliseconds(50)
    await model.switchAsked(to: "desktop/work")
    let question = try #require(model.quitting)
    core.lingering = .max
    await model.quitAndSwitch(question)
    #expect(trail.steps == ["quit \(claudeApp)", "open \(claudeApp)"])
    #expect(apps.running == [claudeApp])
    #expect(model.presentedFailure?.code == "app_still_open")
}

/// The same wait comes before an add's sign-out, and helpers that stay leave Claude
/// signed in and open again.
@MainActor
@Test func anAddWaitsForClaudesHelpersAndReopensWhenTheyStay() async throws {
    let (model, core, apps, trail) = desktopMachine()
    model.present(.add(provider: "desktop"))
    core.lingering = 2
    #expect(await model.desktopAddAsked() == nil)
    #expect(trail.steps == ["quit \(claudeApp)", "signOut desktop", "open \(claudeApp)"])

    model.closeWithin = .milliseconds(50)
    core.lingering = .max
    core.accounts.append(desktop(nil, signedIn: true, uuid: "new"))
    let failure = try #require(await model.desktopEnrollAsked(label: "home"))
    #expect(failure.code == "app_still_open")
    #expect(Array(trail.steps.suffix(2)) == ["quit \(claudeApp)", "open \(claudeApp)"])
    #expect(apps.running == [claudeApp])
}

/// A switch that fails partway leaves half of each account in Claude's folder until the
/// next change finishes or undoes it, so Claude, quit for it, stays closed rather than
/// opened on that.
@MainActor
@Test func aSwitchLeftUnfinishedLeavesClaudeClosed() async throws {
    let (model, core, apps, trail) = desktopMachine()
    await model.switchAsked(to: "desktop/work")
    let question = try #require(model.quitting)
    core.switchFailing = PitboardError.Failed(
        code: "io", cause: nil, message: "could not move an item",
        warnings: [Warning(code: "switch_unfinished", message: "stopped partway")])
    await model.quitAndSwitch(question)
    #expect(trail.steps == ["quit \(claudeApp)"])
    #expect(apps.running.isEmpty)
    #expect(model.presentedFailure?.warnings.map(\.code) == ["switch_unfinished"])
}

/// The same holds for an add's sign-out, and for Claude opening partway, which the core
/// says with a code of its own: it is not opened again by Pitboard either.
@MainActor
@Test func aSignOutLeftUnfinishedLeavesClaudeClosed() async throws {
    let (model, core, apps, trail) = desktopMachine()
    model.present(.add(provider: "desktop"))
    core.switchFailing = PitboardError.Failed(
        code: "app_opened_midway", cause: nil, message: "Claude was opened partway",
        warnings: [])
    let failure = try #require(await model.desktopAddAsked())
    #expect(failure.code == "app_opened_midway")
    #expect(trail.steps == ["quit \(claudeApp)"])
    #expect(apps.running.isEmpty)

    // A failure that moved nothing still opens Claude again.
    apps.running = [claudeApp]
    core.switchFailing = PitboardError.Failed(
        code: "io", cause: nil, message: "could not read", warnings: [])
    #expect(await model.desktopAddAsked() != nil)
    #expect(Array(trail.steps.suffix(2)) == ["quit \(claudeApp)", "open \(claudeApp)"])
}

/// Claude that will not quit stops everything before anything changes, and is not opened:
/// it never closed.
@MainActor
@Test func claudeThatWillNotQuitChangesNothing() async throws {
    let trail = Trail()
    let core = DesktopCore(
        trail: trail,
        accounts: [desktop("personal", signedIn: true), desktop("work", signedIn: false)])
    let apps = StandInAppControl(running: [claudeApp], quits: false)
    let model = AppModel(testing: core, appControl: apps)
    model.quitWithin = .milliseconds(50)
    model.present(.add(provider: "desktop"))
    let failure = try #require(await model.desktopAddAsked())
    #expect(trail.steps.isEmpty)
    #expect(apps.asked == ["quit \(claudeApp)"])
    #expect(failure.code == "app_still_open")
    #expect(failure.message.contains("Claude is still open, so nothing has changed"))
}

// MARK: - Live usage

/// Reading Claude's key can raise the keychain's prompt, so nothing that runs by itself
/// ever turns live usage on: not a read, not one somebody asked for, not a change seen.
@MainActor
@Test func testBackgroundRefreshNeverEnablesLiveUsage() async {
    let (model, core, _, _) = desktopMachine()
    core.live = LiveUsageState(
        enabled: true, approval: "needs_approval", reason: "denied", lastOkAt: nil)
    await model.refresh()
    await model.refresh(asked: true)
    await model.noticeOtherChangesForTesting()
    core.changed += 1
    await model.noticeOtherChangesForTesting()
    await model.refresh(ifOlderThan: 0)
    #expect(core.enableCalls == 0)
    #expect(model.liveUsage?.approval == "needs_approval")
}

/// The toggle and the notice's button only show the sheet that explains the prompt; its
/// Continue is the one thing that turns live usage on. Turning it off asks for nothing.
@MainActor
@Test func testLiveUsageSheetIsTheOnlyPathToEnable() async {
    let (model, core, _, _) = desktopMachine()
    await model.refresh()
    model.liveUsageAsked()
    #expect(model.sheet == .liveUsage)
    #expect(core.enableCalls == 0)

    #expect(await model.liveUsageEnableAsked() == nil)
    #expect(core.enableCalls == 1)
    #expect(model.sheet == nil)
    #expect(model.liveUsage?.enabled == true)

    #expect(await model.liveUsageDisable() == nil)
    #expect(core.disableCalls == 1)
    #expect(core.enableCalls == 1)
    #expect(model.liveUsage?.enabled == false)
}

/// macOS stopping Pitboard reading the key is told once, not at every read, and told
/// again only after it has worked in between. An app opened again does not tell it again.
@MainActor
@Test func testNeedsApprovalNotifiesOnce() async throws {
    let defaults = TestDefaults()
    let (model, core, _, _) = desktopMachine(defaults: defaults)
    core.live = LiveUsageState(
        enabled: true, approval: "needs_approval", reason: "item_changed", lastOkAt: 1)
    await model.refresh()
    await model.refresh()
    #expect(model.liveUsagePausedToldForTesting == 1)
    let notice = try #require(model.notices().first { $0.id == "live-usage" })
    #expect(notice.title == "Live usage for Claude is paused")
    #expect(notice.actions == [.allowLiveUsage])

    let again = AppModel(testing: core, defaults: defaults, appControl: StandInAppControl())
    await again.refresh()
    #expect(again.liveUsagePausedToldForTesting == 0, "told already, before it opened")

    core.live = LiveUsageState(enabled: true, approval: "granted", reason: nil, lastOkAt: 2)
    await model.refresh()
    #expect(model.notices().allSatisfy { $0.id != "live-usage" })
    core.live = LiveUsageState(
        enabled: true, approval: "needs_approval", reason: "denied", lastOkAt: 2)
    await model.refresh()
    #expect(model.liveUsagePausedToldForTesting == 2)
}

/// A read is when the core reads Claude's key and finds macOS stopped allowing it, so
/// that read is the one that says so, not the one after.
@MainActor
@Test func theReadThatFindsLiveUsagePausedSaysSo() async throws {
    let (model, core, _, _) = desktopMachine()
    core.live = LiveUsageState(enabled: true, approval: "granted", reason: nil, lastOkAt: 1)
    core.liveOnRead = LiveUsageState(
        enabled: true, approval: "needs_approval", reason: "item_changed", lastOkAt: 1)
    await model.refresh()
    #expect(model.liveUsage?.approval == "needs_approval")
    #expect(model.notices().contains { $0.id == "live-usage" })
    #expect(model.liveUsagePausedToldForTesting == 1)
}

/// What macOS answered decides what the sheet says to do; a question it could not ask is
/// said in the core's own words, which know why.
@MainActor
@Test func aRefusedEnableSaysWhatMacOSAnswered() async throws {
    let (model, core, _, _) = desktopMachine()
    await model.refresh()
    core.refusing = ("denied", true)
    let denied = try #require(await model.liveUsageEnableAsked())
    #expect(denied.message == Advice.desktop("live_usage_not_allowed", reason: "denied"))
    #expect(denied.message.contains("Always Allow"))

    core.refusing = ("no_gui", false)
    let noScreen = try #require(await model.liveUsageEnableAsked())
    #expect(noScreen.message == "the core's own words for no_gui")
}

// MARK: - Adding an account

/// Adding a second Claude account: Claude is quit, the account in use is parked and Claude
/// left signed out, Claude opens for the sign-in, and once it is named, Claude is quit
/// again, the new sign-in enrolled, and Claude opened on it.
@MainActor
@Test func testAddFlowParksThenEnrollsThenReopens() async {
    let (model, core, apps, trail) = desktopMachine()
    await model.refresh()
    model.present(.add(provider: "desktop"))

    #expect(await model.desktopAddAsked() == nil)
    #expect(trail.steps == ["quit \(claudeApp)", "signOut desktop", "open \(claudeApp)"])
    #expect(model.desktopAwaiting?.fromLabel == "personal")
    #expect(model.sheet == .add(provider: "desktop"), "the sheet stays for the next step")
    #expect(model.lastSwitches.isEmpty, "a sign-out is not a switch to anybody")

    // Somebody signs in to another account in Claude.
    core.accounts.append(desktop(nil, signedIn: true, uuid: "new"))
    #expect(await model.desktopEnrollAsked(label: "home") == nil)
    #expect(
        trail.steps == [
            "quit \(claudeApp)", "signOut desktop", "open \(claudeApp)",
            "quit \(claudeApp)", "enroll desktop/home", "open \(claudeApp)",
        ])
    #expect(model.desktopAwaiting == nil)
    #expect(model.sheet == nil)
    #expect(apps.running == [claudeApp])
    #expect(model.status?.accounts.contains { $0.qualified == "desktop/home" } == true)
}

/// Claude that was not open is left closed by the sign-out, so the next step says to open
/// it and offers to, rather than taking it to be open.
@MainActor
@Test func anAddWithClaudeClosedOffersToOpenIt() async {
    let (model, _, apps, trail) = desktopMachine(running: false)
    await model.refresh()
    model.present(.add(provider: "desktop"))
    #expect(await model.desktopAddAsked() == nil)
    #expect(trail.steps == ["signOut desktop"], "nothing was open, so nothing is opened")
    #expect(model.claudeLeftClosed)

    #expect(model.openClaudeAsked() == nil)
    #expect(trail.steps == ["signOut desktop", "open \(claudeApp)"])
    #expect(apps.running == [claudeApp])
    #expect(!model.claudeLeftClosed)
}

/// Claude opened by the person after Pitboard left it closed is not offered again when the
/// sheet comes back from the menu: what was true when the account was put aside is old.
@MainActor
@Test func aSheetShownAgainAsksWhetherClaudeIsStillClosed() async {
    let (model, _, apps, _) = desktopMachine(running: false)
    await model.refresh()
    model.present(.add(provider: "desktop"))
    #expect(await model.desktopAddAsked() == nil)
    #expect(model.claudeLeftClosed)

    model.sheet = nil
    apps.running.insert(claudeApp)
    model.present(.add(provider: "desktop"))
    #expect(!model.claudeLeftClosed, "Claude is open now, so it is not offered")
}

/// An enrol that fails says why in the sheet, and its warnings stay in the window once the
/// sheet is closed.
@MainActor
@Test func aFailedDesktopEnrolKeepsItsWarnings() async {
    let (model, core, _, trail) = desktopMachine()
    await model.refresh()
    model.present(.add(provider: "desktop"))
    #expect(await model.desktopAddAsked() == nil)
    core.accounts.append(desktop(nil, signedIn: true, uuid: "new"))

    let said = Warning(code: "strays_kept", message: "a stray file was kept")
    core.enrolFailing = PitboardError.Failed(
        code: "desktop_sign_in_incomplete", cause: nil, message: "the core's own words",
        warnings: [said])
    let failure = await model.desktopEnrollAsked(label: "home")
    #expect(failure?.code == "desktop_sign_in_incomplete")
    #expect(failure?.message == Advice.desktop("desktop_sign_in_incomplete"))
    #expect(model.sheet == .add(provider: "desktop"), "the sheet stays with the name in it")
    #expect(trail.steps.last == "open \(claudeApp)", "Claude is opened again")

    model.sheet = nil
    #expect(model.warnings.contains(said))
}

/// An app that quit halfway through adding an account finds it waiting when it opens, and
/// shows the sheet again once, not after every read.
@MainActor
@Test func testAddFlowResumesFromAwaiting() async {
    let (model, core, _, _) = desktopMachine()
    core.accounts = [desktop("personal", signedIn: false), desktop("work", signedIn: false)]
    core.awaiting = Awaiting(fromLabel: "personal", startedAt: 100)
    await model.refresh()
    #expect(model.desktopAwaiting?.fromLabel == "personal")
    #expect(model.sheet == .add(provider: "desktop"))

    model.sheet = nil
    await model.refresh()
    #expect(model.sheet == nil, "asked once, not after every read")
}

/// An add the command line started while the app runs is somebody at a terminal, maybe
/// nobody at the screen: the window does not come forward for it. The menu offers to
/// finish it, and the window says it is waiting.
@MainActor
@Test func anAddStartedElsewhereIsOfferedNotShown() async throws {
    let (model, core, _, _) = desktopMachine()
    await model.refresh()
    let requests = model.windowRequests

    core.accounts = [desktop("personal", signedIn: false), desktop("work", signedIn: false)]
    core.awaiting = Awaiting(fromLabel: "personal", startedAt: 200)
    core.changed += 1
    await model.noticeOtherChangesForTesting()
    await model.noticeOtherChangesForTesting()
    await model.refresh()

    #expect(model.desktopAwaiting?.fromLabel == "personal")
    #expect(model.sheet == nil, "the sheet is not put up uninvited")
    #expect(model.windowRequests == requests, "the window is not brought forward")
    let notice = try #require(model.notices().first { $0.id == "desktop-awaiting" })
    #expect(notice.title == "Adding a Claude account isn’t finished")
    #expect(notice.severity == .warning, "the menu says it too")
    #expect(notice.actions == [.finishDesktopAdd])

    model.finishDesktopAddAsked()
    #expect(model.sheet == .add(provider: "desktop"))
    #expect(model.windowRequests == requests + 1)
}

/// An add the app started itself keeps its sheet up, and once somebody closes it, no read
/// puts it up again: the menu and the notice are the way back.
@MainActor
@Test func anAddStartedHereIsNotShownAgainOnceClosed() async {
    let (model, core, _, _) = desktopMachine()
    await model.refresh()
    model.present(.add(provider: "desktop"))
    #expect(await model.desktopAddAsked() == nil)
    #expect(model.sheet == .add(provider: "desktop"))

    model.sheet = nil
    let requests = model.windowRequests
    core.changed += 1
    await model.noticeOtherChangesForTesting()
    await model.refresh()
    #expect(model.sheet == nil)
    #expect(model.windowRequests == requests)
    #expect(model.notices().contains { $0.id == "desktop-awaiting" })
}

/// Nothing waiting, nothing said.
@MainActor
@Test func noAddWaitingSaysNothing() async {
    let (model, _, _, _) = desktopMachine()
    await model.refresh()
    #expect(!model.notices().contains { $0.id == "desktop-awaiting" })
}

/// Changing one's mind halfway puts the parked account back, through Claude quit and
/// opened again like any switch.
@MainActor
@Test func testPutBackSwitchesToTheParkedAccount() async throws {
    let (model, core, _, trail) = desktopMachine()
    core.accounts = [desktop("personal", signedIn: false), desktop("work", signedIn: false)]
    core.awaiting = Awaiting(fromLabel: "personal", startedAt: 100)
    await model.refresh()
    #expect(model.sheet == .add(provider: "desktop"))

    #expect(await model.desktopPutBack() == nil)
    #expect(
        trail.steps == ["quit \(claudeApp)", "switch desktop/personal", "open \(claudeApp)"])
    #expect(model.desktopAwaiting == nil)
    #expect(model.sheet == nil)
    #expect(model.lastSwitches.first?.notice == nil, "Claude was opened again on it")
}

/// Putting the parked account back with Claude closed is a switch from nobody: nothing is
/// quit or opened, it is said as a switch to that account, and Claude has to be opened.
@MainActor
@Test func aPutBackWithClaudeClosedSaysToOpenIt() async throws {
    let (model, core, _, trail) = desktopMachine(running: false)
    core.accounts = [desktop("personal", signedIn: false), desktop("work", signedIn: false)]
    core.awaiting = Awaiting(fromLabel: "personal", startedAt: 100)
    await model.refresh()

    #expect(await model.desktopPutBack() == nil)
    #expect(trail.steps == ["switch desktop/personal"])
    let last = try #require(model.lastSwitches.first)
    #expect(last.to == "desktop/personal")
    #expect(last.notice == "Open Claude to use personal.")
    #expect(!model.notices().contains { $0.id == "desktop-awaiting" })
}

// MARK: - What is said

/// Claude's own alert, which says why Claude has to quit rather than ChatGPT's reason.
@Test func claudeIsAskedToQuitInItsOwnWords() {
    #expect(
        quitQuestion(name: "Claude", to: "work")
            == "Claude keeps its sign-in in its own files while it runs. Pitboard quits it, "
            + "switches to work, and opens it again.")
    #expect(
        quitQuestion(name: "ChatGPT", to: "spare")
            == "ChatGPT keeps using the account it started with until it quits. Pitboard "
            + "quits it, switches, and opens it again.")
}

/// A Claude Desktop sign-in lapses on its own and cannot be renewed, which a parked row
/// says with its date.
@Test func aDesktopRowSaysWhenItsSignInLapses() throws {
    let now = Date(timeIntervalSince1970: 1_790_000_000)
    let lapses = Int64(1_790_000_000 + 10 * 86_400)
    let parked = Parked(parkedAt: 0, accessExpiresAt: nil, refreshExpiresAt: lapses)
    var formatter = Date.FormatStyle(timeZone: .current).month(.abbreviated).day()
    formatter.locale = Locale(identifier: "en_US_POSIX")
    let day = Date(timeIntervalSince1970: TimeInterval(lapses)).formatted(formatter)
    #expect(
        desktopLapseNote(parked, now: now)
            == "Sign-in lapses on \(day). Pitboard cannot renew Claude Desktop sign-ins.")
    #expect(
        desktopLapseNote(
            Parked(parkedAt: 0, accessExpiresAt: nil, refreshExpiresAt: 1), now: now)
            == "Its sign-in has lapsed. Sign in to it again in Claude to use it.")
    #expect(desktopLapseNote(nil, now: now) == nil)

    let row = AccountDescription(
        Account(
            id: "desktop:w", provider: "desktop", label: "work", qualified: "desktop/work",
            unplaced: false, email: "", accountUuid: "w", signedIn: false, switchable: true,
            parked: parked,
            usage: Usage(
                source: .desktopHistory, observedAt: 0, windows: [], verified: false),
            stale: nil, staleExplanation: nil, lastsSeconds: nil, lastsBurning: false),
        switching: nil, busy: false, now: now)
    #expect(row.parkedNote == desktopLapseNote(parked, now: now))
    #expect(row.sourceNote == "From Claude’s history, unconfirmed")
}

/// The core's own messages name commands to type; in the app the same failure says what
/// to do here.
@Test func desktopFailuresSayWhatToDoInTheApp() {
    for code in [
        "app_still_open", "app_state_unknown", "app_opened_midway", "recovery_waiting",
        "desktop_identity_unconfirmed", "parked_login_expired",
    ] {
        let said = Advice.desktop(code)
        #expect(said != nil, "\(code)")
        #expect(said?.contains("`") == false, "\(code) names no command")
    }
    #expect(Advice.desktop("unknown_account") == nil)
    // Only a refusal before anything moved says nothing has changed.
    #expect(Advice.desktop("app_opened_midway")?.contains("nothing has changed") == false)
    #expect(Advice.desktop("app_state_unknown")?.contains("couldn’t tell") == true)

    // Live usage refused: what to do depends on what macOS answered.
    let reasons = [
        "no_gui", "denied", "item_changed", "auth_failed", "timed_out", "item_missing",
        "key_does_not_decrypt", "no_session", "other",
    ]
    for reason in reasons {
        let said = Advice.desktop("live_usage_not_allowed", reason: reason)
        #expect(said != nil, "\(reason)")
        #expect(said?.contains("`") == false, "\(reason) names no command")
        #expect(said?.contains("Terminal") == false, "\(reason) is said for the app")
    }
    let noScreen = Advice.desktop("live_usage_not_allowed", reason: "no_gui")
    #expect(noScreen?.contains("Always Allow") == false, "nothing was asked to allow")
    #expect(noScreen?.contains("screen") == true)
    #expect(
        Advice.desktop("live_usage_not_allowed", reason: "denied")?.contains("Always Allow")
            == true)
    #expect(
        Advice.desktop("live_usage_not_allowed", reason: "item_changed")
            == Advice.desktop("live_usage_not_allowed", reason: "denied"))
    // A wrong password is answered as Deny is (experiment U-K2), and a question Pitboard
    // stopped waiting on stays on screen (U-K3), so each says what may have happened.
    #expect(
        Advice.desktop("live_usage_not_allowed", reason: "denied")?.contains("password")
            == true)
    let timedOut = Advice.desktop("live_usage_not_allowed", reason: "timed_out")
    #expect(timedOut?.contains("may still") == true)
    #expect(timedOut?.contains("Deny") == true)
    // Without a reason, the core's own message says which it was.
    #expect(Advice.desktop("live_usage_not_allowed") == nil)
}

/// Each new warning has a heading of its own in the menu.
@Test func desktopWarningsHaveTitles() {
    for code in [
        "recovery_waiting", "park_expires_soon", "strays_kept", "replaced_outside_pitboard",
        "switch_unfinished",
    ] {
        #expect(
            warningTitle(Warning(code: code, message: "m")) != "Pitboard has a warning",
            "\(code)")
    }
}

/// What the Settings line says about live usage, in each state it can be in.
@Test func liveUsageIsDescribedInEachState() {
    let off = LiveUsageState(enabled: false, approval: "unknown", reason: nil, lastOkAt: nil)
    let on = LiveUsageState(enabled: true, approval: "granted", reason: nil, lastOkAt: 1)
    let paused = LiveUsageState(
        enabled: true, approval: "needs_approval", reason: "denied", lastOkAt: 1)
    // Claude's history matched claude.ai when measured, so it is not called unconfirmed.
    #expect(
        liveUsageLine(off)
            == "Off. Numbers come from Claude’s own history, without reset times.")
    #expect(liveUsageLine(on) == "On. Numbers come from claude.ai.")
    #expect(
        liveUsageLine(paused) == "Paused: macOS stopped letting Pitboard read Claude’s key.")
}

/// The fixture with Claude Desktop starts where its UI tests expect: two accounts, one
/// whose live usage needs approving, and one parked whose sign-in lapses soon.
@MainActor
@Test func theDesktopFixtureHasTwoClaudeAccounts() async throws {
    let core = FixtureCore(.claudeDesktop)
    #expect(core.tools().map(\.code) == ["claude", "codex", "desktop"])
    let read = try await core.status(fresh: false)
    let desktops = read.accounts.filter { $0.provider == "desktop" }
    #expect(desktops.compactMap(\.qualified) == ["desktop/personal", "desktop/work"])
    #expect(desktops.first?.stale == "live_usage_needs_approval")
    #expect(desktops.last?.parked?.refreshExpiresAt != nil)
    #expect(await core.liveUsage().approval == "needs_approval")
    #expect(await core.awaitingSignIn() == nil)

    // Named the way the real core names Claude's app, so what branches on it runs the same.
    let open = FixtureCore(.claudeDesktop, apps: FixtureApps(running: [FixtureApps.claude]))
    #expect(await open.holding("desktop").map(\.kind) == ["claude_desktop_app"])
}
