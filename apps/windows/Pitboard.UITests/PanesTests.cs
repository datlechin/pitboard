using FlaUI.Core.Definitions;

namespace Pitboard.UITests;

// Settings' own pages, which Troubleshoot's cards open, as Windows Settings opens Troubleshoot's.
[TestClass]
[Elevated(ConditionMode.Exclude)]
public sealed class PanesTests
{
    // The fixture's switches and enrolments, in the order the model gives them.
    [TestMethod]
    public void ActivityListsChanges()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        var window = launched.OpenWindow();
        launched.OpenFromSettings(window, "settings.activity");

        var activity = launched.Element("activity");
        Launched.Within(activity, found => found.ByControlType(ControlType.ListItem), "a change");

        Assert.IsGreaterThan(1, activity.FindAllDescendants(found => found.ByControlType(ControlType.ListItem)).Length);
    }

    // The checks the fixture's doctor makes, under the line that sums them up.
    [TestMethod]
    public void DiagnosticsShowsTheChecks()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        var window = launched.OpenWindow();
        launched.OpenFromSettings(window, "settings.diagnostics");

        launched.Element("machine.check");
        launched.Element("machine.summary");
    }

    // The title bar's back button, a template part UI Automation finds by its name, as a ContentDialog's buttons.
    [TestMethod]
    public void TheTitleBarsBackButtonGoesBackToSettings()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        var window = launched.OpenWindow();
        launched.OpenFromSettings(window, "settings.diagnostics");
        launched.Element("machine.check");

        launched.Invoke("PART_BackButton");

        launched.Element("settings.diagnostics");
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("machine.check")), "Diagnostics stayed");
        Assert.IsTrue(launched.Selected("SettingsItem"), "Settings is not selected");
    }
}
