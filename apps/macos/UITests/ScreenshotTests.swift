import AppKit
import XCTest

/// A picture of each surface, named platform-surface-state, for the docs, which
/// `scripts/docs-screenshots.sh` takes from the result bundle, and for pull requests, which CI
/// keeps as macos-app-pictures. Some are the scenes usepitboard.com's figures are checked
/// against, which its `--figures` keeps.
final class ScreenshotTests: FixtureTestCase {
    @MainActor
    func testTheMenuAndTheWindowForTheDocs() {
        let app = launch(.twoTools)
        let item = app.statusItems.firstMatch
        XCTAssertTrue(item.waitForExistence(timeout: 10))
        XCTAssertTrue(item.wait(until: "title CONTAINS 'work 42%'", timeout: 10))
        app.openWindow()
        let window = app.accountsWindow
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        XCTAssertTrue(app.control("setup.atLimitUnchosen").waitForExistence(timeout: 5))
        keep(window, as: "macos-window-tip-at-limit")
        // The other pictures show the accounts.
        app.putTheTipsAway()

        app.openFromWindowMenu(Words.diagnostics)
        let diagnostics = app.windows["diagnostics"]
        XCTAssertTrue(
            diagnostics.descendants(matching: .any)["checks.summary"].waitForExistence(
                timeout: 10))
        keep(diagnostics, as: "macos-diagnostics")
        diagnostics.buttons[XCUIIdentifierCloseWindow].click()
        app.openFromWindowMenu(Words.activity)
        let activity = app.windows["activity"]
        XCTAssertTrue(
            activity.staticTexts["activity.change"].firstMatch.waitForExistence(timeout: 5))
        keep(activity, as: "macos-activity")
        activity.buttons[XCUIIdentifierCloseWindow].click()

        // The menu is see-through, so it is shown over this window, not over the Mac's screen.
        app.openMenu()
        let menu = item.menus.firstMatch
        XCTAssertTrue(menu.waitForExistence(timeout: 5))
        let below = menu.frame
        app.typeKey(.escape, modifierFlags: [])
        move(window, to: CGPoint(x: below.maxX + 20 - window.frame.width, y: below.minY - 4))
        app.openMenu()
        XCTAssertTrue(app.menuItem("main").waitForExistence(timeout: 5))
        keep(menu, as: "macos-menu-accounts")
        app.typeKey(.escape, modifierFlags: [])

        XCTAssertTrue(app.accountRow("codex/spare").waitForExistence(timeout: 5))
        XCTAssertTrue(
            app.accountRow("claude/work").descendants(matching: .any)["account.inUse"]
                .waitForExistence(timeout: 5))
        keep(window, as: "macos-window-accounts")

        keep(item.screenshot().pngRepresentation, as: "macos-menu-bar-item")

        app.accountRow("claude/personal").click()
        XCTAssertTrue(
            app.control("inspector").descendants(matching: .any)["inspector.action.switch"]
                .waitForExistence(timeout: 5))
        keep(window, as: "macos-window-inspector")

        app.openSettings()
        app.toolbars.buttons[Words.limits].click()
        let settings = app.windows[Words.limits]
        let atLimit = app.popUpButtons["settings.atLimit"]
        XCTAssertTrue(atLimit.waitForExistence(timeout: 5))
        XCTAssertTrue(atLimit.wait(for: \.isEnabled, toEqual: true, timeout: 5))
        choose(Words.switchAutomatically, in: atLimit)
        XCTAssertTrue(app.control("settings.autoSwitchStanding").waitForExistence(timeout: 5))
        keep(settings, as: "macos-settings-limits-switch")

        // Last, since it switches, and to personal, whose five-hour limit is used up: switching
        // automatically would switch away from it again.
        choose(Words.notifyMe, in: atLimit)
        settings.buttons[XCUIIdentifierCloseWindow].click()
        XCTAssertTrue(settings.waitForNonExistence(timeout: 5))
        let personal = app.accountRow("claude/personal")
        XCTAssertTrue(personal.waitForExistence(timeout: 5))
        personal.buttons["account.action"].click()
        let switched = app.descendants(matching: .any)["notice.switch/claude"]
        XCTAssertTrue(switched.waitForExistence(timeout: 10))
        keep(window, as: "macos-notice-switched")

        // personal is at its limit, so the menu offers to switch back to work above the rest.
        XCTAssertTrue(notice(of: app, "advice/").waitForExistence(timeout: 10))
        app.openMenu()
        XCTAssertTrue(app.menuItem(Words.use("work")).waitForExistence(timeout: 5))
        keep(menu, as: "macos-menu-advice")
        app.typeKey(.escape, modifierFlags: [])
    }

