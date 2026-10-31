// A Windows desk: Pitboard's notification area icon and its MenuFlyout (plan 3.2), the menu placed
// where Explorer hands the app its point (docs/facts/site.md), in the hero and, open on a site's
// submenu, in #account-windows.
import { attr, cloned, html, keyed, shown, text } from "./draw.ts";
import { handOver } from "./demo.ts";
import { LiveFigure } from "./figure.ts";
import { bringForward, help } from "./forward.ts";
import { INSET, Menu, drawSites } from "./menu.ts";
import { Tooltips } from "./tooltip.ts";
import { showAccountWindow } from "./windows.ts";
import { model, openWindow, quit, switchFromApp, type State } from "../../lib/model.ts";
import {
  SWITCHING,
  hm,
  isInUse,
  listed,
  menuBarLevel,
  menuLine,
  shellTip,
  shortDate,
  statusLine,
  barTip,
  menuTitle,
} from "../../lib/words.ts";
import { needsAttention } from "../../data/window.ts";
import type { Account, Tool } from "../../data/world.ts";

// WinUI's tooltip waits twice SPI_GETMOUSEHOVERTIME, 400 ms by default, for the pointer and for
// keyboard focus (ToolTipService_Partial.cpp, GetInitialShowDelay); the Shell's tooltip control
// waits TTDT_INITIAL, the double-click time, 500 ms by default
const TIP_MS = 800;
const SHELL_TIP_MS = 500;

class TrayDesk extends LiveFigure {
  #icon!: HTMLButtonElement;
  protected menu!: Menu;
  #tips!: Tooltips;
  #shellTimer = 0;

