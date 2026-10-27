using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Pitboard.Core;
using Pitboard.Windows;
using Windows.ApplicationModel.DataTransfer;
using Windows.Foundation;
using Windows.System;

namespace Pitboard.Views;

public sealed partial class AccountSheet : ContentDialog, IDisposable
{
    private static readonly HashSet<string> Parts =
    [
        nameof(AppModel.Sheet), nameof(AppModel.SheetText), nameof(AppModel.SigningIn), nameof(AppModel.SigningInText),
        nameof(AppModel.StowText), nameof(AppModel.SheetFailure), nameof(AppModel.FailureAlert),
        nameof(AppModel.QuitQuestion), nameof(AppModel.ReplaceQuestion),
    ];

    private readonly AppModel _model;
    private readonly Action<Uri> _open;
    private readonly CancellationTokenSource _withdrawn = new();
    private SheetPage? _page;
    private WindowAlert? _alert;
    private string _tool = "";
    private bool _filled;
    private SheetTool[]? _tools;
    private ulong? _failureShown;
    private string? _announced;
    private bool _told;

    public AccountSheet(AppModel model, Sheet sheet, Action<Uri> open)
    {
        _model = model;
        _open = open;
        Sheet = sheet;
        InitializeComponent();
        CloseButtonText = Words.Window.Cancel;
        // The tool picker marks the focus it takes from the keyboard handled.
        AddHandler(GettingFocusEvent, new TypedEventHandler<UIElement, GettingFocusEventArgs>(OnGettingFocus), handledEventsToo: true);
        model.PropertyChanged += OnModelChanged;
        Show();
    }

    // The model's sheet it shows: another one, even of the same kind, is put up afresh.
    public Sheet Sheet { get; }

    public CancellationToken Withdrawn => _withdrawn.Token;

    // The model has let it go, or put up another: it goes however it is asked to.
    public void Withdraw() => _withdrawn.Cancel();

    // What closing it tells the model, once: as it starts to go, so a sheet asked for meanwhile reaches a model that has
    // heard, rather than one still running the sign-in it held.
    public IReadOnlyList<Intent> Dismissed()
    {
        if (_told)
        {
            return [];
        }

        _told = true;
        return _page?.Closed() ?? [new Intent.CloseSheet()];
    }

    public void Dispose()
    {
        _model.PropertyChanged -= OnModelChanged;
        _withdrawn.Dispose();
    }

    private void OnModelChanged(object? sender, PropertyChangedEventArgs args)
    {
        if (args.PropertyName is { } part && Parts.Contains(part))
        {
            Show();
        }
    }

    // Another sheet's words never reach this one, and one on its way out keeps what it showed.
    private void Show()
    {
        if (_model.Sheet is not { } sheet || sheet != Sheet || SheetPage.Of(_model, _page) is not { } page)
        {
            return;
        }

        var entered = page.GetType() != _page?.GetType();
        _page = page;
        // Each part is shown or hidden once, so a field being typed in keeps its focus through a snapshot.
        var form = page as SheetPage.Form;
        var signingIn = page as SheetPage.SigningIn;
        var stowing = page as SheetPage.Stowing;
        var asked = signingIn is { SignIn.WantsCode: true } && CodePart.Visibility == Visibility.Collapsed;
        ToolPicker.Visibility = Look.Shown(form is { Text.Tools.Length: > 1 });
        AccountPart.Visibility = Look.Shown(form is { Text.Account: not null });
        NameField.Visibility = Look.Shown(page is SheetPage.Naming);
        Working.Visibility = Look.Shown((form?.Text.Progress ?? stowing?.Text.Looking) is not null);
        Link.Visibility = Look.Shown(signingIn is { Text.OffersLink: true });
        CodePart.Visibility = Look.Shown(signingIn is { SignIn.WantsCode: true });
        MissingBar.Visibility = Look.Shown(form is not null);
        Waiting.IsActive = signingIn is not null;
        Waiting.Visibility = Look.Shown(signingIn is not null);
        if (form is not null)
        {
            ShowForm(form);
        }
        else if (page is SheetPage.Naming naming)
        {
            ShowNaming(naming, entered);
        }
        else if (signingIn is not null)
        {
            ShowSigningIn(signingIn);
        }
        else if (stowing is not null)
        {
            ShowStowing(stowing);
        }

        // Appears once the tool asks, with the code on the clipboard to paste.
        if (asked)
        {
            DispatcherQueue.TryEnqueue(Ui.Guarded(() => CodeField.Focus(FocusState.Programmatic)));
        }

        ShowFailure(_model.SheetFailure);
        CloseButtonText = page.CloseTitle ?? "";
        if (page.Closable)
        {
            ClearValue(CloseButtonStyleProperty);
        }
        else
        {
            CloseButtonStyle = (Style)Resources["HeldBack"];
        }

        Enable();
        ShowAlert(WindowAlert.Of(_model));
    }

