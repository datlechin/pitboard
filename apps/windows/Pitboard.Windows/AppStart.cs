using Pitboard.Core;

namespace Pitboard.Windows;

public sealed record AppStart(
    Identity Identity, Dictionary<string, string> Environment, string? Fixture, string DataFolder)
{
    public IReadOnlyList<string> Arguments { get; init; } = [];

    public string? Link => Launching.Link(Arguments, Identity.LinkScheme);

    // A Debug build outside a fixture takes no task, as it takes no link.
    public JumpAsk? Asked => Identity.OpensLinks(Fixture) ? Launching.Asked(Arguments) : null;

    // The core finds the command line in the bin beside exe, where the installer puts it.
    public AppLaunch Launch(string exe) => new(
        Environment,
        AppLocation: exe,
        Windows: new WindowsLaunch(
            DataFolder, PitboardFfiMethods.WindowsRecordsKey(PitboardFfiMethods.PitboardDirectory(Environment)), Identity.LinkScheme));
}
