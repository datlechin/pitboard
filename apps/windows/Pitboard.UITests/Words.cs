namespace Pitboard.UITests;

// What a test still reads as words: a sentence with no part of its own to find. present/words.rs pins the model's; a change
// there changes it here.
internal static class Words
{
    public const string Welcome = "Welcome to Pitboard";

    public const string WarmsUpAgain = "Warms up again at ";

    public const string NotNow = "Not now";

    public const string UseWithCodex = "Use Pitboard with Codex";

    public const string SwitchBeforeRunningOut = "Switch before a limit runs out";

    public const string AddASecondAccount = "Add a second account";

    public const string GoogleStopped = "Pitboard stopped it";

    public const string NotInstalled = "Not installed";

    public const string NewTerminals = "Pitboard’s installer puts pitboard on your PATH";

    public const string NoUpdateKey = "This copy of Pitboard can";

    public static string SignInTo(string site) => $"Sign in to {site}";
}
