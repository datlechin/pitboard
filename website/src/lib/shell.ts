// The shells the site's terminals run, as docs/facts/site.md records them: zsh 5.9 in Terminal on
// macOS, bash in Ptyxis on Linux, Windows PowerShell 5.1 in Windows Terminal. Each reads a line,
// runs `pitboard` and its own built-ins, says what it cannot find, completes with Tab as the
// shipped completion scripts do, and edits the line with its own keys.
import type { Arg, Command } from "./clap.ts";
import { escaped } from "./words.ts";
import type { Platform } from "./platform.ts";

export type Shell = "zsh" | "bash" | "powershell";
export const shellOn: Record<Platform, Shell> = {
  macos: "zsh",
  linux: "bash",
  windows: "powershell",
};

// The prompts the figures draw (question 9: the person's own setting, kept as drawn).
export const prompts: Record<Shell, string> = {
  zsh: "%",
  bash: "$",
  powershell: "PS C:\\Users\\me>",
};

export const homes: Record<Shell, string> = {
  zsh: "/Users/me",
  bash: "/home/me",
  powershell: "C:\\Users\\me",
};

// The folders the page knows: the home, the folder holding it and the root. Nothing else is
// drawn, so every other name is one the shell cannot find.
function folders(shell: Shell): string[] {
  const home = homes[shell];
  return shell === "powershell"
    ? [home, "C:\\Users", "C:\\"]
    : [home, home.slice(0, home.lastIndexOf("/")), "/"];
}

const plain = escaped;
const brightRed = (text: string) => `<span class="br">${escaped(text)}</span>`;

// A line as each shell reads it: words, the operators between commands, and quotes left open.
export interface Simple {
  words: string[];
  // where a word starts in the line, for PowerShell's error records
  at: number[];
  // output sent to a file the page does not draw
  redirected: boolean;
}

export type Read =
  | { kind: "commands"; list: { after: string | null; pipeline: Simple[] }[] }
  | { kind: "open"; quote: "'" | '"' };

const ENV: Record<Shell, Record<string, string>> = {
  zsh: { HOME: "/Users/me", USER: "me", SHELL: "/bin/zsh" },
  bash: { HOME: "/home/me", USER: "me", SHELL: "/bin/bash" },
  powershell: { HOME: "C:\\Users\\me" },
};

