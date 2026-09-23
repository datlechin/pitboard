import Foundation
import PitboardKit

/// Phrases the window and the settings both say, out of a view so a test can read them.
/// A sentence about time that is quietly wrong is worse than no sentence, and inside a
/// `body` there is nothing to assert against.

/// The answer to the question the whole tool exists for: how long the account you are on
/// is good for. `burning` is a limit filling rather than a limit resetting, which is the
/// difference between "about an hour left" and "whole again in an hour".
func lasting(_ seconds: Int64, burning: Bool) -> String {
    // Under a minute the units formatter says "0 min", which reads as though nothing were
    // left when the difference is seconds either way.
    guard seconds >= 60 else { return burning ? "about to run out" : "resets any moment" }
    let span = Duration.seconds(seconds)
        .formatted(.units(allowed: [.days, .hours, .minutes], width: .narrow))
    return burning ? "about \(span) left at this rate" : "resets in \(span)"
}

/// What a renewal run did. Everything here is a login that was going to expire, so "nothing
/// happened" is the good answer and has to read like one.
func renewalNote(_ renewals: [Renewed]) -> String {
    let renewed = renewals.filter { $0.outcome == "renewed" }.count
    switch (renewals.count, renewed) {
    case (0, _): return "Nothing was due."
    case (_, 0): return "\(renewals.count) due; none could be renewed this time."
    case (let all, let done) where all == done:
        return done == 1 ? "Renewed one." : "Renewed all \(done)."
    case (let all, let done): return "Renewed \(done) of \(all)."
    }
}

/// How a person names a window in a sentence: "5-hour", "weekly", "daily", "3-hour".
///
/// From its length, which is the one thing both services agree on: Anthropic names its
/// windows and OpenAI times them, and "session" means nothing to somebody reading about a
/// Codex account. A window whose length is not known is named from its kind.
func windowName(_ window: Limits) -> String {
    guard let seconds = window.lengthSeconds, seconds > 0 else {
        switch window.kind {
        case "session", "five_hour": return "5-hour"
        case "seven_day": return "weekly"
        case let kind where kind.hasPrefix("weekly"): return "weekly"
        default: return window.kind.replacingOccurrences(of: "_", with: " ")
        }
    }
    switch seconds {
    case 7 * 86_400: return "weekly"
    case 86_400: return "daily"
    case let days where days % 86_400 == 0: return "\(days / 86_400)-day"
    case let hours where hours % 3600 == 0: return "\(hours / 3600)-hour"
    case let minutes where minutes % 60 == 0: return "\(minutes / 60)-minute"
    default: return "\(seconds)-second"
    }
}

/// The same name, short enough for the column beside a bar: "5h", "week", "day", "3h".
func windowShortName(_ window: Limits) -> String {
    let base: String
    if let seconds = window.lengthSeconds, seconds > 0 {
        switch seconds {
        case 7 * 86_400: base = "week"
        case 86_400: base = "day"
        case let days where days % 86_400 == 0: base = "\(days / 86_400)d"
        case let hours where hours % 3600 == 0: base = "\(hours / 3600)h"
        case let minutes where minutes % 60 == 0: base = "\(minutes / 60)m"
        default: base = "\(seconds)s"
        }
    } else {
        switch window.kind {
        case "session", "five_hour": base = "5h"
        case "weekly_all", "seven_day", "weekly_scoped": base = "week"
        default: base = window.kind
        }
    }
    return window.scope.map { "\(base) · \($0)" } ?? base
}

/// What a switch means for sessions of a tool that never picks one up by itself. `from`
/// is empty when nothing was signed in before, and then there is no old account to name.
func restartNotice(program: String, from: String) -> String {
    let old = from.isEmpty ? "the account they started with" : from
    return
        "Running \(program) sessions keep using \(old) until they are quit and started again."
}

/// A name typed for a new account, with the tool it is for, as the core takes it. The
/// picker is what says which tool, so a slash typed into the name is the core's to refuse
/// rather than read as a second choice of tool.
func qualified(_ name: String, for provider: String) -> String {
    "\(provider)/\(name)"
}

/// A label as somebody types it at the command line: bare for Claude Code, which is what a
/// bare label has always meant, and with its tool for any other.
func typed(_ account: Account) -> String? {
    account.provider == "claude" ? account.label : account.qualified
}
