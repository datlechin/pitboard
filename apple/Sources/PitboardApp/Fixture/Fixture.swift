#if DEBUG
    import Foundation
    import PitboardKit

    /// A machine in a known state, for the UI tests and for looking at the app without
    /// touching the one it runs on. Only a debug build has these: a release build never reads
    /// `PITBOARD_FIXTURE`, so nothing outside can put the app in a world that is not real.
    public enum Fixture: String, CaseIterable, Sendable {
        /// Claude Code and Codex, each with an account in use and one to switch to, and a
        /// Claude Code account whose parked login needs a sign-in.
        case twoTools
        /// Claude Code alone, with two accounts.
        case oneTool
        /// Claude Code is installed and nobody is signed in to it.
        case empty
        /// `empty`, opened for the first time.
        case firstLaunch
        /// Neither Claude Code nor Codex is on this machine, and nothing is signed in.
        case noClaudeCode
        /// Somebody is signed in to Claude Code and pitboard has no name for them.
        case unnamed
        /// One Claude Code account, so nothing to switch to.
        case onlyOne
        /// The service could not be reached, and the last numbers measured are shown.
        case readFailure
        /// An interrupted switch that cannot be finished until the service answers.
        case stuck

        /// The environment variable a debug build reads the fixture's name from.
        static let variable = "PITBOARD_FIXTURE"

        /// The defaults a fixture keeps its preferences in, emptied at every launch so each
        /// test starts from the same place and nothing reaches the real app's.
        static let suite = "com.usepitboard.Pitboard.fixture"

        /// The fixture's world. It reads, notices changes and reads when a menu opens, as the
        /// app does on a real machine, since that is what the UI tests are testing; only
        /// notifications are left out. Its claude.ai windows load a stand-in page and keep
        /// nothing on disk. `defaults` stands in for the fixture's suite, for a unit
        /// test that must not leave the suite's file behind.
        @MainActor
        func dependencies(defaults given: UserDefaults? = nil) -> Dependencies {
            let defaults = given ?? UserDefaults(suiteName: Self.suite) ?? .standard
            if given == nil { defaults.removePersistentDomain(forName: Self.suite) }
            if self != .firstLaunch { defaults.set(true, forKey: DefaultsKey.hasBeenSeen) }
            return Dependencies(
                core: FixtureCore(self),
                defaults: defaults,
                loginItem: FixtureLoginItem(),
                commandLineTool: Self.commandLineTool(),
                notifies: false,
                watching: true,
                web: .fixture(),
                linkScheme: Self.linkScheme,
                registersServices: false)
        }

        /// The pitboard link scheme a fixture answers: the debug build's, whichever build
        /// this is, so a UI test's link never reaches an installed copy.
        static let linkScheme = "pitboard-debug"
    }

    extension Fixture {
        /// A command line inside a stand-in app in a temporary directory, and a link that
        /// is made there without asking anyone for a password, so linking it can be tried
        /// without writing to `/usr/local/bin`.
        static func commandLineTool() -> CommandLineTool {
            // One folder, emptied at every launch, rather than one per launch left behind.
            let root = FileManager.default.temporaryDirectory
                .appendingPathComponent("pitboard-fixture")
            try? FileManager.default.removeItem(at: root)
            let helper = root.appendingPathComponent("Pitboard.app/Contents/Helpers/pitboard")
            let bin = root.appendingPathComponent("bin")
            try? FileManager.default.createDirectory(
                at: helper.deletingLastPathComponent(), withIntermediateDirectories: true)
            FileManager.default.createFile(
                atPath: helper.path, contents: Data("#!/bin/sh\n".utf8),
                attributes: [.posixPermissions: 0o755])
            let link = bin.appendingPathComponent("pitboard")
            return CommandLineTool(
                helper: helper.path, installPlaces: [bin.path], link: link.path,
                execute: { _ in
                    try? FileManager.default.createDirectory(
                        at: bin, withIntermediateDirectories: true)
                    try? FileManager.default.createSymbolicLink(
                        at: link, withDestinationURL: helper)
                    return nil
                })
        }
    }

    /// A login item that remembers what it was told and registers nothing.
    @MainActor
    final class FixtureLoginItem: LoginItem {
        private(set) var state: LoginItemState = .disabled
        func register() throws { state = .enabled }
        func unregister() throws { state = .disabled }
        func openSystemSettings() {}
    }
#endif
