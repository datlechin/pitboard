import XCTest

final class MenuBarTests: FixtureTestCase {
    @MainActor
    func testTheMenuListsEveryAccountUnderItsTool() {
        let app = launch(.twoTools)
        app.openMenu()
        for title in [
            "Claude Code", "Codex", "work", "personal", "old", "main", "spare",
        ] {
            XCTAssertTrue(app.menuItem(title).waitForExistence(timeout: 5), title)
        }
        for command in [
            Words.addAccount, Words.refresh, Words.openPitboard, Words.settings, Words.quit,
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
        app.menuItem(Words.openPitboard).click()
        let row = app.accountRow("claude/personal")
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        XCTAssertTrue(
            row.descendants(matching: .any)["account.inUse"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testAnExpiredAccountOpensSigningInAgain() {
        let app = launch(.twoTools)
        app.openMenu()
        app.menuItem("old").click()
        XCTAssertTrue(app.control("sheet.signInAgain").waitForExistence(timeout: 5))
    }

    @MainActor
    func testAddAccountOpensTheSheet() {
        let app = launch(.oneTool)
        app.openMenu()
        app.menuItem(Words.addAccount).click()
        XCTAssertTrue(app.control("sheet.add").waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["sheet.confirm"].exists)
    }

    // Command-0 is Actual Size, as in Safari.
    @MainActor
    func testCommandZeroDoesNotOpenPitboard() {
        let app = launch(.oneTool)
        app.openMenu()
        XCTAssertTrue(app.menuItem(Words.openPitboard).waitForExistence(timeout: 5))
        app.typeKey("0", modifierFlags: .command)
        XCTAssertFalse(app.windows.firstMatch.waitForExistence(timeout: 2))
        app.typeKey(.escape, modifierFlags: [])
    }

    @MainActor
    func testSettingsOpens() {
        let app = launch(.oneTool)
        app.openMenu()
        app.menuItem(Words.settings).click()
        XCTAssertTrue(app.windows[Words.general].waitForExistence(timeout: 5))
    }

    @MainActor
    func testAMachineWithoutClaudeCodeSaysSo() {
        let app = launch(.noClaudeCode)
        app.openMenu()
        XCTAssertTrue(app.menuItem(Words.claudeCodeMissing).waitForExistence(timeout: 5))
    }

    /// The fixture's account list cannot be read, so there is no account to list.
    @MainActor
    func testAFailedReadIsSaidInTheMenuAndInTheWindow() {
        let app = launch(.readFailure)
        app.openMenu()
        let item = app.menuItem(Words.oneIssue)
        XCTAssertTrue(item.waitForExistence(timeout: 5))
        item.click()
        XCTAssertTrue(app.control("accounts.readFailed").waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["accounts.retry"].exists)
        XCTAssertFalse(app.accountRow("claude/work").exists)
    }

    /// With the accounts closed and Diagnostics in front, the notice's item opens the accounts
    /// on it.
    @MainActor
    func testShowingANoticeOpensTheAccounts() {
        let app = launch(.stuck)
        app.openWindow()
        app.openFromWindowMenu(Words.diagnostics)
        let diagnostics = app.windows["diagnostics"]
        XCTAssertTrue(diagnostics.waitForExistence(timeout: 5))
        app.accountsWindow.buttons[XCUIIdentifierCloseWindow].click()
        XCTAssertTrue(app.accountsWindow.waitForNonExistence(timeout: 5))

        app.openMenu()
        let item = app.menuItem(Words.oneIssue)
        XCTAssertTrue(item.waitForExistence(timeout: 5))
        item.click()
        XCTAssertTrue(
            app.accountsWindow.buttons["notice.action.abandonStuckSwitch"].waitForExistence(
                timeout: 5))
        XCTAssertTrue(diagnostics.exists)
    }

    @MainActor
    func testQuitQuits() {
        let app = launch(.oneTool)
        app.openMenu()
        app.menuItem(Words.quit).click()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 5))
    }
}
