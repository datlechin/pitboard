import Foundation
import PitboardKit
import Testing

@testable import PitboardApp

// MARK: - Stand-ins

/// Answers what a test sets and nothing more, so the model can be put in each state a notice
/// is about without a keychain, a network or a tool behind it.
private final class StubCore: Core, @unchecked Sendable {
    var answer: Result<Status, Error>
    var offline: Result<Status, Error> = .success(Status(now: 0, accounts: [], warnings: []))
    var switched: Result<Switched, Error> = .success(
        Switched(outcome: .alreadyActive(label: "work"), warnings: []))
    /// What the next sign-in hands back; nil refuses it the way a missing program does.
    var session: SignIn?
    var abandoned: Abandoned?
    /// The tools whose program the app found.
    var found: [Tool] = [claudeCode]

    init(_ answer: Result<Status, Error>) {
        self.answer = answer
    }

    func status(fresh: Bool) async throws -> Status { try answer.get() }
    func statusOffline() async throws -> Status { try offline.get() }
    func doctor() async -> Diagnosis { Diagnosis(checks: [], healthy: true) }
    func holding(_ provider: String) async -> [Holding] { [] }
    func switchTo(_ label: String) async throws -> Switched { try switched.get() }
    func enrollCurrent(_ label: String) async throws -> Enrolled {
        Enrolled(email: "a@b.c", outcome: .current, warnings: [])
    }
    func forget(_ label: String) async throws -> Changed {
        Changed(email: "a@b.c", warnings: [])
    }
    func rename(_ from: String, to: String) async throws -> Changed {
        Changed(email: "a@b.c", warnings: [])
    }
    func signIn(_ label: String) async throws -> SignIn {
        guard let session else {
            throw PitboardError.Failed(
                code: "claude_program_missing", cause: nil,
                message: "`claude` is not on this machine", warnings: [])
        }
        return session
    }
    func abandonRecovery() async throws -> Abandoned? { abandoned }
    func log(limit: UInt32) async -> [Change] { [] }
    func renew() async -> [Renewed] { [] }
    func schedule() async -> Schedule { .absent }
    func scheduleInstall() async throws -> String { "/nowhere" }
    func scheduleUninstall() async throws -> Bool { false }
    func scheduleRepair() async throws -> Bool { false }
    func changedAt() async -> Int64 { 0 }
    func readingsChangedAt() async -> Int64 { 0 }
    func tools() -> [Tool] { bothTools }
    func installed() async -> [Tool] { found }
    func searchPath() async -> String? { nil }
    func switchToSignedOut(_ provider: String) async throws -> Switched {
        throw PitboardError.Failed(
            code: "unsupported", cause: nil, message: "not in this test", warnings: [])
    }
    func awaitingSignIn() async -> Awaiting? { nil }
    func liveUsage() async -> LiveUsageState {
        LiveUsageState(enabled: false, approval: "unknown", reason: nil, lastOkAt: nil)
    }
    func enableLiveUsage() async throws -> LiveUsageState { await liveUsage() }
    func disableLiveUsage() async throws -> LiveUsageState { await liveUsage() }
}

/// A sign-in the browser has already finished: it says nothing, and finishing it enrols what
/// it was made with.
private final class FinishedSignIn: SignIn, @unchecked Sendable {
    private let enrolls: Enrolled

    init(_ enrolls: Enrolled) {
        self.enrolls = enrolls
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) { fatalError("not from the core") }

    override func nextLine() -> String? { nil }
    override func paste(line: String) throws {}
    override func finish() throws -> Enrolled { enrolls }
    override func cancel() {}
}

/// A switch of `provider`'s tool as the core reports one, `from` and `to` typed the way the
/// core types them: bare for Claude Code, with the tool for any other.
private func switchedOver(
    _ provider: String, from: String, to: String, adoption: Adoption,
    warnings: [Warning] = []
) -> Result<Switched, Error> {
    .success(
        Switched(
            outcome: .switched(provider: provider, from: from, to: to, adoption: adoption),
            warnings: warnings))
}

/// A read the core refused, with the code and message it gives.
private func refused(
    _ code: String, _ message: String, warnings: [Warning] = []
) -> Result<Status, Error> {
    .failure(
        PitboardError.Failed(code: code, cause: nil, message: message, warnings: warnings))
}

private let interrupted = Warning(
    code: "recovery_undetermined",
    message: "A switch from personal to work was interrupted and cannot be finished yet.")
private let overridden = Warning(code: "auth_overridden", message: "ANTHROPIC_API_KEY is set")

/// `base` with what the fixtures leave out: a parked login, why it cannot be used, how long
/// it lasts, or no numbers at all.
private func reported(
    _ base: Account, parked: Parked? = nil, explanation: String? = nil,
    lasts: Int64? = nil, burning: Bool = false, measured: Bool = true
) -> Account {
    Account(
        id: base.id, provider: base.provider, label: base.label, qualified: base.qualified,
        unplaced: base.unplaced, email: base.email, accountUuid: base.accountUuid,
        signedIn: base.signedIn, switchable: base.switchable, parked: parked,
        usage: measured ? base.usage : nil, stale: base.stale, staleExplanation: explanation,
        lastsSeconds: lasts, lastsBurning: burning)
}

/// 12:00 UTC on 14 January 2026, a day far from any change of clocks.
private let midday = Date(timeIntervalSince1970: 1_768_392_000)
/// Noon that day on the clock of the Mac running the tests, so "later today" is today
/// wherever that is.
private let noon = Calendar.current.startOfDay(for: midday).addingTimeInterval(12 * 3600)

private func epoch(_ date: Date) -> Int64 { Int64(date.timeIntervalSince1970) }

/// A time as the menu says it, in the locale of the Mac running the tests.
private func shortTime(_ date: Date) -> String {
    date.formatted(date: .omitted, time: .shortened)
}

private func parked(expiring at: Date) -> Parked {
    Parked(parkedAt: 0, accessExpiresAt: nil, refreshExpiresAt: epoch(at))
}

/// `account` as a row describes it at noon, with nothing else running unless a test says so.
private func described(
    _ account: Account, switching: String? = nil, busy: Bool = false, now: Date = noon
) -> AccountDescription {
    AccountDescription(account, switching: switching, busy: busy, now: now)
}

// MARK: - One account, as the menu and the window describe it

/// A row is called what a person called the account. One with no name yet is called by its
/// address, the one thing about it they will recognise, and a login Pitboard cannot use by
/// what is wrong with it, since it has neither.
@Test func anAccountIsCalledByItsNameOrItsAddressOrWhatIsWrongWithIt() {
    #expect(described(account("work")).title == "work")
    #expect(described(account(nil, signedIn: true, uuid: "u")).title == "u@example.com")
    #expect(described(unplaced(of: "codex")).title == "Login Pitboard can’t use")
}

