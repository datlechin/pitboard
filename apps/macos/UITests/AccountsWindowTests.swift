import AppKit
import XCTest

final class AccountsWindowTests: FixtureTestCase {
    /// The fixture's Claude Code takes a code as the real one does, `<code>#<state>`. The
    /// address it prints is copied for another browser, and Open is never clicked: it would
    /// open that address in a real one. The account is added as the sign-in finishes, under
    /// the name its email suggests, and named next.
    @MainActor
    func testAddingAClaudeCodeAccount() {
        let app = launch(.oneTool)
        app.openWindow()
        app.toolbars.buttons["toolbar.addAccount"].click()
        let signIn = app.buttons["sheet.confirm"]
        XCTAssertTrue(signIn.waitForExistence(timeout: 5))
        signIn.click()
        let code = app.textFields["sheet.code"]
        XCTAssertTrue(code.waitForExistence(timeout: 10))
        let sheet = app.sheets.firstMatch
        XCTAssertTrue(sheet.buttons["sheet.openLink"].isEnabled)
        sheet.buttons["sheet.copyLink"].click()
        XCTAssertEqual(
            NSPasteboard.general.string(forType: .string),
            "https://claude.com/cai/oauth/authorize?code=true&state=pitboard-fixture")
        code.click()
        code.typeText("fixture-code#state")
        app.buttons["sheet.submitCode"].click()
        XCTAssertTrue(app.accountRow("claude/travel").waitForExistence(timeout: 10))
        let name = app.textFields["sheet.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        XCTAssertEqual(name.value as? String, "travel", "the name dana@travel.example suggests")
        name.click()
        name.typeKey("a", modifierFlags: .command)
        name.typeText("third")
        app.buttons["sheet.confirm"].click()
        XCTAssertTrue(app.accountRow("claude/third").waitForExistence(timeout: 10))
        XCTAssertTrue(sheet.waitForNonExistence(timeout: 5))
    }

    /// A Codex sign-in finishes by itself once the browser is done.
    @MainActor
    func testAddingACodexAccount() {
        let app = launch(.twoTools)
        app.openWindow()
        app.toolbars.buttons["toolbar.addAccount"].click()
        let tool = app.popUpButtons["sheet.tool"]
        XCTAssertTrue(tool.waitForExistence(timeout: 5))
        tool.click()
        // Its own item, not the menu bar item's Codex heading.
        tool.menuItems["Codex"].click()
        app.buttons["sheet.confirm"].click()
        XCTAssertTrue(app.accountRow("codex/travel").waitForExistence(timeout: 10))
        XCTAssertTrue(app.textFields["sheet.name"].waitForExistence(timeout: 5))
        app.buttons["sheet.confirm"].click()
        XCTAssertTrue(app.sheets.firstMatch.waitForNonExistence(timeout: 5), "kept as it is")
        XCTAssertTrue(app.accountRow("codex/travel").exists)
    }

    /// A tool Pitboard did not find says what to install, and cannot be signed in to.
    @MainActor
    func testAToolPitboardDidNotFindSaysWhatToInstall() {
        // Claude Code and Claude Desktop, and no Codex CLI.
        let app = launch(.claudeDesktop)
        app.openWindow()
        app.toolbars.buttons["toolbar.addAccount"].click()
        let tool = app.popUpButtons["sheet.tool"]
        XCTAssertTrue(tool.waitForExistence(timeout: 5))
        tool.click()
        tool.menuItems["Codex"].click()
        let sheet = app.sheets.firstMatch
        XCTAssertTrue(
            sheet.descendants(matching: .any)["sheet.missing"].waitForExistence(timeout: 5))
        XCTAssertTrue(sheet.descendants(matching: .any)["sheet.missing.link"].exists)
        XCTAssertFalse(app.buttons["sheet.confirm"].isEnabled)
    }

    /// A name typed with its tool says why under the field, and Rename waits for a name.
    @MainActor
    func testANameThatCannotBeOneSaysWhy() {
        let app = launch(.oneTool)
        app.openWindow()
        let row = app.accountRow("claude/personal")
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.rightClick()
        app.shortcutMenu.menuItems[Words.rename].click()
        let name = app.textFields["sheet.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        name.click()
        name.typeKey("a", modifierFlags: .command)
        name.typeText("claude/third")
        let problem = app.staticTexts["sheet.problem"]
        XCTAssertTrue(problem.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["sheet.confirm"].isEnabled)
        name.typeKey("a", modifierFlags: .command)
        name.typeText("third")
        XCTAssertTrue(problem.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.buttons["sheet.confirm"].isEnabled)
    }

    /// Sign In stays disabled while a sign-in runs, so a second sign-in that starts and
    /// finishes is what shows the first one was stopped.
    @MainActor
    func testCancellingASignInAddsNothing() {
        let app = launch(.oneTool)
        app.openWindow()
        app.toolbars.buttons["toolbar.addAccount"].click()
        let signIn = app.buttons["sheet.confirm"]
        XCTAssertTrue(signIn.waitForExistence(timeout: 5))
        signIn.click()
        XCTAssertTrue(app.textFields["sheet.code"].waitForExistence(timeout: 10))
        app.sheets.firstMatch.buttons["sheet.cancel"].click()
        XCTAssertTrue(app.sheets.firstMatch.waitForNonExistence(timeout: 5))
        XCTAssertFalse(app.accountRow("claude/travel").exists)

        app.toolbars.buttons["toolbar.addAccount"].click()
        XCTAssertTrue(signIn.waitForExistence(timeout: 5))
        signIn.click()
        let code = app.textFields["sheet.code"]
        XCTAssertTrue(code.waitForExistence(timeout: 10))
        code.click()
        code.typeText("fixture-code#state")
        app.buttons["sheet.submitCode"].click()
        XCTAssertTrue(app.accountRow("claude/travel").waitForExistence(timeout: 10))
    }

    @MainActor
    func testRenamingAnAccount() {
        let app = launch(.oneTool)
        app.openWindow()
        let row = app.accountRow("claude/personal")
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.rightClick()
        app.shortcutMenu.menuItems[Words.rename].click()
        let name = app.textFields["sheet.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        name.click()
        name.typeKey("a", modifierFlags: .command)
        name.typeText("work")
        app.buttons["sheet.confirm"].click()
        let failure = app.control("sheet.failure")
        XCTAssertTrue(failure.waitForExistence(timeout: 5))
        name.typeKey("a", modifierFlags: .command)
        name.typeText("home")
        XCTAssertTrue(
            failure.waitForNonExistence(timeout: 5), "it was about the name typed before")
        app.buttons["sheet.confirm"].click()
        XCTAssertTrue(app.accountRow("claude/home").waitForExistence(timeout: 5))
    }

    @MainActor
    func testForgettingAsksFirst() {
        let app = launch(.oneTool)
        app.openWindow()
        let row = app.accountRow("claude/personal")
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.rightClick()
        app.shortcutMenu.menuItems[Words.forget].click()
        XCTAssertTrue(app.alertAnswer.waitForExistence(timeout: 5))
        app.alertCancel.click()
        XCTAssertTrue(row.exists)
        row.rightClick()
        app.shortcutMenu.menuItems[Words.forget].click()
        app.alertAnswer.click()
        XCTAssertTrue(row.waitForNonExistence(timeout: 5))
    }

    /// Its shortcut menu leaves Forget out, and the Account menu dims it.
    @MainActor
    func testTheAccountInUseOffersNoForget() {
        let app = launch(.oneTool)
        app.openWindow()
        let row = app.accountRow("claude/work")
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.rightClick()
        XCTAssertTrue(app.shortcutMenu.menuItems[Words.rename].waitForExistence(timeout: 5))
        XCTAssertFalse(app.shortcutMenu.menuItems[Words.forget].exists)
        app.typeKey(.escape, modifierFlags: [])
        row.click()
        app.accountMenu.click()
        let forget = app.accountMenu.menuItems[Words.forget]
        XCTAssertTrue(forget.waitForExistence(timeout: 5))
        XCTAssertFalse(forget.isEnabled)
        app.typeKey(.escape, modifierFlags: [])
    }

    /// Command-Delete in a sheet's field deletes to the start of the line, and asks nothing of
    /// the account selected behind the sheet.
    @MainActor
    func testCommandDeleteInASheetEditsItsField() {
        let app = launch(.oneTool)
        app.openWindow()
        let row = app.accountRow("claude/personal")
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.click()
        app.accountMenu.click()
        app.accountMenu.menuItems[Words.rename].click()
        let name = app.textFields["sheet.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        name.typeKey(.delete, modifierFlags: .command)
        name.typeText("home")
        XCTAssertEqual(name.value as? String, "home")
        app.sheets.firstMatch.buttons["sheet.cancel"].click()
        XCTAssertTrue(app.sheets.firstMatch.waitForNonExistence(timeout: 5))
        XCTAssertFalse(app.alert.waitForExistence(timeout: 2))
    }

    @MainActor
    func testTheAccountMenuActsOnTheSelectedAccount() {
        let app = launch(.oneTool)
        app.openWindow()
        let row = app.accountRow("claude/personal")
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.click()
        app.accountMenu.click()
        XCTAssertTrue(
            app.accountMenu.menuItems[Words.use("personal")].waitForExistence(timeout: 5))
        app.accountMenu.menuItems[Words.rename].click()
        XCTAssertTrue(app.textFields["sheet.name"].waitForExistence(timeout: 5))
        app.sheets.firstMatch.buttons["sheet.cancel"].click()
        XCTAssertTrue(app.sheets.firstMatch.waitForNonExistence(timeout: 5))

        row.click()
        app.typeKey("c", modifierFlags: .command)
        XCTAssertEqual(NSPasteboard.general.string(forType: .string), "dana@home.example")
        app.typeKey(.delete, modifierFlags: .command)
        XCTAssertTrue(app.alertAnswer.waitForExistence(timeout: 5))
        app.alertCancel.click()
        XCTAssertTrue(row.exists)
    }

    // personal warms up at launch, so its period runs and Warm Up Now cannot be chosen.
    @MainActor
    func testWarmUpNowIsLeftOutOfTheShortcutMenuAndDimmedInTheAccountMenu() {
        let app = launch(.warmUp)
        app.openWindow()
        XCTAssertTrue(
            app.text("BEGINSWITH", Words.warmsUpAgain).waitForExistence(timeout: 30))
        let row = app.accountRow("claude/personal")
        row.rightClick()
        XCTAssertTrue(app.shortcutMenu.menuItems[Words.rename].waitForExistence(timeout: 5))
        XCTAssertFalse(app.shortcutMenu.menuItems[Words.warmUpNow].exists)
        app.typeKey(.escape, modifierFlags: [])

        row.click()
        app.accountMenu.click()
        let now = app.accountMenu.menuItems[Words.warmUpNow]
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
        spare.buttons["account.action"].click()
        let notice = app.descendants(matching: .any)["notice.switch/codex"]
        XCTAssertTrue(notice.waitForExistence(timeout: 5))
        XCTAssertTrue(notice.staticTexts["notice.line"].exists, "what the sessions keep")
        notice.buttons["notice.dismiss"].click()
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

        spare.buttons["account.action"].click()
        XCTAssertTrue(app.alertCancel.waitForExistence(timeout: 5))
        app.alertCancel.click()
        XCTAssertTrue(app.alert.waitForNonExistence(timeout: 5))
        XCTAssertFalse(notice.exists, "nothing was switched")

        spare.buttons["account.action"].click()
        XCTAssertTrue(app.alertAnswer.waitForExistence(timeout: 5))
        app.alertAnswer.click()
        XCTAssertTrue(notice.waitForExistence(timeout: 5))
    }

    @MainActor
    func testNamingTheAccountInUse() {
        let app = launch(.unnamed)
        app.openWindow()
        let add = app.buttons["account.action"]
        XCTAssertTrue(add.waitForExistence(timeout: 5))
        add.click()
        let name = app.textFields["sheet.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        XCTAssertEqual(name.value as? String, "work", "the name dana@work.example suggests")
        XCTAssertTrue(app.accountRow("claude/work").exists, "added before it is named")
        app.buttons["sheet.confirm"].click()
        XCTAssertTrue(app.sheets.firstMatch.waitForNonExistence(timeout: 5))
    }

    /// With Codex installed, after the tip to use Pitboard with it.
    @MainActor
    func testAnEmptyMachineOffersAnAccount() {
        let app = launch(.empty)
        app.openWindow()
        let tip = app.control("setup.codexUnused")
        XCTAssertTrue(tip.waitForExistence(timeout: 5))
        tip.buttons["setup.action.declineTip"].click()
        XCTAssertTrue(app.control("accounts.noAccounts").waitForExistence(timeout: 5))
        app.buttons["accounts.add"].click()
        XCTAssertTrue(app.control("sheet.add").waitForExistence(timeout: 5))
    }

    @MainActor
    func testGivingUpOnAnInterruptedSwitch() {
        let app = launch(.stuck)
        app.openWindow()
        let giveUp = app.control("notice.stuck").buttons["notice.action.abandonStuckSwitch"]
        XCTAssertTrue(giveUp.waitForExistence(timeout: 5))
        giveUp.click()
        app.alertAnswer.click()
        let notice = app.descendants(matching: .any)["notice.abandoned"]
        XCTAssertTrue(notice.waitForExistence(timeout: 5))
        XCTAssertTrue(notice.staticTexts["notice.line"].exists, "what was kept")
    }

    @MainActor
    func testOneAccountIsOfferedASecondUntilNotNow() {
        let app = launch(.onlyOne)
        app.openWindow()
        let tip = app.control("setup.onlyOne")
        XCTAssertTrue(tip.waitForExistence(timeout: 5))
        tip.buttons["setup.action.declineSecondAccount"].click()
        XCTAssertTrue(tip.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.accountRow("claude/work").exists)
    }

    /// The accounts come forward with the sheet over them, wherever Command-N was pressed.
    @MainActor
    func testCommandNAddsAnAccountFromAnyWindow() {
        let app = launch(.oneTool)
        app.openWindow()
        app.openFromWindowMenu(Words.activity)
        let activity = app.windows["activity"]
        XCTAssertTrue(
            activity.staticTexts["activity.change"].firstMatch
                .waitForExistence(timeout: 5))
        app.typeKey("n", modifierFlags: .command)
        let sheet = app.accountsWindow.sheets.firstMatch
        XCTAssertTrue(
            sheet.descendants(matching: .any)["sheet.add"].waitForExistence(timeout: 5))
        sheet.buttons["sheet.cancel"].click()
        XCTAssertTrue(sheet.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.accountRow("claude/work").exists)
        XCTAssertTrue(activity.exists, "a window of its own")
    }

    @MainActor
    func testTheFirstLaunchOpensTheWindow() {
        let app = launch(.firstLaunch)
        XCTAssertTrue(app.control("setup.codexUnused").waitForExistence(timeout: 10))
    }
}
