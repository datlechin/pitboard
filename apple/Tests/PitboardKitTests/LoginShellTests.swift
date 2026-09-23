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
    let path = LoginShell.path(environment: fish, marker: marker) { shell, arguments in
        asked = (shell, arguments)
        return "Welcome to fish\n\(marker)\n/opt/homebrew/bin:/usr/bin\n\(marker)\n"
    }
    #expect(path == "/opt/homebrew/bin:/usr/bin")
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
        return nil
    }
    #expect(asked?.hasPrefix("/") == true, "\(asked ?? "nothing")")
}

/// A shell that could not be run has no answer, whatever it might have printed.
@Test func aShellThatCouldNotBeRunHasNoAnswer() {
    let zsh = ["SHELL": "/bin/zsh"]
    #expect(LoginShell.path(environment: zsh, marker: marker) { _, _ in nil } == nil)
}

/// A shell of the test's own, never the person's, which runs what it is asked with a plain
/// `/bin/sh` that reads no startup files, after `before`.
private func fakeShell(_ before: String) throws -> String {
    let dir = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboard-shell-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    let shell = dir.appendingPathComponent("shell").path
    let script = """
        #!/bin/sh
        [ "$1 $2 $3" = "-l -i -c" ] || exit 64
        \(before)
        exec /bin/sh -c "$4"
        """
    try script.write(toFile: shell, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: shell)
    return shell
}

/// The whole round: the shell is started with the environment given, prints `PATH` between
/// the markers after whatever its startup files say, and the answer is read back.
@Test func aShellIsRunAndItsPathReadBack() throws {
    let shell = try fakeShell("echo 'Last login: never'")
    let path = LoginShell.path(environment: [
        "SHELL": shell, "PATH": "/from/the/test/bin:/usr/bin:/bin",
    ])
    #expect(path == "/from/the/test/bin:/usr/bin:/bin")
}

/// A shell that fails, or that never finishes, has no answer, and one that never finishes
/// is stopped rather than waited for.
@Test func aShellThatFailsOrHangsHasNoAnswer() throws {
    let asked = LoginShell.arguments(marker: marker)
    let failing = try fakeShell("exit 3")
    #expect(LoginShell.run(failing, asked, environment: [:], within: 5) == nil)

    let hanging = try fakeShell("/bin/sleep 30")
    let started = Date()
    #expect(LoginShell.run(hanging, asked, environment: [:], within: 0.3) == nil)
    #expect(Date().timeIntervalSince(started) < 5, "it was stopped, not waited for")
}

/// Something a startup file leaves running can keep the shell's output open long after the
/// shell is done. The answer is taken once the shell exits, not when the last holder lets go,
/// which here is well after the shell's time is up.
@Test func aShellIsDoneWhenItExitsEvenIfSomethingItStartedIsNot() throws {
    let shell = try fakeShell("/bin/sleep 10 &")
    let path = LoginShell.path(environment: ["SHELL": shell, "PATH": "/usr/bin:/bin"])
    #expect(path == "/usr/bin:/bin")
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
        environment: home, loginPath: login, isExecutable: here.contains)
    #expect(shell.codexProgram == "/Users/x/.nvm/versions/node/v22.1.0/bin/codex")
    #expect(shell.claudeProgram == "/Users/x/.volta/bin/claude")
    #expect(shell.searchPath == login, "the core looks there too, and a sign-in is given it")

    let named = Settings.forCurrentUser(
        environment: home.merging(["PITBOARD_CODEX": "/elsewhere/codex"]) { $1 },
        loginPath: login, isExecutable: here.contains)
    #expect(named.codexProgram == "/elsewhere/codex")
    #expect(named.claudeProgram == "/Users/x/.volta/bin/claude")

    let unknown = Settings.forCurrentUser(
        environment: home, loginPath: nil, isExecutable: here.contains)
    #expect(unknown.codexProgram == "/opt/homebrew/bin/codex")
    #expect(unknown.claudeProgram == "/opt/homebrew/bin/claude")
    #expect(unknown.searchPath == nil, "the core looks where it always did")

    let nowhere = Settings.forCurrentUser(
        environment: home, loginPath: login, isExecutable: { _ in false })
    #expect(nowhere.codexProgram == nil)
    #expect(nowhere.claudeProgram == nil)
}

/// A relative entry on the login shell's `PATH` names a directory relative to wherever the
/// shell was, which is not where this app is, so it is not looked in.
@Test func aRelativeEntryIsNotLookedIn() {
    var looked: [String] = []
    _ = Settings.forCurrentUser(
        environment: ["HOME": "/Users/x"], loginPath: "bin:./node_modules/.bin:/usr/bin"
    ) {
        looked.append($0)
        return false
    }
    #expect(looked.allSatisfy { $0.hasPrefix("/") }, "\(looked)")
    #expect(looked.first == "/usr/bin/claude")
}
