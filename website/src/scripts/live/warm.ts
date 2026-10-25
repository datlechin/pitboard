// <pb-warm>: Settings at Warm up limits, on macOS or Windows. Its hours and which accounts it warms
// up are the model's warm state, changed at once; its accounts are the page's Claude Code accounts
// in the order they were added, each with the words machine.rs gives it.
import { disclosure } from "./disclosure.ts";
import { LiveFigure } from "./figure.ts";
import { datePicker, timePicker, type Time } from "./time.ts";
import { warmHours } from "../../data/limits.ts";
import {
  model,
  nextWarmUp,
  setWarmRange,
  turnWarmUp,
  warmNote,
  type State,
} from "../../lib/model.ts";
import { menuTitle } from "../../lib/words.ts";

const said = (el: Element | null, text: string) => {
  if (el && el.textContent!.trim() !== text) el.textContent = text;
};

class Warm extends LiveFigure {
  #from!: Time;
  #to!: Time;
  #rows!: HTMLElement;
  #row!: HTMLElement;

  connectedCallback() {
    const part = <T extends HTMLElement = HTMLElement>(name: string) =>
      this.querySelector<T>(`#${this.id}-${name}`)!;
    const from = (m: number) => setWarmRange(m, model.get().warm.to);
    const to = (m: number) => setWarmRange(model.get().warm.from, m);
    if (this.querySelector("[data-stepper]")) {
      this.#from = datePicker(part("from"), from);
      this.#to = datePicker(part("to"), to);
    } else {
      this.#from = timePicker(part<HTMLButtonElement>("from"), part("from-flyout"), from);
      this.#to = timePicker(part<HTMLButtonElement>("to"), part("to-flyout"), to);
      disclosure(part<HTMLButtonElement>("expand"), this.querySelector("[data-header]")!);
    }
    this.#rows = this.querySelector("[data-accounts]")!;
    this.#row = this.querySelector<HTMLTemplateElement>("template[data-row]")!.content
      .firstElementChild as HTMLElement;
    this.#rows.addEventListener("click", (e) => {
      const toggle = (e.target as Element).closest('[role="switch"]');
      const id = toggle?.closest<HTMLElement>("[data-account]")?.dataset.account;
      const a = model.get().accounts.find((x) => x.id === id);
      if (a) turnWarmUp(a.label, toggle!.getAttribute("aria-checked") !== "true");
    });
    // WinUI's ToggleSwitch takes Space alone
    if (!this.querySelector("[data-stepper]"))
      this.#rows.addEventListener("keydown", (e) => {
        if (e.key === "Enter" && (e.target as Element).closest('[role="switch"]'))
          e.preventDefault();
      });
    super.connectedCallback();
  }

  protected render(s: State) {
    this.#from.set(s.warm.from);
    this.#to.set(s.warm.to);
    said(this.querySelector("[data-range]"), warmHours(s.warm.from, s.warm.to));
    const next = nextWarmUp(s);
    const line =
      this.querySelector<HTMLElement>("[data-next-row]") ?? this.querySelector("[data-next]");
    if (line) line.hidden = next === null;
    if (next) said(this.querySelector("[data-next]"), next);

    const have = new Map(
      [...this.#rows.children].map((r) => [(r as HTMLElement).dataset.account!, r as HTMLElement]),
    );
    s.accounts
      .filter((a) => a.tool === "claude")
      .forEach((a, i) => {
        let row = have.get(a.id);
        have.delete(a.id);
        if (!row) row = this.#added(a.id);
        if (this.#rows.children[i] !== row)
          this.#rows.insertBefore(row, this.#rows.children[i] ?? null);
        const title = menuTitle(a);
        said(row.querySelector("[data-title]"), title);
        const note = warmNote(s, a);
        const line = row.querySelector<HTMLElement>("[data-note]")!;
        line.hidden = note === null;
        if (note) said(line, note);
        const on = String(s.warm.on.includes(a.id));
        const toggle = row.querySelector('[role="switch"]')!;
        if (toggle.getAttribute("aria-checked") !== on) toggle.setAttribute("aria-checked", on);
        // Windows names its ToggleSwitch by the account's title, and says On or Off beside it
        if (toggle.hasAttribute("aria-label") && toggle.getAttribute("aria-label") !== title)
          toggle.setAttribute("aria-label", title);
        said(toggle.parentElement!.querySelector("[data-said]"), on === "true" ? "On" : "Off");
      });
    for (const gone of have.values()) gone.remove();
  }

  // A row for an account added on the page, named by its id as Astro names the others.
  #added(id: string): HTMLElement {
    const row = this.#row.cloneNode(true) as HTMLElement;
    row.dataset.account = id;
    const name = row.querySelector<HTMLElement>(".flex-1");
    const toggle = row.querySelector('[role="switch"]')!;
    if (this.querySelector("[data-stepper]") && name) {
      name.id = `${this.id}-${id}`;
      toggle.setAttribute("aria-labelledby", name.id);
    } else toggle.setAttribute("aria-label", "");
    return row;
  }
}

customElements.define("pb-warm", Warm);