export function read(shell: Shell, line: string, cwd: string): Read {
  const list: { after: string | null; pipeline: Simple[] }[] = [];
  let pipeline: Simple[] = [];
  let simple: Simple = { words: [], at: [], redirected: false };
  let word: string | null = null;
  let start = 0;
  let redirect = false;
  const vars = { ...ENV[shell], PWD: cwd };
  const end = () => {
    if (word === null) return;
    if (redirect) {
      simple.redirected = true;
      redirect = false;
    } else {
      simple.words.push(word);
      simple.at.push(start);
    }
    word = null;
  };
  const endSimple = () => {
    end();
    if (simple.words.length || simple.redirected) pipeline.push(simple);
    simple = { words: [], at: [], redirected: false };
  };
  const endPipeline = (op: string | null) => {
    endSimple();
    if (pipeline.length) list.push({ after: op, pipeline });
    pipeline = [];
  };
  const add = (text: string, at: number) => {
    if (word === null) {
      word = "";
      start = at;
    }
    word += text;
  };
  // $NAME, ${NAME}, and in PowerShell $env:NAME and $HOME
  const variable = (i: number): [string, number] => {
    const rest = line.slice(i + 1);
    const m =
      shell === "powershell" ? /^(?:env:)?(\w+)/i.exec(rest) : /^(?:\{(\w+)\}|(\w+))/.exec(rest);
    if (!m) return ["$", i + 1];
    const name = m[1] ?? m[2];
    const found =
      shell === "powershell"
        ? Object.entries({ ...vars, USERPROFILE: homes.powershell, USERNAME: "me" }).find(
            ([k]) => k.toLowerCase() === name.toLowerCase(),
          )?.[1]
        : vars[name as keyof typeof vars];
    return [found ?? "", i + 1 + m[0].length];
  };
  const escape = shell === "powershell" ? "`" : "\\";
  let i = 0;
  while (i < line.length) {
    const c = line[i];
    if (c === " " || c === "\t") {
      end();
      i++;
      continue;
    }
    if (c === "'") {
      const close = line.indexOf("'", i + 1);
      if (close < 0) return { kind: "open", quote: "'" };
      add(line.slice(i + 1, close), i);
      if (word === null) word = "";
      i = close + 1;
      continue;
    }
    if (c === '"') {
      let j = i + 1;
      let text = "";
      for (; j < line.length && line[j] !== '"'; j++) {
        if (line[j] === escape && j + 1 < line.length) {
          const next = line[j + 1];
          if (shell === "powershell" || '"\\$`'.includes(next)) {
            text += next;
            j++;
            continue;
          }
        }
        if (line[j] === "$") {
          const [value, after] = variable(j);
          text += value;
          j = after - 1;
          continue;
        }
        text += line[j];
      }
      if (j >= line.length) return { kind: "open", quote: '"' };
      if (word === null) {
        word = "";
        start = i;
      }
      word += text;
      i = j + 1;
      continue;
    }
    if (c === escape && i + 1 < line.length) {
      add(line[i + 1], i);
      i += 2;
      continue;
    }
    if (c === "$") {
      const [value, after] = variable(i);
      if (value || word !== null) add(value, i);
      i = after;
      continue;
    }
    if (
      c === "~" &&
      word === null &&
      shell !== "powershell" &&
      /^(\/|\s|$)/.test(line.slice(i + 1, i + 2) || " ")
    ) {
      add(homes[shell], i);
      i++;
      continue;
    }
    if (c === "#" && word === null && shell !== "zsh") break;
    if (c === ";") {
      endPipeline(";");
      i++;
      continue;
    }
    if (c === "&" && line[i + 1] === "&" && shell !== "powershell") {
      endPipeline("&&");
      i += 2;
      continue;
    }
    if (c === "|" && line[i + 1] === "|" && shell !== "powershell") {
      endPipeline("||");
      i += 2;
      continue;
    }
    if (c === "|") {
      endSimple();
      i++;
      continue;
    }
    if (c === ">" || (c === "2" && line[i + 1] === ">" && word === null)) {
      end();
      i += c === "2" ? 2 : 1;
      if (line[i] === ">") i++;
      if (line[i] === "&") {
        i++;
        while (i < line.length && /\d/.test(line[i])) i++;
        continue;
      }
      redirect = true;
      continue;
    }
    add(c, i);
    i++;
  }
  endPipeline(null);
  return { kind: "commands", list };
}

// What zsh and bash print where a line leaves a quote open, before the next line is read.
export const continuation: Record<Shell, (quote: "'" | '"') => string> = {
  zsh: (q) => (q === "'" ? "quote> " : "dquote> "),
  bash: () => "> ",
  powershell: () => ">> ",
};

// PowerShell names commands without regard to case; zsh and bash do not.
export const named = (shell: Shell, word: string, name: string) =>
  shell === "powershell" ? word.toLowerCase() === name.toLowerCase() : word === name;

// `pitboard`, and in PowerShell `pitboard.exe` as well.
export const isPitboard = (shell: Shell, word: string) =>
  named(shell, word, "pitboard") || (shell === "powershell" && named(shell, word, "pitboard.exe"));

// A word as each shell says it found no command by that name (docs/facts/site.md).
export function notFound(shell: Shell, word: string, line: string, at: number): string[] {
  if (shell === "zsh") return [plain(`zsh: command not found: ${word}`)];
  if (shell === "bash") return [plain(`bash: ${word}: command not found`)];
  return errorRecord({
    message: `${word} : The term '${word}' is not recognized as the name of a cmdlet, function, script file, or operable program. Check the spelling of the name, or if a path was included, verify that the path is correct and try again.`,
    line,
    at,
    length: word.length,
    category: `ObjectNotFound: (${word}:String) [], CommandNotFoundException`,
    id: "CommandNotFoundException",
  });
}

// Windows PowerShell 5.1's error record in NormalView, red on black, each line wrapped at the
// window's 80 columns with the space it wrapped at kept, and a line of one space after it.
export function errorRecord(e: {
  message: string;
  line: string;
  at: number;
  length: number;
  category: string;
  id: string;
}): string[] {
  const lines = [
    e.message,
    `At line:1 char:${e.at + 1}`,
    `+ ${e.line}`,
    `+ ${" ".repeat(e.at)}${"~".repeat(e.length)}`,
    `    + CategoryInfo          : ${e.category}`,
    `    + FullyQualifiedErrorId : ${e.id}`,
  ];
  return [...lines.flatMap((l) => wrapped(l, 80)), " "].map(brightRed);
}

