// pitboard's command line as clap 4.6 reads it: the tree crates/pitboard's tests write into
// src/snapshots, parsed by clap_builder's parser/parser.rs and validator.rs, and their errors as
// error/format.rs writes them. Help is printed from the help snapshot itself.
import { escaped } from "./words.ts";

export interface Arg {
  id: string;
  long: string | null;
  short: string | null;
  value_names: string[];
  num_args: string | null;
  action: string;
  index: number | null;
  required: boolean;
  global: boolean;
  hidden: boolean;
  possible_values: string[];
  default_values: string[];
  conflicts_with: string[];
  help: string | null;
}

export interface Command {
  name: string;
  hidden: boolean;
  about: string | null;
  arguments: Arg[];
  commands: Command[];
}

// An insta snapshot's body, after its header.
export function snapshot(text: string): string {
  const body = text.replace(/^---\n[\s\S]*?\n---\n/, "");
  return body.endsWith("\n") ? body.slice(0, -1) : body;
}

export const readTree = (text: string): Command => JSON.parse(snapshot(text));

// Each help in the snapshot of every help typed, by the command path it is for: "pitboard use".
export function readHelps(text: string): Map<string, string[]> {
  const helps = new Map<string, string[]>();
  let path: string | null = null;
  for (const line of snapshot(text).split("\n")) {
    const typed = /^\$ (pitboard(?: [a-z-]+)*) --help$/.exec(line);
    if (typed) helps.set((path = typed[1]), []);
    else if (path) helps.get(path)!.push(line);
  }
  return helps;
}

// strsim::jaro, by Unicode scalar values.
export function jaro(a: string, b: string): number {
  const x = [...a];
  const y = [...b];
  if (x.length === 0 && y.length === 0) return 1;
  if (x.length === 0 || y.length === 0) return 0;
  const range = Math.max(0, Math.floor(Math.max(x.length, y.length) / 2) - 1);
  const xs = x.map(() => false);
  const ys = y.map(() => false);
  let matches = 0;
  x.forEach((c, i) => {
    const low = i > range ? i - range : 0;
    const high = Math.min(y.length, i + range + 1);
    for (let j = 0; j < high; j++) {
      if (low <= j && c === y[j] && !ys[j]) {
        xs[i] = ys[j] = true;
        matches++;
        break;
      }
    }
  });
  if (matches === 0) return 0;
  let transpositions = 0;
  let j = 0;
  x.forEach((c, i) => {
    if (!xs[i]) return;
    while (!ys[j]) j++;
    if (c !== y[j]) transpositions++;
    j++;
  });
  transpositions = Math.floor(transpositions / 2);
  return (matches / x.length + matches / y.length + (matches - transpositions) / matches) / 3;
}

// suggestions.rs `did_you_mean`: each over 0.7, the most similar last, ties in the order given.
export function didYouMean(typed: string, possible: string[]): string[] {
  const found: [number, string][] = [];
  for (const p of possible) {
    const score = jaro(typed, p);
    if (score <= 0.7) continue;
    let at = found.findIndex(([s]) => s > score);
    if (at < 0) at = found.length;
    found.splice(at, 0, [score, p]);
  }
  return found.map(([, p]) => p);
}

// Styles::styled(): headings and usage bold and underlined, literals bold, `error:` bold red,
// valid green, invalid yellow; placeholders and context plain.
const plain = (text: string) => escaped(text);
export const literal = (text: string) => `<b>${escaped(text)}</b>`;
export const heading = (text: string) => `<b><u>${escaped(text)}</u></b>`;
const valid = (text: string) => `<span class="g">${escaped(text)}</span>`;
const invalid = (text: string) => `<span class="y">${escaped(text)}</span>`;
export const errorMark = '<b class="r">error:</b>';

const TAB = "  ";

