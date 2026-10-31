// The menu bar of a Pitboard window in front on macOS (answer 5 a), tracked as NSMenu's menu bar
// tracks the pointer.
// departs from the apps: The macOS window's menu bar is a Tab stop
import { attr, shown, text } from "./draw.ts";
import { Menu, drawSites, sectioned, type How } from "./menu.ts";
import { offersOf } from "./offers.ts";
import { model, type State } from "../../lib/model.ts";
import { NO_ACCOUNT } from "../../data/appmenu.ts";
import type { Account } from "../../data/world.ts";

export interface WindowMenus {
  // the account the Account menu and Edit's Copy and Delete act on
  account: Account | null;
  // the window's Refresh with its title, or null where the window has none
  refresh: { title: string; enabled: boolean } | null;
  // a window that can neither minimize nor zoom dims both
  resizes: boolean;
}

export interface AppMenuOptions {
  desk: HTMLElement;
  menus: () => WindowMenus;
  // what a chosen item does: its `data-does`, the Account menu's `offer:<name>`, or `open` for
  // an account's window from File's submenus
  choose: (does: string, item: HTMLElement) => void;
}

export class AppMenus {
  readonly titles: HTMLElement[];
  #menus = new Map<string, Menu>();
  #open: string | null = null;
  #down = "mouse";

  constructor(
    readonly bar: HTMLElement,
    readonly o: AppMenuOptions,
  ) {
    this.titles = [...bar.querySelectorAll<HTMLElement>('[role="menuitem"]')];
    this.titles[0].tabIndex = 0;
    for (const title of this.titles) this.#listen(title);
    // a press anywhere but the bar and the menu open closes it, as a click in another app does
    document.addEventListener(
      "pointerdown",
      (e) => {
        const at = e.target as Node;
        if (this.#open && !this.bar.contains(at) && !this.#current()?.contains(at)) this.close();
      },
      true,
    );
  }

  get open() {
    return this.#open !== null;
  }

  #current() {
    return this.#open ? this.#menus.get(this.#open) : undefined;
  }

  close(refocus = false) {
    this.#current()?.closeAll(refocus);
  }

