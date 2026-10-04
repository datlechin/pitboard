import Foundation
import PitboardKit
import Testing

@testable import PitboardApp

/// Answers what the settings ask of the core about this Mac, and counts each ask, so a test
/// can say what was asked and what was not. Nothing about the machine asks about accounts,
/// so those calls answer nothing.
private final class MachineStub: Core, @unchecked Sendable {
    /// What the scheduler has installed.
    var scheduled: Schedule = .absent
    private(set) var scheduleReads = 0
    private(set) var installs = 0
    private(set) var uninstalls = 0
    /// What the next install or uninstall is refused with, if anything.
    var refusing: Error?
    /// What repairing a schedule an older app wrote comes to.
    var repairs: Result<Bool, Error> = .success(false)
    private(set) var repairAsks = 0
    var renewals: [Renewed] = []
    private(set) var renewAsks = 0
    var checks: [Check] = []
    private(set) var doctorAsks = 0
    /// What Pitboard has changed, oldest first, as the core's log keeps it.
    var history: [Change] = []
    private(set) var logLimits: [UInt32] = []
    /// The login shell's `PATH`, as far as the app looks in it.
    var path: String?
    /// Asked of the model while a renewal, doctor or a change to the schedule is running,
    /// and each answer kept.
    var busy: (@MainActor () -> Bool)?
    private(set) var wasBusy: [Bool] = []
    /// Done while the schedule is being changed, as somebody pressing the switch again.
    var meanwhile: (@MainActor () async -> Void)?

    func schedule() async -> Schedule {
        scheduleReads += 1
        return scheduled
    }
    func scheduleInstall() async throws -> String {
        installs += 1
        if let busy { wasBusy.append(await busy()) }
        await meanwhile?()
        if let refusing { throw refusing }
        scheduled = .installed(path: plist, everySeconds: 86_400)
        return plist
    }
    func scheduleUninstall() async throws -> Bool {
        uninstalls += 1
        if let busy { wasBusy.append(await busy()) }
        await meanwhile?()
        if let refusing { throw refusing }
        defer { scheduled = .absent }
        return scheduled != .absent
    }
    func scheduleRepair() async throws -> Bool {
        repairAsks += 1
        return try repairs.get()
    }
    func renew() async -> [Renewed] {
        renewAsks += 1
        if let busy { wasBusy.append(await busy()) }
        return renewals
    }
    func holding(_ provider: String) async -> [Holding] { [] }

    func doctor() async -> Diagnosis {
        doctorAsks += 1
        if let busy { wasBusy.append(await busy()) }
        return Diagnosis(checks: checks, healthy: true)
    }
    func log(limit: UInt32) async -> [Change] {
        logLimits.append(limit)
        return Array(history.suffix(Int(limit)))
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

    func status(fresh: Bool) async throws -> Status {
        Status(now: 0, accounts: [], warnings: [])
    }
    func statusOffline() async throws -> Status { Status(now: 0, accounts: [], warnings: []) }
    func switchTo(_ label: String) async throws -> Switched { throw unasked() }
    func enrollCurrent(_ label: String) async throws -> Enrolled { throw unasked() }
    func forget(_ label: String) async throws -> Changed { throw unasked() }
    func rename(_ from: String, to: String) async throws -> Changed { throw unasked() }
    func signIn(_ label: String) async throws -> SignIn { throw unasked() }
    func abandonRecovery() async throws -> Abandoned? { nil }
    func changedAt() async -> Int64 { 0 }
    func readingsChangedAt() async -> Int64 { 0 }
    func tools() -> [Tool] { bothTools }
    func installed() async -> [Tool] { [] }

    private func unasked() -> PitboardError {
        .Failed(
            code: "unasked", cause: nil, message: "not a question about this Mac", warnings: [])
    }
}

/// Where the scheduler keeps the renewal job, as the core names it. Never written.
private let plist = "/Users/x/Library/LaunchAgents/com.usepitboard.renew.plist"

/// A login item that answers the way macOS can: a registration may wait for approval or be
/// refused, and the person may change it in System Settings while the app is running.
@MainActor
private final class ScriptedLoginItem: LoginItem {
    var state: LoginItemState
    /// Whether a registration waits for the person to allow it in System Settings.
    var needsApproval = false
    /// What the next register or unregister is refused with, if anything.
    var refusing: Error?
    private(set) var settingsOpened = 0

