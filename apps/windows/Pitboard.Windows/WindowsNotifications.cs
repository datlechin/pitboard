using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using Microsoft.Windows.AppNotifications;
using Microsoft.Windows.AppNotifications.Builder;
using Pitboard.Core;

namespace Pitboard.Windows;

// Why Windows shows none of Pitboard's notifications, and whether Settings > System > Notifications turns them on.
public sealed record NotificationsBlocked(string Why, bool InSettings);

public sealed class WindowsNotifications(Build build, Func<IAppNotifications> system, string icon) : Notifier
{
    // What Windows registers the app's notifications under; the probe registers it with no core to ask.
    public const string DisplayName = "Pitboard";

    public const string IconFile = "Pitboard.ico";

    public const string Argument = "clicked";

    private readonly Lock _lock = new();
    private IAppNotifications? _registered;

    // Why Windows refused to register them, once Start has asked. A Debug build registers nothing and says nothing of
    // it, or its settings would say it in the docs' pictures.
    public string? Off { get; private set; }

    // Windows offers this process no notifications at all; the settings say so in the model's words.
    public bool Unoffered { get; private set; }

    // Register takes the icon as a file, which Pitboard.exe carries beside it.
    public static WindowsNotifications For(Build build, string folder) =>
        new(build, () => new SystemAppNotifications(), Path.Combine(folder, IconFile));

    // In the quickstart's order: listen, register, then read the click that started this process.
    public Intent? Start(Action<Intent> clicked)
    {
        if (build is Build.Debug)
        {
            return null;
        }

        var notifications = system();
        try
        {
            if (!notifications.IsSupported())
            {
                Unoffered = true;
                return null;
            }

            notifications.Listen(arguments => clicked(Read(arguments)));
            notifications.Register(DisplayName, new Uri(icon));
        }
        catch (Exception error) when (Refused(error))
        {
            Off = error.Message;
            return null;
        }

        lock (_lock)
        {
            _registered = notifications;
        }

        try
        {
            return notifications.LaunchedBy() is { } launched ? Read(launched) : null;
        }
        catch (Exception error) when (Refused(error))
        {
            return null;
        }
    }

    // On the model's thread.
    public void Post(Notice notice)
    {
        IAppNotifications? registered;
        lock (_lock)
        {
            registered = _registered;
        }

        try
        {
            registered?.Show(Notification(notice));
        }
        catch (Exception refused)
        {
            // This PC shows the reason, and a message can name a path.
            throw new PlatformException.Failed($"{refused.GetType().Name} 0x{refused.HResult:X8}");
        }
    }

    // At each settings visit, since the person turns them off and on in Settings while Pitboard runs.
    public NotificationsBlocked? Read()
    {
        if (Unoffered)
        {
            return Blocked(AppNotificationSetting.Unsupported);
        }

        if (Off is { } off)
        {
            return new(off, InSettings: false);
        }

        IAppNotifications? registered;
        lock (_lock)
        {
            registered = _registered;
        }

        try
        {
            return registered is null ? null : Blocked(registered.Setting());
        }
        catch (Exception error) when (Refused(error))
        {
            return null;
        }
    }

    // Only the person's own switches are in Settings: Group Policy's and a manifest's are not.
    public static NotificationsBlocked? Blocked(AppNotificationSetting setting) => setting switch
    {
        AppNotificationSetting.Enabled => null,
        AppNotificationSetting.DisabledForApplication => new(Words.Settings.NotificationsOff, InSettings: true),
        AppNotificationSetting.DisabledForUser => new(Words.Own.NotificationsOffForEveryApp, InSettings: true),
        AppNotificationSetting.DisabledByGroupPolicy => new(Words.Own.NotificationsOffByPolicy, InSettings: false),
        _ => new(Words.Own.NotificationsUnoffered, InSettings: false),
    };

    // Before Pitboard exits, so a later click starts it again.
    public void Stop()
    {
        lock (_lock)
        {
            try
            {
                _registered?.Unregister();
            }
            catch (Exception error) when (Refused(error))
            {
                // COM revokes the process's registration as it exits anyway.
            }

            _registered = null;
        }
    }

    // A tag holds 64 characters, and a warm-up's notice is told by its message.
    public static string Tag(string id) => Convert.ToHexStringLower(SHA256.HashData(Encoding.UTF8.GetBytes(id)));

    // Title, tool and body, in the macOS notification's order.
    public static AppNotification Notification(Notice notice)
    {
        var builder = new AppNotificationBuilder().AddText(notice.Title);
        if (notice.Subtitle is { } subtitle)
        {
            builder.AddText(subtitle);
        }

        builder.AddText(notice.Body).SetTag(Tag(notice.Id)).AddArgument(Argument, notice.Clicked);
        if (notice.Button is { } button)
        {
            builder.AddButton(new AppNotificationButton(button.Title).AddArgument(Argument, button.Clicked));
        }

        return builder.BuildNotification();
    }

    // The model reads what a click hands back, from a notification an older Pitboard posted too.
    public static Intent Read(IDictionary<string, string> arguments) =>
        new Intent.NotificationClicked(arguments.TryGetValue(Argument, out var clicked) ? clicked : "");

    private static bool Refused(Exception error) =>
        error is COMException or ArgumentException or InvalidOperationException or UnauthorizedAccessException or IOException;
}
