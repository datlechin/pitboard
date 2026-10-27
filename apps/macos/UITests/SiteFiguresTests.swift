import AppKit
import XCTest

// How macOS's own controls behave in the app, for usepitboard.com's figures. Each test keeps
// what it reads and a picture under a name starting "site-", which docs/facts/site.md records,
// before it asserts what it read.
final class SiteFiguresTests: FixtureTestCase {
    private static let statusMenu = [
        "Claude Code", "work", "old", "personal", "Codex", "main", "spare",
        "Add Account…", "Refresh", "Open claude.ai", "Open chatgpt.com", "Open Pitboard",
        "Settings…", "About Pitboard", "Pitboard Help", "Quit Pitboard",
    ]
    private static let headings = ["Claude Code", "Codex"]

    @MainActor
    func testTheStatusMenuListsItsItemsInOrder() {
        let app = launch(.twoTools)
        let item = app.statusItems.firstMatch
        app.openMenu()
        let menu = item.menus.firstMatch
        XCTAssertTrue(app.menuItem("Quit Pitboard").waitForExistence(timeout: 5))
        let shown = titled(items(of: menu))
        let highlighted = item.isSelected
        record(
            "macOS \(ProcessInfo.processInfo.operatingSystemVersionString)\n"
                + "status item '\(item.title)', highlighted while open: \(highlighted)\n"
                + shown.map(\.line).joined(separator: "\n"),
            as: "site-status-menu-items")
        keep(menu, as: "site-status-menu")
        keep(XCUIScreen.main.screenshot(), as: "site-status-menu-screen")
        XCTAssertTrue(highlighted)
        XCTAssertEqual(item.title, "Pitboard, work 42%, Claude Code, needs attention")
        XCTAssertEqual(shown.map(\.title), Self.statusMenu)
        XCTAssertEqual(shown.filter { !$0.enabled }.map(\.title), Self.headings)
    }

    @MainActor
    func testArrowsInTheStatusMenuOpenedByAClick() {
        let app = launch(.twoTools)
        let menu = app.statusItems.firstMatch.menus.firstMatch
        app.openMenu()
        XCTAssertTrue(app.menuItem("Quit Pitboard").waitForExistence(timeout: 5))
        let opened = highlighted(in: menu)
        var down: [String] = []
        for press in 1...16 {
            app.typeKey(.downArrow, modifierFlags: [])
            down.append(highlighted(in: menu) ?? "-")
            if press == 1 { keep(menu, as: "site-status-menu-first-down") }
        }
        app.typeKey(.escape, modifierFlags: [])

        app.openMenu()
        XCTAssertTrue(app.menuItem("Quit Pitboard").waitForExistence(timeout: 5))
        var up: [String] = []
        for _ in 1...4 {
            app.typeKey(.upArrow, modifierFlags: [])
            up.append(highlighted(in: menu) ?? "-")
        }
        var keys: [String] = []
        for key: XCUIKeyboardKey in [.home, .end, .pageUp, .pageDown] {
            app.typeKey(key, modifierFlags: [])
            keys.append(highlighted(in: menu) ?? "-")
        }
        app.typeKey(.escape, modifierFlags: [])
        record(
            "open: \(opened ?? "-")\nDown: \(down.joined(separator: " | "))\n"
                + "Up: \(up.joined(separator: " | "))\n"
                + "Home, End, Page Up, Page Down: \(keys.joined(separator: " | "))",
            as: "site-status-menu-arrows")

        let enabled = Self.statusMenu.filter { !Self.headings.contains($0) }
        XCTAssertNil(opened)
        XCTAssertEqual(down, enabled + ["Quit Pitboard", "Quit Pitboard"])
        XCTAssertEqual(up, ["Quit Pitboard", "Pitboard Help", "About Pitboard", "Settings…"])
        XCTAssertEqual(keys, ["work", "Quit Pitboard", "work", "Quit Pitboard"])
    }

