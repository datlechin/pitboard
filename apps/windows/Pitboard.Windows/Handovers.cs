namespace Pitboard.Windows;

// AppInstance raises a redirected launch once, which can be before this copy can take it.
public sealed class Handovers
{
    private readonly Lock _lock = new();
    private readonly List<Handover> _held = [];
    private Action<Handover>? _take;

    // From AppInstance.Activated, on a thread of its own.
    public void Arrived(Handover handed)
    {
        lock (_lock)
        {
            if (_take is not null)
            {
                _take(handed);
            }
            // Every launch that asks only for the window, held while Pitboard starts, asks for the same window.
            else if (handed is not Handover.Window || !_held.Contains(handed))
            {
                _held.Add(handed);
            }
        }
    }

    // Under the lock, so the app takes each launch in the order it arrived.
    public void Ready(Action<Handover> take)
    {
        lock (_lock)
        {
            _take = take;
            foreach (var handed in _held)
            {
                take(handed);
            }

            _held.Clear();
        }
    }
}
