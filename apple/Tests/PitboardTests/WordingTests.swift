import PitboardKit
import Testing

@testable import Pitboard

/// The difference between a limit filling and a limit resetting is the difference between
/// "switch now" and "stay where you are", and both are a number of seconds.
@Test func aRunwayReadsAsBurningOrAsResting() {
    #expect(lasting(5400, burning: true) == "about 1h 30m left at this rate")
    #expect(lasting(5400, burning: false) == "resets in 1h 30m")
}

/// Rounding an almost-empty window down to "0 min" reads as though it were already gone,
/// and rounding it up reads as though there were time.
@Test func almostNoRunwayIsSaidInWordsRatherThanZero() {
    #expect(lasting(30, burning: true) == "about to run out")
    #expect(lasting(0, burning: false) == "resets any moment")
    #expect(lasting(-10, burning: true) == "about to run out")
}

private func renewed(_ outcome: String) -> Renewed {
    Renewed(label: "acc", outcome: outcome)
}

/// Nothing due is the ordinary case, and it has to read as ordinary rather than as a
/// failure to do anything.
@Test func aRenewalRunSaysWhatItDid() {
    #expect(renewalNote([]) == "Nothing was due.")
    #expect(renewalNote([renewed("renewed")]) == "Renewed one.")
    #expect(renewalNote([renewed("renewed"), renewed("renewed")]) == "Renewed all 2.")
    #expect(renewalNote([renewed("renewed"), renewed("expired")]) == "Renewed 1 of 2.")
    #expect(renewalNote([renewed("expired")]) == "1 due; none could be renewed this time.")
}