    @MainActor
    func testTypingANameInTheStatusMenu() {
        let app = launch(.twoTools)
        let menu = app.statusItems.firstMatch.menus.firstMatch
        var seen: [String] = []
        func typed(_ first: String, then rest: String = "", after pause: TimeInterval = 0)
            -> String
        {
            app.openMenu()
            XCTAssertTrue(app.menuItem("Quit Pitboard").waitForExistence(timeout: 5))
            type(first, in: app)
            let typedAt = Date()
            Thread.sleep(forTimeInterval: pause)
            let gap = Int(Date().timeIntervalSince(typedAt) * 1000)
            type(rest, in: app)
            let shown = highlighted(in: menu) ?? "-"
            let name = (first + rest).lowercased().filter(\.isLetter)
            seen.append(
                "\(first)\(rest.isEmpty ? "" : ", \(gap) ms, \(rest)"): \(shown)")
            keep(menu, as: "site-status-menu-typed-\(name)-\(gap / 100)")
            app.typeKey(.escape, modifierFlags: [])
            return shown
        }
        let spa = typed("spa")
        let openP = typed("Open P")
        let o = typed("o")
        let subtitle = typed("Needs")
        let quick = typed("o", then: "p")
        let paused = [0.5, 1.0, 1.5].map { typed("o", then: "p", after: $0) }
        let slow = typed("o", then: "p", after: 3)
        record(seen.joined(separator: "\n"), as: "site-status-menu-type-select")

        XCTAssertEqual(spa, "spare")
        XCTAssertEqual(openP, "Open Pitboard")
        XCTAssertEqual(o, "old")
        XCTAssertEqual(subtitle, "old", "a subtitle is not matched")
        XCTAssertEqual(quick, "Open claude.ai")
        XCTAssertEqual(paused, ["Open claude.ai", "personal", "personal"])
        XCTAssertEqual(slow, "personal")
    }

    @MainActor
    func testSpaceOnTheHighlightedItemOfTheStatusMenu() {
        let app = launch(.twoTools)
        let menu = app.statusItems.firstMatch.menus.firstMatch
        app.openMenu()
        XCTAssertTrue(app.menuItem("Quit Pitboard").waitForExistence(timeout: 5))
        // Arrows, not typing: a space typed soon after a name is part of the name.
        app.typeKey(.end, modifierFlags: [])
        for _ in 1...4 { app.typeKey(.upArrow, modifierFlags: []) }
        let before = highlighted(in: menu)
        app.typeKey(.space, modifierFlags: [])
        let opened = app.windows["main"].waitForExistence(timeout: 3)
        let open = isOpen(menu)
        record(
            "highlighted: \(before ?? "-")\nwindow opened: \(opened)\nmenu still open: \(open)",
            as: "site-status-menu-space")
        keep(XCUIScreen.main.screenshot(), as: "site-status-menu-space")
        XCTAssertEqual(before, "Open Pitboard")
        XCTAssertTrue(opened, "Space chose nothing")
        XCTAssertFalse(open)
    }

    @MainActor
    func testKeyEquivalentsWhileTheStatusMenuIsOpen() {
        let app = launch(.oneTool)
        let menu = app.statusItems.firstMatch.menus.firstMatch
        app.openMenu()
        XCTAssertTrue(app.menuItem("Quit Pitboard").waitForExistence(timeout: 5))
        app.typeKey(",", modifierFlags: .command)
        let settings = app.windows["General"].waitForExistence(timeout: 3)
        let openAfterComma = isOpen(menu)

        app.openMenu()
        XCTAssertTrue(app.menuItem("Quit Pitboard").waitForExistence(timeout: 5))
        app.typeKey("n", modifierFlags: .command)
        let sheet = app.control("sheet.add").waitForExistence(timeout: 3)
        let openAfterN = isOpen(menu)
        keep(XCUIScreen.main.screenshot(), as: "site-status-menu-key-equivalents")
        app.typeKey(.escape, modifierFlags: [])
        let cancelled = app.control("sheet.add").waitForNonExistence(timeout: 3)

        app.openMenu()
        XCTAssertTrue(app.menuItem("Quit Pitboard").waitForExistence(timeout: 5))
        app.typeKey("q", modifierFlags: .command)
        let quit = app.wait(for: .notRunning, timeout: 5)
        record(
            "Command-Comma opened Settings: \(settings), menu still open: \(openAfterComma)\n"
                + "Command-N opened Add Account: \(sheet), menu still open: \(openAfterN)\n"
                + "Escape cancelled the sheet: \(cancelled)\n"
                + "Command-Q quit: \(quit)",
            as: "site-status-menu-key-equivalents")
        XCTAssertTrue(settings)
        XCTAssertFalse(openAfterComma)
        XCTAssertTrue(sheet)
        XCTAssertFalse(openAfterN)
        XCTAssertTrue(quit)
    }

