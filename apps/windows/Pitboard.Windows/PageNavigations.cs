using Microsoft.Web.WebView2.Core;

namespace Pitboard.Windows;

// The navigations of a page and its frames, by WebView2's id, for telling whose a download is: WebView2 names no frame
// in DownloadStarting. On both runners a navigation that became a download completed first, unsuccessful and
// ConnectionAborted, and DownloadStarting followed (docs/facts/windows-runners.md). So a navigation counts while it is
// under way, and after it completed so, until the page next navigates. A download nothing accounts for counts as a
// frame's, so it is asked about more often than on macOS, never less.
public sealed class PageNavigations
{
    private readonly Dictionary<ulong, string> _page = [];
    private readonly Dictionary<ulong, string> _frames = [];
    private readonly HashSet<string> _pageAborted = [];
    private readonly HashSet<string> _framesAborted = [];

    // A redirect starts again under the same id. A page's own navigation leaves its frames, and theirs.
    public void Starting(ulong id, string address, bool frame, bool stopped)
    {
        if (!frame)
        {
            _pageAborted.Clear();
            _framesAborted.Clear();
        }

        var under = frame ? _frames : _page;
        if (stopped)
        {
            under.Remove(id);
        }
        else
        {
            under[id] = address;
        }
    }

    public void Completed(ulong id, bool frame, bool succeeded, CoreWebView2WebErrorStatus status)
    {
        if ((frame ? _frames : _page).Remove(id, out var address)
            && !succeeded && status == CoreWebView2WebErrorStatus.ConnectionAborted)
        {
            (frame ? _framesAborted : _pageAborted).Add(address);
        }
    }

    public bool MainFrame(string address) =>
        (_page.ContainsValue(address) || _pageAborted.Contains(address))
        && !_frames.ContainsValue(address) && !_framesAborted.Contains(address);
}
