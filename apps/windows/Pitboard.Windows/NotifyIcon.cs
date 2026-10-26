using Windows.Win32;

namespace Pitboard.Windows;

public enum IconMessage
{
    Add,
    Modify,
    Delete,
    SetFocus,
    SetVersion,
}

// One icon as Shell_NotifyIconW identifies it: by its GUID, or by its window and id.
public sealed record IconData(nint Window, uint Id, Guid? IconGuid, uint CallbackMessage, nint Image, string Tip);

public interface INotificationArea
{
    bool Send(IconMessage message, IconData icon);
}

public sealed class NotifyIcon(INotificationArea area, nint window, nint image, Guid? iconGuid)
{
    public const uint Id = 1;

    public const uint CallbackMessage = PInvoke.WM_APP + 1;

    // szTip holds 128 UTF-16 units with its terminating null.
    public const int TipLength = 127;

    private Guid? _guid = iconGuid;
    private nint _image = image;
    private string _tip = Words.AppName;
    private bool _added;

    public static string Tip(string tip) =>
        tip.Length <= TipLength ? tip : tip[..(char.IsHighSurrogate(tip[TipLength - 1]) ? TipLength - 1 : TipLength)];

    public void Show(string tip)
    {
        _tip = Tip(tip);
        if (!_added)
        {
            Add();
        }
        else
        {
            area.Send(IconMessage.Modify, Data());
        }
    }

    public void Redraw(nint image)
    {
        _image = image;
        if (_added)
        {
            area.Send(IconMessage.Modify, Data());
        }
    }

    // Explorer started again, with no icon of Pitboard's.
    public void Readd()
    {
        _added = false;
        Add();
    }

    // After a menu of the icon's is cancelled from the keyboard, as Shell_NotifyIconW asks.
    public void ReturnFocus()
    {
        if (_added)
        {
            area.Send(IconMessage.SetFocus, Data());
        }
    }

    public void Remove()
    {
        if (_added)
        {
            area.Send(IconMessage.Delete, Data());
            _added = false;
        }
    }

    private void Add()
    {
        _added = area.Send(IconMessage.Add, Data());
        if (!_added && _guid is not null)
        {
            // Explorer keeps a GUID for the path it was first added from, so a moved Pitboard.exe is refused it.
            _guid = null;
            _added = area.Send(IconMessage.Add, Data());
        }

        if (_added)
        {
            area.Send(IconMessage.SetVersion, Data());
        }
    }

    private IconData Data() => new(window, Id, _guid, CallbackMessage, _image, _tip);
}
