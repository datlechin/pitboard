import AppIntents
import Foundation
import PitboardGlance
import PitboardKit

// What the app target's intents do, as plain functions a test calls over a stand-in model: an
// intent's own dependencies resolve only inside the system's perform.
public enum Actions {
    // Shortcuts gives an action 30 seconds, and a switch that quits ChatGPT first can take as
    // long.
    public static let patience: Duration = .seconds(25)

    // What the model answered, asking its question where `asking` can, which macOS 14 can't. An
    // entity's properties are boxes of their own, so `account` reads In Use once it is.
    public static func switching(
        to account: AccountEntity, with model: AppModel, asking: Confirming?,
        within patience: Duration = patience
    ) async throws -> String {
        let late = Refused(said: stillSwitching(label: account.name))
        guard let answer = await model.ask(.switch(account: account.id), within: patience)
        else { throw late }
        if answer.done {
            account.inUse = true
            return answer.said
        }
        guard let question = answer.question, let asking else {
            throw Refused(said: answer.said)
        }
        try await asking(question)
        let quit = OutsideAsk.quitAndSwitch(account: account.id)
        guard let quitting = await model.ask(quit, within: patience) else { throw late }
        guard quitting.done else { throw Refused(said: quitting.said) }
        account.inUse = true
        return quitting.said
    }

    // The account asked about, or else the accounts in use, with what each has used.
    public static func usage(
        of account: AccountEntity?, with model: AppModel, within patience: Duration = patience
    ) async throws -> ([AccountEntity], String) {
        guard let text = await model.widgetFeed(within: patience),
            let glance = Glance(Data(text.utf8))
        else { throw Refused(said: macWords.notReadYet) }
        let accounts = account.map { [$0] } ?? AccountQuery.entities(in: glance).filter(\.inUse)
        guard !accounts.isEmpty else { return ([], macWords.noneInUse) }
        let said = accounts.map { usageOf(name: $0.name, said: $0.usage) }
        return (accounts, said.joined(separator: "\n"))
    }

    @MainActor
    public static func opening(_ model: AppModel) {
        model.send(.show(place: .accounts))
    }

    // The quit question as Shortcuts asks it, the model's verb on its button.
    @available(macOS 15.0, *)
    public static func confirming(in intent: some AppIntent) -> Confirming {
        { question in
            try await intent.requestConfirmation(
                actionName: .custom(
                    acceptLabel: "\(question.confirm)", acceptAlternatives: [],
                    denyLabel: "\(words.window.cancel)", denyAlternatives: []),
                dialog: IntentDialog(
                    full: "\(question.title) \(question.message)",
                    supporting: "\(question.title)"))
        }
    }
}

// Asks the quit question the model answered with, and throws where the person said no.
public typealias Confirming = @Sendable (Question) async throws -> Void
