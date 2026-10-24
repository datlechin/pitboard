namespace Pitboard.Core.Tests;

// pitboard-sites' sites, with the hosts and paths a snapshot carries, for what the app hands back to the core.
internal static class Sites
{
    public static readonly Site Claude = new(
        Host: "claude.ai", Name: "claude.ai", Provider: "claude", Aliases: [], Hosts: ["claude.ai"], SignInHosts: [],
        BlockedHosts: ["accounts.google.com"], SignInPaths: [["magic-link"]], StoreName: "claude", SignInSteps: "",
        BlockedServices: "");

    public static readonly Site ChatGpt = new(
        Host: "chatgpt.com", Name: "chatgpt.com", Provider: "codex",
        Aliases: ["chat.openai.com", "www.chatgpt.com", "chat.com"],
        Hosts: ["chatgpt.com", "chat.openai.com", "www.chatgpt.com", "chat.com"],
        SignInHosts: ["auth.openai.com", "login.live.com", "login.microsoftonline.com", "appleid.apple.com"],
        BlockedHosts: ["accounts.google.com"], SignInPaths: [["api", "auth"]], StoreName: "codex", SignInSteps: "",
        BlockedServices: "");
}