const isFlag = (a: Arg) => a.num_args === "0";
const isPositional = (a: Arg) => a.index !== null;
const takesValue = (a: Arg) => !isFlag(a) && a.action !== "Help" && a.action !== "Version";

function argValue(a: Arg, required: boolean): string {
  const name = a.value_names[0] ?? a.id.toUpperCase();
  return isPositional(a) && !required ? `[${name}]` : `<${name}>`;
}

// Arg's Display: "--at <PERCENT>", "--json", "<SHELL>".
export function argName(a: Arg, required = a.required): string {
  const name = a.long ? `--${a.long}` : a.short ? `-${a.short}` : "";
  if (isPositional(a)) return argValue(a, required);
  return takesValue(a) ? `${name} ${argValue(a, required)}` : name;
}

function argStyled(a: Arg, required: boolean): string {
  if (isPositional(a)) return plain(argValue(a, required));
  const name = literal(a.long ? `--${a.long}` : `-${a.short}`);
  return takesValue(a) ? `${name} ${plain(argValue(a, required))}` : name;
}

export interface Level {
  command: Command;
  // the words that name it: ["pitboard", "watch", "install"]
  path: string[];
  // each argument given on the command line, by id, in the order given
  given: Map<string, string[]>;
  // a subcommand is required: its usage in the help snapshot ends in <COMMAND>
  needsCommand: boolean;
}

export type Parsed =
  | { kind: "run"; levels: Level[] }
  // `missing`: a subcommand was required and nothing was given, which clap answers on stderr
  | { kind: "help"; path: string[]; missing?: boolean }
  | { kind: "version" }
  | { kind: "error"; lines: string[] };

// A value a value parser refuses, with clap's reason, or null where it takes it.
export type Check = (value: string, arg: Arg) => string | null;

export interface Grammar {
  tree: Command;
  helps: Map<string, string[]>;
  // value parsers by "path id": "pitboard watch at"
  checks: Record<string, Check>;
}

const positionals = (c: Command) =>
  c.arguments.filter(isPositional).sort((a, b) => a.index! - b.index!);
// Command::all_subcommand_names: every name, hidden ones and help included.
const subcommandNames = (c: Command) => c.commands.map((s) => s.name);

function needsCommand(g: Grammar, path: string[]): boolean {
  const usage = g.helps.get(path.join(" "))?.find((l) => l.startsWith("Usage: "));
  return usage?.endsWith("<COMMAND>") ?? false;
}

// usage.rs `needs_options_tag`
const needsOptions = (c: Command) =>
  c.arguments.some(
    (a) =>
      !isPositional(a) &&
      a.long !== "help" &&
      a.long !== "version" &&
      a.action !== "Help" &&
      a.action !== "Version" &&
      !a.hidden &&
      !a.required,
  );

// usage.rs `create_usage_with_title`: the help's usage where nothing was used, and otherwise the
// smart usage of what was, with every required argument.
export function usage(level: Pick<Level, "command" | "path" | "needsCommand">, used: string[]) {
  const c = level.command;
  const parts = [literal(level.path.join(" "))];
  if (used.length === 0 && needsOptions(c)) parts.push(plain("[OPTIONS]"));
  const required = c.arguments.filter((a) => a.required).map((a) => a.id);
  const wanted = [...required, ...used];
  const opts: string[] = [];
  for (const id of wanted) {
    const a = c.arguments.find((x) => x.id === id);
    if (!a || isPositional(a)) continue;
    const s = argStyled(a, true);
    if (!opts.includes(s)) opts.push(s);
  }
  parts.push(...opts);
  for (const p of positionals(c)) {
    if (p.hidden) continue;
    parts.push(argStyled(p, wanted.includes(p.id)));
  }
  if (used.length === 0 && c.commands.length > 0)
    parts.push(plain(level.needsCommand ? "<COMMAND>" : "[COMMAND]"));
  else if (used.length > 0 && level.needsCommand) parts.push(plain("<COMMAND>"));
  return `${heading("Usage:")} ${parts.join(" ")}`;
}

