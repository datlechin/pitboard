using System.Collections;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.Windows.AppLifecycle;
using Pitboard.Core;
using Pitboard.Windows;
using Windows.ApplicationModel.Activation;

namespace Pitboard.App;

public static class Program
{
#if DEBUG
    private static readonly Identity Running = new(Build.Debug);
#else
    private static readonly Identity Running = new(Build.Release);
#endif

    internal static readonly string Exe = Environment.ProcessPath ?? Path.Combine(AppContext.BaseDirectory, "Pitboard.exe");

    [STAThread]
    private static int Main(string[] args)
    {
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Shell.Name(Running);
        var environment = Environment.GetEnvironmentVariables().Cast<DictionaryEntry>()
            .ToDictionary(variable => (string)variable.Key, variable => variable.Value as string ?? "");
#if DEBUG
        var fixture = environment.GetValueOrDefault("PITBOARD_FIXTURE");
#else
        string? fixture = null;
#endif
        // Where the core makes it: Rust's temp_dir and .NET's GetTempPath both ask GetTempPath2W.
        var fixtureFolder = fixture is null ? null : Path.Combine(Path.GetTempPath(), "pitboard-fixture");
        ReadFailure? refusal = null;
        AppInstance? running = null;
        var handovers = new Handovers();
        // Connected before the key is held: a launch handed over while Pitboard starts would reach no handler.
        AppInstance.GetCurrent().Activated += (_, activated) => handovers.Arrived(
            activated.Data is ILaunchActivatedEventArgs launch
                ? Launching.Handed(launch.Arguments, Running, fixtureFolder)
                : null);
        var start = Launching.Decide(
            args,
            Running,
            fixtureFolder,
            () => refusal = PitboardFfiMethods.LaunchRefusal(environment),
            () =>
            {
                var holder = AppInstance.FindOrRegisterForKey(Running.InstanceKey(fixtureFolder));
                running = holder.IsCurrent ? null : holder;
                return holder.IsCurrent;
            });
        switch (start)
        {
            case Start.Refused:
                Shell.Refuse(refusal!.Message);
                return 1;
            case Start.LinkRefused:
                Shell.Refuse(Launching.LinkRefusal);
                return 1;
            case Start.Redirect:
                Redirection.To(running!);
                return 0;
            case Start.Unregister:
                Unregistering.Run(
                    Running.Build,
                    Exe,
                    new RegistryRunKey(),
                    new CoreScheduledTasks(environment),
                    new SystemAppNotifications(),
                    new RegistryNotificationRegistration(Running.Aumid));
                return 0;
            case Start.UnregisterShare:
                return SharePackage.Unregister(new SystemSharePackages(), AppContext.BaseDirectory) == 0 ? 0 : 1;
            case Start.Dropped:
                return 0;
        }

        var started = new AppStart(
            Running, environment, fixture, Running.DataFolder(Shell.LocalAppData(), fixtureFolder))
        {
            Arguments = args,
        };
        Application.Start(p =>
        {
            SynchronizationContext.SetSynchronizationContext(
                new DispatcherQueueSynchronizationContext(DispatcherQueue.GetForCurrentThread()));
            _ = new App(started, handovers);
        });
        return 0;
    }
}
