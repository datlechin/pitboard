// The app's and the command line's words for world.ts, by crates/pitboard-core/src/words.rs and
// crates/pitboard/src/render/status.rs.

import {
  accounts,
  inUse,
  logged,
  toolOn,
  tools,
  type Account,
  type Limit,
  type Logged,
  type Moment,
  type Tool,
} from "../data/world.ts";
import type { Platform } from "./platform.ts";

export type Standing = "over" | "under" | "even";

// More than 5 points either side of an even pace counts (concepts/usage#pace).
export function standing(limit: Limit): Standing | null {
  if (limit.even === null) return null;
  const delta = limit.used - limit.even;
  if (delta > 5) return "over";
  if (delta < -5) return "under";
  return "even";
}

export function paceText(limit: Limit): string {
  const s = standing(limit);
  if (s === null) return "";
  if (s === "even") return "on pace";
  return `${Math.round(Math.abs(limit.used - limit.even!))}% ${s} pace`;
}

// the app draws "y" in orange
export function level(used: number): "g" | "y" | "r" {
  return used >= 90 ? "r" : used >= 70 ? "y" : "g";
}

const sentenceName = { "5h": "5-hour", week: "weekly" } as const;

export const isInUse = (a: Account, moment: Moment) => inUse[moment][a.tool] === a.label;

export const toolAccounts = (tool: Tool) => accounts.filter((a) => a.tool === tool);

export const toolsOn = (platform: Platform) =>
  tools.filter(([tool]) => toolOn[tool].includes(platform));

const toolName = (tool: Tool) => tools.find(([t]) => t === tool)![1];

// As every listing gives a tool's accounts: the one in use first (status.rs `assemble`).
export const listed = (tool: Tool, moment: Moment) =>
  [...toolAccounts(tool)].sort((a, b) => Number(isInUse(b, moment)) - Number(isInUse(a, moment)));

export const menuTitle = (a: Account) => (a.plan ? `${a.label} · ${a.plan}` : a.label);

export function menuLine(a: Account, moment: Moment): string {
  if (a.limits.length === 0) return "No usage read";
  const said = a.limits.map((l) => {
    const runs = isInUse(a, moment) && l.runsOut ? ` (runs out in ${l.runsOut})` : "";
    return `${sentenceName[l.kind]} ${l.used}%${runs}`;
  });
  const line = said.join(", ");
  return line[0].toUpperCase() + line.slice(1);
}

// The in-use account closest to running out, a tie going to Claude Code (reference/mac-app#menu-bar-item).
export const fullest = (a: Account) => Math.max(...a.limits.map((l) => l.used));
const titled = (moment: Moment) =>
  (["claude", "codex"] as Tool[])
    .map((tool) => accounts.find((a) => a.tool === tool && isInUse(a, moment))!)
    .reduce((best, a) => (fullest(a) > fullest(best) ? a : best));

export function menuBarTitle(moment: Moment): string {
  const shown = titled(moment);
  return `${shown.label} ${fullest(shown)}%`;
}

// The icon's tooltip in the notification area: Pitboard, then each account in use with its tool and
// the line the menu shows under it (present/accounts.rs `menu_bar`).
export const trayTip = (moment: Moment) =>
  [
    "Pitboard",
    ...(["claude", "codex"] as Tool[]).map((tool) => {
      const a = accounts.find((a) => a.tool === tool && isInUse(a, moment))!;
      return `${a.label} (${toolName(tool)}): ${menuLine(a, moment)}`;
    }),
  ].join("\n");

export function runsOutNote(a: Account, moment: Moment): string | null {
  const l = isInUse(a, moment) ? a.limits.find((l) => l.runsOut) : undefined;
  return l ? `${sentenceName[l.kind]} limit runs out in ${l.runsOut} at this pace` : null;
}

const span = (cls: string, text: string) => (text ? `<span class="${cls}">${text}</span>` : "");
const pad = (text: string, width: number) => text + " ".repeat(Math.max(0, width - text.length));

// crates/pitboard/src/ui.rs `bar`; a run of cells is one span, drawn as one block of its colour.
const cell = "bg-current [-webkit-text-fill-color:transparent]";
const shade = "bg-current/32 [-webkit-text-fill-color:transparent]";
function bar(limit: Limit): string {
  const width = 10;
  const filled = Math.min(width, Math.max(0, Math.round((limit.used / 100) * width)));
  const cells = (from: number, to: number) => {
    const full = Math.min(Math.max(filled, from), to);
    return (
      span(`${level(limit.used)} ${cell}`, "█".repeat(full - from)) +
      span(`dim ${shade}`, "░".repeat(to - full))
    );
  };
  const s = standing(limit);
  if (s === null || s === "even") return cells(0, width);
  const at = Math.min(Math.floor((limit.even! / 100) * width), width - 1);
  return cells(0, at) + span(s === "over" ? "r" : "g", "│") + cells(at + 1, width);
}

