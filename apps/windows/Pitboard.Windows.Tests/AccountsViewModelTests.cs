using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class AccountsViewModelTests
{
    private static readonly Choice Add = new("Add account…", new Intent.PresentSheet(new Sheet.Add(Provider: null)), Enabled: true);

    private static (AccountsViewModel Shown, StandInModel Model, AppModel App, List<string> Told) Over(
        Snapshot snapshot, Action<Uri>? open = null)
    {
        var model = new StandInModel(snapshot);
        var app = new AppModel(model);
        var shown = new AccountsViewModel(app, open ?? (_ => { }));
        shown.Listen();
        var told = new List<string>();
        shown.PropertyChanged += (_, args) => told.Add(args.PropertyName ?? "");
        return (shown, model, app, told);
    }

    [TestMethod]
    public void AnEmptyMachineIsOfferedAnAccount()
    {
        var (shown, model, _, _) = Over(Snapshots.Read(1) with
        {
            AccountsShown = new AccountsShown.NoAccounts("No accounts", "Sign in to add one.", Add),
        });

        Assert.IsFalse(shown.Listed);
        Assert.IsTrue(shown.ShowsEmpty);
        Assert.AreEqual(
            new EmptyShown(EmptyKind.NoAccounts, "No accounts", "Sign in to add one.", "Add account…", Prominent: true, Enabled: true),
            shown.Empty);

        shown.ChooseEmpty();

        Assert.Contains(Add.Intent, model.Sent);
    }

    [TestMethod]
    public void AToolNotInstalledOffersWhereToGetIt()
    {
        var opened = new List<Uri>();
        var (shown, model, _, _) = Over(
            Snapshots.Read(1) with
            {
                AccountsShown = new AccountsShown.NoTool("Claude Code isn’t installed", "Install it.", "Get Claude Code", "https://claude.com/claude-code"),
            },
            opened.Add);

        Assert.AreEqual(EmptyKind.NoTool, shown.Empty.Kind);
        Assert.IsTrue(shown.Empty.Prominent);

        shown.ChooseEmpty();

        Assert.AreEqual(new Uri("https://claude.com/claude-code"), opened.Single());
        Assert.IsEmpty(model.Sent.OfType<Intent.PresentSheet>());
    }

    [TestMethod]
    public void AFailedReadOffersItsRetryWithoutTheAccent()
    {
        var retry = new Choice("Try again", new Intent.Refresh(Asked: true), Enabled: false);
        var (shown, _, _, _) = Over(Snapshots.Read(1) with
        {
            AccountsShown = new AccountsShown.ReadFailed("Couldn’t read accounts", "The keychain is locked.", retry),
        });

        Assert.AreEqual(
            new EmptyShown(EmptyKind.ReadFailed, "Couldn’t read accounts", "The keychain is locked.", "Try again", Prominent: false, Enabled: false),
            shown.Empty);
    }

    [TestMethod]
    public void WhileReadingNothingIsOffered()
    {
        var (shown, _, _, _) = Over(Snapshots.Read(1) with { AccountsShown = new AccountsShown.Reading("Reading accounts…") });

        Assert.AreEqual(new EmptyShown(EmptyKind.Reading, "Reading accounts…", "", "", Prominent: false, Enabled: false), shown.Empty);
    }

    [TestMethod]
    public void AListShowsNoEmptyState()
    {
        var (shown, _, _, _) = Over(Snapshots.Read(1));

        Assert.IsTrue(shown.Listed);
        Assert.IsFalse(shown.ShowsEmpty);
        Assert.AreSame(EmptyShown.None, shown.Empty);
    }

    // An empty group header still takes room, so one tool's accounts are rows with no group.
    [TestMethod]
    public void AccountsAreGroupedOnlyOnceAToolsHeadingIsShown()
    {
        var one = Snapshots.Read(1) with
        {
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true), Items.Account("claude", "spare"))],
        };
        var (shown, _, app, told) = Over(one);

        Assert.IsFalse(shown.Grouped);
        Assert.AreEqual("work, spare", string.Join(", ", shown.Rows.Select(row => row.Item.Title)));
        Assert.IsEmpty(shown.Sections);

        app.Apply(one with
        {
            Revision = 2,
            ShowsTools = true,
            Sections =
            [
                Items.Section("claude", "Claude Code", Items.Account("claude", "work", inUse: true)),
                Items.Section("codex", "Codex", Items.Account("codex", "main")),
            ],
        });

        Assert.IsTrue(shown.Grouped);
        Assert.IsEmpty(shown.Rows);
        Assert.AreEqual("Claude Code, Codex", string.Join(", ", shown.Sections.Select(section => section.Heading)));
        Assert.Contains(nameof(AccountsViewModel.Grouped), told);
    }

    // As the model tells its own parts: a read's end changes the refresh button and nothing else.
    [TestMethod]
    public void ASnapshotTellsOnlyWhatItChanged()
    {
        var (_, _, app, told) = Over(Snapshots.Read(1) with { Reading = true });

        app.Apply(Snapshots.Read(2));

        Assert.AreEqual(nameof(AccountsViewModel.CanRefresh), string.Join(", ", told));
    }

    [TestMethod]
    public void TheSetupStepIsShownWithWhatItIsFor()
    {
        var step = new SetupStep("Name this account", "Pitboard switches between named accounts.", [Add]);
        var (shown, _, app, told) = Over(Snapshots.Read(1));

        Assert.IsFalse(shown.HasSetup);

        app.Apply(Snapshots.Read(2) with { Setup = step, Footing = new Footing.Unnamed(Provider: "claude", Email: "dana@example.com") });

        Assert.IsTrue(shown.HasSetup);
        Assert.AreEqual("Name this account", shown.SetupTitle);
        Assert.AreEqual("Pitboard switches between named accounts.", shown.SetupDetail);
        Assert.IsInstanceOfType<Footing.Unnamed>(shown.SetupFooting);
        Assert.Contains(nameof(AccountsViewModel.HasSetup), told);
    }

    // Windows' own settings page, which only Windows opens.
    [TestMethod]
    public void TheSetupStepsLinkOpensWhereItPoints()
    {
        var opened = new List<Uri>();
        var step = new SetupStep(
            "Start when you sign in to Windows", "Its icon starts behind the arrow.", [Add], new SetupLink("Taskbar settings", "ms-settings:taskbar"));
        var (shown, _, _, _) = Over(Snapshots.Read(1) with { Setup = step, Footing = new Footing.NotOpenedAtLogin() }, opened.Add);

        shown.OpenSetupLink();

        Assert.AreEqual(new Uri("ms-settings:taskbar"), opened.Single());
    }

    [TestMethod]
    public void ARowsActionIsSentAsTheModelGaveIt()
    {
        var (shown, model, _, _) = Over(Snapshots.Read(1) with
        {
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true), Items.Account("claude", "spare"))],
        });

        shown.Rows[1].Act();
        shown.Rows[0].Act();

        Assert.AreEqual(new Intent.SwitchTo("claude/spare"), model.Sent.OfType<Intent.SwitchTo>().Single());
    }

    // Unloaded, the page hears nothing; shown again, it is told everything at once.
    [TestMethod]
    public void AStoppedViewModelHearsNothingUntilItListensAgain()
    {
        var (shown, _, app, told) = Over(Snapshots.Read(1) with { Reading = true });
        shown.Stop();

        app.Apply(Snapshots.Read(2));
        Assert.IsEmpty(told);

        shown.Listen();
        Assert.AreEqual("", told.Single(), "all at once");
        Assert.IsTrue(shown.CanRefresh);
    }

    [TestMethod]
    public void PropertyChangedIsRaisedWithNamesAPageCanBind()
    {
        var names = typeof(AccountsViewModel).GetProperties().Select(property => property.Name).ToHashSet();
        var (_, _, app, told) = Over(Snapshots.Read(1));

        app.Apply(Snapshots.Read(2) with
        {
            Reading = true,
            UpdatedWindow = "Updated 09:00",
            AccountsShown = new AccountsShown.Reading("Reading accounts…"),
            Notices = [Items.Notice("switch/codex", Items.Dismiss("codex"))],
        });

        Assert.IsNotEmpty(told);
        Assert.IsEmpty(told.Where(name => !names.Contains(name)), string.Join(", ", told));
    }
}
