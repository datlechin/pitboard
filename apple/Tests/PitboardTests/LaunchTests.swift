import Testing

@testable import Pitboard

/// A renewal schedule an app up to 0.3.0 wrote starts the app with `renew`, and the command
/// line inside this one does the renewal in its place. A copy with none inside it starts
/// nothing, rather than a menu bar app that would repair the schedule from inside its job.
/// Anything else opens the app.
@Test func aScheduleThatStartsTheAppRenewsWithTheCommandLineInsideIt() {
    let app = "/Applications/Pitboard.app/Contents/MacOS/Pitboard"
    let helper = "/Applications/Pitboard.app/Contents/Helpers/pitboard"
    #expect(Launch.action(for: [app, "renew"], helper: helper) == .renew(helper))
    #expect(Launch.action(for: [app, "renew"], helper: nil) == .fail)

    #expect(Launch.action(for: [app], helper: helper) == .app)
    #expect(Launch.action(for: [], helper: helper) == .app)
    #expect(Launch.action(for: [app, "renew", "--json"], helper: helper) == .app)
    #expect(Launch.action(for: [app, "-AppleLanguages", "(en)"], helper: helper) == .app)
}
