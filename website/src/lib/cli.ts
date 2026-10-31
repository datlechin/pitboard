// What `pitboard` prints in the site's terminals, for each command clap.ts reads, from the one
// state: crates/pitboard/src/main.rs, warm.rs and watch.rs, with the core's words and errors
// (crates/pitboard-core/src/words.rs, error.rs, label.rs). Every change goes through model.ts,
// as the command line.
import {
  errorMark,
  helpLines,
  parse,
  type Arg,
  type Check,
  type Grammar,
  type Level,
} from "./clap.ts";
import { plainText } from "./changelog.ts";
import { doctorRows, laidOut, markOf, type Row } from "./doctor.ts";
import {
  ADOPTION_CEILING_SECONDS,
  HIGHEST,
  LOWEST,
  PERIOD,
  START_STEP,
  decide,
  decideOnce,
  forget as forgotten,
  installWatch,
  logEntry,
  model,
  nameProblem,
  rename as renamed,
  setSchedule,
  setWarmRange,
  turnWarmUp,
  typed,
  uninstallWatch,
  use as switched,
  warmStanding,
  warmUp,
  watchInTerminal,
  type Decision,
  type State,
  type WarmStanding,
} from "./model.ts";
import {
  escaped,
  joined,
  moment,
  notSwitching,
  renewalDue,
  shareOfLimit,
  stamp,
  statusHeading,
  statusLine,
  statusLines,
  warmSaid,
  whole,
} from "./words.ts";
import { DAY, ZONE, toolOn, tools, type Account, type Tool } from "../data/world.ts";
import type { Platform } from "./platform.ts";

export interface Machine {
  platform: Platform;
  // the release the site offers this system, which `pitboard -V` names
  version: string;
  grammar: Grammar;
  // `pitboard doctor` on a healthy computer of this system in the site's world
  doctor: Row[];
  // a shell's completion script, read only when asked for
  script: (shell: string) => Promise<string | null>;
}

// What a run leaves at the prompt: lines printed, then the prompt again, a question asked on
// the prompt line, or a program that runs until Ctrl+C.
export interface Step {
  lines: string[];
  // it exited with an error, which a shell's && and || read
  failed?: boolean;
  ask?: { question: string; answer: (reply: string) => Step };
  runs?: Running;
}

export interface Running {
  // what it prints as the state changes
  next: (s: State) => string[];
  stop: () => void;
}

const b = (text: string) => `<b>${escaped(text)}</b>`;
const dim = (text: string) => `<span class="dim">${escaped(text)}</span>`;
const yellow = (text: string) => `<span class="y">${escaped(text)}</span>`;
const green = (text: string) => `<span class="g">${escaped(text)}</span>`;
const red = (text: string) => `<span class="r">${escaped(text)}</span>`;
const plain = escaped;

const toolName = (tool: Tool) => tools.find(([t]) => t === tool)![1];
const codes: Tool[] = ["claude", "codex", "desktop"];

// The world as this system's command line reads it: Claude Desktop's accounts on macOS alone.
export const seenFrom = (s: State, platform: Platform): State => ({
  ...s,
  accounts: s.accounts.filter((a) => toolOn[a.tool].includes(platform)),
});

// Rust's integer parsing, which the value parsers main.rs and watch.rs use.
const integer = (text: string, signed: boolean, max: bigint): bigint | string => {
  if (text === "") return "cannot parse integer from empty string";
  const m = (signed ? /^([+-]?)(\d+)$/ : /^(\+?)(\d+)$/).exec(text);
  if (!m) return "invalid digit found in string";
  const n = BigInt(m[1] === "-" ? `-${m[2]}` : m[2]);
  if (n > max) return "number too large to fit in target type";
  if (n < -max - 1n) return "number too small to fit in target type";
  return n;
};

// watch.rs `Share`: `value_parser!(u8).range(LOWEST..=HIGHEST)`, read as an i64 first.
const percent: Check = (value) => {
  const n = integer(value, true, 9223372036854775807n);
  if (typeof n === "string") return n;
  return n < BigInt(LOWEST) || n > BigInt(HIGHEST) ? `${n} is not in ${LOWEST}..=${HIGHEST}` : null;
};

// main.rs `Log`: a `usize`
const count: Check = (value) => {
  const n = integer(value, false, 18446744073709551615n);
  return typeof n === "string" ? n : null;
};

const SEPARATOR = "/";
const ONE_WORD = "a label must be one word, such as `personal` or `work`";

// label.rs `choose`: a name for an account that is not enrolled yet, with the tool it is for.
export function choose(text: string): { tool: Tool; label: string } | string {
  const at = text.indexOf(SEPARATOR);
  const prefix = at < 0 ? null : text.slice(0, at);
  const label = at < 0 ? text : text.slice(at + 1);
  const tool = prefix === null ? "claude" : codes.find((t) => t === prefix);
  if (!tool) return `\`${prefix}\` is not a tool Pitboard knows. It knows: ${codes.join(", ")}`;
  const problem = nameProblem(label);
  if (problem === "empty" && at >= 0)
    return `\`${text}\` names a tool but no account. Try \`${tool}${SEPARATOR}work\``;
  if (problem === "empty" || problem === "not_one_word") return ONE_WORD;
  if (problem === "separator")
    return `a label cannot contain \`${SEPARATOR}\`: it separates the tool from the name, so at most one belongs in \`${text}\``;
  return { tool, label };
}

