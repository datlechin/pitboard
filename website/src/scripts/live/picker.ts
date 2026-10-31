// <pb-account-picker>: macOS's Open Link window (AccountPicker.swift) while the link a browser
// shared waits for an account. Its list follows the one state; Open, Return or a double click
// opens the link as the account selected, in that account's window over the desk, and Cancel,
// Escape, ⌘. or the close button put the link away. Either takes the window off the desk until
// Reset, and the app's menu bar works as every window's does.
import { attr, keyed, shown, text } from "./draw.ts";
import { LiveFigure } from "./figure.ts";
import { Grid } from "./grid.ts";
import { showAccountWindow } from "./windows.ts";
import { dismissLink, model, openLink, type State } from "../../lib/model.ts";
import { pickerOf } from "../../data/window.ts";
import { sharedLink } from "../../data/world.ts";

const site = new URL(sharedLink).host;
const part = (in_: ParentNode, name: string) =>
  in_.querySelector<HTMLElement>(`[data-part="${name}"]`);

class Picker extends LiveFigure {
  #window!: HTMLElement;
  #list!: HTMLElement;
  #grid!: Grid;
  // the account the visitor selected, which the selection keeps to while the list offers it
  #picked: string | null = null;
  #drawing = false;

  connectedCallback() {
    this.#window = this.querySelector<HTMLElement>('[data-part="window"]')!;
    const list = (this.#list = this.querySelector<HTMLElement>('[role="listbox"]')!);
    this.#grid = new Grid({
      system: "macos",
      grid: list,
      row: '[role="option"]',
      rows: () =>
        [...list.querySelectorAll<HTMLElement>('[role="option"]')].filter((o) => !o.hidden),
      act: () => this.#open(),
      name: (row) => part(row, "label")?.textContent?.trim() ?? "",
      selected: (row) => {
        if (!this.#drawing) this.#picked = row?.dataset.account ?? null;
      },
    });
    this.#window.addEventListener("click", (e) => {
      const does = (e.target as Element).closest<HTMLElement>("[data-does]")?.dataset.does;
      if (does === "open") this.#open();
      if (does === "cancel") this.#cancel();
    });
    // Return presses Open and Escape or ⌘. presses Cancel, wherever focus is in the window
    this.#window.addEventListener("keydown", (e) => {
      if (e.defaultPrevented || e.isComposing) return;
      const cancels = e.key === "Escape" || (e.key === "." && e.metaKey);
      if (!cancels && e.key !== "Enter") return;
      e.preventDefault();
      if (cancels) this.#cancel();
      else this.#open();
    });
    // the list draws its selection in the accent while it, or nothing else in the window, has focus
    this.#window.addEventListener("focusin", (e) =>
      attr(list, "data-key", list.contains(e.target as Node) ? "" : null),
    );
    super.connectedCallback();
  }

  #open() {
    const a = model.get().accounts.find((x) => x.id === this.#grid.selected?.dataset.account);
    if (!a) return;
    openLink(a.id);
    showAccountWindow(this, "macos", a);
  }

  // the link put away, and focus on Reset, which brings it back
  #cancel() {
    dismissLink();
    this.closest("figure")
      ?.querySelector<HTMLElement>("[data-reset]:not([hidden])")
      ?.focus({ preventScroll: true });
  }

  protected restart() {
    this.#picked = null;
    this.render(model.get());
  }

  protected render(s: State) {
    shown(this.#window, s.running && s.linkWaiting);
    const p = pickerOf(s, site);
    // an account a visitor adds takes an option made from the drawing's template
    const options = keyed(
      this.#list,
      p.accounts.map(({ a }) => a.id),
      0,
      this.querySelector<HTMLTemplateElement>('template[data-part="option"]'),
    );
    p.accounts.forEach(({ a, detail }, i) => {
      text(part(options[i], "label"), a.label);
      text(part(options[i], "detail"), detail);
    });
    const picked = p.accounts.some(({ a }) => a.id === this.#picked) ? this.#picked : p.chosen;
    this.#drawing = true;
    this.#grid.select(
      this.#list.querySelector<HTMLElement>(`[role="option"][data-account="${picked}"]`),
      false,
    );
    this.#drawing = false;
    this.#grid.refresh();
  }
}

customElements.define("pb-account-picker", Picker);
