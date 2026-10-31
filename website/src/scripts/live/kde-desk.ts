// <pb-kde-desk>: the hero's desk on KDE Plasma (plan 3.3 "Hero"): Pitboard's status item in the
// panel's tray, the menu a right click opens (a QMenu dbusmenuimporter.cpp makes of the app's
// DBusMenu, presence.rs `tray`), Plasma's tooltip naming the accounts in use, and Konsole's
// status line. A left click is Activate, which shows the window (application.rs).
import { handOver } from "./demo.ts";
import { attr, html, keyed, shown, text } from "./draw.ts";
import { LiveFigure } from "./figure.ts";
import { bringForward, help } from "./forward.ts";
import { Menu } from "./menu.ts";
import {
  model,
  quit,
  refreshFromApp,
  showAppWindow,
  switchFromApp,
  type State,
} from "../../lib/model.ts";
import { isInUse, listed, menuBarLevel, menuTitle, statusLine } from "../../lib/words.ts";
import { trayAttention, trayTip, type TrayCommand } from "../../data/linux.ts";
import type { Tool } from "../../data/world.ts";

// Plasma's ToolTipArea waits plasmarc's tooltip Delay, 700 ms by default (tooltiparea.cpp)
const TIP_MS = 700;

class KdeDesk extends LiveFigure {
  #icon!: HTMLButtonElement;
  #menu!: Menu;
  #tip!: HTMLElement;
  #tipTimer = 0;

  connectedCallback() {
    this.#icon = this.querySelector<HTMLButtonElement>(`#${this.id}-icon`)!;
    this.#tip = this.querySelector<HTMLElement>(`#${this.id}-tip`)!;
    this.#menu = new Menu(this.querySelector<HTMLElement>(`#${this.id}-menu`)!, {
      toolkit: "qt",
      desk: this,
      choose: (item) => this.#choose(item),
      shown: (open, refocus) => {
        if (open) this.#showTip(false);
        if (refocus) this.#icon.focus({ preventScroll: true });
      },
    });
    this.#listen();
    super.connectedCallback();
    handOver((frame) => {
      // the visitor's own pointer takes over from the one the demo draws
      this.querySelector('[data-part="pointer"]')?.setAttribute("data-state", "gone");
      if (frame === "shut" || frame === "reach") this.#menu.closeAll(false);
    });
  }

  protected restart() {
    this.#showTip(false);
    if (model.get().running) this.#menu.show("pointer", "keep");
  }

  #listen() {
    const icon = this.#icon;
    const menu = this.#menu;
    // Activate: the window, brought into view as the app shows it
    icon.addEventListener("click", () => {
      this.#showTip(false);
      this.#open();
    });
    // ContextMenu: the menu over the panel from the item's left edge, from the pointer or the
    // keyboard alike
    icon.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      this.#openMenu(e.detail === 0 && e.button !== 2 ? "keyboard" : "pointer");
    });
    icon.addEventListener("keydown", (e) => {
      if ((e.key === "F10" && e.shiftKey) || e.key === "ContextMenu") {
        e.preventDefault();
        this.#openMenu("keyboard");
      }
    });
    // Escape puts the tooltip away wherever focus is (WCAG 1.4.13)
    document.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && this.#tip.dataset.state) this.#showTip(false);
    });
    icon.addEventListener("pointerenter", (e) => {
      if (e.pointerType === "touch") return;
      clearTimeout(this.#tipTimer);
      this.#tipTimer = window.setTimeout(() => this.#showTip(true), TIP_MS);
    });
    icon.addEventListener("pointerleave", () => this.#showTip(false));
    icon.addEventListener("pointerdown", () => this.#showTip(false));
    // a menu that hides as a press lands anywhere but on it
    document.addEventListener(
      "pointerdown",
      (e) => {
        if (menu.open && this.offsetParent && !menu.contains(e.target as Element))
          menu.closeAll(false);
      },
      true,
    );
  }

  #open() {
    showAppWindow();
    bringForward("accounts", "kde");
  }

  #openMenu(how: "pointer" | "keyboard") {
    this.#showTip(false);
    this.#menu.closeAll(false);
    // a QMenu opened from the keyboard has no action current until an arrow makes one
    this.#menu.show(how, "menu");
  }

  #showTip(on: boolean) {
    clearTimeout(this.#tipTimer);
    if (on && !this.#menu.open) this.#tip.dataset.state = "shown";
    else delete this.#tip.dataset.state;
  }

  #choose(item: HTMLElement) {
    const s = model.get();
    const a = s.accounts.find((x) => x.id === item.dataset.account);
    if (a) {
      if (!isInUse(a, s)) switchFromApp(a.tool, a.label, "kde");
      return;
    }
    switch (item.dataset.does as TrayCommand) {
      case "open":
        return this.#open();
      case "add-account":
        showAppWindow();
        return bringForward("accounts", "kde", "add");
      case "refresh":
        return refreshFromApp("kde");
      case "preferences":
        showAppWindow();
        return bringForward("settings", "kde", "general");
      case "help":
        return help();
      case "quit":
        return quit();
    }
  }

  protected render(s: State) {
    const icon = this.#icon;
    shown(icon, s.running);
    if (!s.running) {
      this.#menu.closeAll(false);
      this.#showTip(false);
    }
    // the icon at its level, or its attention twin, which Plasma pulses
    const level = menuBarLevel(s);
    const needs = trayAttention(s);
    const attention = String(needs);
    attr(icon, "data-attention", attention);
    for (const svg of icon.querySelectorAll<SVGElement>("[data-level]"))
      shown(svg, svg.dataset.level === level && svg.dataset.attention === attention);
    const tip = trayTip(s, needs);
    text(this.#tip.querySelector('[data-part="title"]'), tip.title);
    text(this.#tip.querySelector('[data-part="text"]'), tip.text);
    html(this.querySelector('[data-part="status"]'), statusLine(s));
    const after = this.#menu.el.querySelector('[data-moment="after"]')!;
    for (const group of after.querySelectorAll<HTMLElement>("[data-tool]"))
      this.#accounts(s, group, group.dataset.tool as Tool);
    attr(
      after.querySelector('[data-does="refresh"]'),
      "aria-disabled",
      s.reading !== null ? "true" : null,
    );
  }

  // a tool's accounts after its name, each a radio, the one in use chosen; while a switch runs
  // the one it is for says so, and the others are held back (present/words/accounts.rs
  // `named_in_menu`)
  #accounts(s: State, group: HTMLElement, tool: Tool) {
    const want = listed(s, tool);
    const template = this.#menu.el.querySelector<HTMLTemplateElement>(
      ':scope > template[data-part="account"]',
    );
    // after the tool's name, where two tools have accounts
    const rows = keyed(
      group,
      want.map((a) => a.id),
      group.hasAttribute("aria-labelledby") ? 1 : 0,
      template,
    );
    want.forEach((a, i) => {
      const on = isInUse(a, s);
      const switching = s.switching?.id === a.id;
      attr(rows[i], "aria-checked", String(on));
      attr(rows[i], "aria-disabled", s.switching && !on ? "true" : null);
      text(rows[i].querySelector('[data-part="title"]'), menuTitle(a, s, "gnome", switching));
    });
  }
}

customElements.define("pb-kde-desk", KdeDesk);