/// The window's row draws every limit and says whose address it is and whether it is in use,
/// from the description alone.
@Test func aDescriptionCarriesTheAddressWhetherItIsInUseAndEveryLimit() {
    let limits = [window("session", 12), window("weekly_all", 64)]
    let inUse = described(account("work", signedIn: true, limits))
    #expect(inUse.email == "work@example.com")
    #expect(inUse.inUse)
    #expect(inUse.limits == limits)
    #expect(!described(account("spare")).inUse)
    #expect(described(unplaced(of: "codex")).limits.isEmpty, "nothing measured")
}

/// Pressing an account in the menu or the window does the one thing its state allows, decided
/// in one place so the two cannot disagree: switch to it, sign in to it again, name it, or
/// nothing at all.
@Test func pressingAnAccountDoesTheOneThingItsStateAllows() {
    #expect(described(account("spare")).action == .use("claude/spare"))
    #expect(
        described(account("spare", of: "codex")).action == .use("codex/spare"),
        "by its label with its tool, since two tools can each have a spare")
    #expect(described(account("work", signedIn: true)).action == .none, "already in use")
    #expect(
        described(account("stale", switchable: false)).action
            == .signInAgain(provider: "claude", label: "stale"))
    #expect(
        described(account(nil, of: "codex", signedIn: true, uuid: "u")).action
            == .name(provider: "codex", email: "u@example.com"))
    #expect(
        described(account(nil, uuid: "u")).action == .none,
        "only the login signed in now can be named")
    #expect(described(unplaced(of: "codex")).action == .none)
    #expect(described(unplaced(of: "codex", signedIn: true)).action == .none)
}

/// Each change waits for the one before, so while a switch runs nothing can be pressed, and
/// only the account it is for says it is switching.
@Test func aSwitchRunningHoldsBackEveryAccountAndMarksOnlyItsOwn() {
    let running = "claude/spare"
    let spare = described(account("spare", [window("session", 5)]), switching: running)
    #expect(spare.switching)
    #expect(spare.summary == "Switching…")
    #expect(spare.action == .none)

    let other = described(account("other", [window("session", 5)]), switching: running)
    #expect(!other.switching)
    #expect(other.summary == "5-hour 5%")
    #expect(other.action == .none)
    #expect(
        described(account(nil, signedIn: true, uuid: "u"), switching: running).action == .none)
    #expect(
        !described(account("spare", of: "codex"), switching: running).switching,
        "Codex's spare is another account")
    #expect(!described(account(nil, signedIn: true, uuid: "u"), switching: running).switching)
}

/// A sign-in waits on somebody in a browser. It holds back another sign-in and nothing else:
/// switching meanwhile, or naming the login signed in now, is theirs to do.
@Test func aSignInRunningHoldsBackOnlyAnotherSignIn() {
    #expect(described(account("spare"), busy: true).action == .use("claude/spare"))
    #expect(
        described(account(nil, signedIn: true, uuid: "u"), busy: true).action
            == .name(provider: "claude", email: "u@example.com"))
    #expect(described(account("stale", switchable: false), busy: true).action == .none)
}

/// Whether an account needs signing in again is a fact about the account, and a sign-in or a
/// switch running elsewhere does not change it. Its item cannot be chosen meanwhile, and
/// without the line saying why it reads as an account that simply does not work.
@Test
func anAccountThatNeedsSigningInAgainSaysSoWhileSomethingElseRuns() {
    let stale = account("stale", switchable: false, [window("session", 12)])
    #expect(described(stale).summary == "Needs signing in again")
    #expect(described(stale, busy: true).summary == "Needs signing in again")
    #expect(described(stale, switching: "claude/spare").summary == "Needs signing in again")
}

/// The line under an account's name in the menu says what stands in its way before anything
/// about its limits, since that is what decides whether it can be chosen.
@Test func theMenuSaysWhatStandsInAnAccountsWayBeforeItsLimits() {
    let limits = [window("session", 12)]
    let login = unplaced(of: "codex")
    #expect(described(login).summary == login.staleExplanation)
    #expect(
        described(reported(login, explanation: nil)).summary == "Can’t be read or switched")
    #expect(
        described(account(nil, signedIn: true, uuid: "u", limits)).summary == "Not named yet")
    #expect(
        described(account("stale", switchable: false, limits)).summary
            == "Needs signing in again")
}

/// An account with nothing measured yet still has a line under its name, and its address is
/// the one true thing to put there.
@Test func anAccountWithNothingMeasuredIsDescribedByItsAddress() {
    #expect(described(account("spare")).summary == "spare@example.com")
    #expect(
        described(reported(account("spare"), measured: false)).summary == "spare@example.com")
    #expect(described(account("work", signedIn: true)).summary == "work@example.com")
}

/// Every limit in one line, in the order the service gave them, the way a sentence starts: a
/// capital on its first word and on nothing after it. A limit scoped to one model is named
/// with its model, so it does not read as the account's own.
@Test func limitsAreSaidInOneLineWithOnlyItsFirstLetterCapitalised() {
    #expect(
        described(account("work", [window("weekly_all", 64.4), window("session", 12)]))
            .summary == "Weekly 64%, 5-hour 12%")
    #expect(
        described(
            account(
                "work",
                [
                    window("session", 12.5), window("weekly_all", 30),
                    window("weekly_scoped", 98, scope: "Fable"),
                ])
        ).summary == "5-hour 13%, weekly 30%, weekly Fable 98%")
    #expect(
        described(account("work", [window("weekly_scoped", 7, scope: "Fable")])).summary
            == "Weekly Fable 7%")
}

/// A limit that has run out is worth knowing with when it comes back. Later today that is a
/// clock time, and on another day its weekday too, so a menu left open past midnight is still
/// right. Once that moment has passed the next reading is what says it came back, and until
/// then the line says only that it is used up.
@Test func aUsedUpLimitSaysWhenItComesBack() {
    let today = noon.addingTimeInterval(2 * 3600)
    let later = noon.addingTimeInterval(2 * 86_400)
    let spent = { (resets: Int64?) in
        described(account("work", signedIn: true, [window("session", 100, resets: resets)]))
            .summary
    }
    #expect(spent(epoch(today)) == "5-hour used up until \(shortTime(today))")
    let day = later.formatted(.dateTime.weekday(.abbreviated))
    #expect(spent(epoch(later)) == "5-hour used up until \(day) \(shortTime(later))")
    #expect(spent(epoch(noon) - 60) == "5-hour used up")
    #expect(spent(epoch(noon)) == "5-hour used up", "coming back now is not coming back later")
    #expect(spent(nil) == "5-hour 100%", "with no reset known there is only the figure")
    #expect(
        described(
            account(
                "work",
                [window("weekly_all", 100, resets: epoch(today)), window("session", 40)])
        ).summary == "Weekly used up until \(shortTime(today)), 5-hour 40%")
}

