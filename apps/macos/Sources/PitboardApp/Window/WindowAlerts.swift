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

    func body(content: Content) -> some View {
        content
            .alert(
                model.failureAlert?.title ?? "",
                isPresented: Binding(
                    get: { shown && model.failureAlert != nil },
                    set: { if !$0 { model.send(.dismissFailure) } }),
                presenting: model.failureAlert
            ) { _ in
                Button("OK") {}
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
            Button(asked.question.confirm) { model.send(asked.confirmed) }
            Button("Cancel", role: .cancel) {}
        } message: { asked in
            Text(asked.question.message)
        }
    }
}
