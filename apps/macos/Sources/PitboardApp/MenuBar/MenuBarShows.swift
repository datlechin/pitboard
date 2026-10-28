import PitboardKit

// macOS hides the widest menu bar items first, so a crowded bar can drop the name, then the figure.
enum MenuBarShows: String, CaseIterable, Identifiable {
    case nameAndUsage
    case usage
    case icon

    var id: String { rawValue }

    var title: String {
        switch self {
        case .nameAndUsage: macWords.accountAndUsage
        case .usage: macWords.usageOnly
        case .icon: macWords.iconOnly
        }
    }

    func text(of bar: MenuBarText) -> String {
        switch self {
        case .nameAndUsage: bar.nameAndUsage
        case .usage: bar.usage
        case .icon: ""
        }
    }
}
