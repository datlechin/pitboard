import AppKit
import Foundation
import PitboardKit

/// The app carries the command line inside it, and the cask for the app links that onto the
/// `PATH`. A copy downloaded from a release has nothing to do that, so the settings offer
/// to, the way editors on macOS put their own command there: one link in `/usr/local/bin`,
/// made after macOS asks for an administrator's password. Which `pitboard` a terminal runs,
/// and whether this copy is one a link would keep reaching, are the model's to say.
struct CommandLineTool: Sendable {
    /// This app's own command line, or nil when the app is not running from its bundle.
    let helper: String?
    /// On the `PATH` macOS gives every shell, in a directory only an administrator can write
    /// to.
    let link: String
    /// A test hands in its own, so nothing it does waits on a password prompt.
    private let execute: Runner

    /// Runs an AppleScript and hands back the error it raised, or nil.
    typealias Runner = @Sendable (String) -> NSDictionary?

    init(helper: String?, link: String, execute: @escaping Runner) {
        self.helper = helper
        self.link = link
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

    /// Whether there is a command line in this app to link to. A build run from Xcode has
    /// none inside it, and a link to where one would be would cost an administrator's
    /// password for a link that runs nothing. Whether the one inside can run is the core's to
    /// say, as it says of every program it finds; whether this copy stays where it is, the
    /// model says before it offers the link.
    var linkable: Bool {
        guard let helper else { return false }
        return canRun(path: helper)
    }

    // Off the main thread, since the password prompt waits on a person.
    func install() async -> Linked {
        guard let helper, linkable else {
            return .failed("This copy of Pitboard can’t link the command line inside it.")
        }
        let type = try? FileManager.default.attributesOfItem(atPath: link)[.type]
        if let type, type as? FileAttributeType != .typeSymbolicLink {
            return .failed("\(link) is already there and isn’t a link, so it was kept.")
        }
        let (source, execute) = (Self.script(linking: helper, at: link), execute)
        return await Task.detached(priority: .userInitiated) {
            Self.outcome(of: execute(source))
        }.value
    }

    /// macOS asks for the password in this app's name before anything runs.
    static func script(linking helper: String, at link: String) -> String {
        "do shell script \(command(linking: helper, at: link)) with administrator privileges"
    }

    /// The shell command that makes the link, as an AppleScript expression. Each path is an
    /// AppleScript string handed to the shell through `quoted form of`, so a quote in a path
    /// cannot end either early, and nothing in one is read by the shell as a command.
    static func command(linking helper: String, at link: String) -> String {
        let directory = (link as NSString).deletingLastPathComponent
        return "\"mkdir -p \" & quoted form of \(literal(directory)) & \" && ln -sfh \" & "
            + "quoted form of \(literal(helper)) & \" \" & quoted form of \(literal(link))"
    }

    /// `text` as an AppleScript string: a backslash and a double quote are the only
    /// characters one cannot hold as they are.
    static func literal(_ text: String) -> String {
        let escaped = text.replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
        return "\"\(escaped)\""
    }

    static func execute(script source: String) -> NSDictionary? {
        guard let script = NSAppleScript(source: source) else {
            return [NSAppleScript.errorMessage: "The link couldn’t be made."]
        }
        var error: NSDictionary?
        script.executeAndReturnError(&error)
        return error
    }

    static func outcome(of error: NSDictionary?) -> Linked {
        guard let error else { return .linked }
        if error[NSAppleScript.errorNumber] as? Int == userCanceledErr { return .cancelled }
        return .failed(
            error[NSAppleScript.errorMessage] as? String ?? "The link couldn’t be made.")
    }
}

/// The settings' Install Command Line Tool…: what linking is doing and why it could not, said
/// beside the button, never as a failure in the window.
@MainActor
@Observable
final class CommandLineLink {
    @ObservationIgnored private let tool: CommandLineTool
    @ObservationIgnored private let model: AppModel
    /// Whether macOS's password prompt is up.
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
