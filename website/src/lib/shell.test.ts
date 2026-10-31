import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { plainText } from "./changelog.ts";
import { readTree, snapshot } from "./clap.ts";
import {
  bashWords,
  cd,
  columns,
  cultureOrder,
  echo,
  edit,
  freshTab,
  history,
  keys,
  notFound,
  powershellWords,
  prompts,
  pwd,
  read,
  tab,
  wrapped,
  zshList,
  type Line,
  type Shell,
  type TabState,
} from "./shell.ts";

const repo = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
const facts = repo("docs/facts/site.md");
const tree = readTree(repo("crates/pitboard/src/snapshots/pitboard__tests__command_tree.snap"));
const windowsTree = readTree(
  repo("crates/pitboard/src/snapshots/pitboard__tests__command_tree_on_windows.snap"),
);
const text = (lines: string[]) => lines.map((l) => plainText(l).replaceAll("&gt;", ">"));

// One Tab after another, each from where the one before left the line.
function tabs(shell: Shell, typed: string, times: number) {
  let line: Line = { value: typed, caret: typed.length };
  let state: TabState = freshTab();
  const lists: string[][] = [];
  const lines: string[] = [];
  for (let i = 0; i < times; i++) {
    const done = tab(shell, shell === "powershell" ? windowsTree : tree, line, state);
    if (!done) {
      lines.push(line.value);
      state = freshTab();
      continue;
    }
    line = done.line;
    state = done.state;
    lines.push(line.value);
    if (done.list) lists.push(text(done.list));
  }
  return { lines, lists };
}

test("the prompts are the ones Term.astro draws, as question 9 keeps them", () => {
  const term = repo("website/src/components/fig/Term.astro");
  assert.ok(term.includes('<span class="text-mac-term-dim">%</span>'));
  assert.ok(term.includes('<span class="dim">$</span>'));
  assert.ok(term.includes('<span class="text-win-term-dim">PS C:\\\\Users\\\\me&gt;</span>'));
  assert.deepEqual(prompts, { zsh: "%", bash: "$", powershell: "PS C:\\Users\\me>" });
  assert.ok(facts.includes("whose\n  own prompt function writes `PS C:\\Users\\me>`"));
});

test("a word no shell finds is said as docs/facts/site.md records it", () => {
  assert.ok(facts.includes("- zsh: `zsh: command not found: foo`, exit 127."));
  assert.deepEqual(text(notFound("zsh", "foo", "foo", 0)), ["zsh: command not found: foo"]);
  assert.ok(facts.includes("- bash 3.2 and 5.3: `bash: foo: command not found`, exit 127."));
  assert.deepEqual(text(notFound("bash", "foo", "foo", 0)), ["bash: foo: command not found"]);
  const block =
    /Laid out 80\n {2}columns wide, as a line typed at the prompt runs:\n\n {2}```text\n([\s\S]*?)\n {2}```/.exec(
      facts,
    )![1];
  const recorded = block.split("\n").map((l) => l.slice(2));
  const lines = text(notFound("powershell", "foo", "foo", 0));
  // each wrapped line keeps the space it wrapped at, and a line of one space follows
  assert.deepEqual(
    lines.map((l) => l.trimEnd()),
    [...recorded, ""],
  );
  assert.ok(lines[0].endsWith(" ") && lines.at(-1) === " ");
  assert.ok(facts.includes("the\n  `~` run is as long as the word typed"));
  const longer = text(notFound("powershell", "pitboardd", "pitboardd use", 0));
  assert.deepEqual(longer.slice(4, 6), ["+ pitboardd use", "+ ~~~~~~~~~"]);
});

test("the console wraps at the last space that fits, which stays on its line", () => {
  assert.deepEqual(wrapped("aaa bbb ccc", 8), ["aaa bbb ", "ccc"]);
  assert.deepEqual(wrapped("abcdefghij", 4), ["abcd", "efgh", "ij"]);
});

