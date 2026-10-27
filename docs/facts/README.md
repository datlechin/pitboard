# Facts

What Pitboard's code rests on about the systems, frameworks, libraries and sites around it,
each as it was measured or read, with the build, run or date it comes from. Measure a fact
again, as [AGENTS.md](../../AGENTS.md) allows, before changing code that rests on it.

What Pitboard rests on about Claude Code, Codex and Claude Desktop is in each tool's register,
`crates/pitboard-core/src/provider/<tool>/assumptions.rs`, which `pitboard-conformance` reads.
These pages are for contributors: `docs/.mintignore` keeps them off the site.

- [macOS](macos.md): `security`, the keychain, APFS and `ps`.
- [Windows runners](windows-runners.md): what `windows-facts.yml` measured on GitHub's runners.
- [The session machine](windows-session.md): what the session of 18 October 2026 measured.
- [Documented, not measured](windows-documented.md): what Microsoft documents and nothing has
  measured yet.
- [What the Windows app's glue rests on](windows-app.md): WinUI, WebView2, the Windows App SDK,
  WinSparkle and Microsoft's guidance.
- [The macOS app](macos-app.md): what the Swift app said, and Foundation and AppKit.
- [WebKit and SwiftUI](webkit.md): the account windows on macOS.
- [Foundation's URLs](foundation-urls.md): how a link was read before `pitboard-sites`.
- [The bindings](bindings.md): what UniFFI generates for C# and Swift.
- [ureq's proxies](ureq.md): the proxy variables and SOCKS.
- [Anthropic's and OpenAI's services](services.md): the clock of Anthropic's answers, and the
  sites the account windows open.
- [One sign-in at a time](sign-in-lock.md): `flock` and std's process starts.
- [The app model](app-model.md): a look beside the app's own changes, and the timers.
- [The fixtures](fixtures.md): the worlds the apps' debug builds launch into.
- [The site's figures](site.md): the shells, prompts and keys usepitboard.com's terminals
  draw, the keys a browser keeps from a page, a box's edges in Firefox, and how the apps' menus,
  dialogs, lists and Limits controls behave.
- [Not measured](not-measured.md): what nobody has measured about the tools, and what Pitboard
  does where it cannot know.
