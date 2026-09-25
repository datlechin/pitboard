import Foundation
import Testing

@testable import PitboardKit

private let marker = "pitboard-path-test"

/// Only what stands between the markers is the answer. Startup files print greetings,
/// warnings and prompts of their own, sometimes on the same line as the marker.
@Test func thePathIsWhatStandsBetweenTheMarkers() {
    let said =
        "Last login: never\nnvm is loaded\(marker)\n/Users/x/.nvm/bin:/usr/bin:/bin\n"
        + "\(marker)\nbye\n"
    #expect(LoginShell.path(in: said, marker: marker) == "/Users/x/.nvm/bin:/usr/bin:/bin")
    #expect(
        LoginShell.path(in: "\(marker)\r\n/usr/bin\r\n\(marker)\r\n", marker: marker)
            == "/usr/bin")
}

/// A shell that stopped early, or said nothing between them, has not answered.
@Test func withoutBothMarkersThereIsNoAnswer() {
    #expect(LoginShell.path(in: "", marker: marker) == nil)
    #expect(LoginShell.path(in: "/usr/bin:/bin\n", marker: marker) == nil)
    #expect(LoginShell.path(in: "\(marker)\n/usr/bin:/bin\n", marker: marker) == nil)
    #expect(LoginShell.path(in: "\(marker)\n\n\(marker)\n", marker: marker) == nil)
}

/// The shell is asked as a login shell and an interactive one, since zsh reads `.zshrc`, where
/// version managers put themselves, only for an interactive shell; and `PATH` is printed by
/// `printenv` between the markers, which all of zsh, bash and fish run as written.
@Test func theLoginShellIsAskedForItsPath() {
    var asked: (shell: String, arguments: [String])?
    let fish = ["SHELL": "/opt/homebrew/bin/fish"]
    let answer = LoginShell.path(environment: fish, marker: marker) { shell, arguments in
        asked = (shell, arguments)
        return .said("Welcome to fish\n\(marker)\n/opt/homebrew/bin:/usr/bin\n\(marker)\n")
    }
    #expect(answer == LoginShell.Answer(path: "/opt/homebrew/bin:/usr/bin"))
    #expect(asked?.shell == "/opt/homebrew/bin/fish")
    #expect(
        asked?.arguments
            == ["-l", "-i", "-c", "echo \(marker); /usr/bin/printenv PATH; echo \(marker)"])
}

/// An app is normally given `SHELL`; when it is not, the account's own shell is asked.
@Test func withoutShellTheAccountsOwnIsAsked() {
    var asked: String?
    _ = LoginShell.path(environment: [:], marker: marker) { shell, _ in
        asked = shell
        return .failed
    }
    #expect(asked?.hasPrefix("/") == true, "\(asked ?? "nothing")")
}

/// A shell that could not be run has no answer, whatever it might have printed, and asking
/// again would get the same.
@Test func aShellThatCouldNotBeRunHasNoAnswer() {
    let zsh = ["SHELL": "/bin/zsh"]
    #expect(LoginShell.path(environment: zsh, marker: marker) { _, _ in .failed } == .init())
}

/// A shell too slow to answer has no answer yet, and says so, since a later ask may have one.
@Test func aShellTooSlowToAnswerSaysItWasLate() {
    let zsh = ["SHELL": "/bin/zsh"]
    #expect(
        LoginShell.path(environment: zsh, marker: marker) { _, _ in .late }
            == LoginShell.Answer(path: nil, late: true))
}

/// A shell of the test's own, never the person's, which runs what it is asked with a plain
/// `/bin/sh` that reads no startup files, after `before`. `before` runs in a directory of
/// the shell's own, which is taken away with it.
private struct FakeShell {
    let path: String
    let dir: URL

