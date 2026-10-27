// This Mac and Windows' Diagnostics (answer 4 a): a check takes as long as a read, and finds what
// `pitboard doctor` finds as the state stands.
// departs from the apps: Diagnostics stays on its spinner
import doctor from "../../../../crates/pitboard/src/render/snapshots/pitboard__render__doctor__tests__doctor_on_a_healthy_mac.snap?raw";
import { attr, cloned, shown, text } from "./draw.ts";
import { TAKES_MS, model, type State } from "../../lib/model.ts";
import { checkedAt, checksOf, spokenLevel } from "../../data/machine.ts";
import { doctorRows, rowsOf } from "../../lib/doctor.ts";
import { hm } from "../../lib/words.ts";

const kept = rowsOf(doctor);
const symbol = { ok: "sf-check-fill", warn: "sf-warning-fill", fail: "sf-warning-fill" };

export class MachinePane {
  checking = false;

  constructor(
    readonly panel: HTMLElement,
    readonly system: "macos" | "windows",
    readonly button: HTMLButtonElement | null,
  ) {}

  show(s: State) {
    if (!this.panel.firstElementChild) {
      const template = document.querySelector<HTMLTemplateElement>(
        `[data-drawing="app ${this.system}"] template[data-part="machine"]`,
      );
      this.panel.append(cloned(template));
    }
    if (this.system === "windows") {
      this.checking = true;
      for (const held of this.panel.querySelectorAll(
        '[data-does="check"], [data-does="copy-report"]',
      ))
        attr(held, "disabled", "");
      return;
    }
    this.#checks(s);
    this.check(s.now);
  }

  check(now: number) {
    if (this.checking) return;
    this.checking = true;
    this.#draw(now);
    setTimeout(() => {
      this.checking = false;
      this.#checks(model.get());
      this.#draw(now);
    }, TAKES_MS.read[this.system]);
  }

  #checks(s: State) {
    const list = this.panel.querySelector<HTMLElement>('[data-part="checks"]');
    if (!list) return;
    const { lines, summary } = checksOf(doctorRows(kept, s, "macos"));
    text(this.panel.querySelector('[data-part="summary"]'), summary);
    attr(list, "aria-label", summary);
    // a row after the first, which draws the rule over it
    const sample = (list.children[1] ?? list.children[0]) as HTMLElement;
    while (list.children.length < lines.length) list.append(sample.cloneNode(true));
    while (list.children.length > lines.length) list.lastElementChild!.remove();
    lines.forEach((line, i) => {
      const li = list.children[i] as HTMLElement;
      li.dataset.level = line.level;
      attr(li.querySelector("svg"), "aria-label", spokenLevel[line.level]);
      attr(li.querySelector("use"), "href", `#${symbol[line.level]}`);
      text(li.querySelector('[data-part="name"]'), line.name);
      text(li.querySelector('[data-part="detail"]'), line.detail);
      const advice = li.querySelector('[data-part="advice"]');
      shown(advice, !!line.advice);
      text(advice, line.advice ?? "");
    });
  }

  #draw(now: number) {
    shown(this.panel.querySelector('[data-part="checking"]'), this.checking);
    const checked = this.panel.querySelector('[data-part="checked"]');
    shown(checked, !this.checking);
    text(checked, checkedAt(hm(now)));
    attr(this.button, "disabled", this.checking ? "" : null);
  }
}