export const checks: Record<string, Check> = {
  "pitboard watch at": percent,
  "pitboard watch install at": percent,
  "pitboard log lines": count,
  // main.rs `new_label`
  "pitboard rename to": (value) => {
    const chosen = choose(value);
    return typeof chosen === "string" ? chosen : null;
  },
  // main.rs `label_to_enroll`: a name with a slash is the core's to read
  "pitboard enroll label": (value) => {
    const problem = nameProblem(value);
    return problem === "empty" || problem === "not_one_word" ? ONE_WORD : null;
  },
};

// What a command produced, before it is printed for a person or a program (main.rs `Report`).
interface Failed {
  code: string;
  message: string;
}

interface Report {
  command: string | null;
  data?: unknown;
  human?: string[];
  error?: Failed;
  // a report a command made that still failed, which only `generate` makes here
  failure?: Failed;
}

const isFailed = (x: unknown): x is Failed =>
  typeof x === "object" && x !== null && "code" in x && "message" in x;

// json.rs `ascii`: one line, every character outside ASCII written as a `\u` escape.
export const ascii = (value: unknown) =>
  JSON.stringify(value).replace(/[^\x00-\x7f]/g, (c) =>
    [...Array(c.length).keys()]
      .map((i) => `\\u${c.charCodeAt(i).toString(16).padStart(4, "0")}`)
      .join(""),
  );

// main.rs `emit`: the human words, or the envelope `--json` promises.
function emit(r: Report, json: boolean): string[] {
  if (json) {
    const error = r.error
      ? { code: r.error.code, message: r.error.message, cause: null }
      : r.failure
        ? { code: r.failure.code, message: r.failure.message }
        : null;
    const envelope = {
      v: 1,
      command: r.command,
      ok: !r.error && !r.failure,
      data: r.error ? null : (r.data ?? null),
      warnings: [],
      error,
    };
    return [plain(ascii(envelope))];
  }
  if (r.error) return `${errorMark} ${plain(r.error.message)}`.split("\n");
  return r.human ?? [];
}

const refusal = (command: string | null, e: Failed): Report => ({ command, error: e });
const done = (command: string, data: unknown, ...human: string[]): Report => ({
  command,
  data,
  human,
});

// label.rs `resolve`: the one account a label names on this system.
function resolve(s: State, text: string): Account | Failed {
  const whole = s.accounts.find((a) => a.label === text);
  const at = text.indexOf(SEPARATOR);
  if (at >= 0) {
    const prefix = text.slice(0, at);
    const label = text.slice(at + 1);
    const tool = codes.find((t) => t === prefix);
    if (!tool)
      return (
        whole ?? {
          code: "provider_unknown",
          message: `\`${prefix}\` is not a tool Pitboard knows. It knows: ${codes.join(", ")}.`,
        }
      );
    const found = s.accounts.find((a) => a.tool === tool && a.label === label);
    if (found) return found;
    if (whole) return whole;
    return {
      code: "account_unknown",
      message: `no account is enrolled as \`${text}\`.${enrolledLabels(
        s.accounts.filter((a) => a.tool === tool).map((a) => a.label),
      )} Run \`pitboard enroll ${text} --sign-in\` to add it.`,
    };
  }
  const named = s.accounts.filter((a) => a.label === text);
  if (named.length === 1) return named[0];
  if (named.length > 1)
    return {
      code: "label_ambiguous",
      message: `\`${text}\` is enrolled for more than one tool: ${named
        .map((a) => `\`${a.tool}/${a.label}\``)
        .join(", ")}. Say which one.`,
    };
  const oneTool = new Set(s.accounts.map((a) => a.tool)).size <= 1;
  return {
    code: "account_unknown",
    message: `no account is enrolled as \`${text}\`.${enrolledLabels(
      s.accounts.map((a) => (oneTool ? a.label : `${a.tool}/${a.label}`)),
    )} Run \`pitboard enroll ${text} --sign-in\` to add it.`,
  };
}

// error.rs `Enrolled`
const enrolledLabels = (labels: string[]) =>
  labels.length === 0
    ? " Nothing is enrolled yet."
    : ` Enrolled: ${labels.map((l) => `\`${l}\``).join(", ")}.`;

// A change the core refused, recorded as service.rs `refused` records it.
function refused(command: string, verb: string, subject: string, e: Failed): Report {
  logEntry("cli", verb, subject, e.code);
  return refusal(command, e);
}

// words.rs `with_email`
const withEmail = (name: string, email: string) => (email ? `${name} (${plain(email)})` : name);

// warm/mod.rs `clock`
const clock = (minutes: number) =>
  `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
const span = (s: State) => `${clock(s.warm.from)} to ${clock(s.warm.to)}`;

// Where each system keeps a job: launchd.rs's labels, systemd.rs's units, and task.rs's tasks
// with the SID of its tests.
const SID = "S-1-5-21-1004-1005-1006-1001";
export function location(platform: Platform, job: "renew" | "watch"): [string, string] {
  switch (platform) {
    case "macos":
      return ["path", `/Users/me/Library/LaunchAgents/com.datlechin.pitboard.${job}.plist`];
    case "linux":
      return [
        "path",
        `/home/me/.config/systemd/user/${job === "renew" ? "pitboard-renew.timer" : "pitboard-watch.service"}`,
      ];
    case "windows":
      return ["task", `\\Pitboard\\${job === "renew" ? "Renew" : "Watch"} ${SID}`];
  }
}

