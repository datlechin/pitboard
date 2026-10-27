import AppKit
import Foundation
import PitboardKit

// The app carries the command line inside it, and the pitboard-app cask links that onto the
// PATH. A copy from a release has nothing to, so the settings offer one link in /usr/local/bin,
// as editors on macOS do, made once macOS has asked for an administrator's password.
struct CommandLineTool: Sendable {
    // nil when the app is not running from its bundle
    let helper: String?
    let link: String
    // Off for the fixture, whose scripts reach no password prompt.
    let administrator: Bool
    private let execute: Runner

    // Runs an AppleScript and hands back the error it raised, or nil.
    typealias Runner = @Sendable (String) -> NSDictionary?

    init(helper: String?, link: String, administrator: Bool = true, execute: @escaping Runner) {
        self.helper = helper
        self.link = link
        self.administrator = administrator
        self.execute = execute
    }

    init(
        bundle: URL = Bundle.main.bundleURL,
        link: String = "/usr/local/bin/pitboard",
        execute: @escaping Runner = CommandLineTool.execute(script:)
    ) {
        self.init(helper: appCommandLine(app: bundle.path), link: link, execute: execute)
    }

    enum Linked: Equatable, Sendable {
        case linked
        case cancelled
        case failed(String)
    }

    // A build run from Xcode has no command line inside it, and a link to where one would be
    // would cost a password for a link that runs nothing.
    var linkable: Bool {
        guard let helper else { return false }
        return canRun(path: helper)
    }

    // Off the main thread, since the password prompt waits on a person.
    func install() async -> Linked {
        guard let helper, linkable else {
            return .failed(words.settings.cannotLinkCommandLine ?? "")
        }
        let type = try? FileManager.default.attributesOfItem(atPath: link)[.type]
        if let type, type as? FileAttributeType != .typeSymbolicLink {
            return .failed(alreadyThere(link: link))
        }
        let source = Self.script(linking: helper, at: link, administrator: administrator)
        return await run(source)
    }

    // The link Install Command Line Tool… makes.
    var linksHere: Bool {
        guard let helper else { return false }
        return (try? FileManager.default.destinationOfSymbolicLink(atPath: link)) == helper
    }

    // Off the main thread, since the password prompt waits on a person.
    func unlink() async -> Linked {
        // No link of this copy's can be there.
        guard let helper else { return .linked }
        return await run(Self.script(removing: link, to: helper, administrator: administrator))
    }

    private func run(_ source: String) async -> Linked {
        let execute = execute
        return await Task.detached(priority: .userInitiated) {
            Self.outcome(of: execute(source))
        }.value
    }

    // macOS asks for the password in this app's name before anything runs.
    static func script(linking helper: String, at link: String, administrator: Bool = true)
        -> String
    {
        shell(command(linking: helper, at: link), administrator: administrator)
    }

    static func script(removing link: String, to helper: String, administrator: Bool = true)
        -> String
    {
        shell(command(removing: link, to: helper), administrator: administrator)
    }

    private static func shell(_ command: String, administrator: Bool) -> String {
        "do shell script \(command)" + (administrator ? " with administrator privileges" : "")
    }

    // Checked in the shell that removes it: another copy's link may have replaced it since.
    static func command(removing link: String, to helper: String) -> String {
        "\"test \\\"$(readlink \" & quoted form of \(literal(link)) & \")\\\" = \" & "
            + "quoted form of \(literal(helper)) & \" && rm \" & quoted form of \(literal(link))"
    }

    // Each path reaches the shell through `quoted form of`, so nothing in one is read as a
    // command.
    static func command(linking helper: String, at link: String) -> String {
        let directory = (link as NSString).deletingLastPathComponent
        return "\"mkdir -p \" & quoted form of \(literal(directory)) & \" && ln -sfh \" & "
            + "quoted form of \(literal(helper)) & \" \" & quoted form of \(literal(link))"
    }

    // A backslash and a double quote are all an AppleScript string cannot hold as they are.
    static func literal(_ text: String) -> String {
        let escaped = text.replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
        return "\"\(escaped)\""
    }

    static func execute(script source: String) -> NSDictionary? {
        guard let script = NSAppleScript(source: source) else {
            return [NSAppleScript.errorMessage: words.settings.linkFailed ?? ""]
        }
        var error: NSDictionary?
        script.executeAndReturnError(&error)
        return error
    }

    static func outcome(of error: NSDictionary?) -> Linked {
        guard let error else { return .linked }
        if error[NSAppleScript.errorNumber] as? Int == userCanceledErr { return .cancelled }
        return .failed(
            error[NSAppleScript.errorMessage] as? String ?? words.settings.linkFailed ?? "")
    }
}

// Install Command Line Tool…: why it could not link is said beside the button, never as a
// failure in the window.
@MainActor
@Observable
final class CommandLineLink {
    @ObservationIgnored private let tool: CommandLineTool
    @ObservationIgnored private let model: AppModel
    // while macOS's password prompt is up
    private(set) var linking = false
    private(set) var failed: String?

    init(_ tool: CommandLineTool, model: AppModel) {
        self.tool = tool
        self.model = model
    }

    var target: String { tool.link }

    func install() async {
        linking = true
        defer { linking = false }
        failed = nil
        if case .failed(let why) = await tool.install() {
            failed = why
        }
        model.send(.lookForCommandLine)
    }
}
