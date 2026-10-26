import { clock } from "./world.ts";
import { updated } from "../lib/words.ts";

// MenuBarContent.swift's commands under the accounts, a null between two sections, each with what
// choosing it does on the page. A debug build has no Check for Updates….
export type Does =
  | "add"
  | "refresh"
  | "claude.ai"
  | "chatgpt.com"
  | "open"
  | "settings"
  | "updates"
  | "about"
  | "help"
  | "quit";
export const menuCommands: ({
  title: string;
  does: Does;
  sub?: string;
  keys?: string;
  more?: boolean;
} | null)[] = [
  { title: "Add Account…", does: "add", keys: "⌘N" },
  { title: "Refresh", does: "refresh", sub: updated(clock.read), keys: "⌘R" },
  null,
  { title: "Open claude.ai", does: "claude.ai", more: true },
  { title: "Open chatgpt.com", does: "chatgpt.com", more: true },
  null,
  { title: "Open Pitboard", does: "open" },
  { title: "Settings…", does: "settings", keys: "⌘," },
  { title: "Check for Updates…", does: "updates" },
  null,
  { title: "About Pitboard", does: "about" },
  { title: "Pitboard Help", does: "help" },
  { title: "Quit Pitboard", does: "quit", keys: "⌘Q" },
];

// The Claude Code session behind the menu, a terminal row a line; each desk sizes the elbow to its type.

const elbow = '<span class="elbow dim inline-block border-b border-l border-current"></span>';

export const session: string[] = [
  `<span class="dim">&gt; the parser test fails about one run in ten, find out why</span>`,
  ` `,
  `<span class="g">●</span> Search(pattern: "flush", path: "src")`,
  `  ${elbow} Found 4 files`,
  ` `,
  `<span class="g">●</span> Read(src/parser_test.rs)`,
  `  ${elbow} Read 142 lines`,
  ` `,
  `<span class="g">●</span> Read(src/writer.rs)`,
  `  ${elbow} Read 88 lines`,
  ` `,
  `● The test opens the log while the writer still holds`,
  `  its buffer, so the parser can read half a line. I will`,
  `  make the test wait until the writer has flushed.`,
  ` `,
  `<span class="g">●</span> Update(src/parser_test.rs)`,
  `  ${elbow} Updated src/parser_test.rs with 2 additions`,
  `      <span class="dim">41</span>     let log = writer.start();`,
  `      <span class="dim">42</span> <span class="g">+</span>   writer.finish();`,
  `      <span class="dim">43</span> <span class="g">+</span>   log.wait_flushed();`,
  `      <span class="dim">44</span>     let lines = parse(&amp;log.path);`,
  ` `,
  `<span class="g">●</span> Bash(cargo test parser)`,
  `  ${elbow} test result: ok. 48 passed; 0 failed`,
  ` `,
  `<span class="g">●</span> Bash(for i in $(seq 50); do cargo test -q parser; done)`,
  `  ${elbow} 50 of 50 runs passed`,
  ` `,
  `● The test now waits for the writer to flush before it`,
  `  parses the log. It passed 50 runs in a row.`,
  ` `,
];
