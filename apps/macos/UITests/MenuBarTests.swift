import XCTest

final class MenuBarTests: FixtureTestCase {
    @MainActor
    func testTheMenuListsEveryAccountUnderItsTool() {
        let app = launch(.twoTools)
        app.openMenu()
        for title in [
            "Claude Code", "Codex", "work", "personal", "old", "main · Pro", "spare · Pro",
        ] {
            XCTAssertTrue(app.menuItem(title).waitForExistence(timeout: 5), title)
        }
        for command in [
            "Add Account…", "Refresh", "Open Pitboard", "Settings…", "Quit Pitboard",
        ] {
            XCTAssertTrue(app.menuItem(command).exists, command)
        }
    }

    @MainActor
    func testChoosingAnotherAccountSwitchesToIt() {
        let app = launch(.oneTool)
        app.openMenu()
        app.menuItem("personal").click()
        app.openMenu()
        app.menuItem("Open Pitboard").click()
        let row = app.accountRow("claude/personal")
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        XCTAssertTrue(row.staticTexts["In Use"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testAnExpiredAccountOpensSigningInAgain() {
        let app = launch(.twoTools)
        app.openMenu()
        app.menuItem("old").click()
        XCTAssertTrue(app.staticTexts["Sign In to old Again"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testAddAccountOpensTheSheet() {
        let app = launch(.oneTool)
        app.openMenu()
        app.menuItem("Add Account…").click()
        XCTAssertTrue(app.staticTexts["Add Account"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.textFields["sheet.name"].exists)
    }

    // Command-0 is Actual Size, as in Safari.
    @MainActor
    func testCommandZeroDoesNotOpenPitboard() {
        let app = launch(.oneTool)
        app.openMenu()
        XCTAssertTrue(app.menuItem("Open Pitboard").waitForExistence(timeout: 5))
        app.typeKey("0", modifierFlags: .command)
        XCTAssertFalse(app.windows.firstMatch.waitForExistence(timeout: 2))
        app.typeKey(.escape, modifierFlags: [])
    }

    @MainActor
    func testSettingsOpens() {
        let app = launch(.oneTool)
        app.openMenu()
        app.menuItem("Settings…").click()
        XCTAssertTrue(app.windows["General"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testAMachineWithoutClaudeCodeSaysSo() {
        let app = launch(.noClaudeCode)
        app.openMenu()
        XCTAssertTrue(app.menuItem("Claude Code isn’t installed").waitForExistence(timeout: 5))
    }

    /// The fixture's account list cannot be read, so there is no account to list.
    @MainActor
    func testAFailedReadIsSaidInTheMenuAndInTheWindow() {
        let app = launch(.readFailure)
        app.openMenu()
        let item = app.menuItem("Couldn’t read usage")
        XCTAssertTrue(item.waitForExistence(timeout: 5))
        item.click()
        XCTAssertTrue(app.staticTexts["Couldn’t Read Accounts"].waitForExistence(timeout: 5))
        XCTAssertTrue(
            app.text("BEGINSWITH", "Couldn’t read Pitboard’s account list at ").exists)
        XCTAssertTrue(app.buttons["Try Again"].exists)
        XCTAssertFalse(app.accountRow("claude/work").exists)
    }

    /// The window is left on another pane first: the notice's item opens it on the accounts
    /// whichever pane it was on.
    @MainActor
    func testShowingANoticeOpensTheWindowOnTheAccounts() {
        let app = launch(.stuck)
        app.openWindow()
        app.descendants(matching: .any)["sidebar.machine"].click()
        let checks = app.staticTexts["Interrupted switch"]
        XCTAssertTrue(checks.waitForExistence(timeout: 5))

        app.openMenu()
        let item = app.menuItem("An interrupted switch is waiting")
        XCTAssertTrue(item.waitForExistence(timeout: 5))
        item.click()
        XCTAssertTrue(app.buttons["Give Up…"].waitForExistence(timeout: 5))
        XCTAssertTrue(checks.waitForNonExistence(timeout: 5))
    }

    @MainActor
    func testQuitQuits() {
        let app = launch(.oneTool)
        app.openMenu()
        app.menuItem("Quit Pitboard").click()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }
}
