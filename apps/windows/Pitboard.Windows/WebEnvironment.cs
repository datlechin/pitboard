using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Web.WebView2.Core;
using Pitboard.Core;

namespace Pitboard.Windows;

// The world a launch's account windows run in, decided once: the sites over https, saving in the person's Downloads, or
// a fixture's stand-ins on a scheme of its own, which reach no network, open nothing elsewhere and save in its folder.
// One browser process serves every window, over the app's own user data folder, with a profile per store.
public sealed class WebEnvironment
{
    public const string FixtureScheme = "pitboard-fixture";

    // CORE_WEBVIEW_TARGET_PRODUCT_VERSION of the WebView2 SDK the Windows App SDK brings, 1.0.3719.77: the first
    // Runtime with every call the windows make.
    public const string MinimumRuntime = "144.0.3719.77";

    // HRESULT_FROM_WIN32(ERROR_DELETE_PENDING), which making a profile of a name deleted earlier gives until its folder
    // has gone.
    public const int DeletePending = unchecked((int)0x8007012F);

    private readonly bool _devTools;
    private readonly Action<string> _openElsewhere;
    private Task<CoreWebView2Environment>? _made;

    public WebEnvironment(string scheme, string userDataFolder, string downloads, bool devTools, Action<string> openElsewhere)
    {
        Scheme = scheme;
        UserDataFolder = userDataFolder;
        Downloads = downloads;
        _devTools = devTools;
        _openElsewhere = openElsewhere;
    }

    // Raised once every page of the browser process has closed or it has failed, and it has let go of the folder.
    public event Action? BrowserExited;

    public string Scheme { get; }

    public string UserDataFolder { get; }

    // The folder the windows save downloads in.
    public string Downloads { get; }

    public static WebEnvironment Live(string dataFolder, bool devTools) =>
        new("https", Path.Combine(dataFolder, "WebView2"), Shell.Downloads(), devTools, OpenWithWindows);

    public static WebEnvironment Fixture(string dataFolder, string? userDataFolder = null) =>
        new(
            FixtureScheme, userDataFolder ?? Path.Combine(dataFolder, "WebView2"), Path.Combine(dataFolder, "Downloads"), devTools: true,
            _ => { });

    // Why this PC cannot show an account window, given the Runtime it has, if any; null where it can. A version
    // string may carry its channel after a space.
    public static string? Refusal(string? found)
    {
        var version = found?.Split(' ', 2)[0];
        if (version is not null && Version.TryParse(version, out var has) && has >= Version.Parse(MinimumRuntime))
        {
            return null;
        }

        return PitboardFfiMethods.WebviewRuntimeTooOld(MinimumRuntime, version);
    }

    public static string? Refusal()
    {
        string? found;
        try
        {
            found = CoreWebView2Environment.GetAvailableBrowserVersionString();
        }
        catch (Exception error) when (error is COMException or FileNotFoundException)
        {
            found = null;
        }

        return Refusal(found);
    }

    // Whether a page of this launch has shown its profile's folder where ProfileFolder says.
    public bool LayoutKnown { get; private set; }

    // The browser processes this launch has started: each environment starts one with its first page.
    public int Starts { get; private set; }

    public bool Running => _made is not null;

    public NavigationPolicy Policy(Site site) => new(site, Scheme);

    // Where WebView2 keeps a profile's data, which goes once the profile is deleted and a later browser process starts.
    // Its name is the profile's after WV2Profile_, as CoreWebView2Profile.ProfilePath read on both runners.
    public string ProfileFolder(string store) => Path.Combine(UserDataFolder, "EBWebView", $"WV2Profile_{store}");

    // A fixture's scheme is registered with the browser process, which keeps its registrations for its life. Once that
    // process has exited, as it does when its last page closes, the next page is made in a new one, as Microsoft's
    // example of BrowserProcessExited makes it.
    public Task<CoreWebView2Environment> Made() => _made ??= Make();

    public static CoreWebView2ControllerOptions Options(CoreWebView2Environment environment, string store)
    {
        var options = environment.CreateCoreWebView2ControllerOptions();
        options.ProfileName = store;
        return options;
    }

    // Each page, before it loads anything. No script, host object or message reaches a site's page. Chromium writes a
    // download into a temporary file in its profile's download folder until it is named, so a fixture's stay in its own.
    public void Prepare(CoreWebView2 page)
    {
        Read(page.Profile);
        page.Profile.DefaultDownloadFolderPath = Downloads;
        var settings = page.Settings;
        settings.AreDevToolsEnabled = _devTools;
        settings.AreHostObjectsAllowed = false;
        settings.IsWebMessageEnabled = false;
    }

