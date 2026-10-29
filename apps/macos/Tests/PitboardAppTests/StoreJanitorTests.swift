import Foundation
import PitboardKit
import Testing

@testable import PitboardApp

// Which stores go, and when, is the model's, tested in pitboard-model's `model/windowing.rs`
// and `account_windows/records.rs`.

@MainActor
private final class Pauses {
    private(set) var asked: [Duration] = []
    func pause(_ duration: Duration) async { asked.append(duration) }
}

/// Each store a janitor told the model of, and whether it went.
@MainActor
private final class Said {
    var told: [(UUID, Bool)] = []
}

@MainActor
private func janitor(_ stores: StandInStores, pauses: Pauses = Pauses()) -> StoreJanitor {
    StoreJanitor(stores: stores, pause: { await pauses.pause($0) })
}

private let one = UUID(uuidString: "00000000-0000-4000-8000-000000000001")!

private func asked(_ ask: UInt64) -> StoreDeletion {
    StoreDeletion(store: one.uuidString.lowercased(), ask: ask)
}

@MainActor
private func delete(_ janitor: StoreJanitor, _ asks: [StoreDeletion], said: Said) async {
    let before = said.told.count
    janitor.delete(asks) { store, went in said.told.append((store, went)) }
    for _ in 0..<500 where said.told.count < before + asks.count { await Task.yield() }
}

/// Measured on macOS 27: a store whose last web view is released stays in use for 20 to 60
/// ms. The janitor waits and tries again, and says it went.
@MainActor
@Test func aStoreInUseIsTriedAgainAfterAPause() async {
    let stores = StandInStores()
    let pauses = Pauses()
    let said = Said()
    let janitor = janitor(stores, pauses: pauses)
    stores.refusals[one] = 2

    await delete(janitor, [asked(1)], said: said)
    #expect(stores.removed == [one])
    #expect(stores.attempts[one] == 3)
    #expect(pauses.asked == Array(StoreJanitor.retries.prefix(2)))
    #expect(said.told.map(\.0) == [one])
    #expect(said.told.map(\.1) == [true])
}

/// The model asks for a held store again after its next read, which deletes it once nothing
/// holds it.
@MainActor
@Test func aStoreStillHeldIsSaidToBeHeld() async {
    let stores = StandInStores()
    let pauses = Pauses()
    let said = Said()
    let janitor = janitor(stores, pauses: pauses)
    stores.refusals[one] = 100

    await delete(janitor, [asked(1)], said: said)
    #expect(stores.removed.isEmpty)
    #expect(stores.attempts[one] == StoreJanitor.retries.count + 1)
    #expect(pauses.asked == StoreJanitor.retries)
    #expect(said.told.map(\.1) == [false])

    stores.refusals[one] = 0
    await delete(janitor, [asked(2)], said: said)
    #expect(stores.removed == [one])
    #expect(said.told.map(\.1) == [false, true])
}

@Test func theRetriesEndWithinFourSeconds() {
    let total = StoreJanitor.retries.reduce(Duration.zero, +)
    #expect(total < .seconds(4))
    #expect(StoreJanitor.retries.first == .milliseconds(50))
}

/// An ask that several snapshots list before the model has heard how it went is deleted
/// once, while a new ask for the same store, after it was held, is deleted again.
@MainActor
@Test func anAskIsDeletedOnce() async {
    let stores = StandInStores()
    let said = Said()
    let janitor = janitor(stores)
    stores.refusals[one] = 1

    janitor.delete([asked(1)]) { store, went in said.told.append((store, went)) }
    await delete(janitor, [asked(1)], said: said)
    #expect(stores.removed == [one])
    #expect(stores.attempts[one] == 2, "one deletion, tried twice")
    #expect(said.told.count == 1)

    await delete(janitor, [asked(2)], said: said)
    #expect(stores.attempts[one] == 3, "a new ask")
}