// main.rs `located`: installed, where, then the rest.
const located = (platform: Platform, job: "renew" | "watch", rest: Record<string, unknown>) => {
  const [key, where] = location(platform, job);
  return { installed: true, [key]: where, ...rest };
};

// Pitboard's own folder, as `Pitboard::shown` writes it.
const folder = (platform: Platform) =>
  platform === "windows" ? "C:\\Users\\me\\AppData\\Local\\Pitboard" : "~/.pitboard";

// words.rs `TERMS_MAY_COVER_THIS` and `IS_THIS_ALLOWED_URL`
const TERMS =
  "Anthropic's terms may cover automatic switching and warm-ups: https://docs.usepitboard.com/concepts/limits#is-this-allowed";

const flag = (level: Level, id: string) => level.given.has(id);

// The value an argument takes on this run: the one typed, or its default.
function value(level: Level, id: string): string {
  const typedValue = level.given.get(id)?.[0];
  if (typedValue !== undefined) return typedValue;
  return level.command.arguments.find((a: Arg) => a.id === id)!.default_values[0];
}

// `pitboard`, `pitboard status`: every tool this system lists, as status.rs prints it.
function status(m: Machine, s: State, offline: boolean): Report {
  const w = seenFrom(offline ? s : { ...s, readAt: s.now }, m.platform);
  const out: string[] = [];
  tools
    .filter(([tool]) => toolOn[tool].includes(m.platform))
    .forEach(([tool], i) => {
      if (i > 0) out.push("");
      out.push(statusHeading(tool), ...statusLines(w, tool));
    });
  return { command: "status", human: out };
}

// main.rs `switched_report` and `followed`
const adoption: Record<Tool, Record<string, unknown>> = {
  claude: { follows: "polling", within_seconds: ADOPTION_CEILING_SECONDS },
  codex: { follows: "restart", program: "codex" },
  desktop: { follows: "open", app: "Claude" },
};
const follows: Record<Tool, string> = {
  claude: `Claude Code sessions already running follow within ${ADOPTION_CEILING_SECONDS} seconds.`,
  codex:
    "Restart any running `codex` for this to take effect. It will not pick the switch up on its own.",
  desktop: "Claude opens signed in to it.",
};

function use(m: Machine, s: State, text: string): Report {
  const w = seenFrom(s, m.platform);
  const to = resolve(w, text);
  if (isFailed(to)) return refused("use", "use", text, to);
  const from = w.accounts.find((a) => a.tool === to.tool && a.label === w.inUse[to.tool])!;
  const outcome = switched(to.tool, to.label, "cli");
  if (outcome === "already_active")
    return done(
      "use",
      { to: typed(to), changed: false, config_updated: false },
      `${b(typed(to))} is already signed in.`,
    );
  if (outcome === "parked_login_expired")
    return refusal("use", {
      code: outcome,
      message: `the parked login for \`${typed(to)}\` has expired. Run \`pitboard enroll ${typed(to)} --sign-in\` to sign in to it again.`,
    });
  return done(
    "use",
    {
      from: typed(from),
      to: typed(to),
      provider: to.tool,
      changed: true,
      parked_at: s.now,
      adoption_ceiling_seconds: to.tool === "claude" ? ADOPTION_CEILING_SECONDS : null,
      adoption: adoption[to.tool],
    },
    `Switched to ${b(typed(to))}; ${b(typed(from))} is parked.`,
    plain(follows[to.tool]),
  );
}

// words.rs `forget_question`
const forgetQuestion = (tool: Tool | null, label: string) =>
  tool === "desktop"
    ? `Forget ${label} and delete its parked login and the folders set aside for it? Adding it again needs a sign-in in Claude. [y/N] `
    : `Forget ${label} and delete its parked login? Adding it again needs a browser sign-in. [y/N] `;

// main.rs `agreed`
export const agreed = (reply: string) => ["y", "Y", "yes"].includes(reply.trim());

function forget(m: Machine, text: string): Report {
  const w = seenFrom(model.get(), m.platform);
  const a = resolve(w, text);
  if (isFailed(a)) return refused("forget", "forget", text, a);
  if (w.inUse[a.tool] === a.label)
    return refused("forget", "forget", typed(a), {
      code: "cannot_forget_active_account",
      message: `\`${typed(a)}\` is signed in; switch to another account before forgetting it.`,
    });
  forgotten(a.tool, a.label, "cli");
  return done("forget", { label: text, email: a.email }, `Forgot ${withEmail(b(text), a.email)}.`);
}

function rename(m: Machine, s: State, from: string, to: string): Report {
  const w = seenFrom(s, m.platform);
  const a = resolve(w, from);
  if (isFailed(a)) return refused("rename", "rename", from, a);
  const chosen = choose(to) as { tool: Tool; label: string };
  // label.rs `moved_by_a_rename`
  if (to.includes(SEPARATOR) && chosen.tool !== a.tool)
    return refused("rename", "rename", typed(a), {
      code: "usage",
      message: `\`${typed(a)}\` is a ${toolName(a.tool)} account, and a rename cannot move it to ${toolName(chosen.tool)}. Sign in to that tool and enrol the account there instead.`,
    });
  const taken = w.accounts.find((x) => x.tool === a.tool && x.label === chosen.label);
  // the model records a refusal of a name taken as the core does
  if (renamed(a.tool, a.label, chosen.label, "cli") === "label_taken")
    return refusal("rename", {
      code: "label_taken",
      message: `\`${typed(taken!)}\` already refers to ${login(a.tool, taken!.email, taken!.email === a.email)}. Choose a different label.`,
    });
  return done(
    "rename",
    { from, to, email: a.email },
    `Renamed ${b(from)} to ${withEmail(b(to), a.email)}.`,
  );
}

