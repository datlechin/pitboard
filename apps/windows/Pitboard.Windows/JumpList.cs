using System.Runtime.InteropServices;
using Pitboard.Core;
using Windows.Win32;
using Windows.Win32.Foundation;
using Windows.Win32.System.Com.StructuredStorage;
using Windows.Win32.System.Variant;
using Windows.Win32.UI.Shell;
using Windows.Win32.UI.Shell.Common;
using Windows.Win32.UI.Shell.PropertiesSystem;

namespace Pitboard.Windows;

public interface IJumpList
{
    // In place of the tasks it held, for the taskbar and Start to show while Pitboard runs and after it has exited.
    void Show(IReadOnlyList<JumpTask> tasks);

    void Delete();
}

// Win32's Jump List needs no package identity: it is kept under the AppUserModelId the process and the Start shortcut
// carry, and each task is a link to Pitboard.exe with its arguments.
public sealed class SystemJumpList(Identity identity, string exe) : IJumpList
{
    public void Show(IReadOnlyList<JumpTask> tasks)
    {
        // A Debug build registers nothing with Windows.
        if (identity.Build is Build.Debug)
        {
            return;
        }

        var list = (ICustomDestinationList)new DestinationList();
        list.SetAppID(identity.Aumid);
        // Tasks are never the person's to remove, so the removed destinations name none.
        list.BeginList(out _, out IObjectArray _);
        try
        {
            list.AddUserTasks(Links(tasks));
            list.CommitList();
        }
        catch
        {
            list.AbortList();
            throw;
        }
    }

    public void Delete() => ((ICustomDestinationList)new DestinationList()).DeleteList(identity.Aumid);

    // A separator between the switches, Add account and the windows, as the icon's menu has.
    public static IEnumerable<JumpTask?> Grouped(IReadOnlyList<JumpTask> tasks)
    {
        for (var index = 0; index < tasks.Count; index++)
        {
            if (index > 0 && tasks[index].Ask.GetType() != tasks[index - 1].Ask.GetType())
            {
                yield return null;
            }

            yield return tasks[index];
        }
    }

    internal IObjectCollection Links(IReadOnlyList<JumpTask> tasks)
    {
        var links = (IObjectCollection)new EnumerableObjectCollection();
        foreach (var task in Grouped(tasks))
        {
            links.AddObject(task is null ? Separator() : Link(task));
        }

        return links;
    }

    internal unsafe IShellLinkW Link(JumpTask task)
    {
        var link = (IShellLinkW)new ShellLink();
        link.SetPath(exe);
        link.SetArguments(string.Join(' ', Launching.Of(task.Ask)));
        link.SetIconLocation(exe, 0);
        var properties = (IPropertyStore)link;
        fixed (char* title = task.Title)
        {
            var value = new PROPVARIANT();
            value.vt = VARENUM.VT_LPWSTR;
            value.pwszVal = title;
            // The store keeps a copy of the value.
            properties.SetValue(PInvoke.PKEY_Title, value);
        }

        properties.Commit();
        return link;
    }

    private static IShellLinkW Separator()
    {
        var link = (IShellLinkW)new ShellLink();
        var properties = (IPropertyStore)link;
        var value = new PROPVARIANT();
        value.vt = VARENUM.VT_BOOL;
        value.boolVal = VARIANT_BOOL.VARIANT_TRUE;
        properties.SetValue(PInvoke.PKEY_AppUserModel_IsDestListSeparator, value);
        properties.Commit();
        return link;
    }
}
