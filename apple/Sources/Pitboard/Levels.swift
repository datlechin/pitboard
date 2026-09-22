import PitboardKit
import SwiftUI

/// How a check's standing is shown, in one place because it is shown in two.
///
/// Colour is not the only difference: the shapes differ too, and VoiceOver is told the
/// standing in words. A check that reads "state: fine" without saying whether it passed is
/// the same as not running it.
extension Level {
    var symbol: String {
        switch self {
        case .ok: "checkmark.circle"
        case .warn: "exclamationmark.triangle"
        case .fail: "xmark.octagon"
        }
    }

    var tint: Color {
        switch self {
        case .ok: .green
        case .warn: .orange
        case .fail: .red
        }
    }

    /// What is said instead of naming a shape or a colour.
    var spoken: String {
        switch self {
        case .ok: "fine"
        case .warn: "worth looking at"
        case .fail: "broken"
        }
    }
}
