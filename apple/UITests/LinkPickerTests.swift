import XCTest

/// A claude.ai link from outside the app, as a browser, a bookmarklet, the Service or the
/// Share extension hands it over: it asks which account opens it, and opens nothing first.
final class LinkPickerTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    /// A pitboard link from outside asks which account opens it and opens nothing first;
    /// choosing one opens that account's claude.ai window on the fixture's stand-in page.
    @MainActor
    func testAPitboardLinkAsksWhichAccountOpensIt() {
        let app = XCUIApplication.launched(.oneTool)
        app.open(link("https%3A%2F%2Fclaude.ai%2Fpublic%2Fartifacts%2F0e5a"))
        let picker = app.windows["Open claude.ai Link"]
        XCTAssertTrue(picker.waitForExistence(timeout: 10))
        // The fixture's accounts, before anything is clicked: a relaunch outside the fixture
        // would list this Mac's own, and must fail here rather than open claude.ai.
        let personal = picker.descendants(matching: .any)["picker.account.personal"]
        XCTAssertTrue(personal.waitForExistence(timeout: 10))
        XCTAssertTrue(personal.label.contains("dana@home.example"))
        XCTAssertTrue(picker.text("==", "claude.ai/public/artifacts/0e5a").exists)
        for label in ["work", "personal"] {
            XCTAssertFalse(
                app.windows[label].exists, "\(label)'s window opened before a choice")
        }
        personal.click()
        picker.buttons["Open"].click()
        let window = app.windows["personal"]
        XCTAssertTrue(window.waitForExistence(timeout: 10))
        XCTAssertTrue(
            window.webViews.firstMatch.text("==", "claude.ai stand-in")
                .waitForExistence(timeout: 10))
    }

    /// A link to anywhere but claude.ai is refused in the picker, and no window opens.
    @MainActor
    func testALinkOutsideClaudeIsRefused() {
        let app = XCUIApplication.launched(.oneTool)
        app.open(link("https%3A%2F%2Fexample.com%2F"))
        let picker = app.windows["Open claude.ai Link"]
        XCTAssertTrue(picker.waitForExistence(timeout: 10))
        XCTAssertTrue(picker.text("==", "Can’t Open This Link").waitForExistence(timeout: 5))
        XCTAssertFalse(app.windows["work"].exists)
        XCTAssertFalse(app.windows["personal"].exists)
        picker.buttons["OK"].click()
        XCTAssertFalse(picker.waitForExistence(timeout: 2))
    }

    /// With no Claude Code account there is nothing to open the link in, and the picker
    /// offers to add one.
    @MainActor
    func testWithNoClaudeAccountThePickerOffersToAddOne() {
        let app = XCUIApplication.launched(.empty)
        app.open(link("https%3A%2F%2Fclaude.ai%2Fnew"))
        let picker = app.windows["Open claude.ai Link"]
        XCTAssertTrue(picker.waitForExistence(timeout: 10))
        XCTAssertTrue(
            picker.text("==", "No Account Has a claude.ai Window").waitForExistence(timeout: 10)
        )
        XCTAssertTrue(picker.buttons["Add Account…"].exists)
        XCTAssertTrue(picker.buttons["Open in Browser"].exists)
    }

    /// Open claude.ai Link… in pitboard's menu takes a link typed or pasted into the picker.
    /// Typing is not reading the clipboard, which pitboard never does by itself.
    @MainActor
    func testOpenClaudeLinkFromTheMenuTakesAPastedLink() {
        let app = XCUIApplication.launched(.oneTool)
        app.openMenu()
        app.menuItem("Open claude.ai Link…").click()
        let picker = app.windows["Open claude.ai Link"]
        XCTAssertTrue(picker.waitForExistence(timeout: 10))
        let field = picker.textFields["picker.field"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        XCTAssertFalse(picker.buttons["Open"].isEnabled, "nothing to open yet")
        field.click()
        field.typeText("claude.ai/new")
        picker.descendants(matching: .any)["picker.account.work"].click()
        XCTAssertTrue(picker.buttons["Open"].isEnabled)
        picker.buttons["Open"].click()
        XCTAssertTrue(app.windows["work"].waitForExistence(timeout: 10))
    }

    /// A pitboard link for the debug build, which is the one a fixture answers.
    private func link(_ encoded: String) -> URL {
        URL(string: "pitboard-debug://open?url=\(encoded)")!
    }
}
