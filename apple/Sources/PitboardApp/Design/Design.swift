import PitboardKit
import SwiftUI

/// pitboard's design rules, in one place.
///
/// The app is made of the platform's own parts: a menu, a window with a sidebar, sheets,
/// grouped forms and alerts. What is decided here is only what those parts do not decide
/// themselves, so it is said once rather than by every view:
///
/// - Type: the system's text styles and nothing else. A name is `.body`, what describes it
///   is `.callout` or `.subheadline` in `.secondary`, figures are monospaced digits so a
///   column of percentages does not shift as they change.
/// - Colour: the system's semantic colours. Colour only ever repeats something a word or a
///   shape already says, because not everybody sees it.
/// - Symbols: SF Symbols, one per meaning, named here.
/// - Space: the platform's default padding and spacing, and the few measures below where a
///   component lays itself out.
enum Design {
    /// Between a row's lines.
    static let lineSpacing: CGFloat = 2
    /// Between the groups inside a row: its name, its limits, its notes.
    static let rowSpacing: CGFloat = 6
    /// Between a symbol and the text it leads.
    static let iconSpacing: CGFloat = 8
}

/// How much of a limit is used, in three steps. Colour follows it in every place a limit is
/// drawn, and the words always say the number itself.
enum UsageLevel: Equatable {
    case plenty
    case low
    case out

    init(percent: Double) {
        switch percent {
        case 90...: self = .out
        case 70...: self = .low
        default: self = .plenty
        }
    }

    var tint: Color {
        switch self {
        case .plenty: .green
        case .low: .orange
        case .out: .red
        }
    }
}

extension Notice.Severity {
    var symbol: String {
        switch self {
        case .info: "info.circle.fill"
        case .warning: "exclamationmark.triangle.fill"
        case .error: "xmark.octagon.fill"
        }
    }

    var tint: Color {
        switch self {
        case .info: .secondary
        case .warning: .orange
        case .error: .red
        }
    }

    /// What is said instead of naming a shape or a colour.
    var spoken: String {
        switch self {
        case .info: "Note"
        case .warning: "Warning"
        case .error: "Problem"
        }
    }
}

/// How a check's standing is shown. The shapes differ as well as the colours, and VoiceOver
/// is told the standing in words: a check that reads "state: fine" without saying whether it
/// passed is the same as not running it.
extension Level {
    var symbol: String {
        switch self {
        case .ok: "checkmark.circle.fill"
        case .warn: "exclamationmark.triangle.fill"
        case .fail: "xmark.octagon.fill"
        }
    }

    var tint: Color {
        switch self {
        case .ok: .green
        case .warn: .orange
        case .fail: .red
        }
    }

    var spoken: String {
        switch self {
        case .ok: "Passed"
        case .warn: "Worth looking at"
        case .fail: "Failed"
        }
    }
}

/// The symbols that mean one thing each, wherever they appear.
enum Symbol {
    static let menuBar = "speedometer"
    static let account = "person.crop.circle"
    static let accountInUse = "person.crop.circle.fill.badge.checkmark"
    static let accounts = "person.2"
    static let activity = "clock.arrow.circlepath"
    static let machine = "stethoscope"
    static let add = "plus"
    static let refresh = "arrow.clockwise"
    static let switchAccount = "arrow.left.arrow.right"
    static let signIn = "person.crop.circle.badge.exclamationmark"
    static let rename = "pencil"
    static let forget = "trash"
    static let update = "arrow.down.circle"
    static let terminal = "terminal"
    static let general = "gearshape"
    /// A claude.ai window's way back and forward through its pages.
    static let back = "chevron.left"
    static let forward = "chevron.right"
    /// A file saved from a claude.ai window. Not `arrow.down.circle`, which is an update.
    static let download = "tray.and.arrow.down"
}

extension View {
    /// Secondary text that wraps rather than truncating: what it says is the point.
    func explanatory() -> some View {
        font(.callout)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
    }

    /// Explanatory text under a form section, which reads from the leading edge like the
    /// rows above it. A grouped form puts a footer against the trailing edge otherwise.
    func footnote() -> some View {
        explanatory().frame(maxWidth: .infinity, alignment: .leading)
    }
}