    /// The fixture's personal warms up as the app starts.
    @MainActor
    func testWarmingUpForTheSite() {
        let app = launch(.warmUp)
        app.openWindow()
        let personal = app.accountRow("claude/personal")
        XCTAssertTrue(personal.waitForExistence(timeout: 5))
        personal.click()
        let warmed = app.control("inspector").text("BEGINSWITH", Words.warmsUpAgain)
        XCTAssertTrue(warmed.waitForExistence(timeout: 30))
        keep(app.accountsWindow, as: "macos-window-warm-up")
        app.openSettings()
        app.toolbars.buttons[Words.limits].click()
        XCTAssertTrue(app.control("settings.warmStanding").waitForExistence(timeout: 5))
        keep(app.windows[Words.limits], as: "macos-settings-limits-warm-up")
    }

    @MainActor
    func testClaudeDesktopForTheSite() {
        let app = launch(.claudeDesktop)
        app.openWindow()
        let window = app.accountsWindow
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        app.putTheTipsAway()
        let last = app.accountRow("desktop/work")
        XCTAssertTrue(last.waitForExistence(timeout: 5))
        // Moving the pointer over a row scrolls it into sight.
        if !last.isHittable { last.hover() }
        XCTAssertTrue(last.isHittable)
        keep(window, as: "macos-window-claude-desktop")

        app.accountRow("desktop/home").buttons["account.action"].click()
        XCTAssertTrue(
            app.descendants(matching: .any)["notice.switch/desktop"].waitForExistence(
                timeout: 10))
        keep(window, as: "macos-notice-switched-claude-desktop")
    }

