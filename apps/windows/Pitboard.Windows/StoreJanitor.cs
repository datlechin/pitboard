using Pitboard.Core;

namespace Pitboard.Windows;

// What the janitor reads and asks of WebView2's profiles.
public interface IProfileFolders
{
    // Whether a page of this launch has shown its profile's folder where Exists looks, as the Runtime updates itself.
    bool LayoutKnown { get; }

    // The browser processes this launch has started, and whether one runs.
    int Starts { get; }

    bool Running { get; }

    bool Exists(string store);

    Task MarkDeleted(string store);

    // Starts a browser process, which deletes the folders of profiles deleted before it, and keeps it until gone says so,
    // or a while.
    Task Sweep(Func<bool> gone);
}

// Which stores go, and when, is the model's, in AccountWindowsShown.Deleting. A deleted profile's folder outlived its
// browser process on both runners and went as the next one started (docs/facts/windows-runners.md), so a store is held
// until then, and the model asks again after its next read: once marked, it is swept by a new browser process when none
// runs, and marked again if a start since left it.
public sealed class StoreJanitor(IProfileFolders profiles)
{
    // Each ask once, however many snapshots list it before the model has heard how it went.
    private readonly HashSet<StoreDeletion> _taken = [];

    // The start each store was marked in; a store held at an earlier launch was marked before this launch's first.
    private readonly Dictionary<string, int> _marked = new(StringComparer.OrdinalIgnoreCase);

    public void Delete(IReadOnlyList<StoreDeletion> asked, IReadOnlyCollection<string> held, Action<string, bool> told)
    {
        _taken.IntersectWith(asked);
        foreach (var ask in asked)
        {
            if (_taken.Add(ask))
            {
                _ = Answer(ask.Store, held.Contains(ask.Store, StringComparer.OrdinalIgnoreCase), told);
            }
        }
    }

    private async Task Answer(string store, bool heldBefore, Action<string, bool> told)
    {
        // A folder that looks absent counts only once a page has shown where folders are.
        bool There() => !profiles.LayoutKnown || profiles.Exists(store);
        if (There())
        {
            try
            {
                await Tidy(store, heldBefore, There);
            }
            // Told as held all the same, or the model would wait on this ask for good.
            catch (Exception)
            {
            }
        }

        told(store, !There());
    }

    private async Task Tidy(string store, bool heldBefore, Func<bool> there)
    {
        if (!_marked.TryGetValue(store, out var at))
        {
            if (!heldBefore)
            {
                // Until the mark takes, every start is after it.
                _marked[store] = -1;
                await profiles.MarkDeleted(store);
                _marked[store] = profiles.Starts;
                return;
            }

            _marked[store] = at = 0;
        }

        if (profiles.Starts > at)
        {
            await profiles.MarkDeleted(store);
            _marked[store] = profiles.Starts;
        }
        else if (!profiles.Running)
        {
            await profiles.Sweep(() => !there());
        }
    }
}

// The app's WebView2 profiles as the janitor reads them, its own pages made in a window of the app's.
public sealed class ProfileFolders(WebEnvironment web, nint parent) : IProfileFolders
{
    public bool LayoutKnown => web.LayoutKnown;

    public int Starts => web.Starts;

    public bool Running => web.Running;

    public bool Exists(string store) => Directory.Exists(web.ProfileFolder(store));

    public Task MarkDeleted(string store) => web.MarkDeleted(store, parent);

    public Task Sweep(Func<bool> gone) => web.Sweep(parent, gone);
}