/// Without `HOME`, a library an app ships takes the account's own home, where Foundation
/// finds it. A library built with the fixtures, as CI builds the one these tests run, is told
/// there is none, so nothing it is given reaches a real home: its directory is then
/// `.pitboard` alone, which the core refuses as a home that is not a full path.
@Test func theRecordsAreKeptUnderTheCoresDirectoryStandardised() {
    let dotted = ["PITBOARD_HOME": "/Users/dana/./work/../pitboard/"]
    #expect(pitboardDirectory(environment: dotted) == "/Users/dana/./work/../pitboard/")
    #expect(WebEnvironment.recordKey(environment: dotted) == "/Users/dana/pitboard")
    #expect(
        WebEnvironment.recordKey(environment: ["HOME": "/Users/dana"])
            == "/Users/dana/.pitboard")
    #expect(
        WebEnvironment.recordKey(environment: ["HOME": "/Users/dana/"])
            == "/Users/dana/.pitboard")
    if fixtureNames().isEmpty {
        let home = FileManager.default.homeDirectoryForCurrentUser
        #expect(
            WebEnvironment.recordKey(environment: [:])
                == home.appendingPathComponent(".pitboard").standardizedFileURL.path,
            "without HOME, the account's own, where Foundation found it")
    } else {
        #expect(
            pitboardDirectory(environment: [:]) == ".pitboard",
            "a library with fixtures names no home of its own")
    }
}

/// The records' key is Foundation's, which the model takes as the app gives it: by name it
/// drops `.`, `..`, doubled and trailing slashes, resolves a relative directory against the
/// current one, and decomposes a name as HFS+ does; by the file system it resolves links where
/// a `..` is and the rest is there, and drops `/private` where what follows is there
/// (docs/facts/foundation-urls.md).
@Test func theRecordsKeyIsFoundationsByNameAndByTheFileSystem() throws {
    func key(_ directory: String) -> String {
        WebEnvironment.recordKey(environment: ["PITBOARD_HOME": directory])
    }
    #expect(key("/Users/dana//./work/../pitboard//") == "/Users/dana/pitboard")
    #expect(key("//Users/dana/.pitboard/.") == "/Users/dana/.pitboard")
    #expect(key("/../x") == "/x")
    #expect(key("pitboard") == FileManager.default.currentDirectoryPath + "/pitboard")
    #expect(key("/Users/dana/B\u{E1}o c\u{E1}o") == "/Users/dana/Ba\u{301}o ca\u{301}o")
    #expect(key("/Users/dana/\u{D55C}") == "/Users/dana/\u{1112}\u{1161}\u{11AB}")
    #expect(key("/Users/dana/\u{F900}") == "/Users/dana/\u{F900}", "HFS+ keeps U+F900")
    #expect(key("/Users/dana/\u{105C9}") == "/Users/dana/\u{105C9}", "and Unicode 16's Todhri")

    let scratch = FileManager.default.temporaryDirectory
        .appendingPathComponent("pitboard-key-\(UUID().uuidString)")
    let inner = scratch.appendingPathComponent("real/inner")
    try FileManager.default.createDirectory(at: inner, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: scratch) }
    let link = scratch.appendingPathComponent("link")
    try FileManager.default.createSymbolicLink(at: link, withDestinationURL: inner)
    let here = scratch.standardizedFileURL.path
    #expect(key(link.path + "/..") == here + "/real", "through the link, since it is there")
    #expect(key(link.path + "/../nowhere") == here + "/nowhere", "by name, since it is not")
    #expect(key("/private" + here) == here, "the folder is there without /private")
    #expect(key("/private/var/pitboard-nowhere") == "/private/var/pitboard-nowhere")
}

/// Nothing is written.
@Test func theRecordsAreKeptInTheAppsOwnFolder() throws {
    let support = try #require(
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first)
    let directory = WebEnvironment.recordsDirectory(bundle: .main)
    #expect(directory.deletingLastPathComponent().path == support.path)
    #expect(
        directory.lastPathComponent
            == (Bundle.main.bundleIdentifier ?? "com.usepitboard.Pitboard"))
}
