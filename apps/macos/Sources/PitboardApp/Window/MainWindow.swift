import PitboardKit
import SwiftUI

/// Pitboard's one window, beside the menu rather than instead of it.
///
/// The menu is the glance and the switch. This is where the things that need room go: each
/// account with its limits drawn out and everything that can be done to it, what Pitboard
/// has to say in full, everything it has changed, and what it finds about this Mac.
struct MainWindow: View {
    /// The scene's id, named once so the menu bar item and the scene cannot drift apart.
    static let id = "main"

    let model: AppModel
    let windows: AccountWindows
    let requests: WindowRequests
    @SceneStorage("pane") private var pane = WindowPane.accounts

    var body: some View {
        NavigationSplitView {
            List(WindowPane.allCases, selection: selection) { pane in
                Label(pane.title, systemImage: pane.symbol)
                    .tag(pane)
                    .accessibilityIdentifier("sidebar.\(pane.rawValue)")
            }
            .navigationSplitViewColumnWidth(min: 150, ideal: 170, max: 220)
        } detail: {
            switch pane {
            case .accounts: AccountsPane(model: model, windows: windows)
            case .activity: ActivityPane(model: model)
            case .machine: MachinePane(model: model)
            }
        }
        .frame(minWidth: 640, minHeight: 440)
        .appWindow(windows.presence)
        // The model keeps the sheet; this one goes however it goes, and the model is told.
        .sheet(
            item: Binding(
                get: { model.sheet },
                set: { if $0 == nil { model.send(.closeSheet) } })
        ) { sheet in
            AccountSheetView(model: model, sheet: sheet)
        }
        .alert(
            model.failureAlert?.title ?? "",
            isPresented: Binding(
                get: { model.failureAlert != nil },
                set: { if !$0 { model.send(.dismissFailure) } }),
            presenting: model.failureAlert
        ) { _ in
            Button("OK") {}
        } message: { alert in
            Text(alert.message)
        }
        // Closing the question, however it closes, keeps the app open; the button that
        // answers it names the switch it answers, so it switches whichever arrives first.
        .alert(
            quitAsked?.question.title ?? "",
            isPresented: Binding(
                get: { quitAsked != nil },
                set: { if !$0 { model.send(.keepAppOpen) } }),
            presenting: quitAsked
        ) { asked in
            Button(asked.question.confirm) {
                model.send(.quitAndSwitch(qualified: asked.qualified))
            }
            Button("Cancel", role: .cancel) {}
        } message: { asked in
            Text(asked.question.message)
        }
        // A request for the window can want a pane: a sheet is about accounts, and so is a
        // notice. Asked for when the window opens as well, since a window opened by the
        // request is not there to see it change; once for each request.
        .onChange(of: model.windowRequest.serial, initial: true) {
            pane = requests.pane(for: model.windowRequest, from: pane)
        }
    }

    /// The sidebar's selection. A list selects nothing when its selection is cleared, and a
    /// window with no pane shows nothing, so clearing it keeps the pane shown.
    private var selection: Binding<WindowPane?> {
        Binding(get: { pane }, set: { if let chosen = $0 { pane = chosen } })
    }

    /// The question about quitting an app, with the switch its answer names.
    private var quitAsked: QuitAsked? {
        guard let question = model.quitConfirmation, let asked = model.quitQuestion else {
            return nil
        }
        return QuitAsked(question: question, qualified: asked.qualified)
    }
}

/// What the quit question asks, and the account to switch to once it is answered.
private struct QuitAsked: Equatable {
    let question: Question
    let qualified: String
}

/// A sheet the model keeps over the window, told apart from another so that one replacing
/// it is put up afresh.
extension Sheet: Identifiable {
    public var id: String {
        switch self {
        case .add(let provider): "add/\(provider ?? "")"
        case .signInAgain(let provider, let label): "again/\(provider)/\(label)"
        case .name(let provider, _): "name/\(provider)"
        case .rename(let provider, let label): "rename/\(provider)/\(label)"
        case .stow: "stow"
        }
    }
}
