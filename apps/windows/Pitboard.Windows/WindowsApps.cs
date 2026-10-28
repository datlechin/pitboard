using System.Runtime.InteropServices;
using Pitboard.Core;
using Windows.Win32.Foundation;
using Windows.Win32.System.RestartManager;

namespace Pitboard.Windows;

internal sealed record AppProcess(uint Id, uint Parent, string? Family, string? Aumid, bool OwnsAWindow);

// Store apps such as ChatGPT, by their AUMID, asked to quit and never forced.
public sealed class WindowsApps : OtherApps
{
    private readonly Func<IReadOnlyList<AppProcess>> _listed;
    private readonly Func<uint, RM_UNIQUE_PROCESS?> _unique;
    private readonly IRestartManager _restart;
    private readonly Action<string> _activate;
    private readonly Action _pitboardForward;

    // pitboardForward brings Pitboard's window in front of the app it opened, where the window is open.
    public WindowsApps(Action pitboardForward)
        : this(PackagedApps.Listed, PackagedApps.Unique, new SystemRestartManager(), PackagedApps.Activate, pitboardForward)
    {
    }

    internal WindowsApps(
        Func<IReadOnlyList<AppProcess>> listed, Func<uint, RM_UNIQUE_PROCESS?> unique, IRestartManager restart,
        Action<string> activate, Action pitboardForward)
    {
        _listed = listed;
        _unique = unique;
        _restart = restart;
        _activate = activate;
        _pitboardForward = pitboardForward;
    }

    // Where its own AUMID cannot be read it still runs, by the core's. A Mac app never runs here.
    public string? Running(AppId app) =>
        app is AppId.WindowsAumid { Id: var aumid } && Windowed(aumid) is { } running ? running.Aumid ?? aumid : null;

    public void RequestQuit(AppId app)
    {
        if (app is AppId.WindowsAumid { Id: var aumid })
        {
            _ = Quit(aumid);
        }
    }

    public void Reopen(string location, bool front)
    {
        _activate(location);
        if (!front)
        {
            _pitboardForward();
        }
    }

    // RmShutdown's error, or 0, which the probe prints: the model polls Running instead.
    public Task<int> Quit(string app)
    {
        if (Windowed(app) is not { } running || _unique(running.Id) is not { } process)
        {
            return Task.FromResult(0);
        }

        Succeeded(_restart.StartSession(out var session), "start a Restart Manager session");
        try
        {
            Succeeded(_restart.Register(session, process), $"register process {running.Id} with Restart Manager");
        }
        catch
        {
            _restart.EndSession(session);
            throw;
        }

        // Without RmForceShutdown, and on a thread of its own: RmShutdown returns once the app has ended or declined.
        return Task.Factory.StartNew(
            () =>
            {
                try
                {
                    return (int)_restart.Shutdown(session, 0);
                }
                finally
                {
                    _restart.EndSession(session);
                }
            },
            CancellationToken.None,
            TaskCreationOptions.LongRunning,
            TaskScheduler.Default);
    }

    // Such as ChatGPT's codex, which Restart Manager is never given and which the probe watches outlive it.
    public IReadOnlyList<uint> Windowless(string app) => [.. Its(app).Where(process => !process.OwnsAWindow).Select(process => process.Id)];

    // Never a codex it runs, which has no window. Of several with windows, the one the package did not start.
    private AppProcess? Windowed(string app)
    {
        var its = Its(app);
        var windowed = its.Where(process => process.OwnsAWindow).ToList();
        return windowed.FirstOrDefault(process => its.All(other => other.Id != process.Parent)) ?? windowed.FirstOrDefault();
    }

    private List<AppProcess> Its(string app)
    {
        var family = app.Split('!')[0];
        return [.. _listed().Where(process => string.Equals(process.Family, family, StringComparison.OrdinalIgnoreCase))];
    }

    private static void Succeeded(WIN32_ERROR result, string what)
    {
        if (result != WIN32_ERROR.ERROR_SUCCESS)
        {
            throw new PlatformException.Failed($"Windows did not {what}: {Marshal.GetPInvokeErrorMessage((int)result)}");
        }
    }
}
