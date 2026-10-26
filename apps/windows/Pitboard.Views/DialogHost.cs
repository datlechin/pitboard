using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pitboard.Core;

namespace Pitboard.Views;

// One ContentDialog shows per thread at a time, so each dialog of any of the thread's windows waits for the one before
// it.
public sealed class DialogHost(Func<XamlRoot?> root)
{
    [ThreadStatic]
    private static Task? _turn;

    private bool _withdrawn;

    public ContentDialog? Open { get; private set; }

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

    public async Task<bool> Confirm(Question question)
    {
        var dialog = new ContentDialog
        {
            Title = question.Title,
            Content = question.Message,
            PrimaryButtonText = question.Confirm,
            CloseButtonText = "Cancel",
        };
        AutomationProperties.SetAutomationId(dialog, "question");
        return await Show(dialog) == ContentDialogResult.Primary;
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
