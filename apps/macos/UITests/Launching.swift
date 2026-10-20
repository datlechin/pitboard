import XCTest

/// Named as the core's fixtures are, which `fixture_names()` gives and the app's unit tests
/// hold to this list.
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
    case chatGPTOpen
    case claudeDesktop
    case claudeDesktopOpen
    case warmUp
    case leftInFile
    case loginReplaced
}

/// Every UI test case: a test stops at its first failure, and starts the app only here.
class FixtureTestCase: XCTestCase {
    override func setUpWithError() throws {
        try super.setUpWithError()
        continueAfterFailure = false
    }

    /// The app, started into `fixture`: nothing it does reaches the keychain, the network,
    /// launchd, the login items or an administrator's password.
    @MainActor
    func launch(_ fixture: Fixture) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment["PITBOARD_FIXTURE"] = fixture.rawValue
        app.launch()
        return app
    }
}

@MainActor
extension XCUIApplication {
    func openMenu() {
        let item = statusItems.firstMatch
        XCTAssertTrue(item.waitForExistence(timeout: 10))
        item.click()
    }

    func openWindow() {
        openMenu()
        menuItem("Open Pitboard").click()
        XCTAssertTrue(windows.firstMatch.waitForExistence(timeout: 5))
    }

    func openSettings() {
        openMenu()
        menuItem("Settings…").click()
        XCTAssertTrue(windows["General"].waitForExistence(timeout: 5))
    }

    /// An item of the menu bar item's menu. The app keeps a main menu, hidden while it has
    /// no Dock icon, and its Settings…, Quit and Add Account… have the same titles, so the
    /// item is looked for under the menu bar item. An account's label is also an item of the
    /// submenu that opens its window, which comes later in the menu, so the first is taken.
    func menuItem(_ title: String) -> XCUIElement {
        statusItems.firstMatch.menuItems.matching(NSPredicate(format: "title == %@", title))
            .firstMatch
    }

    /// A button is looked for in the alert rather than in the whole app, which has a Touch
    /// Bar with a Cancel of its own.
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

    func accountRow(_ qualified: String) -> XCUIElement {
        descendants(matching: .any)["account.\(qualified)"]
    }

    /// macOS titles an account's window with the account's label, then a dash and the page's
    /// title, so the title is the label or starts with it.
    func accountWindow(_ title: String) -> XCUIElement {
        windows.containing(.any, identifier: "account-window")
            .matching(
                NSPredicate(format: "title == %@ OR title BEGINSWITH %@", title, "\(title) ")
            )
            .firstMatch
    }

    var accountWindows: XCUIElementQuery {
        windows.containing(.any, identifier: "account-window")
    }

    var signInWindow: XCUIElement {
        windows["sign-in"]
    }

    var picker: XCUIElement {
        windows.containing(.any, identifier: "account-picker").firstMatch
    }

    /// Opens `link` as the Share extension hands it over: a Pitboard link of the debug build,
    /// opened through the system, which gives it to this app running in its fixture.
    /// `open(_:)` would launch a second copy of the app with it instead, which the test does
    /// not watch. Nothing is sent unless this app is running: the system would start a copy
    /// on the real home to answer it.
    func share(_ link: String) {
        guard state == .runningForeground || state == .runningBackground else {
            XCTFail("a link is shared only with the app running in its fixture")
            return
        }
        let encoded = link.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? ""
        XCUIDevice.shared.system.open(URL(string: "pitboard-debug://open?url=\(encoded)")!)
    }
}

@MainActor
extension XCUIElement {
    /// The first text in this element whose words `comparison` accepts against `words`, a
    /// string operator such as `BEGINSWITH`. SwiftUI keeps a text's words in its value, and at
    /// times in its label, so both are looked at. A query for text that only starts with the
    /// words has to be a predicate: subscripting matches whole strings, and `containing`
    /// matches an element by what is inside it, which a text has nothing of. A web page has
    /// texts whose value is a number, which a string operator throws on, so the value is
    /// compared as a string.
    func text(_ comparison: String, _ words: String) -> XCUIElement {
        let format = "CAST(value, \"NSString\") \(comparison) %@ OR label \(comparison) %@"
        return staticTexts.matching(NSPredicate(format: format, words, words)).firstMatch
    }

    /// Waits for what `wait(for:toEqual:timeout:)` cannot compare, such as `value`, which is
    /// `Any?`, or a label holding more than the words.
    func wait(until format: String, timeout: TimeInterval) -> Bool {
        let expectation = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: format), object: self)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }
}