// words.rs `login`: a login named by its email, or as another organisation's.
function login(tool: Tool, email: string, sharesEmail: boolean): string {
  if (tool === "desktop" && !email) return "another Claude Desktop account";
  if (!sharesEmail) return email;
  const group = { claude: "organisation", codex: "workspace", desktop: "account" }[tool];
  return `${email} in another ${group}`;
}

// label.rs `enrolling`: an account enrolled already keeps its name; a new one is chosen.
function enrolling(w: State, text: string) {
  const whole = text.includes(SEPARATOR) ? w.accounts.find((a) => a.label === text) : undefined;
  return whole ?? choose(text);
}

// `pitboard enroll <label>`: the account its tool is signed in to now, which in this world is
// always enrolled already (switch/enroll.rs `claim` and `record_current`).
function enrollCurrent(m: Machine, s: State, text: string): Report {
  const w = seenFrom(s, m.platform);
  const chosen = enrolling(w, text);
  if (typeof chosen === "string")
    return refused("enroll", "enroll", text, { code: "usage", message: chosen });
  const key = { tool: chosen.tool, label: chosen.label };
  const live = w.accounts.find((a) => a.tool === key.tool && a.label === w.inUse[key.tool])!;
  const taken = w.accounts.find((a) => a.tool === key.tool && a.label === key.label);
  if (taken && taken !== live)
    return refused("enroll", "enroll", typed(key), {
      code: "label_taken",
      message: `\`${typed(key)}\` already refers to ${login(key.tool, taken.email, taken.email === live.email)}. Choose a different label.`,
    });
  const another = key.tool === "claude" ? "<label>" : `${key.tool}/<label>`;
  if (!taken) {
    const message =
      key.tool === "desktop"
        ? `Claude is signed in to \`${typed(live)}\`, which is enrolled already. To add \`${typed(key)}\`, run \`pitboard enroll ${typed(key)} --sign-in\`.`
        : `${live.email} is already enrolled as \`${typed(live)}\`. To add a different account, run \`pitboard enroll ${another} --sign-in\`.`;
    return refused("enroll", "enroll", typed(key), { code: "already_enrolled", message });
  }
  logEntry("cli", "enroll", typed(key));
  const data = { label: text, provider: key.tool, email: live.email, enrolled: "current" };
  const name = b(text);
  if (key.tool === "desktop")
    return done(
      "enroll",
      data,
      `Enrolled ${name}, the account Claude is signed in to now. Check that Claude shows that account: Pitboard cannot ask claude.ai whose it is.`,
      plain(`Add another: pitboard enroll ${another} --sign-in`),
    );
  return done(
    "enroll",
    data,
    `Enrolled ${withEmail(name, live.email)}, the account signed in now.`,
    plain(`Add another without signing out of it: pitboard enroll ${another} --sign-in`),
  );
}

// `pitboard enroll <label> --sign-in` hands over to the tool's own sign-in in a browser, which
// the page never opens: it says what it opens, and waits until Ctrl+C.
function enrollSigningIn(m: Machine, s: State, text: string): Report | string[] {
  const w = seenFrom(s, m.platform);
  const chosen = enrolling(w, text);
  if (typeof chosen === "string")
    return refused("enroll", "enroll", text, { code: "usage", message: chosen });
  const existing = w.accounts.find((a) => a.tool === chosen.tool && a.label === chosen.label);
  if (chosen.tool === "desktop") {
    const home = typed({ tool: "desktop", label: w.inUse.desktop });
    const whom = existing ? `\`${text}\` again` : `the account to add as \`${text}\``;
    // folder/awaited.rs, as words.rs `awaited` says each line
    return [
      plain(`Parked \`${home}\`. Claude is signed out, and nothing was signed out at claude.ai.`),
      plain(
        `Open Claude and sign in to ${whom}. When Claude shows your chats, quit it with Command-Q.`,
      ),
      plain(
        "Do not choose Log out in Claude: it ends the login Claude holds at claude.ai, and Pitboard cannot bring it back.",
      ),
      plain(
        `Waiting for Claude to quit. Control-C stops waiting and keeps \`${home}\` parked; \`pitboard use ${home}\` puts it back.`,
      ),
    ];
  }
  const who = existing ? existing.email : "the account to add";
  return [
    plain(
      `Opening ${toolName(chosen.tool)}'s sign-in. Sign in as ${who}; the account in use now stays signed in.`,
    ),
  ];
}

// main.rs `log`: the newest `n`, oldest first, each verb padded to the longest shown.
function log(m: Machine, s: State, n: number): Report {
  const all = s.log.filter((e) => !e.on || e.on.includes(m.platform));
  const shown = n === 0 ? [] : all.slice(-n);
  const data = {
    entries: shown.map((e) => ({
      at: stamp(e.at),
      caller: e.caller,
      verb: e.verb,
      subject: e.subject,
      outcome: e.outcome,
    })),
  };
  if (shown.length === 0) return done("log", data, "Pitboard has not changed anything yet.");
  const width = Math.max(...shown.map((e) => e.verb.length));
  return {
    command: "log",
    data,
    human: shown.map(
      (e) =>
        `${dim(stamp(e.at))}  ${plain(e.verb.padEnd(width))}  ${plain(e.subject)}  ${
          e.outcome === "ok" ? dim(e.outcome) : yellow(e.outcome)
        }`,
    ),
  };
}

