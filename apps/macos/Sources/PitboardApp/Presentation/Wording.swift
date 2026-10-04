import Foundation
import PitboardKit

/// Phrases the window and the settings both say, out of a view so a test can read them.
/// A sentence about time that is quietly wrong is worse than no sentence, and inside a
/// `body` there is nothing to assert against.
///
/// Where the core's `words` has a sentence, the app calls it through the bindings' free
/// functions instead of keeping a copy, so the command line and the app say it alike. Those
/// read no clock, file or keychain, so a view calls them where it draws. This file holds
/// what the core does not say for the app, and the glue that hands those functions what a
/// view has.

/// When a limit resets, as the column beside its bar says it and `pitboard status` says it
/// too: "resets in 2h 05m", "resetting now", and nothing where no reset is known.
func resetText(_ window: Limit, at now: Date) -> String {
    window.resetsAt.map { resets(resetsAt: $0, now: Int64(now.timeIntervalSince1970)) } ?? ""
}

/// Who is signed in, at the start of a sentence: the email, or words for an account with
/// none, as a Claude Desktop account has, so the sentence never starts with nothing.
func whoIsSignedIn(_ email: String) -> String {
    email.isEmpty ? "An account" : email
}

/// What an account is called where nothing else names it: its label, its email, or words
/// for an account with neither.
func accountName(label: String?, email: String) -> String {
    if let label { return label }
    return email.isEmpty ? "Unnamed account" : email
}

/// How the `pitboard` a terminal runs is kept up to date: with the app when it is the one
/// inside it, and otherwise the way it was installed. No other way of installing it updates
/// it by itself, and saying it "updates on its own" read as though one did.
func updateNote(bundled: Bool) -> String {
    bundled
        ? "The one inside this app, so it updates with the app."
        : "Installed apart from this app, so update it the way you installed it."
}

/// A limit as VoiceOver says it: "5-hour limit, 42 percent used, resets in 3 hours". The
/// column beside the bar says "5h" and "resets in 30m", which is read letter by letter or as
/// a unit: "m" is read as "meters". Once the reset is due it says so, as the column does.
func spokenLimit(_ window: Limit, resettingIn seconds: TimeInterval?) -> String {
    let limit = limitName(limit: window)
    let name = window.scope.map { "\(limit) \($0)" } ?? limit
    let used = "\(name) limit, \(Int(window.percent.rounded())) percent used"
    guard let seconds else { return used }
    guard seconds > 0 else { return "\(used), resetting now" }
    let span = Duration.seconds(max(60, Int64(seconds))).formatted(
        .units(allowed: [.days, .hours, .minutes], width: .wide, maximumUnitCount: 2)
            .locale(english))
    return "\(used), resets in \(span)"
}

/// What a switch means for sessions of a tool that never picks one up by itself. Said of
/// any session and not of running ones, since the core counts those itself when it can,
/// and a notice left in the panel should not claim sessions that may not exist. `from` is
/// empty when nothing was signed in before, and then there is no old account to name.
func restartNotice(program: String, from: String) -> String {
    let old = from.isEmpty ? "the account it started with" : from
    return
        "Any \(program) session started before this switch keeps using \(old) until it is "
        + "quit and started again."
}

/// The locale VoiceOver's spans of time are written in. Pitboard says everything else in
/// English, and a sentence that switches language halfway, "resets in 1 Stunde", reads as a
/// mistake.
private let english = Locale(identifier: "en_US_POSIX")

/// The tool a bare label means, as the core reads one.
let defaultProvider = "claude"

/// Claude Desktop, as the core names the tool.
let desktopProvider = "desktop"

/// A name typed for a new account, with the tool it is for, as the core takes it. The
/// picker is what says which tool, so a slash typed into the name is the core's to refuse
/// rather than read as a second choice of tool.
func qualified(_ name: String, for provider: String) -> String {
    "\(provider)/\(name)"
}

/// A label as the core types it, taken apart: `codex/work` is Codex's `work`, and a bare
/// `work` is Claude Code's.
func split(_ typed: String) -> (provider: String, label: String) {
    let parts = typed.split(separator: "/", maxSplits: 1)
    guard parts.count == 2 else { return (defaultProvider, typed) }
    return (String(parts[0]), String(parts[1]))
}

/// A label as somebody types it at the command line: bare for Claude Code, which is what a
/// bare label has always meant, and with its tool for any other.
func typed(_ account: Account) -> String? {
    account.provider == defaultProvider ? account.label : account.qualified
}

/// A change as the activity list names it: what was done, by the verb the log keeps.
func changeVerb(_ verb: String) -> String {
    switch verb {
    case "switch": "Switch"
    case "enroll": "Enrol"
    case "forget": "Forget"
    case "rename": "Rename"
    case "renew": "Renew"
    case "abandon": "Give up on a switch"
    case "repair": "Repair"
    case "adopt": "Adopt"
    case "uninstall": "Uninstall"
    default: verb.replacingOccurrences(of: "_", with: " ").capitalizedFirst
    }
}

/// How a change ended: "Done", or what stopped it, from the code the log keeps.
func changeOutcome(_ outcome: String) -> String {
    outcome == "ok" ? "Done" : outcome.replacingOccurrences(of: "_", with: " ").capitalizedFirst
}

/// Who asked for a change.
func changeCaller(_ caller: String) -> String {
    switch caller {
    case "app": "Pitboard app"
    case "cli": "Command line"
    case "unknown": "Unknown"
    default: caller.capitalizedFirst
    }
}

/// When a change was made, from the local time the log keeps. The log's own text when it
/// does not parse, rather than nothing.
func changeDate(_ at: String) -> Date? {
    try? Date(at, strategy: .iso8601)
}

/// Why an app has to quit before its tool can switch, as the alert that asks says it.
/// Claude Desktop keeps its sign-in in files it rewrites while it runs, so the reason is
/// not the one an app holding a login in memory has.
func quitQuestion(name: String, to label: String) -> String {
    if name == "Claude" {
        return "Claude keeps its sign-in in its own files while it runs. Pitboard quits it, "
            + "switches to \(label), and opens it again."
    }
    return "\(name) keeps using the account it started with until it quits. Pitboard quits "
        + "it, switches, and opens it again."
}

/// When a parked Claude Desktop sign-in lapses. Pitboard cannot renew one, so the date is
/// the date, and a lapsed one has to be signed in to again in Claude.
func desktopLapseNote(_ parked: Parked?, now: Date) -> String? {
    guard let at = parked?.refreshExpiresAt else { return nil }
    let lapses = Date(timeIntervalSince1970: TimeInterval(at))
    guard lapses > now else {
        return "Its sign-in has lapsed. Sign in to it again in Claude to use it."
    }
    var style = Date.FormatStyle(timeZone: .current).month(.abbreviated).day()
    style.locale = english
    return "Sign-in lapses on \(lapses.formatted(style)). Pitboard cannot renew Claude "
        + "Desktop sign-ins."
}

/// Where Claude Desktop's numbers come from, as Settings says it.
func liveUsageLine(_ state: LiveUsageState) -> String {
    guard state.enabled else {
        return "Off. Numbers come from Claude’s own history, without reset times."
    }
    switch state.approval {
    case "granted": return "On. Numbers come from claude.ai."
    case "needs_approval": return "Paused: macOS stopped letting Pitboard read Claude’s key."
    default: return "On. Pitboard asks claude.ai at the next read."
    }
}
