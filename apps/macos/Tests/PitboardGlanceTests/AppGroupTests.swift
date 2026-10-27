import PitboardGlance
import Testing

// macOS lets a program into a group under its own team without asking, and asks of any other,
// which a widget extension is never asked and is refused.
@Test func aBuildKeepsTheGlanceOnlyInAGroupUnderItsOwnTeam() {
    let ours = "D7HJ5TFYCU.com.usepitboard.Pitboard"
    #expect(AppGroup.named(groups: [ours], team: "D7HJ5TFYCU") == ours)
    #expect(AppGroup.named(groups: [ours], team: nil) == nil)
    #expect(AppGroup.named(groups: [ours], team: "ABCDE12345") == nil)
    #expect(AppGroup.named(groups: [], team: "D7HJ5TFYCU") == nil)
}

// An ad hoc build has no team, as every test, CI and clone build is.
@Test func aBuildWithNoTeamKeepsOutOfEveryGroup() {
    #expect(AppGroup.own == nil)
}