// format.rs: the message, its tips, the usage, and where to read more.
function failure(message: string, more: { tips?: string[]; usage?: string | null }): string[] {
  const text = [`${errorMark} ${message}`];
  if (more.tips?.length) text.push("", ...more.tips);
  if (more.usage) text.push("", more.usage);
  text.push("", `For more information, try '${literal("--help")}'.`);
  return text.join("\n").split("\n");
}

const similar = (what: string, names: string[]) =>
  `${TAB}${valid("tip:")} ${
    names.length === 1 ? `a similar ${what} exists: ` : `some similar ${what}s exist: `
  }${names.map((n) => `'${valid(n)}'`).join(", ")}`;

class Missing {
  path: string[];
  constructor(path: string[]) {
    this.path = path;
  }
}

class Refusal {
  lines: string[];
  constructor(lines: string[]) {
    this.lines = lines;
  }
}

const refuse = (message: string, more: { tips?: string[]; usage?: string | null } = {}) =>
  new Refusal(failure(message, more));

// parser.rs `get_matches_with` for one command and what follows it, then validator.rs.
function parseLevel(g: Grammar, level: Level, words: string[], levels: Level[]): Parsed {
  const c = level.command;
  const find = (pred: (a: Arg) => boolean) => c.arguments.find(pred);
  const present = (id: string) => level.given.has(id);
  const used = () => [...level.given.keys()].filter((id) => !find((a) => a.id === id)?.hidden);
  let trailing = false;
  let pending: Arg | null = null;
  let pos = 1;
  const helpOf = (): Parsed => ({ kind: "help", path: level.path });
  const check = (a: Arg, value: string) => {
    if (a.possible_values.length > 0 && !a.possible_values.includes(value)) {
      const tip = didYouMean(value, a.possible_values).pop();
      throw refuse(
        `invalid value '${invalid(value)}' for '${literal(argName(a))}'\n${TAB}[possible values: ${a.possible_values.map(valid).join(", ")}]`,
        { tips: tip ? [`${TAB}${valid("tip:")} a similar value exists: '${valid(tip)}'`] : [] },
      );
    }
    const why = g.checks[`${level.path.join(" ")} ${a.id}`]?.(value, a);
    if (why !== null && why !== undefined)
      throw refuse(`invalid value '${invalid(value)}' for '${literal(argName(a))}': ${plain(why)}`);
  };
  const set = (a: Arg, value: string) => {
    if (present(a.id))
      throw refuse(`the argument '${invalid(argName(a))}' cannot be used multiple times`, {
        usage: usage(level, []),
      });
    check(a, value);
    level.given.set(a.id, [value]);
  };
  const resolve = () => {
    if (!pending) return;
    const a = pending;
    pending = null;
    throw refuse(`a value is required for '${invalid(argName(a))}' but none was supplied`);
  };
  // `react`: a flag given, by its action
  const react = (a: Arg): Parsed | null => {
    resolve();
    if (a.action === "Help") return helpOf();
    if (a.action === "Version") return { kind: "version" };
    if (present(a.id))
      throw refuse(`the argument '${invalid(argName(a))}' cannot be used multiple times`, {
        usage: usage(level, []),
      });
    level.given.set(a.id, []);
    return null;
  };
  try {
    for (let i = 0; i < words.length; i++) {
      const word = words[i];
      if (!trailing) {
        if (!pending && subcommandNames(c).includes(word)) {
          resolve();
          const sub = c.commands.find((s) => s.name === word)!;
          if (word === "help") return helpCommand(g, level, words.slice(i + 1));
          const path = [...level.path, word];
          const next: Level = {
            command: sub,
            path,
            given: new Map(),
            needsCommand: needsCommand(g, path),
          };
          levels.push(next);
          const child = parseLevel(g, next, words.slice(i + 1), levels);
          if (child.kind !== "run") return child;
          validate(level, true);
          return child;
        }
        if (word === "--") {
          trailing = true;
          continue;
        }
        if (word.startsWith("--")) {
          const [name, ...rest] = word.slice(2).split("=");
          const value = rest.length > 0 ? rest.join("=") : null;
          const a = find((x) => x.long === name);
          if (!a) {
            pending = null;
            throw unknownLong(level, name, words.slice(i + 1), trailing, used());
          }
          if (takesValue(a)) {
            resolve();
            if (value !== null) set(a, value);
            else pending = a;
          } else if (value !== null) {
            resolve();
            const required = c.arguments.filter((x) => x.required).map((x) => x.id);
            throw refuse(
              `unexpected value '${invalid(value)}' for '${literal(argName(a))}' found; no more were expected`,
              {
                usage: usage(level, [...used().filter((id) => !required.includes(id)), a.id]),
              },
            );
          } else {
            const out = react(a);
            if (out) return out;
          }
          continue;
        }
        if (word.startsWith("-") && word !== "-") {
          const flags = [...word.slice(1)];
          for (let k = 0; k < flags.length; k++) {
            const a = find((x) => x.short === flags[k]);
            if (!a) {
              throw refuse(`unexpected argument '${invalid(`-${flags[k]}`)}' found`, {
                tips:
                  positionals(c).length > 0 && !trailing
                    ? [
                        `${TAB}${valid("tip:")} to pass '${invalid(`-${flags[k]}`)}' as a value, use '${valid(`-- -${flags[k]}`)}'`,
                      ]
                    : [],
                usage: usage(level, []),
              });
            }
            if (takesValue(a)) {
              resolve();
              const attached = flags.slice(k + 1).join("");
              const value = attached.startsWith("=") ? attached.slice(1) : attached;
              if (value) set(a, value);
              else pending = a;
              break;
            }
            const out = react(a);
            if (out) return out;
          }
          continue;
        }
        if (pending) {
          const a: Arg = pending;
          pending = null;
          set(a, word);
          continue;
        }
      }
      const p = positionals(c).find((x) => x.index === pos);
      if (p) {
        set(p, word);
        pos++;
        continue;
      }
      throw unmatched(level, word);
    }
    resolve();
    validate(level, false);
  } catch (e) {
    if (e instanceof Refusal) return { kind: "error", lines: e.lines };
    if (e instanceof Missing) return { kind: "help", path: e.path, missing: true };
    throw e;
  }
  return { kind: "run", levels };
}