    @MainActor
    func testASubmenuOfTheStatusMenuFromTheKeyboardAndThePointer() {
        let app = launch(.twoTools)
        let menu = app.statusItems.firstMatch.menus.firstMatch
        let site = app.menuItem("Open claude.ai")
        let submenu = site.menus.firstMatch
        app.openMenu()
        XCTAssertTrue(app.menuItem("Quit Pitboard").waitForExistence(timeout: 5))
        type("Open c", in: app)
        let typed = highlighted(in: menu)
        app.typeKey(.rightArrow, modifierFlags: [])
        let right = eventually(within: 2) { self.isOpen(submenu) }
        let inside = titled(items(of: submenu))
        let first = highlighted(in: submenu)
        let menuFrame = frame(of: menu)
        let submenuFrame = frame(of: submenu)
        let work = submenu.menuItems["work"]
        let reachable = work.isHittable
        keep(XCUIScreen.main.screenshot(), as: "site-status-submenu-right")
        app.typeKey(.leftArrow, modifierFlags: [])
        // A closed submenu keeps its frame in the tree, so whether it is open is read by a hit.
        let left = eventually(within: 2) { !work.isHittable }
        let back = highlighted(in: menu)
        keep(XCUIScreen.main.screenshot(), as: "site-status-submenu-left")
        app.typeKey(.escape, modifierFlags: [])

        app.openMenu()
        XCTAssertTrue(site.waitForExistence(timeout: 5))
        let asked = Date()
        site.hover()
        let hovered = Date()
        let shown = eventually(within: 3) { self.isOpen(submenu) }
        let opened = Date()
        app.typeKey(.escape, modifierFlags: [])
        record(
            "typed: \(typed ?? "-")\nRight opened it: \(right) \(reachable), at \(submenuFrame) beside "
                + "the menu at \(menuFrame)\n"
                + "in it: \(inside.map(\.line).joined(separator: "; "))\n"
                + "highlighted in it: \(first ?? "-")\n"
                + "Left closed it: \(left), highlighted after: \(back ?? "-")\n"
                + "hover opened it: \(shown); the pointer took "
                + "\(milliseconds(from: asked, to: hovered)) ms to arrive, the submenu "
                + "\(milliseconds(from: hovered, to: opened)) ms more",
            as: "site-status-submenu")
        XCTAssertEqual(typed, "Open claude.ai")
        XCTAssertTrue(right)
        XCTAssertTrue(reachable)
        XCTAssertEqual(inside.map(\.title), ["work", "old", "personal"])
        XCTAssertEqual(first, "work")
        XCTAssertTrue(left)
        XCTAssertEqual(back, "Open claude.ai")
        XCTAssertTrue(shown)
    }

    @MainActor
    func testTheAppMenuBarWithTheWindowInFront() {
        let app = launch(.oneTool)
        app.openWindow()
        let personal = app.accountRow("claude/personal")
        XCTAssertTrue(personal.waitForExistence(timeout: 5))
        let bar = app.menuBars.menuBarItems.allElementsBoundByIndex.map(\.title)
        var seen = ["bar: " + bar.joined(separator: " | ")]
        var read: [String: [Shown]] = [:]
        func look(_ titles: [String], _ state: String) {
            seen.append(state)
            for title in titles {
                let shown = barMenu(title, of: app, as: state)
                read["\(title) \(state)"] = shown
                seen.append("\(title):")
                seen.append(contentsOf: shown.map { "  " + $0.line })
            }
        }
        let everyMenu = bar.filter { $0 != "Apple" }
        look(everyMenu, "none")
        personal.click()
        look(everyMenu, "personal")
        app.accountRow("claude/work").click()
        look(["Account"], "work")
        app.typeKey(.delete, modifierFlags: .command)
        let asked = app.alert.waitForExistence(timeout: 2)
        seen.append("Command-Delete on work asked: \(asked)")
        record(seen.joined(separator: "\n"), as: "site-menu-bar")
        keep(XCUIScreen.main.screenshot(), as: "site-menu-bar-screen")

        func titles(_ key: String) -> [String] { titled(read[key] ?? []).map(\.title) }
        func dimmed(_ key: String) -> [String] {
            titled(read[key] ?? []).filter { !$0.enabled }.map(\.title)
        }
        XCTAssertEqual(
            bar,
            ["Apple", "Pitboard", "File", "Edit", "View", "Go", "Account", "Window", "Help"])
        XCTAssertEqual(
            titles("Pitboard none"),
            [
                "About Pitboard", "Settings…", "Services", "Hide Pitboard", "Hide Others",
                "Show All", "Quit Pitboard",
            ])
        XCTAssertEqual(dimmed("Pitboard none"), ["Show All"])
        XCTAssertEqual(
            titles("File none"),
            ["Add Account…", "Open claude.ai", "Remove Website Data…", "Close", "Close All"])
        XCTAssertEqual(dimmed("File none"), ["Remove Website Data…"])
        XCTAssertEqual(
            titles("View none"),
            ["Refresh", "Actual Size", "Zoom In", "Zoom Out", "Enter Full Screen"])
        XCTAssertEqual(
            dimmed("View none"), ["Actual Size", "Zoom In", "Zoom Out", "Enter Full Screen"])
        XCTAssertEqual(titles("Go none"), ["Back", "Forward"])
        XCTAssertEqual(dimmed("Go none"), ["Back", "Forward"])
        XCTAssertEqual(titles("Help none"), ["Pitboard Help"])
        XCTAssertEqual(
            titles("Account none"), ["Switch", "Sign In Again…", "Rename…", "Remove Account…"])
        XCTAssertEqual(dimmed("Account none"), titles("Account none"))
        XCTAssertEqual(
            titles("Account personal"),
            [
                "Switch to personal", "Sign In Again…", "Rename…", "Warm Up Now",
                "Open claude.ai", "Remove Account…",
            ])
        XCTAssertEqual(dimmed("Account personal"), ["Warm Up Now"])
        XCTAssertEqual(
            titles("Account work"),
            [
                "Switch to work", "Sign In Again…", "Rename…", "Warm Up Now", "Open claude.ai",
                "Remove Account…",
            ]
        )
        XCTAssertEqual(
            dimmed("Account work"), ["Switch to work", "Warm Up Now", "Remove Account…"])
        XCTAssertEqual(
            dimmed("Edit none"),
            ["Undo", "Redo", "Cut", "Copy", "Paste", "Delete", "Select All"])
        XCTAssertEqual(dimmed("Edit personal"), ["Undo", "Redo", "Cut", "Paste", "Select All"])
        XCTAssertEqual(dimmed("Window none"), ["Full Screen Tile", "Remove Window from Set"])
        XCTAssertEqual(dimmed("Help none"), [])
        XCTAssertFalse(asked)
    }

