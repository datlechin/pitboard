import AppKit
import XCTest

/// Pictures for the docs, which `scripts/docs-screenshots.sh` takes from the result bundle.
final class ScreenshotTests: FixtureTestCase {
    @MainActor
    func testTheMenuAndTheWindowForTheDocs() {
        let app = launch(.twoTools)
        let item = app.statusItems.firstMatch
        XCTAssertTrue(item.waitForExistence(timeout: 10))
        XCTAssertTrue(item.wait(until: "title CONTAINS 'work 42%'", timeout: 10))
        app.openWindow()
        let window = app.windows["main"]
        XCTAssertTrue(window.waitForExistence(timeout: 5))
        // The pictures show the accounts: opening at login, then what happens at a limit.
        for tip in ["Open Pitboard at login", "Switch before a limit runs out"] {
            XCTAssertTrue(app.staticTexts[tip].waitForExistence(timeout: 5), tip)
            app.buttons["Not Now"].click()
            XCTAssertTrue(app.staticTexts[tip].waitForNonExistence(timeout: 5), tip)
        }

        // The menu is see-through, so it is shown over this window, not over the Mac's screen.
        app.openMenu()
        let menu = item.menus.firstMatch
        XCTAssertTrue(menu.waitForExistence(timeout: 5))
        let below = menu.frame
        app.typeKey(.escape, modifierFlags: [])
        app.descendants(matching: .any)["sidebar.machine"].click()
        move(window, to: CGPoint(x: below.maxX + 20 - window.frame.width, y: below.minY - 4))
        app.openMenu()
        XCTAssertTrue(app.menuItem("main · Pro").waitForExistence(timeout: 5))
        keep(cutOut(menu, cornerRadius: 11), as: "menu")
        app.typeKey(.escape, modifierFlags: [])

        app.descendants(matching: .any)["sidebar.accounts"].click()
        XCTAssertTrue(app.accountRow("codex/spare").waitForExistence(timeout: 5))
        XCTAssertTrue(app.text("BEGINSWITH", "5-hour limit, 42").waitForExistence(timeout: 5))
        keep(cutOut(window, cornerRadius: 16), as: "window")
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
