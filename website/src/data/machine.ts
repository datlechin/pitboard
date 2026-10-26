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

// words.rs `check_name`: the app's own names for doctor's checks, by their codes, which the
// printed checks name as doctor does, in macOS's title case; doctor's own name for the rest
export const appNames: [code: string, doctor: string, app: string][] = [
  ["storage_v5", "storage v5", "New Sign-In Storage"],
  ["machine_id", "machine id", "Machine ID"],
  ["vault", "parked logins", "Saved Sign-Ins"],
  ["pending_parks", "parks being reclaimed", "Saved Sign-Ins Being Deleted"],
  ["discarded", "old parked logins", "Old Saved Sign-Ins"],
  ["desktop_parks_folder", "parked Claude Desktop logins", "Saved Sign-Ins"],
  ["desktop_set_aside", "folders set aside", "Folders Moved Out of the Way"],
  ["desktop_move", "interrupted Claude Desktop move", "Switch That Didn’t Finish"],
  ["interrupted_switch", "interrupted switch", "Switch That Didn’t Finish"],
  ["credential", "credential", "Sign-In"],
  ["credential_store", "credential store", "Sign-In Store"],
  ["credential_size", "login size", "Sign-In Size"],
  ["fallback_login", "fallback login", "Leftover Sign-In File"],
  ["codex_login", "Codex login", "Signed-In Account"],
  ["codex_auth_file", "Codex login file", "Sign-In File"],
  ["codex_backend", "Codex login store", "Sign-In Store"],
  ["private_on_disk", "logins on disk", "Sign-Ins on Disk"],
  ["desktop_awaiting", "awaited sign-in", "Sign-In in Claude"],
];
const checkName = (name: string) =>
  appNames.find(([, doctor]) => doctor === name)?.[2] ?? typeset(capitalised(name));

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
  warn: "Warning",
  fail: "Failed",
};

// words.rs `checked_at` and `checking`
export const checkedAt = (clock: string) => `Checked at ${clock}`;
export const checking = (system: "macos" | "windows") =>
  `Checking ${system === "macos" ? "this Mac" : "this PC"}…`;