    init(_ before: String) throws {
        dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("pitboard-shell-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        path = dir.appendingPathComponent("shell").path
        let script = """
            #!/bin/sh
            [ "$1 $2 $3" = "-l -i -c" ] || exit 64
            cd "$(/usr/bin/dirname "$0")" || exit 65
            \(before)
            exec /bin/sh -c "$4"
            """
        try script.write(toFile: path, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: path)
    }

    /// The processes `before` wrote down in `pids`, one per line.
    func pids() -> [pid_t] {
        let written =
            (try? String(contentsOf: dir.appendingPathComponent("pids"), encoding: .utf8))
        return (written ?? "").split(separator: "\n").compactMap { pid_t($0) }
    }

    func remove() {
        try? FileManager.default.removeItem(at: dir)
    }
}

/// Whether `pid` is gone, asked for a while, since a process killed a moment ago can take a
/// moment to be reaped.
private func gone(_ pid: pid_t) async -> Bool {
    for _ in 0..<200 {
        if kill(pid, 0) == -1, errno == ESRCH { return true }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return false
}

/// The whole round: the shell is started with the environment given, prints `PATH` between
/// the markers after whatever its startup files say, and the answer is read back.
@Test func aShellIsRunAndItsPathReadBack() throws {
    let shell = try FakeShell("echo 'Last login: never'")
    defer { shell.remove() }
    let answer = LoginShell.path(environment: [
        "SHELL": shell.path, "PATH": "/from/the/test/bin:/usr/bin:/bin",
    ])
    #expect(answer == LoginShell.Answer(path: "/from/the/test/bin:/usr/bin:/bin"))
}

/// A shell that fails has no answer.
@Test func aShellThatFailsHasNoAnswer() throws {
    let failing = try FakeShell("exit 3")
    defer { failing.remove() }
    #expect(
        LoginShell.run(
            failing.path, LoginShell.arguments(marker: marker), environment: [:], within: 5)
            == .failed)
}

/// A shell that never finishes is stopped rather than waited for, and so is everything its
/// startup files started, which would otherwise go on running for as long as it likes.
@Test func aShellThatHangsIsStoppedWithEverythingItStarted() async throws {
    let hanging = try FakeShell("echo $$ > pids; /bin/sleep 30 & echo $! >> pids; wait")
    defer { hanging.remove() }
    let started = Date()
    #expect(
        LoginShell.run(
            hanging.path, LoginShell.arguments(marker: marker), environment: [:], within: 2)
            == .late)
    #expect(Date().timeIntervalSince(started) < 5, "it was stopped, not waited for")

    let pids = hanging.pids()
    #expect(pids.count == 2, "the shell and what it started, both started before the end")
    for pid in pids {
        #expect(await gone(pid), "\(pid) is still running")
    }
}

/// Something a startup file leaves running can keep the shell's output open long after the
/// shell is done. The answer is taken once the shell exits, not when the last holder lets go,
/// which here is well after the shell's time is up.
@Test func aShellIsDoneWhenItExitsEvenIfSomethingItStartedIsNot() throws {
    let shell = try FakeShell("/bin/sleep 10 & echo $! > pids")
    defer {
        for pid in shell.pids() { kill(pid, SIGKILL) }
        shell.remove()
    }
    let answer = LoginShell.path(environment: ["SHELL": shell.path, "PATH": "/usr/bin:/bin"])
    #expect(answer == LoginShell.Answer(path: "/usr/bin:/bin"))
}

/// A program is found where the variable naming it outright says, then on the login shell's
/// `PATH`, where a version manager or an npm prefix puts it, then where its installer does.
@Test func aProgramIsFoundByItsVariableThenTheLoginShellThenItsInstaller() {
    let home = ["HOME": "/Users/x"]
    let here: Set<String> = [
        "/Users/x/.nvm/versions/node/v22.1.0/bin/codex",
        "/Users/x/.volta/bin/claude",
        "/opt/homebrew/bin/codex",
        "/opt/homebrew/bin/claude",
    ]
    let login = "/usr/bin:bin:/Users/x/.volta/bin:/Users/x/.nvm/versions/node/v22.1.0/bin"

    let shell = Settings.forCurrentUser(
        environment: home, loginPath: login, bundle: nil, isExecutable: here.contains)
    #expect(shell.codexProgram == "/Users/x/.nvm/versions/node/v22.1.0/bin/codex")
    #expect(shell.claudeProgram == "/Users/x/.volta/bin/claude")
    #expect(
        shell.searchPath
            == "/usr/bin:/Users/x/.volta/bin:/Users/x/.nvm/versions/node/v22.1.0/bin",
        "the core looks where the app looks, and a sign-in is given it")

    let named = Settings.forCurrentUser(
        environment: home.merging(["PITBOARD_CODEX": "/elsewhere/codex"]) { $1 },
        loginPath: login, bundle: nil, isExecutable: here.contains)
    #expect(named.codexProgram == "/elsewhere/codex")
    #expect(named.claudeProgram == "/Users/x/.volta/bin/claude")

    let unknown = Settings.forCurrentUser(
        environment: home, loginPath: nil, bundle: nil, isExecutable: here.contains)
    #expect(unknown.codexProgram == "/opt/homebrew/bin/codex")
    #expect(unknown.claudeProgram == "/opt/homebrew/bin/claude")
    #expect(unknown.searchPath == nil, "the core looks where it always did")

    let nowhere = Settings.forCurrentUser(
        environment: home, loginPath: login, bundle: nil, isExecutable: { _ in false })
    #expect(nowhere.codexProgram == nil)
    #expect(nowhere.claudeProgram == nil)
}