    @MainActor
    func testAnAccountsMenusAndItsNameSheet() {
        let app = launch(.oneTool)
        app.openWindow()
        let window = app.accountsWindow
        let row = app.accountRow("claude/personal")
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.rightClick()
        XCTAssertTrue(app.shortcutMenu.menuItems[Words.rename].waitForExistence(timeout: 5))
        keep(app.shortcutMenu, as: "macos-menu-account-shortcut")
        app.typeKey(.escape, modifierFlags: [])

        row.click()
        app.accountMenu.click()
        let accountMenu = app.accountMenu.menus.firstMatch
        XCTAssertTrue(accountMenu.menuItems[Words.rename].waitForExistence(timeout: 5))
        keep(accountMenu, as: "macos-menu-account")
        accountMenu.menuItems[Words.rename].click()
        let name = app.textFields["sheet.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        keep(window, as: "macos-sheet-rename")

        name.click()
        name.typeKey("a", modifierFlags: .command)
        name.typeText("claude/third")
        XCTAssertTrue(app.staticTexts["sheet.problem"].waitForExistence(timeout: 5))
        keep(window, as: "macos-sheet-rename-problem")
        name.typeKey("a", modifierFlags: .command)
        name.typeKey(.delete, modifierFlags: [])
        XCTAssertTrue(app.text(Words.typeAName).waitForExistence(timeout: 5))
        keep(window, as: "macos-sheet-rename-empty")
        name.typeText("work")
        app.buttons["sheet.confirm"].click()
        XCTAssertTrue(app.control("sheet.failure").waitForExistence(timeout: 5))
        keep(window, as: "macos-sheet-rename-failure")
        app.sheets.firstMatch.buttons["sheet.cancel"].click()
        XCTAssertTrue(app.sheets.firstMatch.waitForNonExistence(timeout: 5))

        row.rightClick()
        app.shortcutMenu.menuItems[Words.forget].click()
        XCTAssertTrue(app.alertAnswer.waitForExistence(timeout: 5))
        keep(window, as: "macos-alert-remove")
        app.alertCancel.click()
    }

    /// The fixture's Claude Code takes a code; its address is never opened.
    @MainActor
    func testAddingAnAccount() {
        let app = launch(.oneTool)
        app.openWindow()
        let window = app.accountsWindow
        app.toolbars.buttons["toolbar.addAccount"].click()
        let signIn = app.buttons["sheet.confirm"]
        XCTAssertTrue(signIn.waitForExistence(timeout: 5))
        keep(window, as: "macos-sheet-add")
        signIn.click()
        let code = app.textFields["sheet.code"]
        XCTAssertTrue(code.waitForExistence(timeout: 10))
        XCTAssertTrue(
            app.sheets.firstMatch.buttons["sheet.openLink"].wait(
                for: \.isEnabled, toEqual: true, timeout: 5))
        keep(window, as: "macos-sheet-add-signing-in")
        code.click()
        code.typeText("fixture-code#state")
        app.buttons["sheet.submitCode"].click()
        XCTAssertTrue(app.textFields["sheet.name"].waitForExistence(timeout: 10))
        keep(window, as: "macos-sheet-add-name")
        app.buttons["sheet.confirm"].click()
        XCTAssertTrue(app.sheets.firstMatch.waitForNonExistence(timeout: 5))
    }

    @MainActor
    func testSigningInAgainAndSwitchingCodex() {
        let app = launch(.twoTools)
        app.openWindow()
        let window = app.accountsWindow
        app.putTheTipsAway()
        let old = app.accountRow("claude/old")
        XCTAssertTrue(old.waitForExistence(timeout: 5))
        old.buttons["account.action"].click()
        XCTAssertTrue(app.control("sheet.signInAgain").waitForExistence(timeout: 5))
        keep(window, as: "macos-sheet-sign-in-again")
        app.sheets.firstMatch.buttons["sheet.cancel"].click()
        XCTAssertTrue(app.sheets.firstMatch.waitForNonExistence(timeout: 5))

        app.accountRow("codex/spare").buttons["account.action"].click()
        XCTAssertTrue(
            app.descendants(matching: .any)["notice.switch/codex"].waitForExistence(timeout: 5))
        keep(window, as: "macos-notice-switched-codex")
    }

    /// The fixture's Claude is a stand-in, which signs in by itself 7 seconds after it opens:
    /// the sheet is put aside before that.
    @MainActor
    func testAddingAClaudeDesktopAccount() {
        let app = launch(.claudeDesktop)
        app.openWindow()
        let window = app.accountsWindow
        app.putTheTipsAway()
        app.toolbars.buttons["toolbar.addAccount"].click()
        let tool = app.popUpButtons["sheet.tool"]
        XCTAssertTrue(tool.waitForExistence(timeout: 5))
        tool.click()
        tool.menuItems["Claude Desktop"].click()
        let sheet = app.sheets.firstMatch
        let confirm = sheet.buttons["sheet.confirm"]
        XCTAssertTrue(confirm.wait(until: "label == 'Continue'", timeout: 5))
        keep(window, as: "macos-sheet-add-claude-desktop")

        confirm.click()
        XCTAssertTrue(sheet.buttons["sheet.undo"].waitForExistence(timeout: 10))
        keep(window, as: "macos-sheet-add-claude-desktop-waiting")
        sheet.buttons["sheet.cancel"].click()
        XCTAssertTrue(app.sheets.firstMatch.waitForNonExistence(timeout: 5))
        XCTAssertTrue(
            app.descendants(matching: .any)["notice.awaiting/desktop"].waitForExistence(
                timeout: 5))
        keep(window, as: "macos-notice-add-unfinished")
    }

    /// The fixture's ChatGPT is a stand-in: nothing is quit.
    @MainActor
    func testAskingToQuitChatGPT() {
        let app = launch(.chatGPTOpen)
        app.openWindow()
        app.putTheTipsAway()
        let spare = app.accountRow("codex/spare")
        XCTAssertTrue(spare.waitForExistence(timeout: 5))
        spare.buttons["account.action"].click()
        XCTAssertTrue(app.alertAnswer.waitForExistence(timeout: 5))
        keep(app.accountsWindow, as: "macos-alert-quit-chatgpt")
        app.alertCancel.click()
    }

    /// The fixture's Claude is a stand-in: nothing is quit.
    @MainActor
    func testAskingToQuitClaude() {
        let app = launch(.claudeDesktopOpen)
        app.openWindow()
        app.putTheTipsAway()
        let home = app.accountRow("desktop/home")
        XCTAssertTrue(home.waitForExistence(timeout: 5))
        home.buttons["account.action"].click()
        XCTAssertTrue(app.alertAnswer.waitForExistence(timeout: 5))
        keep(app.accountsWindow, as: "macos-alert-quit-claude")
        app.alertCancel.click()
    }

    @MainActor
    func testCancellingASwitchThatDidNotFinish() {
        let app = launch(.stuck)
        app.openMenu()
        let issue = app.menuItem(Words.oneIssue)
        XCTAssertTrue(issue.waitForExistence(timeout: 5))
        keep(app.statusItems.firstMatch.menus.firstMatch, as: "macos-menu-issue")
        issue.click()
        let window = app.accountsWindow
        let cancel = app.control("notice.stuck").buttons["notice.action.abandonStuckSwitch"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 5))
        keep(window, as: "macos-notice-switch-unfinished")
        cancel.click()
        XCTAssertTrue(app.alertAnswer.waitForExistence(timeout: 5))
        keep(window, as: "macos-alert-cancel-switch")
        app.alertAnswer.click()
        let cancelled = app.descendants(matching: .any)["notice.abandoned"]
        XCTAssertTrue(cancelled.waitForExistence(timeout: 5))
        XCTAssertTrue(cancelled.staticTexts["notice.line"].waitForExistence(timeout: 5))
        keep(window, as: "macos-notice-switch-cancelled")
    }