    // In place of the step until it is answered, which the step then comes back from as it was left.
    private void ShowAlert(WindowAlert? alert)
    {
        var fresh = !Equals(alert, _alert);
        _alert = alert;
        Steps.Visibility = Look.Shown(alert is null);
        Asked.Visibility = Look.Shown(alert is not null);
        AutomationProperties.SetAutomationId(this, alert switch
        {
            WindowAlert.Asking => "question",
            WindowAlert.Failed => "failure",
            _ => "sheet",
        });
        switch (alert)
        {
            case WindowAlert.Asking asked:
                Title = asked.Question.Title;
                AskedMessage.Text = asked.Question.Message;
                DialogHost.Buttons(this, asked.Question);
                break;
            case WindowAlert.Failed failed:
                Title = failed.Text.Title;
                AskedMessage.Text = failed.Text.Message;
                DialogHost.Buttons(this, failed);
                break;
        }

        AskedLearn.Content = (alert as WindowAlert.Failed)?.Learn?.Title;
        AskedLearn.Visibility = Look.Shown(alert is WindowAlert.Failed { Learn: not null });
        if (alert is not null)
        {
            SecondaryButtonText = "";
            ClearValue(CloseButtonStyleProperty);
        }

        if (!fresh)
        {
            return;
        }

        if (alert is null)
        {
            DispatcherQueue.TryEnqueue(Ui.Guarded(FocusField));
            return;
        }

        Say(AskedMessage, $"{Title} {AskedMessage.Text}");
        var button = DefaultButton == ContentDialogButton.Primary ? "PrimaryButton" : "CloseButton";
        DispatcherQueue.TryEnqueue(Ui.Guarded(() => (GetTemplateChild(button) as Control)?.Focus(FocusState.Programmatic)));
    }

    private void ShowForm(SheetPage.Form form)
    {
        var text = form.Text;
        // The tool starts from the model once, and keeps what was picked when a sign-in that failed comes back to it.
        if (!_filled)
        {
            _filled = true;
            _tool = text.Tool;
        }

        Title = text.Title;
        if (!Structural.Equal(_tools, text.Tools))
        {
            _tools = text.Tools;
            ToolPicker.Items.Clear();
            foreach (var tool in text.Tools)
            {
                var choice = new RadioButton { Content = tool.Name, Tag = tool.Code };
                AutomationProperties.SetAutomationId(choice, $"sheet.tool.{tool.Code}");
                ToolPicker.Items.Add(choice);
            }

            ToolPicker.SelectedIndex = Array.FindIndex(text.Tools, tool => tool.Code == _tool);
        }

        ToolPicker.IsEnabled = !text.Saving;
        AccountText.Text = text.Account ?? "";
        WorkingText.Text = text.Progress ?? "";
        ShowMissing(form.NotFound(_tool));
        Message.Text = form.Message(_tool);
        PrimaryButtonText = form.Confirm(_tool);
        SecondaryButtonText = "";
        DefaultButton = ContentDialogButton.Primary;
    }

    private void ShowNaming(SheetPage.Naming naming, bool entered)
    {
        var text = naming.Text;
        if (entered)
        {
            NameField.Text = text.Name;
        }

        Title = text.Title;
        Message.Text = text.Message;
        NameField.PlaceholderText = text.Prompt;
        NameField.IsEnabled = !text.Saving;
        Describe(NameField, naming.Problem(NameField.Text));
        PrimaryButtonText = text.Confirm;
        SecondaryButtonText = "";
        DefaultButton = ContentDialogButton.Primary;
    }

    private void ShowSigningIn(SheetPage.SigningIn signingIn)
    {
        var (running, text) = (signingIn.SignIn, signingIn.Text);
        Title = text.Title;
        Message.Text = text.Message;
        // The message says what it waits for, so the ring goes unlabelled on screen; Narrator still names it.
        AutomationProperties.SetName(Waiting, text.Waiting);
        // Dimmed rather than left out until the tool prints the address, so the sheet does not grow a moment after it opens.
        CopyLink.IsEnabled = signingIn.Page is not null;
        OpenLink.IsEnabled = signingIn.Page is not null;
        ToolTipService.SetToolTip(OpenLink, running.Url);
        RefusedBar.Message = text.Refused ?? "";
        RefusedBar.Margin = new Thickness(0, text.Refused is null ? 0 : 8, 0, 0);
        RefusedBar.IsOpen = text.Refused is not null;
        Describe(CodeField, text.CodeNote);
        PrimaryButtonText = running.WantsCode ? Words.Sheet.SubmitCode : "";
        SecondaryButtonText = text.Undo?.Title ?? "";
        DefaultButton = running.WantsCode ? ContentDialogButton.Primary : ContentDialogButton.None;
        Announce(text.Announce);
    }

