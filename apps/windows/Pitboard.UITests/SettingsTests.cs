using FlaUI.Core.AutomationElements;
using FlaUI.Core.Definitions;
using FlaUI.Core.Tools;

namespace Pitboard.UITests;

// The macOS PanesAndSettingsTests that are about Settings, on the window's settings page.
[TestClass]
[Elevated(ConditionMode.Exclude)]
public sealed class SettingsTests
{
    private static readonly TimeSpan Shortly = TimeSpan.FromSeconds(10);

    private static void OpenSettings(Launched launched)
    {
        launched.OpenWindow();
        launched.Select("SettingsItem");
    }

    private static AutomationElement Control(Launched launched, string automationId) =>
        launched.Find(found => found.ByAutomationId(automationId), automationId);

    private static void Expand(Launched launched, string automationId) => Launched.Expand(Control(launched, automationId));

    private static ToggleState State(AutomationElement toggle) => toggle.Patterns.Toggle.Pattern.ToggleState.Value;

    // The model answers a change after the click, so each read after one waits.
    private static bool Becomes(AutomationElement toggle, ToggleState state) =>
        Retry.WhileFalse(() => State(toggle) == state, Shortly, TimeSpan.FromMilliseconds(250), ignoreException: true).Result;

    private static bool Enables(AutomationElement control, bool enabled) =>
        Retry.WhileFalse(() => control.IsEnabled == enabled, Shortly, TimeSpan.FromMilliseconds(250), ignoreException: true).Result;

    // The model's choices, in its order: Do nothing, Notify me, Switch automatically.
    private const int DoNothing = 0;
    private const int Notify = 1;
    private const int SwitchAutomatically = 2;

    private static int Chosen(ComboBox choice)
    {
        choice.Expand();
        var chosen = Array.FindIndex(choice.Items, item => item.IsSelected);
        choice.Collapse();
        return chosen;
    }

    // The choice's name says the share it is made at.
    private static bool Names(ComboBox choice, int share) =>
        Retry.WhileFalse(() => choice.Name.EndsWith($" {share}%", StringComparison.Ordinal), Shortly, TimeSpan.FromMilliseconds(250), ignoreException: true).Result;

    private static bool Says(Launched launched, string start, TimeSpan within) =>
        Retry.WhileFalse(
            () => Launched.Lines(launched.Window("Pitboard")).Any(line => line.StartsWith(start, StringComparison.Ordinal)),
            within, TimeSpan.FromMilliseconds(250), ignoreException: true).Result;

    // The fixture's stand-in takes it, so nothing is written to the Run key.
    [TestMethod]
    public void StartAtSignIn()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        OpenSettings(launched);
        var toggle = Control(launched, "settings.startAtSignIn");

        Assert.AreEqual(ToggleState.Off, State(toggle));
        toggle.Patterns.Toggle.Pattern.Toggle();
        Assert.IsTrue(Becomes(toggle, ToggleState.On));
        toggle.Patterns.Toggle.Pattern.Toggle();
        Assert.IsTrue(Becomes(toggle, ToggleState.Off));
    }

    // The fixture's stand-in for a Run value Settings > Apps > Startup turned off. The link is not pressed: it opens
    // the runner's own Settings.
    [TestMethod]
    public void StartAtSignInTurnedOffInWindowsSettingsLinksThere()
    {
        using var launched = Launched.Start(
            Builds.Debug, "oneTool", new Dictionary<string, string> { ["PITBOARD_FIXTURE_STARTUP"] = "off" });
        OpenSettings(launched);
        var toggle = Control(launched, "settings.startAtSignIn");

        Assert.IsTrue(Enables(toggle, false), "the switch can be turned on past Settings");
        Assert.AreEqual(ToggleState.Off, State(toggle));
        Assert.IsTrue(Says(launched, "Turned off in Settings > Apps > Startup", Shortly));
        Control(launched, "settings.startupSettings");
    }

    [TestMethod]
    public void WhatHappensBeforeAnAccountRunsOut()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        OpenSettings(launched);
        var choice = Control(launched, "settings.atLimit").AsComboBox();
        Expand(launched, "settings.atLimitGroup");
        var share = Control(launched, "settings.atLimitShare");

        // Both are held back until the app's preferences have been read.
        Assert.IsTrue(Enables(choice, true));
        Assert.AreEqual(Notify, Chosen(choice), "a fresh install notifies");
        Assert.IsTrue(Enables(share, true));
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("settings.autoSwitchStanding")), "nothing said while it does not switch");
        Assert.IsTrue(Names(choice, 95));

        choice.Select(SwitchAutomatically);
        Control(launched, "settings.autoSwitchStanding");

        share.Patterns.RangeValue.Pattern.SetValue(96);
        Assert.IsTrue(Names(choice, 96), "the model did not take the share");

        choice.Select(DoNothing);
        Assert.IsTrue(Enables(share, false));
        Assert.IsTrue(launched.Gone(found => found.ByAutomationId("settings.autoSwitchStanding")));
    }

    // The fixture's personal is on for warm-ups with no five-hour period running, so it warms up as the app starts.
    [TestMethod]
    public void WarmingUpAccounts()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "warmUp");
        launched.OpenWindow();
        Assert.IsTrue(Says(launched, Words.WarmsUpAgain, TimeSpan.FromSeconds(30)), "personal is not warmed up");

        launched.Select("SettingsItem");
        Expand(launched, "settings.warm");
        var personal = Control(launched, "settings.warm.claude/personal");
        var work = Control(launched, "settings.warm.claude/work");

        Assert.IsTrue(Enables(personal, true), "personal's warm-up cannot be turned off");
        Assert.AreEqual(ToggleState.On, State(personal));
        Assert.AreEqual(ToggleState.Off, State(work));
        Assert.IsTrue(Control(launched, "settings.warmFrom").IsEnabled, "the hours cannot be changed");
        Control(launched, "settings.warmStanding");

        work.Patterns.Toggle.Pattern.Toggle();
        Assert.IsTrue(Becomes(work, ToggleState.On));
    }

    [TestMethod]
    public void DailyRenewal()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        OpenSettings(launched);

        var renew = Control(launched, "settings.renewDaily");
        renew.Patterns.Toggle.Pattern.Toggle();
        Assert.IsTrue(Becomes(renew, ToggleState.On));
    }

    // The fixture puts no pitboard where a terminal looks, and the installer, not the app, puts one on the PATH.
    [TestMethod]
    public void TheCommandLineSectionSaysWherePitboardIs()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        OpenSettings(launched);

        Assert.AreEqual(Words.NotInstalled, Control(launched, "settings.inTerminal").Name);
        Assert.IsTrue(Says(launched, Words.NewTerminals, Shortly));
    }

    // A Debug build carries no update key, and General says so.
    [TestMethod]
    public void UpdatesSayWhenTheBuildCannotUpdate()
    {
        using var launched = Launched.Start(Builds.Debug, fixture: "oneTool");
        OpenSettings(launched);

        Assert.IsTrue(Says(launched, Words.NoUpdateKey, Shortly));
    }
}