    @MainActor
    func testCleaningUpALeftoverSignIn() {
        let app = launch(.leftInFile)
        app.openWindow()
        let window = app.accountsWindow
        let cleanUp = app.buttons["notice.action.stow"]
        XCTAssertTrue(cleanUp.waitForExistence(timeout: 10))
        keep(window, as: "macos-notice-left-in-file")
        app.openFromWindowMenu(Words.diagnostics)
        let diagnostics = app.windows["diagnostics"]
        XCTAssertTrue(
            diagnostics.descendants(matching: .any)["checks.summary"].waitForExistence(
                timeout: 10))
        keep(diagnostics, as: "macos-diagnostics-left-in-file")
        diagnostics.buttons[XCUIIdentifierCloseWindow].click()
        XCTAssertTrue(diagnostics.waitForNonExistence(timeout: 5))
        cleanUp.click()
        let confirm = app.buttons["sheet.stow"]
        XCTAssertTrue(confirm.waitForExistence(timeout: 5))
        XCTAssertTrue(confirm.wait(for: \.isEnabled, toEqual: true, timeout: 10))
        keep(window, as: "macos-sheet-clean-up")
        confirm.click()
        XCTAssertTrue(
            app.descendants(matching: .any)["notice.stowed"].waitForExistence(timeout: 10))
        keep(window, as: "macos-notice-cleaned-up")
    }

