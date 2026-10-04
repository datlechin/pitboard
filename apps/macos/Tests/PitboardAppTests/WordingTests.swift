import Foundation
import PitboardKit
import Testing

@testable import PitboardApp

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

/// This Mac heads its checks with the last line of `pitboard doctor`: what is worth looking
/// at while checks only warn, and not to switch accounts while one fails.
@Test func thisMacSaysNotToSwitchWhileACheckFails() {
    let check = { (level: Level) in
        Check(code: "keychain", name: "Keychain", level: level, detail: "", advice: "")
    }
    #expect(doctorSummary(checks: [check(.ok)]) == "Everything Pitboard checks is in order.")
    #expect(
        doctorSummary(checks: [check(.warn), check(.ok)]) == "One thing is worth looking at.")
    #expect(
        doctorSummary(checks: [check(.warn), check(.fail)])
            == "1 broken: do not switch accounts until fixed.")
}

/// Renew Now says what it did in the words `pitboard renew` says it in. Nothing due is the
/// ordinary case and reads as the good answer it is.
@Test func renewNowSaysWhatItDidAsTheCommandLineDoes() {
    let renewed = { (outcome: String) in
        Renewed(label: "work", provider: "claude", outcome: outcome)
    }
    #expect(renewalNote(renewals: []) == "No parked login was due.")
    #expect(renewalNote(renewals: [renewed("renewed")]) == "Renewed one.")
    #expect(
        renewalNote(renewals: [renewed("renewed"), renewed("renewal_deferred")])
            == "Renewed 1 of 2; the rest are tried again next time.")
}

/// A Claude Desktop login is never renewed by Pitboard, so a run lists each of its parks as
/// `not_renewable`. That is how they are, not something that was due and failed, so they
/// are left out of the count rather than read as a renewal that went wrong.
@Test func aLoginThatIsNeverRenewedIsNeitherDueNorFailed() {
    let renewed = { (outcome: String) in
        Renewed(label: "home", provider: "desktop", outcome: outcome)
    }
    #expect(renewalNote(renewals: [renewed("not_renewable")]) == "No parked login was due.")
    #expect(
        renewalNote(renewals: [renewed("renewed"), renewed("not_renewable")]) == "Renewed one.")
    #expect(
        renewalNote(renewals: [renewed("expired"), renewed("not_renewable")])
            == renewalNote(renewals: [renewed("expired")]))
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
            == "weekly Fable limit, 98 percent used, resetting now",
        "a reset whose time has come is said, as the column says it")
}

/// Pitboard says everything else in English, so a span of time VoiceOver reads inside one of
/// its sentences is English too, whatever the region of the Mac: "resets in 3 Stunden"
/// reads as a mistake. A test cannot change the region of the process it runs in, so this
/// checks that the same span in German reads differently, which is what a span following a
/// German Mac's region would show, and that Pitboard's reads as English. The spans shown on
/// screen are the core's, which has no region.
@Test func aSpokenSpanOfTimeReadsTheSameInEveryRegion() {
    let wide = Duration.seconds(3 * 3600).formatted(
        .units(allowed: [.days, .hours, .minutes], width: .wide, maximumUnitCount: 2)
            .locale(Locale(identifier: "de_DE")))
    #expect(wide != "3 hours")
    #expect(
        spokenLimit(window("five_hour", 42, length: 18_000), resettingIn: 90 * 60)
            == "5-hour limit, 42 percent used, resets in 1 hour, 30 minutes")
}

/// A label as the core types it, taken apart: bare means Claude Code.
@Test func aTypedLabelIsTakenApart() {
    #expect(split("codex/work") == ("codex", "work"))
    #expect(split("work") == ("claude", "work"))
}

/// A Claude Desktop account has no email, so a sentence that would start with one starts
/// with words instead, and an account with neither label nor email is not titled by nothing.
@Test func anAccountWithNoEmailIsNeverNamedByNothing() {
    #expect(whoIsSignedIn("") == "An account")
    #expect(whoIsSignedIn("a@example.com") == "a@example.com")
    #expect(accountName(label: "work", email: "") == "work")
    #expect(accountName(label: nil, email: "a@example.com") == "a@example.com")
    #expect(accountName(label: nil, email: "") == "Unnamed account")
}
