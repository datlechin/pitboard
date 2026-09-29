import Foundation
import PitboardLinks
import Testing

private let claude = URL(string: "https://claude.ai/")!
private let fixture = URL(string: "pitboard-fixture://claude.ai/")!

/// A pitboard link as a browser hands it over.
private func link(_ text: String) -> URL {
    URL(string: text)!
}

private func accepted(_ text: String, home: URL = claude) -> String? {
    guard case .success(let url) = claudeLink(from: text, home: home) else { return nil }
    return url.absoluteString
}

private func refusal(_ text: String, home: URL = claude) -> LinkRefusal? {
    guard case .failure(let refusal) = claudeLink(from: text, home: home) else { return nil }
    return refusal
}

// MARK: - The pitboard link

/// The format is a contract with every bookmarklet and script that builds one, so one link
/// is pinned exactly: everything but the unreserved characters is encoded, a space as `%20`
/// and a plus as `%2B`, which is what JavaScript's `encodeURIComponent` gives for it.
@Test func aPitboardLinkCarriesOneEncodedClaudeLink() {
    let built = pitboardLink(
        opening: "https://claude.ai/public/artifacts/0e5a?x=1&y=a b+c#frag", scheme: "pitboard")
    #expect(
        built.absoluteString
            == "pitboard://open?url=https%3A%2F%2Fclaude.ai%2Fpublic%2Fartifacts%2F0e5a"
            + "%3Fx%3D1%26y%3Da%20b%2Bc%23frag")
    #expect(
        pitboardLink(opening: "https://claude.ai/", scheme: "pitboard-debug").scheme
            == "pitboard-debug")
}

@Test(arguments: [
    "https://claude.ai/new",
    "https://claude.ai/public/artifacts/0e5a?x=1&y=a b+c#frag",
    "https://claude.ai/chat/x?q=100%25&r=a=b",
    "https://claude.ai/x?a='()!*'",
    "https://claude.ai/chat/über?q=日本語",
    "https://claude.ai/x#one#two",
    "https://claude.ai/" + String(repeating: "a", count: 8100),
])
func buildingThenReadingGivesTheSameLink(_ original: String) {
    let built = pitboardLink(opening: original, scheme: "pitboard")
    #expect(linkText(in: built, scheme: "pitboard") == .success(original))
}

/// What the bookmarklet in the docs sends: `encodeURIComponent` leaves `!'()*` bare, and a
/// plus it did not write stays a plus rather than becoming a space.
@Test func whatTheBookmarkletSendsIsRead() {
    let sent = link("pitboard://open?url=https%3A%2F%2Fclaude.ai%2Fx%3Fa%3D(1)!*'%20b")
    #expect(
        linkText(in: sent, scheme: "pitboard") == .success("https://claude.ai/x?a=(1)!*' b"))
    let plus = link("pitboard://open?url=https%3A%2F%2Fclaude.ai%2Fx%3Fa%3D1+2")
    #expect(linkText(in: plus, scheme: "pitboard") == .success("https://claude.ai/x?a=1+2"))
}

@Test func aLinkForAnotherSchemeOrRequestIsRefused() {
    let inside = "url=https%3A%2F%2Fclaude.ai%2F"
    let cases: [(String, LinkRefusal)] = [
        ("pitboard-debug://open?\(inside)", .notPitboard),
        ("https://open?\(inside)", .notPitboard),
        ("pitboard:open?\(inside)", .notPitboard),
        ("pitboard://someone@open?\(inside)", .notPitboard),
        ("pitboard://open:8080?\(inside)", .notPitboard),
        ("pitboard://close?\(inside)", .unknownRequest),
        ("pitboard://open/x?\(inside)", .unknownRequest),
    ]
    for (text, expected) in cases {
        #expect(linkText(in: link(text), scheme: "pitboard") == .failure(expected), "\(text)")
    }
    #expect(
        linkText(in: link("PITBOARD://OPEN/?\(inside)"), scheme: "pitboard")
            == .success("https://claude.ai/"))
    #expect(
        linkText(in: link("pitboard-debug://open?\(inside)"), scheme: "pitboard-debug")
            == .success("https://claude.ai/"))
}