// The console's wrap: at the last space that fits, which stays on the line it ends.
export function wrapped(text: string, width: number): string[] {
  const out: string[] = [];
  let rest = text;
  while (rest.length > width) {
    const cut = rest.lastIndexOf(" ", width - 1);
    const at = cut > 0 ? cut + 1 : width;
    out.push(rest.slice(0, at));
    rest = rest.slice(at);
  }
  out.push(rest);
  return out;
}

// `history`: zsh lists the lines before it, bash itself too, PowerShell a table of them.
export function history(shell: Shell, lines: string[]): string[] {
  if (shell === "powershell") {
    const id = Math.max(2, String(lines.length).length);
    const width = Math.max(4, id + 2);
    return [
      "",
      `${"Id".padStart(width)} CommandLine`,
      `${"--".padStart(width)} -----------`,
      ...lines.map((l, i) => `${String(i + 1).padStart(width)} ${l}`),
      "",
      "",
    ].map(plain);
  }
  return lines.map((l, i) => plain(`${String(i + 1).padStart(5)}  ${l}`));
}

// `pwd`, and PowerShell's Get-Location table.
export const pwd = (shell: Shell, cwd: string) =>
  shell === "powershell" ? ["", "Path", "----", plain(cwd), "", ""] : [plain(cwd)];

// `echo`: zsh and bash join their words, with -n leaving the line open; PowerShell writes each
// word as an object of its own.
export function echo(shell: Shell, words: string[]): { lines: string[]; open: boolean } {
  if (shell === "powershell") return { lines: words.map(plain), open: false };
  let open = false;
  let rest = words;
  while (rest[0] === "-n" || rest[0] === "-e" || rest[0] === "-E") {
    if (rest[0] === "-n") open = true;
    rest = rest.slice(1);
  }
  return { lines: [plain(rest.join(" "))], open };
}

// `cd`: the home, the folder holding it and the root are the folders there are.
export function cd(
  shell: Shell,
  cwd: string,
  args: string[],
  line: string,
  at: number,
): { cwd: string } | { lines: string[] } {
  const known = folders(shell);
  const home = homes[shell];
  const sep = shell === "powershell" ? "\\" : "/";
  if (shell === "bash" && args.length > 1)
    return { lines: [plain("bash: cd: too many arguments")] };
  let target = args[0];
  if (shell === "zsh" && args.length === 2) {
    if (!cwd.includes(args[0])) return { lines: [plain(`cd: string not in pwd: ${args[0]}`)] };
    target = cwd.replace(args[0], args[1]);
  }
  if (shell === "zsh" && args.length > 2) return { lines: [plain("cd: too many arguments")] };
  if (target === undefined || target === "~") return { cwd: shell === "powershell" ? cwd : home };
  const parts =
    target.startsWith(sep) || /^[A-Za-z]:/.test(target) ? [] : cwd.split(sep).filter(Boolean);
  const root = shell === "powershell" ? "C:" : "";
  for (const part of target
    .replace(/^[A-Za-z]:/, "")
    .split(/[\\/]/)
    .filter(Boolean)) {
    if (part === "..") parts.pop();
    else if (part !== ".") parts.push(part);
  }
  if (shell === "powershell" && parts[0]?.toUpperCase() === "C:") parts.shift();
  const path = shell === "powershell" ? `${root}\\${parts.join("\\")}` : `/${parts.join("/")}`;
  const found = known.find((k) =>
    shell === "powershell" ? k.toLowerCase() === path.toLowerCase() : k === path,
  );
  if (found) return { cwd: found };
  if (shell === "zsh") return { lines: [plain(`cd: no such file or directory: ${target}`)] };
  if (shell === "bash") return { lines: [plain(`bash: cd: ${target}: No such file or directory`)] };
  return {
    lines: errorRecord({
      message: `cd : Cannot find path '${path}' because it does not exist.`,
      line,
      at,
      length: line.trimEnd().length - at,
      category: `ObjectNotFound: (${path}:String) [Set-Location], ItemNotFoundException`,
      id: "PathNotFound,Microsoft.PowerShell.Commands.SetLocationCommand",
    }),
  };
}