test("zsh completes as site.md measured it with the shipped _pitboard", () => {
  assert.ok(facts.includes("`pitboard us` gives `pitboard use `"));
  assert.deepEqual(tabs("zsh", "pitboard us", 1).lines, ["pitboard use "]);
  assert.ok(
    facts.includes(
      "`pitboard u` lists `uninstall` then `use`; `pitboard s` lists `schedule`,\n  `status`, `statusline`, `stow`.",
    ),
  );
  const u = tabs("zsh", "pitboard u", 3);
  assert.deepEqual(u.lines, ["pitboard u", "pitboard uninstall ", "pitboard use "]);
  assert.deepEqual(
    u.lists[0].map((l) => l.split(" ")[0]),
    ["uninstall", "use"],
  );
  assert.ok(u.lists[0].every((l) => l.length <= 76));
  assert.deepEqual(
    tabs("zsh", "pitboard s", 1).lists[0].map((l) => l.split(" ")[0]),
    ["schedule", "status", "statusline", "stow"],
  );
  assert.ok(
    facts.includes(
      "`pitboard -` lists three lines, aligned: `--help     -h  -- Print help`,\n  `--json         -- Print JSON instead of text` and `--version  -V  -- Print version`.",
    ),
  );
  assert.deepEqual(tabs("zsh", "pitboard -", 1).lists[0], [
    "--help     -h  -- Print help",
    "--json         -- Print JSON instead of text",
    "--version  -V  -- Print version",
  ]);
  // `:label …:_default` completes files, and the page draws none
  assert.deepEqual(tabs("zsh", "pitboard use ", 1).lines, ["pitboard use "]);
});

test("bash completes as site.md measured it with readline 8.3 and the shipped script", () => {
  assert.ok(
    facts.includes(
      "`pitboard u` lists `use  uninstall`; `pitboard -` lists\n  `-h  -V  --json  --help  --version`.",
    ),
  );
  assert.deepEqual(tabs("bash", "pitboard u", 2).lists, [["use        uninstall"]]);
  assert.deepEqual(tabs("bash", "pitboard -", 2).lists, [
    ["-h         -V         --json     --help     --version"],
  ]);
  assert.ok(
    facts.includes(
      "`pitboard use `\n  gives `-`, the second Tab rings, and the third lists `-y  -h  --yes  --json  --help`.",
    ),
  );
  const use = tabs("bash", "pitboard use ", 3);
  assert.deepEqual(use.lines, ["pitboard use -", "pitboard use -", "pitboard use -"]);
  assert.deepEqual(use.lists, [["-y      -h      --yes   --json  --help"]]);
  // the script's words for each command, which `Cli::command()` makes without what `parser()`
  // adds, all of it hidden
  const opts = [
    ...repo("crates/pitboard/src/snapshots/pitboard__tests__completions_bash.snap").matchAll(
      /opts="([^"]*)"/g,
    ),
  ].map((m) => m[1]);
  const shown = (c: typeof tree): typeof tree => ({
    ...c,
    arguments: c.arguments.filter((a) => !a.hidden),
    commands: c.commands.filter((s) => !s.hidden).map(shown),
  });
  const ours: string[] = [];
  const walk = (c: typeof tree) => {
    ours.push(bashWords(c).join(" "));
    c.commands.forEach(walk);
  };
  walk(shown(tree));
  assert.equal(opts[1], ours[0]);
  assert.deepEqual(new Set(opts.slice(1)), new Set(ours));
});

