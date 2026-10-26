using System.ComponentModel;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pitboard.Windows;

namespace Pitboard.Views;

// The model's alerts in front of the main window, each shown once and taken down when the model drops it.
public sealed partial class WindowAlerts : IDisposable
{
    private readonly AppModel _model;
    private readonly DialogHost _dialogs;
    private WindowAlert? _shown;
    private CancellationTokenSource? _withdraw;

    public WindowAlerts(AppModel model, DialogHost dialogs)
    {
        _model = model;
        _dialogs = dialogs;
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
            _model.Send(alert.Answered(result == ContentDialogResult.Primary));
        }
    }

    private static ContentDialog Dialog(WindowAlert alert)
    {
        switch (alert)
        {
            case WindowAlert.Failed failed:
                var dialog = new ContentDialog
                {
                    Title = failed.Text.Title,
                    Content = failed.Text.Message,
                    CloseButtonText = Words.Window.Ok,
                };
                AutomationProperties.SetAutomationId(dialog, "failure");
                return dialog;
            case WindowAlert.Asking asked:
                return DialogHost.Asking(asked.Question);
            default:
                throw new ArgumentOutOfRangeException(nameof(alert));
        }
    }
}
