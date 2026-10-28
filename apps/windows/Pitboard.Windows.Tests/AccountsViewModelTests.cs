using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class AccountsViewModelTests
{
    private static readonly Choice Add = new("Add account…", new Intent.PresentSheet(new Sheet.Add(Tool: null)), Enabled: true);

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

    // A tool's name heads its accounts once another tool's are shown, as one list of sections either way.
    [TestMethod]
    public void EachToolsAccountsAreASectionHeadedOnceAnotherToolsAreShown()
    {
        var one = Snapshots.Read(1) with
        {
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true), Items.Account("claude", "spare"))],
        };
        var (shown, _, app, _) = Over(one);
        string Said() => string.Join(" | ", shown.Sections.Select(section => $"{section.Heading}: {string.Join(", ", section.Select(row => row.Item.Title))}"));

        Assert.AreEqual(": work, spare", Said());
        Assert.IsFalse(shown.Sections.Single().Headed);

        app.Apply(one with
        {
            Revision = 2,
            Sections =
            [
                Items.Section("claude", "Claude Code", Items.Account("claude", "work", inUse: true)),
                Items.Section("codex", "Codex", Items.Account("codex", "main")),
            ],
        });

        Assert.AreEqual("Claude Code: work | Codex: main", Said());
        Assert.IsTrue(shown.Sections.All(section => section.Headed));
    }

    // The model makes every list anew for each snapshot: a row it still lists is the same object, open if it was opened.
    [TestMethod]
    public void AnOpenedRowStaysOpenAcrossSnapshots()
    {
        var first = Snapshots.Read(1) with
        {
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true), Items.Account("claude", "spare"))],
        };
        var (shown, _, app, _) = Over(first);
        var spare = shown.Sections.Single()[1];
        spare.Expanded = true;

        app.Apply(first with
        {
            Revision = 2,
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true), Items.Account("claude", "spare") with { Title = "spare 12%" })],
        });

        Assert.AreSame(spare, shown.Sections.Single()[1]);
        Assert.IsTrue(spare.Expanded);
        Assert.AreEqual("spare 12%", spare.Item.Title);
    }

    // Plan 4.9's expanded row: the account's windows, Rename and Remove account, in that order. Made again only when they
    // change, a button with focus keeps it through a read of the usage.
    [TestMethod]
    public void AnOpenedRowsCommandsAreMadeAgainOnlyWhenTheyChange()
    {
        var spare = Items.Account("claude", "spare") with
        {
            Limits = [Items.Limit(42)],
            Actions =
            [
                Items.Action(ActionKind.Switch, "Switch to spare", new Intent.SwitchTo(new AccountKey(Tool.Claude, "spare"))), Items.RenameAction("claude", "spare"),
                Items.WindowAction(Items.Window("claude", "spare")), Items.RemoveAction("claude", "spare"),
            ],
        };
        var first = Snapshots.Read(1) with { Sections = [Items.Section("claude", null, spare)] };
        var (shown, _, app, _) = Over(first);
        var row = shown.Sections.Single().Single();
        var commands = row.Commands.ToArray();
        var told = new List<string>();
        row.PropertyChanged += (_, args) => told.Add(args.PropertyName ?? "");

        Assert.AreEqual("detail.window.claude.ai, detail.rename, detail.remove", string.Join(", ", commands.Select(command => command.Id)));

        app.Apply(first with { Revision = 2, Sections = [Items.Section("claude", null, spare with { Limits = [Items.Limit(50)] })] });

        Assert.IsTrue(commands.SequenceEqual(row.Commands, ReferenceEqualityComparer.Instance), "a read made the buttons again");
        Assert.AreEqual(nameof(ShownAccount.Item), string.Join(", ", told));

        told.Clear();
        app.Apply(first with
        {
            Revision = 3,
            Sections = [Items.Section("claude", null, spare with { Actions = [.. spare.Actions.SkipLast(1), Items.RemoveAction("claude", "spare", enabled: false)] })],
        });

        Assert.IsFalse(row.Commands[^1].Action.Enabled);
        Assert.AreEqual($"{nameof(ShownAccount.HasCommands)}, {nameof(ShownAccount.Item)}", string.Join(", ", told));
    }

    // As the model tells its own parts: a read's end changes the refresh button and nothing else.
    [TestMethod]
    public void ASnapshotTellsOnlyWhatItChanged()
    {
        var (_, _, app, told) = Over(Snapshots.Read(1) with { Reading = true });

        app.Apply(Snapshots.Read(2) with { Changed = new(1, [Part.Reading]) });

        Assert.AreEqual(nameof(AccountsViewModel.CanRefresh), string.Join(", ", told));
    }

    [TestMethod]
    public void TheSetupStepIsShownWithWhatItIsFor()
    {
        var step = new SetupStep("Add a second account", "Pitboard switches once there are two.", [Add]);
        var (shown, _, app, told) = Over(Snapshots.Read(1));

        Assert.IsNull(shown.Setup);

        app.Apply(Snapshots.Read(2) with { Setup = step, Footing = new Footing.OnlyOne(Account: new AccountKey(Tool.Claude, "work")) });

        Assert.AreSame(step, shown.Setup);
        Assert.IsInstanceOfType<Footing.OnlyOne>(shown.SetupFooting);
        Assert.Contains(nameof(AccountsViewModel.Setup), told);
    }

    [TestMethod]
    public void ARowsActionIsSentAsTheModelGaveIt()
    {
        var (shown, model, _, _) = Over(Snapshots.Read(1) with
        {
            Sections = [Items.Section("claude", null, Items.Account("claude", "work", inUse: true), Items.Account("claude", "spare"))],
        });

        shown.Sections.Single()[1].Act();
        shown.Sections.Single()[0].Act();

        Assert.AreEqual(new Intent.SwitchTo(new AccountKey(Tool.Claude, "spare")), model.Sent.OfType<Intent.SwitchTo>().Single());
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
