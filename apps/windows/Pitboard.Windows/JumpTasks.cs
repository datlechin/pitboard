using System.Runtime.InteropServices;
using Pitboard.Core;

namespace Pitboard.Windows;

// The model's Jump List, and what a task chosen there asks once it reaches this copy. A switch is a request from
// outside, so it never asks a question, and one refused says why in a notification: a Jump List has no reply.
public sealed class JumpTasks(AppModel model, IJumpList list, Action<WindowAccount> open, Action<Notice> post)
{
    private readonly HashSet<ulong> _asked = [];
    private readonly List<string> _windows = [];
    private ulong _requests;

    public void Take(QuickAsk ask)
    {
        switch (ask)
        {
            case QuickAsk.Switch switching:
                var request = ++_requests;
                _asked.Add(request);
                model.Send(new Intent.Outside(request, new OutsideAsk.Switch(switching.Account)));
                break;
            case QuickAsk.AddAccount:
                model.Send(new Intent.PresentSheet(new Sheet.Add(Tool: null)));
                break;
            case QuickAsk.OpenWindow window:
                _windows.Add(window.Store);
                OpenWaiting();
                break;
        }
    }

    // Each time the model's list changes, which it has once the accounts are read.
    public void Follow()
    {
        if (model.QuickActions is not { } tasks)
        {
            return;
        }

        try
        {
            list.Show(tasks);
        }
        catch (Exception refused) when (refused is COMException or IOException or UnauthorizedAccessException)
        {
            // Windows keeps the list it had, and the next change writes this one again.
        }

        OpenWaiting();
    }

    // A window asked for before the accounts are read opens once they are, and one they no longer list opens nothing.
    private void OpenWaiting()
    {
        if (model.QuickActions is null)
        {
            return;
        }

        foreach (var store in _windows)
        {
            if (model.AccountWindows.Accounts.FirstOrDefault(
                    window => string.Equals(window.Store, store, StringComparison.OrdinalIgnoreCase)) is { } shown)
            {
                open(shown);
            }
        }

        _windows.Clear();
    }

    // Each time the model's answers change.
    public void Answered()
    {
        foreach (var answer in model.Answers)
        {
            if (!_asked.Remove(answer.Request) || answer.Done)
            {
                continue;
            }

            try
            {
                post(PitboardModelMethods.OutsideRefused(Identity.Front, answer));
            }
            catch (PlatformException)
            {
                // Windows shows none of Pitboard's notifications, which Settings says.
            }
        }
    }
}
