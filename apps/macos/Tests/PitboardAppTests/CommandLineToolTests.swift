import Foundation
import Testing

@testable import PitboardApp

/// Whether the app stays where it is, rather than running from the temporary copy macOS
/// makes of one opened where it was downloaded, is the model's to say before it offers the
/// link. The app is a stand-in the test makes, so the answer does not depend on whether the
/// Mac running the tests has Pitboard installed.
@Test func onlyTheCommandLineInsideAnAppIsLinked() throws {
    let app = try StandInApp()
    defer { app.remove() }
    let installed = CommandLineTool(bundle: app.bundle)
    #expect(installed.helper == app.helper.path)
    #expect(installed.linkable)

    let gone = CommandLineTool(bundle: URL(fileURLWithPath: "/nowhere/at/all/Pitboard.app"))
    #expect(gone.helper == "/nowhere/at/all/Pitboard.app/Contents/Helpers/pitboard")
    #expect(!gone.linkable, "nothing is there")

    let built = CommandLineTool(bundle: URL(fileURLWithPath: "/Users/x/apple/.build/debug"))
    #expect(built.helper == nil)
    #expect(!built.linkable)
}

/// Judged as the core judges a program wherever it looks for one. A directory where the
/// command line would be is not one, though macOS lets this user search it, which
/// `FileManager.isExecutableFile` took for running it.
@Test func aLinkIsOfferedOnlyToACommandLineThisUserMayRun() throws {
    let app = try StandInApp()
    defer { app.remove() }
    try FileManager.default.removeItem(at: app.helper)
    try FileManager.default.createDirectory(
        at: app.helper, withIntermediateDirectories: false)
    #expect(!CommandLineTool(bundle: app.bundle).linkable, "a directory")

    try FileManager.default.removeItem(at: app.helper)
    try app.program(at: app.helper, runnable: false)
    #expect(!CommandLineTool(bundle: app.bundle).linkable, "a file nobody may run")
}

/// A path reaches the shell as it is: a quote cannot end the AppleScript string or the
/// shell's early, and nothing in it is read by the shell as a command.
@MainActor
@Test func thePathsReachTheShellAsTheyAre() throws {
    let helper =
        #"/Users/x/it's "mine" \ $(echo injected)/Pitboard.app/Contents/Helpers/pitboard"#
    let command = CommandLineTool.command(linking: helper, at: "/usr/local/bin/pitboard")

    var error: NSDictionary?
    let said = NSAppleScript(source: "return \(command)")?.executeAndReturnError(&error)
    #expect(error == nil, "\(String(describing: error))")
    #expect(
        said?.stringValue
            == #"mkdir -p '/usr/local/bin' && ln -sfh '/Users/x/it'\''s "mine" \ $(echo injected)"#
            + #"/Pitboard.app/Contents/Helpers/pitboard' '/usr/local/bin/pitboard'"#)

    let script = CommandLineTool.script(linking: helper, at: "/usr/local/bin/pitboard")
    #expect(script == "do shell script \(command) with administrator privileges")
    // Compiled and not run: running it asks for a password.
    let compiled = try #require(NSAppleScript(source: script))
    var refused: NSDictionary?
    let compiles = compiled.compileAndReturnError(&refused)
    #expect(compiles, "\(String(describing: refused))")
}

/// Run without administrator rights, in a scratch directory whose name a shell would
/// otherwise read commands in.
@MainActor
@Test func theCommandMakesTheLink() throws {
    let app = try StandInApp()
    defer { app.remove() }
    let hostile = try app.directory(#"a "b" \ $(echo injected)"#)
    let link = "\(hostile)/bin/pitboard"
    let command = CommandLineTool.command(linking: app.helper.path, at: link)

    func run() throws {
        var error: NSDictionary?
        NSAppleScript(source: "do shell script \(command)")?.executeAndReturnError(&error)
        #expect(error == nil, "\(String(describing: error))")
        #expect(
            try FileManager.default.destinationOfSymbolicLink(atPath: link)
                == app.helper.path)
    }
    try run()
    try FileManager.default.removeItem(atPath: link)
    try app.symlink(link, to: "/Applications/Moved.app/Contents/Helpers/pitboard")
    try run()
}

@Test func somebodysOwnFileWhereTheLinkGoesIsKept() async throws {
    let app = try StandInApp()
    defer { app.remove() }
    let bin = try app.directory("bin")
    let link = "\(bin)/pitboard"
    try Data("mine".utf8).write(to: URL(fileURLWithPath: link))
    let scripts = Scripts()
    let tool = CommandLineTool(bundle: app.bundle, link: link, execute: scripts.run)

    #expect(
        await tool.install()
            == .failed("\(link) is already there and isn’t a link, so it was kept."))
    #expect(try String(contentsOfFile: link, encoding: .utf8) == "mine")
    try FileManager.default.removeItem(atPath: link)
    try FileManager.default.createDirectory(atPath: link, withIntermediateDirectories: false)
    #expect(await tool.install() != .linked, "a directory is kept too")
    #expect(scripts.ran.isEmpty)

    try FileManager.default.removeItem(atPath: link)
    #expect(await tool.install() == .linked, "nothing there")
    try app.symlink(link, to: "/Applications/Moved.app/Contents/Helpers/pitboard")
    #expect(await tool.install() == .linked, "a link to a copy that has moved")
    try FileManager.default.removeItem(atPath: link)
    try app.symlink(link, to: "\(bin)/../Pitboard.app/Contents/Helpers/pitboard")
    #expect(await tool.install() == .linked, "a link to a file that is there")
    let script = CommandLineTool.script(linking: app.helper.path, at: link)
    #expect(scripts.ran == [script, script, script])

    let gone = CommandLineTool(
        bundle: URL(fileURLWithPath: "/nowhere/at/all/Pitboard.app"), link: link,
        execute: scripts.run)
    #expect(
        await gone.install()
            == .failed("This copy of Pitboard can’t link the command line inside it."))
    #expect(scripts.ran.count == 3)
}

@Test func aDismissedPasswordPromptIsAnAnswer() {
    func error(_ number: Int, _ message: String? = nil) -> NSDictionary {
        var error: [String: Any] = [NSAppleScript.errorNumber: number]
        error[NSAppleScript.errorMessage] = message
        return error as NSDictionary
    }
    #expect(CommandLineTool.outcome(of: nil) == .linked)
    #expect(CommandLineTool.outcome(of: error(-128, "User canceled.")) == .cancelled)
    #expect(
        CommandLineTool.outcome(of: error(1, "ln: /usr/local/bin/pitboard: Permission denied"))
            == .failed("ln: /usr/local/bin/pitboard: Permission denied"))
    #expect(CommandLineTool.outcome(of: error(1)) == .failed("The link couldn’t be made."))
}
