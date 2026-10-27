using System.Runtime.InteropServices;
using Microsoft.Web.WebView2.Core;
using Pitboard.Core;

namespace Pitboard.Windows;

// Every window's downloads, as the macOS app's DownloadCenter keeps them: each saved under the name
// download_destination gives, never over a file or another download's name, asked about first where the policy says,
// and told to the model from its start to its end. WebView2's own download dialog lists them.
public sealed class Downloads
{
    private readonly string _folder;
    private readonly Action<Intent> _send;
    private readonly List<Running> _live = [];

    // A download runs in the browser process of the windows' pages, and goes with it.
    public Downloads(WebEnvironment web, Action<Intent> send)
    {
        _folder = web.Downloads;
        _send = send;
        web.BrowserExited += () => Stop(cancel: false);
    }

    public event Action? Ended;

    // Counted here rather than from a snapshot: a download started a moment before Quit is under way before the
    // snapshot that lists it.
    public Question? QuitQuestion => PitboardFfiMethods.DownloadsQuitQuestion((uint)_live.Count);

    // The stores of the windows whose downloads are under way, the newest first.
    public IEnumerable<string> Stores => _live.Select(running => running.Store).Reverse();

    public bool UnderWay(string store) =>
        _live.Any(running => string.Equals(running.Store, store, StringComparison.OrdinalIgnoreCase));

    // Holds WebView2 until it is answered, so nothing is saved before the person says yes. Pitboard may quit meanwhile.
    public async Task Start(
        string store, CoreWebView2DownloadStartingEventArgs args, bool ask, Func<Question, Task<bool>> allow)
    {
        var deferral = args.GetDeferral();
        var operation = args.DownloadOperation;
        var name = Path.GetFileName(args.ResultFilePath);
        var running = new Running(Guid.NewGuid().ToString(), store, operation);
        _live.Add(running);
        _send(new Intent.DownloadStarted(running.Id, store, name));
        var allowed = !ask || await allow(PitboardFfiMethods.DownloadQuestion(name, PitboardFfiMethods.DownloadHost(operation.Uri)));
        var file = allowed && _live.Contains(running)
            ? PitboardFfiMethods.DownloadDestination(_folder, name, [.. _live.Select(other => other.File).OfType<string>()])
            : null;
        try
        {
            if (file is null)
            {
                args.Cancel = true;
            }
            else
            {
                args.ResultFilePath = file;
                operation.StateChanged += (sender, _) => Changed(running, sender);
            }

            deferral.Complete();
        }
        catch (Exception closed) when (closed is COMException or InvalidOperationException)
        {
            // Its page closed while the question was up, and took the download with it.
            file = null;
        }

        if (file is null)
        {
            End(running, new DownloadEnd.Cancelled());
            return;
        }

        running.File = file;
        _send(new Intent.DownloadSaving(running.Id, file));
    }

    // Quitting stops every download, and none resumes.
    public void StopAll() => Stop(cancel: true);

    // A paused download is interrupted and can resume, and is still under way.
    private void Changed(Running running, CoreWebView2DownloadOperation operation)
    {
        switch (operation.State)
        {
            case CoreWebView2DownloadState.Completed:
                End(running, new DownloadEnd.Finished());
                break;
            case CoreWebView2DownloadState.Interrupted when !operation.CanResume:
                End(running, operation.InterruptReason is CoreWebView2DownloadInterruptReason.UserCanceled
                    or CoreWebView2DownloadInterruptReason.UserShutdown
                    ? new DownloadEnd.Cancelled()
                    : new DownloadEnd.Failed(operation.InterruptReason.ToString()));
                break;
        }
    }

    private void Stop(bool cancel)
    {
        foreach (var running in _live.ToArray())
        {
            try
            {
                if (cancel)
                {
                    running.Operation.Cancel();
                }
            }
            catch (Exception gone) when (gone is COMException or InvalidOperationException)
            {
                // Its browser process has gone, and the download with it.
            }

            End(running, new DownloadEnd.Cancelled());
        }
    }

    private void End(Running running, DownloadEnd end)
    {
        if (_live.Remove(running))
        {
            _send(new Intent.DownloadEnded(running.Id, end));
            Ended?.Invoke();
        }
    }

    private sealed class Running(string id, string store, CoreWebView2DownloadOperation operation)
    {
        public string Id => id;

        public string Store => store;

        public CoreWebView2DownloadOperation Operation => operation;

        public string? File { get; set; }
    }
}