// The built-ins the page answers (question 8): clearing the window, the line history, echo,
// pwd and cd, and PowerShell's aliases of its cmdlets for them.
export type Builtin = "clear" | "history" | "echo" | "pwd" | "cd" | "exit";
const builtins: Record<Shell, Record<string, Builtin>> = {
  zsh: {
    clear: "clear",
    history: "history",
    echo: "echo",
    pwd: "pwd",
    cd: "cd",
    exit: "exit",
    logout: "exit",
  },
  bash: {
    clear: "clear",
    history: "history",
    echo: "echo",
    pwd: "pwd",
    cd: "cd",
    exit: "exit",
    logout: "exit",
  },
  powershell: {
    clear: "clear",
    cls: "clear",
    "clear-host": "clear",
    history: "history",
    h: "history",
    ghy: "history",
    "get-history": "history",
    echo: "echo",
    write: "echo",
    "write-output": "echo",
    pwd: "pwd",
    gl: "pwd",
    "get-location": "pwd",
    cd: "cd",
    chdir: "cd",
    sl: "cd",
    "set-location": "cd",
    exit: "exit",
  },
};

export function builtin(shell: Shell, word: string): Builtin | null {
  const key = shell === "powershell" ? word.toLowerCase() : word;
  return builtins[shell][key] ?? null;
}

// Ctrl+C at the prompt: zsh leaves the line as typed, bash adds ^C, PowerShell's CancelLine a
// bright red ^C (PSReadLine BasicEditing.cs). Each starts a new prompt.
export const interrupted = (shell: Shell) =>
  shell === "zsh" ? "" : shell === "bash" ? "^C" : brightRed("^C");

// Ctrl+C while a program runs: the terminal echoes ^C, and PowerShell's console does not.
export const stopped = (shell: Shell) => (shell === "powershell" ? null : "^C");

// ---- Completion, as the shipped scripts complete in each shell ----

export interface Candidate {
  text: string;
  about: string | null;
  // a flag, which zsh offers once a word starts with `-`
  flag: boolean;
}

// The tree a release's completions are generated from: `Cli::command()`, without what
// `parser()` adds, which is everything hidden.
const visible = (c: Command): Command => ({
  ...c,
  arguments: c.arguments.filter((a) => !a.hidden),
  commands: c.commands.filter((s) => !s.hidden).map(visible),
});

const takesValue = (a: Arg) => a.num_args !== "0" && a.index === null && a.action === "Set";

// The command the words before the cursor name, as each script finds it.
function walk(
  tree: Command,
  words: string[],
  stopAtFlags: boolean,
): { at: Command; path: string[] } {
  let at = tree;
  const path: string[] = [];
  for (const w of words) {
    if (stopAtFlags && w.startsWith("-")) break;
    const sub = at.commands.find((s) => s.name === w);
    if (sub) {
      at = sub;
      path.push(w);
    } else if (stopAtFlags) break;
  }
  return { at, path };
}

// bash.rs `all_options_for_path`: shorts, longs, a positional's values, then subcommands.
export function bashWords(c: Command): string[] {
  const out: string[] = [];
  for (const a of c.arguments) if (a.index === null && a.short) out.push(`-${a.short}`);
  for (const a of c.arguments) if (a.index === null && a.long) out.push(`--${a.long}`);
  for (const a of c.arguments) if (a.index !== null) out.push(...a.possible_values);
  out.push(...c.commands.map((s) => s.name));
  return out;
}

// powershell.rs: each flag and subcommand, sorted by Sort-Object, which compares by culture:
// the hyphens are passed over, case is not minded, and the fewer hyphens of two alike go first.
export function powershellWords(c: Command): Candidate[] {
  const out: Candidate[] = [];
  for (const a of c.arguments) {
    if (a.index !== null) continue;
    if (a.short) out.push({ text: `-${a.short}`, about: a.help, flag: true });
    if (a.long) out.push({ text: `--${a.long}`, about: a.help, flag: true });
  }
  for (const s of c.commands) out.push({ text: s.name, about: s.about, flag: false });
  return out;
}

