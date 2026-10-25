// The window's This Mac pane: doctor's checks as present/machine.rs `checks` words them for
// MachinePane.swift, from what the site's macOS terminal prints for `pitboard doctor`.
import { summary, type Level, type Row } from "../lib/doctor.ts";

export type { Level };

export interface CheckLine {
  name: string;
  level: Level;
  detail: string;
  // shown only for a check that did not pass
  advice: string | null;
}

// words.rs `capitalised` and `prose_capitalised`: a detail that starts with a plain lower-case
// word is capitalised, and one that starts with a path, a name or a number is not
const capitalised = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);
function prose(text: string): string {
  const first = text.split(/[\s,;:]/)[0];
  return first.split("-").every((part) => /^[a-z]+$/.test(part)) ? capitalised(text) : text;
}

// words.rs `typeset`: what the core quotes in backticks keeps every letter and loses its
// backticks, and an apostrophe elsewhere curls
function typeset(core: string): string {
  return core
    .split("`")
    .map((part, i) => (i % 2 === 1 ? part : part.replace(/'/g, "’")))
    .join("");
}

// words.rs `check_name`: the app's own names for two checks, and doctor's for the rest
function checkName(name: string): string {
  if (name === "storage v5") return "Claude Code’s new login storage";
  if (name === "machine ID") return "Machine ID";
  return typeset(capitalised(name));
}

// Each check doctor made, in its order, as present/machine.rs `checks` words it: the advice under a
// check that did not pass, and the sentence over them.
export function checksOf(rows: Row[]): { lines: CheckLine[]; summary: string } {
  return {
    lines: rows.map((r) => ({
      name: checkName(r.name),
      level: r.level,
      detail: typeset(prose(r.detail)),
      advice: r.level !== "ok" && r.advice ? typeset(prose(r.advice)) : null,
    })),
    summary: summary(rows),
  };
}

// words.rs `spoken_level`
export const spokenLevel: Record<Level, string> = {
  ok: "Passed",
  warn: "Worth looking at",
  fail: "Failed",
};

// words.rs `checked_at` and `checking`
export const checkedAt = (clock: string) => `Checked at ${clock}`;
export const checking = (system: "macos" | "windows") =>
  `Checking ${system === "macos" ? "this Mac" : "this PC"}…`;
