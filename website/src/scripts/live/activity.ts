// The Activity pane and page (plan 3.8, answer 14): the log as the app reads it, when the pane
// shows and on its Refresh, and not as it changes (ActivityPane.swift, state.rs's one ReadLog).
// Newest first, in the rows the window has room for, as a table that has more scrolls them out of
// sight; no selection and no sorting, as the app takes neither.
import { shown, text } from "./draw.ts";
import type { State } from "../../lib/model.ts";
import { activity } from "../../lib/words.ts";

export function readLog(within: ParentNode, system: "macos" | "windows", s: State) {
  const rows = [...within.querySelector('[data-part="log"]')!.children] as HTMLElement[];
  const read = activity(s.log, system);
  rows.forEach((row, i) => {
    const e = read[i];
    const cells = [...row.children] as HTMLElement[];
    const words = e ? [e.date, e.change, e.account, e.result, e.by] : ["", "", "", "", ""];
    cells.forEach((cell, j) => text(cell, words[j]));
    if (system === "macos") {
      if (e) row.removeAttribute("aria-hidden");
      else row.setAttribute("aria-hidden", "true");
    } else shown(row, !!e);
  });
}
