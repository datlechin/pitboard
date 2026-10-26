using CommunityToolkit.WinUI.Controls;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;
using Windows.Foundation;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class SettingsPageTests
{
    private const string Watching = "Pitboard watches work: 42% used at 09:00.";

    private static readonly string[] Sections = ["General", "Limits", "Advanced", "Troubleshoot", "About"];

    private static readonly WarmShown TwoWarmed = new(
        Title: "Warm up limits", From: 360, To: 24 * 60, RangeLabel: "6:00 AM to midnight",
        Accounts:
        [
            new WarmAccountShown("claude/personal", "personal · Max", On: true, Standing: "Warms up again at 22:50"),
            new WarmAccountShown("claude/work", "work", On: false, Standing: null),
        ],
        NoneEnrolled: null, Standing: "Next warm-up: personal at 22:50.", Note: "Pitboard warms up Claude Code accounts only.",
        Enabled: true, Terms: Snapshots.Terms);

    private const string NewTerminals = "Pitboard’s installer puts pitboard on your PATH, which terminals opened after it ran see.";

    private static Snapshot With(ulong revision, Func<MachineShown, MachineShown> change)
    {
        var read = Snapshots.Read(revision);
        return read with { Machine = change(read.Machine) };
    }

    private static Task<SettingsPage> Shown(
        AppModel model, StartAtSignIn? start = null, IUpdates? updates = null, string version = "0.0.0", Action<Uri>? open = null,
        Func<NotificationsBlocked?>? notifications = null) =>
        Views.Navigated<SettingsPage>(Views.Context(model, start, updates, version, open, notifications: notifications));

    private static bool Shows(SettingsPage page, string automationId) =>
        Views.Visible<TextBlock>(page).Any(text => AutomationProperties.GetAutomationId(text) == automationId);

    // The expander's repeater makes its rows over more than one layout, so a test waits on the last row it needs.
    private static async Task Expand(SettingsPage page, string automationId, string inside)
    {
        Views.Element<SettingsExpander>(page, automationId).IsExpanded = true;
        await Views.Until(() => Views.Texts(page).Contains(inside), $"{automationId} did not open on '{inside}'");
    }

    private static Rect Bounds(FrameworkElement element, UIElement within) =>
        element.TransformToVisual(within).TransformBounds(new Rect(0, 0, element.ActualWidth, element.ActualHeight));

    private static SettingsCard Card(DependencyObject inside) =>
        Look(inside) ?? throw new AssertFailedException("not in a SettingsCard");

    private static SettingsCard? Look(DependencyObject? at) =>
        at is null ? null : at as SettingsCard ?? Look(VisualTreeHelper.GetParent(at));

    private static TextBlock Said(SettingsPage page, string text) =>
        Views.Visible<TextBlock>(page).SingleOrDefault(shown => shown.Text == text)
        ?? throw new AssertFailedException($"'{text}' is not among: {string.Join(", ", Views.Texts(page))}");

    // A notice about a limit, or the tip about one, opens the page on Limits, and a check's fix on Advanced, wherever it
    // was scrolled to.
    [UITestMethod]
    public async Task ShowingASectionBringsItToTheTop()
    {
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))));
        page.Height = 300;
        page.UpdateLayout();
        var scroller = Views.Descendants(page).OfType<ScrollViewer>().First();

        foreach (var (section, heading) in new[] { (SettingsSection.Limits, "Limits"), (SettingsSection.Advanced, "Advanced"), (SettingsSection.General, "General") })
        {
            var header = Said(page, heading);
            page.Show(section);

            await Views.Until(
                () => Bounds(header, scroller).Top is > -1 and < 60,
                () => $"{heading} at {Bounds(header, scroller).Top}, scrolled {scroller.VerticalOffset}");
        }
    }

    // Plan 4.11: Uninstall Pitboard in Advanced asks the model's question, whose Uninstall Enter does not press, and only
    // then asks the model to uninstall.
    [UITestMethod]
    public async Task UninstallAsksTheModelsQuestionFirst()
    {
        var question = new Question(
            "Uninstall Pitboard?", "Pitboard deletes its saved sign-ins, settings and account windows.", "Uninstall",
            Destructive: false, ConfirmIsDefault: false);
        var model = new StandInModel(Snapshots.Read(1) with { Uninstall = new ItemOffer("Uninstall…", new Intent.Uninstall(), Enabled: true, question) });
        var context = Views.Context(new AppModel(model));
        var page = await Views.Navigated<SettingsPage>(context);
        var uninstall = Views.Element<Button>(page, "settings.uninstall");
        Assert.AreEqual("Uninstall…", uninstall.Content);
        Assert.AreEqual("Uninstall Pitboard", Card(uninstall).Header);

        Views.Press(uninstall);
        await Views.Until(() => context.Dialogs.Open is { IsLoaded: true }, "no question");
        var asked = context.Dialogs.Open!;
        Assert.AreEqual("Uninstall Pitboard?", asked.Title);
        Assert.AreEqual(ContentDialogButton.None, asked.DefaultButton);
        Assert.IsEmpty(model.Sent.OfType<Intent.Uninstall>(), "uninstalled before the answer");
        Views.Press(Views.Descendants(asked).OfType<Button>().Single(button => button.Name == "PrimaryButton"));

        await Views.Until(() => model.Sent.Contains(new Intent.Uninstall()), "the answer did not uninstall");
    }

    // As Windows Settings keeps Troubleshoot: a card for each of the pages that look into Pitboard, which open them.
    [UITestMethod]
    public async Task TroubleshootHasACardForDiagnosticsAndActivity()
    {
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))));

        SettingsCard[] cards = [Views.Element<SettingsCard>(page, "settings.diagnostics"), Views.Element<SettingsCard>(page, "settings.activity")];

        Assert.AreEqual(
            "Diagnostics: Check how Pitboard, Claude Code and Codex are set up on this PC. | Activity: Every switch and change Pitboard made.",
            string.Join(" | ", cards.Select(card => $"{card.Header}: {card.Description}")));
        Assert.IsTrue(cards.All(card => card.IsClickEnabled), "a card that does not open its page");
    }

    // The macOS Settings' three panes, Troubleshoot, then About.
    [UITestMethod]
    public async Task ThePageHasTheSectionsOfTheMacOSPanes()
    {
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))));
        var heading = (Style)Application.Current.Resources["SettingsSectionHeaderTextBlockStyle"];

        CollectionAssert.AreEqual(
            Sections, Views.Visible<TextBlock>(page).Where(text => text.Style == heading).Select(text => text.Text).ToArray());
    }

    // Plan 4.11 and the approved mockup: each setting with its icon, as Windows Settings draws its own.
    [UITestMethod]
    public async Task EachSettingHasItsIcon()
    {
        var page = await Shown(
            new AppModel(new StandInModel(Snapshots.Read(1))), updates: new StandInUpdates(),
            notifications: () => new("Off.", InSettings: true));
        var settings = ((Panel)Said(page, Words.Settings.General).Parent).Children
            .Where(setting => setting is SettingsCard or SettingsExpander && setting.Visibility == Visibility.Visible)
            .ToArray();

        Assert.HasCount(12, settings, "4 in General, 2 in Limits, 3 in Advanced, 2 in Troubleshoot and About");
        Assert.AreEqual(
            "",
            string.Join(", ", settings
                .Where(setting => setting is SettingsCard { HeaderIcon: null } or SettingsExpander { HeaderIcon: null })
                .Select(setting => (setting as SettingsCard)?.Header ?? (setting as SettingsExpander)?.Header)));
    }

    // The welcome's sentence under the same switch, and the version under its own name with Check now, as the mockup has
    // them.
    [UITestMethod]
    public async Task GeneralSaysWhatStartingAtSignInIsForAndWhichVersionThisIs()
    {
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))), updates: new StandInUpdates(), version: "1.1.0");
        var checkNow = Views.Element<Button>(page, "settings.checkNow");

        Assert.AreSame(
            Card(Views.Element<ToggleSwitch>(page, "settings.startAtSignIn")), Card(Said(page, Words.Settings.OpenAtLoginNote)));
        Assert.AreEqual(Words.Settings.Version, Card(checkNow).Header);
        Assert.AreSame(Card(checkNow), Card(Said(page, PitboardFfiMethods.NamedVersion("1.1.0"))));
        Assert.AreEqual("C", checkNow.AccessKey);
    }

    [UITestMethod]
    public async Task ShowingThePageReadsTheScheduleAndLooksForTheCommandLine()
    {
        var model = new StandInModel(Snapshots.Read(1));

        await Shown(new AppModel(model));

        Assert.Contains(new Intent.ReadSchedule(), model.Sent);
        Assert.Contains(new Intent.LookForCommandLine(), model.Sent);
    }

    [UITestMethod]
    public async Task StartAtSignInTurnsOnAndOffAndSaysWhereElseItIsTurnedOff()
    {
        var start = new StartAtSignIn(new FixtureStartupItem());
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))), start);
        var toggle = Views.Element<ToggleSwitch>(page, "settings.startAtSignIn");

        Assert.IsFalse(toggle.IsOn);
        Assert.DoesNotContain("Settings > Apps > Startup can turn it off too.", Views.Texts(page));

        toggle.IsOn = true;
        Assert.IsTrue(start.On);

        toggle.IsOn = false;
        Assert.IsFalse(start.On);
    }

    // As the macOS settings offer Open Login Items Settings: only Windows' own switch turns it on again.
    [UITestMethod]
    public async Task StartAtSignInTurnedOffInWindowsSettingsLinksThere()
    {
        var opened = new List<Uri>();
        var page = await Shown(
            new AppModel(new StandInModel(Snapshots.Read(1))), new StartAtSignIn(new FixtureStartupItem(offInSettings: true)),
            open: opened.Add);
        var toggle = Views.Element<ToggleSwitch>(page, "settings.startAtSignIn");
        var link = Views.Element<HyperlinkButton>(page, "settings.startupSettings");

        Assert.IsFalse(toggle.IsOn);
        Assert.IsFalse(toggle.IsEnabled);
        Assert.Contains(Words.Settings.StartupTurnedOff!, Views.Texts(page));
        Assert.AreSame(Card(toggle), Card(link));
        new HyperlinkButtonAutomationPeer(link).Invoke();
        Assert.AreEqual(new Uri("ms-settings:startupapps"), opened.Single());

        var on = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))));

        Assert.IsFalse(
            Views.Visible<HyperlinkButton>(on).Any(shown => AutomationProperties.GetAutomationId(shown) == "settings.startupSettings"),
            "a link to Settings while nothing there holds Pitboard back");
    }

    [UITestMethod]
    public async Task ADebugBuildHoldsTheSwitchBackAndSaysWhy()
    {
        var start = StartAtSignIn.For(
            new AppStart(new Identity(Build.Debug), new(), null, @"C:\Local\Pitboard App Debug"), @"C:\Build\Pitboard.exe");
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))), start);

        Assert.IsFalse(Views.Element<ToggleSwitch>(page, "settings.startAtSignIn").IsEnabled);
        Assert.Contains(Words.Settings.DebugNeverStarts!, Views.Texts(page));
    }

    [UITestMethod]
    public async Task AChangeWindowsRefusedIsSaidAndTheSwitchShowsWhatWindowsHas()
    {
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))), new StartAtSignIn(new Refusing()));
        var toggle = Views.Element<ToggleSwitch>(page, "settings.startAtSignIn");

        toggle.IsOn = true;

        Assert.IsFalse(toggle.IsOn);
        Assert.AreEqual(AutomationLiveSetting.Assertive, AutomationProperties.GetLiveSetting(Said(page, "Access is denied.")));
    }

    // Narrator says only the switch's new state, so the reason is a live region.
    [UITestMethod]
    public async Task AScheduleWindowsRefusedIsSaidAloud()
    {
        var page = await Shown(new AppModel(new StandInModel(With(1, machine => machine with
        {
            Schedule = machine.Schedule with { Failed = "Task Scheduler refused the task." },
        }))));

        Assert.AreEqual(
            AutomationLiveSetting.Assertive, AutomationProperties.GetLiveSetting(Said(page, "Task Scheduler refused the task.")));
    }

    [UITestMethod]
    public async Task ChoosingWhatHappensAtALimitSendsTheChoiceAndTheShare()
    {
        var model = new StandInModel(Snapshots.Read(1));
        var app = new AppModel(model);
        var page = await Shown(app);
        var choice = Views.Element<ComboBox>(page, "settings.atLimit");
        await Expand(page, "settings.atLimitGroup", "Share of a limit");
        var share = Views.Element<NumberBox>(page, "settings.atLimitShare");

        Assert.IsTrue(choice.IsEnabled);
        CollectionAssert.AreEqual(new object[] { "Do nothing", "Notify me", "Switch automatically" }, choice.Items.ToArray());
        Assert.AreEqual(1, choice.SelectedIndex, "a fresh install notifies");
        Assert.AreEqual("When a limit reaches 95%", AutomationProperties.GetName(choice));
        Assert.AreEqual("When a limit reaches", Views.Element<SettingsExpander>(page, "settings.atLimitGroup").Header);
        Assert.IsTrue(share.IsEnabled);
        Assert.IsFalse(Shows(page, "settings.autoSwitchStanding"), "nothing said while it does not switch");

        choice.SelectedIndex = 2;
        Assert.Contains(new Intent.SetAtLimit(AtLimit.Switch, 95), model.Sent);

        app.Apply(With(2, machine => machine with
        {
            AtLimit = machine.AtLimit with { Choice = AtLimit.Switch, Standing = Watching, Terms = Snapshots.Terms },
        }));
        Assert.IsTrue(Shows(page, "settings.autoSwitchStanding"));
        Assert.Contains(Watching, Views.Texts(page));
        // Plan 4.11: the note and Is this allowed?, the terms' own sentence left to the page it opens.
        Assert.IsTrue(Views.Visible<HyperlinkButton>(page).Any(link => AutomationProperties.GetAutomationId(link) == "settings.atLimitTerms"));
        Assert.DoesNotContain(Snapshots.Terms.Sentence, Views.Texts(page));

        share.Value = 96;
        Assert.Contains(new Intent.SetAtLimit(AtLimit.Switch, 96), model.Sent);
    }

    [UITestMethod]
    public async Task TheModelHeldBackTheChoiceUntilItHasReadThePreferences()
    {
        var unread = With(1, machine => machine with { AtLimit = machine.AtLimit with { Enabled = false, ShareEnabled = false } });
        var page = await Shown(new AppModel(new StandInModel(unread)));
        await Expand(page, "settings.atLimitGroup", "Share of a limit");

        Assert.IsFalse(Views.Element<ComboBox>(page, "settings.atLimit").IsEnabled);
        Assert.IsFalse(Views.Element<NumberBox>(page, "settings.atLimitShare").IsEnabled);
    }

    [UITestMethod]
    public async Task TheTermsLinkOpensWhetherThisIsAllowed()
    {
        var opened = new List<Uri>();
        var page = await Shown(new AppModel(new StandInModel(With(1, machine => machine with { Warm = TwoWarmed }))), open: opened.Add);
        await Expand(page, "settings.warm", Snapshots.Terms.LinkTitle);

        Assert.DoesNotContain(Snapshots.Terms.Sentence, Views.Texts(page));
        new HyperlinkButtonAutomationPeer(Views.Element<HyperlinkButton>(page, "settings.warmTerms")).Invoke();

        Assert.AreEqual(new Uri(Snapshots.Terms.Link), opened.Single());
    }

    // Each control raises its change event when the page sets it to what the model says.
    [UITestMethod]
    public async Task ShowingWhatTheModelSaysSendsNothing()
    {
        var model = new StandInModel(Snapshots.Read(1));
        var app = new AppModel(model);
        var page = await Shown(app);
        await Expand(page, "settings.warm", "No Claude Code account is enrolled yet.");
        await Expand(page, "settings.atLimitGroup", "Share of a limit");
        var sent = model.Sent.Count;

        app.Apply(With(2, machine => machine with
        {
            AtLimit = machine.AtLimit with { Choice = AtLimit.Switch, At = 90, AtLabel = "When a limit reaches 90%" },
            Warm = TwoWarmed with { From = 420, To = 1260 },
            Schedule = machine.Schedule with { On = true },
        }));
        // The expander's repeater makes a card for each account on its next layout.
        await Views.Until(
            () => Views.Descendants(page).OfType<ToggleSwitch>().Any(on => AutomationProperties.GetAutomationId(on) == "settings.warm.claude/personal"),
            "the warm-up's accounts did not show");

        Assert.AreEqual(90.0, Views.Element<NumberBox>(page, "settings.atLimitShare").Value);
        Assert.AreEqual(2, Views.Element<ComboBox>(page, "settings.atLimit").SelectedIndex);
        Assert.IsTrue(Views.Element<ToggleSwitch>(page, "settings.warm.claude/personal").IsOn);
        Assert.AreEqual(new TimeSpan(7, 0, 0), Views.Element<TimePicker>(page, "settings.warmFrom").SelectedTime);
        Assert.AreEqual(sent, model.Sent.Count, string.Join(", ", model.Sent.Skip(sent)));
    }

    [UITestMethod]
    public async Task WarmUpListsEachAccountUnderItsExpander()
    {
        var model = new StandInModel(With(1, machine => machine with { Warm = TwoWarmed }));
        var page = await Shown(new AppModel(model));

        Assert.IsTrue(Shows(page, "settings.warmStanding"), "what comes next shows while it is closed");
        Assert.AreSame(Said(page, TwoWarmed.Note).Parent, Said(page, TwoWarmed.Standing!).Parent);
        Assert.AreEqual(TwoWarmed.RangeLabel, (Views.Element<SettingsExpander>(page, "settings.warm").Content as TextBlock)?.Text);
        await Expand(page, "settings.warm", "work");

        var texts = Views.Texts(page);
        Assert.Contains("Warms up again at 22:50", texts);
        Assert.Contains("Next warm-up: personal at 22:50.", texts);
        Assert.DoesNotContain("No Claude Code account is enrolled yet.", texts);
        Assert.IsTrue(Views.Element<ToggleSwitch>(page, "settings.warm.claude/personal").IsOn);

        Views.Element<ToggleSwitch>(page, "settings.warm.claude/work").IsOn = true;
        Assert.Contains(new Intent.SetWarm("claude/work", true), model.Sent);
    }

    [UITestMethod]
    public async Task WarmUpWithNoAccountSaysSoAndHoldsItsHoursUntilTheCoreHasSpoken()
    {
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))));

        await Expand(page, "settings.warm", "No Claude Code account is enrolled yet.");

        Assert.IsFalse(Views.Element<TimePicker>(page, "settings.warmFrom").IsEnabled);
        Assert.IsFalse(Views.Element<TimePicker>(page, "settings.warmTo").IsEnabled);
    }

    // A time picker holds no 24:00, so each end sends the model's own other end.
    [UITestMethod]
    public async Task EachEndOfTheHoursSendsTheModelsOtherEnd()
    {
        var model = new StandInModel(With(1, machine => machine with { Warm = TwoWarmed }));
        var page = await Shown(new AppModel(model));
        await Expand(page, "settings.warm", "personal · Max");
        var from = Views.Element<TimePicker>(page, "settings.warmFrom");
        var to = Views.Element<TimePicker>(page, "settings.warmTo");

        Assert.AreEqual(TimeSpan.Zero, to.SelectedTime);

        from.SelectedTime = new TimeSpan(7, 0, 0);
        Assert.Contains(new Intent.SetWarmRange(420, 1440), model.Sent);

        to.SelectedTime = new TimeSpan(21, 0, 0);
        Assert.Contains(new Intent.SetWarmRange(360, 1260), model.Sent);
    }

    // The toolkit's own sample gives a card in ItemsHeader or ItemsFooter the style of the expander's items.
    [UITestMethod]
    public async Task TheWarmUpsHoursAndLinkAreCardsAsItsAccountsAre()
    {
        var page = await Shown(new AppModel(new StandInModel(With(1, machine => machine with { Warm = TwoWarmed }))));
        await Expand(page, "settings.warm", "personal · Max");
        var account = Card(Views.Element<ToggleSwitch>(page, "settings.warm.claude/personal"));

        foreach (var (name, card) in new[]
        {
            ("From", Card(Views.Element<TimePicker>(page, "settings.warmFrom"))),
            ("To", Card(Views.Element<TimePicker>(page, "settings.warmTo"))),
            ("Is this allowed?", Card(Views.Element<HyperlinkButton>(page, "settings.warmTerms"))),
        })
        {
            Assert.AreEqual(account.Padding, card.Padding, name);
            Assert.AreEqual(account.MinHeight, card.MinHeight, name);
            Assert.AreEqual(account.BorderThickness, card.BorderThickness, name);
            Assert.AreEqual(account.CornerRadius, card.CornerRadius, name);
        }
    }

    // The app renews due logins as it reads them, and pitboard schedule status says where the task is.
    [UITestMethod]
    public async Task RenewalIsTheSwitchAlone()
    {
        var model = new StandInModel(With(1, machine => machine with
        {
            Schedule = machine.Schedule with { On = true },
        }));
        var page = await Shown(new AppModel(model));
        var texts = Views.Texts(page);

        Assert.DoesNotContain("Every day", texts);
        Assert.IsFalse(Views.Visible<Button>(page).Any(button => button.Content is "Renew now" or "Copy"));
        Assert.IsTrue(Views.Element<ToggleSwitch>(page, "settings.renewDaily").IsOn);

        Views.Element<ToggleSwitch>(page, "settings.renewDaily").IsOn = false;
        Assert.Contains(new Intent.SetSchedule(false), model.Sent);
    }

    [UITestMethod]
    public async Task RenewalWithNoSchedulerSaysWhy()
    {
        const string none = "This PC has no scheduler Pitboard knows how to write to.";
        var page = await Shown(new AppModel(new StandInModel(With(1, machine => machine with
        {
            Schedule = machine.Schedule with { Enabled = false, Note = none },
        }))));

        Assert.Contains(none, Views.Texts(page));
        Assert.IsFalse(Views.Element<ToggleSwitch>(page, "settings.renewDaily").IsEnabled);
    }

    // The installer owns the PATH entry, so the section offers no link even where the model would.
    [UITestMethod]
    public async Task TheCommandLineSaysWherePitboardIsAndOffersNoButton()
    {
        const string bundled = @"C:\Users\dana\AppData\Local\Programs\Pitboard\bin\pitboard.exe";
        var page = await Shown(new AppModel(new StandInModel(With(1, machine => machine with
        {
            CommandLine = new CommandLineShown(
                new FoundCommandLine.Bundled(bundled), bundled, "The one inside this app, so it updates with the app.", OffersLink: true,
                CannotLink: null, TerminalNote: null),
        }))));
        var texts = Views.Texts(page);

        Assert.Contains(bundled, texts);
        Assert.Contains("The one inside this app, so it updates with the app.", texts);
        Assert.AreEqual(Words.Settings.CommandLine, Card(Said(page, bundled)).Header);
        Assert.DoesNotContain(NewTerminals, texts);
        Assert.IsFalse(Views.Visible<Button>(page).Any(button => button.Content is string said && said.StartsWith("Install", StringComparison.Ordinal)));
    }

    [UITestMethod]
    public async Task ACommandLineFoundNowhereSaysWhoPutsItOnThePath()
    {
        var page = await Shown(new AppModel(new StandInModel(With(1, machine => machine with
        {
            CommandLine = new CommandLineShown(
                new FoundCommandLine.Nowhere(), "Not installed", null, OffersLink: true, CannotLink: null, TerminalNote: NewTerminals),
        }))));
        var texts = Views.Texts(page);

        Assert.Contains("Not installed", texts);
        Assert.Contains(NewTerminals, texts);
    }

    [UITestMethod]
    public async Task UntilItHasLookedTheCommandLineSectionWaits()
    {
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))));

        Assert.IsTrue(
            Views.Visible<ProgressRing>(page).Single(ring => AutomationProperties.GetName(ring) == Words.Settings.LookingForCommandLine).IsActive);
    }

    [UITestMethod]
    public async Task UpdatesSayWhenTheBuildCannotUpdate()
    {
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))));
        var texts = Views.Texts(page);

        Assert.IsTrue(texts.Any(text => text.StartsWith("This copy of Pitboard can", StringComparison.Ordinal)));
        Assert.Contains("Pitboard 0.0.0", texts);
        Assert.DoesNotContain(Words.Settings.BetaUpdates!, texts);
        Assert.IsFalse(Views.Element<Button>(page, "settings.checkNow").IsEnabled);
    }

    // As the macOS settings: automatic checks only where the build can update, and Check now beside them.
    [UITestMethod]
    public async Task AutomaticChecksShowWhileTheUpdaterIsOnAndTurnOff()
    {
        var updates = new StandInUpdates();
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))), updates: updates);
        var toggle = Views.Element<ToggleSwitch>(page, "settings.checkAutomatically");

        Assert.IsTrue(toggle.IsOn);
        toggle.IsOn = false;
        Assert.IsFalse(updates.ChecksAutomatically);

        Views.Press(Views.Element<Button>(page, "settings.checkNow"));
        Assert.AreEqual(1, updates.Checks);

        var off = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))));
        Assert.IsFalse(
            Views.Visible<ToggleSwitch>(off).Any(shown => AutomationProperties.GetAutomationId(shown) == "settings.checkAutomatically"),
            "a build that cannot update offers automatic checks");
    }

    [UITestMethod]
    public async Task AnUpdateFoundShowsAtOnce()
    {
        var updates = new StandInUpdates();
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))), updates: updates);

        updates.Found("1.0.1");

        Assert.Contains(PitboardFfiMethods.ReadyToInstall("1.0.1"), Views.Texts(page));
    }

    [UITestMethod]
    public async Task ABetaSaysHowItGetsTheNextBeta()
    {
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))), version: "1.0.0-beta.1");

        Assert.Contains(Words.Settings.BetaUpdates!, Views.Texts(page));
    }

    // Before 1.0.0 the feed of full releases is GitHub's 404, which Check now showed as WinSparkle's error.
    [UITestMethod]
    public async Task ABetaHoldsChecksBackUntilAFullReleaseHasAFeed()
    {
        var updates = new StandInUpdates(beta: true);
        updates.Feed(found: false);
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))), updates: updates, version: "1.0.0-beta.4");
        var checkNow = Views.Element<Button>(page, "settings.checkNow");

        Assert.IsFalse(checkNow.IsEnabled);
        Assert.Contains(Words.Settings.NoReleaseToUpdateTo!, Views.Texts(page));
        Assert.IsFalse(
            Views.Visible<ToggleSwitch>(page).Any(shown => AutomationProperties.GetAutomationId(shown) == "settings.checkAutomatically"),
            "automatic checks are offered");
        Assert.AreEqual(1, updates.Looks, "the visit did not look for the feed again");

        updates.Feed(found: true);

        Assert.IsTrue(checkNow.IsEnabled);
        await Views.Until(
            () => !Views.Texts(page).Contains(Words.Settings.NoReleaseToUpdateTo!)
                && Views.Visible<ToggleSwitch>(page).Any(shown => AutomationProperties.GetAutomationId(shown) == "settings.checkAutomatically"),
            "the page still holds checks back once a full release has a feed");
        Assert.Contains(Words.Settings.BetaUpdates!, Views.Texts(page));
    }

    [UITestMethod]
    public async Task AboutOpensOnItsVersionAndItsLinks()
    {
        var opened = new List<Uri>();
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))), version: "1.2.3", open: opened.Add);

        // Scrolled to first, as a person does: the expander's ItemsRepeater makes no item far below the view, where About
        // is on CI's smaller screens.
        var about = Views.Element<SettingsExpander>(page, "settings.about");
        about.StartBringIntoView(new BringIntoViewOptions { AnimationDesired = false });
        about.IsExpanded = true;
        await Views.Until(() => Views.Visible<HyperlinkButton>(page).Any(), "About did not open");

        Assert.Contains("1.2.3", Views.Texts(page));
        Assert.Contains(Words.Settings.Copyright, Views.Texts(page));
        Assert.AreEqual(Words.AppName, about.Header);
        Assert.AreEqual(Words.Menu.About, AutomationProperties.GetName(about));
        CollectionAssert.AreEqual(
            new[] { "usepitboard.com", Words.Settings.Help, Words.Settings.SourceCode, Words.Settings.ReportAProblem, Words.Settings.License },
            Views.Visible<HyperlinkButton>(page).Select(link => link.Content as string).ToArray());
        var report = Views.Visible<HyperlinkButton>(page).Single(link => link.Content as string == Words.Settings.ReportAProblem);
        new HyperlinkButtonAutomationPeer(report).Invoke();
        Assert.AreEqual(new Uri("https://github.com/datlechin/pitboard/issues"), opened.Single());

        // The window had no way to the help but the icon's menu.
        var help = Views.Visible<HyperlinkButton>(page).Single(link => link.Content as string == Words.Settings.Help);
        new HyperlinkButtonAutomationPeer(help).Invoke();
        Assert.AreEqual(new Uri("https://docs.usepitboard.com"), opened.Last());
    }

    // A switch sat about 100 epx in from its card's edge with "Off" after it. Windows Settings puts it at
    // the edge with its state before it, as the SettingsCard's own toggle style does.
    [UITestMethod]
    public async Task EachSwitchIsAtItsCardsRightEdgeWithItsStateBeforeIt()
    {
        var page = await Shown(
            new AppModel(new StandInModel(With(1, machine => machine with { Warm = TwoWarmed }))), updates: new StandInUpdates());
        await Expand(page, "settings.warm", "work");

        var toggles = Views.Visible<ToggleSwitch>(page).ToArray();
        Assert.HasCount(5, toggles, "start at sign-in, two warm-ups, daily renewal and automatic checks");
        foreach (var toggle in toggles)
        {
            var card = Card(toggle);
            var name = AutomationProperties.GetAutomationId(toggle);
            var track = Views.Descendants(toggle).OfType<Rectangle>().Single(shape => shape.Name == "OuterBorder");
            var state = Views.Descendants(toggle).OfType<ContentPresenter>()
                .Single(presenter => presenter.Name == (toggle.IsOn ? "OnContentPresenter" : "OffContentPresenter"));

            var edge = Bounds(card, page).Right - card.BorderThickness.Right - card.Padding.Right;
            Assert.AreEqual(edge, Bounds(track, page).Right, 0.5, $"{name} is not at its card's edge");
            Assert.IsLessThanOrEqualTo(Bounds(track, page).Left, Bounds(state, page).Right, $"{name} says its state after the switch");
        }
    }

    // Plan 4.5 and the approved mockup: At shows only where something happens at the share.
    [UITestMethod]
    public async Task TheShareHidesWhileNothingHappensAtIt()
    {
        var model = new StandInModel(Snapshots.Read(1));
        var app = new AppModel(model);
        var page = await Shown(app);
        await Expand(page, "settings.atLimitGroup", "Share of a limit");
        var card = Card(Views.Element<NumberBox>(page, "settings.atLimitShare"));

        Assert.AreEqual(Visibility.Visible, card.Visibility);

        app.Apply(With(2, machine => machine with
        {
            AtLimit = machine.AtLimit with { Choice = AtLimit.Nothing, ShareEnabled = false },
        }));

        Assert.AreEqual(Visibility.Collapsed, card.Visibility);
    }

    // The header repeated the share the box beside it held; the choice's name says it for Narrator.
    [UITestMethod]
    public async Task TheShareCardKeepsItsHeaderAsTheShareChanges()
    {
        var model = new StandInModel(Snapshots.Read(1));
        var app = new AppModel(model);
        var page = await Shown(app);
        await Expand(page, "settings.atLimitGroup", "Share of a limit");
        var share = Views.Element<NumberBox>(page, "settings.atLimitShare");
        var card = Card(share);

        app.Apply(With(2, machine => machine with
        {
            AtLimit = machine.AtLimit with { At = 90, AtLabel = "When a limit reaches 90%" },
        }));

        Assert.AreEqual("Share of a limit", card.Header);
        Assert.AreEqual("From 50% to 99%", card.Description);
        Assert.AreEqual("Share of a limit", AutomationProperties.GetName(share));
        Assert.AreEqual("When a limit reaches 90%", AutomationProperties.GetName(Views.Element<ComboBox>(page, "settings.atLimit")));
        Assert.AreEqual(90.0, share.Value);
    }

    // A failed change was said in the colour of the notes around it.
    [UITestMethod]
    public async Task AFailedChangeIsSaidInItsCardInTheCriticalColour()
    {
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))), new StartAtSignIn(new Refusing()));
        var toggle = Views.Element<ToggleSwitch>(page, "settings.startAtSignIn");

        toggle.IsOn = true;

        var failed = Said(page, "Access is denied.");
        var critical = ((SolidColorBrush)Application.Current.Resources["SystemFillColorCriticalBrush"]).Color;
        Assert.AreSame(Card(toggle), Card(failed));
        Assert.AreEqual(critical, ((SolidColorBrush)failed.Foreground).Color);
    }

    // A Debug build's own off would be said in the docs' pictures, so only a Release build's refusal is said.
    [UITestMethod]
    public async Task AReleaseBuildWhoseNotificationsAreOffSaysWhy()
    {
        var why = Words.Settings.NotificationsUnoffered!;
        var off = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))), notifications: () => new(why, InSettings: false));

        Assert.AreSame(Card(Said(off, why)), Views.Element<SettingsCard>(off, "settings.notifications"));
        Assert.IsFalse(
            Views.Visible<HyperlinkButton>(off).Any(link => AutomationProperties.GetAutomationId(link) == "settings.notificationSettings"),
            "a link to a switch that is not the person's");

        var on = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))));

        Assert.IsFalse(Views.Visible<SettingsCard>(on).Any(card => AutomationProperties.GetAutomationId(card) == "settings.notifications"));
    }

    // docs/reference/windows-app.mdx promised that Settings says why Windows shows none; only a failed registration was said.
    [UITestMethod]
    public async Task NotificationsTurnedOffInWindowsSettingsLinkThere()
    {
        var opened = new List<Uri>();
        NotificationsBlocked? blocked = new(Words.Settings.NotificationsOff, InSettings: true);
        var page = await Shown(new AppModel(new StandInModel(Snapshots.Read(1))), open: opened.Add, notifications: () => blocked);
        var card = Views.Element<SettingsCard>(page, "settings.notifications");
        var link = Views.Element<HyperlinkButton>(page, "settings.notificationSettings");

        Assert.AreEqual(Words.Settings.Notifications, card.Header);
        Assert.AreSame(card, Card(Said(page, Words.Settings.NotificationsOff)));
        Assert.AreSame(card, Card(link));
        Assert.AreEqual(Words.Settings.OpenNotificationSettings, link.Content);
        new HyperlinkButtonAutomationPeer(link).Invoke();
        Assert.AreEqual(new Uri("ms-settings:notifications"), opened.Single());

        blocked = null;
        page.ViewModel!.ReadWindowsSettings();

        Assert.AreEqual(Visibility.Collapsed, card.Visibility);
    }

    private sealed class Refusing : IStartupItem
    {
        public string? Refusal => null;

        public bool Read() => false;

        public bool TurnedOff() => false;

        public void Write(bool starts) => throw new UnauthorizedAccessException("Access is denied.");
    }
}