/// A relative entry on the login shell's `PATH` names a directory relative to wherever the
/// shell was, which is not where this app is, so it is not looked in.
@Test func aRelativeEntryIsNotLookedIn() {
    var looked: [String] = []
    _ = Settings.forCurrentUser(
        environment: ["HOME": "/Users/x"], loginPath: "bin:./node_modules/.bin:/usr/bin",
        bundle: nil
    ) {
        looked.append($0)
        return false
    }
    #expect(looked.allSatisfy { $0.hasPrefix("/") }, "\(looked)")
    #expect(looked.first == "/usr/bin/claude")
}

/// A folder macOS guards, such as Documents or iCloud Drive, is not looked in, and is not
/// where the core looks or what a sign-in is given: looking there asks the person whether
/// pitboard may, and a program started from there would have it asked on its behalf.
@Test func aGuardedFolderOnThePathIsNotLookedIn() {
    var looked: [String] = []
    let login = [
        "/Users/x/Documents/bin", "/Users/x/Library/Mobile Documents/com~apple~CloudDocs/bin",
        "/Users/x/Desktop", "/Users/x/Downloads/tools/bin", "/Users/x/Library/CloudStorage/bin",
        "/Users/x/Documentsbin", "/usr/bin",
    ].joined(separator: ":")
    let settings = Settings.forCurrentUser(
        environment: ["HOME": "/Users/x"], loginPath: login, bundle: nil
    ) {
        looked.append($0)
        return false
    }
    #expect(
        Array(looked.prefix(3))
            == [
                "/Users/x/Documentsbin/claude", "/usr/bin/claude", "/Users/x/.local/bin/claude",
            ],
        "\(looked)")
    #expect(looked.count == 10, "the same five places for each tool: \(looked)")
    #expect(settings.searchPath == "/Users/x/Documentsbin:/usr/bin")
}

/// The renewal schedule runs the command line inside the app, since the app has no renewal
/// of its own. Anything not running from an app bundle names none, which means it cannot
/// schedule renewal: the only other thing to schedule is the app itself.
@Test func theScheduleRunsTheCommandLineInsideTheApp() {
    func scheduled(from bundle: URL?) -> String? {
        Settings.forCurrentUser(
            environment: ["HOME": "/Users/x"], loginPath: nil, bundle: bundle,
            isExecutable: { _ in false }
        ).scheduleProgram
    }
    #expect(
        scheduled(from: URL(fileURLWithPath: "/Applications/Pitboard.app"))
            == "/Applications/Pitboard.app/Contents/Helpers/pitboard")
    #expect(
        scheduled(from: URL(fileURLWithPath: "/Users/x/My Apps/Pitboard.app"))
            == "/Users/x/My Apps/Pitboard.app/Contents/Helpers/pitboard")
    #expect(
        scheduled(from: URL(fileURLWithPath: "/Users/x/pitboard/apple/.build/debug")) == nil)
    #expect(scheduled(from: nil) == nil)
    #expect(Settings.bundledCommandLine(in: Bundle.main.bundleURL) == nil, "these tests")
}
