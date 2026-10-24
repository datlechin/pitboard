import XCTest

final class PanesAndSettingsTests: FixtureTestCase {
    /// Newest first: the fixture's two switches are newer than both its enrolments.
    @MainActor
    func testActivityListsChanges() {
        let app = launch(.oneTool)
        app.openWindow()
        app.descendants(matching: .any)["sidebar.activity"].click()
        let switched = app.text("Switch")
        let enrolled = app.text("Enrol")
        XCTAssertTrue(switched.waitForExistence(timeout: 5))
        XCTAssertTrue(enrolled.exists)
        XCTAssertLessThan(switched.frame.minY, enrolled.frame.minY, "newest first")
    }

    /// The one thing worth looking at in this fixture is personal's parked login, which
    /// lapses soon.
    @MainActor
    func testThisMacShowsTheChecks() {
        let app = launch(.oneTool)
        app.openWindow()
        app.descendants(matching: .any)["sidebar.machine"].click()
        XCTAssertTrue(app.staticTexts["Account personal"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["One thing is worth looking at."].exists)
    }

    // The app renews as it reads, and `pitboard schedule status` says where the job is.
    @MainActor
    func testDailyRenewal() {
        let app = launch(.oneTool)
        app.openSettings()
        app.toolbars.buttons["Advanced"].click()
        let renew = app.control("settings.renewDaily")
        XCTAssertTrue(renew.waitForExistence(timeout: 5))
        renew.click()
        XCTAssertTrue(renew.wait(until: "value == 1", timeout: 5))
        XCTAssertFalse(app.text("Every day").exists)
        XCTAssertFalse(app.buttons["Renew Now"].exists)
    }

    /// Opening at login turns on and off without registering the test build.
    @MainActor
    func testOpenAtLogin() {
        let app = launch(.oneTool)
        app.openSettings()
        let toggle = app.control("settings.openAtLogin")
        XCTAssertTrue(toggle.waitForExistence(timeout: 5))
        XCTAssertEqual(toggle.value as? Int, 0)
        toggle.click()
        XCTAssertTrue(toggle.wait(until: "value == 1", timeout: 5))
        toggle.click()
        XCTAssertTrue(toggle.wait(until: "value == 0", timeout: 5))
    }

    @MainActor
    func testSettingsHaveThreePanes() {
        let app = launch(.oneTool)
        app.openSettings()
        let panes = app.windows["General"].toolbars.buttons
        XCTAssertEqual(panes.count, 3)
        for pane in ["General", "Limits", "Advanced"] {
            XCTAssertTrue(panes[pane].exists, pane)
        }
    }

    /// The model answers a change after the click, so each read after one waits.
    @MainActor
    func testWhatHappensBeforeAnAccountRunsOut() {
        let app = launch(.oneTool)
        app.openSettings()
        app.toolbars.buttons["Limits"].click()
        let choice = app.popUpButtons["settings.atLimit"]
        let share = app.control("settings.atLimitShare")
        let standing = app.control("settings.autoSwitchStanding")
        XCTAssertTrue(choice.waitForExistence(timeout: 5))
        XCTAssertTrue(share.exists)
        // Both are held back until the app's preferences have been read.
        XCTAssertTrue(choice.wait(for: \.isEnabled, toEqual: true, timeout: 5))
        XCTAssertTrue(share.isEnabled)
        XCTAssertEqual(choice.value as? String, "Notify Me", "a fresh install notifies")
        XCTAssertFalse(standing.exists, "nothing said while it does not switch")

        choice.click()
        choice.menuItems["Switch Automatically"].click()
        XCTAssertTrue(choice.wait(until: "value == 'Switch Automatically'", timeout: 5))
        XCTAssertTrue(standing.waitForExistence(timeout: 5), "what the look came to")

        let raise = share.incrementArrows.firstMatch
        XCTAssertTrue(raise.exists, "a stepper shows its arrows")
        raise.click()
        XCTAssertTrue(says(app, "When a limit reaches 96%").waitForExistence(timeout: 5))

        let typed = app.textFields["settings.atLimitShareField"]
        XCTAssertTrue(typed.exists, "a field beside the stepper")
        typed.doubleClick()
        typed.typeText("80\r")
        XCTAssertTrue(says(app, "When a limit reaches 80%").waitForExistence(timeout: 5))

        choice.click()
        choice.menuItems["Do Nothing"].click()
        XCTAssertTrue(choice.wait(until: "value == 'Do Nothing'", timeout: 5))
        XCTAssertTrue(standing.waitForNonExistence(timeout: 5))
    }

    /// The fixture's personal is on for warm-ups with no five-hour period running, so it
    /// warms up as the app starts.
    @MainActor
    func testWarmingUpAccounts() {
        let app = launch(.warmUp)
        app.openWindow()
        let warmed = app.text("BEGINSWITH", "Warms up again at ")
        XCTAssertTrue(warmed.waitForExistence(timeout: 30), "personal is warmed up")

        app.openSettings()
        app.toolbars.buttons["Limits"].click()
        let personal = app.control("settings.warm.claude/personal")
        let work = app.control("settings.warm.claude/work")
        XCTAssertTrue(personal.waitForExistence(timeout: 5))
        XCTAssertTrue(personal.wait(for: \.isEnabled, toEqual: true, timeout: 5))
        XCTAssertEqual(personal.value as? Int, 1)
        XCTAssertEqual(work.value as? Int, 0)
        XCTAssertTrue(app.control("settings.warmFrom").exists)
        XCTAssertTrue(app.control("settings.warmStanding").waitForExistence(timeout: 5))

        work.click()
        XCTAssertTrue(work.wait(until: "value == 1", timeout: 5))
    }

    /// What names the share a limit is reached at, as `words`: a text beside the stepper,
    /// or the stepper itself where macOS reads its label as the stepper's.
    @MainActor
    private func says(_ app: XCUIApplication, _ words: String) -> XCUIElement {
        let format = "label == %@ OR CAST(value, \"NSString\") == %@"
        let named = NSPredicate(format: format, words, words)
        return app.descendants(matching: .any).matching(named).firstMatch
    }

    // The fixture has no pitboard of its own, so the pane offers the one inside the app.
    @MainActor
    func testInstallingTheCommandLine() {
        let app = launch(.oneTool)
        app.openSettings()
        app.toolbars.buttons["Advanced"].click()
        let install = app.buttons["Install Command Line Tool…"]
        XCTAssertTrue(install.waitForExistence(timeout: 5))
        install.click()
        XCTAssertTrue(install.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.text("ENDSWITH", "/bin/pitboard").waitForExistence(timeout: 5))
    }

    // A debug build carries no update key.
    @MainActor
    func testUpdatesSayWhenTheBuildCannotUpdate() {
        let app = launch(.oneTool)
        app.openSettings()
        app.toolbars.buttons["General"].click()
        let prefix = "This copy of Pitboard can"
        let note = app.staticTexts.matching(
            NSPredicate(format: "value BEGINSWITH %@ OR label BEGINSWITH %@", prefix, prefix)
        ).firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 5))
    }
}
