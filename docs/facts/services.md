# Anthropic's and OpenAI's services

## api.anthropic.com

On 22 September 2026 the machine measured sat within 0.75 seconds of the `Date` header of
api.anthropic.com across eight requests, and `Date` has a granularity of one second. That
spread is inside the noise, so Pitboard keeps no estimate of clock skew, and a renewal's
expiries are counted from the `Date` of the answer that carried them.

## claude.ai and chatgpt.com

Read and measured on 29 September 2026. Nobody has watched a sign-in complete in a window.
That needs a person, in a debug build with a scratch home.

- `chat.openai.com` answers 308, `www.chatgpt.com` 301 and `chat.com` 307, each to the same
  path and query on `chatgpt.com`, measured with `curl`. So `Site.chatGPT` takes them as
  aliases.
- OpenAI's help centre, article 7426629, lists signing in to ChatGPT with a password or with
  Google, Microsoft or Apple, and names `auth.openai.com` among the hosts its sign-in needs.
- Third-party code that drives the sign-in, and the buttons' connection names, put the
  Microsoft and Apple sign-ins on `login.live.com`, `login.microsoftonline.com` and
  `appleid.apple.com`. The same reading has the sign-in come back through
  `chatgpt.com/api/auth`. How chatgpt.com's Microsoft and Apple buttons open their sign-in,
  in a new window, by a link or by a redirect, is not known.
- Not measured: whether email sign-in completes in a window, whether a window's session
  survives a restart, and whether Cloudflare's challenges pass WebKit's own user agent.
- The Share menu lists app extensions in Safari's toolbar and **File** menu, and in
  **File** > **Share** in Chrome and in Firefox from 92. That was read from the browsers'
  source and bug trackers, and no browser was run. Which other browsers list the extension
  is not known.
