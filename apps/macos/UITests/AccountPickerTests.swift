import XCTest

final class AccountPickerTests: FixtureTestCase {
    @MainActor
    func testASharedLinkAsksWhichAccountOpensIt() {
        let app = launch(.twoTools)
        app.share("https://chatgpt.com/c/shared")
        let picker = app.picker
        XCTAssertTrue(picker.waitForExistence(timeout: 10))
        XCTAssertTrue(picker.staticTexts["picker.choose"].waitForExistence(timeout: 10))
        // Each row is one element for VoiceOver, its label and email read together.
        let spare = picker.descendants(matching: .any)["picker.account.spare"]
        XCTAssertTrue(spare.waitForExistence(timeout: 5))
        XCTAssertTrue(picker.descendants(matching: .any)["picker.account.main"].exists)
        XCTAssertFalse(
            picker.descendants(matching: .any)["picker.account.work"].exists,
            "only the site's own accounts")
        XCTAssertEqual(app.accountWindows.count, 0, "nothing opens before a choice")

        spare.click()
        // Open waits a moment after the accounts appear, so a stray Return opens nothing.
        let open = picker.buttons["picker.open"]
        XCTAssertTrue(open.wait(for: \.isEnabled, toEqual: true, timeout: 5))
        open.click()
        let window = app.accountWindow("spare")
        XCTAssertTrue(window.waitForExistence(timeout: 10))
        XCTAssertTrue(
            window.webViews.firstMatch.text("CONTAINS", "/c/shared").waitForExistence(
                timeout: 10))
        XCTAssertTrue(picker.waitForNonExistence(timeout: 5))
    }

    @MainActor
    func testALinkToAnotherSiteIsRefused() {
        let app = launch(.twoTools)
        app.share("https://example.com/page")
        let picker = app.picker
        XCTAssertTrue(picker.waitForExistence(timeout: 10))
        XCTAssertTrue(picker.staticTexts["picker.refused"].waitForExistence(timeout: 10))
        XCTAssertTrue(picker.text("CONTAINS", "example.com").exists, "where the link is")
        picker.buttons["picker.ok"].click()
        XCTAssertTrue(picker.waitForNonExistence(timeout: 5))
        XCTAssertEqual(app.accountWindows.count, 0)
    }

    @MainActor
    func testCancellingOpensNothing() {
        let app = launch(.onlyOne)
        app.share("https://claude.ai/new")
        let picker = app.picker
        XCTAssertTrue(picker.staticTexts["picker.choose"].waitForExistence(timeout: 10))
        picker.buttons["picker.cancel"].click()
        XCTAssertTrue(picker.waitForNonExistence(timeout: 5))
        XCTAssertEqual(app.accountWindows.count, 0)
    }

    @MainActor
    func testALinkForASiteWithNoAccountOffersToAddOne() {
        let app = launch(.onlyOne)
        app.share("https://chatgpt.com/")
        let picker = app.picker
        XCTAssertTrue(picker.staticTexts["picker.noAccount"].waitForExistence(timeout: 10))
        XCTAssertTrue(picker.buttons["picker.add"].exists)
        XCTAssertTrue(picker.buttons["picker.inBrowser"].exists)
        picker.buttons["picker.cancel"].click()
    }

    /// Someone else's sign-in link is refused: it would sign the window in as them.
    @MainActor
    func testASignInLinkIsRefused() {
        let app = launch(.onlyOne)
        app.share("https://claude.ai/magic-link#someone")
        let picker = app.picker
        XCTAssertTrue(picker.staticTexts["picker.refused"].waitForExistence(timeout: 10))
        XCTAssertTrue(picker.staticTexts["picker.detail"].exists)
        picker.buttons["picker.ok"].click()
    }
}