    @MainActor
    func testTheAboutPanel() {
        let app = launch(.oneTool)
        app.openWindow()
        app.menuBars.menuBarItems["Pitboard"].click()
        app.menuBars.menuItems["About Pitboard"].firstMatch.click()
        // The standard About panel is a dialog, not one of the app's windows.
        let panel = app.dialogs.firstMatch
        let shown = panel.waitForExistence(timeout: 5)
        let texts =
            shown ? panel.staticTexts.allElementsBoundByIndex.map { "\($0.value ?? "")" } : []
        let icon = shown && panel.images.firstMatch.exists
        let enabled = panel.buttons.allElementsBoundByIndex.filter(\.isEnabled).map(
            \.identifier)
        record(
            "shown: \(shown), at \(frame(of: panel))\n\(tree(of: panel))",
            as: "site-about-panel")
        keep(XCUIScreen.main.screenshot(), as: "site-about-panel-screen")
        if shown { keep(panel, as: "site-about-panel") }
        XCTAssertTrue(shown, "no About panel")
        XCTAssertTrue(icon)
        XCTAssertEqual(texts.filter { !$0.isEmpty }.first, "Pitboard")
        XCTAssertEqual(texts.filter { $0.hasSuffix(")") }.count, 1, "no version and build")
        XCTAssertEqual(frame(of: panel).size, CGSize(width: 284, height: 170))
        XCTAssertEqual(enabled, ["_XCUI:CloseWindow"])
    }

    @MainActor
    func testReturnInTheForgetAlert() {
        let app = launch(.oneTool)
        let personal = askToForgetPersonal(app)
        let forget = app.alertAnswer
        let cancel = app.alertCancel
        let sheet = app.sheets["alert"].exists
        let dialogs = app.dialogs.count
        let order = cancel.frame.maxX < forget.frame.minX ? "Cancel, Remove" : "Remove, Cancel"
        keep(app.alert, as: "site-forget-alert")
        keep(app.windows["main"], as: "site-forget-alert-window")
        let tree = tree(of: app.alert)
        app.typeKey(.return, modifierFlags: [])
        let closed = app.alert.waitForNonExistence(timeout: 3)
        let kept = personal.exists
        if !closed { app.typeKey(.escape, modifierFlags: []) }
        record(
            "as a sheet: \(sheet), dialogs: \(dialogs)\nbuttons: \(order)\n\(tree)\n"
                + "Return closed it: \(closed), personal kept: \(kept)",
            as: "site-forget-alert-return")
        XCTAssertTrue(sheet)
        XCTAssertEqual(dialogs, 0)
        XCTAssertEqual(order, "Cancel, Remove")
        XCTAssertFalse(closed, "Return pressed a button")
        XCTAssertTrue(kept)
    }

    @MainActor
    func testCommandPeriodAndEscapeInTheForgetAlert() {
        let app = launch(.oneTool)
        let personal = askToForgetPersonal(app)
        app.typeKey(".", modifierFlags: .command)
        let period = app.alert.waitForNonExistence(timeout: 3)
        let keptAfterPeriod = personal.exists
        if period {
            app.typeKey(.delete, modifierFlags: .command)
            XCTAssertTrue(app.alertAnswer.waitForExistence(timeout: 5))
        }
        app.typeKey(.escape, modifierFlags: [])
        let escape = app.alert.waitForNonExistence(timeout: 3)
        let keptAfterEscape = personal.exists
        record(
            "Command-Period closed it: \(period), personal kept: \(keptAfterPeriod)\n"
                + "Escape closed it: \(escape), personal kept: \(keptAfterEscape)",
            as: "site-forget-alert-cancel")
        XCTAssertTrue(period)
        XCTAssertTrue(keptAfterPeriod)
        XCTAssertTrue(escape)
        XCTAssertTrue(keptAfterEscape)
    }

