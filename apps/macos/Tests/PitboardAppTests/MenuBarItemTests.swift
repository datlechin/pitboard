import Foundation
import PitboardKit
import Testing

@testable import PitboardApp

// Only Xcode compiles the app's asset catalog, so a test reads each symbol set as written.
private func symbol(attention: Bool) throws -> String {
    let bar = MenuBarText(
        nameAndUsage: "work 42%", usage: "42%", spoken: "Pitboard, work 42%", level: .plenty,
        attention: attention, tip: "")
    let set = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .appending(path: "App/Assets.xcassets/\(bar.symbol).symbolset")
    let contents = try JSONSerialization.jsonObject(
        with: Data(contentsOf: set.appending(path: "Contents.json")))
    let file = try #require(
        ((contents as? [String: Any])?["symbols"] as? [[String: Any]])?.first?["filename"]
            as? String)
    return try String(contentsOf: set.appending(path: file), encoding: .utf8)
}

private func margins(of symbol: String) -> [Substring] {
    symbol.split(separator: "\n").filter { $0.contains("margin-Regular-M") }
}

@Test(arguments: [false, true])
func theMenuBarItemShowsASymbolOfTheAppsOwn(attention: Bool) throws {
    #expect(try symbol(attention: attention).contains("id=\"Regular-M\""))
}

// So the item keeps its width, and its words their place, as the badge comes and goes.
@Test func theAttentionSymbolIsTheMarkWithABadgeInTheSameMargins() throws {
    let (mark, attention) = (try symbol(attention: false), try symbol(attention: true))
    #expect(mark != attention)
    #expect(margins(of: mark).count == 2)
    #expect(margins(of: mark) == margins(of: attention))
}
