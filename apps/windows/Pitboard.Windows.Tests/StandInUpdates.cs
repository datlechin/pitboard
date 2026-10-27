namespace Pitboard.Windows.Tests;

// An updater that is on, which a test sets as WinSparkle would.
internal sealed class StandInUpdates(string? waiting = null, bool beta = false) : IUpdates
{
    public event EventHandler? Changed;

    public bool Available { get; private set; } = true;

    public string? Refusal { get; private set; }

    public bool Beta => beta;

    public string? Waiting { get; private set; } = waiting;

    public bool ChecksAutomatically { get; set; } = true;

    public int Starts { get; private set; }

    public int Checks { get; private set; }

    public int Looks { get; private set; }

    public int Shutdowns { get; private set; }

    public void Start() => Starts++;

    public void Check() => Checks++;

    public void LookForFeed() => Looks++;

    public void Shutdown() => Shutdowns++;

    public void Found(string? version)
    {
        Waiting = version;
        Changed?.Invoke(this, EventArgs.Empty);
    }

    // As SparkleUpdates is for a beta once it has read the feed of full releases, or found none.
    public void Feed(bool found)
    {
        Available = found;
        Refusal = found ? null : Words.Settings.NoReleaseToUpdateTo;
        Changed?.Invoke(this, EventArgs.Empty);
    }
}
