import Foundation
import PitboardKit
import PitboardLinks

/// What the account picker shows, decided from what it was asked and what the last read
/// found, so every state and its words can be tested without drawing it.
enum LinkPick: Equatable {
    /// Nothing is asked: the window closes itself.
    case idle
    /// The accounts are not read yet: a launch by a link arrives before the first read.
    case reading(link: URL?)
    /// The first read failed, and there is nothing to choose from.
    case readFailed(problem: String)
    /// A claude.ai link, and the accounts that can open it.
    case choose(link: URL, accounts: [ClaudeAccount])
    /// A link to be pasted, and the accounts that can open it.
    case enter(accounts: [ClaudeAccount])
    /// No account has a claude.ai window.
    case noAccount(link: URL?)
    /// No account has a claude.ai window, and the Claude Code login on this Mac has no name
    /// in pitboard, which is what would give it one.
    case unnamed(email: String, link: URL?)
    /// The link will not be opened, and why.
    case refused(LinkRefusal)

    var title: String {
        switch self {
        case .idle, .reading, .choose, .enter: "Open claude.ai Link"
        case .readFailed: "Couldn’t Read Accounts"
        case .noAccount, .unnamed: "No Account Has a claude.ai Window"
        case .refused: "Can’t Open This Link"
        }
    }

    var message: String {
        switch self {
        case .idle, .reading, .choose:
            "Choose the account whose claude.ai window opens this link."
        case .enter:
            "Paste a claude.ai link, such as an artifact’s, and choose the account whose "
                + "claude.ai window opens it."
        case .readFailed(let problem):
            problem
        case .noAccount:
            "pitboard opens claude.ai links in the window of a Claude Code account you have "
                + "added. Add one, then open the link again."
        case .unnamed(let email, _):
            "The Claude Code login on this Mac, \(email), has no name in pitboard. Name it to "
                + "give it a claude.ai window."
        case .refused(let refusal):
            refusal.message
        }
    }

    /// The link, when there is one to show or hand to the browser.
    var link: URL? {
        switch self {
        case .reading(let link), .noAccount(let link), .unnamed(_, let link): link
        case .choose(let link, _): link
        case .idle, .readFailed, .enter, .refused: nil
        }
    }

    /// The accounts to choose from.
    var accounts: [ClaudeAccount] {
        switch self {
        case .choose(_, let accounts), .enter(let accounts): accounts
        default: []
        }
    }
}

/// The picker's state. Every request shows the picker, even with one account, so nothing
/// from outside opens an account's window without the person choosing it.
func linkPick(pending: LinkModel.Request?, status: Status?, problem: String?) -> LinkPick {
    guard let pending else { return .idle }
    let link: URL?
    switch pending {
    case .link(.failure(let refusal)): return .refused(refusal)
    case .link(.success(let url)): link = url
    case .enter: link = nil
    }
    guard let status else {
        return problem.map { .readFailed(problem: $0) } ?? .reading(link: link)
    }
    let accounts = claudeWindows(in: status)
    guard !accounts.isEmpty else {
        let unnamed = status.accounts.first {
            $0.provider == defaultProvider && $0.signedIn && $0.label == nil && !$0.unplaced
        }
        return unnamed.map { .unnamed(email: $0.email, link: link) } ?? .noAccount(link: link)
    }
    return link.map { .choose(link: $0, accounts: accounts) } ?? .enter(accounts: accounts)
}

/// The account the picker offers first: the one chosen last in this run, else the Claude
/// Code account in use, else the first.
func defaultChoice(in accounts: [ClaudeAccount], lastChosen: UUID?) -> UUID? {
    if let lastChosen, accounts.contains(where: { $0.store == lastChosen }) {
        return lastChosen
    }
    return (accounts.first(where: \.inUse) ?? accounts.first)?.store
}

/// What an account's row adds when its window is open: opening the link there navigates that
/// window, as a link does in a browser tab, so it is said before the choice is made.
let windowOpenNote = "Window open. The link replaces what it shows; Back returns."