// The help subcommand: the help of the command its words name.
function helpCommand(g: Grammar, level: Level, words: string[]): Parsed {
  let c = level.command;
  const path = [...level.path];
  for (const word of words) {
    const sub = c.commands.find((s) => s.name === word && s.name !== "help");
    if (!sub) {
      const at: Level = { command: c, path, given: new Map(), needsCommand: needsCommand(g, path) };
      return {
        kind: "error",
        lines: failure(`unrecognized subcommand '${invalid(word)}'`, { usage: usage(at, []) }),
      };
    }
    c = sub;
    path.push(word);
  }
  return { kind: "help", path };
}

// parser.rs `match_arg_error`: a word no argument takes.
function unmatched(level: Level, word: string): Refusal {
  const c = level.command;
  const hasPositionals = positionals(c).length > 0;
  if (c.commands.length > 0) {
    const names = didYouMean(word, subcommandNames(c));
    if (names.length > 0)
      return refuse(`unrecognized subcommand '${invalid(word)}'`, {
        tips: [similar("subcommand", names)],
        usage: usage(level, []),
      });
    if (!hasPositionals)
      return refuse(`unrecognized subcommand '${invalid(word)}'`, { usage: usage(level, []) });
  }
  return refuse(`unexpected argument '${invalid(word)}' found`, { usage: usage(level, []) });
}

