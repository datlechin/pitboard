using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pitboard.Core;
using Pitboard.Windows;

namespace Pitboard.Views;

// The model's alerts in front of the main window, each shown once and taken down when the model drops it.
public sealed partial class WindowAlerts : IDisposable
{
    private readonly AppModel _model;
    private readonly DialogHost _dialogs;
    private readonly Action<Uri> _open;
    private WindowAlert? _shown;
    private CancellationTokenSource? _withdraw;

    public WindowAlerts(AppModel model, DialogHost dialogs, Action<Uri> open)
    {
        _model = model;
        _dialogs = dialogs;
        _open = open;
        model.PropertyChanged += OnModelChanged;
        Follow();
    }

    public void Dispose()
    {
        _model.PropertyChanged -= OnModelChanged;
        _withdraw?.Cancel();
    }

    private void OnModelChanged(object? sender, PropertyChangedEventArgs args)
    {
        if (args.PropertyName is nameof(AppModel.FailureAlert) or nameof(AppModel.QuitQuestion)
            or nameof(AppModel.ReplaceQuestion) or nameof(AppModel.Sheet))
        {
            Follow();
        }
    }

    private void Follow()
    {
        var alert = WindowAlert.InSheet(_model) ? null : WindowAlert.Of(_model);
        if (Equals(alert, _shown))
        {
            return;
        }

        _withdraw?.Cancel();
        _withdraw = null;
        _shown = alert;
        if (alert is not null)
        {
            Present(alert);
        }
    }

    private async void Present(WindowAlert alert)
    {
        using var withdraw = new CancellationTokenSource();
        _withdraw = withdraw;
        var answer = await _dialogs.Show(Dialog(alert), withdraw.Token);
        if (_withdraw == withdraw)
        {
            _withdraw = null;
        }

        if (answer is { } result && !withdraw.IsCancellationRequested)
        {
            foreach (var intent in alert.Answered(result == ContentDialogResult.Primary))
            {
                _model.Send(intent);
            }
        }
    }

    private ContentDialog Dialog(WindowAlert alert) =>
        alert switch
        {
            WindowAlert.Failed failed => Failure(failed),
            WindowAlert.Asking asked => DialogHost.Asking(asked.Question),
            _ => throw new ArgumentOutOfRangeException(nameof(alert)),
        };

    // Its next step is its button, or a link to the docs under what it says.
    private ContentDialog Failure(WindowAlert.Failed failed)
    {
        var said = new StackPanel { Spacing = 8 };
        said.Children.Add(new TextBlock { Text = failed.Text.Message, TextWrapping = TextWrapping.Wrap });
        if (failed.Learn is { } learn)
        {
            said.Children.Add(Learning(learn, _open));
        }

        var dialog = new ContentDialog
        {
            Title = failed.Text.Title,
            Content = said,
            PrimaryButtonText = failed.Act?.Title ?? "",
            IsPrimaryButtonEnabled = failed.Act?.Enabled ?? false,
            CloseButtonText = Words.Window.Ok,
            DefaultButton = failed.Act is null ? ContentDialogButton.None : ContentDialogButton.Primary,
        };
        AutomationProperties.SetAutomationId(dialog, "failure");
        return dialog;
    }

    private static HyperlinkButton Learning(SetupLink learn, Action<Uri> open)
    {
        var link = new HyperlinkButton { Content = learn.Title, Padding = new Thickness(0) };
        AutomationProperties.SetAutomationId(link, "failure.learn");
        link.Click += (_, _) => open(new Uri(learn.Link));
        return link;
    }
}