    // A fixture's pages, to each page once it is a window's: a new window's only after it is set as one.
    public void Serve(CoreWebView2 page)
    {
        if (Scheme == FixtureScheme)
        {
            page.AddWebResourceRequestedFilter(
                $"{FixtureScheme}:*", CoreWebView2WebResourceContext.All, CoreWebView2WebResourceRequestSourceKinds.All);
            page.WebResourceRequested += Answer;
        }
    }

    public void OpenElsewhere(string address) => _openElsewhere(address);

    // From a page of its own, which the deletion closes. A name deleted at an earlier launch is refused until its
    // folder has gone.
    public async Task MarkDeleted(string store, nint parent)
    {
        var environment = await Made();
        CoreWebView2Controller controller;
        try
        {
            controller = await environment.CreateCoreWebView2ControllerAsync(
                CoreWebView2ControllerWindowReference.CreateFromWindowHandle((ulong)parent), Options(environment, store));
        }
        catch (Exception error) when (error.HResult == DeletePending)
        {
            return;
        }

        var profile = controller.CoreWebView2.Profile;
        Read(profile);
        profile.Delete();
    }

    // A page of the default profile, kept until gone says so or fifteen seconds pass: a browser process deletes the
    // folders of profiles deleted before it about five seconds after it starts (docs/facts/windows-runners.md).
    public async Task Sweep(nint parent, Func<bool> gone)
    {
        var keeper = await Keeper(parent);
        try
        {
            var waited = Stopwatch.StartNew();
            while (!gone() && waited.Elapsed < TimeSpan.FromSeconds(15))
            {
                await Task.Delay(250);
            }
        }
        finally
        {
            keeper.Close();
        }
    }

    // A page of a store's profile, which make makes. A browser process started after a profile's deletion refused its
    // name for its first seconds, even where the folder had gone as the last one exited
    // (docs/facts/windows-runners.md), so until it takes the name a page of the default profile keeps it running, for
    // thirty seconds at most.
    public async Task<T> Reusing<T>(nint parent, Func<Task<T>> make)
    {
        try
        {
            return await make();
        }
        catch (Exception refused) when (refused.HResult == DeletePending)
        {
        }

        var keeper = await Keeper(parent);
        try
        {
            var waited = Stopwatch.StartNew();
            while (true)
            {
                await Task.Delay(250);
                try
                {
                    return await make();
                }
                catch (Exception refused) when (refused.HResult == DeletePending && waited.Elapsed < TimeSpan.FromSeconds(30))
                {
                }
            }
        }
        finally
        {
            keeper.Close();
        }
    }

    private async Task<CoreWebView2Controller> Keeper(nint parent)
    {
        var environment = await Made();
        return await environment.CreateCoreWebView2ControllerAsync(
            CoreWebView2ControllerWindowReference.CreateFromWindowHandle((ulong)parent));
    }

    private void Read(CoreWebView2Profile profile) =>
        LayoutKnown |= string.Equals(profile.ProfilePath, ProfileFolder(profile.ProfileName), StringComparison.OrdinalIgnoreCase);

    private async Task<CoreWebView2Environment> Make()
    {
        Starts++;
        var options = new CoreWebView2EnvironmentOptions();
        if (Scheme == FixtureScheme)
        {
            options.CustomSchemeRegistrations =
                [new CoreWebView2CustomSchemeRegistration(FixtureScheme) { HasAuthorityComponent = true }];
        }

        var made = await CoreWebView2Environment.CreateWithOptionsAsync("", UserDataFolder, options);
        made.BrowserProcessExited += (_, _) =>
        {
            _made = null;
            BrowserExited?.Invoke();
        };
        return made;
    }

    private static void Answer(CoreWebView2 page, CoreWebView2WebResourceRequestedEventArgs args)
    {
        var address = args.Request.Uri;
        if (!address.StartsWith(FixtureScheme + ":", StringComparison.OrdinalIgnoreCase))
        {
            return;
        }

        var body = new MemoryStream(Encoding.UTF8.GetBytes(PitboardFfiMethods.FixturePage(address)));
        args.Response = page.Environment.CreateWebResourceResponse(
            body.AsRandomAccessStream(), 200, "OK", "Content-Type: text/html; charset=utf-8");
    }

    private static void OpenWithWindows(string address)
    {
        if (Uri.TryCreate(address, UriKind.Absolute, out var uri))
        {
            _ = global::Windows.System.Launcher.LaunchUriAsync(uri);
        }
    }
}
