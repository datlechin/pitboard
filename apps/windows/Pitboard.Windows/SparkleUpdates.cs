using System.Net;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Xml;
using Pitboard.Core;

namespace Pitboard.Windows;

// A release's updater, as the macOS app's SparkleUpdater: WinSparkle over the feed of full releases.
public sealed class SparkleUpdates : IUpdates
{
    public const string Feed = "https://github.com/datlechin/pitboard/releases/latest/download/appcast-windows.xml";

    // The assembly metadata the build writes the public key into, from WINDOWS_UPDATE_PUBLIC_KEY.
    public const string KeyName = "WindowsUpdatePublicKey";

    private static readonly HttpClient Http = new() { Timeout = TimeSpan.FromSeconds(30) };

    private readonly ISparkle _sparkle;
    private readonly string _version;
    private readonly UpdateSettings _settings;
    private readonly Func<Action, bool> _post;
    private readonly Func<Task<bool>> _mayQuit;
    private readonly Action _quit;
    private readonly Func<Task<string>> _feed;
    private readonly Lock _lock = new();
    private TaskCompletionSource<bool>? _asking;
    private bool _started;
    private bool _stopped;
    private bool _looking;
    private bool _noFeed;

    internal SparkleUpdates(
        ISparkle sparkle, string version, UpdateSettings settings, Func<Action, bool> post, Func<Task<bool>> mayQuit,
        Action quit, Func<Task<string>> feed)
    {
        _sparkle = sparkle;
        _version = version;
        _settings = settings;
        _post = post;
        _mayQuit = mayQuit;
        _quit = quit;
        _feed = feed;
    }

    public event EventHandler? Changed;

    // A beta offers checks only once WinSparkle has started over a feed.
    public bool Available => !Beta || _started;

    public string? Refusal => _noFeed ? Words.Own.NoReleaseToUpdateTo : null;

    public bool Beta => Versions.IsBeta(_version);

    public string? Waiting { get; private set; }

    public bool ChecksAutomatically
    {
        get => _started && _sparkle.ChecksAutomatically;
        set
        {
            if (_started)
            {
                _sparkle.ChecksAutomatically = value;
            }
        }
    }

    // post runs an action on the app's thread; mayQuit asks about downloads there, and quit ends Pitboard.
    public static IUpdates For(
        Build build, string? key, string version, string dataFolder, Func<Action, bool> post, Func<Task<bool>> mayQuit,
        Action quit) =>
        For(build, key, version, dataFolder, new WinSparkle(), post, mayQuit, quit, () => Http.GetStringAsync(Feed));

    // With no key WinSparkle installs updates nobody signed, so a build without one it takes never starts it.
    internal static IUpdates For(
        Build build, string? key, string version, string dataFolder, ISparkle sparkle, Func<Action, bool> post,
        Func<Task<bool>> mayQuit, Action quit, Func<Task<string>> feed)
    {
        if (build is Build.Debug || string.IsNullOrEmpty(key))
        {
            return new NoUpdates();
        }

        try
        {
            if (sparkle.SetKey(key) == 0)
            {
                return new NoUpdates(Words.Own.UpdateKeyInvalid);
            }
        }
        catch (DllNotFoundException)
        {
            return new NoUpdates(Words.Own.WinSparkleMissing);
        }

        var settings = new UpdateSettings(Path.Combine(dataFolder, "updates.json"));
        return new SparkleUpdates(sparkle, version, settings, post, mayQuit, quit, feed);
    }

    public static string? Key(Assembly app) =>
        app.GetCustomAttributes<AssemblyMetadataAttribute>().FirstOrDefault(metadata => metadata.Key == KeyName)?.Value;

    // Once Pitboard's icon is in the notification area, as WinSparkle asks of an app's main window.
    public void Start()
    {
        if (Beta)
        {
            LookForFeed();
        }
        else
        {
            Begin();
        }
    }

    // GitHub answers the feed with a 404 until a full release carries one, and a check would end in WinSparkle's error.
    public void LookForFeed()
    {
        if (!Beta || _started || _stopped || _looking)
        {
            return;
        }

        _looking = true;
        _ = Look();
    }

    private void Begin()
    {
        _sparkle.Configure(
            Feed, _version, _settings,
            new SparkleEvents(MayShutDown, () => _post(_quit), Found, () => _post(() => Show(null)), () => _post(() => Show(null))));
        // On with no question, as SUEnableAutomaticChecks has it on macOS, until the person turns them off.
        if (_settings.Read("CheckForUpdates") is null)
        {
            _sparkle.ChecksAutomatically = true;
        }

        _sparkle.Start();
        _started = true;
    }

    // WinSparkle's own window says what it finds, as Sparkle's does on macOS.
    public void Check()
    {
        Show(null);
        if (_started)
        {
            _sparkle.Check();
        }
    }

    // A question WinSparkle still waits on is answered no, so its thread lets Shutdown join it.
    public void Shutdown()
    {
        lock (_lock)
        {
            if (_stopped)
            {
                return;
            }

            _stopped = true;
            if (!_started)
            {
                return;
            }

            _asking?.TrySetResult(false);
        }

        _sparkle.Stop();
    }

    // WinSparkle's thread waits for the downloads question, asked on the app's.
    private bool MayShutDown()
    {
        var asking = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        lock (_lock)
        {
            if (_stopped)
            {
                return false;
            }

            _asking = asking;
        }

        if (!_post(async () => asking.TrySetResult(await _mayQuit())))
        {
            return false;
        }

        return asking.Task.GetAwaiter().GetResult();
    }

    // Any answer but a 404, such as none while offline, starts WinSparkle, which says what it finds once asked.
    private async Task Look()
    {
        var found = true;
        try
        {
            await _feed();
        }
        catch (HttpRequestException missing) when (missing.StatusCode is HttpStatusCode.NotFound)
        {
            found = false;
        }
        catch (Exception unread) when (unread is HttpRequestException or TaskCanceledException)
        {
        }

        _post(() => Looked(found));
    }

    private void Looked(bool found)
    {
        _looking = false;
        if (_stopped)
        {
            return;
        }

        _noFeed = !found;
        if (found)
        {
            Begin();
        }

        Changed?.Invoke(this, EventArgs.Empty);
    }

    // WinSparkle names no version to its callback, so it is read from the feed it just read.
    private void Found() => _ = Name();

    private async Task Name()
    {
        string? newest;
        try
        {
            newest = UpdateFeed.Newest(await _feed(), UpdateFeed.Marker(RuntimeInformation.ProcessArchitecture));
        }
        catch (Exception unread) when (unread is HttpRequestException or TaskCanceledException or XmlException)
        {
            return;
        }

        if (newest is not null)
        {
            _post(() => Show(newest));
        }
    }

    private void Show(string? waiting)
    {
        if (Waiting == waiting)
        {
            return;
        }

        Waiting = waiting;
        Changed?.Invoke(this, EventArgs.Empty);
    }
}