export function cultureOrder(a: string, b: string): number {
  const key = (s: string) => s.replace(/-/g, "").toLowerCase();
  const hyphens = (s: string) => s.length - s.replace(/-/g, "").length;
  const ka = key(a);
  const kb = key(b);
  if (ka !== kb) return ka < kb ? -1 : 1;
  return hyphens(a) - hyphens(b) || (a < b ? -1 : a > b ? 1 : 0);
}

// zsh.rs: at a command's place its subcommands with what each does; a word starting `-` its
// options, the long and short of one beside each other; a positional's values; files otherwise.
function zshCandidates(
  c: Command,
  current: string,
  given: string[],
  afterCommand: boolean,
): Candidate[] {
  if (current.startsWith("-")) {
    const out: Candidate[] = [];
    for (const a of c.arguments) {
      if (a.index !== null) continue;
      const spelled = [a.short && `-${a.short}`, a.long && `--${a.long}`].filter(
        Boolean,
      ) as string[];
      if (spelled.some((s) => given.includes(s))) continue;
      for (const s of spelled) out.push({ text: s, about: a.help, flag: true });
    }
    return out;
  }
  if (c.commands.length > 0 && !afterCommand)
    return c.commands.map((s) => ({ text: s.name, about: s.about, flag: false }));
  const positional = c.arguments.find((a) => a.index !== null && a.possible_values.length > 0);
  if (positional && !given.some((g) => positional.possible_values.includes(g)))
    return positional.possible_values.map((v) => ({ text: v, about: null, flag: false }));
  return [];
}

export interface Line {
  value: string;
  caret: number;
}

// Where a Tab leaves the line, what it lists below it, and what the next Tab does.
export interface Tabbed {
  line: Line;
  list: string[] | null;
  // zsh's list stays below the prompt; bash's goes above the line it draws again
  listAbove: boolean;
  state: TabState;
}

export interface TabState {
  // the line as the last Tab left it, so the next one knows it follows
  after: string | null;
  // zsh: listed, then cycling; bash: nothing changed last time; PowerShell: cycling
  stage: "none" | "listed" | "cycling" | "unchanged";
  matches: string[];
  index: number;
  // the word being completed, from where it starts
  from: number;
  typed: string;
}

export const freshTab = (): TabState => ({
  after: null,
  stage: "none",
  matches: [],
  index: 0,
  from: 0,
  typed: "",
});

// The word under the caret and the words before it, split at spaces as a completer sees them.
function words(line: Line) {
  const before = line.value.slice(0, line.caret);
  const from = before.search(/\S*$/);
  const all = before.slice(0, from).trim().split(/\s+/).filter(Boolean);
  return { from, current: before.slice(from), before: all };
}

const replace = (line: Line, from: number, text: string): Line => {
  const value = line.value.slice(0, from) + text + line.value.slice(line.caret);
  return { value, caret: from + text.length };
};

const sharedPrefix = (items: string[]) =>
  items.reduce((p, s) => {
    let i = 0;
    while (i < p.length && i < s.length && p[i] === s[i]) i++;
    return p.slice(0, i);
  });

// zsh's list: each match as `name  -- about`, cut to end at the 76th column, or, with no about,
// in columns; matches with one about side by side.
export function zshList(matches: Candidate[], width = 80): string[] {
  if (matches.every((m) => m.about === null))
    return columns(
      matches.map((m) => m.text),
      width,
    );
  const groups: { names: string[]; about: string }[] = [];
  for (const m of [...matches].sort((a, b) => (a.text < b.text ? -1 : 1))) {
    const g = groups.find((x) => x.about === m.about);
    if (g) g.names.push(m.text);
    else groups.push({ names: [m.text], about: m.about ?? "" });
  }
  const most = Math.max(...groups.map((g) => g.names.length));
  const widths = [...Array(most).keys()].map((i) =>
    Math.max(...groups.map((g) => g.names[i]?.length ?? 0)),
  );
  return groups.map((g) => {
    const names = widths.map((w, i) => (g.names[i] ?? "").padEnd(w)).join("  ");
    return plain(`${names}  -- ${g.about}`.slice(0, width - 4));
  });
}

// readline's list: down columns, each as wide as the widest match and two spaces.
export function columns(items: string[], width = 80): string[] {
  const wide = Math.max(...items.map((i) => i.length)) + 2;
  const cols = Math.max(1, Math.floor(width / wide));
  const rows = Math.ceil(items.length / cols);
  const out: string[] = [];
  for (let r = 0; r < rows; r++) {
    const row: string[] = [];
    for (let c = 0; c < cols; c++) {
      const item = items[c * rows + r];
      if (item !== undefined) row.push(item);
    }
    out.push(plain(row.map((item, i) => (i < row.length - 1 ? item.padEnd(wide) : item)).join("")));
  }
  return out;
}

