// What `pitboard doctor` finds on each system, from the snapshot crates/pitboard's tests keep of a
// healthy machine in the site's world: the rows that name an account are drawn again from the
// state, in doctor/claude.rs, parks.rs, codex.rs and desktop.rs's words, and laid out as
// render/doctor.rs `human` lays them out.
import { typed, type State } from "./model.ts";
import type { Platform } from "./platform.ts";
import { moment, parkedLifeColumn, renewalDue, sessionLifeColumn } from "./words.ts";

export type Level = "ok" | "warn" | "fail";

export interface Row {
  level: Level;
  name: string;
  detail: string;
  advice: string;
  // the tool whose heading the row is under, or null for Claude Code's and Pitboard's own
  section: string | null;
}

const marks: Record<string, Level> = { "✓": "ok", "!": "warn", "✗": "fail" };
export const markOf: Record<Level, string> = { ok: "✓", warn: "!", fail: "✗" };

// The rows of a snapshot `human` printed, without its header and its last line.
export function rowsOf(printed: string): Row[] {
  const body = printed.split("\n---\n")[1] ?? printed;
  const rows: Row[] = [];
  let section: string | null = null;
  for (const line of body.split("\n")) {
    if (line.trim() === "") continue;
    const level = marks[line[0]];
    if (level) {
      const [, name, detail] = /^. (.+?)\s{2,}(.*)$/.exec(line)!;
      rows.push({ level, name, detail, advice: "", section });
    } else if (line.startsWith(" ") && rows.length) rows.at(-1)!.advice = line.trim();
    else if (!line.endsWith(".")) section = line;
  }
  return rows;
}

// doctor/words.rs
const enrolled = (n: number) => (n === 0 ? "none enrolled yet" : `${n} enrolled`);
const signInAgain = (account: string) => `Run \`pitboard enroll ${account} --sign-in\`.`;
const EXPIRED = "This login has expired: sign in to Claude Code again.";
const EXPIRES_SOON = "This login expires soon and will need signing in again.";
const ifClaudeAsks = (account: string) =>
  `If Claude asks you to sign in once you use it, run \`pitboard enroll ${account} --sign-in\`.`;

// words.rs `doctor_summary`
export function summary(rows: Row[]): string {
  const failed = rows.filter((r) => r.level === "fail").length;
  const worth = rows.filter((r) => r.level === "warn").length;
  if (failed) return `${failed} broken: do not switch accounts until fixed.`;
  if (worth === 1) return "One thing is worth looking at.";
  if (worth) return `${worth} things are worth looking at.`;
  return "Everything Pitboard checks is in order.";
}

// parks.rs `judge_park`, for an account that is not Claude Desktop's
function park(s: State, a: State["accounts"][number]): Row {
  const name = `account ${typed(a)}`;
  const row = (level: Level, detail: string, advice = ""): Row => ({
    level,
    name,
    detail,
    advice,
    section: null,
  });
  if (s.inUse[a.tool] === a.label) return row("ok", "signed in; parked when you switch away");
  const at = a.refreshExpiresAt;
  if (at === undefined) return row("ok", "parked");
  const life = parkedLifeColumn(at, s.now);
  if (at <= s.now)
    return row("warn", `its parked login ${life} ${moment(at, s.now)}`, signInAgain(typed(a)));
  if (renewalDue(at, s.now)) return row("warn", `its parked login ${life}`, signInAgain(typed(a)));
  return row("ok", `parked, ${life}`);
}

// desktop.rs `judge_desktop_park`, for a Claude Desktop account parked
function desktopPark(s: State, a: State["accounts"][number]): Row {
  const name = `account ${typed(a)}`;
  const at = a.sessionExpiresAt;
  if (at === undefined) return { level: "ok", name, detail: "parked", advice: "", section: null };
  const detail = `parked, ${sessionLifeColumn(at, s.now)}`;
  return at > s.now
    ? { level: "ok", name, detail, advice: "", section: null }
    : { level: "warn", name, detail, advice: ifClaudeAsks(typed(a)), section: null };
}

