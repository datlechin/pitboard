# Writing pitboard's documentation

This folder is the Mintlify source of docs.usepitboard.com. Pages are MDX with YAML
frontmatter. `docs.json` holds navigation and settings.

Merging to `main` publishes the site, and readers run the tagged release, so the facts come
from the code at that release. Where a Markdown file in the repository disagrees with the
code, the code wins. Release procedures, design notes and unreleased behaviour are not site
content; they live in `RELEASING.md`, `ARCHITECTURE.md` and the code.

## Before you commit

Run these in `docs/`. All three must pass:

```sh
mint validate
mint broken-links --check-anchors --check-redirects
mint a11y
```

Preview with `mint dev`. The checks miss the silent failures listed under "MDX", so read the
rendered page too.

## Pages

- Every page has a quoted `title` in sentence case and a quoted one-sentence
  `description` of at most about 20 words, ending with a full stop. Never start it with
  `Learn`, `This page`, `A guide to` or `Welcome`.
- The title is the page's only H1. Headings start at `##`, in sentence case, with no end
  punctuation, no questions and no links. No heading holds only other headings.
- Each page is one type: quickstart, how-to, explanation or reference. Most pages stay
  under 600 words; paragraphs are one to three sentences.
- The first sentence says what the page does or what is true. Stop when the content
  stops; at most one closing line that links to the next page.
- Add a new page to `navigation` in `docs.json`, path without extension.
- Link root-relative without extension: `[Switch accounts](/guides/switch)`. Link text
  names the destination. Never `here` or a bare URL.
- Images go in `images/`, referenced as `/images/name.png`, inside `<Frame>`, always with
  alt text that says what the image shows.

## MDX

- Put every command, flag, path, placeholder, JSON field and anything with `{` or `}` in
  backticks. Mintlify turns `--` in plain text into an em dash, `...` into an ellipsis,
  and drops anything in braces.
- A bare `<label>` breaks the build. Write `` `<label>` ``.
- Comments are `{/* */}`. HTML comments break the build.
- Quote every frontmatter value. An unquoted value that contains a colon and a space breaks
  the build.

## Voice

- Write to `you`, present tense, active voice. When pitboard or a tool acts, make it the
  subject. Never `we`, `our` or `let's`.
- Say what happens, then why. Give the measured number. State limits plainly, with no
  apology and no promise. Say what is not known instead of guessing.
- One idea per sentence, under 25 words.
- Describe; do not sell or reassure.
- British spelling and no contractions in prose: enrol, licence, notarised, behaviour,
  organisation. Commands, fields and UI labels keep their own spelling.
- pitboard is always lower case, also at the start of a sentence and in titles.
- Numerals for durations, sizes and percentages; words for small counts; dates as
  18 June 2026. No serial comma unless needed.

## Banned

- No em dash or en dash as punctuation; write `to` for ranges. No emoji. No exclamation
  marks. Bold only for UI elements.
- Words: `simply`, `just`, `easy`, `quick`, `seamless`, `powerful`, `robust`,
  `effortless`, `intuitive`, `smart`, `unlock`, `empower`, `leverage`, `utilise`, `delve`,
  `crucial`, `key` (adjective), `enhance`, `comprehensive`, `serves as`, `boasts`,
  `Additionally`, `Furthermore`, `Moreover`, `Notably`, `allows you to`, `in order to`,
  `currently`, `now`, `new`, `latest`, `soon`, `e.g.`, `i.e.`, `etc.`, `via`, `click on`,
  `hit`, `toggle`, `enable`, `disable`, `above`, `below`.
- Phrases: `Welcome to`, `In this guide`, `Let's`, `Note that`, `Keep in mind`,
  `It's worth noting`, `In summary`, `That's it`, `Feel free to`.
- Structures: `not just X but Y`, triplets for rhythm, rhetorical questions, trailing
  verdict clauses (`, making it painless`), cycling synonyms, future tense for ordinary
  behaviour.
- `now`, `new` and `latest` are banned when they date a release or a feature. "The new
  login" after a sign-in is fine. Quoted UI text and program output keep their own words.

## Terms

Use the first word, never the others.

| Use | Not |
| --- | --- |
| account | profile, user, identity |
| in use | active, current, logged in |
| login (noun) | credential, token, auth, session (token only on security and reference pages) |
| parked login, park (verb) | stored, saved or cached login, backup |
| label | alias, profile name (app steps: "enter a name") |
| tool (Claude Code or Codex) | provider, client, agent |
| switch | swap, rotate, activate, change account |
| sign in (verb), sign-in (noun) | log in, login (verb), authenticate |
| enrol (prose), `enroll` (command) | register |
| forget | delete or remove an account |
| renew (a parked login) | refresh, rotate |
| usage, limit, five-hour limit, weekly limit, resets | quota, cap, 5h limit, session limit, refills |
| keychain | Keychain, keyring |
| the app, the menu bar app; the command line | Pitboard.app, the GUI; the CLI, the binary |
| pitboard's item in the menu bar; the pitboard window; pane | status item, tray icon, panel, dashboard, tab |
| shortcut menu; dialog; Settings | context menu; sheet, modal; Preferences |
| Claude Code; Codex ("OpenAI's Codex CLI" on first mention) | Claude (for the tool), Codex CLI |
| claude.ai window | web window, browser window, web view |
| pitboard link | deep link, URL scheme link |
| account picker (prose); **Open claude.ai Link** (its title) | chooser, account chooser |
| Share menu; Services menu | share sheet (except Arc's own), services list |

## UI and commands

- UI text in bold, exactly as shown, without a trailing ellipsis: **Add Account**,
  **Sign In Again**, **Sign Out of This Window**. Quote UI text with its contraction:
  **Couldn't Read Accounts**.
- Mac verbs: choose a menu item; click a button, tab or pane; turn on or turn off a
  switch; select a row; press a key; enter text. Paths: **Settings** > **General**.
- Shortcuts spelled out, no bold or code: Command-N, Command-Comma.
- Commands are never verbs: "enrol it with `pitboard enroll`".
- Code blocks always have a language: `sh` for commands, `text` for output, `json`,
  `toml`. No `$` prompt. Commands and output in separate blocks.
- Prefer real values (`work`, `codex/work`, `me@example.com`) to placeholders.

## Components

- Callouts: only `<Warning>` (an action that loses or revokes a login, or cannot be
  undone) and `<Note>` (a fact that changes what the reader does). At most one per
  section, never first on a page, never two in a row.
- `<Steps>` for three or more steps. `<Tabs>` only for the same task done two ways,
  titled "App" and "Command line", in that order.
- `<Columns>` and `<Card>` only on the home page, without icons.
- `<Accordion>` only on reference pages, for detail most readers skip.
- `<ResponseField>` and `<Expandable>` only for the JSON envelope. Not `ParamField`.
- Not used: Tip, Info, Check, Danger, Callout, CodeGroup, Badge, Tooltip, Tree, Mermaid,
  snippets, Icon.