/// Why an account cannot be used is said in full only where it cannot be: a login Pitboard
/// cannot use, or an account that cannot be switched to. Numbers that are merely not live do
/// not stop anybody choosing an account, and the account in use needs no switch.
@Test func onlyAnAccountThatCannotBeUsedSaysWhy() {
    let refusedLogin = "Its parked login was refused. Sign in to it again."
    let oldNumbers = "Anthropic did not answer, so these are the last numbers measured."
    let login = unplaced(of: "codex")
    #expect(described(login).problem == login.staleExplanation)
    #expect(
        described(reported(account("stale", switchable: false), explanation: refusedLogin))
            .problem == refusedLogin)
    #expect(described(account("stale", switchable: false)).problem == nil, "nothing to say")
    #expect(
        described(reported(account("spare"), explanation: oldNumbers)).problem == nil,
        "it can still be switched to")
    #expect(
        described(
            reported(
                account("work", signedIn: true, switchable: false), explanation: oldNumbers)
        ).problem == nil, "it is the one in use")
}

/// Numbers that are not new are worth knowing about where the account can still be used: its
/// service could not be reached or is rate limiting, or its session has expired. That is said
/// beside the numbers of the account in use and of one that can be switched to, and not a
/// second time where the account cannot be used and why is said already.
@Test func whyAUsableAccountsNumbersAreOldIsSaidBesideThem() {
    let oldNumbers = "Anthropic did not answer, so these are the last numbers measured."
    let refusedLogin = "Its parked login was refused. Sign in to it again."
    let spare = described(reported(account("spare"), explanation: oldNumbers))
    #expect(spare.staleNote == oldNumbers)
    #expect(spare.problem == nil)
    let inUse = described(
        reported(account("work", signedIn: true, switchable: false), explanation: oldNumbers))
    #expect(inUse.staleNote == oldNumbers)
    #expect(inUse.problem == nil)

    let expired = described(
        reported(account("stale", switchable: false), explanation: refusedLogin))
    #expect(expired.problem == refusedLogin)
    #expect(expired.staleNote == nil, "said once, as why it cannot be used")
    #expect(described(unplaced(of: "codex")).staleNote == nil)
    #expect(described(account("spare")).staleNote == nil, "numbers that are new")
}

/// How long a parked login stays usable is about a switch to it, so it is said of every
/// account but the one in use, whose login is not parked.
@Test func aParkedLoginsLifeIsSaidOnlyOfAnAccountNotInUse() {
    let parking = parked(expiring: noon.addingTimeInterval(3 * 86_400 + 60))
    #expect(
        described(reported(account("spare"), parked: parking)).parkedNote
            == "Parked login good for 3 more days")
    #expect(
        described(reported(account("work", signedIn: true), parked: parking)).parkedNote == nil)
    #expect(described(account("spare")).parkedNote == nil)
}

/// How long an account lasts is a sentence of its own in the window, so it starts with a
/// capital whichever way the account is going. Its span of time reads as the reset beside
/// each bar does, and as `pitboard status` says it, minutes in two digits. Under a minute it
/// says so in words, since rounding to "0 min" reads as though nothing were left and "<1m"
/// is shorthand where a sentence can be plain.
@Test func howLongAnAccountLastsIsSaidAsASentence() {
    let pace = { (left: Int64?, burning: Bool) in
        described(reported(account("work", signedIn: true), lasts: left, burning: burning)).pace
    }
    #expect(pace(5400, true) == "About 1h 30m left at this rate")
    #expect(pace(5400, false) == "Resets in 1h 30m")
    #expect(pace(3900, false) == "Resets in 1h 05m")
    #expect(pace(3 * 86_400 + 7200 + 300, true) == "About 3d 2h left at this rate")
    #expect(pace(30, true) == "About to run out")
    #expect(pace(0, false) == "Resets any moment")
    #expect(pace(nil, true) == nil, "nothing to go on yet")
}

// MARK: - The menu bar

/// The settings choose how much the item says beside its mark. The mark alone says nothing
/// beside it, whatever there is to say.
@Test func theMenuBarSaysAsMuchAsTheSettingsAskAndTheMarkAloneSaysNothing() {
    let read = status([account("work", signedIn: true, [window("session", 42)])])
    #expect(menuTitle(for: read, showing: .nameAndUsage) == "work 42%")
    #expect(
        menuTitle(for: read) == "work 42%", "the name and the figure unless asked otherwise")
    #expect(menuTitle(for: read, showing: .usage) == "42%")
    let every: [Status?] = [
        nil, status([]), read,
        status([account("a-very-long-account-label", signedIn: true)]),
        status([account(nil, signedIn: true, uuid: "u", [window("session", 99)])]),
    ]
    for shown in every {
        #expect(menuTitle(for: shown, order: bothTools, showing: .icon).isEmpty)
    }
}

/// The bar is shared with everything else running, so a name longer than twelve characters
/// is cut to eleven and an ellipsis, and a name that fits is left whole.
@Test func aNameLongerThanTwelveCharactersIsCutToElevenAndAnEllipsis() {
    let bar = { (label: String, shows: MenuBarShows) in
        menuTitle(
            for: status([account(label, signedIn: true, [window("session", 5)])]),
            showing: shows)
    }
    #expect(bar("abcdefghijkl", .nameAndUsage) == "abcdefghijkl 5%")
    #expect(bar("abcdefghijklm", .nameAndUsage) == "abcdefghijk… 5%")
    #expect(bar("a-very-long-account-label", .nameAndUsage) == "a-very-long… 5%")
    #expect(bar("a-very-long-account-label", .usage) == "5%")
}

/// Before anything is measured the bar still says whose account is in use, and a setting
/// that asks for the figure alone shows the mark until there is one.
@Test func anAccountWithNothingMeasuredIsNamedAloneInTheBar() {
    let unmeasured = status([account("work", signedIn: true)])
    #expect(menuTitle(for: unmeasured) == "work")
    #expect(
        menuTitle(for: status([reported(account("work", signedIn: true), measured: false)]))
            == "work")
    #expect(menuTitle(for: status([account(nil, signedIn: true, uuid: "u")])) == "unnamed")
    #expect(menuTitle(for: unmeasured, showing: .usage).isEmpty)
}

/// With nobody signed in there is no account in use to name, and naming one that is not
/// would be wrong.
@Test func nobodySignedInLeavesTheBarToItsMark() {
    let read = status([account("work", [window("session", 5)]), account("spare")])
    for shows in MenuBarShows.allCases {
        #expect(menuTitle(for: read, order: bothTools, showing: shows).isEmpty)
    }
}

