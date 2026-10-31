// The Activity window and page (plan 3.8): the log is read when either shows and on Refresh, not
// as it changes, as ActivityWindow.swift reads it.
import { shown, text } from "./draw.ts";
import { LiveFigure } from "./figure.ts";
import { model, type State } from "../../lib/model.ts";
import { platformOf, type System } from "../../lib/platform.ts";
import { activity } from "../../lib/words.ts";

export function readLog(within: ParentNode, system: System, s: State) {
  const rows = [...within.querySelector('[data-part="log"]')!.children] as HTMLElement[];
  const read = activity(s.log, platformOf(system));
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

// <pb-activity>: macOS's Activity window, which reads the log as it comes into view
class ActivityWindow extends LiveFigure {
  #seen?: IntersectionObserver;

  connectedCallback() {
    this.#seen = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) readLog(this, "macos", model.get());
    });
    this.#seen.observe(this);
    this.addEventListener("click", (e) => {
      if ((e.target as Element).closest('[data-does="read"]')) readLog(this, "macos", model.get());
    });
    super.connectedCallback();
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.#seen?.disconnect();
  }

  protected render() {}

  // View's Refresh reads the log, as the window's own button does
  protected windowMenus() {
    return { account: null, refresh: { title: "Refresh", enabled: true }, resizes: true };
  }

  protected fromMenuBar(does: string): boolean {
    if (does !== "refresh") return false;
    readLog(this, "macos", model.get());
    return true;
  }
}

customElements.define("pb-activity", ActivityWindow);
