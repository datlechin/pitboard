using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pitboard.Core;
using Pitboard.Windows;

namespace Pitboard.Views;

// One ContentDialog shows per thread at a time, so each dialog of any of the thread's windows waits for the one before
// it.
public sealed class DialogHost(Func<XamlRoot?> root)
{
    [ThreadStatic]
    private static Task? _turn;

    private bool _withdrawn;
    private ContentDialog? _open;

    // As a dialog shows and as it goes.
    public event EventHandler? OpenChanged;

    public ContentDialog? Open
    {
        get => _open;
        private set
        {
            _open = value;
            OpenChanged?.Invoke(this, EventArgs.Empty);
        }
    }

    // As its window closes: the dialog it shows goes, and none waiting shows.
    public void Withdraw()
    {
        _withdrawn = true;
        Open?.Hide();
    }

    // Null when the dialog never showed: withdrawn before its turn, or no window to show it in.
    public async Task<ContentDialogResult?> Show(ContentDialog dialog, CancellationToken withdrawn = default)
    {
        var before = _turn ?? Task.CompletedTask;
        var done = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        _turn = done.Task;
        try
        {
            await before;
            if (_withdrawn || withdrawn.IsCancellationRequested || root() is not { } shownIn)
            {
                return null;
            }

            dialog.XamlRoot = shownIn;
            // As the WinUI Gallery styles a ContentDialog it makes in code.
            if (dialog.Style is null && Application.Current.Resources.TryGetValue("DefaultContentDialogStyle", out var style))
            {
                dialog.Style = style as Style;
            }

            Open = dialog;
            using (withdrawn.Register(dialog.Hide))
            using (new Theming(dialog, shownIn.Content as FrameworkElement))
            {
                return await dialog.ShowAsync();
            }
        }
        finally
        {
            Open = null;
            done.SetResult();
        }
    }

    public async Task<bool> Confirm(Question question) => await Show(Asking(question)) == ContentDialogResult.Primary;

    public static ContentDialog Asking(Question question)
    {
        var dialog = new ContentDialog { Title = question.Title, Content = question.Message };
        Buttons(dialog, question);
        AutomationProperties.SetAutomationId(dialog, "question");
        return dialog;
    }

    // The default button is the accent one, which Enter presses and the focus opens on: Cancel where the model says its
    // verb is not the default, as Uninstall's is. WinUI has no destructive style.
    public static void Buttons(ContentDialog dialog, Question question)
    {
        dialog.PrimaryButtonText = question.Confirm;
        dialog.IsPrimaryButtonEnabled = true;
        dialog.CloseButtonText = Words.Window.Cancel;
        dialog.DefaultButton = question.ConfirmIsDefault ? ContentDialogButton.Primary : ContentDialogButton.Close;
    }

    // A failure's next step is its default button, beside which OK would read as taking it.
    public static void Buttons(ContentDialog dialog, WindowAlert.Failed failed)
    {
        dialog.PrimaryButtonText = failed.Act?.Title ?? "";
        dialog.IsPrimaryButtonEnabled = failed.Act?.Enabled ?? false;
        dialog.CloseButtonText = failed.Act is null ? Words.Window.Ok : Words.Window.Close;
        dialog.DefaultButton = failed.Act is null ? ContentDialogButton.None : ContentDialogButton.Primary;
    }

    // A dialog draws in a popup, which takes no theme from its window's content, so it is given the content's, as the WinUI
    // Gallery gives one its page's, and follows it while it is up.
    private sealed class Theming : IDisposable
    {
        private readonly ContentDialog _dialog;
        private readonly FrameworkElement? _root;

        public Theming(ContentDialog dialog, FrameworkElement? root)
        {
            _dialog = dialog;
            _root = root;
            if (root is not null)
            {
                dialog.RequestedTheme = root.ActualTheme;
                root.ActualThemeChanged += OnThemeChanged;
            }
        }

        public void Dispose()
        {
            if (_root is not null)
            {
                _root.ActualThemeChanged -= OnThemeChanged;
            }
        }

        private void OnThemeChanged(FrameworkElement sender, object args) => _dialog.RequestedTheme = sender.ActualTheme;
    }
}
