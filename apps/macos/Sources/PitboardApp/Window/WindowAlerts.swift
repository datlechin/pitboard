import PitboardKit
import SwiftUI

extension View {
    // A window that has a sheet holds back another until it goes, so these show from the sheet.
    func windowAlerts(_ model: AppModel, shown: Bool) -> some View {
        modifier(WindowAlerts(model: model, shown: shown))
    }
}

private struct WindowAlerts: ViewModifier {
    let model: AppModel
    let shown: Bool
    @Environment(\.openURL) private var openURL

    func body(content: Content) -> some View {
        content
            .alert(
                model.failureAlert?.title ?? "",
                isPresented: Binding(
                    get: { shown && model.failureAlert != nil },
                    set: { if !$0 { model.send(.dismissFailure) } }),
                presenting: model.failureAlert
            ) { alert in
                // First, so Return reads the alert away (docs/facts/macos-app.md).
                Button(words.window.ok) {}
                switch alert.next {
                case .act(let choice)?:
                    Button(choice.title) { model.send(choice.intent) }
                case .learn(let link)?:
                    Button(link.title) {
                        if let page = URL(string: link.link) { openURL(page) }
                    }
                case nil:
                    EmptyView()
                }
            } message: { alert in
                Text(alert.message)
            }
            .question(model.quitQuestion, of: model, shown: shown)
            .question(model.replaceQuestion, of: model, shown: shown)
    }
}

extension View {
    // The answer names what it answers, so the close it also sends may arrive before it.
    fileprivate func question(_ asked: WindowQuestion?, of model: AppModel, shown: Bool)
        -> some View
    {
        alert(
            asked?.question.title ?? "",
            isPresented: Binding(
                get: { shown && asked != nil },
                set: { if !$0, let asked { model.send(asked.declined) } }),
            presenting: asked
        ) { asked in
            Answers(question: asked.question) { model.send(asked.confirmed) }
        } message: { asked in
            Text(asked.question.message)
        }
    }
}

// A question the model asks before something that cannot be undone, and what its answer sends.
struct Asking: Equatable {
    let question: Question
    let intent: Intent
}

extension View {
    func asking(_ asking: Binding<Asking?>, of model: AppModel) -> some View {
        alert(
            asking.wrappedValue?.question.title ?? "",
            isPresented: Binding(
                get: { asking.wrappedValue != nil },
                set: { if !$0 { asking.wrappedValue = nil } }),
            presenting: asking.wrappedValue
        ) { asked in
            Answers(question: asked.question) { model.send(asked.intent) }
        } message: { asked in
            Text(asked.question.message)
        }
    }
}

// Return is Cancel's where the answer is not the default (docs/facts/macos-app.md).
private struct Answers: View {
    let question: Question
    let answer: () -> Void

    var body: some View {
        Button(
            question.confirm, role: question.destructive ? .destructive : nil, action: answer)
        if question.confirmIsDefault {
            Button(words.window.cancel, role: .cancel) {}
        } else {
            Button(words.window.cancel, role: .cancel) {}
                .keyboardShortcut(.defaultAction)
        }
    }
}
