// Settings' Limits controls on either system, wherever a figure draws them; the words under each
// are machine.rs's for the state.
import { combobox, type Combobox } from "./combobox.ts";
import { disclosure } from "./disclosure.ts";
import { macShare, numberBox, type Share } from "./spin.ts";
import { datePicker, timePicker, type Time } from "./time.ts";
import { atLabel, warmHours } from "../../data/limits.ts";
import {
  model,
  nextWarmUp,
  setChoice,
  setShare,
  setWarmRange,
  turnWarmUp,
  warmNote,
  type AtLimit,
  type State,
} from "../../lib/model.ts";
import { atLimitNote, autoStandingSaid } from "../../lib/words.ts";

const said = (el: Element | null, text: string) => {
  if (el && el.textContent!.trim() !== text) el.textContent = text;
};

// a SettingsExpander, open or closed as drawn, which Reset puts back
function expander(root: HTMLElement, id: string): (() => void) | null {
  const button = document.getElementById(id) as HTMLButtonElement | null;
  const header = root.querySelector<HTMLElement>("[data-header]");
  if (!button || !header) return null;
  const drawn = button.getAttribute("aria-expanded");
  disclosure(button, header);
  return () => {
    if (button.getAttribute("aria-expanded") !== drawn) button.click();
  };
}

export class AtLimitControls {
  #choice: Combobox;
  #share: Share;
  #box: HTMLElement;
  #expander: (() => void) | null = null;

  constructor(
    readonly root: HTMLElement,
    prefix: string,
  ) {
    const part = <T extends Element = HTMLElement>(name: string) =>
      document.getElementById(`${prefix}-${name}`) as unknown as T;
    this.#box = part("at");
    const stepper = part<SVGElement>("stepper");
    const choose = (v: string) => setChoice(v as AtLimit);
    if (stepper) {
      this.#choice = combobox(this.#box, part("choices"), "popup", choose);
      this.#share = macShare(part<HTMLInputElement>("share"), stepper, setShare);
    } else {
      this.#choice = combobox(this.#box, part("choices"), "combobox", choose);
      this.#share = numberBox(
        part<HTMLInputElement>("share"),
        root.querySelector("[data-spin=up]")!,
        root.querySelector("[data-spin=down]")!,
        setShare,
      );
      this.#expander = expander(root, `${prefix}-expand`);
    }
  }

  restart() {
    this.#choice.close();
    this.#expander?.();
  }

  render(s: State) {
    const p = s.preferences;
    this.#choice.set(p.atLimit);
    this.#share.set(p.share, p.atLimit !== "nothing");
    // Windows' ComboBox says the share in its name; macOS's pop-up is named by its row
    if (
      this.#box.hasAttribute("aria-label") &&
      this.#box.getAttribute("aria-label") !== atLabel(p.share)
    )
      this.#box.setAttribute("aria-label", atLabel(p.share));
    // macOS leaves the share's row out under Do Nothing
    for (const el of this.root.querySelectorAll<HTMLElement>("[data-share-row]"))
      el.hidden = p.atLimit === "nothing";
    // under Switch Automatically, what switching by itself last came to
    const standing = p.atLimit === "switch" ? s.autoStanding : null;
    for (const el of this.root.querySelectorAll<HTMLElement>("[data-standing]")) {
      el.hidden = standing === null;
      if (standing) said(el.querySelector("[data-said]") ?? el, autoStandingSaid(standing));
    }
    for (const el of this.root.querySelectorAll<HTMLElement>("[data-note]")) {
      el.hidden = el.dataset.note !== p.atLimit;
      if (!el.hidden) said(el, atLimitNote(p)!);
    }
    for (const el of this.root.querySelectorAll<HTMLElement>("[data-terms]"))
      el.hidden = p.atLimit !== "switch";
    // Windows turns Share of a limit's card off with its NumberBox
    const card = this.root.querySelector<HTMLElement>("[data-share-card]");
    if (card) {
      if (p.atLimit === "nothing") card.dataset.state = "off";
      else delete card.dataset.state;
    }
  }
}

export class WarmControls {
  #from: Time;
  #to: Time;
  #rows: HTMLElement;
  #row: HTMLElement;
  #mac: boolean;
  #expander: (() => void) | null = null;

  constructor(
    readonly root: HTMLElement,
    readonly prefix: string,
  ) {
    const part = <T extends HTMLElement = HTMLElement>(name: string) =>
      document.getElementById(`${prefix}-${name}`) as T;
    const from = (m: number) => setWarmRange(m, model.get().warm.to);
    const to = (m: number) => setWarmRange(model.get().warm.from, m);
    this.#mac = !!root.querySelector("[data-stepper]");
    if (this.#mac) {
      this.#from = datePicker(part("from"), from);
      this.#to = datePicker(part("to"), to);
    } else {
      this.#from = timePicker(
        part<HTMLButtonElement>("from"),
        part<HTMLDialogElement>("from-flyout"),
        from,
      );
      this.#to = timePicker(
        part<HTMLButtonElement>("to"),
        part<HTMLDialogElement>("to-flyout"),
        to,
      );
      this.#expander = expander(root, `${prefix}-warm-expand`);
    }
    this.#rows = root.querySelector("[data-accounts]")!;
    this.#row = root.querySelector<HTMLTemplateElement>("template[data-row]")!.content
      .firstElementChild as HTMLElement;
    this.#rows.addEventListener("click", (e) => {
      const toggle = (e.target as Element).closest('[role="switch"]');
      const id = toggle?.closest<HTMLElement>("[data-account]")?.dataset.account;
      const a = model.get().accounts.find((x) => x.id === id);
      if (a) turnWarmUp(a.label, toggle!.getAttribute("aria-checked") !== "true");
    });
    // WinUI's ToggleSwitch takes Space alone
    if (!this.#mac)
      this.#rows.addEventListener("keydown", (e) => {
        if (e.key === "Enter" && (e.target as Element).closest('[role="switch"]'))
          e.preventDefault();
      });
  }

  restart() {
    this.#from.close?.();
    this.#to.close?.();
    this.#expander?.();
  }

  render(s: State) {
    this.#from.set(s.warm.from);
    this.#to.set(s.warm.to);
    said(this.root.querySelector("[data-range]"), warmHours(s.warm.from, s.warm.to));
    const next = nextWarmUp(s);
    const line =
      this.root.querySelector<HTMLElement>("[data-next-row]") ??
      this.root.querySelector("[data-next]");
    if (line) line.hidden = next === null;
    if (next) said(this.root.querySelector("[data-next]"), next);

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
        const title = a.label;
        said(row.querySelector("[data-title]"), title);
        const note = warmNote(s, a);
        const noteLine = row.querySelector<HTMLElement>("[data-note]")!;
        noteLine.hidden = note === null;
        if (note) said(noteLine, note);
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
    if (this.#mac && name) {
      name.id = `${this.prefix}-${id}`;
      toggle.setAttribute("aria-labelledby", name.id);
    } else toggle.setAttribute("aria-label", "");
    return row;
  }
}