/// A link written by hand without encoding reads as a shorter link, another item and a
/// fragment. Opening the first part would open the wrong page without a word.
@Test(arguments: [
    "pitboard://open?url=https://claude.ai/x?a=1&b=2#c",
    "pitboard://open?url=https://claude.ai/x#c",
    "pitboard://open?url=a&url=b",
    "pitboard://open?url=https%3A%2F%2Fclaude.ai%2F&also=1",
    "pitboard://open?url=",
    "pitboard://open?link=https%3A%2F%2Fclaude.ai%2F",
    "pitboard://open?",
    "pitboard://open",
])
func anAmbiguousLinkIsRefused(_ text: String) {
    #expect(linkText(in: link(text), scheme: "pitboard") == .failure(.ambiguous))
}

@Test func aLinkOver8192CharactersIsRefused() {
    let long = link("pitboard://open?url=" + String(repeating: "a", count: 8200))
    #expect(linkText(in: long, scheme: "pitboard") == .failure(.tooLong))
    let built = pitboardLink(
        opening: "https://claude.ai/" + String(repeating: "a", count: 8200), scheme: "pitboard")
    #expect(linkText(in: built, scheme: "pitboard") == .failure(.tooLong))
}

// MARK: - What may be opened

@Test func onlyClaudeLinksAreAccepted() {
    #expect(accepted("https://claude.ai/new") == "https://claude.ai/new")
    #expect(accepted("  https://claude.ai/new\n") == "https://claude.ai/new")
    #expect(accepted("claude.ai/chat/abc") == "https://claude.ai/chat/abc")
    #expect(accepted("Claude.AI/chat/abc") == "https://claude.ai/chat/abc")
    #expect(accepted("claude.ai") == "https://claude.ai/")
    #expect(accepted("http://claude.ai/chat/abc") == "https://claude.ai/chat/abc")
    #expect(accepted("HTTPS://CLAUDE.AI/new") == "https://claude.ai/new")
    #expect(accepted("https://claude.ai") == "https://claude.ai/")
    #expect(
        accepted("https://claude.ai/public/artifacts/0e5a?x=1&y=%20#frag")
            == "https://claude.ai/public/artifacts/0e5a?x=1&y=%20#frag")

    #expect(refusal("https://example.com/") == .notClaude(host: "example.com"))
    #expect(refusal("example.com/x") == .noLink, "only claude.ai is taken without a scheme")
    #expect(
        refusal("https://claude.ai.evil.example/") == .notClaude(host: "claude.ai.evil.example")
    )
    #expect(refusal("https://evilclaude.ai/") == .notClaude(host: "evilclaude.ai"))
    #expect(refusal("https://sub.claude.ai/") == .notClaude(host: "sub.claude.ai"))
    #expect(refusal("https://claude.ai:8443/") == .notClaude(host: "claude.ai:8443"))
    #expect(refusal("https://x@claude.ai/") == .notClaude(host: nil))
    #expect(refusal("https://x:y@claude.ai/") == .notClaude(host: nil))
    #expect(
        refusal("https://claude.ai@evil.example/") == .notClaude(host: "evil.example"),
        "the host is what follows the @")
    #expect(refusal("javascript:alert(1)") == .noLink)
    #expect(refusal("data:text/html,hi") == .noLink)
    #expect(refusal("file:///etc/hosts") == .noLink)
    #expect(refusal("pitboard-fixture://claude.ai/x") == .noLink)
    #expect(refusal("") == .noLink)
    #expect(refusal("   ") == .noLink)
    #expect(refusal("nothing to open") == .noLink)
}

/// A link is opened on the window's own origin, so in a fixture a claude.ai link opens the
/// stand-in page and never the network.
@Test func aClaudeLinkOpensOnTheWindowsHome() {
    #expect(
        accepted("https://claude.ai/public/artifacts/0e5a?x=1#y", home: fixture)
            == "pitboard-fixture://claude.ai/public/artifacts/0e5a?x=1#y")
    #expect(accepted("claude.ai", home: fixture) == "pitboard-fixture://claude.ai/")
    #expect(refusal("https://example.com/", home: fixture) == .notClaude(host: "example.com"))
}