    init(_ state: LoginItemState = .disabled) {
        self.state = state
    }

    func register() throws {
        if let refusing { throw refusing }
        state = needsApproval ? .requiresApproval : .enabled
    }
    func unregister() throws {
        if let refusing { throw refusing }
        state = .disabled
    }
    func openSystemSettings() { settingsOpened += 1 }
}

private let translocated =
    "/private/var/folders/xy/abc/T/AppTranslocation/0A1B2C/d/Pitboard.app"

extension CommandLineTool {
    /// The command line inside the app at `app`, or none when `app` is nil. Where it would
    /// be linked is never looked at, and no script is run to link it.
    fileprivate static func inside(_ app: String?) -> CommandLineTool {
        CommandLineTool(
            helper: app.map { "\($0)/Contents/Helpers/pitboard" }, installPlaces: [],
            link: nowhere.link,
            execute: { _ in [NSAppleScript.errorMessage: "No script is run in a test."] })
    }
}

extension MachineModel {
    /// The machine's model over `service`, with a login item that registers nothing and no
    /// command line inside the app unless a test gives it one. A test that needs one makes a
    /// `StandInApp`: a path to an app it did not make would pass only on a Mac that has
    /// Pitboard installed there.
    fileprivate convenience init(
        testing service: any Core, commandLineTool: CommandLineTool = .nowhere,
        loginItem: any LoginItem = ScriptedLoginItem()
    ) {
        self.init(service: service, commandLineTool: commandLineTool, loginItem: loginItem)
    }
}

/// A stand-in app in a temporary directory with a command line inside it, and a `bin` for
/// the link. Removed by `remove()`.
private struct StandInApp {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboard-machine-\(UUID().uuidString)")
    var helper: String {
        root.appendingPathComponent("Pitboard.app/Contents/Helpers/pitboard").path
    }
    var bin: String { root.appendingPathComponent("bin").path }
    var link: String { "\(bin)/pitboard" }

    init() throws {
        try FileManager.default.createDirectory(
            atPath: bin, withIntermediateDirectories: true)
        try program(at: helper)
    }

    /// This app's command line, looked for in `bin` alone, linked by running `execute`.
    func tool(_ execute: @escaping CommandLineTool.Runner) -> CommandLineTool {
        CommandLineTool(helper: helper, installPlaces: [bin], link: link, execute: execute)
    }

    /// What the script run as an administrator does.
    func makeLink() {
        try? FileManager.default.createSymbolicLink(
            atPath: link, withDestinationPath: helper)
    }

    /// A `pitboard` that is not this app's, in a directory of its own under the root.
    func another(in directory: String) throws -> String {
        let made = root.appendingPathComponent(directory).path
        try program(at: "\(made)/pitboard")
        return made
    }

    func remove() { try? FileManager.default.removeItem(at: root) }

    private func program(at path: String) throws {
        try FileManager.default.createDirectory(
            atPath: (path as NSString).deletingLastPathComponent,
            withIntermediateDirectories: true)
        try Data("#!/bin/sh\n".utf8).write(to: URL(fileURLWithPath: path))
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path)
    }
}

// MARK: - Opening at login

/// The switch shows what macOS has, not what was last asked for, and the person can change
/// it in System Settings at any time, so it is read again when the settings ask.
@MainActor
@Test func openingAtLoginIsWhatMacOSSays() {
    let item = ScriptedLoginItem(.enabled)
    let machine = MachineModel(testing: MachineStub(), loginItem: item)
    #expect(machine.openAtLogin == .enabled)

    machine.setOpenAtLogin(false)
    #expect(machine.openAtLogin == .disabled)
    machine.setOpenAtLogin(true)
    #expect(machine.openAtLogin == .enabled)
    #expect(machine.loginItemFailed == nil)

    item.state = .disabled
    #expect(machine.openAtLogin == .enabled, "not read again until something asks")
    machine.readLoginItem()
    #expect(machine.openAtLogin == .disabled)
}

