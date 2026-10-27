// The hero's macOS desk: Pitboard's menu bar item and its menu, as MenuBarExtra's menu style runs
// them (plan 3.1).
import { attr, html, keyed, shown, text } from "./draw.ts";
import { handOver } from "./demo.ts";
import { LiveFigure } from "./figure.ts";
import { bringForward, help } from "./forward.ts";
import { Menu, drawSites } from "./menu.ts";
import { showAbout, showAccountWindow } from "./windows.ts";
import {
  model,
  openWindow,
  quit,
  refreshFromApp,
  switchFromApp,
  type State,
} from "../../lib/model.ts";
import {
  READING,
  SWITCHING,
  isInUse,
  listed,
  menuBarClock,
  menuBarShown,
  menuBarSpoken,
  menuLine,
  statusLine,
  updated,
} from "../../lib/words.ts";
import type { Tool } from "../../data/world.ts";

class MenuBarDesk extends LiveFigure {
  #item!: HTMLButtonElement;
  #menu!: Menu;
  #down = "mouse";

  connectedCallback() {
    this.#item = this.querySelector<HTMLButtonElement>(`#${this.id}-item`)!;
    const item = this.#item;
    this.#menu = new Menu(this.querySelector<HTMLElement>(`#${this.id}-menu`)!, {
      system: "macos",
      desk: this,
      choose: (chosen) => this.#choose(chosen),
      shown: (open, refocus) => {
        item.setAttribute("aria-expanded", String(open));
        if (refocus) item.focus({ preventScroll: true });
      },
    });
    this.#listen();
    super.connectedCallback();
    handOver((frame) => {
      // the visitor's own pointer takes over from the one the demo draws
      this.querySelector('[data-part="pointer"]')?.setAttribute("data-state", "gone");
      // the frames that draw the menu closed hand over a closed menu
      if (frame === "shut" || frame === "reach") this.#menu.closeAll(false);
    });
  }

  protected restart() {
    if (model.get().running) this.#menu.show("pointer", "keep");
  }

  #listen() {
    const item = this.#item;
    const menu = this.#menu;
    // the menu opens as the button goes down, and a press dragged to an item chooses it as the
    // button comes up; let go anywhere else but on the item, it closes
    item.addEventListener("pointerdown", (e) => {
      this.#down = e.pointerType;
      if (e.pointerType === "touch" || e.button !== 0) return;
      if (menu.open) return menu.closeAll(false);
      menu.show("pointer", "menu");
      document.addEventListener(
        "pointerup",
        (up) => {
          const at = up.target as Element;
          if (!item.contains(at) && !menu.contains(at)) menu.closeAll(false);
        },
        { once: true },
      );
    });
    // the menu bar item never takes focus from a press
    item.addEventListener("mousedown", (e) => e.preventDefault());
    // a tap opens it and leaves it open
    item.addEventListener("click", (e) => {
      if (e.detail === 0 || this.#down !== "touch") return;
      if (menu.open) menu.closeAll(false);
      else menu.show("pointer", "menu");
    });
    item.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown") {
        e.preventDefault();
        menu.show("keyboard", "first");
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        menu.show("keyboard", "last");
      } else if (e.key === "Escape" && menu.open) {
        e.preventDefault();
        menu.closeAll(false);
      }
    });
    // a click anywhere else closes it, as a click in another app does (plan question 7), while
    // its desk is shown
    document.addEventListener(
      "pointerdown",
      (e) => {
        const at = e.target as Element;
        if (menu.open && this.offsetParent && !item.contains(at) && !menu.contains(at))
          menu.closeAll(false);
      },
      true,
    );
  }

  #choose(chosen: HTMLElement) {
    const s = model.get();
    const account = s.accounts.find((a) => a.id === chosen.dataset.account);
    const inSubmenu = chosen.closest('[role="menu"]') !== this.#menu.el;
    if (account && (inSubmenu || chosen.hasAttribute("data-one"))) {
      openWindow(account.tool, account.label);
      return showAccountWindow(this, "macos", account);
    }
    if (account) {
      // the account in use stays in use; any other is switched to
      if (!isInUse(account, s)) switchFromApp(account.tool, account.label, "macos");
      return;
    }
    switch (chosen.dataset.does) {
      case "add":
        return bringForward("accounts", "macos", "add");
      case "refresh":
        return refreshFromApp("macos");
      case "open":
        return bringForward("accounts", "macos");
      case "settings":
        return bringForward("settings", "macos");
      case "about":
        return showAbout(this);
      case "help":
        return help();
      case "quit":
        return quit();
    }
  }

  protected render(s: State) {
    const item = this.#item;
    shown(this.querySelector('[data-part="extra"]'), s.running);
    if (!s.running) this.#menu.closeAll(false);
    text(this.querySelector("[data-clock]"), menuBarClock(s.now));
    text(item.querySelector('[data-part="title"]'), menuBarShown(s, s.general.menuBarShows));
    attr(item, "aria-label", menuBarSpoken(s));
    html(this.querySelector('[data-part="status"]'), statusLine(s));

    const after = this.#menu.el.querySelector('[data-moment="after"]')!;
    for (const group of after.querySelectorAll<HTMLElement>("[data-tool]"))
      this.#accounts(s, group, group.dataset.tool as Tool);
    const refresh = this.#menu.el.querySelector('[data-does="refresh"]');
    attr(refresh, "aria-disabled", s.reading !== null ? "true" : null);
    text(
      refresh?.querySelector('[data-part="line"]'),
      s.reading !== null ? READING : updated(s.readAt),
    );
    drawSites(this.#menu, s);
  }

  // a tool's accounts, the one in use first; while a switch runs, every one not in use is dimmed
  #accounts(s: State, group: HTMLElement, tool: Tool) {
    const want = listed(s, tool);
    const template = this.#menu.el.querySelector<HTMLTemplateElement>(
      ':scope > template[data-part="account"]',
    );
    // the heading comes first
    const rows = keyed(
      group,
      want.map((a) => a.id),
      1,
      template,
    );
    want.forEach((a, i) => {
      const on = isInUse(a, s);
      attr(rows[i], "aria-checked", String(on));
      attr(rows[i], "aria-disabled", s.switching && !on ? "true" : null);
      text(rows[i].querySelector('[data-part="title"]'), a.label);
      text(
        rows[i].querySelector('[data-part="line"]'),
        s.switching?.id === a.id ? SWITCHING : menuLine(a, s),
      );
    });
  }
}

customElements.define("pb-menubar", MenuBarDesk);
