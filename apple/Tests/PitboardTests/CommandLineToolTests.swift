import Foundation
import Testing

@testable import Pitboard

/// A scratch directory with an app bundle carrying a command line, and whatever a test puts
/// beside it. Its name has a space and a quote in it, as a folder somebody made might.
private struct Scratch {
    let root: URL
    var app: URL { root.appendingPathComponent("Fake.app") }
    var helper: URL { app.appendingPathComponent("Contents/Helpers/pitboard") }

    init() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("pitboard's tool \(UUID().uuidString)")
        try FileManager.default.createDirectory(
            at: helper.deletingLastPathComponent(), withIntermediateDirectories: true)
        try program(at: helper)
    }

    /// A directory under the root, made if it is not there.
    func directory(_ name: String) throws -> String {
        let made = root.appendingPathComponent(name)
        try FileManager.default.createDirectory(at: made, withIntermediateDirectories: true)
        return made.path
    }

    func program(at url: URL, runnable: Bool = true) throws {
        try Data("#!/bin/sh\n".utf8).write(to: url)
        try FileManager.default.setAttributes(
            [.posixPermissions: runnable ? 0o755 : 0o644], ofItemAtPath: url.path)
    }

    func link(_ path: String, to destination: String) throws {
        try FileManager.default.createSymbolicLink(
            atPath: path, withDestinationPath: destination)
    }

    func remove() { try? FileManager.default.removeItem(at: root) }
}

/// The first `pitboard` on the path is the one a terminal runs, and a link to this app's own,
/// the way Homebrew makes one, is this app's own however many links it takes to get there.
/// A directory, a file nobody can run and a link to nothing are not a `pitboard`.
@Test func theFirstPitboardFoundSaysWhoseItIs() throws {
    let scratch = try Scratch()
    defer { scratch.remove() }
    let empty = try scratch.directory("empty")
    let folder = try scratch.directory("folder")
    _ = try scratch.directory("folder/pitboard")
    let plain = try scratch.directory("plain")
    try scratch.program(at: URL(fileURLWithPath: "\(plain)/pitboard"), runnable: false)
    let dangling = try scratch.directory("dangling")
    try scratch.link("\(dangling)/pitboard", to: "\(scratch.root.path)/gone/pitboard")
    let brew = try scratch.directory("brew/bin")
    try scratch.link("\(brew)/pitboard", to: "../../Fake.app/Contents/Helpers/pitboard")
    let cargo = try scratch.directory("cargo/bin")
    try scratch.program(at: URL(fileURLWithPath: "\(cargo)/pitboard"))
    let helpers = "\(scratch.root.path)/helpers"
    try scratch.link(helpers, to: scratch.helper.deletingLastPathComponent().path)

    let tool = CommandLineTool(bundle: scratch.app)
    #expect(
        tool.find(in: [empty, folder, plain, dangling, brew, cargo])
            == .bundled("\(brew)/pitboard"))
    #expect(tool.find(in: [helpers]) == .bundled("\(helpers)/pitboard"))
    #expect(tool.find(in: [cargo, brew]) == .another("\(cargo)/pitboard"))
    #expect(tool.find(in: [empty, folder, plain, dangling]) == .nowhere)
    #expect(tool.find(in: []) == .nowhere)

    let elsewhere = CommandLineTool(bundle: scratch.root.appendingPathComponent("Other.app"))
    #expect(elsewhere.find(in: [brew]) == .another("\(brew)/pitboard"), "another copy's")
    let built = CommandLineTool(bundle: scratch.root)
    #expect(built.find(in: [brew]) == .another("\(brew)/pitboard"), "not run from an app")
}

/// A link is offered only to an app with a command line inside it that stays where it is.
/// macOS runs an app opened where it was downloaded from a temporary copy, and a link into
/// that stops working once the app quits.
@Test func aLinkIsOfferedOnlyToAnAppThatStaysWhereItIs() {
    let installed = CommandLineTool(bundle: URL(fileURLWithPath: "/Applications/Pitboard.app"))
    #expect(installed.helper == "/Applications/Pitboard.app/Contents/Helpers/pitboard")
    #expect(installed.linkable)
    #expect(!installed.translocated)

    let downloaded = CommandLineTool(
        bundle: URL(
            fileURLWithPath:
                "/private/var/folders/xy/abc/T/AppTranslocation/0A1B2C/d/Pitboard.app"))
    #expect(downloaded.translocated)
    #expect(!downloaded.linkable)

    let built = CommandLineTool(bundle: URL(fileURLWithPath: "/Users/x/apple/.build/debug"))
    #expect(built.helper == nil)
    #expect(!built.linkable)
    #expect(!built.translocated)
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

/// The command makes the directory and the link, and replaces a link already there, such as
/// one left by a copy of the app that has since moved. Run here without administrator
/// rights, in a scratch directory whose name a shell would otherwise read commands in.
@MainActor
@Test func theCommandMakesTheLink() throws {
    let scratch = try Scratch()
    defer { scratch.remove() }
    let hostile = try scratch.directory(#"a "b" \ $(echo injected)"#)
    let link = "\(hostile)/bin/pitboard"
    let command = CommandLineTool.command(linking: scratch.helper.path, at: link)

    func run() throws {
        var error: NSDictionary?
        NSAppleScript(source: "do shell script \(command)")?.executeAndReturnError(&error)
        #expect(error == nil, "\(String(describing: error))")
        #expect(
            try FileManager.default.destinationOfSymbolicLink(atPath: link)
                == scratch.helper.path)
    }
    try run()
    let tool = CommandLineTool(bundle: scratch.app)
    #expect(tool.find(in: ["\(hostile)/bin"]) == .bundled(link))

    try FileManager.default.removeItem(atPath: link)
    try scratch.link(link, to: "/Applications/Moved.app/Contents/Helpers/pitboard")
    #expect(tool.find(in: ["\(hostile)/bin"]) == .nowhere)
    try run()
    #expect(tool.find(in: ["\(hostile)/bin"]) == .bundled(link))
}
