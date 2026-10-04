import Foundation
import PitboardKit
import Testing

/// A scratch home whose Claude Code and Codex directories are its own, so the credential
/// slot read is hashed from it and never the machine's real login, and the Codex login read
/// is a file that is not there. The test that makes one removes it, so no run leaves homes
/// behind in the temporary directory.
private struct ScratchHome {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboardkit-\(UUID().uuidString)")

    init() throws {
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    /// A Claude app of its own, which is not there unless `installClaudeDesktop` makes one,
    /// so nothing looks in /Applications.
    var desktopApp: String { root.appendingPathComponent("Claude.app").path }

    /// Claude Desktop's data and app are this home's own.
    func settings(codex: String? = nil, schedules: String? = nil) -> Settings {
        Settings(
            home: root.path,
            pitboardHome: root.appendingPathComponent("pitboard").path,
            claudeConfigDir: root.appendingPathComponent("claude").path,
            secureStorageDir: nil,
            user: NSUserName(),
            claudeProgram: nil,
            codexHome: root.appendingPathComponent("codex").path,
            codexProgram: codex,
            scheduleProgram: schedules,
            desktopDir: root.appendingPathComponent("claude-desktop").path,
            desktopApp: desktopApp
        )
    }

    /// What an app started with this home would have in its environment, and `extra`. Its
    /// shell is not there, so no shell of the person's is ever started for its `PATH`.
    func environment(_ extra: [String: String] = [:]) -> [String: String] {
        [
            "HOME": root.path, "PITBOARD_HOME": root.appendingPathComponent("pitboard").path,
            "CLAUDE_CONFIG_DIR": root.appendingPathComponent("claude").path,
            "CODEX_HOME": root.appendingPathComponent("codex").path, "USER": NSUserName(),
            "PATH": "/usr/bin:/bin", "SHELL": root.appendingPathComponent("no-shell").path,
            "PITBOARD_CLAUDE_DESKTOP_DIR": root.appendingPathComponent("claude-desktop").path,
            "PITBOARD_CLAUDE_DESKTOP_APP": desktopApp,
        ].merging(extra) { $1 }
    }

    /// Signs Claude Code in to a made-up login the way it is kept where there is no keychain
    /// item: `.credentials.json` in its config directory, mode 0600. Pitboard reads it after
    /// the item this home's slot names, and that item is never there.
    func signInToClaudeCode() throws {
        let directory = root.appendingPathComponent("claude")
        try FileManager.default.createDirectory(
            at: directory, withIntermediateDirectories: true)
        let expires = Int64(Date().timeIntervalSince1970 * 1000) + 8 * 3_600_000
        let login = """
            {"claudeAiOauth": {"accessToken": "access-not-a-token", \
            "refreshToken": "refresh-not-a-token", "expiresAt": \(expires), \
            "scopes": ["user:inference", "user:profile"]}}
            """
        let file = directory.appendingPathComponent(".credentials.json")
        try Data(login.utf8).write(to: file)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600], ofItemAtPath: file.path)
    }

    /// Installs a Claude app here: a bundle with its program, which is all Pitboard looks
    /// for to call it installed.
    func installClaudeDesktop() throws {
        let program = URL(fileURLWithPath: desktopApp).appendingPathComponent(
            "Contents/MacOS/Claude")
        try FileManager.default.createDirectory(
            at: program.deletingLastPathComponent(), withIntermediateDirectories: true)
        FileManager.default.createFile(
            atPath: program.path, contents: Data("#!/bin/sh\n".utf8),
            attributes: [.posixPermissions: 0o755])
    }

    func remove() { try? FileManager.default.removeItem(at: root) }
}

/// A login shell of the test's own, which answers only once the test lets it go. It writes
/// down each time it is asked, and whether it was let go or gave up waiting, which it does
/// after two seconds or so, before the core stops waiting for it.
private struct GatedShell {
    let directory: URL
    var path: String { directory.appendingPathComponent("shell").path }

