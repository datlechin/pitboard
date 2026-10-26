import PitboardKit
import SwiftUI

struct AccountsWindow: View {
    let model: AppModel
    let windows: AccountWindows

    var body: some View {
        AccountsPane(model: model, windows: windows)
            .frame(minWidth: 560, minHeight: 400)
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
    }
}

// Told apart from another, so that one replacing it is put up afresh.
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
