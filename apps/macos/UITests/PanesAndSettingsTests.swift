import AppKit
import XCTest

final class PanesAndSettingsTests: FixtureTestCase {
    // The fixture's switches and enrolments, in a window of their own.
    @MainActor
    func testActivityListsChanges() {
        let app = launch(.oneTool)
        app.openWindow()
        app.openFromWindowMenu(Words.activity)
        let activity = app.windows["activity"]
        XCTAssertTrue(activity.waitForExistence(timeout: 5))
        let changes = activity.staticTexts.matching(identifier: "activity.change")
        XCTAssertTrue(changes.firstMatch.waitForExistence(timeout: 5))
        XCTAssertGreaterThan(changes.count, 1)
        XCTAssertTrue(app.accountsWindow.exists)
    }

    // The fixture's checks under their summary, and doctor's report, redacted.
    @MainActor
    func testDiagnosticsShowsTheChecks() {
        let app = launch(.oneTool)
        app.openWindow()
        app.openFromWindowMenu(Words.diagnostics)
        let diagnostics = app.windows["diagnostics"]
        XCTAssertTrue(diagnostics.waitForExistence(timeout: 5))
        XCTAssertTrue(
            diagnostics.descendants(matching: .any)["checks.summary"]
                .waitForExistence(timeout: 10))
        XCTAssertTrue(diagnostics.descendants(matching: .any)["check.0"].exists)
        let copy = diagnostics.toolbars.buttons["toolbar.copyReport"]
        XCTAssertTrue(copy.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        NSPasteboard.general.clearContents()
        copy.click()
        let report = NSPasteboard.general.string(forType: .string) ?? ""
        XCTAssertTrue(report.hasPrefix("{"), "doctor's JSON report")
        XCTAssertFalse(report.contains("dana@"), "with email addresses hidden")
    }

    @MainActor
    func testDailyRenewal() {
        let app = launch(.oneTool)
        app.openSettings()
        app.toolbars.buttons[Words.advanced].click()
        let renew = app.control("settings.renewDaily")
        XCTAssertTrue(renew.waitForExistence(timeout: 5))
        renew.click()
        XCTAssertTrue(renew.wait(until: "value == 1", timeout: 5))
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

    /// macOS can refuse the login item, which General says.
    @MainActor
    func testOpenAtLoginMacOSRefusesSaysWhy() {
        let app = launch(.oneTool, environment: ["PITBOARD_FIXTURE_LOGIN_ITEM_REFUSES": "1"])
        app.openSettings()
        let toggle = app.control("settings.openAtLogin")
        XCTAssertTrue(toggle.waitForExistence(timeout: 5))
        toggle.click()
        XCTAssertTrue(app.control("settings.openAtLoginFailed").waitForExistence(timeout: 5))
        XCTAssertEqual(toggle.value as? Int, 0)
    }

    // Plan 4.5: titled by the pane it shows, in the middle of the screen, not against its top.
    @MainActor
    func testSettingsHaveThreePanesAndOpenCentred() throws {
        let app = launch(.oneTool)
        app.openSettings()
        let settings = app.windows[Words.general]
        let panes = settings.toolbars.buttons
        XCTAssertEqual(panes.count, 3)
        for pane in [Words.general, Words.limits, Words.advanced] {
            XCTAssertTrue(panes[pane].exists, pane)
        }
        // The first screen's frame, whose top left is where a window's frame is measured from.
        let screen = try XCTUnwrap(NSScreen.screens.first).frame
        XCTAssertEqual(settings.frame.midX, screen.midX, accuracy: 2)
        XCTAssertEqual(settings.frame.midY, screen.midY, accuracy: screen.height / 8)
        panes[Words.limits].click()
        XCTAssertTrue(app.windows[Words.limits].waitForExistence(timeout: 5))
    }

    /// The model answers a change after the click, so each read after one waits.
    @MainActor
    func testWhatHappensBeforeAnAccountRunsOut() {
        let app = launch(.oneTool)
        app.openSettings()
        app.toolbars.buttons[Words.limits].click()
        let choice = app.popUpButtons["settings.atLimit"]
        let share = app.control("settings.atLimitShare")
        let standing = app.control("settings.autoSwitchStanding")
        XCTAssertTrue(choice.waitForExistence(timeout: 5))
        XCTAssertTrue(share.exists)
        // Both are held back until the app's preferences have been read.
        XCTAssertTrue(choice.wait(for: \.isEnabled, toEqual: true, timeout: 5))
        XCTAssertTrue(share.isEnabled)
        XCTAssertEqual(choice.value as? String, Words.notifyMe, "a fresh install notifies")
        XCTAssertFalse(standing.exists, "nothing said while it does not switch")

        choose(Words.switchAutomatically, in: choice)
        XCTAssertTrue(standing.waitForExistence(timeout: 5), "what the look came to")
        XCTAssertTrue(app.control("settings.terms").exists, "Is this allowed?")

        let typed = app.textFields["settings.atLimitShareField"]
        XCTAssertTrue(typed.exists, "a field beside the stepper")
        let raise = share.incrementArrows.firstMatch
        XCTAssertTrue(raise.exists, "a stepper shows its arrows")
        raise.click()
        XCTAssertTrue(typed.wait(until: "value == '96%'", timeout: 5))

        typed.doubleClick()
        typed.typeText("80\r")
        XCTAssertTrue(typed.wait(until: "value == '80%'", timeout: 5))

        choose(Words.doNothing, in: choice)
        XCTAssertTrue(standing.waitForNonExistence(timeout: 5))
        XCTAssertTrue(share.waitForNonExistence(timeout: 5), "no share to reach")
        XCTAssertFalse(typed.exists)
    }

    @MainActor
    private func choose(_ title: String, in popUp: XCUIElement) {
        popUp.click()
        popUp.menuItems[title].click()
        XCTAssertTrue(popUp.wait(until: "value == '\(title)'", timeout: 5))
    }

    /// The fixture's personal is on for warm-ups with no five-hour period running, so it
    /// warms up as the app starts.
    @MainActor
    func testWarmingUpAccounts() {
        let app = launch(.warmUp)
        app.openWindow()
        let row = app.accountRow("claude/personal")
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.click()
        let warmed = app.control("inspector").text("BEGINSWITH", Words.warmsUpAgain)
        XCTAssertTrue(warmed.waitForExistence(timeout: 30), "personal is warmed up")

        app.openSettings()
        app.toolbars.buttons[Words.limits].click()
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

    // The fixture has no pitboard of its own, so the pane offers the one inside the app.
    @MainActor
    func testInstallingTheCommandLine() {
        let app = launch(.oneTool)
        app.openSettings()
        app.toolbars.buttons[Words.advanced].click()
        let install = app.buttons["settings.installCommandLine"]
        XCTAssertTrue(install.waitForExistence(timeout: 5))
        install.click()
        XCTAssertTrue(install.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.text("ENDSWITH", "/bin/pitboard").waitForExistence(timeout: 5))
    }

    // The fixture's own app and folders are removed, nothing of the app under test.
    @MainActor
    func testUninstallingPitboardMovesItsCopyToTheTrashAndQuits() {
        let app = launch(.oneTool)
        app.openSettings()
        app.toolbars.buttons[Words.advanced].click()
        let uninstall = app.buttons["settings.uninstall"]
        XCTAssertTrue(uninstall.waitForExistence(timeout: 5))
        uninstall.click()
        XCTAssertTrue(app.alertCancel.waitForExistence(timeout: 5))
        app.alertAnswer.click()
        let trash = app.buttons[Words.moveToTrash]
        XCTAssertTrue(trash.waitForExistence(timeout: 30))
        trash.click()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10))
    }

    // A debug build carries no update key.
    @MainActor
    func testUpdatesSayWhenTheBuildCannotUpdate() {
        let app = launch(.oneTool)
        app.openSettings()
        app.toolbars.buttons[Words.general].click()
        XCTAssertTrue(app.staticTexts["settings.noUpdateKey"].waitForExistence(timeout: 5))
    }
}