/// The figure is a whole percentage, rounded the way a person rounds: a half goes up.
@Test func theBarsFigureIsRoundedToTheNearestWholePercent() {
    let bar = { (percent: Double) in
        menuTitle(
            for: status([account("work", signedIn: true, [window("session", percent)])]),
            showing: .usage)
    }
    #expect(bar(64.4) == "64%")
    #expect(bar(64.5) == "65%")
    #expect(bar(0.4) == "0%")
    #expect(bar(100) == "100%")
}

/// With an account in use in each tool and one bar, the bar is about whichever is closest to
/// running out, judged by its own limits and not one scoped to a single model.
@Test func theBarIsAboutTheToolClosestToRunningOut() {
    let read = [
        account(
            "work", signedIn: true,
            [window("session", 30), window("weekly_scoped", 99, scope: "Fable")]),
        account("job", of: "codex", signedIn: true, [window("five_hour", 50)]),
    ]
    #expect(titled(read, order: bothTools)?.label == "job")
    #expect(menuTitle(for: status(read), order: bothTools) == "job 50%")
}

/// A tie goes to the tool the listing puts first, whatever order the rows came in, so the bar
/// does not move between two accounts at the same figure from one read to the next.
@Test func aTieGoesToTheToolListedFirstAndNotTheRowThatCameFirst() {
    let tied = [
        account("job", of: "codex", signedIn: true, [window("five_hour", 50)]),
        account("work", signedIn: true, [window("session", 50)]),
    ]
    #expect(titled(tied, order: [claudeCode, codex])?.label == "work")
    #expect(titled(tied, order: [codex, claudeCode])?.label == "job")
}

/// An account with nothing measured is not closer to running out than one at nought.
@Test func anAccountWithNothingMeasuredGivesTheBarToOneThatWasMeasured() {
    let read = [
        account("work", signedIn: true),
        account("job", of: "codex", signedIn: true, [window("five_hour", 0)]),
    ]
    #expect(titled(read, order: bothTools)?.label == "job")
}

/// With nobody enrolled signed in to any tool the bar says what is signed in, as it would with
/// one tool, rather than nothing. With nobody signed in at all it is about nobody.
@Test func withNobodyEnrolledSignedInTheBarIsAboutWhoeverIsSignedIn() {
    let loginOnly = [
        account("work"),
        account(nil, of: "codex", signedIn: true, uuid: "c", [window("five_hour", 3)]),
    ]
    #expect(titled(loginOnly, order: bothTools)?.id == "codex:c")
    #expect(menuTitle(for: status(loginOnly), order: bothTools) == "unnamed 3%")
    #expect(titled([account("work"), account("job", of: "codex")], order: bothTools) == nil)
    #expect(titled([], order: bothTools) == nil)
}

// MARK: - Sections

/// A tool Pitboard does not list yet still gets a section, after the ones it lists, headed by
/// its code, rather than its accounts going missing.
@Test func aToolNobodyListsIsGroupedLastUnderItsCode() {
    let groups = grouped(
        [account("g", of: "gemini"), account("job", of: "codex"), account("work")],
        by: bothTools)
    #expect(groups.map(\.id) == ["claude", "codex", "gemini"])
    #expect(groups.map(\.name) == ["Claude Code", "Codex", "gemini"])
    #expect(groups.map { $0.accounts.compactMap(\.label) } == [["work"], ["job"], ["g"]])
}

/// One tool's accounts are one section with no heading, known by the tool's code, with the
/// rows in the order they came.
@Test func oneToolsAccountsAreOneSectionKnownByItsCode() {
    let accounts = [account("spare", of: "codex"), account("job", of: "codex", signedIn: true)]
    let groups = grouped(accounts, by: bothTools)
    #expect(groups.map(\.id) == ["codex"])
    #expect(groups.first?.name == nil)
    #expect(groups.first?.accounts == accounts)
}

/// Sections come in the order the tools are listed, each once, whatever order the rows came
/// in, and a code the listing does not know follows in the order it first appeared.
@Test func toolCodesFollowTheListingEachOnce() {
    #expect(
        inOrder(["x", "codex", "claude", "codex", "y", "x"], by: bothTools)
            == ["claude", "codex", "x", "y"])
    #expect(
        inOrder(["codex"], by: bothTools) == ["codex"], "a listed tool with no rows is left out"
    )
    #expect(inOrder(["b", "a", "b"], by: []) == ["b", "a"])
    #expect(inOrder([], by: bothTools).isEmpty)
}

// MARK: - What the window and the menu have to tell somebody

/// A machine with nothing wrong has nothing to say, before the first read and after it.
@MainActor
@Test func aMachineWithNothingWrongHasNoNotices() async {
    let model = AppModel(
        testing: StubCore(
            .success(
                status([
                    account("work", signedIn: true, [window("session", 10)]),
                    account("spare"),
                ]))))
    #expect(model.notices().isEmpty)
    await model.refresh()
    #expect(model.notices().isEmpty)
}

/// An interrupted switch nothing can finish stops Pitboard working, so it is said first, with
/// the way out. The warning the read carries about it is the same fact and is not said a
/// second time; anything else the read warned about still is.
@MainActor
@Test func anInterruptedSwitchIsSaidFirstWithTheWayOutAndOnlyOnce() async {
    let read = status(
        [account("work", signedIn: true), account("job", of: "codex", signedIn: true)],
        warnings: [interrupted, overridden])
    let model = AppModel(testing: StubCore(.success(read)))
    await model.refresh()

    let notices = model.notices()
    #expect(notices.count == 2)
    #expect(
        notices.first
            == Notice(
                id: "stuck", severity: .error, title: "An interrupted switch is waiting",
                lines: [
                    "An interrupted switch can’t be finished until Anthropic or OpenAI "
                        + "answers.",
                    "Giving up on it keeps every login. Nothing is deleted.",
                ],
                actions: [.giveUp]))
    #expect(notices.last?.title == "An environment variable overrides the login")
    #expect(!notices.contains { $0.lines.contains(interrupted.message) })
}

/// When the read itself failed because of the interrupted switch, what stopped it is the
/// reason given, and it is not said again as a read that failed.
@MainActor
@Test func anInterruptedSwitchThatStoppedTheReadGivesItsReason() async {
    let reason = "Anthropic could not be reached to finish the switch to work."
    let core = StubCore(refused("recovery_undetermined", reason, warnings: [interrupted]))
    core.offline = .success(status([account("work", signedIn: true)]))
    let model = AppModel(testing: core)
    await model.refresh()

    #expect(
        model.notices() == [
            Notice(
                id: "stuck", severity: .error, title: "An interrupted switch is waiting",
                lines: [reason, "Giving up on it keeps every login. Nothing is deleted."],
                actions: [.giveUp])
        ])
}

