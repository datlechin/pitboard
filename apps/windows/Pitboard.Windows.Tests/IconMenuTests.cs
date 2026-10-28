using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class IconMenuTests
{
    private static readonly MenuEntry Install = new(
        "Claude Code isn’t installed", "Install it, then come back", null, null, null, "https://example.com/install", true);

    private static readonly MenuEntry Advice = new(
        "Switch Claude Code to personal", "work is at 91%", null, Severity.Warning,
        new Intent.SwitchTo("claude/personal"), null, true);

    private static readonly MenuEntry Stuck = new(
        "1 issue", "A switch didn’t finish", "Pitboard keeps trying until Anthropic answers.", Severity.Error,
        new Intent.Show(new Place.Accounts()), null, true);

    private static AppModel Model(Func<Snapshot, Snapshot> change) => new(change(Snapshots.Read(1)));

    private static AppModel TwoTools() => Model(snapshot => snapshot with
    {
        Sections =
        [
            Items.Section("claude", "Claude Code", Items.Account("claude", "work", inUse: true), Items.Account("claude", "personal")),
            Items.Section("codex", "Codex", Items.Account("codex", "main", inUse: true)),
        ],
    });

    // One line per entry, by what UI Automation finds it by: a separator as "-", a heading as "# " and its words, an
    // account as "( )" or "(x)".
    private static string Said(IEnumerable<IconMenuEntry> entries) => string.Join(" | ", entries.Select(entry => entry switch
    {
        IconMenuEntry.Separator => "-",
        IconMenuEntry.Heading heading => "# " + heading.Text,
        IconMenuEntry.Account account => (account.InUse ? "(x) " : "( ) ") + account.Id,
        IconMenuEntry.Command command => command.Id,
        IconMenuEntry.Submenu submenu => $"{submenu.Id} > {Said(submenu.Entries).Replace(" | ", ", ", StringComparison.Ordinal)}",
        _ => throw new AssertFailedException($"an entry of no kind: {entry}"),
    }));

    private static IconMenuEntry.Command Command(IEnumerable<IconMenuEntry> entries, string id) =>
        entries.OfType<IconMenuEntry.Command>().FirstOrDefault(command => command.Id == id)
        ?? throw new AssertFailedException($"no '{id}' in: {Said(entries)}");

    // Microsoft's notification area guidance: Open first and in bold, Exit last. What needs attention shows in the window
    // the icon's warning points to, and the window refreshes.
    [TestMethod]
    public void TheMenuGoesInMicrosoftsOrder()
    {
        var model = Model(snapshot => snapshot with
        {
            MenuNotices = new MenuNotices(Install, [Advice], [], Stuck),
            Sections =
            [
                Items.Section("claude", "Claude Code", Items.Account("claude", "work", inUse: true), Items.Account("claude", "personal")),
                Items.Section("codex", "Codex", Items.Account("codex", "main", inUse: true)),
            ],
        });

        var entries = IconMenu.Entries(model, new NoUpdates());

        Assert.AreEqual(
            "menu.openPitboard | - "
            + "| # Claude Code | (x) menu.account.claude/work | ( ) menu.account.claude/personal | # Codex | (x) menu.account.codex/main | - "
            + "| menu.addAccount | - | menu.settings | menu.help | - | menu.exit",
            Said(entries));
    }

    [TestMethod]
    public void EachItemSaysWhatTheModelSays()
    {
        var entries = IconMenu.Entries(TwoTools(), new StandInUpdates());

        foreach (var (id, words) in new[]
        {
            ("menu.openPitboard", Words.Menu.OpenPitboard),
            ("menu.addAccount", Words.Menu.AddAccount),
            ("menu.settings", Words.Menu.Settings),
            ("menu.checkForUpdates", Words.Menu.CheckForUpdates),
            ("menu.help", Words.Menu.Help),
            ("menu.exit", Words.Menu.Quit),
        })
        {
            Assert.AreEqual(words, Command(entries, id).Text, id);
        }
    }

    [TestMethod]
    public void EachToolsAccountsAreARadioGroupOfTheirOwn()
    {
        var accounts = IconMenu.Entries(TwoTools(), new NoUpdates()).OfType<IconMenuEntry.Account>().ToArray();

        Assert.AreEqual("claude, claude, codex", string.Join(", ", accounts.Select(account => account.Group)));
    }

    // A machine with one tool shows its accounts unheaded, as the window does.
    [TestMethod]
    public void OneToolsAccountsHaveNoHeading()
    {
        var model = Model(snapshot => snapshot with
        {
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true))],
        });

        Assert.AreEqual(
            "menu.openPitboard | - | (x) menu.account.claude/work | - | menu.addAccount | - | menu.settings | menu.help | - | menu.exit",
            Said(IconMenu.Entries(model, new NoUpdates())));
    }

    // "Reading accounts…" or "No accounts yet" would be an item nobody can choose.
    [TestMethod]
    public void NoAccountsLeaveTheirPlaceOut()
    {
        var model = Model(snapshot => snapshot with { MenuAccountsNote = "No accounts yet", Sections = [] });

        Assert.AreEqual(
            "menu.openPitboard | - | menu.addAccount | - | menu.settings | menu.help | - | menu.exit",
            Said(IconMenu.Entries(model, new NoUpdates())));
    }

    // Its name says how much it has used or why it can't be switched to; its tooltip says the rest.
    [TestMethod]
    public void AnAccountIsNamedAsTheModelNamesItInTheMenu()
    {
        var model = Model(snapshot => snapshot with
        {
            Sections =
            [
                Items.Section(
                    "claude", null,
                    Items.Account("claude", "work", inUse: true) with
                    {
                        MenuTitle = "work (42%)",
                        Summary = "5-hour 42% used, Weekly 12%",
                        Help = "Usage is from 5 minutes ago.",
                    }),
            ],
        });

        var account = IconMenu.Entries(model, new NoUpdates()).OfType<IconMenuEntry.Account>().Single();

        Assert.AreEqual("work (42%)", account.Text);
        Assert.AreEqual("5-hour 42% used, Weekly 12%\nUsage is from 5 minutes ago.", account.Detail);
    }

    [TestMethod]
    public void ChoosingAnAccountDoesWhatItsButtonDoes()
    {
        var expired = Items.Account("claude", "old") with
        {
            MenuTitle = "old (sign-in expired)",
            Action = new ItemAction("Sign in…", "Sign in to old again…", new Intent.PresentSheet(new Sheet.SignInAgain("claude", "old"))),
        };
        var switching = Items.Account("claude", "spare") with { MenuTitle = "spare (switching now)", Action = null };
        var model = Model(snapshot => snapshot with
        {
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true), Items.Account("claude", "personal"), expired, switching)],
        });

        var accounts = IconMenu.Entries(model, new NoUpdates()).OfType<IconMenuEntry.Account>().ToDictionary(account => account.Id!);

        Assert.AreEqual(new IconCommand.Send(new Intent.SwitchTo("claude/personal")), accounts["menu.account.claude/personal"].Does);
        Assert.AreEqual(
            new IconCommand.Send(new Intent.PresentSheet(new Sheet.SignInAgain("claude", "old"))), accounts["menu.account.claude/old"].Does);
        Assert.IsNull(accounts["menu.account.claude/work"].Does);
        Assert.IsTrue(accounts["menu.account.claude/work"].Enabled, "the account in use");
        // Each account stays in the list it makes, so the list keeps its place while a switch runs.
        Assert.IsFalse(accounts["menu.account.claude/spare"].Enabled, "an account with nothing to do now");
    }

    [TestMethod]
    public void TheCommandsSendWhatTheWindowsCommandsSend()
    {
        var entries = IconMenu.Entries(TwoTools(), new NoUpdates());

        Assert.AreEqual(new IconCommand.Send(new Intent.PresentSheet(new Sheet.Add(Provider: null))), Command(entries, "menu.addAccount").Does);
        Assert.AreEqual(new IconCommand.Send(new Intent.Show(Place: null)), Command(entries, "menu.openPitboard").Does);
        Assert.AreEqual(new IconCommand.ShowSettings(), Command(entries, "menu.settings").Does);
        Assert.AreEqual(new IconCommand.OpenLink("https://docs.usepitboard.com"), Command(entries, "menu.help").Does);
        Assert.AreEqual(new IconCommand.Quit(), Command(entries, "menu.exit").Does);
    }

    // After Add account, as the plan's menu has them.
    [TestMethod]
    public void EachSiteOffersItsAccountsWindows()
    {
        var work = Items.Window("claude", "work", inUse: true);
        var main = Items.Window("codex", "main");
        var spare = Items.Window("codex", "spare");
        var model = Model(snapshot => snapshot with
        {
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true))],
            AccountWindows = snapshot.AccountWindows with
            {
                Accounts = [work, main, spare],
                Menus =
                [
                    new SiteMenu.One("Open claude.ai as work", work),
                    new SiteMenu.Several("Open chatgpt.com", main.Site, [main, spare]),
                ],
            },
        });

        var entries = IconMenu.Entries(model, new NoUpdates());

        Assert.Contains(
            "menu.addAccount | - | window.claude.ai/work | menu.site.chatgpt.com > window.chatgpt.com/main, window.chatgpt.com/spare | - "
            + "| menu.settings",
            Said(entries));
        Assert.AreEqual(new IconCommand.OpenWindow(work), Command(entries, "window.claude.ai/work").Does);
        var chatGPT = entries.OfType<IconMenuEntry.Submenu>().Single();
        Assert.AreEqual(new IconCommand.OpenWindow(spare), ((IconMenuEntry.Command)chatGPT.Entries[1]).Does);
    }

    [TestMethod]
    public void NoAccountWithAWindowOffersNoSite()
    {
        Assert.DoesNotContain("window.", Said(IconMenu.Entries(TwoTools(), new NoUpdates())));
    }

    // Its old store is still being deleted, which WebView2 does only as its browser process exits.
    [TestMethod]
    public void AWindowWhoseStoreIsHeldIsLeftOutUntilItOpens()
    {
        var work = Items.Window("claude", "work", inUse: true);
        var main = Items.Window("codex", "main");
        var spare = Items.Window("codex", "spare");
        var model = Model(snapshot => snapshot with
        {
            AccountWindows = snapshot.AccountWindows with
            {
                Accounts = [work, main, spare],
                Menus =
                [
                    new SiteMenu.One("Open claude.ai as work", work),
                    new SiteMenu.Several("Open chatgpt.com", main.Site, [main, spare]),
                ],
                Held = [work.Store.ToUpperInvariant(), main.Store],
            },
        });

        Assert.Contains(
            "menu.addAccount | - | menu.site.chatgpt.com > window.chatgpt.com/spare | - | menu.settings",
            Said(IconMenu.Entries(model, new NoUpdates())));
    }

    [TestMethod]
    public void ASiteWhoseWindowsAreAllHeldIsLeftOut()
    {
        var main = Items.Window("codex", "main");
        var spare = Items.Window("codex", "spare");
        var model = Model(snapshot => snapshot with
        {
            AccountWindows = snapshot.AccountWindows with
            {
                Accounts = [main, spare],
                Menus = [new SiteMenu.Several("Open chatgpt.com", main.Site, [main, spare])],
                Held = [main.Store, spare.Store],
            },
        });

        Assert.DoesNotContain("menu.site.", Said(IconMenu.Entries(model, new NoUpdates())));
    }

    [TestMethod]
    public void OpenPitboardIsWhatAClickOnTheIconDoes()
    {
        var defaults = IconMenu.Entries(TwoTools(), new NoUpdates()).OfType<IconMenuEntry.Command>().Where(command => command.Default);

        Assert.AreEqual("menu.openPitboard", defaults.Single().Id);
    }

    [TestMethod]
    public void CheckForUpdatesShowsOnlyWhileTheUpdaterIsOn()
    {
        Assert.DoesNotContain("menu.checkForUpdates", Said(IconMenu.Entries(TwoTools(), new NoUpdates())));

        var entries = IconMenu.Entries(TwoTools(), new StandInUpdates(waiting: null));

        Assert.AreEqual(new IconCommand.CheckForUpdates(), Command(entries, "menu.checkForUpdates").Does);
        Assert.Contains("menu.settings | menu.checkForUpdates | menu.help | -", Said(entries));
    }

    // In Check for updates' place, which opens WinSparkle's dialog on it.
    [TestMethod]
    public void AnUpdateWaitingIsOfferedByItsVersion()
    {
        var entries = IconMenu.Entries(TwoTools(), new StandInUpdates(waiting: "1.2.0"));

        Assert.Contains("menu.settings | menu.update | menu.help | -", Said(entries));
        Assert.AreEqual(PitboardModelMethods.InstallUpdate("1.2.0"), Command(entries, "menu.update").Text);
        Assert.AreEqual(new IconCommand.CheckForUpdates(), Command(entries, "menu.update").Does);
    }

    // A beta finds no later beta, so the menu offers no check; it still says when 1.0.0 is found.
    [TestMethod]
    public void ABetaOffersNoCheckForUpdates()
    {
        Assert.DoesNotContain("menu.checkForUpdates", Said(IconMenu.Entries(TwoTools(), new StandInUpdates(beta: true))));
        var waiting = IconMenu.Entries(TwoTools(), new StandInUpdates(waiting: "1.0.0", beta: true));
        Assert.Contains("menu.settings | menu.update | menu.help", Said(waiting));
    }
}