    @MainActor
    func testWhereFocusGoesAfterForget() {
        let app = launch(.oneTool)
        let personal = askToForgetPersonal(app)
        app.alertAnswer.click()
        XCTAssertTrue(personal.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.alert.waitForNonExistence(timeout: 5))
        let window = app.windows["main"]
        let after = selectedAccounts(in: window)
        keep(window, as: "site-focus-after-forget")
        // macOS gives a UI test no focus to read: Down moves a selection only in the list.
        app.typeKey(.downArrow, modifierFlags: [])
        let down = selectedAccounts(in: window)
        record(
            "selected: \(after)\nDown: \(down)\n\(tree(of: window))",
            as: "site-focus-after-forget")
        XCTAssertEqual(after, [])
        XCTAssertEqual(down, ["claude/work"])
    }

    @MainActor
    func testSelectingAndTypingInTheAccountList() {
        let app = launch(.oneTool)
        app.openWindow()
        let window = app.windows["main"]
        let personal = app.accountRow("claude/personal")
        XCTAssertTrue(personal.waitForExistence(timeout: 5))
        var seen: [(String, [String])] = [("at first", selectedAccounts(in: window))]
        personal.click()
        seen.append(("personal clicked", selectedAccounts(in: window)))
        keep(window, as: "site-list-selected-focused")
        app.typeKey(.upArrow, modifierFlags: [])
        seen.append(("Up", selectedAccounts(in: window)))
        app.typeKey(.downArrow, modifierFlags: [])
        seen.append(("Down", selectedAccounts(in: window)))
        type("w", in: app)
        seen.append(("typed w", selectedAccounts(in: window)))
        // Long enough apart that "p" starts a new name.
        sleep(2)
        type("p", in: app)
        seen.append(("typed p", selectedAccounts(in: window)))
        app.descendants(matching: .any)["sidebar.accounts"].click()
        seen.append(("sidebar clicked", selectedAccounts(in: window)))
        keep(window, as: "site-list-selected-unfocused")
        record(
            seen.map { "\($0.0): \($0.1)" }.joined(separator: "\n"), as: "site-list-selection")
        XCTAssertEqual(
            seen.map { $0.1 },
            [
                [], ["claude/personal"], ["claude/work"], ["claude/personal"], ["claude/work"],
                ["claude/personal"], ["claude/personal"],
            ])
    }

    @MainActor
    func testAShortcutMenuOnARowNotSelected() {
        let app = launch(.oneTool)
        app.openWindow()
        let window = app.windows["main"]
        let personal = app.accountRow("claude/personal")
        let work = app.accountRow("claude/work")
        XCTAssertTrue(personal.waitForExistence(timeout: 5))
        work.click()
        let clicked = selectedAccounts(in: window)
        personal.rightClick()
        let copy = app.shortcutMenu.menuItems["Copy Email Address"]
        XCTAssertTrue(copy.waitForExistence(timeout: 5))
        let rightClicked = selectedAccounts(in: window)
        let shown = titled(items(of: app.shortcutMenu))
        keep(window, as: "site-shortcut-menu-row-not-selected")
        keep(app.shortcutMenu, as: "site-shortcut-menu")
        NSPasteboard.general.clearContents()
        copy.click()
        let copied = NSPasteboard.general.string(forType: .string) ?? "-"
        let after = selectedAccounts(in: window)
        record(
            "work clicked: \(clicked)\npersonal right-clicked: \(rightClicked)\n"
                + "its menu: \(shown.map(\.line).joined(separator: "; "))\n"
                + "copied: \(copied)\nafter: \(after)",
            as: "site-shortcut-menu-row-not-selected")
        XCTAssertEqual(clicked, ["claude/work"])
        XCTAssertEqual(rightClicked, ["claude/work"])
        XCTAssertEqual(
            shown.map(\.title),
            [
                "Switch to personal", "Sign In Again…", "Rename…", "Open claude.ai",
                "Copy Email Address", "Remove Account…",
            ])
        XCTAssertEqual(copied, "dana@home.example")
        XCTAssertEqual(after, ["claude/work"])
    }