// A tool's heading in `pitboard`, with what is true of all its accounts said once on it.
export function statusHeading(tool: Tool): string {
  const name = `<b>${toolName(tool)}</b>`;
  return tool === "desktop"
    ? `${name}${span("dim", " · Pitboard reads no usage for Claude Desktop")}`
    : name;
}

function standingText(a: Account, on: boolean): string {
  if (on)
    return a.read
      ? `${span("g", "signed in")} ${span("dim", `· read ${a.read} ago`)}`
      : span("g", "signed in");
  const until = a.parked
    ? `good for ${a.parked.cli}`
    : a.lapses
      ? `sign-in may lapse in ${a.lapses.cli}`
      : null;
  return until ? `${span("g", "ready")} ${span("dim", `· ${until}`)}` : span("g", "ready");
}

// `pitboard` for one tool's accounts, one HTML string a line, usage lines only where it is read.
export function statusLines(tool: Tool, moment: Moment): string[] {
  const rows = listed(tool, moment);
  const labelWidth = Math.max(...accounts.map((a) => a.label.length));
  const emailWidth = Math.max(...accounts.map((a) => a.email.length));
  const planWidth = Math.max(...accounts.map((a) => a.plan?.length ?? 0));
  const resetsWidth = Math.max(
    ...accounts.flatMap((a) => a.limits.map((l) => `resets in ${l.resets}`.length)),
  );
  const lines: string[] = [];
  rows.forEach((a, i) => {
    if (i > 0 && a.limits.length > 0) lines.push("");
    const on = isInUse(a, moment);
    lines.push(
      `${on ? span("g", "●") : span("dim", "○")} <b>${pad(a.label, labelWidth)}</b>  ${span("dim", pad(a.email, emailWidth))}  ${span("dim", pad(a.plan ?? "", planWidth))}  ${standingText(a, on)}`,
    );
    for (const l of a.limits) {
      const resets = `resets in ${l.resets}`;
      const pace = paceText(l);
      const after = pace
        ? `${span("dim", pad(resets, resetsWidth))}  ${span(standing(l) === "over" ? "r" : standing(l) === "under" ? "g" : "dim", pace)}`
        : span("dim", resets);
      lines.push(
        `    ${pad(l.kind, 4)}  ${bar(l)}  ${span(level(l.used), `${String(l.used).padStart(3)}%`)}  ${after}`,
      );
    }
    const note = runsOutNote(a, moment);
    if (note) lines.push(`    ${pad("", 4)}  ${span("r", note)}`);
  });
  return lines;
}

// reference/integration-commands#statusline
export function statusLine(moment: Moment): string {
  const share = (l: Limit | undefined, marks: boolean) => {
    if (!l) return span("dim", "?");
    const s = standing(l);
    const mark =
      marks && s === "over" ? span("r", "▲") : marks && s === "under" ? span("g", "▼") : "";
    return `${span(level(l.used), `${l.used}%`)}${mark}`;
  };
  const shares = (a: Account, marks: boolean) =>
    `${share(
      a.limits.find((l) => l.kind === "5h"),
      marks,
    )}${span("dim", "·")}${share(
      a.limits.find((l) => l.kind === "week"),
      marks,
    )}`;
  const claude = toolAccounts("claude");
  const current = claude.find((a) => isInUse(a, moment))!;
  const others = claude.filter((a) => a !== current);
  return [
    `<b>${current.label}</b> ${shares(current, true)}`,
    ...others.map((a) => `${span("dim", a.label)} ${shares(a, false)}`),
  ].join("  ");
}

// The activity list's changes, newest first, in the app's words (present/words.rs `change_verb`,
// `change_outcome` and `change_caller`).
const verbs: Record<string, string> = { use: "Switch", enroll: "Enrol", "warm-up": "Warm-up" };
const callers = { app: "Pitboard app", cli: "Command line" };
const months = "Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec".split(" ");

// As each app dates a change: "13 Oct 2026 at 20:49" on macOS, and in the en-GB regional format's
// "g" on Windows, "13/10/2026 20:49".
const dated = (e: Logged, platform: Platform) => {
  const [year, month, day] = e.day.split("-");
  const time = e.time.slice(0, 5);
  return platform === "windows"
    ? `${day}/${month}/${year} ${time}`
    : `${Number(day)} ${months[Number(month) - 1]} ${year} at ${time}`;
};

export const activity = (platform: Platform) =>
  logged
    .filter((e) => !e.on || e.on.includes(platform))
    .reverse()
    .map((e) => ({
      date: dated(e, platform),
      change: verbs[e.verb],
      account: e.account,
      result: "Done",
      by: callers[e.caller],
    }));