const periodEnds = (started: number) => started - (started % START_STEP) + PERIOD;

// warm.rs `standing_json`
function standingJson(w: WarmStanding): Record<string, unknown> {
  switch (w.stands) {
    case "due":
      return { state: "due" };
    case "running":
      return { state: "running", until: w.until, next_at: w.next };
    case "resting":
      return { state: "resting", next_at: w.at };
    case "held":
      return { state: "held", reason: w.why, until: w.limit?.resetsAt ?? null };
  }
}

// `pitboard warm`: the span, and each Claude Code account with where it stands.
function warmOverview(s: State): Report {
  const claude = s.accounts.filter((a) => a.tool === "claude");
  const data = {
    range: { from: clock(s.warm.from), to: clock(s.warm.to) },
    halted: null,
    accounts: claude.map((a) =>
      s.warm.on.includes(a.id)
        ? { label: a.label, on: true, ...standingJson(warmStanding(s, a, "by-itself")) }
        : { label: a.label, on: false },
    ),
  };
  if (claude.length === 0)
    return done(
      "warm",
      data,
      "No Claude Code account is enrolled, so there is nothing to warm up.",
    );
  const width = Math.max(...claude.map((a) => [...a.label].length));
  const said = (w: WarmStanding) => warmSaid(w, (at) => moment(at, s.now));
  const out = [
    plain(
      `Pitboard warms up these accounts from ${span(s)}, while the app or \`pitboard watch\` runs.`,
    ),
  ];
  for (const a of claude) {
    const name = b(a.label + " ".repeat(width - [...a.label].length));
    out.push(
      s.warm.on.includes(a.id)
        ? `  ${name}  on   ${plain(said(warmStanding(s, a, "by-itself")))}`
        : `  ${name}  ${dim("off")}`,
    );
  }
  if (!claude.some((a) => s.warm.on.includes(a.id)))
    out.push(plain("Turn it on for an account with `pitboard warm on <account>`."));
  return { command: "warm", data, human: out };
}

// error.rs `warm_up_unsupported`
const unsupported = (a: Account): Failed => ({
  code: "warm_up_unsupported",
  message:
    a.tool === "desktop"
      ? `\`${typed(a)}\` is a Claude Desktop account, and Pitboard sends no request for one, so it warms up none.`
      : `\`${typed(a)}\` is a ${toolName(a.tool)} account, and Pitboard warms up Claude Code accounts only: when a ${toolName(a.tool)} period starts has not been measured.`,
});

function warmNow(m: Machine, s: State, text: string): Report {
  const a = resolve(seenFrom(s, m.platform), text);
  if (isFailed(a)) return refused("warm", "warm-up", text, a);
  if (a.tool !== "claude") return refusal("warm", unsupported(a));
  const at = (t: number) => moment(t, s.now);
  const standing = warmStanding(s, a, "now");
  if (standing.stands === "running")
    return done(
      "warm",
      { label: typed(a), outcome: "running", until: standing.until },
      `${b(typed(a))}'s 5-hour period runs already, until ${at(standing.until)}.`,
    );
  if (standing.stands === "held")
    return refusal("warm", {
      code: standing.why,
      message: `\`${typed(a)}\` ${warmSaid(standing, at)}, so it is not warmed up now.`,
    });
  warmUp(a.label, "cli");
  const until = periodEnds(s.now);
  return done(
    "warm",
    { label: typed(a), outcome: "started", until },
    `Warmed up ${b(typed(a))}: its 5-hour period runs until ${at(until)}.`,
  );
}

function warmTurn(m: Machine, s: State, text: string, on: boolean): Report {
  const a = resolve(seenFrom(s, m.platform), text);
  if (isFailed(a)) return refused("warm", on ? "warm-on" : "warm-off", text, a);
  if (a.tool !== "claude") return refusal("warm", unsupported(a));
  const changed = s.warm.on.includes(a.id) !== on;
  turnWarmUp(a.label, on, "cli");
  const name = b(text);
  const said = on
    ? changed
      ? `Pitboard warms up ${name} from now on, within the span \`pitboard warm\` shows.`
      : `${name} was warmed up already.`
    : changed
      ? `Pitboard no longer warms up ${name}.`
      : `${name} was not warmed up.`;
  return done("warm", { label: text, on, changed }, said);
}

// warm/mod.rs `Range::parse`: "06:30" as minutes from midnight, 24:00 a day's end.
function minutes(text: string): number | null {
  const m = /^(\d{2}):(\d{2})$/.exec(text.trim());
  if (!m) return null;
  const [h, min] = [Number(m[1]), Number(m[2])];
  const total = h * 60 + min;
  return min < 60 && total <= 24 * 60 ? total : null;
}

function warmRange(from: string, to: string): Report {
  const start = minutes(from);
  const end = minutes(to);
  const usage = (why: string) =>
    refusal("warm", { code: "usage", message: `\`${from} ${to}\`: ${why}` });
  if (start === null || start >= 24 * 60 || end === null)
    return usage("give each time as hours and minutes, such as 06:00 or 22:30");
  const stop = end === 0 ? 24 * 60 : end;
  if (start === stop) return usage("the span starts and ends at the same time");
  setWarmRange(start, stop, "cli");
  return done(
    "warm",
    { range: { from: clock(start), to: clock(stop) } },
    plain(`Pitboard warms up accounts from ${clock(start)} to ${clock(stop)}.`),
  );
}

