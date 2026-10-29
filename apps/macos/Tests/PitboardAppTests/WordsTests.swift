import Foundation
import Testing

// A view says the model's words, from app_words() or a sentence of its own, which
// present/words/ pins.
@Test func noViewHandsAControlWordsOfItsOwn() throws {
    let views = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .appending(path: "Sources/PitboardApp")
    let files = try #require(
        FileManager.default.enumerator(at: views, includingPropertiesForKeys: nil))
    var said: [String] = []
    for case let file as URL in files where file.pathExtension == "swift" {
        let code = try String(contentsOf: file, encoding: .utf8)
        said += try literalsDrawn(in: code).map { "\(file.lastPathComponent): \($0)" }
    }
    #expect(said == [])
}

@Test func aLiteralIsFoundWhereverAControlTakesOne() throws {
    #expect(try literalsDrawn(in: #"Text("Version")"#) == ["Version"])
    #expect(try literalsDrawn(in: "Toggle(\n    \"Open at login\",") == ["Open at login"])
    #expect(
        try literalsDrawn(in: #"Label("General", systemImage: Symbol.general)"#) == ["General"])
    #expect(try literalsDrawn(in: #".help("Read the log again")"#) == ["Read the log again"])
    #expect(try literalsDrawn(in: #"return .failed("The link couldn’t be made.")"#).count == 1)
    #expect(try literalsDrawn(in: #"alert.messageText = "Quit?""#) == ["Quit?"])
    #expect(try literalsDrawn(in: #"case .usage: "Usage only""#) == ["Usage only"])
    // what no one reads: an identifier, a symbol, an asset, or only what is put in
    #expect(try literalsDrawn(in: #".accessibilityIdentifier("settings.atLimit")"#).isEmpty)
    #expect(try literalsDrawn(in: #"case .info: "info.circle.fill""#).isEmpty)
    #expect(try literalsDrawn(in: #"Image("pitboard")"#).isEmpty)
    #expect(try literalsDrawn(in: #"Text("\(label) \(timer)")"#).isEmpty)
    #expect(try literalsDrawn(in: #"// Text("Said in a comment")"#).isEmpty)
}

// What a literal says, its interpolations left out, where a control, an alert or a case takes it.
func literalsDrawn(in code: String) throws -> [String] {
    let code = code.split(separator: "\n", omittingEmptySubsequences: false)
        .filter { !$0.trimmingCharacters(in: .whitespaces).hasPrefix("//") }
        .joined(separator: "\n")
    let string = #""((?:[^"\\\n]|\\.)*)""#
    let controls = [
        "Text", "Button", "Label", "Toggle", "Picker", "LabeledContent", "Section", "Link",
        "TextField", "SecureField", "Stepper", "DatePicker", "ProgressView",
        "ContentUnavailableView", "Menu", "CommandMenu", "Window", "WindowGroup",
        "Announcement", "failed", "help", "accessibilityLabel", "accessibilityValue",
        "accessibilityHint", "navigationTitle", "navigationSubtitle", "alert",
        "confirmationDialog",
    ]
    // Simple boundaries: Unicode's join "alert.messageText" into one word.
    let taken = try Regex(#"\b(?:\#(controls.joined(separator: "|")))\(\s*\#(string)"#)
        .wordBoundaryKind(.simple)
    let assigned = try Regex(
        #"(?:withTitle:|\b(?:messageText|informativeText|title|message)\s*=)\s*\#(string)"#
    ).wordBoundaryKind(.simple)
    // A symbol's or an identifier's name is lower case and one word; words are neither.
    let returned = try Regex(#"\bcase [^\n:]+:\s*\#(string)"#).wordBoundaryKind(.simple)
    var said: [String] = []
    for (pattern, wordsOnly) in [(taken, false), (assigned, false), (returned, true)] {
        for match in code.matches(of: pattern) {
            guard let literal = match.output[1].substring else { continue }
            let text = withoutInterpolations(literal)
            guard text.contains(where: \.isLetter) else { continue }
            if wordsOnly, !(text.first?.isUppercase ?? false), !text.contains(" ") { continue }
            said.append(text)
        }
    }
    return said
}

private func withoutInterpolations(_ literal: Substring) -> String {
    var text = ""
    var rest = literal[...]
    while let start = rest.range(of: #"\("#) {
        text += rest[..<start.lowerBound]
        var depth = 1
        var index = start.upperBound
        while index < rest.endIndex, depth > 0 {
            if rest[index] == "(" { depth += 1 }
            if rest[index] == ")" { depth -= 1 }
            index = rest.index(after: index)
        }
        rest = rest[index...]
    }
    return text + rest
}
