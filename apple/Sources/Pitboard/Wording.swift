import Foundation
import PitboardKit

/// Phrases the window and the settings both say, out of a view so a test can read them.
/// A sentence about time that is quietly wrong is worse than no sentence, and inside a
/// `body` there is nothing to assert against.

/// The answer to the question the whole tool exists for: how long the account you are on
/// is good for. `burning` is a limit filling rather than a limit resetting, which is the
/// difference between "about an hour left" and "whole again in an hour".
func lasting(_ seconds: Int64, burning: Bool) -> String {
    // Under a minute the units formatter says "0 min", which reads as though nothing were
    // left when the difference is seconds either way.
    guard seconds >= 60 else { return burning ? "about to run out" : "resets any moment" }
    let span = Duration.seconds(seconds)
        .formatted(.units(allowed: [.days, .hours, .minutes], width: .narrow))
    return burning ? "about \(span) left at this rate" : "resets in \(span)"
}

/// What a renewal run did. Everything here is a login that was going to expire, so "nothing
/// happened" is the good answer and has to read like one.
func renewalNote(_ renewals: [Renewed]) -> String {
    let renewed = renewals.filter { $0.outcome == "renewed" }.count
    switch (renewals.count, renewed) {
    case (0, _): return "Nothing was due."
    case (_, 0): return "\(renewals.count) due; none could be renewed this time."
    case (let all, let done) where all == done:
        return done == 1 ? "Renewed one." : "Renewed all \(done)."
    case (let all, let done): return "Renewed \(done) of \(all)."
    }
}
