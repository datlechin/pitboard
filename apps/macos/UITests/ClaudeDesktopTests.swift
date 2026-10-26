import XCTest

// The fixture's Claude is a stand-in: no real app is quit or opened.
final class ClaudeDesktopTests: FixtureTestCase {
    @MainActor
    func testClaudeDesktopAccountsAreListedUnderTheirTool() throws {
        let app = launch(.claudeDesktop)
        app.openMenu()
        XCTAssertTrue(app.menuItem("Claude Desktop").waitForExistence(timeout: 5))
        // Claude Code has a work of its own, in the section above.
        let titles = app.statusItems.firstMatch.menuItems.allElementsBoundByIndex.map(\.title)
        let heading = try XCTUnwrap(titles.firstIndex(of: "Claude Desktop"))
        XCTAssertEqual(Set(titles.dropFirst(heading + 1).prefix(2)), ["work", "home"])
        app.typeKey(.escape, modifierFlags: [])

        app.openWindow()
        let home = app.accountRow("desktop/home")
        XCTAssertTrue(home.waitForExistence(timeout: 5))
        XCTAssertTrue(home.buttons["account.action"].exists)
    }

    @MainActor
    func testUsingAClaudeDesktopAccountWithClaudeClosedSwitchesAtOnce() {
        let app = launch(.claudeDesktop)
        app.openWindow()
        let home = app.accountRow("desktop/home")
        XCTAssertTrue(home.waitForExistence(timeout: 5))
        home.buttons["account.action"].click()
        let notice = app.descendants(matching: .any)["notice.switch/desktop"]
        XCTAssertTrue(notice.waitForExistence(timeout: 10))
        XCTAssertFalse(app.alert.exists, "Claude was closed, so nothing asked")
    }

    @MainActor
    func testUsingAClaudeDesktopAccountAsksToQuitClaudeFirst() {
        let app = launch(.claudeDesktopOpen)
        app.openWindow()
        let home = app.accountRow("desktop/home")
        XCTAssertTrue(home.waitForExistence(timeout: 5))
        let notice = app.descendants(matching: .any)["notice.switch/desktop"]

        home.buttons["account.action"].click()
        XCTAssertTrue(app.alertCancel.waitForExistence(timeout: 5))
        app.alertCancel.click()
        XCTAssertTrue(app.alert.waitForNonExistence(timeout: 5))
        XCTAssertFalse(notice.exists, "nothing was switched")

        home.buttons["account.action"].click()
        XCTAssertTrue(app.alertAnswer.waitForExistence(timeout: 5))
        app.alertAnswer.click()
        XCTAssertTrue(notice.waitForExistence(timeout: 10))
    }

    @MainActor
    func testAddingAClaudeDesktopAccountAsksToQuitClaudeOverTheSheet() {
        let app = launch(.claudeDesktopOpen)
        app.openWindow()
        startAdding(app)

        let park = app.buttons["sheet.confirm"]
        park.click()
        XCTAssertTrue(app.alertCancel.waitForExistence(timeout: 5))
        XCTAssertTrue(app.alertAnswer.exists)
        XCTAssertTrue(app.control("sheet.add").exists, "asked over the sheet, which stays")
        app.alertCancel.click()
        XCTAssertTrue(app.alert.waitForNonExistence(timeout: 5))

        XCTAssertTrue(park.wait(for: \.isEnabled, toEqual: true, timeout: 5))
        park.click()
        XCTAssertTrue(app.alertAnswer.waitForExistence(timeout: 5))
        app.alertAnswer.click()
        let waiting = app.sheets.firstMatch.buttons["sheet.undo"]
        XCTAssertTrue(waiting.waitForExistence(timeout: 10))
        // The fixture's Claude signs in to travel and quits 7 seconds after it is opened. A
        // Claude Desktop account has no email, so it is added as desktop and named next.
        let added = app.accountRow("desktop/desktop")
        XCTAssertTrue(added.waitForExistence(timeout: 30))
        XCTAssertTrue(
            added.descendants(matching: .any)["account.inUse"].waitForExistence(timeout: 5))
        let name = app.textFields["sheet.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        name.click()
        name.typeKey("a", modifierFlags: .command)
        name.typeText("travel")
        app.buttons["sheet.confirm"].click()
        XCTAssertTrue(app.accountRow("desktop/travel").waitForExistence(timeout: 10))
        XCTAssertTrue(app.sheets.firstMatch.waitForNonExistence(timeout: 5))
    }

    @MainActor
    func testAnAddLeftHalfwayOffersToPutTheParkedAccountBack() {
        let app = launch(.claudeDesktop)
        app.openWindow()
        startAdding(app)

        app.buttons["sheet.confirm"].click()
        let waiting = app.sheets.firstMatch.buttons["sheet.undo"]
        XCTAssertTrue(waiting.waitForExistence(timeout: 10))
        XCTAssertFalse(app.alert.exists, "Claude was closed, so nothing asked")
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(waiting.exists, "the waiting page stays")

        app.sheets.firstMatch.buttons["sheet.cancel"].click()
        XCTAssertTrue(app.sheets.firstMatch.waitForNonExistence(timeout: 5))
        let notice = app.descendants(matching: .any)["notice.awaiting/desktop"]
        XCTAssertTrue(notice.waitForExistence(timeout: 5))
        XCTAssertTrue(notice.buttons["notice.action.add"].exists)
        let putBack = notice.buttons["notice.action.switchTo"]
        XCTAssertTrue(putBack.exists)

        putBack.click()
        let work = app.accountRow("desktop/work")
        XCTAssertTrue(
            work.descendants(matching: .any)["account.inUse"].waitForExistence(timeout: 10))
        XCTAssertTrue(notice.waitForNonExistence(timeout: 5))
    }

    @MainActor
    private func startAdding(_ app: XCUIApplication) {
        app.toolbars.buttons["toolbar.addAccount"].click()
        let tool = app.popUpButtons["sheet.tool"]
        XCTAssertTrue(tool.waitForExistence(timeout: 5))
        tool.click()
        // Its own item, not the menu bar item's Claude Desktop heading.
        tool.menuItems["Claude Desktop"].click()
    }
}