/// Somebody else's sign-in link would sign the window in as whoever it belongs to, while the
/// window stays titled with the person's own label.
@Test func aSignInLinkIsRefusedFromOutside() {
    for text in [
        "https://claude.ai/magic-link#a:b", "https://claude.ai/MAGIC-LINK",
        "https://claude.ai/magic-link/x", "https://claude.ai/magic-link/",
        "https://claude.ai/magic%2Dlink#a:b", "claude.ai/magic-link#a:b",
        // WebKit drops dot segments, `%2e` included, before it loads a link, and keeps an
        // empty segment that a server merging slashes would route to sign-in all the same.
        "https://claude.ai/./magic-link#a:b", "https://claude.ai/x/../magic-link#a:b",
        "https://claude.ai/a/../magic-link#a:b", "https://claude.ai/%2e/magic-link#a:b",
        "claude.ai/%2e%2e/magic-link#a:b", "claude.ai/%2E%2E/magic-link#a:b",
        "https://claude.ai/./magic-link", "https://claude.ai//magic-link#a:b",
        "https://claude.ai/%2Fmagic-link#a:b",
    ] {
        #expect(refusal(text) == .signInLink, "\(text)")
    }
    #expect(accepted("https://claude.ai/magic-links-guide") != nil)
    #expect(accepted("https://claude.ai/chat/magic-link") != nil)
}

/// claude.ai's own links never have a dot segment. One that does not lead to sign-in is
/// refused too, rather than opened somewhere other than it reads.
@Test func aLinkWithADotSegmentIsRefused() {
    #expect(refusal("https://claude.ai/chat/../new") == .noLink)
    #expect(refusal("https://claude.ai/./new") == .noLink)
    #expect(refusal("https://claude.ai/magic-link/..") == .noLink)
    #expect(accepted("https://claude.ai/chat/a.b") == "https://claude.ai/chat/a.b")
    #expect(accepted("https://claude.ai/x/...") == "https://claude.ai/x/...")
}

// MARK: - The Service and the picker

@Test func theFirstClaudeLinkInASelectionIsTaken() {
    #expect(
        serviceLink(url: URL(string: "https://claude.ai/a"), text: "https://example.com")
            == "https://claude.ai/a",
        "a link the app sent is taken as it is")
    #expect(
        serviceLink(url: nil, text: "see https://example.com and claude.ai/share/x")
            == "http://claude.ai/share/x")
    #expect(serviceLink(url: nil, text: "claude.ai/chat/abc") == "http://claude.ai/chat/abc")
    #expect(
        serviceLink(url: nil, text: "only https://example.com/page here")
            == "https://example.com/page",
        "another host is kept, so the refusal can name it")
    #expect(serviceLink(url: nil, text: "no link at all") == nil)
    #expect(serviceLink(url: nil, text: nil) == nil)
}

/// The Service runs on the main thread, so only the first 64 KB of a selection is looked
/// through: all of a 10 MB one took 1.67 seconds.
@Test func onlyTheStartOfALargeSelectionIsLookedThrough() {
    let filler = String(repeating: "x ", count: 64 * 1024 / 2)
    #expect(
        serviceLink(url: nil, text: "claude.ai/chat/abc " + filler)
            == "http://claude.ai/chat/abc")
    #expect(serviceLink(url: nil, text: filler + " claude.ai/chat/abc") == nil)
}

@Test func aShownLinkLeavesOutTheScheme() {
    #expect(
        shownLink(URL(string: "https://claude.ai/public/artifacts/0e5a")!)
            == "claude.ai/public/artifacts/0e5a")
    #expect(shownLink(URL(string: "pitboard-fixture://claude.ai/x?y=1")!) == "claude.ai/x?y=1")
}

@Test func anExtensionFindsTheAppItIsIn() {
    let appex = URL(
        fileURLWithPath: "/Applications/Pitboard.app/Contents/PlugIns/PitboardShare.appex")
    #expect(containingApp(of: appex)?.path == "/Applications/Pitboard.app")
    #expect(containingApp(of: URL(fileURLWithPath: "/tmp/PitboardShare.appex")) == nil)
}

/// The picker and the extension say a refusal the same way, from its reason.
@Test func eachRefusalSaysWhy() {
    #expect(
        LinkRefusal.notClaude(host: "example.com").message
            == "pitboard opens claude.ai links only. This link is on example.com.")
    #expect(LinkRefusal.notClaude(host: nil).message == "pitboard opens claude.ai links only.")
    #expect(LinkRefusal.signInLink.message.contains("sign-in links from outside"))
    #expect(LinkRefusal.unknownRequest.message == LinkRefusal.notPitboard.message)
    #expect(LinkRefusal.notPitboard.message.contains("Open claude.ai Link…"))
    #expect(LinkRefusal.ambiguous.message.contains("encoded"))
    #expect(LinkRefusal.noLink.message == "There is no claude.ai link in what was sent.")
    #expect(LinkRefusal.tooLong.message.contains("too long"))
}