/// macOS can hold a new login item until the person allows it, and until then the app does
/// not open at login. That is not a failure: the settings say it is waiting and offer
/// System Settings, where it is allowed.
@MainActor
@Test func aLoginItemWaitingForApprovalSaysSo() {
    let item = ScriptedLoginItem()
    item.needsApproval = true
    let machine = MachineModel(testing: MachineStub(), loginItem: item)

    machine.setOpenAtLogin(true)
    #expect(machine.openAtLogin == .requiresApproval)
    #expect(machine.loginItemFailed == nil)
    machine.openLoginItemSettings()
    #expect(item.settingsOpened == 1)

    item.state = .enabled
    machine.readLoginItem()
    #expect(machine.openAtLogin == .enabled)
}

/// A refused change says why beside the switch, and the switch shows what macOS has rather
/// than what was asked for. The next change that works puts the reason away.
@MainActor
@Test func aRefusedLoginItemSaysWhyAndShowsWhatMacOSHas() {
    let item = ScriptedLoginItem()
    item.refusing = NSError(
        domain: "LoginItems", code: 1,
        userInfo: [NSLocalizedDescriptionKey: "Operation not permitted"])
    let machine = MachineModel(testing: MachineStub(), loginItem: item)

    machine.setOpenAtLogin(true)
    #expect(machine.loginItemFailed == "Operation not permitted")
    #expect(machine.openAtLogin == .disabled)

    item.state = .enabled
    machine.setOpenAtLogin(false)
    #expect(machine.loginItemFailed == "Operation not permitted")
    #expect(machine.openAtLogin == .enabled)

    item.refusing = nil
    machine.setOpenAtLogin(false)
    #expect(machine.loginItemFailed == nil)
    #expect(machine.openAtLogin == .disabled)
}

// MARK: - The renewal schedule

/// Daily renewal reads as on only while the scheduler has the job. A scheduler this Mac
/// does not have is not a schedule that is on. Nothing is read until the settings ask.
@MainActor
@Test func dailyRenewalIsOnOnlyWhileTheSchedulerHasIt() async {
    let core = MachineStub()
    let machine = MachineModel(testing: core)
    #expect(machine.schedule == .absent)
    #expect(core.scheduleReads == 0)

    let answers: [(Schedule, Bool)] = [
        (.installed(path: plist, everySeconds: 86_400), true), (.absent, false),
        (.unsupported, false),
    ]
    for (scheduled, on) in answers {
        core.scheduled = scheduled
        await machine.readSchedule()
        #expect(machine.schedule == scheduled)
        #expect(machine.renewsDaily == on)
    }
}

/// Turning daily renewal on or off changes the scheduler, and the switch then shows what
/// the scheduler has, read again rather than assumed.
@MainActor
@Test func turningDailyRenewalOnAndOffChangesTheScheduler() async throws {
    let app = try StandInApp()
    defer { app.remove() }
    let core = MachineStub()
    let machine = MachineModel(testing: core, commandLineTool: app.tool(Scripts().run))

    await machine.setSchedule(on: true)
    #expect(core.installs == 1)
    #expect(core.scheduleReads == 1)
    #expect(machine.schedule == .installed(path: plist, everySeconds: 86_400))
    #expect(machine.renewsDaily)
    #expect(machine.scheduleFailed == nil)

    await machine.setSchedule(on: false)
    #expect(core.uninstalls == 1)
    #expect(core.scheduleReads == 2)
    #expect(machine.schedule == .absent)
    #expect(!machine.renewsDaily)
}

/// The schedule runs the command line inside the app long after the app has quit. A copy
/// macOS runs from a temporary place is gone by then, and one with no command line inside
/// it has nothing to schedule, so turning renewal on there is refused with the reason and
/// nothing is asked of the scheduler. Turning it off never is: that is how a schedule that
/// cannot work is taken away.
@MainActor
@Test func dailyRenewalIsRefusedWhereNothingWouldBeThereToRun() async throws {
    let app = try StandInApp()
    defer { app.remove() }
    #expect(
        MachineModel(testing: MachineStub(), commandLineTool: app.tool(Scripts().run))
            .cannotSchedule == nil)

    let reasons: [(String?, String)] = [
        (
            translocated,
            "Move Pitboard to your Applications folder first. Until then macOS runs it from "
                + "a temporary copy, which is gone once Pitboard quits."
        ),
        (nil, "This copy of Pitboard has no command line inside it to run on a schedule."),
    ]
    for (app, reason) in reasons {
        let core = MachineStub()
        core.scheduled = .installed(path: plist, everySeconds: 86_400)
        let machine = MachineModel(testing: core, commandLineTool: .inside(app))
        #expect(machine.cannotSchedule == reason)

        await machine.setSchedule(on: true)
        #expect(machine.scheduleFailed == reason)
        #expect(core.installs == 0)
        #expect(core.scheduleReads == 0)

        await machine.setSchedule(on: false)
        #expect(machine.scheduleFailed == nil)
        #expect(core.uninstalls == 1)
        #expect(machine.schedule == .absent)
    }
}

