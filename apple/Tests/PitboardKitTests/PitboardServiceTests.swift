import Foundation
import PitboardKit
import Testing

/// A scratch home whose Claude Code and Codex directories are its own, so the credential
/// slot read is hashed from it and never the machine's real login, and the Codex login read
/// is a file that is not there.
private func scratch(codex: String? = nil, schedules: String? = nil) throws -> Settings {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboardkit-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    return Settings(
        home: root.path,
        pitboardHome: root.appendingPathComponent("pitboard").path,
        claudeConfigDir: root.appendingPathComponent("claude").path,
        secureStorageDir: nil,
        user: NSUserName(),
        claudeProgram: nil,
        codexHome: root.appendingPathComponent("codex").path,
        codexProgram: codex,
        scheduleProgram: schedules
    )
}

@Test func statusOfAnEmptyHomeHasNoAccounts() async throws {
    let status = try await PitboardService(settings: try scratch()).status(fresh: false)
    #expect(status.accounts.isEmpty)
    #expect(status.warnings.isEmpty)
}

@Test func aFailedChangeCarriesItsStableCode() async throws {
    let service = PitboardService(settings: try scratch())
    do {
        _ = try await service.rename("nobody", to: "somebody")
        Issue.record("renaming an account that does not exist must fail")
    } catch let PitboardError.Failed(code, cause, message, _) {
        #expect(code == "account_unknown")
        #expect(message.contains("nobody"))
        #expect(cause == nil, "nothing was asked of Anthropic, so nothing went wrong there")
    }
}

/// A tool is offered only where its program was found, and a program named outright
/// counts as found: that is what `PITBOARD_CLAUDE` and `PITBOARD_CODEX` are for.
@Test func onlyToolsWithAProgramAreInstalled() async throws {
    let neither = PitboardService(settings: try scratch())
    #expect(neither.tools().map(\.code) == ["claude", "codex"])
    #expect(await neither.installed().isEmpty)
    let codex = PitboardService(settings: try scratch(codex: "/nowhere/codex"))
    #expect(await codex.installed().map(\.code) == ["codex"])
}

/// Counts how often the settings were asked for, and whether any ask was on the main
/// thread.
private final class Asks: @unchecked Sendable {
    // Unchecked because the counts are written from the service's queues; every read and
    // write holds `lock`.
    private let lock = NSLock()
    private var asked = 0
    private var onMain = false

    func note() {
        let main = Thread.isMainThread
        lock.withLock {
            asked += 1
            onMain = onMain || main
        }
    }

    var count: Int { lock.withLock { asked } }
    var anyOnMain: Bool { lock.withLock { onMain } }
}

/// Where the tools are can take asking the person's login shell, so the settings are asked
/// for when something first needs the core, off the main thread, and once however many
/// calls arrive at the same time.
@Test func theSettingsAreAskedForOnceOffTheMainThread() async throws {
    let settings = try scratch(codex: "/nowhere/codex")
    let asks = Asks()
    let service = PitboardService {
        asks.note()
        return settings
    }
    #expect(service.tools().count == 2, "listing the tools asks nothing")
    #expect(asks.count == 0)
    async let installed = service.installed()
    async let changed = service.changedAt()
    async let diagnosis = service.doctor()
    let (found, _, _) = await (installed, changed, diagnosis)
    #expect(found.map(\.code) == ["codex"])
    #expect(asks.count == 1)
    #expect(!asks.anyOnMain)
}

/// A login shell too slow to answer, which startup files are while the machine is busy
/// logging in, is asked once more when something next asks what is installed, a while later,
/// and what it answers then is what is used. Once more and no more: a shell that is always
/// that slow would otherwise cost its patience on every ask.
@Test func aLoginShellTooSlowToAnswerIsAskedOnceMoreLater() async throws {
    let (slow, answered) = (try scratch(), try scratch(codex: "/nowhere/codex"))
    let asks = Asks()
    let service = PitboardService(
        asking: {
            asks.note()
            return asks.count == 1 ? (slow, true) : (answered, false)
        }, askAgainAfter: 0.2)

    #expect(await service.installed().isEmpty, "what the first, late, ask found")
    #expect(await service.installed().isEmpty, "and nothing more until the while is up")
    #expect(asks.count == 1)
    try await Task.sleep(for: .milliseconds(300))
    #expect(await service.installed().map(\.code) == ["codex"])
    #expect(asks.count == 2)
    #expect(await service.installed().map(\.code) == ["codex"])
    #expect(asks.count == 2, "asked once more, and no more")
}

/// Not before the while is up, and never when the shell answered or could not be asked.
@Test func aLoginShellIsNotAskedAgainSoonerOrForNothing() async throws {
    let settings = try scratch()
    let soon = Asks()
    let early = PitboardService(
        asking: {
            soon.note()
            return (settings, true)
        }, askAgainAfter: 3600)
    _ = await early.installed()
    _ = await early.installed()
    #expect(soon.count == 1)

    let answered = Asks()
    let service = PitboardService(
        asking: {
            answered.note()
            return (settings, false)
        }, askAgainAfter: 0)
    _ = await service.installed()
    _ = await service.installed()
    #expect(answered.count == 1)
}

@Test func doctorReportsEveryCheck() async throws {
    let diagnosis = await PitboardService(settings: try scratch()).doctor()
    #expect(!diagnosis.checks.isEmpty)
    #expect(diagnosis.checks.contains { $0.code == "state" })
}

/// The app asks for a repair every time it starts, so where there is nothing to repair it
/// answers that and changes nothing. A scratch home has no schedule of its own.
@Test func aHomeWithNoScheduleHasNothingToRepair() async throws {
    let bundled = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboardkit-helper-\(UUID().uuidString)")
    try Data("#!/bin/sh\n".utf8).write(to: bundled)
    defer { try? FileManager.default.removeItem(at: bundled) }
    for program in [nil, bundled.path] {
        let service = PitboardService(settings: try scratch(schedules: program))
        #expect(try await service.scheduleRepair() == false)
    }
}
