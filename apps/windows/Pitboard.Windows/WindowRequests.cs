using Pitboard.Core;

namespace Pitboard.Windows;

// As the macOS app's: a request answered is not answered again when the window opens another way.
public sealed class WindowRequests
{
    private ulong _opened;
    private ulong _shown;

    public bool Opens(WindowRequest request)
    {
        if (request.Serial <= _opened)
        {
            return false;
        }

        _opened = request.Serial;
        return true;
    }

    // Null is the settings page, which one long page scrolls to Limits for, as macOS opens its Limits pane.
    public (Pane? Pane, bool Limits) PaneFor(WindowRequest request, Pane? pane)
    {
        if (request.Serial <= _shown)
        {
            return (pane, false);
        }

        _shown = request.Serial;
        return request.Place switch
        {
            Place.Activity => (Pane.Activity, false),
            Place.Checks => (Pane.Checks, false),
            Place.Settings { Section: var section } => (null, section == SettingsSection.Limits),
            // The welcome has no window of its own yet, so it opens on the accounts.
            Place.Accounts or Place.Welcome => (Pane.Accounts, false),
            // None asks for the window as it was left.
            _ => (pane, false),
        };
    }
}
