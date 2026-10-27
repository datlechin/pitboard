using CommunityToolkit.WinUI.Controls;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.VisualStudio.TestTools.UnitTesting.AppContainer;
using Pitboard.Core;
using Pitboard.Views;

namespace Pitboard.Windows.Tests;

// The welcome over what the model says of it, with no model: it sends what a person chose, and opens no Settings page.
[TestClass]
public sealed class WelcomeWindowTests
{
    private const string Says = "Pitboard switches Claude Code and Codex between your own accounts, and shows how much of each limit you’ve used.";

    private const string WhereItLives = "Pitboard’s icon is in the notification area. Select ^ to find it.";

    private static readonly WelcomeTool Added = new(
        "claude", "Claude Code", "dana@work.example", new WelcomeStep.Added("Added as work"));

    private static readonly WelcomeTool Codex = new(
        "codex", "Codex", null, new WelcomeStep.Add(new Choice("Add", new Intent.Enrol("codex"), Enabled: true)));

    private static Snapshot Welcoming(params WelcomeTool[] tools) =>
        Snapshots.Read(1) with
        {
            Welcome = new WelcomeShown(
                Title: "Welcome to Pitboard",
                Lines: [Says, WhereItLives],
                IconLink: new SetupLink("Taskbar settings", "ms-settings:taskbar"),
                ToolsHeading: "Found on this PC",
                Tools: tools,
                OpenAtLogin: new WelcomeLogin("Start when you sign in to Windows", On: false, Note: "Pitboard watches your limits only while it’s running."),
                Done: new Choice("Get started", new Intent.Show(new Place.Accounts()), Enabled: true)),
        };

    private static SettingsCard Card(WelcomeWindow window, string code) =>
        Views.Element<SettingsCard>(window.Content, $"welcome.tool.{code}");

    [UITestMethod]
    public async Task ItSaysWhatPitboardDoesAndWhereItsIconIs()
    {
        var opened = new List<Uri>();
        var window = await Views.Activated(new WelcomeWindow(
            new AppModel(new StandInModel(Welcoming(Added))), new StartAtSignIn(new FixtureStartupItem()), opened.Add));
        try
        {
            Assert.AreEqual("Welcome to Pitboard", window.Title);
            var texts = Views.Texts(window.Content);
            foreach (var said in new[] { "Welcome to Pitboard", Says, WhereItLives, "Found on this PC", "Start when you sign in to Windows" })
            {
                Assert.Contains(said, texts);
            }

            var link = Views.Element<HyperlinkButton>(window.Content, "welcome.iconLink");
            Assert.AreEqual("Taskbar settings", link.Content);
            new HyperlinkButtonAutomationPeer(link).Invoke();
            CollectionAssert.AreEqual(new[] { new Uri("ms-settings:taskbar") }, opened);
        }
        finally
        {
            window.Close();
        }
    }

    // Plan 4.12, and #282's rule: a tool's login is read only once its Add is clicked.
    [UITestMethod]
    public async Task EachToolFoundOffersWhatItsAddIs()
    {
        var model = new StandInModel(Welcoming(Added, Codex));
        var app = new AppModel(model);
        var opened = new List<Uri>();
        var window = await Views.Activated(new WelcomeWindow(app, new StartAtSignIn(new FixtureStartupItem()), opened.Add));
        try
        {
            await Views.Until(() => Views.Texts(window.Content).Contains("Codex"), "the tools were not listed");
            var claude = Card(window, "claude");
            Assert.AreEqual("Claude Code, Codex", $"{AutomationProperties.GetName(claude)}, {AutomationProperties.GetName(Card(window, "codex"))}");
            Assert.Contains("dana@work.example", Views.Texts(claude));
            Assert.Contains("Added as work", Views.Texts(claude));
            Assert.IsFalse(Views.Visible<Button>(claude).Any(), "Add for a tool added already");

            Views.Press(Views.Button(Card(window, "codex"), "Add"));
            CollectionAssert.AreEqual(new Intent[] { new Intent.Enrol("codex") }, model.Sent.ToArray());

            var install = new SetupLink("How to install Claude Code", "https://code.claude.com/docs/en/setup");
            app.Apply(Welcoming(new WelcomeTool("claude", "Claude Code", null, new WelcomeStep.Install(install))) with { Revision = 2 });
            await Views.Until(() => Views.Texts(window.Content).Contains("How to install Claude Code"), "no install link");
            new HyperlinkButtonAutomationPeer(Views.Visible<HyperlinkButton>(Card(window, "claude")).Single()).Invoke();

            CollectionAssert.AreEqual(new[] { new Uri(install.Link) }, opened);
        }
        finally
        {
            window.Close();
        }
    }

    // The brief: off until it is turned on, as Windows has it, and turned on here as in Settings.
    [UITestMethod]
    public async Task StartAtSignInIsOffUntilTurnedOn()
    {
        var start = new StartAtSignIn(new FixtureStartupItem());
        var window = await Views.Activated(new WelcomeWindow(new AppModel(new StandInModel(Welcoming(Added))), start, _ => { }));
        try
        {
            var toggle = Views.Element<ToggleSwitch>(window.Content, "welcome.startAtSignIn");
            Assert.IsFalse(toggle.IsOn);
            Assert.IsTrue(toggle.IsEnabled);

            toggle.IsOn = true;

            Assert.IsTrue(start.On);
        }
        finally
        {
            window.Close();
        }
    }

    [UITestMethod]
    public async Task GetStartedOpensTheAccountsAndClosesTheWelcome()
    {
        var model = new StandInModel(Welcoming(Added));
        var window = await Views.Activated(new WelcomeWindow(new AppModel(model), new StartAtSignIn(new FixtureStartupItem()), _ => { }));
        var closed = false;
        window.Closed += (_, _) => closed = true;
        try
        {
            var done = Views.Element<Button>(window.Content, "welcome.done");
            Assert.AreEqual("Get started", done.Content);
            Assert.AreEqual(Application.Current.Resources["AccentButtonStyle"], done.Style);
            Assert.AreEqual("G", done.AccessKey);
            // A window has no default button: Enter presses Get started where the focus starts on it.
            await Views.Until(() => FocusManager.GetFocusedElement(done.XamlRoot) == done, "the welcome opened with the focus elsewhere");

            Views.Press(done);

            CollectionAssert.AreEqual(new Intent[] { new Intent.Show(new Place.Accounts()) }, model.Sent.ToArray());
            Assert.IsTrue(closed, "the welcome stayed open");
        }
        finally
        {
            if (!closed)
            {
                window.Close();
            }
        }
    }
}
