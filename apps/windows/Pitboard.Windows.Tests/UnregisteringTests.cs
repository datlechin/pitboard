using System.Runtime.InteropServices;
using Microsoft.Windows.AppNotifications;
using Pitboard.Core;

namespace Pitboard.Windows.Tests;

[TestClass]
public sealed class UnregisteringTests
{
    private const string Installed = @"C:\Users\Dana Lee\AppData\Local\Programs\Pitboard\Pitboard.exe";

    private const string Activator = "{29A5793B-2C97-4210-9BA2-C64128F59978}";

    [TestMethod]
    public void TheUninstallDeletesTheRunValueNamingThisPitboardExeInAnyCase()
    {
        var key = new StandInRunKey();
        key.Values["Pitboard"] = $"\"{Installed.ToUpperInvariant()}\" --sign-in";
        var tasks = new StandInScheduledTasks();
        var jumpList = new JumpTasksTests.StandInJumpList();
        var notifications = new StandInNotifications();
        var registration = new StandInRegistration(Activator);

        Unregistering.Run(Build.Release, Installed, key, tasks, jumpList, notifications, registration);

        Assert.IsFalse(key.Values.ContainsKey("Pitboard"));
        Assert.AreEqual($"remove for {Installed}", string.Join(", ", tasks.Asked));
        Assert.AreEqual(1, jumpList.Deleted);
        Assert.AreEqual("unregister all", string.Join(", ", notifications.Asked));
        Assert.AreEqual($"delete {Activator}", string.Join(", ", registration.Asked));
    }

    [TestMethod]
    public void ItLeavesARunValueNamingAnotherPitboardExe()
    {
        var key = new StandInRunKey();
        key.Values["Pitboard"] = @"""C:\Users\Dana Lee\AppData\Local\Programs\Other\Pitboard.exe"" --sign-in";

        Unregistering.Run(
            Build.Release, Installed, key, new StandInScheduledTasks(), new JumpTasksTests.StandInJumpList(), new StandInNotifications(),
            new StandInRegistration(null));

        Assert.AreEqual(@"""C:\Users\Dana Lee\AppData\Local\Programs\Other\Pitboard.exe"" --sign-in", key.Values["Pitboard"]);
        CollectionAssert.DoesNotContain(key.Asked, "remove Pitboard");
    }

    // UnregisterAll makes the AppUserModelId key as it looks for the class, so notifications never registered are left alone.
    [TestMethod]
    public void NotificationsThatNeverRegisteredAreNotAsked()
    {
        var tasks = new StandInScheduledTasks();
        var notifications = new StandInNotifications();
        var registration = new StandInRegistration(null);

        Unregistering.Run(Build.Release, Installed, new StandInRunKey(), tasks, new JumpTasksTests.StandInJumpList(), notifications, registration);

        Assert.AreEqual($"remove for {Installed}", string.Join(", ", tasks.Asked));
        Assert.IsEmpty(notifications.Asked);
        Assert.IsEmpty(registration.Asked);
    }

    [TestMethod]
    public void ACoreThatPanicsOverTheTasksStillLetsTheNotificationsGo()
    {
        var tasks = new StandInScheduledTasks { Fails = new PanicException("Rust panic") };
        var notifications = new StandInNotifications();
        var registration = new StandInRegistration(Activator);

        Unregistering.Run(Build.Release, Installed, new StandInRunKey(), tasks, new JumpTasksTests.StandInJumpList(), notifications, registration);

        Assert.AreEqual($"remove for {Installed}", string.Join(", ", tasks.Asked));
        Assert.AreEqual("unregister all", string.Join(", ", notifications.Asked));
        Assert.AreEqual($"delete {Activator}", string.Join(", ", registration.Asked));
    }

    // The session's PC kept HKCU\Software\Classes\AppUserModelId\Pitboard.Pitboard once the MSI had removed Pitboard
    // (2026-10-18), though its removal ran Pitboard.exe --unregister.
    [TestMethod]
    public void WhatRegisterWroteIsDeletedByNameWhereUnregisterAllFails()
    {
        var notifications = new StandInNotifications { Fails = Marshal.GetExceptionForHR(unchecked((int)0x80004005)) };
        var registration = new StandInRegistration(Activator);

        Unregistering.Run(
            Build.Release, Installed, new StandInRunKey(), new StandInScheduledTasks(), new JumpTasksTests.StandInJumpList(), notifications,
            registration);

        Assert.AreEqual("unregister all", string.Join(", ", notifications.Asked));
        Assert.AreEqual($"delete {Activator}", string.Join(", ", registration.Asked));
    }

