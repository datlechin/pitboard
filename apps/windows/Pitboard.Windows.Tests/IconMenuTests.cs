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
        "An interrupted switch is waiting", null, "Pitboard can finish it or give it up.", null,
        new Intent.Show(new Place.Accounts()), null, true);

    private static AppModel Model(Func<Snapshot, Snapshot> change) => new(change(Snapshots.Read(1)));

    private static AppModel TwoTools(bool reading = false) => Model(snapshot => snapshot with
    {
        Reading = reading,
        Sections =
        [
            Items.Section("claude", "Claude Code", Items.Account("claude", "work", inUse: true), Items.Account("claude", "personal")),
            Items.Section("codex", "Codex", Items.Account("codex", "main", inUse: true)),
        ],
    });

    // One line per entry, by what UI Automation finds it by: a separator as "-", a heading as "# " and a note as "> " with
    // their words, an account as "( )" or "(x)".
    private static string Said(IEnumerable<IconMenuEntry> entries) => string.Join(" | ", entries.Select(entry => entry switch
    {
        IconMenuEntry.Separator => "-",
        IconMenuEntry.Label { Heading: true } label => "# " + label.Text,
        IconMenuEntry.Label label => "> " + label.Text,
        IconMenuEntry.Account account => (account.InUse ? "(x) " : "( ) ") + account.Id,
        IconMenuEntry.Command command => command.Id,
        IconMenuEntry.Submenu submenu => $"{submenu.Id} > {Said(submenu.Entries).Replace(" | ", ", ", StringComparison.Ordinal)}",
        _ => throw new AssertFailedException($"an entry of no kind: {entry}"),
    }));

    private static IconMenuEntry.Command Command(IEnumerable<IconMenuEntry> entries, string id) =>
        entries.OfType<IconMenuEntry.Command>().FirstOrDefault(command => command.Id == id)
        ?? throw new AssertFailedException($"no '{id}' in: {Said(entries)}");

    [TestMethod]
    public void TheMenuGoesInTheMacOSOrder()
    {
        var model = Model(snapshot => snapshot with
        {
            MenuNotices = new MenuNotices(Install, [Advice], Stuck),
            Sections =
            [
                Items.Section("claude", "Claude Code", Items.Account("claude", "work", inUse: true), Items.Account("claude", "personal")),
                Items.Section("codex", "Codex", Items.Account("codex", "main", inUse: true)),
            ],
        });

        var entries = IconMenu.Entries(model, new NoUpdates());

        Assert.AreEqual(
            "menu.install | menu.switch.0 | menu.others | - "
            + "| # Claude Code | (x) menu.account.claude/work | ( ) menu.account.claude/personal | - | # Codex | (x) menu.account.codex/main | - "
            + "| menu.addAccount | menu.refresh | - | menu.openPitboard | menu.settings | - | menu.help | menu.exit",
            Said(entries));
    }

    [TestMethod]
    public void EachToolsAccountsAreARadioGroupOfTheirOwn()
    {
        var accounts = IconMenu.Entries(TwoTools(), new NoUpdates()).OfType<IconMenuEntry.Account>().ToArray();

        Assert.AreEqual("claude, claude, codex", string.Join(", ", accounts.Select(account => account.Group)));
    }

    // A machine with one tool shows its accounts unheaded, as the macOS menu does.
    [TestMethod]
    public void OneToolsAccountsHaveNoHeading()
    {
        var model = Model(snapshot => snapshot with
        {
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true))],
        });

        Assert.AreEqual(
            "(x) menu.account.claude/work | - | menu.addAccount | menu.refresh | - | menu.openPitboard | menu.settings | - | menu.help | menu.exit",
            Said(IconMenu.Entries(model, new NoUpdates())));
    }

    [TestMethod]
    public void TheNoteStandsInForTheAccounts()
    {
        var model = Model(snapshot => snapshot with
        {
            MenuAccountsNote = "Couldn’t read usage",
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true))],
        });

        var entries = IconMenu.Entries(model, new NoUpdates());

        Assert.StartsWith("> Couldn’t read usage | - | menu.addAccount", Said(entries));
        Assert.IsEmpty(entries.OfType<IconMenuEntry.Account>());
    }

    // What macOS shows as a second line: an account's summary, a notice's subtitle, when the menu last read.
    [TestMethod]
    public void EachSecondLineIsTheEntrysDetail()
    {
        var model = Model(snapshot => snapshot with
        {
            MenuNotices = new MenuNotices(null, [Advice], Stuck),
            Sections =
            [
                Items.Section(
                    "claude", null,
                    Items.Account("claude", "work", inUse: true) with { Summary = "5h 42% · resets in 2h", Help = "In use by Claude Code" }),
            ],
        });

        var entries = IconMenu.Entries(model, new NoUpdates());

        Assert.AreEqual("5h 42% · resets in 2h\nIn use by Claude Code", entries.OfType<IconMenuEntry.Account>().Single().Detail);
        Assert.AreEqual("work is at 91%", Command(entries, "menu.switch.0").Detail);
        Assert.AreEqual("Pitboard can finish it or give it up.", Command(entries, "menu.others").Detail);
        Assert.AreEqual("Updated 08:00", Command(entries, "menu.refresh").Detail);
        Assert.IsNull(Command(entries, "menu.openPitboard").Detail);
    }

    [TestMethod]
    public void ChoosingAnAccountDoesWhatItsButtonDoes()
    {
        var expired = Items.Account("claude", "old") with
        {
            Action = new ItemAction("Sign in", "Sign in to old again", new Intent.PresentSheet(new Sheet.SignInAgain("claude", "old"))),
        };
        var unusable = Items.Account("claude", "broken") with { Action = null };
        var model = Model(snapshot => snapshot with
        {
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true), Items.Account("claude", "personal"), expired, unusable)],
        });

        var accounts = IconMenu.Entries(model, new NoUpdates()).OfType<IconMenuEntry.Account>().ToDictionary(account => account.Id!);

        Assert.AreEqual(new IconCommand.Send(new Intent.SwitchTo("claude/personal")), accounts["menu.account.claude/personal"].Does);
        Assert.AreEqual(
            new IconCommand.Send(new Intent.PresentSheet(new Sheet.SignInAgain("claude", "old"))), accounts["menu.account.claude/old"].Does);
        Assert.IsNull(accounts["menu.account.claude/work"].Does);
        Assert.IsTrue(accounts["menu.account.claude/work"].Enabled, "the account in use");
        Assert.IsFalse(accounts["menu.account.claude/broken"].Enabled, "an account with nothing to do");
    }

    [TestMethod]
    public void ANoticeSendsItsIntentOrOpensItsLink()
    {
        var held = Advice with { Enabled = false };
        var entries = IconMenu.Entries(Model(snapshot => snapshot with { MenuNotices = new MenuNotices(Install, [held], Stuck) }), new NoUpdates());

        Assert.AreEqual(new IconCommand.OpenLink("https://example.com/install"), Command(entries, "menu.install").Does);
        Assert.AreEqual(IconMark.Help, Command(entries, "menu.install").Mark);
        Assert.AreEqual(new IconCommand.Send(Advice.Intent!), Command(entries, "menu.switch.0").Does);
        Assert.AreEqual(IconMark.Warning, Command(entries, "menu.switch.0").Mark);
        Assert.IsFalse(Command(entries, "menu.switch.0").Enabled);
        Assert.AreEqual(new IconCommand.Send(new Intent.Show(new Place.Accounts())), Command(entries, "menu.others").Does);
        Assert.AreEqual(IconMark.Warning, Command(entries, "menu.others").Mark);
    }

    [TestMethod]
    public void AnAdviceWithoutASeverityShowsTheSwitchMark()
    {
        var entries = IconMenu.Entries(Model(snapshot => snapshot with { MenuNotices = new MenuNotices(null, [Advice with { Severity = null }], null) }), new NoUpdates());

        Assert.AreEqual(IconMark.Switch, Command(entries, "menu.switch.0").Mark);
    }

    [TestMethod]
    public void RefreshWaitsForTheReadUnderWay()
    {
        Assert.IsTrue(Command(IconMenu.Entries(TwoTools(), new NoUpdates()), "menu.refresh").Enabled);
        Assert.IsFalse(Command(IconMenu.Entries(TwoTools(reading: true), new NoUpdates()), "menu.refresh").Enabled);
    }

    [TestMethod]
    public void TheCommandsSendWhatTheMacOSMenuSends()
    {
        var entries = IconMenu.Entries(TwoTools(), new NoUpdates());

        Assert.AreEqual(new IconCommand.Send(new Intent.PresentSheet(new Sheet.Add(Provider: null))), Command(entries, "menu.addAccount").Does);
        Assert.AreEqual(new IconCommand.Send(new Intent.Refresh(Asked: true)), Command(entries, "menu.refresh").Does);
        Assert.AreEqual(new IconCommand.Send(new Intent.Show(Place: null)), Command(entries, "menu.openPitboard").Does);
        Assert.AreEqual(new IconCommand.ShowSettings(), Command(entries, "menu.settings").Does);
        Assert.AreEqual(new IconCommand.OpenLink("https://docs.usepitboard.com"), Command(entries, "menu.help").Does);
        Assert.AreEqual(new IconCommand.Quit(), Command(entries, "menu.exit").Does);
    }

    // Between Refresh and Open Pitboard, as the macOS menu has them.
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
            "menu.refresh | - | window.claude.ai/work | menu.site.chatgpt.com > window.chatgpt.com/main, window.chatgpt.com/spare | - "
            + "| menu.openPitboard",
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
    public void AWindowWhoseStoreIsHeldSaysWhyAndCannotBeChosen()
    {
        var work = Items.Window("claude", "work", inUse: true);
        var model = Model(snapshot => snapshot with
        {
            AccountWindows = snapshot.AccountWindows with
            {
                Accounts = [work],
                Menus = [new SiteMenu.One("Open claude.ai as work", work)],
                Held = [work.Store.ToUpperInvariant()],
            },
        });

        var entry = Command(IconMenu.Entries(model, new NoUpdates()), "window.claude.ai/work");

        Assert.IsFalse(entry.Enabled);
        Assert.AreEqual(
            "What this window kept before is deleted once every account window has closed. It opens after that.",
            entry.Detail);
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
        Assert.Contains("menu.openPitboard | menu.settings | menu.checkForUpdates | -", Said(entries));
    }

    // A beta finds no later beta, so the menu offers no check; it still says when 1.0.0 is found.
    [TestMethod]
    public void ABetaOffersNoCheckForUpdates()
    {
        Assert.DoesNotContain("menu.checkForUpdates", Said(IconMenu.Entries(TwoTools(), new StandInUpdates(beta: true))));
        var waiting = IconMenu.Entries(TwoTools(), new StandInUpdates(waiting: "1.0.0", beta: true));
        Assert.StartsWith("menu.update", Said(waiting));
        Assert.AreEqual("Install Pitboard 1.0.0…", Command(waiting, "menu.update").Text);
    }

    [TestMethod]
    public void AnUpdateWaitingIsOfferedFirst()
    {
        var entries = IconMenu.Entries(TwoTools(), new StandInUpdates(waiting: "1.2.0"));

        Assert.StartsWith("menu.update | - | # Claude Code", Said(entries));
        Assert.AreEqual("Install Pitboard 1.2.0…", Command(entries, "menu.update").Text);
        Assert.AreEqual(IconMark.Update, Command(entries, "menu.update").Mark);
    }
}
