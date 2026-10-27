import AppIntents
import PitboardGlance
import PitboardWidgetViews
import SwiftUI
import WidgetKit

// Sandboxed, and links no Rust: it reads the glance the app keeps in their App Group, and
// nothing else. The gallery's names and descriptions are WidgetKit's to show before any glance.
@main
struct PitboardWidgets: WidgetBundle {
    init() {
        AppDependencyManager.shared.add(dependency: GlanceSource.group)
        AppDependencyManager.shared.add(dependency: GlanceActions.refusing(.group))
    }

    var body: some Widget {
        AccountWidget()
        AccountsWidget()
        if #available(macOS 26.0, *) {
            AccountControl()
        }
    }
}

// The entity, its query and the buttons' intents are PitboardGlance's.
struct PitboardWidgetsIntents: AppIntentsPackage {
    static var includedPackages: [any AppIntentsPackage.Type] { [PitboardGlancePackage.self] }
}

struct AccountChoice: WidgetConfigurationIntent {
    static let title: LocalizedStringResource = "Account"
    static let description = IntentDescription(
        "The account the widget shows. Without one, it shows the account in use.")

    @Parameter(title: "Account") var account: AccountEntity?
}

struct AccountTimeline: AppIntentTimelineProvider {
    func placeholder(in context: Context) -> GlanceEntry {
        GlanceEntry(date: .now, glance: nil)
    }

    func snapshot(for configuration: AccountChoice, in context: Context) async -> GlanceEntry {
        GlanceEntry(date: .now, glance: AppGroup.glance(), chosen: configuration.account?.id)
    }

    func timeline(for configuration: AccountChoice, in context: Context) async
        -> Timeline<GlanceEntry>
    {
        GlanceEntry.timeline(of: AppGroup.glance(), chosen: configuration.account?.id, at: .now)
    }
}

struct AccountWidget: Widget {
    var body: some WidgetConfiguration {
        AppIntentConfiguration(
            kind: "Account", intent: AccountChoice.self, provider: AccountTimeline()
        ) { entry in
            AccountView(entry: entry)
        }
        .configurationDisplayName("Account")
        .description("How much of each limit an account has used.")
        .supportedFamilies([.systemSmall])
    }
}

struct AccountsTimeline: TimelineProvider {
    func placeholder(in context: Context) -> GlanceEntry {
        GlanceEntry(date: .now, glance: nil)
    }

    func getSnapshot(in context: Context, completion: @escaping (GlanceEntry) -> Void) {
        completion(GlanceEntry(date: .now, glance: AppGroup.glance()))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<GlanceEntry>) -> Void)
    {
        completion(GlanceEntry.timeline(of: AppGroup.glance(), at: .now))
    }
}

struct AccountsWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "Accounts", provider: AccountsTimeline()) { entry in
            AccountsFamily(entry: entry)
        }
        .configurationDisplayName("Accounts")
        .description(
            "How much each account in use has used, or every account in the large size."
        )
        .supportedFamilies([.systemMedium, .systemLarge])
    }
}

struct AccountsFamily: View {
    let entry: GlanceEntry
    @Environment(\.widgetFamily) private var family

    var body: some View {
        AccountsView(entry: entry, family: family)
    }
}

@available(macOS 26.0, *)
struct AccountControlChoice: ControlConfigurationIntent {
    static let title: LocalizedStringResource = "Account"

    @Parameter(title: "Account") var account: AccountEntity?
}

@available(macOS 26.0, *)
struct AccountControlValue: AppIntentControlValueProvider {
    func previewValue(configuration: AccountControlChoice) -> ControlShown {
        shown(configuration)
    }

    func currentValue(configuration: AccountControlChoice) async throws -> ControlShown {
        shown(configuration)
    }

    private func shown(_ configuration: AccountControlChoice) -> ControlShown {
        ControlShown(
            AppGroup.glance(), chosen: configuration.account?.id, app: Bundle.main.displayName)
    }
}

// On macOS 26 a control's intent runs in the app only as an OpenIntent.
@available(macOS 26.0, *)
struct AccountControl: ControlWidget {
    var body: some ControlWidgetConfiguration {
        AppIntentControlConfiguration(kind: "AccountControl", provider: AccountControlValue()) {
            shown in
            ControlWidgetButton(shown.name, action: OpenPlaceIntent(target: .accounts)) { _ in
                Label(shown.value, image: "pitboard")
            }
        }
        .displayName("Account")
        .description("How much of its limits an account has used. Opens Pitboard.")
        .promptsForUserConfiguration()
    }
}
