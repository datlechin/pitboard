import XCTest

/// In a fixture an account's window loads a stand-in page, so nothing reaches either site.
final class AccountWindowTests: FixtureTestCase {
    @MainActor
    func testOpeningAClaudeAccountsWindowFromTheMenu() {
        let app = launch(.onlyOne)
        app.openMenu()
        app.menuItem(Words.openSite("claude.ai", as: "work")).click()
        let window = app.accountWindow("work")
        XCTAssertTrue(window.waitForExistence(timeout: 10))
        XCTAssertTrue(
            page(of: window).text("==", "claude.ai stand-in").waitForExistence(timeout: 10))
    }

    @MainActor
    func testEachSiteOffersItsOwnToolsAccounts() {
        let app = launch(.twoTools)
        app.openMenu()
        let claude = app.menuItem(Words.openSite("claude.ai"))
        XCTAssertTrue(claude.waitForExistence(timeout: 5))
        claude.hover()
        for label in ["work", "personal", "old"] {
            XCTAssertTrue(claude.menuItems[label].waitForExistence(timeout: 5), label)
        }
        XCTAssertFalse(claude.menuItems["main"].exists)
        let chatGPT = app.menuItem(Words.openSite("chatgpt.com"))
        XCTAssertTrue(chatGPT.exists)
        chatGPT.hover()
        for label in ["main", "spare"] {
            XCTAssertTrue(chatGPT.menuItems[label].waitForExistence(timeout: 5), label)
        }
        XCTAssertFalse(chatGPT.menuItems["work"].exists)
        app.typeKey(.escape, modifierFlags: [])
    }

    @MainActor
    func testNoAccountOffersNoWindow() {
        let app = launch(.empty)
        app.openMenu()
        XCTAssertTrue(app.menuItem(Words.openPitboard).waitForExistence(timeout: 5))
        XCTAssertFalse(app.menuItem(Words.openSite("claude.ai")).exists)
        XCTAssertFalse(app.menuItem(Words.openSite("chatgpt.com")).exists)
        app.typeKey(.escape, modifierFlags: [])
    }

    @MainActor
    func testOpeningAWindowAgainBringsTheSameOneForward() {
        let app = openChatGPT()
        app.openMenu()
        let open = app.menuItem(Words.openSite("chatgpt.com"))
        open.hover()
        open.menuItems["main"].click()
        XCTAssertTrue(app.accountWindow("main").waitForExistence(timeout: 5))
        XCTAssertEqual(app.accountWindows.count, 1)
    }

    /// WebKit asks about a sign-in link that wants a new window before it asks for the
    /// window, and refusing it there would open none.
    @MainActor
    func testASignInLinkOpensASignInWindowThatClosesItself() {
        let app = openChatGPT()
        let window = app.accountWindow("main")
        page(of: window).links["Continue with appleid.apple.com"].click()
        let signIn = app.signInWindow
        XCTAssertTrue(signIn.waitForExistence(timeout: 10))
        let done = signIn.webViews.firstMatch.buttons["Done"]
        XCTAssertTrue(done.waitForExistence(timeout: 10))
        done.click()
        XCTAssertTrue(signIn.waitForNonExistence(timeout: 10))
        XCTAssertTrue(page(of: window).text("==", "chatgpt.com stand-in").exists)
    }

    @MainActor
    func testASignInScriptOpensASignInWindow() {
        let app = openChatGPT()
        page(of: app.accountWindow("main")).buttons["Sign in in a window"].click()
        XCTAssertTrue(app.signInWindow.waitForExistence(timeout: 10))
        let done = app.signInWindow.webViews.firstMatch.buttons["Done"]
        XCTAssertTrue(done.waitForExistence(timeout: 10))
        done.click()
        XCTAssertTrue(app.signInWindow.waitForNonExistence(timeout: 10))
    }

    @MainActor
    func testClosingTheAccountsWindowClosesItsSignInWindow() {
        let app = openChatGPT()
        let window = app.accountWindow("main")
        page(of: window).links["Continue with appleid.apple.com"].click()
        XCTAssertTrue(app.signInWindow.waitForExistence(timeout: 10))
        window.buttons[XCUIIdentifierCloseWindow].click()
        XCTAssertTrue(window.waitForNonExistence(timeout: 10))
        XCTAssertTrue(app.signInWindow.waitForNonExistence(timeout: 10))
    }

