using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class IconCommandsTests
{
    private static (IconCommands Commands, StandInModel Model, StandInUpdates Updates, StandInTargets Targets) Made()
    {
        var model = new StandInModel(Snapshots.Read(1));
        var updates = new StandInUpdates();
        var targets = new StandInTargets();
        return (new IconCommands(new AppModel(model), updates, targets), model, updates, targets);
    }

    [TestMethod]
    public void AClickOnTheIconOpensTheWindow()
    {
        var (commands, model, _, targets) = Made();

        commands.Heard(new IconEvent.Activated());

        CollectionAssert.AreEqual(new Intent[] { new Intent.Show(Place: null) }, model.Sent.ToArray());
        Assert.IsEmpty(targets.Did);
    }

    [TestMethod]
    public void ARightClickOpensTheMenuWhereItWasAndOpeningItIsAGlance()
    {
        var (commands, model, updates, targets) = Made();

        commands.Heard(new IconEvent.Menu(640, -12));
        var menu = targets.Menu!;
        menu.Opened();

        Assert.AreEqual(new IconEvent.Menu(640, -12), menu.At);
        CollectionAssert.AreEqual(
            IconMenu.Entries(new AppModel(new StandInModel(Snapshots.Read(1))), updates).Select(entry => entry.ToString()).ToArray(),
            menu.Entries.Select(entry => entry.ToString()).ToArray());
        CollectionAssert.AreEqual(new Intent[] { new Intent.Glanced() }, model.Sent.ToArray());
    }

    [TestMethod]
    public void AChoiceInTheMenuDoesWhatItSays()
    {
        var (commands, model, _, targets) = Made();
        commands.Heard(new IconEvent.Menu(0, 0));

        targets.Menu!.Chosen(new IconCommand.Send(new Intent.SwitchTo("claude/personal")));

        CollectionAssert.AreEqual(new Intent[] { new Intent.SwitchTo("claude/personal") }, model.Sent.ToArray());
    }

    [TestMethod]
    public async Task EachCommandReachesItsTarget()
    {
        var (commands, model, updates, targets) = Made();
        var window = Items.Window("codex", "main");

        await commands.Perform(new IconCommand.Send(new Intent.SwitchTo("claude/work")));
        await commands.Perform(new IconCommand.OpenLink(IconMenu.HelpLink));
        await commands.Perform(new IconCommand.OpenWindow(window));
        await commands.Perform(new IconCommand.ShowSettings());
        await commands.Perform(new IconCommand.CheckForUpdates());

        CollectionAssert.AreEqual(new Intent[] { new Intent.SwitchTo("claude/work") }, model.Sent.ToArray());
        Assert.AreEqual($"open link {IconMenu.HelpLink}, open window {window.Store}, show settings", string.Join(", ", targets.Did));
        Assert.AreEqual(1, updates.Checks);
    }

    // Exit stops the downloads still running, so it asks first.
    [TestMethod]
    public async Task ExitAsksWhetherPitboardMayQuitFirst()
    {
        var (commands, _, _, targets) = Made();

        targets.MayQuitAnswer = false;
        await commands.Perform(new IconCommand.Quit());
        targets.MayQuitAnswer = true;
        await commands.Perform(new IconCommand.Quit());

        Assert.AreEqual("may quit, may quit, quit", string.Join(", ", targets.Did));
    }

    internal sealed record ShownMenu(
        IReadOnlyList<IconMenuEntry> Entries, IconEvent.Menu At, Action Opened, Action<IconCommand> Chosen);

    internal sealed class StandInTargets : IIconTargets
    {
        public List<string> Did { get; } = [];

        public ShownMenu? Menu { get; private set; }

        public bool MayQuitAnswer { get; set; } = true;

        public void ShowMenu(IReadOnlyList<IconMenuEntry> entries, IconEvent.Menu at, Action opened, Action<IconCommand> chosen) =>
            Menu = new ShownMenu(entries, at, opened, chosen);

        public void OpenLink(string url) => Did.Add($"open link {url}");

        public void OpenWindow(WindowAccount window) => Did.Add($"open window {window.Store}");

        public void ShowSettings() => Did.Add("show settings");

        public Task<bool> MayQuit()
        {
            Did.Add("may quit");
            return Task.FromResult(MayQuitAnswer);
        }

        public void Quit() => Did.Add("quit");
    }
}
