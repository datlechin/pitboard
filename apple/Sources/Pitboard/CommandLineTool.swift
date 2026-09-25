import AppKit
import Foundation
import PitboardKit

/// The `pitboard` a terminal runs, and a way to put this app's own there.
///
/// The app carries the command line inside it, and the cask for the app links that onto the
/// `PATH`. A copy downloaded from a release has nothing to do that, so the settings offer
/// to, the way editors on macOS put their own command there: one link in `/usr/local/bin`,
/// made after macOS asks for an administrator's password.
struct CommandLineTool: Sendable {
    /// Where the link goes: on the `PATH` macOS gives every shell, and a directory only an
    /// administrator can write to.
    static let link = "/usr/local/bin/pitboard"

    /// This app's own command line, or nil when the app is not running from its bundle.
    let helper: String?

    init(bundle: URL = Bundle.main.bundleURL) {
        helper = Settings.bundledCommandLine(in: bundle)
    }

    /// The first `pitboard` found.
    enum Found: Equatable, Sendable {
        /// This app's own, at this path or linked to from it.
        case bundled(String)
        /// Another install, at this path.
        case another(String)
        case nowhere
    }

    /// What linking came to.
    enum Linked: Equatable, Sendable {
        case linked
        /// The password prompt was dismissed, which is an answer and not a failure.
        case cancelled
        case failed(String)
    }

    /// Where each way of installing pitboard puts it, looked in after the login shell's
    /// `PATH`, which may not have answered: cargo, a copy from a release, Homebrew on either
    /// kind of Mac, and the link made here.
    static func places(home: String) -> [String] {
        ["\(home)/.cargo/bin", "\(home)/.local/bin", "/opt/homebrew/bin", "/usr/local/bin"]
    }

    /// The first `pitboard` in `directories` that can be run, and whether it is this app's
    /// own once every link on the way to it is followed.
    func find(in directories: [String]) -> Found {
        guard
            let found = directories.lazy.map({ "\($0)/pitboard" }).first(where: Self.runnable)
        else { return .nowhere }
        guard let helper, let own = Self.resolved(helper), Self.resolved(found) == own else {
            return .another(found)
        }
        return .bundled(found)
    }

    /// macOS runs an app opened where it was downloaded from a temporary copy until it is
    /// moved, and a link into that copy stops working once the app quits.
    var translocated: Bool { helper?.contains("/AppTranslocation/") ?? false }

    /// Whether there is a command line in this app that a link would keep reaching.
    var linkable: Bool { helper != nil && !translocated }

    /// Links `link` to this app's command line once macOS has asked for an administrator's
    /// password. Off the main thread, since the prompt waits on a person. Anything at `link`
    /// that is not a link is somebody's own, and is left where it is.
    func install() async -> Linked {
        guard let helper, linkable else {
            return .failed("This copy of pitboard cannot link the command line inside it.")
        }
        let type = try? FileManager.default.attributesOfItem(atPath: Self.link)[.type]
        if let type, type as? FileAttributeType != .typeSymbolicLink {
            return .failed("\(Self.link) is already there and is not a link, so it was kept.")
        }
        let source = Self.script(linking: helper, at: Self.link)
        return await Task.detached(priority: .userInitiated) { Self.run(source) }.value
    }

    /// The script that runs `command` as an administrator. macOS asks for the password in
    /// this app's name before anything runs.
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

    private static func run(_ source: String) -> Linked {
        guard let script = NSAppleScript(source: source) else {
            return .failed("The link could not be made.")
        }
        var error: NSDictionary?
        script.executeAndReturnError(&error)
        guard let error else { return .linked }
        if error[NSAppleScript.errorNumber] as? Int == userCanceledErr { return .cancelled }
        return .failed(
            error[NSAppleScript.errorMessage] as? String ?? "The link could not be made.")
    }

    /// A file, not a directory, that this user may run, once every link is followed.
    private static func runnable(_ path: String) -> Bool {
        var directory: ObjCBool = false
        return FileManager.default.fileExists(atPath: path, isDirectory: &directory)
            && !directory.boolValue && FileManager.default.isExecutableFile(atPath: path)
    }

    /// `path` with every link on the way followed, as `realpath` gives it.
    private static func resolved(_ path: String) -> String? {
        guard let real = realpath(path, nil) else { return nil }
        defer { free(real) }
        return String(cString: real)
    }
}