/// A read that failed says why, and that the numbers shown are the last ones measured rather
/// than now's. A warning that only repeats why is not said twice; any other it carried is.
@MainActor
@Test func aFailedReadSaysWhyAndThatTheNumbersAreOld() async {
    let reason = "Anthropic could not be reached"
    let core = StubCore(
        refused(
            "unreachable", reason,
            warnings: [Warning(code: "unreachable", message: reason), overridden]))
    core.offline = .success(status([account("work", signedIn: true, [window("session", 10)])]))
    let model = AppModel(testing: core)
    await model.refresh()

    let notices = model.notices()
    #expect(
        notices.map(\.title)
            == ["Couldn’t read usage", "An environment variable overrides the login"])
    #expect(
        notices.first
            == Notice(
                id: "read", severity: .error, title: "Couldn’t read usage",
                lines: [reason, "The numbers shown are the last ones measured."], actions: []))
    #expect(notices.last?.lines == [overridden.message])
}

/// The line saying the numbers shown are the last ones measured is said only where there are
/// numbers shown. With nothing measured, or nothing known at all, it pointed at numbers that
/// were not there.
@MainActor
@Test func aFailedReadSaysTheNumbersAreOldOnlyWhereThereAreSome() async {
    let reason = "Anthropic could not be reached"
    let known: [Result<Status, Error>] = [
        .success(status([reported(account("work", signedIn: true), measured: false)])),
        .success(status([])),
        refused("state_unreadable", "~/.pitboard/state.json could not be read"),
    ]
    for offline in known {
        let core = StubCore(refused("unreachable", reason))
        core.offline = offline
        let model = AppModel(testing: core)
        await model.refresh()
        #expect(model.notices().first?.lines == [reason])
    }
}

/// A failed read with nothing to list is the window's whole content, with why and a way to try
/// again, and not a spinner that never stops or a list with nothing in it. The window keys
/// that off the read's problem with no account to show, whether nothing is known at all or
/// what is known is empty, and not off a machine without Claude Code, which says that instead.
/// With accounts to show, the list stays, with the failure above it as a notice.
@MainActor
@Test func aFailedReadWithNothingToListIsWhatTheWindowShows() async {
    let reason = "~/.pitboard/state.json was written on another Mac."
    let failed = refused("state_wrong_machine", reason)

    let nothing = StubCore(failed)
    nothing.offline = refused("state_wrong_machine", reason)
    let unknown = AppModel(testing: nothing)
    await unknown.refresh()
    #expect(unknown.status == nil)
    #expect(unknown.problem == reason)
    #expect(unknown.footing == .ready, "not a machine without Claude Code")

    let empty = AppModel(testing: StubCore(failed))
    await empty.refresh()
    #expect(empty.status?.accounts.isEmpty == true)
    #expect(empty.problem == reason)
    #expect(empty.footing == .noOneSignedIn, "the read's problem is said ahead of it")

    let listed = StubCore(failed)
    listed.offline = .success(status([account("work", signedIn: true, [window("session", 5)])]))
    let some = AppModel(testing: listed)
    await some.refresh()
    #expect(some.status?.accounts.isEmpty == false)
    #expect(some.notices().map(\.id) == ["read"])

    let bare = StubCore(failed)
    bare.found = []
    let missing = AppModel(testing: bare)
    await missing.refresh()
    #expect(missing.status?.accounts.isEmpty == true)
    #expect(missing.footing == .noClaudeCode)
}

/// Without a tool there is nothing to read, and the menu says how to install one instead.
/// A failed read beside that would be the same fact said as a fault. Once another tool has an
/// account here the machine is not without a tool, and a failed read is one.
@MainActor
@Test func aMachineWithoutClaudeCodeIsNotToldItsReadFailed() async {
    let missing = refused("unreachable", "Anthropic could not be reached")
    let nothing = StubCore(missing)
    nothing.found = []
    let bare = AppModel(testing: nothing)
    await bare.refresh()
    #expect(bare.footing == .noClaudeCode)
    #expect(bare.notices().isEmpty)

    let core = StubCore(missing)
    core.found = []
    core.offline = .success(status([account("job", of: "codex", signedIn: true)]))
    let withCodex = AppModel(testing: core)
    await withCodex.refresh()
    #expect(withCodex.notices().map(\.id) == ["read"])
}

/// Advice offers the account of the same tool with the most room, as a button that switches
/// to it by its label with its tool. It names the tool only once two are shown, so a machine
/// with one tool reads as it always did.
@MainActor
@Test func adviceOffersTheAccountWithRoomAndNamesItsToolOnlyBesideAnother() async {
    let spent = [
        account("work", signedIn: true, [window("session", 100)]),
        account("spare", [window("session", 20)]),
    ]
    let alone = AppModel(testing: StubCore(.success(status(spent))))
    await alone.refresh()
    #expect(
        alone.notices() == [
            Notice(
                id: "advice/claude/work/session/", severity: .warning,
                title: "work has no 5-hour limit left",
                lines: ["spare has 80% of its own left."],
                actions: [.use(qualified: "claude/spare", label: "spare")])
        ])

    let beside = AppModel(
        testing: StubCore(
            .success(status(spent + [account("job", of: "codex", signedIn: true)]))))
    await beside.refresh()
    #expect(beside.notices().map(\.title) == ["Claude Code: work has no 5-hour limit left"])
}

/// A switch of one tool leaves another tool's accounts as they were. Advice about a Claude
/// Code account still out, beside another that still has room, is as true after a Codex switch
/// as before it, and it is never told again, so putting it away loses it.
@MainActor
@Test
func adviceAboutOneToolOutlivesASwitchOfAnother() async {
    let spent = account("work", signedIn: true, [window("session", 100)])
    let spare = account("spare", [window("session", 20)])
    let core = StubCore(
        .success(
            status([
                spent, spare, account("side", of: "codex", signedIn: true),
                account("job", of: "codex"),
            ])))
    let model = AppModel(testing: core)
    await model.refresh()
    #expect(model.notices().map(\.id) == ["advice/claude/work/session/"])

    core.answer = .success(
        status([
            spent, spare, account("side", of: "codex"),
            account("job", of: "codex", signedIn: true),
        ]))
    core.switched = switchedOver(
        "codex", from: "codex/side", to: "codex/job", adoption: .restart(program: "codex"))
    await model.use("codex/job")
    #expect(model.notices().map(\.id) == ["advice/claude/work/session/", "switch/codex"])
}

