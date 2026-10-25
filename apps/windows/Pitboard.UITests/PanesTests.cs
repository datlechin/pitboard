using FlaUI.Core.Definitions;

namespace Pitboard.UITests;

[TestClass]
[Elevated(ConditionMode.Exclude)]
public sealed class PanesTests
{
    // The fixture's switches and enrolments, in the order the model gives them.
    [TestMethod]
    public void ActivityListsChanges()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        launched.OpenWindow();
        launched.Select("pane.activity");

        var activity = launched.Element("activity");
        Launched.Within(activity, found => found.ByControlType(ControlType.ListItem), "a change");

        Assert.IsGreaterThan(1, activity.FindAllDescendants(found => found.ByControlType(ControlType.ListItem)).Length);
    }

    // The checks the fixture's doctor makes, under the line that sums them up.
    [TestMethod]
    public void ThisPCShowsTheChecks()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        launched.OpenWindow();
        launched.Select("pane.machine");

        launched.Element("machine.check");
        launched.Element("machine.summary");
    }
}