// parser.rs `did_you_mean_error`: a long flag no argument has.
function unknownLong(
  level: Level,
  name: string,
  after: string[],
  trailing: boolean,
  used: string[],
): Refusal {
  const c = level.command;
  const longs = c.arguments.flatMap((a) => (a.long ? [a.long] : []));
  let suggestion: [string, string | null] | null = null;
  const own = didYouMean(name, longs).pop();
  if (own) suggestion = [own, null];
  else {
    let best: [number, [string, string]] | null = null;
    for (const sub of c.commands) {
      const theirs = didYouMean(
        name,
        sub.arguments.flatMap((a) => (a.long ? [a.long] : [])),
      ).pop();
      const score = after.indexOf(sub.name);
      if (!theirs || score < 0) continue;
      if (!best || score < best[0]) best = [score, [theirs, sub.name]];
    }
    if (best) suggestion = best[1];
  }
  const tips: string[] = [];
  const shown = [...used];
  const hasPositionals = positionals(c).length > 0;
  if (!suggestion && !trailing && hasPositionals)
    tips.push(
      `${TAB}${valid("tip:")} to pass '${invalid(`--${name}`)}' as a value, use '${valid(`-- --${name}`)}'`,
    );
  if (suggestion?.[1])
    tips.push(`${TAB}${valid("tip:")} '${valid(`${suggestion[1]} --${suggestion[0]}`)}' exists`);
  if (suggestion && !suggestion[1]) {
    const a = c.arguments.find((x) => x.long === suggestion![0])!;
    if (!shown.includes(a.id)) shown.push(a.id);
    tips.unshift(
      `${TAB}${valid("tip:")} a similar argument exists: '${valid(`--${suggestion[0]}`)}'`,
    );
  }
  return refuse(`unexpected argument '${invalid(`--${name}`)}' found`, {
    tips,
    usage: usage(level, shown),
  });
}

// validator.rs: a subcommand required, conflicts, then every required argument.
function validate(level: Level, hasSubcommand: boolean) {
  const c = level.command;
  // a subcommand clap derives as required asks for help where nothing at all was given
  if (!hasSubcommand && level.needsCommand && level.given.size === 0) throw new Missing(level.path);
  if (!hasSubcommand && level.needsCommand) {
    const names = subcommandNames(c);
    throw new Refusal([
      `${errorMark} '${invalid(level.path.join(" "))}' requires a subcommand but one was not provided`,
      `${TAB}[subcommands: ${names.map(valid).join(", ")}]`,
      "",
      usage(level, []),
      "",
      `For more information, try '${literal("--help")}'.`,
    ]);
  }
  const given = [...level.given.keys()];
  const arg = (id: string) => c.arguments.find((a) => a.id === id)!;
  for (const id of given) {
    const against = given.filter(
      (other) =>
        other !== id &&
        (arg(id).conflicts_with.includes(other) || arg(other).conflicts_with.includes(id)),
    );
    if (against.length === 0) continue;
    const shown = given.filter((x) => !arg(x).hidden && !against.includes(x));
    const prior =
      against.length === 1
        ? ` '${invalid(argName(arg(against[0])))}'`
        : `:${against.map((x) => `\n${TAB}${invalid(argName(arg(x)))}`).join("")}`;
    throw refuse(`the argument '${invalid(argName(arg(id)))}' cannot be used with${prior}`, {
      usage: usage(level, shown),
    });
  }
  if (hasSubcommand) return;
  const missing = positionals(c).filter((p) => p.required && !level.given.has(p.id));
  if (missing.length === 0) return;
  const highest = Math.max(...missing.map((p) => p.index!));
  const shownMissing = positionals(c).filter(
    (p) => !level.given.has(p.id) && (p.required || p.index! < highest),
  );
  const used = [...given.filter((x) => !arg(x).hidden), ...shownMissing.map((p) => p.id)];
  throw new Refusal(
    failure(
      `the following required arguments were not provided:${shownMissing
        .map((p) => `\n${TAB}${valid(argName(p, true))}`)
        .join("")}`,
      { usage: usage(level, used) },
    ),
  );
}

