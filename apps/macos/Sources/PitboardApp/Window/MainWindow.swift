import PitboardKit
import SwiftUI

struct MainWindow: View {
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
            case .accounts:
                AccountsPane(model: model, windows: windows)
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
        .windowAlerts(model, shown: model.sheet == nil)
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
}

/// A sheet the model keeps over the window, told apart from another so that one replacing
/// it is put up afresh.
extension Sheet: Identifiable {
    public var id: String {
        switch self {
        case .add(let provider): "add/\(provider ?? "")"
        case .signInAgain(let provider, let label): "again/\(provider)/\(label)"
        case .name(let provider, let label, _): "name/\(provider)/\(label)"
        case .rename(let provider, let label): "rename/\(provider)/\(label)"
        case .stow: "stow"
        }
    }
}