test("PowerShell offers the shipped script's words in Sort-Object's order, one a Tab", () => {
  const order = /- `pitboard ` offers `([^`]*)`\./
    .exec(facts.replace(/\n {2}/g, " "))![1]
    .split(" ");
  const words = powershellWords({
    ...windowsTree,
    arguments: windowsTree.arguments.filter((a) => !a.hidden),
    commands: windowsTree.commands.filter((s) => !s.hidden),
  }).map((c) => c.text);
  assert.deepEqual([...words].sort(cultureOrder), order);
  assert.ok(
    facts.includes(
      "`pitboard u` offers `uninstall use`; `pitboard s` offers `schedule status statusline stow`.",
    ),
  );
  assert.deepEqual(tabs("powershell", "pitboard u", 3).lines, [
    "pitboard uninstall",
    "pitboard use",
    "pitboard uninstall",
  ]);
  assert.ok(facts.includes("- `pitboard use ` offers `-h --help --json -y --yes`;"));
  assert.deepEqual(
    tabs("powershell", "pitboard use ", 5).lines.map((l) => l.slice(13)),
    ["-h", "--help", "--json", "-y", "--yes"],
  );
  assert.ok(facts.includes("- `pitboard -` offers nothing,"));
  assert.deepEqual(tabs("powershell", "pitboard -", 1).lines, ["pitboard -"]);
  const script = repo(
    "crates/pitboard/src/snapshots/pitboard__tests__completions_powershell_on_windows.snap",
  );
  const block = /'pitboard;watch' \{([\s\S]*?)break/.exec(script)![1];
  const named = [...block.matchAll(/\]::new\('([^']+)'/g)].map((m) => m[1]);
  const watch = windowsTree.commands.find((c) => c.name === "watch")!;
  assert.deepEqual(
    powershellWords({ ...watch, arguments: watch.arguments.filter((a) => !a.hidden) }).map(
      (c) => c.text,
    ),
    named,
  );
  assert.ok(script.includes("Sort-Object -Property ListItemText"));
});

test("the command's own name completes, as each shell completes a program on its PATH", () => {
  assert.deepEqual(tabs("zsh", "pit", 1).lines, ["pitboard "]);
  assert.deepEqual(tabs("bash", "pitb", 1).lines, ["pitboard "]);
  assert.deepEqual(tabs("powershell", "pit", 1).lines, ["pitboard.exe"]);
  assert.deepEqual(tabs("zsh", "pi", 1).lines, ["pi"]);
});

test("the line-editing keys are the ones site.md read from each shell", () => {
  const table = facts.split("## Keys at the prompt")[1].split("\n\n")[1];
  const rows = Object.fromEntries(
    table
      .split("\n")
      .slice(2)
      .map((r) =>
        r
          .split("|")
          .slice(1, -1)
          .map((c) => c.trim()),
      )
      .map(([key, ...cells]) => [key, cells]),
  );
  const said = (cell: string) => cell.replace(/`/g, "");
  const named: Record<string, string> = {
    "beginning-of-line": "beginning-of-line",
    "end-of-line": "end-of-line",
    "backward-char": "backward-char",
    "forward-char": "forward-char",
    "kill-line": "kill-line",
    "kill-whole-line": "kill-whole-line",
    "unix-line-discard": "unix-line-discard",
    "backward-kill-word": "backward-kill-word",
    "unix-word-rubout": "unix-word-rubout",
    yank: "yank",
    BackwardKillWord: "backward-kill-word",
    Redo: "redo",
    "Ctrl+Z Undo": "undo",
    "Ctrl+_ undo": "undo",
    "clear-screen": "clear-screen",
    ClearScreen: "clear-screen",
  };
  const shells: Shell[] = ["zsh", "bash", "powershell"];
  for (const [key, letters] of [
    ["Ctrl+A, Ctrl+E", ["a", "e"]],
    ["Ctrl+B, Ctrl+F", ["b", "f"]],
    ["Ctrl+K", ["k"]],
    ["Ctrl+U", ["u"]],
    ["Ctrl+W", ["w"]],
    ["Ctrl+Y", ["y"]],
    ["Ctrl+L", ["l"]],
  ] as const) {
    const cells = rows[key] ?? rows[key.split(",")[0]];
    for (const [i, shell] of shells.entries()) {
      const cell = said(cells[i]);
      const bound = letters.map((l) => keys[shell][l]).filter(Boolean);
      if (cell === "nothing" || cell === "SelectAll" || cell === "")
        assert.ok(bound.length === 0, `${shell} ${key}`);
      else
        for (const name of cell.split(/, |, the same| /).filter((n) => named[n]))
          assert.ok(bound.includes(named[name] as never), `${shell} ${key} ${name}`);
    }
  }
  assert.equal(keys.zsh._, "undo");
  assert.equal(keys.powershell.z, "undo");
  // Ctrl+R's search line is not drawn: the key is the browser's
  for (const shell of shells) assert.equal(keys[shell].r, undefined);
});