// One Tab, in `shell`, with the command tree its completion script was made from.
// departs from the apps: The bell is silent
export function tab(shell: Shell, tree: Command, line: Line, state: TabState): Tabbed | null {
  const t = visible(tree);
  const w = words(line);
  const follows = state.after === line.value;
  const first = w.before.length === 0;
  // the command's own name, which only `pitboard` among the commands drawn begins so
  if (first) {
    const name = shell === "powershell" ? "pitboard.exe" : "pitboard";
    if (w.current.length < 3 || !name.startsWith(w.current.toLowerCase())) return null;
    const next = replace(line, w.from, shell === "powershell" ? name : `${name} `);
    return {
      line: next,
      list: null,
      listAbove: false,
      state: { ...freshTab(), after: next.value },
    };
  }
  if (!isPitboard(shell, w.before[0])) return null;
  const rest = w.before.slice(1);

  if (shell === "powershell") {
    // powershell.rs: the command is the words up to the first flag or the word being completed
    if (follows && state.stage === "cycling" && state.matches.length > 0) {
      const index = (state.index + 1) % state.matches.length;
      const next = replace(line, state.from, state.matches[index]);
      return {
        line: next,
        list: null,
        listAbove: false,
        state: { ...state, index, after: next.value },
      };
    }
    // Windows PowerShell 5.1 hands no word starting `-` to a native command's completer
    if (w.current.startsWith("-")) return null;
    const { at } = walk(t, rest, true);
    const matches = powershellWords(at)
      .filter((c) => c.text.toLowerCase().startsWith(w.current.toLowerCase()))
      .map((c) => c.text)
      .sort(cultureOrder);
    if (matches.length === 0) return null;
    const next = replace(line, w.from, matches[0]);
    return {
      line: next,
      list: null,
      listAbove: false,
      state: {
        after: next.value,
        stage: "cycling",
        matches,
        index: 0,
        from: w.from,
        typed: w.current,
      },
    };
  }

  if (shell === "bash") {
    // bash.rs: the subcommand the words name; after an option taking a value, file names
    const { at } = walk(t, rest, false);
    const prev = w.before.at(-1)!;
    const valued = at.arguments.find(
      (a) => takesValue(a) && (prev === `--${a.long}` || prev === `-${a.short}`),
    );
    const matches = valued ? [] : bashWords(at).filter((x) => x.startsWith(w.current));
    if (matches.length === 0) return null;
    if (matches.length === 1) {
      const next = replace(line, w.from, `${matches[0]} `);
      return {
        line: next,
        list: null,
        listAbove: false,
        state: { ...freshTab(), after: next.value },
      };
    }
    const prefix = sharedPrefix(matches);
    if (prefix.length > w.current.length) {
      const next = replace(line, w.from, prefix);
      return {
        line: next,
        list: null,
        listAbove: false,
        state: { ...freshTab(), after: next.value },
      };
    }
    if (follows && state.stage === "unchanged")
      return {
        line,
        list: columns(matches),
        listAbove: true,
        state: { ...state, after: line.value },
      };
    return {
      line,
      list: null,
      listAbove: false,
      state: { ...freshTab(), stage: "unchanged", after: line.value },
    };
  }

  // zsh
  if (follows && state.stage === "cycling") {
    const index = (state.index + 1) % state.matches.length;
    const next = replace(line, state.from, `${state.matches[index]} `);
    return {
      line: next,
      list: null,
      listAbove: false,
      state: { ...state, index, after: next.value },
    };
  }
  if (follows && state.stage === "listed") {
    const next = replace(line, state.from, `${state.matches[0]} `);
    return {
      line: next,
      list: null,
      listAbove: false,
      state: { ...state, stage: "cycling", index: 0, after: next.value },
    };
  }
  const { at, path } = walk(t, rest, false);
  const afterCommand = rest.length > path.length || at.commands.length === 0;
  const all = zshCandidates(at, w.current, rest, afterCommand);
  const matches = all.filter((m) => m.text.startsWith(w.current));
  if (matches.length === 0) return null;
  const names = [...new Set(matches.map((m) => m.text))].sort();
  if (names.length === 1) {
    const next = replace(line, w.from, `${names[0]} `);
    return {
      line: next,
      list: null,
      listAbove: false,
      state: { ...freshTab(), after: next.value },
    };
  }
  const prefix = sharedPrefix(names);
  if (prefix.length > w.current.length) {
    const next = replace(line, w.from, prefix);
    return {
      line: next,
      list: null,
      listAbove: false,
      state: { ...freshTab(), after: next.value },
    };
  }
  return {
    line,
    list: zshList(matches),
    listAbove: false,
    state: {
      after: line.value,
      stage: "listed",
      matches: names,
      index: 0,
      from: w.from,
      typed: w.current,
    },
  };
}