    init(in root: URL) throws {
        directory = root.appendingPathComponent("gated-shell")
        try FileManager.default.createDirectory(
            at: directory, withIntermediateDirectories: true)
        let script = """
            #!/bin/sh
            [ "$1 $2 $3" = "-l -i -c" ] || exit 64
            cd "$(/usr/bin/dirname "$0")" || exit 65
            echo asked >> asks
            waited=0
            while [ ! -e open ]; do
                waited=$((waited + 1))
                [ "$waited" -gt 200 ] && { echo gave-up > outcome; exit 1; }
                /bin/sleep 0.01
            done
            echo opened > outcome
            exec /bin/sh -c "$4"
            """
        try script.write(toFile: path, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path)
    }

    /// How often it has been asked.
    var asks: Int { lines("asks").count }

    /// `opened` once let go, `gave-up` once it stopped waiting, nil before either.
    var outcome: String? { lines("outcome").first.map(String.init) }

    func open() throws { try Data().write(to: directory.appendingPathComponent("open")) }

    private func lines(_ name: String) -> [Substring] {
        let file = directory.appendingPathComponent(name)
        return ((try? String(contentsOf: file, encoding: .utf8)) ?? "").split(separator: "\n")
    }
}

/// Asking the person's login shell can take seconds, so the core asks it when something first
/// needs the core, off the main thread, and once however many calls arrive together. The
/// shell here answers only once the main thread lets it go, which a main thread busy asking
/// it never could.
@MainActor @Test func theLoginShellIsAskedOnceOffTheMainThread() async throws {
    let home = try ScratchHome()
    defer { home.remove() }
    let shell = try GatedShell(in: home.root)
    let environment = home.environment([
        "SHELL": shell.path, "PITBOARD_CLAUDE": "/nowhere/claude",
        "PITBOARD_CODEX": "/nowhere/codex",
    ])
    let service = PitboardService.forThisApp(environment: environment, bundle: home.root)
    #expect(service.tools().count == 3, "listing the tools asks nothing")
    #expect(shell.asks == 0, "and nor does making the service")

    // Waits on the main thread for the shell to be asked, then lets it go. `installed` is
    // called from the main thread too, as the app calls it, and two more calls arrive with it.
    let letGo = Task { @MainActor in
        for _ in 0..<500 where shell.asks == 0 {
            try? await Task.sleep(for: .milliseconds(10))
        }
        try? shell.open()
    }
    async let changed = service.changedAt()
    async let diagnosis = service.doctor()
    let found = await service.installed()
    _ = await (changed, diagnosis)
    await letGo.value

    #expect(shell.outcome == "opened", "the main thread let the shell go while it was asked")
    #expect(shell.asks == 1)
    #expect(found.map(\.code) == ["claude", "codex"])
    #expect(await service.searchPath() == "/usr/bin:/bin")
    #expect(shell.asks == 1, "and asked no more")
}

/// The app reads what the command line reads from the environment it was started with: a
/// custom OAuth endpoint refuses every change to a Claude Code account, a variable that signs
/// Claude Code in some other way is reported, and so is Claude Code's successor credential
/// backend switched on. The app used to pass on only the variables that move where things
/// are kept.
@Test func theAppReadsItsEnvironmentAsTheCommandLineDoes() async throws {
    let home = try ScratchHome()
    defer { home.remove() }
    let environment = home.environment([
        "CLAUDE_CODE_CUSTOM_OAUTH_URL": "https://oauth.example",
        "ANTHROPIC_API_KEY": "not-a-key", "CLAUDE_CODE_HOVER_REST": "1",
    ])
    let service = PitboardService.forThisApp(environment: environment, bundle: home.root)
    do {
        _ = try await service.enrollCurrent("work")
        Issue.record("a change to a Claude Code account must be refused")
    } catch let PitboardError.Failed(code, _, _, _) {
        #expect(code == "custom_oauth_endpoint")
    }
    let checks = await service.doctor().checks
    #expect(checks.first { $0.code == "auth_source" }?.level == .warn)
    #expect(checks.first { $0.code == "storage_v5" }?.detail != "inactive")
}

