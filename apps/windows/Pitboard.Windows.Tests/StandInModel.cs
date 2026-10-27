using Pitboard.Core;

namespace Pitboard.Windows.Tests;

// Hands out the snapshot a test gives it and keeps every intent, so nothing a test sends reaches a core.
internal sealed class StandInModel(Snapshot first) : IPitboardModel
{
    private readonly List<Intent> _sent = [];

    public IReadOnlyList<Intent> Sent
    {
        get
        {
            lock (_sent)
            {
                return [.. _sent];
            }
        }
    }

    // Called with each intent as it is sent, on the sender's thread.
    public Action<Intent>? Told { get; set; }

    public void Send(Intent intent)
    {
        lock (_sent)
        {
            _sent.Add(intent);
        }

        Told?.Invoke(intent);
    }

    public bool ShutDown { get; private set; }

    public void Shutdown() => ShutDown = true;

    public Snapshot Snapshot() => first;

    public string? Glance() => null;
}
