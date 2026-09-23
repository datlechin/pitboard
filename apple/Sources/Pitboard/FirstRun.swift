import PitboardKit
import SwiftUI

/// The one next thing to do, for a machine that is not set up yet.
///
/// pitboard installs as a cask that puts both the app and the command line on the machine,
/// and somebody who opens the app has usually never run a command and may never want to.
/// Before this, an empty panel said "Run `pitboard enroll <label>`" and a machine without
/// Claude Code said a one-line error, which between them sent every new person to a
/// terminal to find out whether the thing they just installed works.
///
/// One state at a time, and never more than one thing to press. Each is the actual next
/// step, not a tour.
struct FirstRun: View {
    let model: AppModel
    /// The `.onlyOne` nudge is true but not urgent: somebody may keep one account on
    /// purpose and watch its limits. The two blocking states are not dismissible, because
    /// dismissing them would leave an app that does nothing and says nothing.
    @AppStorage("hideSecondAccountNudge") private var hidden = false

    var body: some View {
        switch model.footing {
        case .noClaudeCode:
            Card(
                symbol: "questionmark.folder",
                title: "Claude Code is not on this machine",
                detail:
                    "pitboard parks and restores Claude Code's logins, so there is nothing "
                    + "for it to do until Claude Code is installed and signed in once."
            ) {
                Link(
                    "How to install it",
                    destination: URL(
                        string: "https://docs.claude.com/en/docs/claude-code/setup")!
                )
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
            }
        case .noOneSignedIn:
            Card(
                symbol: "person.crop.circle.badge.plus",
                title: "Nobody is signed in to \(addable)",
                detail:
                    "Sign in once here and pitboard can park that login, so signing in to a "
                    + "second account does not cost you the first."
            ) {
                Button("Sign in…") { model.naming = .another(nil) }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
            }
        case .unnamed(let provider, let email):
            Card(
                symbol: "tag",
                title: "Give this account a name",
                detail:
                    "\(email) is signed in\(to(provider)). pitboard parks logins under a name "
                    + "you choose, and cannot park this one until it has one."
            ) {
                Button("Name it…") { model.naming = .theOneInUse(provider) }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
            }
        case .onlyOne(let provider, let label):
            if !hidden {
                Card(
                    symbol: "arrow.left.arrow.right",
                    title: "Add a second \(tool(provider))account",
                    detail:
                        "\(label) is the only \(tool(provider))account pitboard knows, so there "
                        + "is nothing to switch to. Adding another signs in to it and parks "
                        + "this one."
                ) {
                    Button("Add another account…") { model.naming = .another(provider) }
                        .buttonStyle(.borderedProminent)
                        .keyboardShortcut(.defaultAction)
                    Button("Not now") { hidden = true }
                }
            }
        case .ready:
            EmptyView()
        }
    }

    /// "Claude Code", or "Claude Code or Codex" where both could be signed in to here.
    private var addable: String { model.addable.map(\.name).joined(separator: " or ") }

    /// " to Codex", once accounts of more than one tool are shown, and nothing before.
    private func to(_ provider: String) -> String {
        model.showsTools ? " to \(model.tool(provider)?.name ?? provider)" : ""
    }

    /// "Codex ", once accounts of more than one tool are shown, and nothing before.
    private func tool(_ provider: String) -> String {
        model.showsTools ? "\(model.tool(provider)?.name ?? provider) " : ""
    }
}

/// One state, said once: a mark, a line, the reason, and what to press.
private struct Card<Actions: View>: View {
    let symbol: String
    let title: String
    let detail: String
    @ViewBuilder let actions: Actions

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: symbol)
                .font(.title2)
                .foregroundStyle(.tint)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 6) {
                Text(title).font(.headline)
                Text(detail)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                HStack(spacing: 8) { actions }
            }
            Spacer(minLength: 0)
        }
        .padding(10)
        .background(.quaternary.opacity(0.4), in: .rect(cornerRadius: 8))
    }
}