// What doctor finds on `platform` with the state as it stands: the snapshot's rows, those that name
// an account drawn from the state, each in its section.
export function doctorRows(snapshot: Row[], s: State, platform: Platform): Row[] {
  const accounts = s.accounts.filter((a) => platform === "macos" || a.tool !== "desktop");
  const inUse = (tool: "claude" | "codex" | "desktop") =>
    accounts.find((a) => a.tool === tool && a.label === s.inUse[tool]);
  const claude = inUse("claude");
  const codex = inUse("codex");
  const desktop = inUse("desktop");
  // in_use.rs: Anthropic named the login at the last read, or at the switch that put it in use
  const named = Math.max(s.readAt, s.lastSwitch.claude?.at ?? 0);
  const out: Row[] = [];
  for (const r of snapshot) {
    const section = r.section;
    const put = (rows: Row[]) => out.push(...rows.map((x) => ({ ...x, section })));
    if (r.name.startsWith("account ")) continue;
    if (r.name === "identity" && claude) {
      put([{ ...r, detail: `${claude.email}  ·  org ${claude.org ?? ""}` }]);
    } else if (r.name === "in use" && claude) {
      put([{ ...r, detail: `\`${typed(claude)}\`, as Anthropic said ${moment(named, s.now)}` }]);
    } else if (r.name === "credential" && claude) {
      const at = claude.refreshExpiresAt;
      const life = at === undefined ? "" : `  ·  ${parkedLifeColumn(at, s.now)}`;
      const detail = `refresh ${claude.login ?? ""}${life}  ·  keys ["claudeAiOauth"]`;
      const advice =
        at === undefined ? "" : at <= s.now ? EXPIRED : renewalDue(at, s.now) ? EXPIRES_SOON : "";
      put([{ ...r, detail, level: advice ? "warn" : "ok", advice }]);
    } else if (r.name === "accounts") {
      put([{ ...r, detail: enrolled(accounts.length) }]);
      put(accounts.filter((a) => a.tool === "claude").map((a) => park(s, a)));
    } else if (r.name === "Codex login" && codex) {
      put([
        {
          ...r,
          detail: `${codex.email}  ·  account ${codex.uuid ?? ""}  ·  refresh ${codex.login ?? ""}`,
        },
      ]);
      put(accounts.filter((a) => a.tool === "codex").map((a) => park(s, a)));
    } else if (r.name === "Claude Desktop in use" && desktop) {
      const read = desktop.readAt ?? s.readAt;
      put([
        {
          ...r,
          detail: `\`${typed(desktop)}\`, as Pitboard last read it ${moment(read, s.now)}`,
        },
      ]);
      put(
        accounts
          .filter((a) => a.tool === "desktop" && a.label !== s.inUse.desktop)
          .map((a) => desktopPark(s, a)),
      );
    } else put([r]);
  }
  return out;
}

// render/doctor.rs `human`: each section's names padded to its longest, a heading where each
// other tool's section starts, advice under its check, and the sentence the checks come to.
export function laidOut(rows: Row[]): {
  kind: "row" | "advice" | "heading" | "blank" | "summary";
  level?: Level;
  text: string;
}[] {
  const out: ReturnType<typeof laidOut> = [];
  let section: string | null | undefined = undefined;
  const width = (of: string | null) =>
    Math.max(...rows.filter((r) => r.section === of).map((r) => r.name.length));
  for (const r of rows) {
    if (r.section !== section) {
      if (r.section !== null) {
        out.push({ kind: "blank", text: "" });
        out.push({ kind: "heading", text: r.section });
      }
      section = r.section;
    }
    const w = width(r.section);
    out.push({
      kind: "row",
      level: r.level,
      text: `${markOf[r.level]} ${r.name.padEnd(w)}  ${r.detail}`,
    });
    if (r.advice) out.push({ kind: "advice", text: `  ${"".padEnd(w)}  ${r.advice}` });
  }
  out.push({ kind: "blank", text: "" });
  out.push({ kind: "summary", text: summary(rows) });
  return out;
}
