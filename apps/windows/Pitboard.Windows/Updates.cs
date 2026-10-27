using Pitboard.Core;

namespace Pitboard.Windows;

// The updater, as the macOS app's Updates: one that cannot update says why and offers nothing.
public interface IUpdates
{
    // Raised on the app's thread when Available, Refusal or Waiting changes.
    event EventHandler? Changed;

    bool Available { get; }

    string? Refusal { get; }

    // A beta finds no later beta, only the next full release.
    bool Beta { get; }

    string? Waiting { get; }

    bool ChecksAutomatically { get; set; }

    // Once Pitboard's icon is in the notification area.
    void Start();

    void Check();

    // Each settings visit: a beta that found no full release with a feed reads it again.
    void LookForFeed();

    void Shutdown();
}

public sealed class NoUpdates(string? refusal = null) : IUpdates
{
    public event EventHandler? Changed
    {
        add { }
        remove { }
    }

    public bool Available => false;

    public string? Refusal => refusal ?? PitboardFfiMethods.NoUpdateKey();

    public bool Beta => false;

    public string? Waiting => null;

    public bool ChecksAutomatically
    {
        get => false;
        set { }
    }

    public void Start()
    {
    }

    public void Check()
    {
    }

    public void LookForFeed()
    {
    }

    public void Shutdown()
    {
    }
}
