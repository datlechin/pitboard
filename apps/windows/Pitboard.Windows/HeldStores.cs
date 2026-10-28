using Pitboard.Core;

namespace Pitboard.Windows;

// A store whose deletion waits for WebView2's browser process to exit opens no window until then, and the entry that
// would open it says why.
public static class HeldStores
{
    public static string? Note(WindowAccount window, IReadOnlyCollection<string> held) =>
        held.Contains(window.Store, StringComparer.OrdinalIgnoreCase)
            ? PitboardModelMethods.WindowNote(new WindowNoteKind.StoreHeld(), window)
            : null;
}
