import Foundation
import PitboardKit
import Testing

/// A scratch home whose Claude Code directory is its own, so the credential slot read is
/// hashed from it and never the machine's real login.
private func scratch() throws -> Settings {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboardkit-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    return Settings(
        home: root.path,
        pitboardHome: root.appendingPathComponent("pitboard").path,
        claudeConfigDir: root.appendingPathComponent("claude").path,
        secureStorageDir: nil,
        user: NSUserName(),
        claudeProgram: nil
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

@Test func doctorReportsEveryCheck() async throws {
    let diagnosis = await PitboardService(settings: try scratch()).doctor()
    #expect(!diagnosis.checks.isEmpty)
    #expect(diagnosis.checks.contains { $0.code == "state" })
}
