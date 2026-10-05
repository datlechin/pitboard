import Foundation
import PitboardKit

/// What the menu bar item shows beside its mark.
///
/// macOS hides menu bar items to make room, the widest first, and a notched display has
/// little room to begin with. The name is what gets dropped first, and the mark alone is
/// for somebody who only ever opens the menu.
enum MenuBarShows: String, CaseIterable, Identifiable {
    case nameAndUsage
    case usage
    case icon

    var id: String { rawValue }

    var title: String {
        switch self {
        case .nameAndUsage: "Account and usage"
        case .usage: "Usage only"
        case .icon: "Icon only"
        }
    }
}

/// The limit worth putting in the menu bar: the account's own, not one scoped to a single
/// model, and one the account is working against when the server says which. A scoped row
/// at 98% would otherwise read as though everything had stopped.
func headline(of windows: [Limit]) -> Limit? {
    let ownLimits = windows.filter { $0.scope == nil }
    let candidates = ownLimits.isEmpty ? windows : ownLimits
    let active = candidates.filter(\.isActive)
    return (active.isEmpty ? candidates : active).max { $0.percent < $1.percent }
}

/// What the menu bar says: the account in use and the limit closest to its end. Nothing is
/// known until the first read, and an account signed in but not enrolled has no name here.
///
/// With more than one tool there is an account in use in each, and one bar. It follows the
/// enrolled one whose headline limit is most used, so what it shows is the account closest
/// to running out; a tie goes to the tool `order` lists first.
func menuTitle(
    for status: Status?, order tools: [Tool] = [], showing shows: MenuBarShows = .nameAndUsage
) -> String {
    guard shows != .icon, let account = titled(status?.accounts ?? [], order: tools) else {
        return ""
    }
    let percent = headline(of: account.usage?.windows ?? []).map {
        "\(Int($0.percent.rounded()))%"
    }
    if shows == .usage { return percent ?? "" }
    // Long labels are bounded, because this sits in a bar someone else also wants space in.
    let full = account.label ?? "unnamed"
    let name = full.count > 12 ? full.prefix(11) + "…" : full[...]
    return percent.map { "\(name) \($0)" } ?? String(name)
}

/// The account the menu bar is about.
func titled(_ accounts: [Account], order tools: [Tool]) -> Account? {
    let providers = inOrder(accounts.map(\.provider), by: tools)
    guard providers.count > 1 else { return accounts.first(where: \.signedIn) }
    let used = { (account: Account) in
        headline(of: account.usage?.windows ?? [])?.percent ?? -1
    }
    var closest: Account?
    for provider in providers {
        let inUse = accounts.first { $0.provider == provider && $0.signedIn && $0.label != nil }
        if let inUse, closest.map({ used(inUse) > used($0) }) ?? true {
            closest = inUse
        }
    }
    // Nobody enrolled is signed in anywhere: say what is signed in, as one tool would.
    return closest ?? accounts.first(where: \.signedIn)
}

/// One tool's accounts, under its name.
struct AccountGroup: Identifiable {
    /// The tool's code.
    let id: String
    /// Nil when every account shown is one tool's: a heading there says what nobody asked.
    let name: String?
    let accounts: [Account]
}

/// The accounts a section per tool, in the order `tools` lists them. One section, unnamed
/// and in the order given, when they are all one tool's, so a machine with one tool looks
/// exactly as it always did.
func grouped(_ accounts: [Account], by tools: [Tool]) -> [AccountGroup] {
    let providers = inOrder(accounts.map(\.provider), by: tools)
    guard providers.count > 1 else {
        return providers.map { AccountGroup(id: $0, name: nil, accounts: accounts) }
    }
    return providers.map { code in
        AccountGroup(
            id: code, name: tools.first { $0.code == code }?.name ?? code,
            accounts: accounts.filter { $0.provider == code })
    }
}

/// Tool codes without repeats, in the order `tools` lists them, and any it does not list
/// after those in the order they came. The core already lists rows this way; this keeps a
/// view from depending on it.
func inOrder(_ codes: [String], by tools: [Tool]) -> [String] {
    var seen: [String] = []
    for code in tools.map(\.code) + codes where codes.contains(code) && !seen.contains(code) {
        seen.append(code)
    }
    return seen
}

/// What pressing an account does, decided once so the menu and the window cannot disagree.
enum AccountAction: Equatable {
    /// Switch to it, by its label with its tool.
    case use(String)
    /// Its parked login can no longer be used: sign in to it again.
    case signInAgain(provider: String, label: String)
    /// It is signed in and has no name yet: name it.
    case name(provider: String, email: String)
    /// Nothing: it is the one in use, a switch is running, or Pitboard cannot use it.
    case none
}