    // CI's fresh standard user got ERROR_PATH_NOT_FOUND from DeleteList, with no list to delete (2026-10-27).
    [TestMethod]
    [DataRow(unchecked((int)0x80004005))]
    [DataRow(unchecked((int)0x80070005))]
    [DataRow(unchecked((int)0x80070003))]
    public void AJumpListWindowsRefusesToDeleteStillLetsTheNotificationsGo(int refusal)
    {
        var jumpList = new JumpTasksTests.StandInJumpList { Refuses = Marshal.GetExceptionForHR(refusal) };
        var notifications = new StandInNotifications();
        var registration = new StandInRegistration(Activator);

        Unregistering.Run(
            Build.Release, Installed, new StandInRunKey(), new StandInScheduledTasks(), jumpList, notifications, registration);

        Assert.AreEqual(1, jumpList.Deleted);
        Assert.AreEqual("unregister all", string.Join(", ", notifications.Asked));
        Assert.AreEqual($"delete {Activator}", string.Join(", ", registration.Asked));
    }

    [TestMethod]
    public void ARunKeyItCannotReadStillLetsTheNotificationsGo()
    {
        var key = new StandInRunKey { ReadRefuses = new UnauthorizedAccessException("Access is denied.") };
        var tasks = new StandInScheduledTasks();
        var jumpList = new JumpTasksTests.StandInJumpList();
        var notifications = new StandInNotifications();
        var registration = new StandInRegistration(Activator);

        Unregistering.Run(Build.Release, Installed, key, tasks, jumpList, notifications, registration);

        Assert.AreEqual($"remove for {Installed}", string.Join(", ", tasks.Asked));
        Assert.AreEqual("unregister all", string.Join(", ", notifications.Asked));
        Assert.AreEqual($"delete {Activator}", string.Join(", ", registration.Asked));
    }

    [TestMethod]
    public void AJumpListWindowsCannotDeleteStillLetsTheNotificationsGo()
    {
        var jumpList = new JumpTasksTests.StandInJumpList { Refuses = Marshal.GetExceptionForHR(unchecked((int)0x80004005)) };
        var notifications = new StandInNotifications();
        var registration = new StandInRegistration(Activator);

        Unregistering.Run(
            Build.Release, Installed, new StandInRunKey(), new StandInScheduledTasks(), jumpList, notifications, registration);

        Assert.AreEqual(1, jumpList.Deleted);
        Assert.AreEqual("unregister all", string.Join(", ", notifications.Asked));
        Assert.AreEqual($"delete {Activator}", string.Join(", ", registration.Asked));
    }

    [TestMethod]
    public void ADebugBuildRegisteredNothingAndTouchesNothing()
    {
        var key = new StandInRunKey();
        key.Values["Pitboard"] = $"\"{Installed}\" --sign-in";
        var tasks = new StandInScheduledTasks();
        var jumpList = new JumpTasksTests.StandInJumpList();
        var notifications = new StandInNotifications();
        var registration = new StandInRegistration(Activator);

        Unregistering.Run(Build.Debug, Installed, key, tasks, jumpList, notifications, registration);

        Assert.IsEmpty(key.Asked);
        Assert.IsEmpty(tasks.Asked);
        Assert.AreEqual(0, jumpList.Deleted);
        Assert.IsEmpty(notifications.Asked);
        Assert.IsEmpty(registration.Asked);
    }

    private sealed class StandInNotifications : IAppNotifications
    {
        public List<string> Asked { get; } = [];

        public Exception? Fails { get; init; }

        public bool IsSupported() => throw new AssertFailedException("IsSupported was asked");

        public void Listen(Action<IDictionary<string, string>> invoked) => throw new AssertFailedException("Listen was asked");

        public void Register(string displayName, Uri icon) => throw new AssertFailedException("Register was asked");

        public IDictionary<string, string>? LaunchedBy() => throw new AssertFailedException("LaunchedBy was asked");

        public void Show(AppNotification notification) => throw new AssertFailedException("Show was asked");

        public AppNotificationSetting Setting() => throw new AssertFailedException("Setting was asked");

        public void Unregister() => throw new AssertFailedException("Unregister was asked");

        public void UnregisterAll()
        {
            Asked.Add("unregister all");
            if (Fails is { } failed)
            {
                throw failed;
            }
        }
    }

    // In place of the core, which would ask Task Scheduler: tests never touch it.
    private sealed class StandInScheduledTasks : IScheduledTasks
    {
        public List<string> Asked { get; } = [];

        public Exception? Fails { get; init; }

        public void Remove(string exe)
        {
            Asked.Add($"remove for {exe}");
            if (Fails is { } failed)
            {
                throw failed;
            }
        }
    }

    private sealed class StandInRegistration(string? activator) : INotificationRegistration
    {
        public List<string> Asked { get; } = [];

        public string? Activator() => activator;

        public void Delete(string named) => Asked.Add($"delete {named}");
    }
}
