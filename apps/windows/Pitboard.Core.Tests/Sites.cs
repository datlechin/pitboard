namespace Pitboard.Core.Tests;

// The sites as a snapshot carries them, for what the app hands back to the core.
internal static class Sites
{
    public static readonly Site Claude = Of("claude.ai", "claude", []);

    public static readonly Site ChatGpt = Of("chatgpt.com", "codex", ["auth.openai.com"]);

    private static Site Of(string host, string provider, string[] signInHosts) =>
        new(
            Host: host, Name: host, Provider: provider, Aliases: [], Hosts: [host], SignInHosts: signInHosts,
            BlockedHosts: [], SignInPaths: [], StoreName: provider, SignInSteps: "", BlockedServices: "");
}
