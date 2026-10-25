using System.Runtime.InteropServices;
using System.Security;
using Pitboard.Core;

namespace Pitboard.Windows;

// What the MSI's full uninstall asks of Pitboard.exe --unregister: what Pitboard made itself, which no table of the MSI
// removes. Each is tried whatever became of the others.
public static class Unregistering
{
    public static void Run(
        Build build, string exe, IRunKey run, IScheduledTasks tasks, IAppNotifications notifications, INotificationRegistration registration)
    {
        if (build is Build.Debug)
        {
            return;
        }

        try
        {
            if (RunValue.Names(run.Value(RunValue.Name), exe))
            {
                run.Remove(RunValue.Name);
            }
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or SecurityException)
        {
        }

        try
        {
            tasks.Remove(exe);
        }
        catch (Exception error) when (error is UniffiException or TypeInitializationException or DllNotFoundException)
        {
        }

        try
        {
            // UnregisterAll makes the AppUserModelId key as it looks for the class the key names.
            if (registration.Activator() is not { } activator)
            {
                return;
            }

            try
            {
                notifications.UnregisterAll();
            }
            catch (Exception failed) when (failed is COMException or InvalidOperationException or UnauthorizedAccessException)
            {
            }

            // The session's PC kept the AppUserModelId key past it (2026-10-18), so what Register wrote goes by name.
            registration.Delete(activator);
        }
        catch (Exception error) when (error is COMException or IOException or UnauthorizedAccessException or SecurityException)
        {
        }
    }
}