    @MainActor
    func testGoogleSignInIsStoppedInTheWindow() {
        let app = openChatGPT()
        let window = app.accountWindow("main")
        page(of: window).links["Continue with Google"].click()
        let note = window.descendants(matching: .any)["window.note"]
        XCTAssertTrue(
            note.staticTexts["window.note.googleRefused"].waitForExistence(timeout: 5))
        XCTAssertTrue(page(of: window).text("==", "chatgpt.com stand-in").exists)
    }

    @MainActor
    func testASiteDownloadIsListedInTheWindow() {
        let app = openChatGPT()
        let window = app.accountWindow("main")
        page(of: window).links["Download notes"].click()
        let downloads = window.toolbars.buttons["window.downloads"]
        XCTAssertTrue(downloads.waitForExistence(timeout: 10))
        downloads.click()
        // A row is one element for VoiceOver, its name and state read together.
        let row = app.descendants(matching: .any)["download"]
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        XCTAssertTrue(
            row.wait(
                until: "label CONTAINS 'notes.txt' AND label CONTAINS '\(Words.downloaded)'",
                timeout: 10))
    }

    @MainActor
    func testFindShowsTheFindBar() {
        let app = openChatGPT()
        let window = app.accountWindow("main")
        page(of: window).click()
        app.typeKey("f", modifierFlags: .command)
        XCTAssertTrue(window.searchFields.firstMatch.waitForExistence(timeout: 5))
    }

    @MainActor
    func testRemovingWebsiteDataAsksFirstAndStartsOver() {
        let app = openChatGPT()
        let window = app.accountWindow("main")
        let note = window.descendants(matching: .any)["window.note"]
        // The first opening says how to sign in; put it away, and leave the home page.
        note.buttons["window.note.dismiss"].click()
        XCTAssertTrue(note.waitForNonExistence(timeout: 5))
        page(of: window).links["A chat"].click()
        XCTAssertTrue(
            page(of: window).text("BEGINSWITH", "A Pitboard fixture page at /chat/fixture")
                .waitForExistence(timeout: 10))

        let file = app.menuBars.menuBarItems[Words.fileMenu]
        file.click()
        file.menuItems[Words.removeWebsiteData].click()
        // AppKit names an alert's answer action-button-1 (docs/facts/macos-app.md).
        let remove = window.sheets.firstMatch.buttons["action-button-1"]
        XCTAssertTrue(remove.waitForExistence(timeout: 5))
        remove.click()
        XCTAssertTrue(note.staticTexts["window.note.signIn"].waitForExistence(timeout: 10))
        XCTAssertTrue(
            page(of: window).text("BEGINSWITH", "A Pitboard fixture page at /.")
                .waitForExistence(timeout: 10))
    }

    @MainActor
    func testAnAccountsShortcutMenuOpensItsWindow() {
        let app = launch(.twoTools)
        app.openWindow()
        let row = app.accountRow("codex/spare")
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.rightClick()
        app.shortcutMenu.menuItems[Words.openSite("chatgpt.com")].click()
        XCTAssertTrue(app.accountWindow("spare").waitForExistence(timeout: 10))
    }

    @MainActor
    private func openChatGPT() -> XCUIApplication {
        let app = launch(.twoTools)
        app.openMenu()
        let open = app.menuItem(Words.openSite("chatgpt.com"))
        XCTAssertTrue(open.waitForExistence(timeout: 5))
        open.hover()
        open.menuItems["main"].click()
        let window = app.accountWindow("main")
        XCTAssertTrue(window.waitForExistence(timeout: 10))
        XCTAssertTrue(
            page(of: window).text("==", "chatgpt.com stand-in").waitForExistence(timeout: 10))
        return app
    }

    @MainActor
    private func page(of window: XCUIElement) -> XCUIElement {
        window.webViews.firstMatch
    }
}