// watch.rs `service`
function watchInstall(m: Machine, at: number): Report {
  installWatch(at, "cli");
  return done(
    "watch",
    located(m.platform, "watch", { threshold: at }),
    plain(
      `Pitboard watches Claude Code in the background from now on, and at each sign-in. It switches once a limit of the account in use reaches ${at}%, and warms up the accounts \`pitboard warm on\` turned on.`,
    ),
    ...(m.platform === "linux"
      ? [
          plain(
            "Your user's systemd runs it while you are signed in. To keep it running when you are not, run `loginctl enable-linger`.",
          ),
        ]
      : []),
    plain(location(m.platform, "watch")[1]),
    "",
    plain(TERMS),
  );
}

function watchStatus(m: Machine, s: State): Report {
  if (!s.watchService)
    return done(
      "watch",
      { installed: false },
      "Nothing watches Claude Code in the background here.",
      plain("Run `pitboard watch install` to change that."),
    );
  return done(
    "watch",
    located(m.platform, "watch", { threshold: s.watchService.at }),
    `Pitboard watches Claude Code in the background, and switches once a limit of the account in use reaches ${s.watchService.at}%.`,
    plain(location(m.platform, "watch")[1]),
  );
}

function watchUninstall(): Report {
  const removed = uninstallWatch("cli");
  return done(
    "watch",
    { installed: false, removed },
    removed
      ? "Stopped watching Claude Code in the background."
      : "Nothing was watching Claude Code in the background.",
  );
}

// watch.rs `event`: what a decision came to, as a person reads it.
function decisionLine(d: Decision, s: State, share: number): string | null {
  const from = s.accounts.find((a) => a.tool === "claude" && a.label === s.inUse.claude);
  const at = (t: number) => moment(t, s.now);
  switch (d.decided) {
    case "no-reading":
      return plain(
        `Pitboard is not switching Claude Code: there is no reading of ${from ? typed(from) : ""} from Anthropic yet.`,
      );
    case "below": {
      let said = `Watching ${b(from!.label)}`;
      if (d.nearest) said += `: ${plain(shareOfLimit(d.nearest))}`;
      return `${said}, as of ${at(d.asOf)}.`;
    }
    case "hold": {
      const until = d.why.code === "settling" ? ` It decides again at ${at(d.why.until)}.` : "";
      return `${b(from!.label)} has used ${plain(shareOfLimit(d.limit))}. Pitboard is not switching: ${plain(notSwitching(d.why, share))}.${until}`;
    }
    case "switch":
      return null;
  }
}

const switchedLine = (from: string, to: string, limit: string) =>
  `Switched Claude Code from ${b(from)} to ${b(to)}: ${b(from)} has used ${plain(limit)}. Claude Code sessions already running follow within ${ADOPTION_CEILING_SECONDS} seconds.`;

// watch.rs `stamp`
const stamped = (s: State, line: string) => {
  const d = new Date((s.now + ZONE) * 1000);
  const two = (n: number) => String(n).padStart(2, "0");
  return `${dim(`${two(d.getUTCHours())}:${two(d.getUTCMinutes())}:${two(d.getUTCSeconds())}`)} ${line}`;
};

// `pitboard watch`: what it decides, said whenever it says something new, until Ctrl+C. While
// it runs the model switches Claude Code at its share, as the watch service does.
function watchRun(s: State, share: number): Step {
  const on = s.accounts.filter((a) => a.tool === "claude" && s.warm.on.includes(a.id));
  const warms = on.length
    ? `, and warms up ${joined(on.map((a) => b(a.label)))} from ${span(s)}`
    : "";
  // watch.rs `Told`
  let told = new Set<string>();
  let watching: string | null = null;
  let decidedAt: number | null = null;
  let logged = s.log.length;
  watchInTerminal(share);
  const say = (state: State): string[] => {
    const out: string[] = [];
    for (const e of state.log.slice(logged)) {
      if (e.caller !== "cli" || e.outcome !== "ok") continue;
      if (e.verb === "auto-switch") {
        const last = state.lastSwitch.claude!;
        const from = state.accounts.find((a) => a.tool === "claude" && a.label === last.from);
        const limit = from?.limits
          .filter((l) => whole(l.percent) >= share)
          .sort((x, y) => y.percent - x.percent)[0];
        out.push(
          stamped(state, switchedLine(last.from, last.to, limit ? shareOfLimit(limit) : "")),
        );
        told = new Set();
        watching = null;
        // the pass that switched says nothing more; the next says what it watches
        decidedAt = state.decidedAt;
      }
      if (e.verb === "warm-up") {
        const a = state.accounts.find((x) => x.tool === "claude" && x.label === e.subject);
        const started = a ? state.warm.started[a.id] : undefined;
        out.push(
          stamped(
            state,
            started !== undefined
              ? `Warmed up ${b(e.subject)}: its 5-hour period runs until ${moment(periodEnds(started), state.now)}.`
              : `Warmed up ${b(e.subject)}. Anthropic has not said yet when its 5-hour period resets.`,
          ),
        );
      }
    }
    logged = state.log.length;
    if (state.decidedAt !== decidedAt) {
      decidedAt = state.decidedAt;
      const d = decide(state, share);
      const line = decisionLine(d, state, share);
      if (line && d.decided === "below") {
        if (watching !== state.inUse.claude) out.push(stamped(state, line));
        watching = state.inUse.claude;
      } else if (line) {
        const key =
          d.decided === "hold"
            ? `${d.why.code}/${d.limit.lengthSeconds}/${d.limit.resetsAt}`
            : d.decided;
        if (!told.has(key)) out.push(stamped(state, line));
        told.add(key);
        watching = null;
      }
    }
    return out;
  };
  return {
    lines: [
      `Watching Claude Code. Pitboard switches to another of your accounts once a limit of the one in use reaches ${share}%${warms}. Stop with Ctrl-C.`,
      ...say(model.get()),
    ],
    runs: { next: say, stop: () => watchInTerminal(null) },
  };
}

