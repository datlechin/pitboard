import Foundation
import PitboardKit

/// Something a person asked for that did not happen, said to them where they asked.
///
/// Returned by the action rather than left in a property the next read replaces: a failed
/// switch used to be said in the panel above the accounts, where a read a few seconds later
/// could put it away before anyone looked, and a failed enrolment closed the form that had
/// the name typed in it.
public struct ActionFailure: Identifiable, Equatable, Sendable {
    public let id = UUID()
    /// What was being done, as an alert's title says it: "Couldn't switch to personal".
    public let title: String
    /// What went wrong. Pitboard's errors already say what to do, so they are shown as
    /// they are.
    public let message: String
    /// The stable code behind it, for deciding what to offer.
    public let code: String?
    /// Everything else the failure warned about.
    public let warnings: [Warning]

    init(_ title: String, message: String, code: String? = nil, warnings: [Warning] = []) {
        self.title = title
        self.message = message
        self.code = code
        self.warnings = warnings
    }

    init(_ title: String, error: Error) {
        self.init(
            title, message: AppModel.saying(error), code: AppModel.code(of: error),
            warnings: AppModel.warnings(of: error))
    }

    public static func == (lhs: ActionFailure, rhs: ActionFailure) -> Bool {
        lhs.id == rhs.id
    }
}

/// A sheet over the main window, and what it is for.
public enum AccountSheet: Identifiable, Equatable, Sendable {
    /// A new account, which means the tool's own sign-in in a browser. The tool it starts
    /// on, when something already said which; the sheet can change it.
    case add(provider: String?)
    /// An enrolled account whose parked login can no longer be used, signed in to again
    /// through the same sign-in as a new one.
    case signInAgain(provider: String, label: String)
    /// Record the login signed in now to this tool under a name: no browser.
    case name(provider: String, email: String)
    /// A new name for an enrolled account.
    case rename(provider: String, label: String)
    /// Live usage for Claude Desktop, which reads Claude's key and so can make macOS ask
    /// for the login password: said first, and turned on only from this sheet.
    case liveUsage

    public var id: String {
        switch self {
        case .add(let provider): "add/\(provider ?? "")"
        case .signInAgain(let provider, let label): "again/\(provider)/\(label)"
        case .name(let provider, _): "name/\(provider)"
        case .rename(let provider, let label): "rename/\(provider)/\(label)"
        case .liveUsage: "liveUsage"
        }
    }
}
