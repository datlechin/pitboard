import Foundation
import PitboardKit
import Testing

/// A scratch home whose Claude Code and Codex directories are its own, so the credential
/// slot read is hashed from it and never the machine's real login, and the Codex login read
/// is a file that is not there.
private func scratch(codex: String? = nil) throws -> Settings {
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
        codexProgram: codex
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
@Test func onlyToolsWithAProgramAreInstalled() throws {
    let neither = PitboardService(settings: try scratch())
    #expect(neither.tools().map(\.code) == ["claude", "codex"])
    #expect(neither.installed().isEmpty)
    let codex = PitboardService(settings: try scratch(codex: "/nowhere/codex"))
    #expect(codex.installed().map(\.code) == ["codex"])
}

@Test func doctorReportsEveryCheck() async throws {
    let diagnosis = await PitboardService(settings: try scratch()).doctor()
    #expect(!diagnosis.checks.isEmpty)
    #expect(diagnosis.checks.contains { $0.code == "state" })
}
