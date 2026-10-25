// The Activity pane and page (plan 3.8, answer 14): the log as the app reads it, when the pane
// comes into view and on its Refresh, and not as it changes (ActivityPane.swift, state.rs's one
// ReadLog). Newest first, in the rows the window has room for, as a table that has more scrolls
// them out of sight; no selection and no sorting, as the app takes neither.
import { shown, text } from "./draw.ts";
import { LiveFigure } from "./figure.ts";
import { model, type State } from "../../lib/model.ts";
import { activity } from "../../lib/words.ts";

class ActivityPane extends LiveFigure {
  #system: "macos" | "windows" = "macos";
  #seen: IntersectionObserver | null = null;

  connectedCallback() {
    const drawing = this.closest<HTMLElement>("[data-drawing]")?.dataset.drawing ?? "";
    this.#system = drawing.endsWith("windows") ? "windows" : "macos";
    this.addEventListener("click", (e) => {
      if ((e.target as Element).closest('[data-does="refresh"]')) this.#read(model.get());
    });
    this.#seen = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) this.#read(model.get());
    });
    this.#seen.observe(this);
    super.connectedCallback();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.#seen?.disconnect();
  }

  // the pane draws what it read, whatever changes after
  protected render() {}

  #read(s: State) {
    const rows = [...this.querySelector('[data-part="log"]')!.children] as HTMLElement[];
    const read = activity(s.log, this.#system);
    rows.forEach((row, i) => {
      const e = read[i];
      const cells = [...row.children] as HTMLElement[];
      const words = e ? [e.date, e.change, e.account, e.result, e.by] : ["", "", "", "", ""];
      cells.forEach((cell, j) => text(cell, words[j]));
      if (this.#system === "macos") {
        if (e) row.removeAttribute("aria-hidden");
        else row.setAttribute("aria-hidden", "true");
      } else shown(row, !!e);
    });
  }
}

customElements.define("pb-activity", ActivityPane);
