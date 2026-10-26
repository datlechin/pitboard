import AppKit
import PitboardKit
import SwiftUI

// Shown once, at the first launch, before the accounts.
struct WelcomeWindow: View {
    let model: AppModel
    let openAtLogin: OpenAtLogin
    let presence: AppPresence
    @Environment(\.dismissWindow) private var dismissWindow

    var body: some View {
        let welcome = model.welcome
        VStack(spacing: 0) {
            VStack(spacing: Design.rowSpacing) {
                Image(nsImage: NSApp.applicationIconImage)
                    .resizable()
                    .frame(width: 96, height: 96)
                    .accessibilityHidden(true)
                Text(welcome.title)
                    .font(.largeTitle)
                    .fontWeight(.bold)
                    .accessibilityAddTraits(.isHeader)
                if let says = welcome.lines.first {
                    Text(says)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .padding(.horizontal, 40)
            // The line says where Pitboard lives, under a picture of it.
            if welcome.lines.count > 1 {
                VStack(spacing: Design.rowSpacing) {
                    MenuBarPicture()
                    Text(welcome.lines[1]).font(.callout).foregroundStyle(.secondary)
                }
                .padding(.top, 20)
            }
            Form {
                if !welcome.tools.isEmpty {
                    Section {
                        ForEach(welcome.tools, id: \.code) { tool in
                            WelcomeToolRow(tool: tool) { model.send($0) }
                        }
                    }
                }
                Section {
                    Toggle(
                        welcome.openAtLogin.title,
                        isOn: Binding(
                            get: { openAtLogin.state != .disabled },
                            set: { openAtLogin.set($0) })
                    )
                    .accessibilityIdentifier("welcome.openAtLogin")
                    LoginItemApproval(openAtLogin: openAtLogin)
                } footer: {
                    Text(welcome.openAtLogin.note).footnote()
                }
            }
            .formStyle(.grouped)
            // On the window's own background, as the rest of the page is.
            .scrollContentBackground(.hidden)
            .scrollDisabled(true)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 20)
            Button {
                model.send(welcome.done.intent)
                dismissWindow(id: PlaceWindow.welcome.id)
            } label: {
                Text(welcome.done.title).frame(minWidth: 160)
            }
            .keyboardShortcut(.defaultAction)
            .controlSize(.large)
            .disabled(!welcome.done.enabled)
            .accessibilityIdentifier("welcome.done")
            .padding(.bottom, 30)
        }
        .padding(.top, 30)
        .frame(width: 480)
        .appWindow(presence)
    }
}

private struct WelcomeToolRow: View {
    let tool: WelcomeTool
    let send: (Intent) -> Void

    var body: some View {
        LabeledContent {
            switch tool.step {
            case .added(let said):
                Text(said).foregroundStyle(.secondary)
            case .add(let choice):
                // Reads the tool's login only now, on the click (#282).
                Button(choice.title) { send(choice.intent) }
                    .disabled(!choice.enabled)
                    .accessibilityIdentifier("welcome.add.\(tool.code)")
            case .install(let link):
                if let page = URL(string: link.link) {
                    Link(link.title, destination: page)
                }
            }
        } label: {
            Text(tool.name)
            if let account = tool.account {
                Text(account)
            }
        }
    }
}

// The right end of a menu bar, Pitboard's item marked as it is while its menu is open.
private struct MenuBarPicture: View {
    var body: some View {
        HStack(spacing: 14) {
            Image("pitboard")
                .symbolRenderingMode(.hierarchical)
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(.quaternary, in: .rect(cornerRadius: 4))
                .overlay {
                    RoundedRectangle(cornerRadius: 5).strokeBorder(.tint, lineWidth: 2)
                        .padding(-2)
                }
            Image(systemName: "battery.75percent")
            Image(systemName: "wifi")
            Image(systemName: "switch.2")
            Text(.now, format: .dateTime.weekday().hour().minute())
                .monospacedDigit()
        }
        .font(.system(size: 13))
        .lineLimit(1)
        .padding(.horizontal, 14)
        .frame(width: 340, height: 34, alignment: .trailing)
        .background(.background.secondary, in: .rect(cornerRadius: 8))
        .overlay { RoundedRectangle(cornerRadius: 8).strokeBorder(.separator) }
        .accessibilityHidden(true)
    }
}
