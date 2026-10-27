using System.Diagnostics;
using System.Globalization;
using System.Runtime.InteropServices;
using Microsoft.Win32;
using Pitboard.Core;
using Pitboard.Windows;
using Windows.Win32;
using Windows.Win32.Foundation;
using Windows.Win32.System.Threading;

namespace Pitboard.Probe;

// An AppUserModelID of its own, so an installed Pitboard keeps its notification registration.
public static class Program
{
    private const string Aumid = "Pitboard.Probe";

    // pitboard-probe's, which the owner puts in each throwaway account's profile folder.
    private const string Marker = "pitboard-probe-throwaway.marker";

    private const string Usage =
        "Usage: Pitboard.Probe notify | unregister | package-of --pid <process id> | app-control running|quit|reopen --aumid <id> [--front]";

    private const string AumidKey = $@"Software\Classes\AppUserModelId\{Aumid}";

    private static readonly RunOutNotice RunOut = new(
        Id: "pitboard-probe-run-out", Title: "work has no 5-hour limit left", Subtitle: null,
        Body: "spare has 80% of its own left.", Clicked: "accounts", Button: new NotificationButton("Switch", "switch/claude/spare"));

    // As long as the model waits for an app it asked to quit, looking as often.
    private static readonly TimeSpan QuitWithin = TimeSpan.FromSeconds(30);
    private static readonly TimeSpan QuitCheckedEvery = TimeSpan.FromMilliseconds(200);

    private static int Main(string[] args)
    {
        if (!File.Exists(Path.Combine(Shell.KnownFolder(PInvoke.FOLDERID_Profile), Marker)))
        {
            Console.Error.WriteLine(
                $"This account carries no {Marker} in its profile folder: Pitboard.Probe runs only in a throwaway account the owner marked.");
            return 2;
        }

        WinRT.ComWrappersSupport.InitializeComWrappers();
        PInvoke.SetCurrentProcessExplicitAppUserModelID(Aumid).ThrowOnFailure();
        return args switch
        {
            ["notify"] => Notify(),
            [Launching.Notified, ..] => StartedByAClick(),
            ["unregister"] => Unregister(),
            ["package-of", "--pid", var given] when uint.TryParse(given, CultureInfo.InvariantCulture, out var pid) => PackageOf(pid),
            ["app-control", "running", "--aumid", var app] => Running(app),
            ["app-control", "quit", "--aumid", var app] => Quit(app),
            ["app-control", "reopen", "--aumid", var app] => Reopen(app, front: false),
            ["app-control", "reopen", "--aumid", var app, "--front"] => Reopen(app, front: true),
            _ => Refuse(Usage),
        };
    }

    private static WindowsNotifications Notifications() =>
        new(Build.Release, () => new SystemAppNotifications(), Path.Combine(AppContext.BaseDirectory, WindowsNotifications.IconFile));

    // Posts one notification and waits for its Switch, or for Enter to quit before it.
    private static int Notify()
    {
        var notifications = Notifications();
        var clicked = new TaskCompletionSource<Intent>(TaskCreationOptions.RunContinuationsAsynchronously);
        notifications.Start(intent => clicked.TrySetResult(intent));
        if (!Registered(notifications))
        {
            return 1;
        }

        try
        {
            notifications.Post(RunOut);
            Console.WriteLine($"Posted, tagged {WindowsNotifications.Tag(RunOut.Id)}. Choose Switch, or press Enter to quit first.");
            var entered = Task.Run(Console.ReadLine);
            Console.WriteLine(Task.WaitAny(clicked.Task, entered) == 0 ? $"Clicked: {Said(clicked.Task.Result)}." : "Quit before a click.");
            return 0;
        }
        finally
        {
            notifications.Stop();
        }
    }

    // COM starts the probe this way for a click on a notification it posted before it quit.
    private static int StartedByAClick()
    {
        var notifications = Notifications();
        var launchedBy = notifications.Start(intent => Console.WriteLine($"Clicked: {Said(intent)}."));
        if (!Registered(notifications))
        {
            return 1;
        }

        Console.WriteLine($"Started by a click: {Said(launchedBy)}. Press Enter to close.");
        Console.ReadLine();
        notifications.Stop();
        return 0;
    }

    private static bool Registered(WindowsNotifications notifications)
    {
        Console.WriteLine(
            notifications.Off is { } off ? $"Not registered: {off}"
            : notifications.Unoffered ? "Not registered: AppNotificationManager.IsSupported() is false."
            : "Registered.");
        return notifications.Off is null && !notifications.Unoffered;
    }

    private static string Said(Intent? intent) =>
        intent switch
        {
            Intent.NotificationClicked { Argument: "switch/claude/spare" } => "Switch to claude/spare",
            Intent.NotificationClicked => "the notification itself",
            _ => "nothing",
        };

    // UnregisterAll makes the AppUserModelId key as it looks for the class the key names, so it runs only after a Register.
    private static int Unregister()
    {
        string? activator;
        using (var key = Registry.CurrentUser.OpenSubKey(AumidKey))
        {
            activator = key?.GetValue("CustomActivator") as string;
        }

        if (activator is null)
        {
            Console.WriteLine($"Nothing to unregister: HKCU\\{AumidKey} names no class.");
            return 0;
        }

        Left("Before", activator);
        new SystemAppNotifications().UnregisterAll();
        Left("After", activator);
        return 0;
    }

