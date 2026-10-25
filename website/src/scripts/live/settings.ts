// <pb-settings>: Settings at When a limit reaches, on macOS or Windows. The choice and the share are
// the model's preferences, saved at once; the words under them are machine.rs's for the state.
import { combobox, type Combobox } from "./combobox.ts";
import { disclosure } from "./disclosure.ts";
import { LiveFigure } from "./figure.ts";
import { macShare, numberBox, type Share } from "./spin.ts";
import { atLabel } from "../../data/limits.ts";
import {
  atLimitNote,
  autoStandingSaid,
  setChoice,
  setShare,
  type AtLimit,
  type State,
} from "../../lib/model.ts";

const said = (el: Element, text: string) => {
  if (el.textContent!.trim() !== text) el.textContent = text;
};

class Settings extends LiveFigure {
  #choice!: Combobox;
  #share!: Share;
  #box!: HTMLElement;

  connectedCallback() {
    const part = <T extends Element = HTMLElement>(name: string) =>
      this.querySelector<T>(`#${this.id}-${name}`)!;
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
        this.querySelector("[data-spin=up]")!,
        this.querySelector("[data-spin=down]")!,
        setShare,
      );
      disclosure(part<HTMLButtonElement>("expand"), this.querySelector("[data-header]")!);
    }
    super.connectedCallback();
  }

  // Settings has no Refresh, and its window neither minimizes nor zooms
  protected windowMenus() {
    return { account: null, refresh: null, resizes: false };
  }

  protected render(s: State) {
    const p = s.preferences;
    this.#choice.set(p.atLimit);
    this.#share.set(p.share, p.atLimit !== "nothing");
    if (this.#box.getAttribute("aria-label") !== atLabel(p.share))
      this.#box.setAttribute("aria-label", atLabel(p.share));
    // under Switch Automatically, what switching by itself last came to
    const standing = p.atLimit === "switch" ? s.autoStanding : null;
    for (const el of this.querySelectorAll<HTMLElement>("[data-standing]")) {
      el.hidden = standing === null;
      if (standing) said(el.querySelector("[data-said]") ?? el, autoStandingSaid(standing));
    }
    for (const el of this.querySelectorAll<HTMLElement>("[data-note]")) {
      el.hidden = el.dataset.note !== p.atLimit;
      if (!el.hidden) said(el, atLimitNote(p)!);
    }
    for (const el of this.querySelectorAll<HTMLElement>("[data-terms]"))
      el.hidden = p.atLimit !== "switch";
    // Windows turns Share of a limit's card off with its NumberBox
    const card = this.querySelector<HTMLElement>("[data-share-card]");
    if (card) {
      if (p.atLimit === "nothing") card.dataset.state = "off";
      else delete card.dataset.state;
    }
  }
}

customElements.define("pb-settings", Settings);
