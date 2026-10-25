import PitboardKit
import SwiftUI

/// Pitboard's design rules, in one place.
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
/// - Symbols: SF Symbols.
/// - Space: the platform's default padding and spacing, and the few measures below where a
///   component lays itself out.
enum Design {
    static let lineSpacing: CGFloat = 2
    /// Between the groups inside a row: its name, its limits, its notes.
    static let rowSpacing: CGFloat = 6
    static let iconSpacing: CGFloat = 8
}

/// How much of a limit is used, in the core's three steps, which are where the command
/// line's colour changes too. Colour follows it in every place a limit is drawn, and the
/// words always say the number itself.
extension UsageLevel {
    var tint: Color {
        switch self {
        case .plenty: .green
        case .low: .orange
        case .out: .red
        }
    }
}

/// Which side of an even pace a limit is on, as the mark on its bar is tinted: red over,
/// green under. The words beside the bar say it too, in the core's own words, so the colour
/// only repeats them. Over is the one worth reading at a glance, so it is the one whose
/// words are tinted; under and even are said in the secondary style every other figure is.
extension PaceStanding {
    var tint: Color {
        switch self {
        case .over: .red
        case .under: .green
        case .even: .secondary
        }
    }

    var wordsStyle: AnyShapeStyle {
        self == .over ? AnyShapeStyle(.red) : AnyShapeStyle(.secondary)
    }
}

/// VoiceOver is told a notice's severity in the model's word, `PanelNotice.spokenSeverity`.
extension Severity {
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
}

/// The shapes differ as well as the colours, and VoiceOver is told the standing in the
/// model's words, `CheckLine.spokenLevel`: a check that reads "state: fine" without saying
/// whether it passed is the same as not running it.
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
}

/// The symbols that mean one thing each, wherever they appear.
enum Symbol {
    static let account = "person.crop.circle"
    static let accounts = "person.2"
    static let activity = "clock.arrow.circlepath"
    static let machine = "stethoscope"
    static let add = "plus"
    static let refresh = "arrow.clockwise"
    static let switchAccount = "arrow.left.arrow.right"
    static let signIn = "person.crop.circle.badge.exclamationmark"
    static let update = "arrow.down.circle"
    static let terminal = "terminal"
    static let general = "gearshape"
    static let limits = "gauge.open.with.lines.needle.33percent"
    static let advanced = "gearshape.2"
    static let site = "globe"
    /// Back and forward in a window, which mirror for a language read right to left.
    static let back = "chevron.backward"
    static let forward = "chevron.forward"
    static let stop = "xmark"
    static let downloads = "arrow.down.circle"
    static let warning = "exclamationmark.triangle"
    static let note = "info.circle"
}

extension View {
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

// What the UI tests find a control by: the model's case it shows or sends, "switchTo", and for an
// intent that puts up a sheet, the sheet's, "rename".
func caseName(_ value: some Any) -> String {
    if let intent = value as? Intent, case .presentSheet(let sheet) = intent {
        return caseName(sheet)
    }
    return String(String(describing: value).prefix { $0 != "(" })
}