/// A build run from Xcode is an app with no command line inside it. A link to where one would
/// be runs nothing, and a schedule of it renews nothing, so neither is offered, and nobody is
/// asked for an administrator's password to make one. A command line inside the app that
/// nobody can run is the same as none.
@MainActor
@Test func anAppWithoutACommandLineItCanRunCannotLinkOrSchedule() async throws {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboard-helperless-\(UUID().uuidString)")
    defer { try? FileManager.default.removeItem(at: root) }
    let app = root.appendingPathComponent("Pitboard.app")
    try FileManager.default.createDirectory(
        at: app.appendingPathComponent("Contents/MacOS"), withIntermediateDirectories: true)
    let scripts = Scripts()
    let tool = CommandLineTool(
        helper: Settings.bundledCommandLine(in: app), installPlaces: [],
        link: root.appendingPathComponent("bin/pitboard").path, execute: scripts.run)
    let helper = app.appendingPathComponent("Contents/Helpers/pitboard")
    #expect(tool.helper == helper.path, "where the app's own would be")
    #expect(!tool.translocated)
    #expect(!tool.linkable)

    let core = MachineStub()
    let machine = MachineModel(testing: core, commandLineTool: tool)
    let none = "This copy of Pitboard has no command line inside it to run on a schedule."
    #expect(machine.cannotSchedule == none)
    await machine.setSchedule(on: true)
    #expect(machine.scheduleFailed == none)
    #expect(core.installs == 0)
    await machine.installCommandLine()
    #expect(
        machine.linkFailed == "This copy of Pitboard cannot link the command line inside it.")
    #expect(scripts.ran.isEmpty)

    try FileManager.default.createDirectory(
        at: helper.deletingLastPathComponent(), withIntermediateDirectories: true)
    try Data("#!/bin/sh\n".utf8).write(to: helper)
    try FileManager.default.setAttributes([.posixPermissions: 0o644], ofItemAtPath: helper.path)
    #expect(!tool.linkable, "there, and nobody can run it")
    #expect(machine.cannotSchedule == none)

    try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: helper.path)
    #expect(tool.linkable)
    #expect(machine.cannotSchedule == nil)
}

/// The switch shows the state asked for while the scheduler is being changed, rather than
/// snapping back to the old one until it answers, which can be a while behind a renewal or a
/// switch. Pressed again meanwhile it does nothing: it was pressed on a switch that did not
/// yet show the first press.
@MainActor
@Test func dailyRenewalShowsWhatWasAskedForWhileItChanges() async throws {
    let app = try StandInApp()
    defer { app.remove() }
    let core = MachineStub()
    let machine = MachineModel(testing: core, commandLineTool: app.tool(Scripts().run))
    #expect(machine.scheduling == nil)

    core.busy = { machine.scheduling == true }
    core.meanwhile = {
        core.meanwhile = nil
        await machine.setSchedule(on: false)
    }
    await machine.setSchedule(on: true)
    #expect(core.wasBusy == [true])
    #expect(core.installs == 1)
    #expect(core.uninstalls == 0, "the press made meanwhile")
    #expect(machine.scheduling == nil)
    #expect(machine.renewsDaily)

    core.busy = { machine.scheduling == false }
    core.meanwhile = {
        core.meanwhile = nil
        await machine.setSchedule(on: true)
    }
    await machine.setSchedule(on: false)
    #expect(core.wasBusy == [true, true])
    #expect(core.uninstalls == 1)
    #expect(core.installs == 1, "the press made meanwhile")
    #expect(machine.scheduling == nil)
    #expect(!machine.renewsDaily)
}

