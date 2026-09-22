import AppKit
import PitboardKit
import SwiftUI

struct MenuView: View {
    let model: AppModel
    let updater: Updater
    /// The account a "Forget" is waiting to be confirmed for.
    @State private var forgetting: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let advice = model.advice {
                Label(
                    "\(advice.ran) has none of its \(advice.limit) limit left. "
                        + "\(advice.use) has \(advice.left)% of its own left.",
                    systemImage: "exclamationmark.circle"
                )
                .font(.callout)
                .fixedSize(horizontal: false, vertical: true)
            }
            if let adopted = model.adopted, adopted > Date() {
                Text(
                    "Sessions already open follow in ",
                    comment: "followed by a countdown"
                )
                .font(.caption).foregroundStyle(.secondary)
                    + Text(timerInterval: Date()...adopted, countsDown: true)
                    .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
            }
            if updater.available, let version = updater.waiting {
                HStack {
                    Label("Version \(version) is ready", systemImage: "arrow.down.circle")
                        .font(.callout)
                    Spacer()
                    Button("Install") { updater.check() }
                }
            }
            if let problem = model.problem {
                Label(problem, systemImage: "exclamationmark.triangle")
                    .font(.callout)
                    .foregroundStyle(.orange)
                    .fixedSize(horizontal: false, vertical: true)
            }
            // Every warning, not only the first. A switch can warn about an overriding
            // environment variable and a config that did not update at once, and showing
            // one of them is how somebody fixes the wrong thing.
            ForEach(model.warnings.dropFirst(), id: \.code) { warning in
                Label(warning.message, systemImage: "exclamationmark.triangle")
                    .font(.callout)
                    .foregroundStyle(.orange)
                    .fixedSize(horizontal: false, vertical: true)
            }
            // An interrupted switch nothing can finish. Until now this sent the person to a
            // terminal, which is the one place somebody who installed only the app has not
            // got.
            if model.stuck {
                HStack(alignment: .firstTextBaseline) {
                    Text("An interrupted switch cannot be finished until Anthropic answers.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                    Spacer()
                    Button("Give up on it") { Task { await model.abandonStuckSwitch() } }
                        .help("Keeps every login. Nothing is deleted.")
                }
            }
            // Before the accounts, because on a machine that is not set up yet the thing
            // to do comes before the nothing there is to show.
            if model.naming == nil, model.signingIn == nil {
                FirstRun(model: model)
            }
            if let status = model.status {
                ForEach(status.accounts, id: \.accountUuid) { account in
                    AccountRow(account: account, model: model)
                        .contextMenu {
                            if let label = account.label, !account.signedIn {
                                Button("Forget \(label)…", role: .destructive) {
                                    forgetting = label
                                }
                            }
                        }
                }
                if let asking = model.naming {
                    NameIt(model: model, asking: asking)
                }
                if let signingIn = model.signingIn {
                    SigningInView(model: model, signingIn: signingIn)
                }
            } else if model.problem == nil {
                ProgressView().controlSize(.small)
            }
            if !model.checks.isEmpty {
                Divider()
                DiagnosisPanel(model: model)
            }
            Divider()
            Footer(model: model, updater: updater)
        }
        .padding(14)
        .frame(width: 400)
        .task { await model.refresh(ifOlderThan: AppModel.staleAfter) }
        .alert(
            "Forget \(forgetting ?? "")?",
            isPresented: .init(get: { forgetting != nil }, set: { if !$0 { forgetting = nil } })
        ) {
            Button("Cancel", role: .cancel) { forgetting = nil }
            Button("Forget", role: .destructive) {
                if let label = forgetting {
                    Task { await model.forget(label) }
                }
                forgetting = nil
            }
        } message: {
            Text("Its parked login is deleted. Adding it again needs a browser sign-in.")
        }
    }
}

struct Footer: View {
    let model: AppModel
    let updater: Updater
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        HStack {
            Text(model.updated).font(.caption).foregroundStyle(.secondary)
            Spacer()
            Button("Refresh") { Task { await model.refresh(asked: true) } }
            Menu {
                if updater.available {
                    Button("Check for Updates…") { updater.check() }
                }
                if model.unenrolled {
                    Button("Enrol the account in use…") { model.naming = .theOneInUse }
                }
                Button("Add another account…") { model.naming = .another }
                Divider()
                Button("Open pitboard") {
                    // An app with no Dock icon has nothing to bring forward but itself, and
                    // the window opens behind whatever is in front otherwise.
                    NSApp.activate(ignoringOtherApps: true)
                    openWindow(id: DetailWindow.id)
                }
                .keyboardShortcut("0")
                // A link rather than a button: opening Settings from a menu bar app by hand
                // is the one thing SwiftUI does not reliably do.
                SettingsLink { Text("Settings…") }
                    .keyboardShortcut(",")
                Divider()
                Button(model.checks.isEmpty ? "Check this machine…" : "Hide checks") {
                    if model.checks.isEmpty {
                        Task { await model.diagnose() }
                    } else {
                        model.forgetDiagnosis()
                    }
                }
                Divider()
                Text(
                    "pitboard \(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "")"
                )
                Link(
                    "Documentation",
                    destination: URL(string: "https://usepitboard.com/guide/")!)
                Divider()
                Button("Quit pitboard") { NSApp.terminate(nil) }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
        }
    }
}