    private void ShowStowing(SheetPage.Stowing stowing)
    {
        var text = stowing.Text;
        Title = text.Title;
        Message.Text = text.Message;
        WorkingText.Text = text.Looking ?? "";
        PrimaryButtonText = text.Confirm;
        SecondaryButtonText = "";
        DefaultButton = ContentDialogButton.Primary;
    }

    // Where Pitboard did not find the tool picked: what it needs, and where to read how to get it.
    private void ShowMissing(Missing? missing)
    {
        MissingBar.IsOpen = missing is not null;
        MissingBar.Margin = new Thickness(0, missing is null ? 0 : 12, 0, 0);
        if (missing is null)
        {
            return;
        }

        MissingBar.Title = missing.Title;
        MissingBar.Message = missing.Detail;
        MissingLink.Content = missing.LinkTitle;
    }

    // The InfoBar announces itself as it opens, so a failure after another opens it again. Focus goes back to the field
    // the failure is about, so Enter tries again.
    private void ShowFailure(FailureText? failure)
    {
        var fresh = failure is not null && failure.Id != _failureShown;
        _failureShown = failure?.Id;
        if (failure is null)
        {
            FailureBar.IsOpen = false;
            FailureBar.Margin = new Thickness(0);
            return;
        }

        FailureBar.Title = failure.Title;
        FailureBar.Message = failure.Message;
        Warnings.Children.Clear();
        foreach (var warning in failure.Warnings)
        {
            Warnings.Children.Add(new TextBlock { Text = warning, TextWrapping = TextWrapping.Wrap });
        }

        Warnings.Visibility = Look.Shown(failure.Warnings.Length > 0);
        FailureBar.Margin = new Thickness(0, 12, 0, 0);
        if (fresh)
        {
            FailureBar.ActionButton = NextStepOf(failure.Next);
            FailureBar.IsOpen = false;
            DispatcherQueue.TryEnqueue(Ui.Guarded(FocusField));
        }

        FailureBar.IsOpen = true;
    }

    // An action in the app as the bar's button, or the docs about it as its link.
    private ButtonBase NextStepOf(NextStep next)
    {
        ButtonBase button = next switch
        {
            NextStep.Act { Choice: var choice } => new Button { Content = choice.Title, IsEnabled = choice.Enabled },
            NextStep.Learn { Link: var link } => new HyperlinkButton { Content = link.Title },
            _ => throw new ArgumentOutOfRangeException(nameof(next)),
        };
        AutomationProperties.SetAutomationId(button, "sheet.failure.next");
        button.Click += (_, _) =>
        {
            if (next is NextStep.Act { Choice: var choice })
            {
                _model.Send(choice.Intent);
            }
            else if (next is NextStep.Learn { Link: var link } && Uri.TryCreate(link.Link, UriKind.Absolute, out var page))
            {
                _open(page);
            }
        };
        return button;
    }

    private void Enable()
    {
        IsPrimaryButtonEnabled = _page switch
        {
            SheetPage.Form form => form.Start(_tool) is not null,
            SheetPage.Naming naming => naming.Save(NameField.Text) is not null,
            SheetPage.SigningIn signingIn => signingIn.Submit(CodeField.Text) is not null,
            SheetPage.Stowing stowing => stowing.PutAway() is not null,
            _ => false,
        };
        IsSecondaryButtonEnabled = _page is SheetPage.SigningIn { Text.Undo.Enabled: true };
    }

    // What appears by itself is not read by Narrator, so the sheet says it.
    private void Announce(string? said)
    {
        if (said is not null && said != _announced)
        {
            Say(Message, said);
        }

        _announced = said;
    }

    private static void Say(UIElement from, string said) =>
        (FrameworkElementAutomationPeer.FromElement(from) ?? FrameworkElementAutomationPeer.CreatePeerForElement(from))
            ?.RaiseNotificationEvent(
                AutomationNotificationKind.Other, AutomationNotificationProcessing.ImportantMostRecent, said, "sheet");

    private void OnToolChanged(object sender, SelectionChangedEventArgs args)
    {
        if (ToolPicker.SelectedItem is RadioButton { Tag: string tool } && _page is SheetPage.Form form)
        {
            // The picker is filled again when what is found changes, which picks the same tool.
            if (tool != _tool)
            {
                Edited();
            }

            _tool = tool;
            Message.Text = form.Message(tool);
            PrimaryButtonText = form.Confirm(tool);
            ShowMissing(form.NotFound(tool));
            Enable();
        }
    }