/// When the scheduler refuses, the core's own words are said beside the switch, and the
/// switch shows the schedule as it is. The next try starts from nothing said.
@MainActor
@Test func aScheduleTheCoreCouldNotChangeSaysWhyUntilTheNextTry() async throws {
    let app = try StandInApp()
    defer { app.remove() }
    let core = MachineStub()
    core.refusing = PitboardError.Failed(
        code: "schedule_refused", cause: nil,
        message: "launchctl refused the renewal job: Input/output error", warnings: [])
    let machine = MachineModel(testing: core, commandLineTool: app.tool(Scripts().run))

    await machine.setSchedule(on: true)
    #expect(machine.scheduleFailed == "launchctl refused the renewal job: Input/output error")
    #expect(core.scheduleReads == 1)
    #expect(!machine.renewsDaily)

    core.refusing = nil
    await machine.setSchedule(on: true)
    #expect(machine.scheduleFailed == nil)
    #expect(machine.renewsDaily)
}

/// An app up to 0.3.0 scheduled itself, which renews nothing. The repair points that job at
/// the command line inside this app, and the schedule is read again only when it did.
/// Nothing repaired, or a repair that failed, says nothing: the schedule is as it was, and
/// doctor still reports it.
@MainActor
@Test func anOlderAppsScheduleIsReadAgainOnlyOnceItWasRepaired() async {
    let refused = PitboardError.Failed(
        code: "schedule_refused", cause: nil, message: "the scheduler refused", warnings: [])
    let answers: [(Result<Bool, Error>, Int)] = [
        (.success(true), 1), (.success(false), 0), (.failure(refused), 0),
    ]
    for (answer, reads) in answers {
        let core = MachineStub()
        core.scheduled = .installed(path: plist, everySeconds: 86_400)
        core.repairs = answer
        let machine = MachineModel(testing: core)

        await machine.repairSchedule()
        #expect(core.repairAsks == 1)
        #expect(core.scheduleReads == reads)
        #expect(machine.renewsDaily == (reads == 1))
        #expect(machine.scheduleFailed == nil)
    }
}

/// Renewing now says what each parked login came to, shows it is running while it runs,
/// and has the accounts read again once, with what it renewed.
@MainActor
@Test func renewingNowSaysWhatItRenewedAndReadsTheAccountsOnce() async {
    let core = MachineStub()
    core.renewals = [
        Renewed(label: "work", provider: "claude", outcome: "renewed"),
        Renewed(label: "codex/spare", provider: "codex", outcome: "renewal_deferred"),
    ]
    let machine = MachineModel(testing: core)
    core.busy = { machine.renewing }
    var reads = 0
    machine.renewed = { reads += 1 }
    #expect(machine.renewals == nil)

    await machine.renewNow()
    #expect(core.renewAsks == 1)
    #expect(core.wasBusy == [true])
    #expect(!machine.renewing)
    #expect(machine.renewals == core.renewals)
    #expect(reads == 1)
}

// MARK: - The command line

/// A terminal runs the first `pitboard` on its login shell's `PATH`, then looks where each
/// way of installing Pitboard puts one, so that is the order it is looked for in here. It
/// is looked for only when the settings ask.
@MainActor
@Test func theCommandLineIsLookedForWhereATerminalWouldFindIt() async throws {
    let app = try StandInApp()
    defer { app.remove() }
    let core = MachineStub()
    let machine = MachineModel(testing: core, commandLineTool: app.tool(Scripts().run))
    #expect(machine.commandLine == nil)

    await machine.findCommandLine()
    #expect(machine.commandLine == .nowhere)
    app.makeLink()
    await machine.findCommandLine()
    #expect(machine.commandLine == .bundled(app.link))

    let cargo = try app.another(in: "cargo/bin")
    core.path = "/nowhere/bin:\(cargo)"
    await machine.findCommandLine()
    #expect(machine.commandLine == .another("\(cargo)/pitboard"))
}

