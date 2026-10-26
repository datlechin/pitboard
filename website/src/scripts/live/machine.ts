// This PC (answer 4 a), which stays on its spinner: no `pitboard doctor` run on Windows has been
// read for the site to draw its checks from.
// departs from the apps: This PC stays on its spinner
import { attr, cloned } from "./draw.ts";

export class MachinePane {
  readonly checking = true;

  constructor(readonly panel: HTMLElement) {}

  show() {
    if (!this.panel.firstElementChild)
      this.panel.append(
        cloned(
          document.querySelector<HTMLTemplateElement>(
            '[data-drawing="app windows"] template[data-part="machine"]',
          ),
        ),
      );
    attr(this.panel.querySelector('[data-does="check"]'), "disabled", "");
  }
}
