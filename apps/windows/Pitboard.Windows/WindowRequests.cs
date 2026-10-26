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

    // None where the window stays as it was left: a request with no place, or one answered already.
    public Place? PlaceFor(WindowRequest request)
    {
        if (request.Serial <= _shown)
        {
            return null;
        }

        _shown = request.Serial;
        // The welcome has no window of its own yet, so it opens on the accounts.
        return request.Place is Place.Welcome ? new Place.Accounts() : request.Place;
    }
}