  connectedCallback() {
    this.#icon = this.querySelector<HTMLButtonElement>(`#${this.id}-icon`)!;
    const icon = this.#icon;
    this.menu = new Menu(this.querySelector<HTMLElement>(`#${this.id}-menu`)!, {
      toolkit: "winui",
      desk: this,
      choose: (chosen) => this.#choose(chosen),
      shown: (open, refocus) => {
        icon.dataset.state = open ? "open" : "closed";
        if (refocus) icon.focus({ preventScroll: true });
      },
    });
    this.#tips = new Tooltips({
      desk: this,
      owners: '[data-moment="after"] [data-tool] [data-account]',
      delay: { pointer: TIP_MS, keyboard: TIP_MS },
    });
    this.#listen();
    super.connectedCallback();
    if (!this.hasAttribute("data-demo-desk")) return;
    handOver((frame) => {
      // the visitor's own pointer takes over from the one the demo draws
      this.pointerGone();
      if (frame === "shut" || frame === "reach") this.menu.closeAll(false);
      else icon.dataset.state = "open";
    });
  }

  // the pointer the desk draws goes once the visitor's own takes over
  protected pointerGone() {
    attr(this.querySelector('[data-part="pointer"]'), "data-state", "gone");
  }

  protected restart() {
    this.#tips.hide();
    this.#shellTip(false);
    this.menu.el.style.removeProperty("--right");
    if (model.get().running) this.menu.show("pointer", "keep");
  }

  #listen() {
    const icon = this.#icon;
    const menu = this.menu;
    // NIN_SELECT and NIN_KEYSELECT: the window, on the pane it shows
    icon.addEventListener("click", () => {
      this.#shellTip(false);
      bringForward("accounts", "windows");
    });
    // WM_CONTEXTMENU, from the pointer or the keyboard, which the app cannot tell apart
    icon.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      this.#shellTip(false);
      this.#openAt(e.clientX, "pointer");
    });
    icon.addEventListener("keydown", (e) => {
      if ((e.key === "F10" && e.shiftKey) || e.key === "ContextMenu") {
        e.preventDefault();
        this.#openAt(null, "keyboard");
      }
    });
    icon.addEventListener("pointerenter", (e) => {
      if (e.pointerType === "touch") return;
      clearTimeout(this.#shellTimer);
      this.#shellTimer = window.setTimeout(() => this.#shellTip(true), SHELL_TIP_MS);
    });
    icon.addEventListener("pointerleave", () => this.#shellTip(false));
    icon.addEventListener("pointerdown", () => this.#shellTip(false));
    // light dismiss: a press anywhere but the menu closes it, while its desk is shown
    document.addEventListener(
      "pointerdown",
      (e) => {
        const at = e.target as Element;
        if (menu.open && this.offsetParent && !menu.contains(at) && this.dismissedBy(at))
          menu.closeAll(false);
      },
      true,
    );
  }

  // whether a press off the menu closes it: on the desk, or past it on the page
  protected dismissedBy(_at: Node) {
    return true;
  }

  // The menu's right edge at `x`, or from the keyboard at the icon's middle, kept on the desk.
  #openAt(x: number | null, how: "pointer" | "keyboard") {
    const menu = this.menu;
    menu.closeAll(false);
    const desk = this.getBoundingClientRect();
    const edge = parseFloat(getComputedStyle(this).borderRightWidth);
    const icon = this.#icon.getBoundingClientRect();
    const at = x ?? icon.left + icon.width / 2;
    menu.el.hidden = false;
    const width = menu.el.getBoundingClientRect().width;
    const right = Math.min(Math.max(at, desk.left + INSET + width), desk.right - INSET);
    menu.el.style.setProperty("--right", `${desk.right - edge - right}px`);
    menu.show(how, "first");
  }

  #shellTip(on: boolean) {
    clearTimeout(this.#shellTimer);
    const tip = this.querySelector<HTMLElement>('[data-part="shell-tip"]');
    if (!tip) return;
    if (on && !this.menu.open) tip.dataset.state = "shown";
    else delete tip.dataset.state;
  }

  #choose(chosen: HTMLElement) {
    const s = model.get();
    const account = s.accounts.find((a) => a.id === chosen.dataset.account);
    const inSubmenu = chosen.closest('[role="menu"]') !== this.menu.el;
    if (account && (inSubmenu || chosen.hasAttribute("data-one"))) {
      openWindow(account.tool, account.label);
      return showAccountWindow(this, "windows", account);
    }
    if (account) {
      if (!isInUse(account, s)) switchFromApp(account.tool, account.label, "windows");
      return;
    }
    switch (chosen.dataset.does) {
      case "Add account…":
        return bringForward("accounts", "windows", "add");
      case "Open Pitboard":
        return bringForward("accounts", "windows");
      case "Settings":
        return bringForward("settings", "windows");
      case "Pitboard help":
        return help();
      case "Exit":
        return quit();
    }
  }

  protected render(s: State) {
    const icon = this.#icon;
    shown(icon, s.running);
    if (!s.running) this.menu.closeAll(false);
    text(this.querySelector("[data-time]"), hm(s.now));
    text(this.querySelector("[data-date]"), shortDate(s.now));
    // departs from the apps: The Windows icon draws no warning over it while its menu needs attention
    const tip = shellTip(barTip(s, "windows", needsAttention(s, "windows")));
    attr(icon, "aria-label", tip);
    const level = menuBarLevel(s);
    for (const svg of icon.querySelectorAll("[data-level]"))
      shown(svg, svg.getAttribute("data-level") === level);
    const lines = tip.split("\n");
    this.querySelectorAll('[data-part="shell-tip"] [data-part="line"]').forEach((line, i) => {
      shown(line, i < lines.length);
      text(line, lines[i] ?? "");
    });
    html(this.querySelector('[data-part="status"]'), statusLine(s));

    const after = this.menu.el.querySelector('[data-moment="after"]')!;
    for (const group of after.querySelectorAll<HTMLElement>("[data-tool]"))
      this.#accounts(s, group, group.dataset.tool as Tool);
    drawSites(this.menu, s);
    this.#tips.refresh();
  }

  #tipId(id: string) {
    return `${this.id}-tip-${id}`;
  }

  // the line an account's item and tooltip say, and the item's one line: why it can't be switched
  // to in place of its figure (present/words/accounts.rs `named_in_menu`)
  #said(s: State, a: Account) {
    const switching = s.switching?.id === a.id;
    return {
      title: menuTitle(a, s, "windows", switching),
      tip: switching ? SWITCHING : menuLine(a, s, "windows"),
    };
  }

  // a tool's accounts, the one in use first, each with its tooltip; while a switch runs, every
  // one not in use is disabled
  #accounts(s: State, group: HTMLElement, tool: Tool) {
    const want = listed(s, tool);
    const template = this.menu.el.querySelector<HTMLTemplateElement>(
      ':scope > template[data-part="account"]',
    );
    // the heading comes first
    const rows = keyed(
      group,
      want.map((a) => a.id),
      1,
      template,
      (row, id) => row.setAttribute("aria-describedby", this.#tipId(id)),
    );
    const tips = this.querySelector('[data-part="tips"]')!;
    want.forEach((a, i) => {
      let tip = document.getElementById(this.#tipId(a.id));
      if (!tip) {
        tip = cloned(tips.querySelector<HTMLTemplateElement>('template[data-part="tip"]'));
        tip.id = this.#tipId(a.id);
        tips.append(tip);
      }
      const on = isInUse(a, s);
      const said = this.#said(s, a);
      attr(rows[i], "aria-checked", String(on));
      attr(rows[i], "aria-disabled", s.switching && !on ? "true" : null);
      text(rows[i].querySelector('[data-part="title"]'), said.title);
      text(tip, said.tip);
    });
  }
}

customElements.define("pb-tray", TrayDesk);

// #account-windows' desk: the menu open on a site's submenu, which Reset opens again, where the
// pointer drawn rests until the visitor's own, a press, a key or focus takes over, as in the hero.
// Until then a press elsewhere on the page leaves it open, for the visitor to come to.
class OpenLinkDesk extends TrayDesk {
  #taken = false;

  connectedCallback() {
    super.connectedCallback();
    for (const type of ["pointerenter", "pointerdown", "keydown", "focusin"])
      this.addEventListener(type, () => {
        this.#taken = true;
        this.pointerGone();
      });
  }

  protected dismissedBy(at: Node) {
    return this.#taken || this.contains(at);
  }

  protected restart() {
    super.restart();
    this.#taken = false;
    const opener = this.menu.el.querySelector<HTMLElement>(
      `[data-does="${this.dataset.submenu}"]:not([data-one])`,
    );
    if (this.menu.open && opener && !opener.hidden) this.menu.expand(opener);
  }
}

customElements.define("pb-open-link", OpenLinkDesk);