export function parse(g: Grammar, words: string[]): Parsed {
  const root: Level = {
    command: g.tree,
    path: ["pitboard"],
    given: new Map(),
    needsCommand: needsCommand(g, ["pitboard"]),
  };
  const out = parseLevel(g, root, words, [root]);
  if (out.kind !== "run") return out;
  // main.rs `command_line`: `watch`'s own --at, --once or --scheduled beside a subcommand
  const watch = out.levels.find((l) => l.path.join(" ") === "pitboard watch");
  const sub = out.levels[out.levels.indexOf(watch!) + 1];
  if (watch && sub) {
    const given = ["at", "once", "scheduled"].find((id) => watch.given.has(id));
    if (given)
      return {
        kind: "error",
        lines: failure(
          `the subcommand '${plain(sub.path.at(-1)!)}' cannot be used with '--${plain(given)}'`,
          { usage: usage(watch, []) },
        ),
      };
  }
  return out;
}

// The help a help flag or the help subcommand prints, styled as clap styles it.
export function helpLines(g: Grammar, path: string[]): string[] {
  const help = g.helps.get(path.join(" "));
  if (!help) throw new Error(`no help for ${path.join(" ")}`);
  let section = "";
  return help.map((line) => {
    if (line.startsWith("Usage: ")) {
      const rest = line.slice("Usage: ".length);
      const at = rest.search(/ [[<]/);
      const name = at < 0 ? rest : rest.slice(0, at);
      return `${heading("Usage:")} ${literal(name)}${plain(at < 0 ? "" : rest.slice(at))}`;
    }
    if (/^[A-Z][a-z]+:$/.test(line)) {
      section = line;
      return heading(line);
    }
    if (section === "Commands:" && /^ {2}\S/.test(line)) {
      const [, name, rest] = /^ {2}(\S+)(.*)$/.exec(line)!;
      return `${TAB}${literal(name)}${plain(rest)}`;
    }
    if (section === "Options:" && /^ {2}(-\w|    --)/.test(line)) {
      const [, flags, rest] = /^ {2}((?:-\w, )?(?: {4})?--[\w-]+|-\w)(.*)$/.exec(line)!;
      const styled = flags.replace(/(-\w)(, )?|(--[\w-]+)/g, (_, s, comma, l) =>
        s ? `${literal(s)}${comma ?? ""}` : literal(l),
      );
      return `${TAB}${styled}${plain(rest)}`;
    }
    if (section === "Examples:" && /^ {2}\S/.test(line)) {
      return `${TAB}${literal(line.slice(2, 32))}${plain(line.slice(32))}`;
    }
    return plain(line);
  });
}

// The words a Windows build says otherwise, as pairs of what each tree holds: main.rs gives
// `ABOUT`, the labels' help and `stow`'s about by `cfg!(windows)`, and nothing else differs.
export function differing(off: Command, on: Command): [string, string][] {
  const pairs: [string, string][] = [];
  const add = (a: string | null, b: string | null) => {
    if (a && b && a !== b && !pairs.some(([x]) => x === a)) pairs.push([a, b]);
  };
  const walk = (x: Command, y: Command) => {
    add(x.about, y.about);
    x.arguments.forEach((arg, i) => add(arg.help, y.arguments[i]?.help ?? null));
    x.commands.forEach((sub, i) => walk(sub, y.commands[i]));
  };
  walk(off, on);
  return pairs;
}

// A script kept for one side, with the other side's words: what the same generator writes there.
export function swapped(script: string, pairs: [string, string][], toWindows: boolean): string {
  let out = script;
  for (const [off, on] of pairs) out = out.replaceAll(toWindows ? off : on, toWindows ? on : off);
  return out;
}
