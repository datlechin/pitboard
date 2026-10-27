import AppKit
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
                // The first button takes Return (docs/facts/macos-app.md): the step, where the
                // alert has one to take.
                switch alert.next {
                case .act(let choice)?:
                    Button(choice.title) { model.send(choice.intent) }
                    Button(words.window.cancel, role: .cancel) {}
                case .learn(let link)?:
                    Button(words.window.ok) {}
                    Button(link.title) {
                        if let page = URL(string: link.link) { openURL(page) }
                    }
                case nil:
                    Button(words.window.ok) {}
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

extension Binding<Asking?> {
    @MainActor
    func ask(_ intent: Intent, first question: Question?, of model: AppModel) {
        guard let question else { return model.send(intent) }
        if question.confirmIsDefault {
            wrappedValue = Asking(question: question, intent: intent)
        } else {
            askWithNoDefault(question) { model.send(intent) }
        }
    }
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

private struct Answers: View {
    let question: Question
    let answer: () -> Void

    var body: some View {
        Button(
            question.confirm, role: question.destructive ? .destructive : nil, action: answer)
        Button(words.window.cancel, role: .cancel) {}
    }
}

// SwiftUI's alert gives Return to the answer, or to Cancel at the cost of Escape. AppKit's gives
// a first button called Cancel Escape and no button Return (docs/facts/macos-app.md).
@MainActor
private func askWithNoDefault(_ question: Question, answer: @escaping @MainActor () -> Void) {
    let alert = NSAlert()
    alert.messageText = question.title
    alert.informativeText = question.message
    alert.addButton(withTitle: words.window.cancel)
    alert.addButton(withTitle: question.confirm).hasDestructiveAction = question.destructive
    let answered = { (response: NSApplication.ModalResponse) in
        if response == .alertSecondButtonReturn { answer() }
    }
    if let window = NSApp.keyWindow {
        alert.beginSheetModal(for: window, completionHandler: answered)
    } else {
        answered(alert.runModal())
    }
}
