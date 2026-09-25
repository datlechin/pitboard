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
    Renewed(label: "acc", provider: "claude", outcome: outcome)
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

/// A `pitboard` installed apart from the app is updated the way it was installed, and none
/// of those ways does it by itself. Saying it "updates on its own" read as though nothing
/// needed doing, until the app moved on and the command line refused its newer files.
@Test func aCommandLineInstalledApartSaysHowToUpdateIt() {
    #expect(updateNote(bundled: true) == "The one inside this app, so it updates with the app.")
    #expect(
        updateNote(bundled: false)
            == "Installed apart from this app, so update it the way you installed it.")
}

/// A check is shown as a shape and a colour, and said as a word. If two levels ever came to
/// look or sound the same, a broken check would read as a passing one.
@Test func everyLevelLooksAndSoundsLikeItself() {
    let levels: [Level] = [.ok, .warn, .fail]
    #expect(Set(levels.map(\.symbol)).count == levels.count)
    #expect(Set(levels.map(\.spoken)).count == levels.count)
    #expect(levels.allSatisfy { !$0.spoken.isEmpty })
}

/// A window is named by how long it runs, which both services agree on, so a Codex limit
/// reads the way a Claude Code one does.
@Test func aWindowIsNamedForItsLength() {
    #expect(windowName(window("five_hour", 1, length: 18_000)) == "5-hour")
    #expect(windowName(window("seven_day", 1, length: 604_800)) == "weekly")
    #expect(windowName(window("1_day", 1, length: 86_400)) == "daily")
    #expect(windowName(window("3_hour", 1, length: 10_800)) == "3-hour")
    #expect(windowName(window("2_day", 1, length: 172_800)) == "2-day")
    #expect(windowShortName(window("3_hour", 1, length: 10_800)) == "3h")
    #expect(windowShortName(window("1_day", 1, length: 86_400)) == "day")
    #expect(windowShortName(window("seven_day", 1, length: 604_800)) == "week")
}

/// A reading taken before the length was kept names its window the way it always did.
@Test func aWindowOfUnknownLengthIsNamedFromItsKind() {
    #expect(windowName(window("session", 1)) == "5-hour")
    #expect(windowName(window("weekly_all", 1)) == "weekly")
    #expect(windowName(window("primary_window", 1)) == "primary window")
    #expect(windowShortName(window("session", 1)) == "5h")
    #expect(windowShortName(window("weekly_scoped", 1, scope: "Fable")) == "week · Fable")
}

/// VoiceOver reads the column's "30m" as thirty meters and "5h" as letters, so a limit is
/// said in words: its name as a sentence says it, what it has used, and when it resets.
@Test func aLimitIsSpokenInWordsAndNotInItsColumnsShorthand() {
    #expect(
        spokenLimit(window("five_hour", 42, length: 18_000), resettingIn: 3 * 3600)
            == "5-hour limit, 42 percent used, resets in 3 hours")
    #expect(
        spokenLimit(window("30_minute", 12, length: 1800), resettingIn: nil)
            == "30-minute limit, 12 percent used")
    #expect(
        spokenLimit(window("weekly_scoped", 98, scope: "Fable"), resettingIn: 0)
            == "weekly Fable limit, 98 percent used",
        "a reset already passed is not said, as the column does not show it")
}

/// A label as the core types it, taken apart: bare means Claude Code.
@Test func aTypedLabelIsTakenApart() {
    #expect(split("codex/work") == ("codex", "work"))
    #expect(split("work") == ("claude", "work"))
}

/// A row is headed by its label, by "unenrolled" before it has one, and for a login no
/// account can be named for, by what is wrong with it.
@Test func aRowIsHeadedTheSameWayEverywhere() {
    #expect(account("work").heading == "work")
    #expect(account(nil, signedIn: true, uuid: "u").heading == "unenrolled")
    #expect(
        unplaced(of: "codex").heading
            == "Codex's login could not be read; run `pitboard doctor`")
}
