import XCTest

/// A Claude Code account's claude.ai window. In a fixture it loads the stand-in page, so
/// nothing reaches claude.ai.
final class ClaudeWindowTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// A Claude Code account's claude.ai window opens titled with its label, on the fixture's
    /// stand-in page, and nothing reaches claude.ai.
    @MainActor
    func testOpeningClaudeForAnAccountShowsItsOwnWindow() {
        let app = XCUIApplication.launched(.onlyOne)
        app.openMenu()
        app.menuItem("Open claude.ai as work").click()
        let window = app.claudeWindow("work")
        XCTAssertTrue(window.waitForExistence(timeout: 10))
        XCTAssertTrue(
            window.webViews.firstMatch.text("==", "claude.ai stand-in")
                .waitForExistence(timeout: 10))
    }

    /// Google's sign-in is stopped in the window, before any request leaves it, and the
    /// window says so and stays on its page. A main frame taken for a frame would load it.
    @MainActor
    func testGoogleSignInIsStoppedInTheWindow() {
        let app = XCUIApplication.launched(.onlyOne)
        app.openMenu()
        app.menuItem("Open claude.ai as work").click()
        let window = app.claudeWindow("work")
        XCTAssertTrue(window.waitForExistence(timeout: 10))
        let page = window.webViews.firstMatch
        XCTAssertTrue(page.text("==", "claude.ai stand-in").waitForExistence(timeout: 10))
        let google = page.links["Continue with Google"]
        XCTAssertTrue(google.waitForExistence(timeout: 5))
        google.click()
        let note = window.descendants(matching: .any)["claude.note"]
        XCTAssertTrue(note.text("CONTAINS", "pitboard stopped it").waitForExistence(timeout: 5))
        XCTAssertTrue(page.text("==", "claude.ai stand-in").exists)
    }

    /// While a claude.ai window is open the app has its menus, and the Window menu does not
    /// offer the account picker, which opens only for a link.
    @MainActor
    func testTheWindowMenuDoesNotOfferTheAccountPicker() {
        let app = XCUIApplication.launched(.onlyOne)
        app.openMenu()
        app.menuItem("Open claude.ai as work").click()
        XCTAssertTrue(app.claudeWindow("work").waitForExistence(timeout: 10))
        let menu = app.menuBars.menuBarItems["Window"]
        XCTAssertTrue(menu.waitForExistence(timeout: 10))
        menu.click()
        XCTAssertTrue(menu.menuItems["Minimize"].waitForExistence(timeout: 5))
        XCTAssertFalse(menu.menuItems["Open claude.ai Link"].exists)
        app.typeKey(.escape, modifierFlags: [])
    }

    /// Only Claude Code accounts have a claude.ai window: a Codex account's is not offered,
    /// and with several accounts the menu offers each by its label.
    @MainActor
    func testTheMenuOffersAWindowForEachClaudeCodeAccount() {
        let app = XCUIApplication.launched(.twoTools)
        app.openMenu()
        let open = app.menuItem("Open claude.ai")
        XCTAssertTrue(open.waitForExistence(timeout: 5))
        open.hover()
        for label in ["work", "personal", "old"] {
            XCTAssertTrue(open.menuItems[label].waitForExistence(timeout: 5), label)
        }
        XCTAssertFalse(open.menuItems["main"].exists)
        XCTAssertFalse(open.menuItems["spare"].exists)
        XCTAssertTrue(app.menuItem("Open claude.ai Link…").exists)
    }

    /// With no Claude Code account there is no claude.ai window to offer.
    @MainActor
    func testNoClaudeCodeAccountOffersNoWindow() {
        let app = XCUIApplication.launched(.empty)
        app.openMenu()
        XCTAssertTrue(app.menuItem("Open pitboard").waitForExistence(timeout: 5))
        XCTAssertFalse(app.menuItem("Open claude.ai").exists)
        XCTAssertFalse(app.menuItem("Open claude.ai Link…").exists)
    }
}