/// Sessions of a tool that follows a switch by itself pick it up within a moment, and the
/// notice counts down to it. Once that moment has passed a countdown would count to something
/// already over, so there is none. Dismissing the notice puts it away.
@MainActor
@Test func aSwitchCountsDownOnlyUntilOpenSessionsHaveFollowed() async throws {
    let core = StubCore(
        .success(status([account("work", signedIn: true), account("personal")])))
    core.switched = switchedOver(
        "claude", from: "personal", to: "work", adoption: .follows(withinSeconds: 33))
    let model = AppModel(testing: core)
    await model.use("claude/work")
    let adopted = try #require(model.lastSwitches.first?.adopted)

    let counting = Notice(
        id: "switch/claude", severity: .info, title: "Switched to work", lines: [],
        follows: adopted, followsLabel: "Sessions already open follow in",
        actions: [.dismissSwitch(provider: "claude")])
    #expect(model.notices(at: adopted.addingTimeInterval(-10)) == [counting])

    var followed = counting
    followed.follows = nil
    followed.followsLabel = nil
    #expect(model.notices(at: adopted) == [followed])
    #expect(model.notices(at: adopted.addingTimeInterval(60)) == [followed])

    model.forgetSwitch(of: "claude")
    #expect(model.notices(at: adopted.addingTimeInterval(-10)).isEmpty)
}

/// With two tools each switch is said on its own, so it names its tool, and so does its
/// countdown: one beside a Codex notice would otherwise read as contradicting it.
@MainActor
@Test func aSwitchNamesItsToolOnlyBesideAnother() async throws {
    let core = StubCore(
        .success(
            status([
                account("work", signedIn: true), account("job", of: "codex", signedIn: true),
            ])
        ))
    core.switched = switchedOver(
        "claude", from: "personal", to: "work", adoption: .follows(withinSeconds: 33))
    let model = AppModel(testing: core)
    await model.use("claude/work")
    let adopted = try #require(model.lastSwitches.first?.adopted)

    let notice = try #require(model.notices(at: adopted.addingTimeInterval(-1)).first)
    #expect(notice.title == "Switched Claude Code to work")
    #expect(notice.followsLabel == "Claude Code sessions already open follow in")
}

/// A running `codex` never picks a switch up, so a countdown would promise what does not
/// happen. The notice is a warning that says to start its sessions again, or, once the core
/// has counted them, the core's own warning, which says the same with the count.
@MainActor
@Test func aSwitchThatNeedsARestartIsAWarningWithoutACountdown() async {
    let core = StubCore(
        .success(
            status([
                account("work", of: "codex", signedIn: true), account("personal", of: "codex"),
            ])))
    core.switched = switchedOver(
        "codex", from: "codex/personal", to: "codex/work", adoption: .restart(program: "codex"))
    let model = AppModel(testing: core)
    await model.use("codex/work")
    #expect(
        model.notices() == [
            Notice(
                id: "switch/codex", severity: .warning, title: "Switched to work",
                lines: [
                    "Any codex session started before this switch keeps using personal until "
                        + "it is quit and started again."
                ],
                actions: [.dismissSwitch(provider: "codex")])
        ])

    let counted = Warning(
        code: "sessions_still_running",
        message: "2 `codex` sessions started before this switch are still running.")
    core.switched = switchedOver(
        "codex", from: "codex/personal", to: "codex/work", adoption: .restart(program: "codex"),
        warnings: [counted])
    await model.use("codex/work")
    let notices = model.notices()
    #expect(notices.map(\.severity) == [.warning])
    #expect(notices.first?.lines == [counted.message])
    #expect(notices.first?.follows == nil)
}

/// Signing in to the account in use puts its new login in use at once, and sessions already
/// running keep the old one, exactly as a switch leaves them on the old account. So it is said
/// the way a switch is: the account has a new login, and what that means for them.
@MainActor
@Test func aSignInThatPutANewLoginInUseSaysTheAccountHasOne() async {
    let core = StubCore(.success(status([account("work", signedIn: true)])))
    core.session = FinishedSignIn(
        Enrolled(email: "work@example.com", outcome: .inUse(again: true), warnings: []))
    let model = AppModel(testing: core)
    #expect(await model.signIn("work", for: "claude") == nil)
    #expect(
        model.notices() == [
            Notice(
                id: "switch/claude", severity: .info, title: "work has a new login",
                lines: ["Signed in to work again. Its new login is the one in use now."],
                actions: [.dismissSwitch(provider: "claude")])
        ])

    let oldLogin = Warning(
        code: "sessions_keep_old_login",
        message: "2 `codex` sessions are still using `codex/work`'s old login.")
    let codexCore = StubCore(.success(status([account("work", of: "codex", signedIn: true)])))
    codexCore.session = FinishedSignIn(
        Enrolled(
            email: "work@example.com", outcome: .inUse(again: false), warnings: [oldLogin]))
    let codexModel = AppModel(testing: codexCore)
    await codexModel.signIn("work", for: "codex")
    #expect(
        codexModel.notices() == [
            Notice(
                id: "switch/codex", severity: .warning, title: "work has a new login",
                lines: [
                    "Enrolled work, the account signed in now. Its new login is the one in "
                        + "use.",
                    oldLogin.message,
                ],
                actions: [.dismissSwitch(provider: "codex")])
        ])
}

/// Two tools can each have a `work`, so once both are shown a notice about one account says
/// which tool it is for, as a switch's notice and advice both do.
@MainActor
@Test
func aSignInNoticeNamesItsToolBesideAnother() async throws {
    let core = StubCore(
        .success(
            status([
                account("work", signedIn: true), account("work", of: "codex", signedIn: true),
            ])))
    core.session = FinishedSignIn(
        Enrolled(email: "work@example.com", outcome: .inUse(again: true), warnings: []))
    let model = AppModel(testing: core)
    await model.signIn("work", for: "codex")
    let notice = try #require(model.notices().first)
    #expect(notice.id == "switch/codex")
    #expect(notice.title.contains("Codex"))
}

/// Every warning a read carries is a notice of its own, headed from its code so the menu can
/// name it in a line, with the core's message under it saying what to do. Two warnings of one
/// code are two notices, each with an identity of its own.
@MainActor
@Test func eachWarningAReadCarriesIsANoticeHeadedFromItsCode() async {
    let second = Warning(code: "auth_overridden", message: "CLAUDE_CODE_OAUTH_TOKEN is set")
    let novel = Warning(code: "something_new", message: "Something Pitboard does not know yet")
    let warned = [overridden, novel, second]
    let model = AppModel(
        testing: StubCore(.success(status([account("work", signedIn: true)], warnings: warned)))
    )
    await model.refresh()

    let notices = model.notices()
    #expect(
        notices.map(\.title) == [
            "An environment variable overrides the login", "Pitboard has a warning",
            "An environment variable overrides the login",
        ])
    #expect(notices.map(\.lines) == warned.map { [$0.message] })
    #expect(notices.allSatisfy { $0.severity == .warning && $0.actions.isEmpty })
    #expect(zip(notices, warned).allSatisfy { $0.id.hasPrefix("warning/\($1.code)/") })
    #expect(Set(notices.map(\.id)).count == warned.count)
}

