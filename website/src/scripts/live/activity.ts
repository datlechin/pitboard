// The Activity pane and page (plan 3.8): the log is read when the pane shows and on Refresh,
// not as it changes, as ActivityPane.swift reads it.
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
