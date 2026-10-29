using Pitboard.Core;

namespace Pitboard.Windows;

// What WebView2 says of a navigation, as the policy reads it. WebView2 names no frame that asked for a page's own
// navigation, so the page asks as what it shows; a frame's own navigation, and a frame that asked for a window, are a
// frame's.
public static class PageRequests
{
    public static NavigationRequest Navigation(
        NavigationPolicy policy, PageRole role, string address, bool frame, bool userInitiated, string? shown) =>
        new(
            address, frame ? NavigationTarget.Frame : NavigationTarget.Page, userInitiated, Download: false,
            Asking(policy, role, shown, mainFrame: !frame));

    public static NavigationRequest NewWindow(
        NavigationPolicy policy, PageRole role, string address, bool userInitiated, string? source, bool mainFrame) =>
        new(address, NavigationTarget.NewWindow, userInitiated, Download: false, Asking(policy, role, source, mainFrame));

    // WebView2 raises DownloadStarting only for what it will not show, and gives an empty header for none.
    public static ResponseFacts Download(string address, string disposition, bool mainFrame) =>
        new(address, mainFrame, CanShow: false, disposition.Length > 0 ? disposition : null);

    // A port of the scheme's own is 0, and an address with no host, such as about:blank, has an empty one.
    public static FrameOrigin Origin(string? address, bool mainFrame) =>
        Uri.TryCreate(address, UriKind.Absolute, out var uri)
            ? new FrameOrigin(mainFrame, uri.Scheme, uri.Host, uri.IsDefaultPort ? 0 : uri.Port)
            : new FrameOrigin(mainFrame, "", "", 0);

    // A sign-in window's own page asks for itself, and nothing in it is the site's.
    private static Asker Asking(NavigationPolicy policy, PageRole role, string? source, bool mainFrame) =>
        role switch
        {
            PageRole.Popup => mainFrame ? Asker.SignIn : Asker.Other,
            _ when source is null => Asker.Other,
            _ => PitboardModelMethods.FrameAsker(policy, Origin(source, mainFrame)),
        };
}