    @MainActor
    func testHowLongASwitchAndAReadTake() {
        let app = launch(.oneTool)
        app.openWindow()
        let window = app.windows["main"]
        let personal = app.accountRow("claude/personal")
        XCTAssertTrue(personal.waitForExistence(timeout: 5))
        let use = personal.buttons["account.action"]
        XCTAssertTrue(use.waitForExistence(timeout: 5))
        let inUseLabel = personal.descendants(matching: .any)["account.inUse"]
        var switching = false
        let start = Date()
        use.click()
        let clicked = Date()
        while !inUseLabel.exists, Date().timeIntervalSince(start) < 10 {
            switching =
                switching || personal.descendants(matching: .any)["account.switching"].exists
        }
        let switched = Date()
        let inUse = inUseLabel.exists
        let selected = selectedAccounts(in: window)
        keep(window, as: "site-after-use")

        let refresh = app.toolbars.buttons["toolbar.refresh"]
        var dimmed = false
        let asked = Date()
        refresh.click()
        let pressed = Date()
        while Date().timeIntervalSince(asked) < 10 {
            if !refresh.isEnabled {
                dimmed = true
            } else if dimmed || Date().timeIntervalSince(asked) > 2 {
                break
            }
        }
        record(
            "Use: the click took \(milliseconds(from: start, to: clicked)) ms, In Use "
                + "\(milliseconds(from: clicked, to: switched)) ms after it, Switching… seen: "
                + "\(switching)\nselected after Use: \(selected)\n"
                + "Refresh: the click took \(milliseconds(from: asked, to: pressed)) ms, "
                + "dimmed seen: \(dimmed)\nwindow title: '\(window.title)'",
            as: "site-switch-and-read")
        XCTAssertTrue(inUse)
        XCTAssertEqual(selected, [], "Use selected the row")
    }

    @MainActor
    func testTheLimitsPopUp() {
        let app = launch(.oneTool)
        let choice = openLimits(app)
        let closed = choice.frame
        choice.click()
        let menu = choice.menus.firstMatch
        XCTAssertTrue(menu.waitForExistence(timeout: 5))
        let shown = titled(items(of: menu))
        record(
            "pop-up at \(closed), value \(choice.value ?? "-")\nmenu at \(menu.frame)\n"
                + shown.map(\.line).joined(separator: "\n"),
            as: "site-limits-pop-up")
        keep(app.windows["Limits"], as: "site-limits-pop-up-open")
        keep(XCUIScreen.main.screenshot(), as: "site-limits-pop-up-screen")
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertEqual(shown.map(\.title), ["Do Nothing", "Notify Me", "Switch Automatically"])
        let chosen = shown.first(where: \.selected)
        XCTAssertEqual(chosen?.title, "Notify Me")
        XCTAssertEqual(chosen?.frame.midY ?? 0, closed.midY, accuracy: 3, "not over the pop-up")
    }

    @MainActor
    func testTheShareFieldAndStepperAtTheirBounds() {
        let app = launch(.oneTool)
        _ = openLimits(app)
        let field = app.textFields["settings.atLimitShareField"]
        let stepper = app.control("settings.atLimitShare")
        let up = stepper.incrementArrows.firstMatch
        let down = stepper.decrementArrows.firstMatch
        var seen: [String] = []
        var shares: [String: Int] = [:]
        // Waits for the share the step should leave, where there is one, then reads it.
        func read(_ what: String, expecting expected: Int? = nil) {
            let said = share(app, expecting: expected)
            shares[what] = Int(said.filter(\.isNumber))
            seen.append("\(what): field \(field.value ?? "-"), says \(said)")
        }
        func typed(_ text: String, _ what: String, expecting expected: Int) {
            field.click()
            field.typeKey("a", modifierFlags: .command)
            field.typeText(text)
            read(what, expecting: expected)
        }
        read("at first", expecting: 95)
        up.click()
        read("up from 95", expecting: 96)
        down.click()
        read("down from 96", expecting: 95)
        down.press(forDuration: 4)
        read("down held 4 s", expecting: 50)
        down.click()
        read("down at the lowest", expecting: 50)
        // How fast a hold repeats, each from the lowest.
        for (held, name) in [(0.5, "0.5 s"), (1, "1 s"), (2, "2 s")] {
            up.press(forDuration: held)
            read("up held \(name)")
            down.press(forDuration: 4)
            _ = share(app, expecting: 50)
        }
        up.press(forDuration: 4)
        read("up held 4 s", expecting: 99)
        up.click()
        read("up at the highest", expecting: 99)
        down.press(forDuration: 4)
        _ = share(app, expecting: 50)
        // Typed from the lowest, so the first digit, clamped, changes nothing.
        typed("120\r", "typed 120", expecting: 99)
        typed("10\r", "typed 10", expecting: 50)
        typed("90\r", "typed 90", expecting: 90)
        typed("abc\r", "typed abc", expecting: 90)
        typed("7", "typed 7, no Return", expecting: 50)
        keep(app.windows["Limits"], as: "site-limits-share")
        field.typeKey(.return, modifierFlags: [])
        read("then Return", expecting: 50)
        field.typeKey(.upArrow, modifierFlags: [])
        read("Up arrow in the field", expecting: 50)
        record(seen.joined(separator: "\n"), as: "site-limits-share")

        XCTAssertEqual(shares["at first"], 95)
        XCTAssertEqual(shares["up from 95"], 96)
        XCTAssertEqual(shares["down from 96"], 95)
        XCTAssertEqual(shares["down held 4 s"], 50)
        XCTAssertEqual(shares["down at the lowest"], 50)
        XCTAssertGreaterThan(shares["up held 1 s"] ?? 0, 51, "holding did not repeat")
        XCTAssertGreaterThan(shares["up held 2 s"] ?? 0, shares["up held 1 s"] ?? 99)
        XCTAssertEqual(shares["up held 4 s"], 99)
        XCTAssertEqual(shares["up at the highest"], 99)
        XCTAssertEqual(shares["typed 120"], 99)
        XCTAssertEqual(shares["typed 10"], 50)
        XCTAssertEqual(shares["typed 90"], 90)
        XCTAssertEqual(shares["typed abc"], 90)
        // "typed 7, no Return" is kept, not asserted: the field's binding clamps each key as it
        // is typed, which the site does not copy (docs/facts/site.md).
        XCTAssertEqual(shares["then Return"], 50)
        XCTAssertEqual(shares["Up arrow in the field"], 50)
    }