    @MainActor
    func testASignInReplacedOutsidePitboard() {
        let app = launch(.loginReplaced)
        app.openWindow()
        XCTAssertTrue(app.buttons["notice.action.signInAgain"].waitForExistence(timeout: 10))
        keep(app.accountsWindow, as: "macos-notice-login-replaced")
    }

    @MainActor
    func testASwitchByItselfOffersSwitchBack() {
        let app = launch(.switchedByItself)
        app.openWindow()
        XCTAssertTrue(app.buttons["notice.action.switchBack"].waitForExistence(timeout: 30))
        keep(app.accountsWindow, as: "macos-notice-switched-by-itself")
    }

    @MainActor
    func testTheFirstLaunch() {
        let app = launch(.firstLaunch)
        let welcome = app.windows["welcome"]
        XCTAssertTrue(welcome.buttons["welcome.add.codex"].waitForExistence(timeout: 10))
        keep(welcome, as: "macos-welcome")
        welcome.buttons["welcome.done"].click()
        XCTAssertTrue(app.control("setup.codexUnused").waitForExistence(timeout: 10))
        keep(app.accountsWindow, as: "macos-window-first-launch")
    }

    @MainActor
    func testUninstallingPitboard() {
        let app = launch(.oneTool)
        app.openSettings()
        app.toolbars.buttons[Words.advanced].click()
        let settings = app.windows[Words.advanced]
        let uninstall = app.buttons["settings.uninstall"]
        XCTAssertTrue(uninstall.waitForExistence(timeout: 5))
        uninstall.click()
        let answer = app.alert.buttons[Words.uninstall]
        XCTAssertTrue(answer.waitForExistence(timeout: 5))
        keep(settings, as: "macos-alert-uninstall")
        answer.click()
        let trash = app.dialog.buttons[Words.moveToTrash]
        XCTAssertTrue(trash.waitForExistence(timeout: 30))
        keep(XCUIScreen.main.screenshot().pngRepresentation, as: "macos-alert-move-to-trash")
        trash.click()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10))
    }

    @MainActor
    func testNoAccounts() {
        let app = launch(.empty)
        app.openWindow()
        let tip = app.control("setup.codexUnused")
        XCTAssertTrue(tip.waitForExistence(timeout: 5))
        tip.buttons["setup.action.declineTip"].click()
        XCTAssertTrue(app.control("accounts.noAccounts").waitForExistence(timeout: 5))
        keep(app.accountsWindow, as: "macos-window-no-accounts")
    }

    @MainActor
    func testNoClaudeCode() {
        let app = launch(.noClaudeCode)
        app.openMenu()
        XCTAssertTrue(app.menuItem(Words.claudeCodeMissing).waitForExistence(timeout: 5))
        keep(app.statusItems.firstMatch.menus.firstMatch, as: "macos-menu-no-tool")
        app.menuItem(Words.openPitboard).click()
        XCTAssertTrue(app.control("accounts.noTool").waitForExistence(timeout: 5))
        keep(app.accountsWindow, as: "macos-window-no-tool")
        app.toolbars.buttons["toolbar.addAccount"].click()
        XCTAssertTrue(app.control("sheet.missing").waitForExistence(timeout: 5))
        keep(app.accountsWindow, as: "macos-sheet-add-missing-tool")
    }

    @MainActor
    func testAFailedRead() {
        let app = launch(.readFailure)
        app.openMenu()
        let issue = app.menuItem(Words.oneIssue)
        XCTAssertTrue(issue.waitForExistence(timeout: 5))
        keep(app.statusItems.firstMatch.menus.firstMatch, as: "macos-menu-read-failure")
        issue.click()
        XCTAssertTrue(app.control("accounts.readFailed").waitForExistence(timeout: 5))
        keep(app.accountsWindow, as: "macos-window-read-failure")
    }

    @MainActor
    func testASignInPitboardHasNoNameFor() {
        let app = launch(.unnamed)
        app.openWindow()
        let window = app.accountsWindow
        let add = app.buttons["account.action"]
        XCTAssertTrue(add.waitForExistence(timeout: 5))
        keep(window, as: "macos-window-unnamed")
        add.click()
        XCTAssertTrue(app.textFields["sheet.name"].waitForExistence(timeout: 5))
        keep(window, as: "macos-sheet-name-in-use")
    }

    @MainActor
    func testOneAccount() {
        let app = launch(.onlyOne)
        app.openWindow()
        XCTAssertTrue(app.control("setup.onlyOne").waitForExistence(timeout: 5))
        keep(app.accountsWindow, as: "macos-window-tip-second-account")
    }

    @MainActor
    func testTheSettingsPanes() {
        let app = launch(.oneTool)
        app.openSettings()
        XCTAssertTrue(app.staticTexts["settings.noUpdateKey"].waitForExistence(timeout: 5))
        keep(app.windows[Words.general], as: "macos-settings-general")
        app.toolbars.buttons[Words.limits].click()
        let atLimit = app.popUpButtons["settings.atLimit"]
        XCTAssertTrue(atLimit.waitForExistence(timeout: 5))
        XCTAssertTrue(atLimit.wait(for: \.isEnabled, toEqual: true, timeout: 5))
        keep(app.windows[Words.limits], as: "macos-settings-limits-notify")
        app.toolbars.buttons[Words.advanced].click()
        XCTAssertTrue(app.control("settings.renewDaily").waitForExistence(timeout: 5))
        keep(app.windows[Words.advanced], as: "macos-settings-advanced")
    }

    @MainActor
    private func notice(of app: XCUIApplication, _ prefix: String) -> XCUIElement {
        app.descendants(matching: .any)
            .matching(NSPredicate(format: "identifier BEGINSWITH %@", "notice.\(prefix)"))
            .firstMatch
    }

    @MainActor
    private func choose(_ title: String, in popUp: XCUIElement) {
        popUp.click()
        popUp.menuItems[title].click()
        XCTAssertTrue(popUp.wait(until: "value == '\(title)'", timeout: 5))
    }

    @MainActor
    private func move(_ window: XCUIElement, to origin: CGPoint) {
        let frame = window.frame
        let titleBar = window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0))
            .withOffset(CGVector(dx: 0, dy: 10))
        titleBar.click(
            forDuration: 0.3,
            thenDragTo: titleBar.withOffset(
                CGVector(dx: origin.x - frame.minX, dy: origin.y - frame.minY)))
    }

    /// A window with the sheet or alert over it, or a menu, cut out at its corners.
    @MainActor
    private func keep(_ element: XCUIElement, as name: String) {
        keep(cutOut(element, cornerRadius: element.elementType == .menu ? 11 : 16), as: name)
    }

    private func keep(_ png: Data, as name: String) {
        let attachment = XCTAttachment(data: png, uniformTypeIdentifier: "public.png")
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    /// A screenshot is of the element's frame, so outside its rounded corners it holds
    /// whatever is behind; `cornerRadius` is in points, as measured on macOS 27.
    @MainActor
    private func cutOut(_ element: XCUIElement, cornerRadius: CGFloat) -> Data {
        let screenshot = element.screenshot().image
        let image = screenshot.cgImage(forProposedRect: nil, context: nil, hints: nil)!
        let bounds = CGRect(x: 0, y: 0, width: image.width, height: image.height)
        let radius = cornerRadius * bounds.width / element.frame.width
        let context = CGContext(
            data: nil, width: image.width, height: image.height, bitsPerComponent: 8,
            bytesPerRow: 0, space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        context.addPath(
            CGPath(
                roundedRect: bounds, cornerWidth: radius, cornerHeight: radius, transform: nil))
        context.clip()
        context.draw(image, in: bounds)
        let cut = NSBitmapImageRep(cgImage: context.makeImage()!)
        return cut.representation(using: .png, properties: [:])!
    }
}