// ---- Line editing: the keys each shell binds (docs/facts/site.md, Keys at the prompt) ----

export type Edit =
  | "beginning-of-line"
  | "end-of-line"
  | "backward-char"
  | "forward-char"
  | "kill-line"
  | "kill-whole-line"
  | "unix-line-discard"
  | "backward-kill-word"
  | "unix-word-rubout"
  | "yank"
  | "undo"
  | "redo"
  | "clear-screen";

// Ctrl and a letter, by shell. A key the browser keeps never reaches the page, and a key no
// shell binds is the browser's.
// departs from the apps: Ctrl+R's history search is not drawn
export const keys: Record<Shell, Record<string, Edit>> = {
  zsh: {
    a: "beginning-of-line",
    e: "end-of-line",
    b: "backward-char",
    f: "forward-char",
    k: "kill-line",
    u: "kill-whole-line",
    w: "backward-kill-word",
    y: "yank",
    _: "undo",
    l: "clear-screen",
  },
  bash: {
    a: "beginning-of-line",
    e: "end-of-line",
    b: "backward-char",
    f: "forward-char",
    k: "kill-line",
    u: "unix-line-discard",
    w: "unix-word-rubout",
    y: "yank",
    _: "undo",
    l: "clear-screen",
  },
  powershell: {
    w: "backward-kill-word",
    y: "redo",
    z: "undo",
    l: "clear-screen",
  },
};

// zsh's WORDCHARS by default; PSReadLine's word delimiters in its Windows mode.
const ZSH_WORDCHARS = "*?_-.[]~=/&;!#$%^(){}<>";
const PS_DELIMITERS = ";:,.[]{}()/\\|^&*-=+'\"\u2013\u2014\u2015";

export interface Edited {
  line: Line;
  killed?: string;
}

export function edit(shell: Shell, what: Edit, line: Line, kill: string): Edited | null {
  const { value, caret } = line;
  const to = (v: string, c: number, killed?: string): Edited => ({
    line: { value: v, caret: c },
    killed,
  });
  switch (what) {
    case "beginning-of-line":
      return to(value, 0);
    case "end-of-line":
      return to(value, value.length);
    case "backward-char":
      return to(value, Math.max(0, caret - 1));
    case "forward-char":
      return to(value, Math.min(value.length, caret + 1));
    case "kill-line":
      return to(value.slice(0, caret), caret, value.slice(caret));
    case "kill-whole-line":
      return to("", 0, value);
    case "unix-line-discard":
      return to(value.slice(caret), 0, value.slice(0, caret));
    case "unix-word-rubout": {
      let i = caret;
      while (i > 0 && value[i - 1] === " ") i--;
      while (i > 0 && value[i - 1] !== " ") i--;
      return to(value.slice(0, i) + value.slice(caret), i, value.slice(i, caret));
    }
    case "backward-kill-word": {
      const inWord =
        shell === "powershell"
          ? (c: string) => c !== " " && !PS_DELIMITERS.includes(c)
          : (c: string) => /[\p{L}\p{N}]/u.test(c) || ZSH_WORDCHARS.includes(c);
      let i = caret;
      while (i > 0 && !inWord(value[i - 1])) i--;
      while (i > 0 && inWord(value[i - 1])) i--;
      return to(value.slice(0, i) + value.slice(caret), i, value.slice(i, caret));
    }
    case "yank":
      return kill
        ? to(value.slice(0, caret) + kill + value.slice(caret), caret + kill.length)
        : null;
    default:
      return null;
  }
}