/// A warning a switch carried is said with the switch, where it belongs, and nowhere else.
@MainActor
@Test func aWarningTheLastSwitchCarriedIsSaidWithItAndNowhereElse() async {
    let core = StubCore(.success(status([account("work", signedIn: true)])))
    core.switched = switchedOver(
        "claude", from: "personal", to: "work", adoption: .follows(withinSeconds: 33),
        warnings: [overridden])
    let model = AppModel(testing: core)
    await model.use("claude/work")

    let notices = model.notices()
    #expect(notices.map(\.id) == ["switch/claude"])
    #expect(notices.first?.severity == .warning)
    #expect(notices.first?.lines == [overridden.message])
}

/// An environment variable overriding the login is warned about by the switch and by every
/// read after it. The switch leaves it to the read, so the read has to say it: a warning both
/// carry is said once, and not dropped from both.
@MainActor
@Test
func aWarningTheSwitchAndTheReadAfterItBothCarryIsSaidOnce() async {
    let core = StubCore(
        .success(status([account("work", signedIn: true)], warnings: [overridden])))
    core.switched = switchedOver(
        "claude", from: "personal", to: "work", adoption: .follows(withinSeconds: 33),
        warnings: [overridden])
    let model = AppModel(testing: core)
    await model.use("claude/work")

    let said = model.notices().flatMap(\.lines).filter { $0 == overridden.message }
    #expect(said.count == 1)
}

/// Giving up on an interrupted switch deletes nothing, and says so, with how many logins were
/// kept, until somebody puts it away.
@MainActor
@Test func givingUpOnASwitchSaysWhatWasKeptUntilPutAway() async {
    let core = StubCore(
        .success(status([account("work", signedIn: true)], warnings: [interrupted])))
    let model = AppModel(testing: core)
    await model.refresh()
    #expect(model.notices().map(\.id) == ["stuck"])

    core.answer = .success(status([account("work", signedIn: true)]))
    core.abandoned = Abandoned(from: "personal", to: "work", loginsKept: 2)
    #expect(await model.abandonStuckSwitch() == nil)
    #expect(
        model.notices() == [
            Notice(
                id: "abandoned", severity: .info, title: "Gave up on the interrupted switch",
                lines: [
                    "The switch from personal to work was given up. 2 logins kept, and "
                        + "nothing was deleted."
                ],
                actions: [.dismissAbandoned])
        ])

    core.abandoned = Abandoned(from: "personal", to: "work", loginsKept: 1)
    await model.abandonStuckSwitch()
    #expect(
        model.notices().first?.lines == [
            "The switch from personal to work was given up. 1 login kept, and nothing was "
                + "deleted."
        ])

    model.forgetAbandoned()
    #expect(model.notices().isEmpty)
}

/// The most pressing first: what stops Pitboard working, then an account that ran out, then
/// what each tool's last switch said, then warnings, then what is only worth knowing.
@MainActor
@Test func noticesComeTheMostPressingFirst() async {
    let accounts = { (work: Double) in
        [
            account("work", signedIn: true, [window("session", work)]),
            account("spare", [window("session", 20)]),
            account("job", of: "codex", signedIn: true),
        ]
    }
    let core = StubCore(.success(status(accounts(10), warnings: [interrupted])))
    let model = AppModel(testing: core)
    await model.refresh()

    core.answer = .success(status(accounts(10)))
    core.abandoned = Abandoned(from: "personal", to: "work", loginsKept: 2)
    await model.abandonStuckSwitch()

    core.switched = switchedOver(
        "codex", from: "codex/side", to: "codex/job", adoption: .restart(program: "codex"))
    await model.use("codex/job")

    core.answer = .success(status(accounts(100)))
    await model.refresh()

    core.answer = refused(
        "unreachable", "Anthropic could not be reached", warnings: [overridden])
    await model.refresh()

    let notices = model.notices()
    #expect(
        notices.map { $0.id.prefix { $0 != "/" } }
            == ["read", "advice", "switch", "warning", "abandoned"])
    #expect(notices.map(\.severity) == [.error, .warning, .warning, .warning, .info])
}

// MARK: - Headings, actions and levels

/// A warning is named in the menu by a heading from its code, each its own, and a code the app
/// does not know yet still gets one rather than an empty line.
@Test func everyWarningCodeHasItsOwnHeadingAndAnUnknownOneAGeneralOne() {
    let headings = [
        "sessions_still_running": "Open sessions still use the previous account",
        "sessions_keep_old_login": "Open sessions still use the old login",
        "auth_overridden": "An environment variable overrides the login",
        "parked_login_refused": "A parked login was refused",
        "lock_compromised": "The login may have been written twice",
        "parks_pending_removal": "Old parked logins are still there",
        "written_on_the_command_line": "A login was passed on the command line",
        "sign_in_parked_not_in_use": "The new login was parked, not put in use",
        "interrupted_switch_finished": "An interrupted switch was finished",
        "interrupted_switch_undone": "An interrupted switch was undone",
        "recovery_undetermined": "An interrupted switch is waiting",
    ]
    for (code, heading) in headings {
        #expect(warningTitle(Warning(code: code, message: "")) == heading, "\(code)")
    }
    #expect(Set(headings.values).count == headings.count, "no two codes read alike")
    #expect(
        warningTitle(Warning(code: "not_a_code_yet", message: "x")) == "Pitboard has a warning")
    #expect(warningTitle(Warning(code: "", message: "")) == "Pitboard has a warning")
}

/// What can be done about a notice is a button under it, and putting it away is the icon at
/// its end: never both, and never neither.
@Test func aNoticesActionIsEitherAButtonOrWhatPutsItAway() {
    let actions: [Notice.Action] = [
        .use(qualified: "codex/spare", label: "spare"), .giveUp,
        .dismissSwitch(provider: "codex"), .dismissAbandoned,
    ]
    #expect(actions.map(\.title) == ["Switch to spare", "Give Up…", "Dismiss", "Dismiss"])
    #expect(actions.map(\.dismisses) == [false, false, true, true])
    #expect(actions.map(\.isButton) == [true, true, false, false])
}