/// One account as the menu and the window describe it.
///
/// Built from the account alone and the moment it is described at, so what a row says can
/// be tested without drawing it, and the menu and the window say the same thing about the
/// same account.
struct AccountDescription: Equatable {
    /// Its name, or its email address while it has none.
    let title: String
    let email: String
    let inUse: Bool
    /// Its parked login can no longer be used, whatever else is running.
    let needsSignIn: Bool
    /// A switch to it is running.
    let switching: Bool
    let action: AccountAction
    /// The one line under its name in the menu: its limits, or what stands in their way.
    let summary: String
    /// Why it cannot be used, in full, when that is the case.
    let problem: String?
    /// Why its numbers are not new, when they are not and it can be used all the same: its
    /// service could not be reached or is rate limiting, or its session has expired.
    let staleNote: String?
    /// How long the parked login stays usable, for an account not in use. For Claude
    /// Desktop, when its sign-in lapses: Pitboard cannot renew one.
    let parkedNote: String?
    /// Where the numbers came from, when that is not the account's service: Claude
    /// Desktop's own history, called unconfirmed only while the reading is not verified.
    let sourceNote: String?
    /// How long the account in use lasts at the rate it is going.
    let pace: String?
    let limits: [Limit]

    /// `switching` is the account a switch is running for, and `busy` whether a sign-in is.
    init(_ account: Account, switching: String?, busy: Bool, now: Date = Date()) {
        email = account.email
        inUse = account.signedIn
        needsSignIn =
            !account.unplaced && account.label != nil && !account.signedIn
            && !account.switchable
        limits = account.usage?.windows ?? []
        self.switching = account.qualified != nil && account.qualified == switching
        if account.unplaced {
            title = "Login Pitboard can’t use"
        } else {
            title = accountName(label: account.label, email: account.email)
        }
        // A switch running holds everything back, since each change waits for the one
        // before. A sign-in running holds back only another sign-in: it waits on a person in
        // a browser, and switching meanwhile is theirs to do.
        if account.unplaced || switching != nil {
            action = .none
        } else if account.label == nil {
            action = account.signedIn ? .name(provider: account.provider, email: email) : .none
        } else if account.signedIn {
            action = .none
        } else if account.switchable, let qualified = account.qualified {
            action = .use(qualified)
        } else if let label = account.label, !busy {
            action = .signInAgain(provider: account.provider, label: label)
        } else {
            action = .none
        }
        problem =
            account.unplaced || !account.switchable && !account.signedIn
            ? account.staleExplanation : nil
        staleNote = problem == nil ? account.staleExplanation : nil
        parkedNote =
            account.signedIn
            ? nil
            : account.provider == desktopProvider
                ? desktopLapseNote(account.parked, now: now)
                : parkedLife(parked: account.parked, now: Int64(now.timeIntervalSince1970))
        sourceNote = account.usage.flatMap { usage in
            guard usage.source == .desktopHistory else { return nil }
            return usage.verified
                ? "From Claude’s history" : "From Claude’s history, unconfirmed"
        }
        let lasts = runway(seconds: account.lastsSeconds, burning: account.lastsBurning)
        pace = lasts?.capitalizedFirst
        summary = Self.summary(
            of: account, switching: self.switching, needsSignIn: needsSignIn, now: now)
    }

    /// Under the name in the menu, one line: what the account's limits stand at, and when
    /// one that has run out comes back. What stands in the way instead, when something
    /// does.
    private static func summary(
        of account: Account, switching: Bool, needsSignIn: Bool, now: Date
    ) -> String {
        if switching { return "Switching…" }
        if account.unplaced { return account.staleExplanation ?? "Can’t be read or switched" }
        if account.label == nil { return "Not named yet" }
        if needsSignIn { return "Needs signing in again" }
        let windows = account.usage?.windows ?? []
        guard !windows.isEmpty else { return account.email }
        return windows.map { window in
            let name = limitName(limit: window)
            let scoped = window.scope.map { "\(name) \($0)" } ?? name
            guard window.percent >= 100, let at = window.resetsAt else {
                return "\(scoped) \(Int(window.percent.rounded()))%"
            }
            let back = Date(timeIntervalSince1970: TimeInterval(at))
            guard back > now else { return "\(scoped) used up" }
            return "\(scoped) used up until \(clockTime(back, from: now))"
        }.joined(separator: ", ").capitalizedFirst
    }
}

/// A moment later today as a clock time, and a later day by its weekday as well, so a reset
/// named in a menu that stays open, or is read again an hour later, is still true.
func clockTime(_ date: Date, from now: Date = Date(), calendar: Calendar = .current) -> String {
    let time = date.formatted(date: .omitted, time: .shortened)
    guard !calendar.isDate(date, inSameDayAs: now) else { return time }
    return "\(date.formatted(.dateTime.weekday(.abbreviated))) \(time)"
}

extension String {
    /// "5-hour" stays "5-hour" and "weekly" becomes "Weekly": a phrase that starts a line
    /// starts with a capital.
    var capitalizedFirst: String {
        guard let first else { return self }
        return first.uppercased() + dropFirst()
    }
}
