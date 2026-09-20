# pitboard

Park and restore your own Claude Code logins on one machine, and see what each one has left.

**Status: pre-release.** The name is reserved; the implementation is in progress.

pitboard keeps one shared `~/.claude` — one history, one set of sessions, one `settings.json` —
and changes only which account is signed in. It never signs you in itself: enrollment prints a
`claude auth login` command for you to run, so sign-in always completes through Anthropic's own flow.

## What it will not do

These are rules, not gaps:

- No automatic switching on any server signal.
- No request pooling, proxying, or `ANTHROPIC_BASE_URL` interception.
- No failover when an account is on hold.
- No OAuth grant of any kind — refreshing tokens is Claude Code's job, never pitboard's.
- No export, import, or sync of parked credentials between machines.

Not affiliated with Anthropic. See `NOTICE`.

## Licence

Apache-2.0
