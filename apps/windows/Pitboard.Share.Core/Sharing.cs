namespace Pitboard.Share.Core;

// A share target's ShareOperation and the start of Pitboard.exe, which tests replace.
public interface IShareOperation
{
    void ReportStarted();

    Task<string?> WebLinkAsync();

    void StartPitboard(string link);

    void ReportCompleted();

    void ReportError(string sentence);
}

// Pitboard.exe checks the link it is handed again, as it does any link.
public static class Sharing
{
    // An exception that left the program would end it without a word to the share sheet.
    public static async Task HandOverAsync(IShareOperation share, string scheme)
    {
        ArgumentNullException.ThrowIfNull(share);
        share.ReportStarted();
        string? webLink;
        try
        {
            webLink = await share.WebLinkAsync().ConfigureAwait(false);
        }
        catch (Exception)
        {
            share.ReportError(PitboardShareFfiMethods.ShareWords().NotRead);
            return;
        }

        string link;
        try
        {
            link = PitboardShareFfiMethods.ShareLink(webLink ?? "", scheme);
        }
        catch (ShareRefusal.Refused refused)
        {
            share.ReportError(refused.reason);
            return;
        }

        try
        {
            share.StartPitboard(link);
        }
        catch (Exception)
        {
            share.ReportError(PitboardShareFfiMethods.ShareWords().NotStarted);
            return;
        }

        share.ReportCompleted();
    }
}
