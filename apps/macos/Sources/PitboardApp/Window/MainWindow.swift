import SwiftUI

/// Pitboard's one window, beside the menu rather than instead of it.
///
/// The menu is the glance and the switch. This is where the things that need room go: each
/// account with its limits drawn out and everything that can be done to it, what Pitboard
/// has to say in full, everything it has changed, and what it finds about this Mac.
struct MainWindow: View {
    /// The scene's id, named once so the menu bar item and the scene cannot drift apart.
    static let id = "main"

    @Bindable var model: AppModel
    let windows: AccountWindows
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
            case .activity: ActivityPane(machine: model.machine)
            case .machine: MachinePane(machine: model.machine)
            }
        }
        .frame(minWidth: 640, minHeight: 440)
        .appWindow(windows.presence)
        .sheet(item: $model.sheet) { sheet in
            AccountSheetView(model: model, sheet: sheet)
        }
        .failureAlert($model.presentedFailure)
        .alert(
            "Quit \(model.quitting?.name ?? "") to switch?",
            isPresented: Binding(
                get: { model.quitting != nil }, set: { if !$0 { model.closeQuitQuestion() } }),
            presenting: model.quitting
        ) { quitting in
            Button("Quit \(quitting.name) and Switch") {
                Task { await model.quitAndSwitch(quitting) }
            }
            Button("Cancel", role: .cancel) { model.closeQuitQuestion() }
        } message: { quitting in
            Text(quitQuestion(name: quitting.name, to: split(quitting.qualified).label))
        }
        // A request for the window from the menu or the model can want a pane: a sheet is
        // about accounts, and so is a notice. Asked for when the window opens as well, since
        // a window opened by the request is not there to see it change.
        .onChange(of: model.windowRequests) { showRequestedPane() }
        .onAppear { showRequestedPane() }
    }

    private func showRequestedPane() {
        if let wanted = model.requestedPane { pane = wanted }
    }

    /// The sidebar's selection. A list selects nothing when its selection is cleared, and a
    /// window with no pane shows nothing, so clearing it keeps the pane shown.
    private var selection: Binding<WindowPane?> {
        Binding(get: { pane }, set: { if let chosen = $0 { pane = chosen } })
    }
}

extension View {
    /// An alert for a failure somebody should hear about, gone once it is read.
    func failureAlert(_ failure: Binding<ActionFailure?>) -> some View {
        alert(
            failure.wrappedValue?.title ?? "",
            isPresented: Binding(
                get: { failure.wrappedValue != nil },
                set: { if !$0 { failure.wrappedValue = nil } }),
            presenting: failure.wrappedValue
        ) { _ in
            Button("OK") { failure.wrappedValue = nil }
        } message: { shown in
            Text(([shown.message] + shown.warnings.map(\.message)).joined(separator: "\n\n"))
        }
    }
}