// `pitboard watch --once`: one decision from what was last read, which may switch.
function watchOnce(s: State, share: number): string[] {
  const d = decide(s, share);
  if (d.decided !== "switch") return [decisionLine(d, s, share) ?? ""];
  const from = s.inUse.claude;
  decideOnce(share);
  return [switchedLine(from, d.to.label, shareOfLimit(d.limit))];
}

// main.rs `schedule`
function schedule(m: Machine, s: State, what: string): Report {
  const [key, where] = location(m.platform, "renew");
  if (what === "status")
    return s.schedule
      ? done(
          "schedule",
          located(m.platform, "renew", { every_seconds: DAY }),
          "Parked logins are renewed every 1d 0h by this computer's own scheduler.",
          plain(where),
        )
      : done(
          "schedule",
          { installed: false },
          plain(
            "Nothing is keeping parked logins alive here. They are renewed when you run `pitboard`, and otherwise not.",
          ),
          plain("Run `pitboard schedule install` to change that."),
        );
  if (what === "install") {
    setSchedule(true);
    return done(
      "schedule",
      { installed: true, [key]: where },
      "Parked logins will be renewed daily.",
      plain(where),
      "",
      "It renews your own parked logins and does nothing else: it never switches account and never asks Anthropic for usage.",
    );
  }
  const removed = setSchedule(false);
  return done(
    "schedule",
    { installed: false, removed },
    removed
      ? plain(
          "Stopped renewing parked logins on a schedule. They are renewed when you run `pitboard`, and otherwise not.",
        )
      : "There was nothing scheduled.",
  );
}

// main.rs `renew`, with words.rs `renewal_note`: the parked logins due within three days.
function renew(m: Machine, s: State): Report {
  const due = seenFrom(s, m.platform).accounts.filter(
    (a) =>
      s.inUse[a.tool] !== a.label &&
      a.refreshExpiresAt !== undefined &&
      renewalDue(a.refreshExpiresAt, s.now),
  );
  const note =
    due.length === 0
      ? "No parked login was due."
      : due.length === 1
        ? "Renewed one."
        : `Renewed all ${due.length}.`;
  return done(
    "renew",
    {
      accounts: due.map((a) => ({ label: typed(a), provider: a.tool, outcome: "renewed" })),
      renewed: due.length,
    },
    note,
  );
}

// main.rs `uninstall_question` and `uninstall`
const uninstallQuestion = (platform: Platform) =>
  `Delete every parked login this Pitboard wrote, the daily renewal schedule, the watch service and ${folder(platform)}? Each tool stays signed in to the account in use; the others need signing in again. [y/N] `;

function uninstall(m: Machine): Report {
  const s = model.get();
  const parked = seenFrom(s, m.platform).accounts.filter((a) => s.inUse[a.tool] !== a.label);
  const out = [
    `Removed ${parked.length} parked login(s). The login each tool is signed in with is untouched.`,
  ];
  const watch = s.watchService !== null;
  if (watch) out.push("Stopped the watch service.");
  if (s.schedule) out.push("Turned off the daily renewal schedule.");
  out.push(
    plain(`${folder(m.platform)} is gone. Remove Pitboard itself the way you installed it.`),
  );
  const data = {
    parks_removed: parked.length,
    parks_pending: 0,
    parks_left: 0,
    home_removed: true,
    home_kept_by: [],
    warming_up: false,
    schedule_removed: s.schedule,
    watch_service_removed: watch,
    set_aside_removed: 0,
  };
  for (const a of parked) forgotten(a.tool, a.label, "cli");
  uninstallWatch("cli");
  setSchedule(false);
  logEntry("cli", "uninstall", "");
  return { command: "uninstall", data, human: out };
}

// error.rs `ConfirmationNeeded`: `--json` asks nobody.
const confirmationNeeded = (command: string): Report =>
  refusal(command, {
    code: "confirmation_needed",
    message: `\`pitboard ${command}\` asks before it deletes anything, and can ask only in a terminal without \`--json\`. Nothing was changed. Run it in a terminal, or add \`--yes\`.`,
  });

// `pitboard doctor`, as the state stands on this system.
function doctor(m: Machine, s: State): string[] {
  const paint = { ok: green, warn: yellow, fail: red };
  const rows = doctorRows(m.doctor, s, m.platform);
  const worst = rows.some((r) => r.level === "fail")
    ? "fail"
    : rows.some((r) => r.level === "warn")
      ? "warn"
      : "ok";
  return laidOut(rows).map((line) => {
    switch (line.kind) {
      case "row":
        return `${paint[line.level!](markOf[line.level!])} ${plain(line.text.slice(2))}`;
      case "advice": {
        const [, gap, said] = /^( +)(.*)$/.exec(line.text)!;
        return `${gap}${dim(said)}`;
      }
      case "heading":
        return b(line.text);
      case "summary":
        return paint[worst](line.text);
      default:
        return "";
    }
  });
}

