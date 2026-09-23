import AppKit
import PitboardKit
import SwiftUI

struct MenuView: View {
    let model: AppModel
    let updater: Updater
    /// The account a "Forget" is waiting to be confirmed for.
    @State private var forgetting: Account?
    /// The panel grows with the text in it. Bounded because this is a popover hung off the
    /// menu bar and not a window: past about half a small screen it stops being a glance.
    @ScaledMetric(relativeTo: .body) private var width: CGFloat = 400

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(model.advice, id: \.key) { advice in
                Label(advice.said, systemImage: "exclamationmark.circle")
                    .font(.callout)
                    .fixedSize(horizontal: false, vertical: true)
            }
            ForEach(model.lastSwitches, id: \.provider) { last in
                AfterSwitch(model: model, last: last)
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
            ForEach(model.otherWarnings, id: \.code) { warning in
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
                    Text(
                        "An interrupted switch cannot be finished until \(model.services) answers."
                    )
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
            if model.status != nil {
                // A heading per tool once there is more than one, and none before: a machine
                // with one tool looks exactly as it did.
                ForEach(model.groups) { group in
                    if let name = group.name {
                        Text(name)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.secondary)
                            .accessibilityAddTraits(.isHeader)
                    }
                    ForEach(group.accounts, id: \.id) { account in
                        AccountRow(account: account, model: model)
                            .contextMenu {
                                if let label = account.label, !account.signedIn {
                                    Button("Forget \(label)…", role: .destructive) {
                                        forgetting = account
                                    }
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
        .frame(width: min(width, 620))
        .task { await model.refresh(ifOlderThan: AppModel.staleAfter) }
        .alert(
            "Forget \(forgetting.map(model.name(of:)) ?? "")?",
            isPresented: .init(get: { forgetting != nil }, set: { if !$0 { forgetting = nil } })
        ) {
            Button("Cancel", role: .cancel) { forgetting = nil }
            Button("Forget", role: .destructive) {
                if let qualified = forgetting?.qualified {
                    Task { await model.forget(qualified) }
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
                .keyboardShortcut("r")
            Menu {
                if updater.available {
                    Button("Check for Updates…") { updater.check() }
                }
                ForEach(model.unnamed, id: \.id) { login in
                    Button(
                        model.showsTools
                            ? "Enrol the \(model.tool(login.provider)?.name ?? login.provider) "
                                + "account in use…"
                            : "Enrol the account in use…"
                    ) { model.naming = .theOneInUse(login.provider) }
                }
                Button("Add another account…") { model.naming = .another(nil) }
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
            .accessibilityLabel("More")
        }
    }
}

/// What a tool's last switch means for sessions already running, and what it warned about.
///
/// A tool whose running sessions never pick a switch up gets a plain sentence where a
/// countdown would otherwise be, and the switch's own warnings are kept here after the read
/// that follows it: that read replaces the panel's warnings, and these are about the switch.
/// A sign-in that put a new login in use says so here, above what it warned about.
private struct AfterSwitch: View {
    let model: AppModel
    let last: AppModel.LastSwitch

    var body: some View {
        if let adopted = last.adopted, adopted > Date() {
            Text(following)
                .font(.caption).foregroundStyle(.secondary)
                + Text(timerInterval: Date()...adopted, countsDown: true)
                .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
        }
        let warned = model.warnings(after: last)
        if last.said != nil || last.notice != nil || !warned.isEmpty {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 6) {
                    if let said = last.said {
                        Text(said)
                            .font(.callout)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    if let notice = last.notice {
                        Text(notice)
                            .font(.callout)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    ForEach(warned, id: \.code) { warning in
                        Label(warning.message, systemImage: "exclamationmark.triangle")
                            .font(.callout)
                            .foregroundStyle(.orange)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                Spacer(minLength: 0)
                Button("Dismiss", systemImage: "xmark") {
                    model.forgetSwitch(of: last.provider)
                }
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
                .help("Dismiss")
            }
        }
    }

    /// Followed by the countdown. Names the tool once more than one is shown, since each
    /// tool's last switch is said on its own and a countdown beside a Codex notice would
    /// otherwise read as contradicting it.
    private var following: String {
        guard model.showsTools, let tool = model.tool(last.provider) else {
            return "Sessions already open follow in "
        }
        return "\(tool.name) sessions already open follow in "
    }
}