    @MainActor
    func testTheWarmUpControls() {
        let app = launch(.warmUp)
        app.openWindow()
        XCTAssertTrue(
            app.text("BEGINSWITH", "Warms up again at ").waitForExistence(timeout: 30))
        app.openSettings()
        app.toolbars.buttons["Limits"].click()
        let personal = app.control("settings.warm.claude/personal")
        XCTAssertTrue(personal.waitForExistence(timeout: 5))
        let from = app.control("settings.warmFrom")
        let to = app.control("settings.warmTo")
        record(
            "toggle: \(kind(personal.elementType))\nfrom: \(kind(from.elementType))\n"
                + tree(of: app.windows["Limits"]),
            as: "site-limits-warm-up")
        keep(app.windows["Limits"], as: "site-limits-warm-up")
        XCTAssertEqual(personal.elementType, .switch)
        XCTAssertEqual(from.elementType, .datePicker)
        XCTAssertEqual(to.elementType, .datePicker)
        XCTAssertTrue(from.steppers.firstMatch.exists, "a time picker with no stepper")
    }

    // MARK: - Reading what is shown

    private struct Shown {
        let title: String
        let enabled: Bool
        let selected: Bool
        let kind: String
        let value: String
        let frame: CGRect

        var line: String {
            [
                kind, "'\(title)'", enabled ? "" : "dimmed", selected ? "selected" : "",
                value.isEmpty ? "" : "value \(value)",
                "\(Int(frame.minX)),\(Int(frame.minY)) \(Int(frame.width))x\(Int(frame.height))",
            ].filter { !$0.isEmpty }.joined(separator: " ")
        }
    }

    @MainActor
    private func items(of element: XCUIElement) -> [Shown] {
        guard let snapshot = try? element.snapshot() else { return [] }
        return snapshot.children.map(shown)
    }

    // A menu's separators and the boxes around its headings have no title.
    private func titled(_ shown: [Shown]) -> [Shown] {
        shown.filter { !$0.title.isEmpty }
    }

    @MainActor
    private func shown(_ snapshot: any XCUIElementSnapshot) -> Shown {
        Shown(
            title: snapshot.title.isEmpty ? snapshot.label : snapshot.title,
            enabled: snapshot.isEnabled, selected: snapshot.isSelected,
            kind: kind(snapshot.elementType),
            value: snapshot.value.map { "\($0)" } ?? "", frame: snapshot.frame)
    }

    @MainActor
    private func highlighted(in menu: XCUIElement) -> String? {
        items(of: menu).first(where: \.selected)?.title
    }

    // A closed menu is still in the tree, with no size.
    @MainActor
    private func isOpen(_ menu: XCUIElement) -> Bool {
        !frame(of: menu).isEmpty
    }

    @MainActor
    private func frame(of element: XCUIElement) -> CGRect {
        (try? element.snapshot().frame) ?? .zero
    }

    // Polled every 50 ms, as the submenu's reading times what it waits for.
    @MainActor
    private func eventually(within seconds: TimeInterval, _ condition: () -> Bool) -> Bool {
        let end = Date().addingTimeInterval(seconds)
        while Date() < end {
            if condition() { return true }
            Thread.sleep(forTimeInterval: 0.05)
        }
        return condition()
    }

    private func milliseconds(from start: Date, to end: Date) -> Int {
        Int(end.timeIntervalSince(start) * 1000)
    }

