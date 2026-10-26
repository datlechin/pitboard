// A figure that works: the custom element around its drawing, inert until its module defines it
// (plan 2.4).
import { appMenus, type AppMenus, type WindowMenus } from "./appmenu.ts";
import { shown, text } from "./draw.ts";
import { bringForward, help } from "./forward.ts";
import { showAbout, showAccountWindow } from "./windows.ts";
import { model, openWindow, quit, reset, run, type State } from "../../lib/model.ts";
import { menuBarClock } from "../../lib/words.ts";

const RESET = "figures-reset";

export abstract class LiveFigure extends HTMLElement {
  #stop?: () => void;
  #restart = () => {
    this.menuBar?.close();
    this.restart();
  };
  protected menuBar: AppMenus | null = null;

  connectedCallback() {
    this.inert = false;
    this.menuBar = appMenus(this, {
      menus: () => this.windowMenus(),
      choose: (does, item) => {
        if (!this.fromMenuBar(does, item)) fromApp(does, item, this);
      },
    });
    const figure = this.closest("figure");
    // `data-drawing` is "<surface> <platform>", as Figure.astro and Term.astro mark each drawing
    const [surface, platform] = (
      this.closest<HTMLElement>("[data-drawing]")?.dataset.drawing ?? ""
    ).split(" ");
    for (const words of figure?.querySelectorAll<HTMLElement>("[data-describes]") ?? []) {
      const on = words.dataset.platforms?.split(" ") ?? [];
      if (words.dataset.describes === surface && on.includes(platform)) words.hidden = true;
    }
    const again = figure?.querySelector<HTMLButtonElement>("[data-reset]");
    if (again)
      again.onclick = () => {
        reset();
        document.dispatchEvent(new Event(RESET));
        // the button goes with the change it undid, so focus moves to the first control of the
        // drawing shown, where a figure draws one for each system
        const shown = [...figure!.querySelectorAll<HTMLElement>("[data-output]")].find(
          (root) => root.offsetParent !== null && customElements.get(root.localName),
        );
        shown?.querySelector<HTMLElement>('[tabindex="0"], button:not(:disabled)')?.focus();
      };
    this.#stop = model.subscribe((state) => {
      this.render(state);
      if (this.menuBar) {
        // the app's menus leave the menu bar once it has quit
        shown(this.menuBar.bar, state.running);
        if (!state.running) this.menuBar.close();
        this.menuBar.render(state);
        text(
          this.menuBar.bar.parentElement!.querySelector("[data-clock]"),
          menuBarClock(state.now),
        );
      }
      if (again) again.hidden = !state.changed;
    });
    document.addEventListener(RESET, this.#restart);
    run();
  }

  disconnectedCallback() {
    this.#stop?.();
    document.removeEventListener(RESET, this.#restart);
  }

  protected abstract render(state: State): void;

  // the figure's own controls as they start: menus, selection, dialogs
  protected restart() {}

  // what the window's menu bar acts on: its account selected and its Refresh
  protected windowMenus(): WindowMenus {
    return { account: null, refresh: null, resizes: true };
  }

  // an item of the menu bar the window answers itself, where it does
  protected fromMenuBar(_does: string, _item: HTMLElement): boolean {
    return false;
  }
}

// What the app does for an item of the menu bar whichever of its windows is in front, on `desk`.
function fromApp(does: string, item: HTMLElement, desk: HTMLElement) {
  const s = model.get();
  switch (does) {
    case "settings":
      return bringForward("settings", "macos");
    case "add":
      return bringForward("accounts", "macos", "add");
    case "about":
      return showAbout(desk);
    case "open": {
      const a = s.accounts.find((x) => x.id === item.dataset.account);
      if (!a) return;
      openWindow(a.tool, a.label);
      return showAccountWindow(desk, "macos", a);
    }
    case "help":
      return help();
    case "quit":
      return quit();
  }
}