const ask = (question: string, go: () => Step): Step => ({
  lines: [],
  ask: { question, answer: (reply) => (agreed(reply) ? go() : { lines: [] }) },
});

// Runs `pitboard` with `words`, as the shell split them.
export async function run(m: Machine, words: string[]): Promise<Step> {
  // main.rs `parse`: with `--json` anywhere, a usage error is an envelope too
  const json = words.includes("--json");
  const parsed = parse(m.grammar, words);
  if (parsed.kind === "version") return { lines: [plain(`pitboard ${m.version}`)] };
  if (parsed.kind === "help" || parsed.kind === "error") {
    const lines = parsed.kind === "help" ? helpLines(m.grammar, parsed.path) : parsed.lines;
    const failed = parsed.kind === "error" || parsed.missing === true;
    if (!json || (parsed.kind === "help" && !parsed.missing)) return { lines, failed };
    const message = lines
      .map((l) => plainText(l))
      .join("\n")
      .trim();
    return { lines: emit(refusal(null, { code: "usage", message }), true), failed };
  }
  const leaf = parsed.levels.at(-1)!;
  const path = leaf.path.slice(1).join(" ");
  // departs from the apps: `--json` of `pitboard`, status, doctor and watch prints nothing
  if (json && ["", "status", "doctor", "watch"].includes(path)) return { lines: [] };
  const s = model.get();
  const out = (r: Report): Step => ({ lines: emit(r, json), failed: !!(r.error ?? r.failure) });
  const at = () => Number(value(leaf, "at"));
  switch (path) {
    case "":
      return out(status(m, s, false));
    case "status":
      return out(status(m, s, flag(leaf, "offline")));
    case "use":
      return out(use(m, s, value(leaf, "label")));
    case "forget": {
      const label = value(leaf, "label");
      const go = () => out(forget(m, label));
      if (flag(leaf, "yes")) return go();
      // main.rs: a label that names nothing gets its own error, asked or not
      const named = resolve(seenFrom(s, m.platform), label);
      if (json) return isFailed(named) ? go() : out(confirmationNeeded("forget"));
      return ask(forgetQuestion(isFailed(named) ? null : named.tool, label), go);
    }
    case "rename":
      return out(rename(m, s, value(leaf, "from"), value(leaf, "to")));
    case "enroll": {
      if (!flag(leaf, "sign_in")) return out(enrollCurrent(m, s, value(leaf, "label")));
      const signing = enrollSigningIn(m, s, value(leaf, "label"));
      if (!Array.isArray(signing)) return out(signing);
      return { lines: signing, runs: { next: () => [], stop: () => {} } };
    }
    case "log":
      return out(log(m, s, Number(value(leaf, "lines"))));
    case "warm":
      return out(warmOverview(s));
    case "warm now":
      return out(warmNow(m, s, value(leaf, "label")));
    case "warm on":
      return out(warmTurn(m, s, value(leaf, "label"), true));
    case "warm off":
      return out(warmTurn(m, s, value(leaf, "label"), false));
    case "warm range":
      return out(warmRange(value(leaf, "from"), value(leaf, "to")));
    case "watch":
      return flag(leaf, "once") ? { lines: watchOnce(s, at()) } : watchRun(s, at());
    case "watch install":
      return out(watchInstall(m, at()));
    case "watch status":
      return out(watchStatus(m, s));
    case "watch uninstall":
      return out(watchUninstall());
    case "schedule install":
    case "schedule status":
    case "schedule uninstall":
      return out(schedule(m, s, leaf.path.at(-1)!));
    case "renew":
      return out(renew(m, s));
    case "statusline": {
      const line = statusLine(seenFrom(s, m.platform));
      return out(done("statusline", { line: plainText(line) }, line));
    }
    case "doctor":
      return { lines: doctor(m, seenFrom(s, m.platform)) };
    case "uninstall": {
      const go = () => out(uninstall(m));
      if (flag(leaf, "yes")) return go();
      if (json) return out(confirmationNeeded("uninstall"));
      return ask(uninstallQuestion(m.platform), go);
    }
    case "stow":
      return out(
        done(
          "stow",
          { stowed: false },
          "Nothing is left behind the store Claude Code keeps its login in, so there is nothing to put away.",
        ),
      );
    case "repair":
      return out(
        done(
          "repair",
          { given_back: [], deleted: [], strangers: [], unreadable: [], set_aside: [] },
          "Every parked login here is accounted for.",
        ),
      );
    case "abandon":
      return out(
        done(
          "abandon",
          { abandoned: false, moves_given_up: [] },
          "There is no interrupted switch to give up on.",
        ),
      );
    case "adopt":
      return out(
        done(
          "adopt",
          { adopted: false },
          "This Pitboard directory was already written on this computer.",
        ),
      );
    case "completions": {
      if (json)
        return out({
          command: "generate",
          data: {},
          failure: {
            code: "output_is_not_a_report",
            message: "this command writes a generated file to stdout, so it has no JSON form",
          },
        });
      const script = await m.script(value(leaf, "shell"));
      return { lines: script === null ? [] : script.split("\n").map(plain) };
    }
    default:
      return { lines: [] };
  }
}