/// `PITBOARD_API_BASE` reaches the app as it reaches the command line: who the signed-in
/// login belongs to is asked at the loopback address it names, which refuses the connection,
/// and not at Anthropic. The app used to drop it, and asked Anthropic.
@Test func theAppAsksWherePitboardAPIBaseSays() async throws {
    let home = try ScratchHome()
    defer { home.remove() }
    try home.signInToClaudeCode()
    let environment = home.environment(["PITBOARD_API_BASE": "http://127.0.0.1:1"])
    let service = PitboardService.forThisApp(environment: environment, bundle: home.root)
    do {
        _ = try await service.enrollCurrent("work")
        Issue.record("a login nobody could identify was enrolled")
    } catch let PitboardError.Failed(code, cause, message, _) {
        #expect(code == "identity_unverifiable")
        #expect(cause?.code == "unreachable")
        #expect(message.contains("Connection refused"), "\(message)")
    }
}

@Test func statusOfAnEmptyHomeHasNoAccounts() async throws {
    let home = try ScratchHome()
    defer { home.remove() }
    let status = try await PitboardService(settings: home.settings()).status(fresh: false)
    #expect(status.accounts.isEmpty)
    #expect(status.warnings.isEmpty)
}

@Test func aFailedChangeCarriesItsStableCode() async throws {
    let home = try ScratchHome()
    defer { home.remove() }
    let service = PitboardService(settings: home.settings())
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
    let (bare, withCodex) = (try ScratchHome(), try ScratchHome())
    defer {
        bare.remove()
        withCodex.remove()
    }
    let neither = PitboardService(settings: bare.settings())
    #expect(neither.tools().map(\.code) == ["claude", "codex", "desktop"])
    #expect(await neither.installed().isEmpty)
    let codex = PitboardService(settings: withCodex.settings(codex: "/nowhere/codex"))
    #expect(await codex.installed().map(\.code) == ["codex"])
}

/// Claude Desktop is installed where its app has its program: the app named by
/// `PITBOARD_CLAUDE_DESKTOP_APP`, as the command line reads it, and never anywhere on a
/// `PATH` or asked of a login shell. An app without its program is not one.
@Test func claudeDesktopIsInstalledWhereItsAppIs() async throws {
    let (bare, withApp) = (try ScratchHome(), try ScratchHome())
    defer {
        bare.remove()
        withApp.remove()
    }
    try FileManager.default.createDirectory(
        atPath: bare.desktopApp, withIntermediateDirectories: true)
    try withApp.installClaudeDesktop()
    let missing = PitboardService.forThisApp(environment: bare.environment(), bundle: bare.root)
    #expect(await !missing.installed().map(\.code).contains("desktop"))
    let found = PitboardService.forThisApp(
        environment: withApp.environment(), bundle: withApp.root)
    #expect(await found.installed().map(\.code).contains("desktop"))
}

/// A Claude app named in the settings counts as found only once its program is there,
/// unlike a program named outright: a bundle that is not there is not an installed app.
@Test func aClaudeAppNamedInTheSettingsIsInstalledOnceItsProgramIs() async throws {
    let home = try ScratchHome()
    defer { home.remove() }
    #expect(await PitboardService(settings: home.settings()).installed().isEmpty)
    try home.installClaudeDesktop()
    let service = PitboardService(settings: home.settings())
    #expect(await service.installed().map(\.code) == ["desktop"])
}

@Test func doctorReportsEveryCheck() async throws {
    let home = try ScratchHome()
    defer { home.remove() }
    let diagnosis = await PitboardService(settings: home.settings()).doctor()
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
        let home = try ScratchHome()
        defer { home.remove() }
        let service = PitboardService(settings: home.settings(schedules: program))
        #expect(try await service.scheduleRepair() == false)
    }
}