    // TextBox's template shows a string description on one line; a TextBlock of its own wraps under the field.
    private static void Describe(TextBox field, string? text)
    {
        if (text is null)
        {
            field.Description = null;
        }
        else if (field.Description is TextBlock note)
        {
            note.Text = text;
        }
        else
        {
            field.Description = new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap };
        }
    }

    private void OnNameChanged(object sender, TextChangedEventArgs args)
    {
        Describe(NameField, (_page as SheetPage.Naming)?.Problem(NameField.Text));
        Edited();
        Enable();
    }

    // What went wrong was about the name or the tool it was asked with.
    private void Edited()
    {
        if (_model.SheetFailure is not null)
        {
            _model.Send(new Intent.DismissSheetFailure());
        }
    }

    private void OnMissingLink(object sender, RoutedEventArgs args)
    {
        if (_page is SheetPage.Form form && form.NotFound(_tool) is { } missing
            && Uri.TryCreate(missing.Link, UriKind.Absolute, out var page))
        {
            _open(page);
        }
    }

    private void OnCodeChanged(object sender, TextChangedEventArgs args) => Enable();

    private void OnAskedLearn(object sender, RoutedEventArgs args)
    {
        if (_alert is WindowAlert.Failed { Learn: { } learn } && Uri.TryCreate(learn.Link, UriKind.Absolute, out var page))
        {
            _open(page);
        }
    }

    // The model closes the sheet once it is done, so the dialog stays up meanwhile, saying what went wrong if anything does.
    private void OnPrimaryButtonClick(ContentDialog sender, ContentDialogButtonClickEventArgs args)
    {
        args.Cancel = true;
        if (_alert is { } alert)
        {
            Send(alert.Answered(confirmed: true));
            return;
        }

        var intent = _page switch
        {
            SheetPage.Form form => form.Start(_tool),
            SheetPage.Naming naming => naming.Save(NameField.Text),
            SheetPage.SigningIn signingIn => signingIn.Submit(CodeField.Text),
            SheetPage.Stowing stowing => stowing.PutAway(),
            _ => null,
        };
        if (intent is null)
        {
            return;
        }

        _model.Send(intent);
        if (intent is Intent.PasteCode)
        {
            CodeField.Text = "";
        }
    }

    private void OnSecondaryButtonClick(ContentDialog sender, ContentDialogButtonClickEventArgs args)
    {
        args.Cancel = true;
        if (_page is SheetPage.SigningIn { Text.Undo: { Enabled: true } undo })
        {
            _model.Send(undo.Intent);
        }
    }

    private void OnClosing(ContentDialog sender, ContentDialogClosingEventArgs args)
    {
        if (_withdrawn.IsCancellationRequested)
        {
            return;
        }

        // Cancel and OK answer what the window asked, and the step it asked over stays.
        if (_alert is { } alert)
        {
            args.Cancel = true;
            Send(alert.Answered(confirmed: false));
            return;
        }

        if (_page is { Closable: false })
        {
            args.Cancel = true;
            return;
        }

        Send(Dismissed());
    }

    private void Send(IEnumerable<Intent> intents)
    {
        foreach (var intent in intents)
        {
            _model.Send(intent);
        }
    }

    // Esc presses the close button, which a sign-in waiting on its app keeps for a click.
    private void OnPreviewKeyDown(object sender, KeyRoutedEventArgs args)
    {
        if (args.Key == VirtualKey.Escape && _alert is null && _page is { EscapeCloses: false })
        {
            args.Handled = true;
        }
    }

    // The dialog focuses its first control as it loads, at times after Opened; focus coming in from outside goes to the field.
    private void OnGettingFocus(UIElement sender, GettingFocusEventArgs args)
    {
        if (Field() is { } field && args.NewFocusedElement != field && Look.Ancestor<AccountSheet>(args.OldFocusedElement) != this)
        {
            args.TrySetNewFocusedElement(field);
        }
    }

    private TextBox? Field() =>
        NameField.Visibility == Visibility.Visible && NameField.IsEnabled ? NameField
        : CodePart.Visibility == Visibility.Visible ? CodeField
        : null;

    private void FocusField() => Field()?.Focus(FocusState.Programmatic);

    // As printed, as an address for an app that reads one and as text for a field that reads only text, such as a browser's address bar.
    private void OnCopyLink(object sender, RoutedEventArgs args)
    {
        if (_page is SheetPage.SigningIn { SignIn.Url: { } printed, Page: { } page })
        {
            var package = new DataPackage();
            package.SetText(printed);
            package.SetWebLink(page);
            Clipboard.SetContent(package);
        }
    }

    private void OnOpenLink(object sender, RoutedEventArgs args)
    {
        if (_page is SheetPage.SigningIn { Page: { } page })
        {
            _open(page);
        }
    }
}
