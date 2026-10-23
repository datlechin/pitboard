import AppKit
import XCTest

final class AccountsWindowTests: FixtureTestCase {
    /// The fixture's Claude Code takes a code as the real one does, `<code>#<state>`. The
    /// address it prints is copied for another browser, and Open is never clicked: it would
    /// open that address in a real one.
    @MainActor
    func testAddingAClaudeCodeAccount() {
        let app = launch(.oneTool)
        app.openWindow()
        app.toolbars.buttons["Add Account"].click()
        let name = app.textFields["sheet.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        name.click()
        name.typeText("third")
        app.buttons["Sign In"].click()
        let code = app.textFields["sheet.code"]
        XCTAssertTrue(code.waitForExistence(timeout: 10))
        let sheet = app.sheets.firstMatch
        XCTAssertTrue(sheet.buttons["Open"].isEnabled)
        sheet.buttons["Copy Link"].click()
        XCTAssertEqual(
            NSPasteboard.general.string(forType: .string),
            "https://claude.com/cai/oauth/authorize?code=true&state=pitboard-fixture")
        code.click()
        code.typeText("fixture-code#state")
        app.buttons["Submit Code"].click()
        XCTAssertTrue(app.accountRow("claude/third").waitForExistence(timeout: 10))
        XCTAssertTrue(sheet.waitForNonExistence(timeout: 5))
    }

    /// A Codex sign-in finishes by itself once the browser is done.
    @MainActor
    func testAddingACodexAccount() {
        let app = launch(.twoTools)
        app.openWindow()
        app.toolbars.buttons["Add Account"].click()
        let tool = app.popUpButtons["sheet.tool"]
        XCTAssertTrue(tool.waitForExistence(timeout: 5))
        tool.click()
        // Its own item, not the menu bar item's Codex heading.
        tool.menuItems["Codex"].click()
        let name = app.textFields["sheet.name"]
        name.click()
        name.typeText("third")
        app.buttons["Sign In"].click()
        XCTAssertTrue(app.accountRow("codex/third").waitForExistence(timeout: 10))
    }

    /// A tool Pitboard did not find says what to install, and cannot be signed in to.
    @MainActor
    func testAToolPitboardDidNotFindSaysWhatToInstall() {
        // Claude Code and Claude Desktop, and no Codex CLI.
        let app = launch(.claudeDesktop)
        app.openWindow()
        app.toolbars.buttons["Add Account"].click()
        let tool = app.popUpButtons["sheet.tool"]
        XCTAssertTrue(tool.waitForExistence(timeout: 5))
        tool.click()
        tool.menuItems["Codex"].click()
        let sheet = app.sheets.firstMatch
        XCTAssertTrue(
            sheet.staticTexts["Pitboard can’t find the Codex CLI on this Mac."]
                .waitForExistence(timeout: 5))
        XCTAssertTrue(sheet.descendants(matching: .any)["sheet.missing.link"].exists)
        let name = app.textFields["sheet.name"]
        name.click()
        name.typeText("third")
        XCTAssertFalse(app.buttons["Sign In"].isEnabled)
    }

    /// A name typed with its tool says why under the field, and Sign In waits for a name.
    @MainActor
    func testANameThatCannotBeOneSaysWhy() {
        let app = launch(.oneTool)
        app.openWindow()
        app.toolbars.buttons["Add Account"].click()
        let name = app.textFields["sheet.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        name.click()
        name.typeText("claude/third")
        let problem = app.staticTexts["A name can’t contain “/”. Choose the tool above."]
        XCTAssertTrue(problem.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Sign In"].isEnabled)
        name.typeKey("a", modifierFlags: .command)
        name.typeText("third")
        XCTAssertTrue(problem.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.buttons["Sign In"].isEnabled)
    }

    /// Sign In stays disabled while a sign-in runs, so a second sign-in that starts and
    /// finishes is what shows the first one was stopped.
    @MainActor
    func testCancellingASignInAddsNothing() {
        let app = launch(.oneTool)
        app.openWindow()
        app.toolbars.buttons["Add Account"].click()
        let name = app.textFields["sheet.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        name.click()
        name.typeText("third")
        app.buttons["Sign In"].click()
        XCTAssertTrue(app.textFields["sheet.code"].waitForExistence(timeout: 10))
        app.sheets.firstMatch.buttons["Cancel"].click()
        XCTAssertTrue(app.sheets.firstMatch.waitForNonExistence(timeout: 5))
        XCTAssertFalse(app.accountRow("claude/third").exists)

        app.toolbars.buttons["Add Account"].click()
        let again = app.textFields["sheet.name"]
        XCTAssertTrue(again.waitForExistence(timeout: 5))
        again.click()
        again.typeText("third")
        app.buttons["Sign In"].click()
        let code = app.textFields["sheet.code"]
        XCTAssertTrue(code.waitForExistence(timeout: 10))
        code.click()
        code.typeText("fixture-code#state")
        app.buttons["Submit Code"].click()
        XCTAssertTrue(app.accountRow("claude/third").waitForExistence(timeout: 10))
    }

    @MainActor
    func testRenamingAnAccount() {
        let app = launch(.oneTool)
        app.openWindow()
        let row = app.accountRow("claude/personal")
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.rightClick()
        app.shortcutMenu.menuItems["Rename…"].click()
        let name = app.textFields["sheet.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        name.click()
        name.typeKey("a", modifierFlags: .command)
        name.typeText("work")
        app.buttons["Rename"].click()
        let failure = app.staticTexts["Couldn’t rename personal"]
        XCTAssertTrue(failure.waitForExistence(timeout: 5))
        name.typeKey("a", modifierFlags: .command)
        name.typeText("home")
        XCTAssertTrue(
            failure.waitForNonExistence(timeout: 5), "it was about the name typed before")
        app.buttons["Rename"].click()
        XCTAssertTrue(app.accountRow("claude/home").waitForExistence(timeout: 5))
    }

    @MainActor
    func testForgettingAsksFirst() {
        let app = launch(.oneTool)
        app.openWindow()
        let row = app.accountRow("claude/personal")
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.rightClick()
        app.shortcutMenu.menuItems["Forget…"].click()
        XCTAssertTrue(app.alert.buttons["Forget"].waitForExistence(timeout: 5))
        app.alert.buttons["Cancel"].click()
        XCTAssertTrue(row.exists)
        row.rightClick()
        app.shortcutMenu.menuItems["Forget…"].click()
        app.alert.buttons["Forget"].click()
        XCTAssertTrue(row.waitForNonExistence(timeout: 5))
    }

    @MainActor
    func testTheAccountInUseOffersNoForget() {
        let app = launch(.oneTool)
        app.openWindow()
        let row = app.accountRow("claude/work")
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.rightClick()
        XCTAssertTrue(app.shortcutMenu.menuItems["Rename…"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.shortcutMenu.menuItems["Forget…"].exists)
        app.typeKey(.escape, modifierFlags: [])
    }

    @MainActor
    func testTheAccountMenuActsOnTheSelectedAccount() {
        let app = launch(.oneTool)
        app.openWindow()
        let row = app.accountRow("claude/personal")
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.click()
        app.accountMenu.click()
        XCTAssertTrue(app.accountMenu.menuItems["Use personal"].waitForExistence(timeout: 5))
        app.accountMenu.menuItems["Rename…"].click()
        XCTAssertTrue(app.textFields["sheet.name"].waitForExistence(timeout: 5))
        app.sheets.firstMatch.buttons["Cancel"].click()
        XCTAssertTrue(app.sheets.firstMatch.waitForNonExistence(timeout: 5))

        row.click()
        app.typeKey("c", modifierFlags: .command)
        XCTAssertEqual(NSPasteboard.general.string(forType: .string), "dana@home.example")
        app.typeKey(.delete, modifierFlags: .command)
        XCTAssertTrue(app.alert.buttons["Forget"].waitForExistence(timeout: 5))
        app.alert.buttons["Cancel"].click()
        XCTAssertTrue(row.exists)
    }

    // personal warms up at launch, so its period runs and Warm Up Now cannot be chosen.
    @MainActor
    func testWarmUpNowIsLeftOutOfTheShortcutMenuAndDimmedInTheAccountMenu() {
        let app = launch(.warmUp)
        app.openWindow()
        XCTAssertTrue(
            app.text("BEGINSWITH", "Warms up again at ").waitForExistence(timeout: 30))
        let row = app.accountRow("claude/personal")
        row.rightClick()
        XCTAssertTrue(app.shortcutMenu.menuItems["Rename…"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.shortcutMenu.menuItems["Warm Up Now"].exists)
        app.typeKey(.escape, modifierFlags: [])

        row.click()
        app.accountMenu.click()
        let now = app.accountMenu.menuItems["Warm Up Now"]
        XCTAssertTrue(now.waitForExistence(timeout: 5))
        XCTAssertFalse(now.isEnabled)
        app.typeKey(.escape, modifierFlags: [])
    }

    @MainActor
    func testUsingACodexAccountSaysSessionsKeepTheOldOne() {
        let app = launch(.twoTools)
        app.openWindow()
        // With a tip above them, spare is scrolled to and the notice goes out of sight.
        app.putTheTipsAway()
        let spare = app.accountRow("codex/spare")
        XCTAssertTrue(spare.waitForExistence(timeout: 5))
        // Named with its tool, since Claude Code could have a `spare` too.
        spare.buttons["Use spare (Codex)"].click()
        let notice = app.descendants(matching: .any)["notice.switch/codex"]
        XCTAssertTrue(notice.waitForExistence(timeout: 5))
        let restart = "Any codex session started before this switch keeps using main"
        XCTAssertTrue(notice.text("BEGINSWITH", restart).exists)
        notice.buttons["Dismiss"].click()
        XCTAssertTrue(notice.waitForNonExistence(timeout: 5))
    }

    /// The fixture's ChatGPT is a stand-in: no real app is quit.
    @MainActor
    func testUsingACodexAccountAsksToQuitChatGPTFirst() {
        let app = launch(.chatGPTOpen)
        app.openWindow()
        let spare = app.accountRow("codex/spare")
        XCTAssertTrue(spare.waitForExistence(timeout: 5))
        let notice = app.descendants(matching: .any)["notice.switch/codex"]

        spare.buttons["Use spare (Codex)"].click()
        XCTAssertTrue(app.alert.buttons["Cancel"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.alert.staticTexts["Quit ChatGPT to switch?"].exists)
        app.alert.buttons["Cancel"].click()
        XCTAssertTrue(app.alert.waitForNonExistence(timeout: 5))
        XCTAssertFalse(notice.exists, "nothing was switched")

        spare.buttons["Use spare (Codex)"].click()
        XCTAssertTrue(
            app.alert.buttons["Quit ChatGPT and Switch"].waitForExistence(timeout: 5))
        app.alert.buttons["Quit ChatGPT and Switch"].click()
        XCTAssertTrue(notice.waitForExistence(timeout: 5))
    }

    @MainActor
    func testNamingTheAccountInUse() {
        let app = launch(.unnamed)
        app.openWindow()
        XCTAssertTrue(app.staticTexts["Give this account a name"].waitForExistence(timeout: 5))
        app.buttons["Name…"].firstMatch.click()
        let name = app.textFields["sheet.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        XCTAssertEqual(name.value as? String, "work", "the name dana@work.example suggests")
        app.buttons["Save"].click()
        XCTAssertTrue(app.accountRow("claude/work").waitForExistence(timeout: 5))
    }

    /// With Codex installed, after the tip to use Pitboard with it.
    @MainActor
    func testAnEmptyMachineOffersAnAccount() {
        let app = launch(.empty)
        app.openWindow()
        XCTAssertTrue(app.staticTexts["Use Pitboard with Codex"].waitForExistence(timeout: 5))
        app.buttons["Not Now"].click()
        XCTAssertTrue(app.staticTexts["No Accounts"].waitForExistence(timeout: 5))
        app.buttons["Add Account…"].click()
        XCTAssertTrue(app.textFields["sheet.name"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testGivingUpOnAnInterruptedSwitch() {
        let app = launch(.stuck)
        app.openWindow()
        let giveUp = app.buttons["Give Up…"]
        XCTAssertTrue(giveUp.waitForExistence(timeout: 5))
        giveUp.click()
        app.alert.buttons["Give Up"].click()
        let notice = app.descendants(matching: .any)["notice.abandoned"]
        XCTAssertTrue(notice.waitForExistence(timeout: 5))
        XCTAssertTrue(notice.staticTexts["Gave up on the interrupted switch"].exists)
        XCTAssertTrue(notice.text("CONTAINS", "2 logins kept").exists)
    }

    @MainActor
    func testOneAccountIsOfferedASecondUntilNotNow() {
        let app = launch(.onlyOne)
        app.openWindow()
        let tip = app.staticTexts["Add a second account"]
        XCTAssertTrue(tip.waitForExistence(timeout: 5))
        app.buttons["Not Now"].click()
        XCTAssertTrue(tip.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.accountRow("claude/work").exists)
    }

    /// The window goes to the accounts, where the new one is listed.
    @MainActor
    func testCommandNAddsAnAccountFromAnyPane() {
        let app = launch(.oneTool)
        app.openWindow()
        app.descendants(matching: .any)["sidebar.activity"].click()
        let activity = app.text("Switch")
        XCTAssertTrue(activity.waitForExistence(timeout: 5))
        app.typeKey("n", modifierFlags: .command)
        XCTAssertTrue(app.textFields["sheet.name"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Add Account"].exists)
        app.sheets.firstMatch.buttons["Cancel"].click()
        XCTAssertTrue(app.sheets.firstMatch.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.accountRow("claude/work").waitForExistence(timeout: 5))
        XCTAssertTrue(activity.waitForNonExistence(timeout: 5))
    }

    @MainActor
    func testTheFirstLaunchOpensTheWindow() {
        let app = launch(.firstLaunch)
        XCTAssertTrue(app.staticTexts["Use Pitboard with Codex"].waitForExistence(timeout: 10))
    }
}
