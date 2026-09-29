import XCTest

/// A machine in a known state, as the app's debug build knows it from `PITBOARD_FIXTURE`.
/// Named the same as the app's own `Fixture` cases.
enum Fixture: String {
    case twoTools
    case oneTool
    case empty
    case firstLaunch
    case noClaudeCode
    case unnamed
    case onlyOne
    case readFailure
    case stuck
}

@MainActor
extension XCUIApplication {
    /// The app, started into `fixture`: nothing it does reaches the keychain, the network,
    /// launchd, the login items or an administrator's password.
    static func launched(_ fixture: Fixture) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment["PITBOARD_FIXTURE"] = fixture.rawValue
        app.launch()
        return app
    }

    /// Opens the menu bar item's menu.
    func openMenu() {
        let item = statusItems.firstMatch
        XCTAssertTrue(item.waitForExistence(timeout: 10))
        item.click()
    }

    /// Opens the main window from the menu, as a person does.
    func openWindow() {
        openMenu()
        menuItem("Open pitboard").click()
        XCTAssertTrue(windows.firstMatch.waitForExistence(timeout: 5))
    }

    /// Opens the settings from the menu.
    func openSettings() {
        openMenu()
        menuItem("Settings…").click()
        XCTAssertTrue(windows["General"].waitForExistence(timeout: 5))
    }

    /// An item of the menu bar item's menu. The app keeps a main menu, hidden while it has
    /// no Dock icon, and its Settings…, Quit and Add Account… have the same titles, so the
    /// item is looked for under the menu bar item. Only the menu's own items: the submenu of
    /// Open claude.ai lists the accounts' labels again, and an account's item is the one
    /// that switches.
    func menuItem(_ title: String) -> XCUIElement {
        statusItems.firstMatch.menus.firstMatch.children(matching: .menuItem)[title]
    }

    /// The alert showing over the window. A button is looked for in it rather than in the
    /// whole app, which has a Touch Bar with a Cancel of its own.
    var alert: XCUIElement {
        sheets["alert"]
    }

    /// A text anywhere in the app whose words are `words`. A table keeps a cell's words in
    /// its value, which a subscript does not look at.
    func text(_ words: String) -> XCUIElement {
        text("==", words)
    }

    /// A control by its identifier, whatever kind macOS draws it as: a toggle in a grouped
    /// form is a switch on one version and a check box on another.
    func control(_ identifier: String) -> XCUIElement {
        descendants(matching: .any)[identifier]
    }

    /// An account's row in the window, by its label with its tool.
    func accountRow(_ qualified: String) -> XCUIElement {
        descendants(matching: .any)["account.\(qualified)"]
    }

    /// An account's claude.ai window, by the account's label. macOS titles it with the label
    /// and the page's title after it, "work – claude.ai stand-in", so it is looked for among
    /// the claude.ai scene's windows by a title that is the label or starts with it.
    func claudeWindow(_ label: String) -> XCUIElement {
        let format = "identifier BEGINSWITH %@ AND (title == %@ OR title BEGINSWITH %@)"
        return windows.matching(NSPredicate(format: format, "claude-", label, "\(label) "))
            .firstMatch
    }

    /// The account picker, by its scene's identifier.
    var linkPicker: XCUIElement {
        windows["link-picker"]
    }

    /// The picker's headline, by its identifier: it says what the picker is asking, or why it
    /// cannot open the link.
    var pickerTitle: XCUIElement {
        linkPicker.descendants(matching: .any)["picker.title"]
    }
}

@MainActor
extension XCUIElement {
    /// The first text in this element whose words `comparison` accepts against `words`, a
    /// string operator such as `BEGINSWITH`. SwiftUI keeps a text's words in its value, and at
    /// times in its label, so both are looked at. A query for text that only starts with the
    /// words has to be a predicate: subscripting matches whole strings, and `containing`
    /// matches an element by what is inside it, which a text has nothing of.
    func text(_ comparison: String, _ words: String) -> XCUIElement {
        let format = "value \(comparison) %@ OR label \(comparison) %@"
        return staticTexts.matching(NSPredicate(format: format, words, words)).firstMatch
    }
}