/// The menu turns advice into an item that switches, and only advice: a notice offers an
/// account when one of its actions is a switch to it.
@Test func aNoticeOffersAnAccountOnlyWhenItCanSwitchToOne() throws {
    let notice = { (actions: [Notice.Action]) in
        Notice(id: "n", severity: .warning, title: "", lines: [], actions: actions)
    }
    let offered = try #require(
        notice([
            .dismissSwitch(provider: "codex"), .use(qualified: "codex/spare", label: "spare"),
        ]).switchesTo)
    #expect(offered.qualified == "codex/spare")
    #expect(offered.label == "spare")
    let none: [[Notice.Action]] = [
        [], [.giveUp], [.dismissSwitch(provider: "claude")], [.dismissAbandoned],
    ]
    for actions in none {
        #expect(notice(actions).switchesTo == nil)
    }
}

/// A notice is shown as a shape and a colour and said as a word. If two severities ever
/// looked or sounded the same an error would pass for a note, and they order by how pressing
/// they are.
@Test func everySeverityLooksAndSoundsLikeItselfAndOrdersByHowPressingItIs() {
    let severities: [Notice.Severity] = [.info, .warning, .error]
    #expect(Set(severities.map(\.symbol)).count == severities.count)
    #expect(Set(severities.map(\.spoken)).count == severities.count)
    #expect([Notice.Severity.error, .info, .warning].sorted() == severities)
}

/// The settings list what the menu bar can show in this order and by these names. The raw
/// values are what the preference is stored as, so changing one would quietly reset what
/// everybody chose.
@Test func theMenuBarChoicesKeepTheirNamesAndTheirStoredValues() {
    #expect(MenuBarShows.allCases == [.nameAndUsage, .usage, .icon])
    #expect(
        MenuBarShows.allCases.map(\.title) == ["Account and usage", "Usage only", "Icon only"])
    #expect(MenuBarShows.allCases.map(\.rawValue) == ["nameAndUsage", "usage", "icon"])
    #expect(MenuBarShows.allCases.allSatisfy { $0.id == $0.rawValue })
}

// MARK: - Wording

/// The column beside a bar says when a limit resets as `pitboard status` does, minutes in two
/// digits, and that it is resetting once that moment has come. A limit with no reset known
/// has nothing there. Every span it can say is the core's to test.
@Test func aResetIsSaidAsTheCommandLineSaysIt() {
    let resets = { (at: Date?) in
        resetText(window("session", 42, resets: at.map(epoch)), at: noon)
    }
    #expect(resets(noon.addingTimeInterval(3600 + 5 * 60)) == "resets in 1h 05m")
    #expect(resets(noon.addingTimeInterval(2 * 86_400 + 4 * 3600)) == "resets in 2d 4h")
    #expect(resets(noon) == "resetting now")
    #expect(resets(noon.addingTimeInterval(-60)) == "resetting now")
    #expect(resets(nil) == "")
}

/// A moment later today is a clock time, and one on another day names its day as well,
/// whether it is days away or a minute past midnight: which day it falls on decides, not how
/// far off it is.
@Test func aClockTimeNamesItsDayOnlyWhenItIsNotToday() throws {
    var utc = Calendar(identifier: .gregorian)
    utc.timeZone = try #require(TimeZone(identifier: "UTC"))
    let evening = midday.addingTimeInterval(6 * 3600)
    let small = midday.addingTimeInterval(-11 * 3600)
    let today = clockTime(evening, from: midday, calendar: utc)
    #expect(today == shortTime(evening))
    #expect(clockTime(small, from: midday, calendar: utc) == shortTime(small))

    let tomorrow = evening.addingTimeInterval(86_400)
    let another = clockTime(tomorrow, from: midday, calendar: utc)
    #expect(another.count > today.count)
    #expect(another.hasSuffix(" \(today)"))
    #expect(another.hasPrefix(tomorrow.formatted(.dateTime.weekday(.abbreviated))))

    let pastMidnight = midday.addingTimeInterval(12 * 3600 + 60)
    let justAfter = clockTime(pastMidnight, from: midday, calendar: utc)
    #expect(justAfter.count > shortTime(pastMidnight).count)
    #expect(justAfter.hasSuffix(" \(shortTime(pastMidnight))"))
}

/// The activity list names a change by the verb the log keeps, in words, and one it does not
/// know yet by the verb itself made readable rather than not at all.
@Test func aChangeIsNamedByItsVerbAndAnUnknownOneReadably() {
    let verbs = [
        "switch": "Switch", "enroll": "Enrol", "forget": "Forget", "rename": "Rename",
        "renew": "Renew", "abandon": "Give up on a switch", "repair": "Repair",
        "adopt": "Adopt", "uninstall": "Uninstall",
    ]
    for (verb, name) in verbs {
        #expect(changeVerb(verb) == name)
    }
    #expect(changeVerb("sign_in") == "Sign in")
    #expect(changeVerb("") == "")
}

/// A change that worked says so in a word, and one that did not says what stopped it, from
/// the code the log keeps.
@Test func aChangeSaysHowItEnded() {
    #expect(changeOutcome("ok") == "Done")
    #expect(changeOutcome("parked_login_expired") == "Parked login expired")
    #expect(changeOutcome("refused") == "Refused")
}

/// Who asked for a change: this app, a terminal, or a line written before that was recorded,
/// and a caller the app does not know yet by its own name.
@Test func aChangeSaysWhoAskedForIt() {
    #expect(changeCaller("app") == "Pitboard app")
    #expect(changeCaller("cli") == "Command line")
    #expect(changeCaller("unknown") == "Unknown")
    #expect(changeCaller("schedule") == "Schedule")
}

/// The log keeps local time with its offset. The same moment written in three time zones is
/// one moment, and text that is not a time is nil, so the list shows the log's own text.
@Test func aChangesTimeIsReadWithItsOffset() {
    let moment = Date(timeIntervalSince1970: 1_790_492_709)
    #expect(changeDate("2026-09-27T14:05:09+07:00") == moment)
    #expect(changeDate("2026-09-27T07:05:09+00:00") == moment)
    #expect(changeDate("2026-09-27T02:05:09-05:00") == moment)
    #expect(changeDate("yesterday") == nil)
    #expect(changeDate("") == nil)
}

/// A phrase that starts a line starts with a capital, and nothing else about it changes: a
/// figure first stays as it is, and so does a phrase already capitalised.
@Test func onlyTheFirstLetterOfAPhraseIsCapitalised() {
    #expect("".capitalizedFirst == "")
    #expect("w".capitalizedFirst == "W")
    #expect("weekly used up".capitalizedFirst == "Weekly used up")
    #expect("Weekly 64%, weekly Fable 98%".capitalizedFirst == "Weekly 64%, weekly Fable 98%")
    #expect("5-hour 12%".capitalizedFirst == "5-hour 12%")
    #expect("éclair".capitalizedFirst == "Éclair")
}