    @MainActor
    private func tree(of element: XCUIElement) -> String {
        guard let snapshot = try? element.snapshot() else { return "no snapshot" }
        var lines: [String] = []
        func walk(_ node: any XCUIElementSnapshot, _ depth: Int) {
            let id = node.identifier.isEmpty ? "" : " #\(node.identifier)"
            lines.append(String(repeating: "  ", count: depth) + shown(node).line + id)
            guard depth < 12 else { return }
            for child in node.children { walk(child, depth + 1) }
        }
        walk(snapshot, 0)
        return lines.joined(separator: "\n")
    }

    // The accounts whose list rows are selected, by the identifier of the row's content.
    @MainActor
    private func selectedAccounts(in window: XCUIElement) -> [String] {
        guard let snapshot = try? window.snapshot() else { return [] }
        var found: [String] = []
        func walk(_ node: any XCUIElementSnapshot, inSelected: Bool) {
            let inSelected = inSelected || (node.elementType == .outlineRow && node.isSelected)
            // A row's controls, account.action and the rest, are inside its own identifier.
            if inSelected, node.identifier.hasPrefix("account.") {
                found.append(String(node.identifier.dropFirst("account.".count)))
                return
            }
            for child in node.children { walk(child, inSelected: inSelected) }
        }
        walk(snapshot, inSelected: false)
        return found
    }

    @MainActor
    private func barMenu(_ title: String, of app: XCUIApplication, as state: String) -> [Shown]
    {
        let entry = app.menuBars.menuBarItems[title]
        entry.click()
        let menu = entry.menus.firstMatch
        guard menu.waitForExistence(timeout: 3) else { return [] }
        let shown = items(of: menu)
        keep(menu, as: "site-menu-bar-\(title.lowercased())-\(state)")
        app.typeKey(.escape, modifierFlags: [])
        return shown
    }

    @MainActor
    private func askToForgetPersonal(_ app: XCUIApplication) -> XCUIElement {
        app.openWindow()
        let personal = app.accountRow("claude/personal")
        XCTAssertTrue(personal.waitForExistence(timeout: 5))
        personal.click()
        app.typeKey(.delete, modifierFlags: .command)
        XCTAssertTrue(app.alertAnswer.waitForExistence(timeout: 5))
        return personal
    }

    @MainActor
    private func openLimits(_ app: XCUIApplication) -> XCUIElement {
        app.openSettings()
        app.toolbars.buttons["Limits"].click()
        let choice = app.popUpButtons["settings.atLimit"]
        XCTAssertTrue(choice.waitForExistence(timeout: 5))
        XCTAssertTrue(choice.wait(for: \.isEnabled, toEqual: true, timeout: 5))
        return choice
    }

    // The share as the pop-up's spoken name says it, once it says `expected` or two seconds on.
    @MainActor
    private func share(_ app: XCUIApplication, expecting expected: Int? = nil) -> String {
        let named = NSPredicate(format: "label BEGINSWITH %@", "When a limit reaches ")
        let element = app.descendants(matching: .any).matching(named).firstMatch
        if let expected {
            _ = element.wait(
                for: \.label, toEqual: "When a limit reaches \(expected)%", timeout: 2)
        }
        return element.label
    }

    // One key at a time, as typeText asks for a focused element and a menu has none.
    @MainActor
    private func type(_ text: String, in app: XCUIApplication) {
        for character in text { app.typeKey(String(character), modifierFlags: []) }
    }

    private func kind(_ type: XCUIElement.ElementType) -> String {
        switch type {
        case .menuItem: "menuItem"
        case .menuBarItem: "menuBarItem"
        case .menu: "menu"
        case .button: "button"
        case .popUpButton: "popUpButton"
        case .staticText: "text"
        case .textField: "textField"
        case .checkBox: "checkBox"
        case .switch: "switch"
        case .stepper: "stepper"
        case .datePicker: "datePicker"
        case .outline: "outline"
        case .outlineRow: "outlineRow"
        case .table: "table"
        case .tableRow: "tableRow"
        case .cell: "cell"
        case .group: "group"
        case .window: "window"
        case .sheet: "sheet"
        case .dialog: "dialog"
        case .image: "image"
        case .scrollView: "scrollView"
        case .toolbar: "toolbar"
        case .other: "other"
        default: "type \(type.rawValue)"
        }
    }

    @MainActor
    private func record(_ text: String, as name: String) {
        let attachment = XCTAttachment(string: text)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    @MainActor
    private func keep(_ element: XCUIElement, as name: String) {
        keep(element.screenshot(), as: name)
    }

    @MainActor
    private func keep(_ screenshot: XCUIScreenshot, as name: String) {
        let attachment = XCTAttachment(screenshot: screenshot)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