    // What Register wrote and what Windows keeps for the probe's notifications, by name only.
    private static void Left(string when, string activator)
    {
        string[] keys =
        [
            AumidKey,
            $@"Software\Classes\CLSID\{activator}",
            $@"Software\Microsoft\Windows\CurrentVersion\Notifications\Settings\{Aumid}",
        ];
        foreach (var name in keys)
        {
            using var key = Registry.CurrentUser.OpenSubKey(name);
            Console.WriteLine($"{when}: HKCU\\{name} {(key is null ? "is gone" : "is there")}.");
        }
    }

    private static int PackageOf(uint pid)
    {
        using var process = PInvoke.OpenProcess_SafeHandle(PROCESS_ACCESS_RIGHTS.PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
        if (process.IsInvalid)
        {
            return Refuse($"Process {pid} could not be opened: {Marshal.GetLastPInvokeErrorMessage()}");
        }

        var length = 0u;
        var error = PInvoke.GetPackageFullName(process, ref length, default);
        if (error == WIN32_ERROR.APPMODEL_ERROR_NO_PACKAGE)
        {
            Console.WriteLine($"Process {pid} has no package identity.");
            return 0;
        }

        if (error == WIN32_ERROR.ERROR_INSUFFICIENT_BUFFER)
        {
            var name = new char[length];
            error = PInvoke.GetPackageFullName(process, ref length, name);
            if (error == WIN32_ERROR.ERROR_SUCCESS)
            {
                Console.WriteLine($"Process {pid} runs as {new string(name, 0, (int)length - 1)}.");
                return 0;
            }
        }

        return Refuse($"GetPackageFullName failed for process {pid}: {error}");
    }

    // This console's window stands in for Pitboard's, which an app opened without --front goes behind.
    private static WindowsApps Apps() => new(() => PInvoke.SetForegroundWindow(PInvoke.GetConsoleWindow()));

    private static int Running(string app)
    {
        Console.WriteLine(Apps().Running(app) is { } running ? $"Running as {running}." : "Not running.");
        return 0;
    }

    // Through Restart Manager, as Pitboard asks, then looked for as the model looks. With RmShutdown's answer, and the
    // app's processes without a window, which Restart Manager is never given (CF9, session step A4).
    private static int Quit(string app)
    {
        var apps = Apps();
        if (apps.Running(app) is null)
        {
            Console.WriteLine("Not running.");
            return 0;
        }

        var windowless = apps.Windowless(app);
        Console.WriteLine($"Without a window: {Listed(windowless)}.");
        var asked = Stopwatch.StartNew();
        var answered = apps.Quit(app).ContinueWith(quit => (Error: quit.Result, After: asked.Elapsed), TaskScheduler.Default);
        var gone = true;
        while (apps.Running(app) is not null)
        {
            if (asked.Elapsed >= QuitWithin)
            {
                gone = false;
                break;
            }

            Thread.Sleep(QuitCheckedEvery);
        }

        var looked = asked.Elapsed;
        if (answered.Wait(QuitWithin))
        {
            var (error, after) = answered.Result;
            Console.WriteLine($"RmShutdown gave {error} ({Marshal.GetPInvokeErrorMessage(error)}) after {after.TotalMilliseconds:0} ms.");
        }
        else
        {
            Console.WriteLine($"RmShutdown had not answered after {asked.Elapsed.TotalSeconds:0} s.");
        }

        Console.WriteLine(gone ? $"Gone after {looked.TotalSeconds:0.0} s." : $"Still running after {QuitWithin.TotalSeconds:0} s.");
        Console.WriteLine($"Of those without a window, still running: {Listed([.. apps.Windowless(app).Intersect(windowless)])}.");
        return gone ? 0 : 1;
    }

    private static string Listed(IReadOnlyList<uint> processes) =>
        processes.Count == 0 ? "none" : string.Join(", ", processes.Select(id => $"process {id}"));

    private static int Reopen(string app, bool front)
    {
        var asked = Stopwatch.StartNew();
        Apps().Reopen(app, front);
        Console.WriteLine($"Opened {app}{(front ? "" : ", then this window brought forward")} in {asked.Elapsed.TotalSeconds:0.0} s.");
        Thread.Sleep(TimeSpan.FromSeconds(2));
        Console.WriteLine($"In front 2 s later: {InFront()}.");
        return 0;
    }

    private static string InFront()
    {
        var window = PInvoke.GetForegroundWindow();
        if (window.IsNull)
        {
            return "no window";
        }

        PInvoke.GetWindowThreadProcessId(window, out var pid);
        try
        {
            using var process = Process.GetProcessById((int)pid);
            return $"{process.ProcessName}, process {pid}";
        }
        catch (ArgumentException)
        {
            return $"process {pid}, since ended";
        }
    }

    private static int Refuse(string said)
    {
        Console.Error.WriteLine(said);
        return 2;
    }
}