/// The button says it is linking for as long as macOS's password prompt is up, and once
/// the link is made the settings show this app's own command line on the `PATH`.
@MainActor
@Test func linkingSaysSoUntilThePasswordPromptIsAnswered() async throws {
    let app = try StandInApp()
    defer { app.remove() }
    let (prompted, prompting) = AsyncStream.makeStream(of: Void.self)
    let answered = DispatchSemaphore(value: 0)
    let machine = MachineModel(
        testing: MachineStub(),
        commandLineTool: app.tool { _ in
            prompting.yield()
            answered.wait()
            app.makeLink()
            return nil
        })

    let linking = Task { await machine.installCommandLine() }
    for await _ in prompted { break }
    #expect(machine.linking)
    answered.signal()
    await linking.value
    #expect(!machine.linking)
    #expect(machine.linkFailed == nil)
    #expect(machine.commandLine == .bundled(app.link))
}

/// Why a link could not be made is said beside the button, in AppleScript's words. A
/// dismissed password prompt is somebody's answer, so it says nothing and puts away what
/// the last try said. A copy with nothing to link says so without asking for a password.
@MainActor
@Test func aLinkThatFailedSaysWhyAndADismissedPromptSaysNothing() async throws {
    let app = try StandInApp()
    defer { app.remove() }
    let scripts = Scripts()
    let machine = MachineModel(testing: MachineStub(), commandLineTool: app.tool(scripts.run))

    scripts.raise(1, "ln: \(app.link): Permission denied")
    await machine.installCommandLine()
    #expect(machine.linkFailed == "ln: \(app.link): Permission denied")
    #expect(!machine.linking)
    #expect(machine.commandLine == .nowhere)

    scripts.raise(-128, "User canceled.")
    await machine.installCommandLine()
    #expect(machine.linkFailed == nil)
    #expect(scripts.ran.count == 2)

    let unlinkable = MachineModel(
        testing: MachineStub(),
        commandLineTool: CommandLineTool(
            helper: nil, installPlaces: [app.bin], link: app.link, execute: scripts.run))
    await unlinkable.installCommandLine()
    #expect(
        unlinkable.linkFailed == "This copy of Pitboard cannot link the command line inside it."
    )
    #expect(scripts.ran.count == 2)
}

// MARK: - Checks and changes

/// Doctor's checks are shown with when they were made, in place of the last ones, and the
/// pane says it is checking while doctor runs.
@MainActor
@Test func doctorsChecksAreShownWithWhenTheyWereMade() async throws {
    let core = MachineStub()
    core.checks = [
        Check(
            code: "keychain", name: "Keychain", level: .ok, detail: "login keychain, unlocked",
            advice: ""),
        Check(
            code: "schedule", name: "Daily renewal", level: .warn, detail: "not scheduled",
            advice: "Turn on daily renewal in Pitboard's settings."),
    ]
    let machine = MachineModel(testing: core)
    core.busy = { machine.checking }
    #expect(machine.checks.isEmpty)
    #expect(machine.checkedAt == nil)
    #expect(core.doctorAsks == 0)

    let asked = Date()
    await machine.diagnose()
    #expect(core.wasBusy == [true])
    #expect(!machine.checking)
    #expect(machine.checks == core.checks)
    let checkedAt = try #require(machine.checkedAt)
    #expect(checkedAt >= asked && checkedAt <= Date())

    core.checks = [
        Check(
            code: "keychain", name: "Keychain", level: .fail, detail: "locked",
            advice: "Unlock the login keychain.")
    ]
    await machine.diagnose()
    #expect(machine.checks == core.checks)
}

/// The core's log is kept newest last, and the activity pane lists what changed last
/// first. As many as the pane asks for, and the newest of them.
@MainActor
@Test func changesAreShownNewestFirst() async {
    let core = MachineStub()
    core.history = [
        Change(
            at: "2026-09-24T09:00:00Z", caller: "cli", verb: "enroll", subject: "work",
            outcome: "ok"),
        Change(
            at: "2026-09-26T09:00:00Z", caller: "app", verb: "switch", subject: "personal",
            outcome: "ok"),
        Change(
            at: "2026-09-27T09:00:00Z", caller: "app", verb: "switch", subject: "codex/spare",
            outcome: "nothing_parked"),
    ]
    let machine = MachineModel(testing: core)
    #expect(machine.changes.isEmpty)

    await machine.readChanges()
    #expect(machine.changes == core.history.reversed())
    await machine.readChanges(2)
    #expect(machine.changes == [core.history[2], core.history[1]])
    #expect(core.logLimits == [500, 2])
}