test("zsh and bash kill a word and a line each their own way", () => {
  const line = { value: "pitboard use codex/main", caret: 23 };
  assert.equal(edit("zsh", "backward-kill-word", line, "")!.line.value, "pitboard use ");
  assert.equal(edit("bash", "unix-word-rubout", line, "")!.line.value, "pitboard use ");
  assert.equal(
    edit("powershell", "backward-kill-word", line, "")!.line.value,
    "pitboard use codex/",
  );
  const middle = { value: "pitboard use work", caret: 9 };
  assert.equal(edit("zsh", "kill-whole-line", middle, "")!.line.value, "");
  assert.deepEqual(edit("bash", "unix-line-discard", middle, "")!, {
    line: { value: "use work", caret: 0 },
    killed: "pitboard ",
  });
  assert.equal(edit("bash", "yank", { value: "", caret: 0 }, "pitboard ")!.line.value, "pitboard ");
});

test("history lists the lines as each shell does: zsh before it, bash with it, PowerShell a table", () => {
  assert.deepEqual(text(history("zsh", ["pitboard", "pitboard log"])), [
    "    1  pitboard",
    "    2  pitboard log",
  ]);
  assert.deepEqual(text(history("powershell", ["pitboard"])), [
    "",
    "  Id CommandLine",
    "  -- -----------",
    "   1 pitboard",
    "",
    "",
  ]);
});

test("a line reads as its shell reads it: quotes, variables, the home and its operators", () => {
  const words = (shell: Shell, line: string) => {
    const r = read(shell, line, "/Users/me");
    return r.kind === "commands" ? r.list.map((c) => c.pipeline.map((p) => p.words)) : r;
  };
  assert.deepEqual(words("zsh", `pitboard rename "my work" 'x y' a\\ b`), [
    [["pitboard", "rename", "my work", "x y", "a b"]],
  ]);
  assert.deepEqual(words("bash", "echo $HOME ~ $NOPE; pitboard && pwd"), [
    [["echo", "/home/me", "/home/me"]],
    [["pitboard"]],
    [["pwd"]],
  ]);
  assert.deepEqual(words("powershell", 'echo $env:USERPROFILE "a`"b"'), [
    [["echo", "C:\\Users\\me", 'a"b']],
  ]);
  assert.deepEqual(read("zsh", 'echo "open', "/"), { kind: "open", quote: '"' });
  assert.deepEqual(words("bash", "pitboard log | head > out.txt"), [
    [["pitboard", "log"], ["head"]],
  ]);
});

test("cd knows the home, the folder holding it and the root, in each shell's words", () => {
  assert.deepEqual(cd("zsh", "/Users/me", [".."], "cd ..", 0), { cwd: "/Users" });
  assert.deepEqual(
    text((cd("zsh", "/Users/me", ["foo"], "cd foo", 0) as { lines: string[] }).lines),
    ["cd: no such file or directory: foo"],
  );
  assert.deepEqual(
    text((cd("bash", "/home/me", ["foo"], "cd foo", 0) as { lines: string[] }).lines),
    ["bash: cd: foo: No such file or directory"],
  );
  assert.deepEqual(
    text((cd("bash", "/home/me", ["a", "b"], "cd a b", 0) as { lines: string[] }).lines),
    ["bash: cd: too many arguments"],
  );
  assert.deepEqual(cd("powershell", "C:\\Users\\me", [".."], "cd ..", 0), { cwd: "C:\\Users" });
  const refused = text(
    (cd("powershell", "C:\\Users\\me", ["foo"], "cd foo", 0) as { lines: string[] }).lines,
  );
  assert.equal(refused[0], "cd : Cannot find path 'C:\\Users\\me\\foo' because it does not exist.");
  assert.equal(refused[3], "+ ~~~~~~");
  assert.deepEqual(text(pwd("powershell", "C:\\Users\\me")), [
    "",
    "Path",
    "----",
    "C:\\Users\\me",
    "",
    "",
  ]);
  assert.deepEqual(echo("powershell", ["a", "b"]).lines, ["a", "b"]);
  assert.deepEqual(echo("zsh", ["-n", "a", "b"]), { lines: ["a b"], open: true });
});

test("columns lay matches down columns, each as wide as the widest and two spaces", () => {
  assert.deepEqual(text(columns(["a", "bbb", "cc"], 12)), ["a    cc", "bbb"]);
  assert.deepEqual(text(zshList([{ text: "x", about: null, flag: false }])), ["x"]);
  void snapshot;
});
