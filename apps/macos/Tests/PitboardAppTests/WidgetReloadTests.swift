import PitboardKit
import PitboardTestSupport
import Testing

@testable import PitboardApp

// WidgetKit gives a widget "from 40 to 70 refreshes" a day, and the accounts are read every
// five minutes.
@MainActor
@Test func widgetsAndControlsReloadOnlyAsTheGlanceTheyShowMoves() {
    let app = AppModel(model: StandInModel(snapshot(1)))
    var reloads = 0
    GlanceWatch(shortcuts: nil) { reloads += 1 }.watch(app)

    app.apply(snapshot(2))
    #expect(reloads == 0)
    app.apply(snapshot(3, widgetRevision: 1))
    app.apply(snapshot(4, widgetRevision: 1))
    #expect(reloads == 1)
    app.apply(snapshot(5, widgetRevision: 2))
    #expect(reloads == 2)
}