  #listen(title: HTMLElement) {
    const id = title.dataset.menu!;
    title.addEventListener("pointerdown", (e) => {
      this.#down = e.pointerType;
      if (e.pointerType === "touch" || e.button !== 0) return;
      // a title never takes focus from a press
      e.preventDefault();
      if (this.#open === id) return this.close();
      this.#show(id, "pointer");
      document.addEventListener(
        "pointerup",
        (up) => {
          const at = up.target as Node;
          if (!this.bar.contains(at) && !this.#current()?.contains(at)) this.close();
        },
        { once: true },
      );
    });
    title.addEventListener("pointerenter", (e) => {
      if (e.pointerType !== "touch" && this.#open && this.#open !== id) this.#show(id, "pointer");
    });
    // a tap opens it and leaves it open
    title.addEventListener("click", (e) => {
      if (e.detail === 0 || this.#down !== "touch") return;
      if (this.#open === id) this.close();
      else this.#show(id, "pointer");
    });
    title.addEventListener("keydown", (e) => {
      const i = this.titles.indexOf(title);
      const to = (j: number) => {
        e.preventDefault();
        this.#focus(this.titles[(j + this.titles.length) % this.titles.length]);
      };
      switch (e.key) {
        case "ArrowRight":
          return to(i + 1);
        case "ArrowLeft":
          return to(i - 1);
        case "Home":
          return to(0);
        case "End":
          return to(this.titles.length - 1);
        case "ArrowDown":
        case "Enter":
        case " ":
          e.preventDefault();
          return this.#show(id, "keyboard", "first");
        case "ArrowUp":
          e.preventDefault();
          return this.#show(id, "keyboard", "last");
      }
    });
  }

  // the title Tab comes back to
  #focus(title: HTMLElement) {
    for (const t of this.titles) t.tabIndex = t === title ? 0 : -1;
    title.focus({ preventScroll: true });
  }

  #show(id: string, how: How, focus: "first" | "last" | "menu" = "menu") {
    this.close();
    const menu = this.#menu(id);
    const title = this.titles.find((t) => t.dataset.menu === id)!;
    for (const t of this.titles) t.tabIndex = t === title ? 0 : -1;
    this.#open = id;
    this.render(model.get());
    // the menu drops from its title's left edge
    const desk = this.o.desk.getBoundingClientRect();
    const edge = parseFloat(getComputedStyle(this.o.desk).borderLeftWidth);
    menu.el.style.setProperty("--x", `${title.getBoundingClientRect().left - desk.left - edge}px`);
    menu.show(how, how === "keyboard" ? focus : "menu");
  }

  // Each menu the first time it opens, from the page's templates.
  #menu(id: string): Menu {
    const made = this.#menus.get(id);
    if (made) return made;
    const template = document.querySelector<HTMLTemplateElement>(`template[data-app-menu="${id}"]`);
    if (!template) throw new Error(`no template for the ${id} menu`);
    const title = this.titles.find((t) => t.dataset.menu === id)!;
    const parts = [...template.content.children].map((el) => el.cloneNode(true) as HTMLElement);
    const [el, ...subs] = parts;
    el.id = `${title.id}-menu`;
    el.setAttribute("aria-labelledby", title.id);
    title.setAttribute("aria-controls", el.id);
    for (const sub of subs) {
      const site = sub.dataset.site!;
      sub.id = `${title.id}-${site}`;
      const opener = el.querySelector<HTMLElement>(`[data-does="${site}"]:not([data-one])`)!;
      opener.id = `${sub.id}-item`;
      opener.setAttribute("aria-controls", sub.id);
      sub.setAttribute("aria-labelledby", opener.id);
    }
    this.o.desk.append(...parts);
    const menu = new Menu(el, {
      toolkit: "appkit",
      desk: this.o.desk,
      choose: (item) => this.#chosen(item),
      shown: (open, refocus) => {
        attr(title, "aria-expanded", String(open));
        if (open) return;
        if (this.#open === id) this.#open = null;
        if (refocus) this.#focus(title);
      },
      across: (by) => {
        const i = this.titles.indexOf(title);
        const next = this.titles[(i + by + this.titles.length) % this.titles.length];
        this.#show(next.dataset.menu!, "keyboard", "first");
      },
    });
    this.#menus.set(id, menu);
    return menu;
  }

  #chosen(item: HTMLElement) {
    if (item.dataset.account) return this.o.choose("open", item);
    if (item.dataset.offer) return this.o.choose(`offer:${item.dataset.offer}`, item);
    if (item.dataset.does) this.o.choose(item.dataset.does, item);
  }

  render(s: State) {
    const menu = this.#current();
    if (!menu?.el) return;
    const w = this.o.menus();
    const el = menu.el;
    const dim = (item: Element | null, on: boolean) =>
      attr(item, "aria-disabled", on ? null : "true");
    switch (this.#open) {
      case "file":
        return drawSites(menu, s);
      case "edit":
        dim(el.querySelector('[data-does="copy"]'), !!w.account?.email);
        return dim(el.querySelector('[data-does="delete"]'), !!w.account);
      case "view": {
        const refresh = el.querySelector('[data-does="refresh"]');
        text(refresh?.querySelector('[data-part="title"]'), w.refresh?.title ?? "Refresh");
        return dim(refresh, !!w.refresh?.enabled);
      }
      case "window":
        for (const title of ["Minimize", "Zoom"])
          dim(
            [...el.querySelectorAll('[role="menuitem"]')].find(
              (i) => i.querySelector('[data-part="title"]')?.textContent?.trim() === title,
            ) ?? null,
            w.resizes,
          );
        return;
      case "account":
        return this.#account(el, s, w.account);
    }
  }

  // AccountCommands.swift: every offer of the account selected, dimmed where it does not apply;
  // with none, the model's offers and Remove Account, each dimmed.
  #account(el: HTMLElement, s: State, a: Account | null) {
    const item = (name: string) => el.querySelector<HTMLElement>(`[data-offer="${name}"]`)!;
    const set = (name: string, on: boolean, enabled: boolean, words?: string) => {
      const it = item(name);
      shown(it, on);
      attr(it, "aria-disabled", enabled ? null : "true");
      if (words !== undefined) text(it.querySelector('[data-part="title"]'), words);
      return on;
    };
    if (!a) {
      set("use", true, false, NO_ACCOUNT[0]);
      set("sign-in-again", true, false);
      set("rename", true, false);
      set("warm", false, false);
      set("open", false, false);
      set("forget", true, false);
      return sectioned(el, { offers: true, open: false, forget: true });
    }
    const o = offersOf(a, s);
    set("use", true, o.use.enabled, o.use.title);
    set("sign-in-again", true, o.signInAgain);
    set("rename", true, o.rename);
    set("warm", o.warm !== null, !!o.warm?.enabled);
    set("open", o.open !== null, true, o.open ?? "");
    set("forget", true, o.forget);
    sectioned(el, { offers: true, open: o.open !== null, forget: true });
  }
}

// The menu bar of the window figure `desk`, where it draws one.
export function appMenus(desk: HTMLElement, o: Omit<AppMenuOptions, "desk">): AppMenus | null {
  const bar = desk.querySelector<HTMLElement>('[data-part="app-menu-bar"] [role="menubar"]');
  return bar ? new AppMenus(bar, { desk, ...o }) : null;
}
