// A menu as NSMenu and WinUI's MenuFlyout run one, in WAI-ARIA's menu pattern, with the system's
// keys (plan 3.1, 3.2 and section 6; docs/facts/site.md). On macOS the highlighted item is the one
// with focus, the pointer moves it, arrows stop at the ends, Home and End go to them, Return and
// Space choose as the key goes down, typing goes to the first item whose title or subtitle starts
// with it, a chosen item blinks before the menu fades, and a submenu opens as the pointer reaches
// its item. In WinUI the pointer paints what it is over and leaves focus alone, arrows wrap, Enter
// and Space choose as the key comes up, Tab, Home, End and letters do nothing, a choice runs once
// the menu has closed, and a submenu opens once the pointer rests on its item. On both, dimmed
// items are skipped.

import { keyed, shown, text } from "./draw.ts";
import type { State } from "../../lib/model.ts";
import { listed, openSiteAs } from "../../lib/words.ts";
import { sites } from "../../data/windows.ts";

export type System = "macos" | "windows";
export type How = "pointer" | "keyboard";
export type Focus = "first" | "last" | "menu" | "keep";

const ITEMS = '[role="menuitem"], [role="menuitemradio"], [role="menuitemcheckbox"]';
// WinUI's MenuShowDelay. NSMenu opened a submenu 16 ms after the pointer reached its item, and
// waits this long only to move from one open submenu to another, which is not measured.
export const SUBMENU_MS = 400;
// keys 0.5 s apart made one name in NSMenu, and 1 s apart started a new one
const TYPED_MS = 1000;
// NSMenu's blink and fade, until measured on macOS 27
const BLINK_MS = 70;
const FADE_MS = 200;
// how far inside its desk a menu stays, as a system keeps one off its screen's edge
export const INSET = 5;

export interface MenuOptions {
  system: System;
  // the desk the menu stays inside
  desk: HTMLElement;
  // what an item does, once the menu has closed
  choose: (item: HTMLElement) => void;
  // the menu has opened or closed, and with `refocus` focus goes back to what opened it
  shown: (open: boolean, refocus: boolean) => void;
}

export const usable = (item: HTMLElement) => item.getAttribute("aria-disabled") !== "true";

export class Menu {
  readonly subs = new Map<HTMLElement, Menu>();
  #rest = 0;
  #over: HTMLElement | null = null;
  #typed = "";
  #typedAt = 0;
  #pressed: HTMLElement | null = null;
  #choosing = false;

  constructor(
    readonly el: HTMLElement,
    readonly o: MenuOptions,
    readonly parent: Menu | null = null,
    readonly opener: HTMLElement | null = null,
  ) {
    for (const item of this.#own('[aria-haspopup="menu"]')) {
      const sub = document.getElementById(item.getAttribute("aria-controls") ?? "");
      if (sub) this.subs.set(item, new Menu(sub, o, this, item));
    }
    this.#listen();
  }

  get open() {
    return !this.el.hidden;
  }

  get top(): Menu {
    return this.parent ? this.parent.top : this;
  }

  // a node in this menu or a submenu it has open, wherever the submenu is drawn
  contains(node: Node | null): boolean {
    return (
      !!node &&
      (this.el.contains(node) || [...this.subs.values()].some((s) => s.open && s.contains(node)))
    );
  }

  #own(selector: string): HTMLElement[] {
    return [...this.el.querySelectorAll<HTMLElement>(selector)].filter(
      (el) => el.closest('[role="menu"]') === this.el,
    );
  }

  // the items shown, in order
  items(): HTMLElement[] {
    return this.#own(ITEMS).filter((i) => i.getClientRects().length > 0);
  }

  #itemOf(target: EventTarget | null): HTMLElement | null {
    const item = target instanceof Element ? target.closest<HTMLElement>(ITEMS) : null;
    return item && item.closest('[role="menu"]') === this.el ? item : null;
  }

  // an event in this menu itself, not in a submenu drawn inside it
  #mine(e: Event) {
    return e.target instanceof Element && e.target.closest('[role="menu"]') === this.el;
  }

  show(how: How, focus: Focus) {
    this.el.hidden = false;
    delete this.el.dataset.state;
    this.#place();
    if (this.opener) this.opener.setAttribute("aria-expanded", "true");
    else this.o.shown(true, false);
    const items = this.items().filter(usable);
    const to = focus === "first" ? items[0] : focus === "last" ? items.at(-1) : null;
    if (to) to.focus({ preventScroll: true });
    else if (focus === "menu" || (focus !== "keep" && how === "keyboard"))
      this.el.focus({ preventScroll: true });
  }

  // A menu that would leave its desk moves in, and a submenu opens on the side its item has room
  // on, its first item level with the item: each system places its menus on its screen so, and
  // the desk is the screen.
  #place() {
    const desk = this.o.desk.getBoundingClientRect();
    if (!this.opener) {
      this.el.style.setProperty("--dx", "0px");
      const r = this.el.getBoundingClientRect();
      if (r.right > desk.right - INSET)
        this.el.style.setProperty(
          "--dx",
          `${Math.max(desk.right - INSET - r.right, desk.left + INSET - r.left)}px`,
        );
      return;
    }
    const box = (this.el.offsetParent ?? this.o.desk).getBoundingClientRect();
    const from = this.opener.getBoundingClientRect();
    const { width, height } = this.el.getBoundingClientRect();
    const first = this.items()[0];
    const lift = first
      ? first.getBoundingClientRect().top - this.el.getBoundingClientRect().top
      : 0;
    const after = from.right + width <= desk.right - INSET;
    const x = after || from.left - width < desk.left + INSET ? from.right : from.left - width;
    const y = Math.min(from.top - lift, desk.bottom - INSET - height);
    this.el.style.setProperty("--x", `${x - box.left}px`);
    this.el.style.setProperty("--y", `${y - box.top}px`);
  }

  // this menu and every submenu it has open
  close() {
    clearTimeout(this.#rest);
    this.#over = null;
    for (const sub of this.subs.values()) if (sub.open) sub.close();
    this.el.hidden = true;
    if (this.opener) this.opener.setAttribute("aria-expanded", "false");
  }

  // the whole menu, giving focus back to what opened it where focus was in it
  closeAll(refocus: boolean) {
    const top = this.top;
    if (!top.open) return;
    const had = top.contains(document.activeElement);
    top.close();
    delete top.el.dataset.state;
    top.o.shown(false, refocus && had);
  }

  #openSub(sub: Menu, how: How) {
    clearTimeout(this.#rest);
    for (const other of this.subs.values()) if (other !== sub && other.open) other.close();
    if (!sub.open) sub.show(how, how === "keyboard" ? "first" : "keep");
    else if (how === "keyboard") sub.items().filter(usable)[0]?.focus({ preventScroll: true });
  }

  #listen() {
    const el = this.el;
    el.addEventListener("keydown", (e) => {
      if (this.#mine(e)) this.#key(e);
    });
    el.addEventListener("keyup", (e) => {
      if (this.#mine(e)) this.#keyUp(e);
    });
    el.addEventListener("pointerover", (e) => {
      if (e.pointerType !== "touch" && this.#mine(e)) this.#hover(e.target);
    });
    el.addEventListener("pointerleave", (e) => {
      if (e.pointerType !== "touch" && !this.top.contains(e.relatedTarget as Node | null))
        this.top.#left();
    });
    // a mouse chooses as its button comes up, after a press and drag too; WinUI chooses with
    // either button; a touch chooses with its tap
    el.addEventListener("pointerup", (e) => {
      const button = e.button === 0 || (this.o.system === "windows" && e.button === 2);
      if (e.pointerType !== "touch" && button && this.#mine(e)) this.#release(e.target);
    });
    let touched = false;
    el.addEventListener("pointerdown", (e) => (touched = e.pointerType === "touch"));
    el.addEventListener("click", (e) => {
      if (touched && this.#mine(e)) this.#release(e.target);
    });
    // no browser menu opens over a menu
    el.addEventListener("contextmenu", (e) => e.preventDefault());
  }

  #hover(target: EventTarget | null) {
    const item = this.#itemOf(target);
    // the pointer reached this submenu, so its parent leaves it open
    if (this.parent) clearTimeout(this.parent.#rest);
    if (item === this.#over) return;
    this.#over = item;
    if (this.o.system === "macos") {
      if (item && usable(item)) item.focus({ preventScroll: true });
      else if (this.el.contains(document.activeElement)) this.el.focus({ preventScroll: true });
    }
    clearTimeout(this.#rest);
    const sub = item && usable(item) ? this.subs.get(item) : undefined;
    const open = [...this.subs.values()].find((s) => s.open);
    if (sub && !sub.open && this.o.system === "macos" && !open) this.#openSub(sub, "pointer");
    else if (sub && !sub.open)
      this.#rest = window.setTimeout(() => this.#openSub(sub, "pointer"), SUBMENU_MS);
    else if (open && item !== open.opener)
      this.#rest = window.setTimeout(() => open.close(), SUBMENU_MS);
  }

  // the pointer left the menu: macOS takes the highlight away, unless it is on an item whose
  // submenu is open
  #left() {
    if (this.o.system !== "macos") return;
    const find = (m: Menu): Menu | null => {
      for (const sub of m.subs.values()) {
        const found = sub.open ? find(sub) : null;
        if (found) return found;
      }
      return m.el.contains(document.activeElement) ? m : null;
    };
    const at = find(this);
    if (!at || [...at.subs.values()].some((s) => s.open)) return;
    at.#over = null;
    at.el.focus({ preventScroll: true });
  }

  #release(target: EventTarget | null) {
    const item = this.#itemOf(target);
    if (!item || !usable(item)) return;
    const sub = this.subs.get(item);
    if (sub) this.#openSub(sub, "pointer");
    else this.#chosen(item);
  }

  #chosen(item: HTMLElement) {
    const top = this.top;
    if (top.#choosing) return;
    top.#choosing = true;
    const done = () => {
      top.#choosing = false;
      this.closeAll(true);
      this.o.choose(item);
    };
    if (this.o.system === "windows") return done();
    const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
    item.focus({ preventScroll: true });
    item.dataset.state = "blink";
    window.setTimeout(() => {
      delete item.dataset.state;
      window.setTimeout(() => {
        top.el.dataset.state = "shut";
        for (const sub of top.subs.values()) if (sub.open) sub.el.dataset.state = "shut";
        window.setTimeout(done, still ? 0 : FADE_MS);
      }, BLINK_MS);
    }, BLINK_MS);
  }

  #key(e: KeyboardEvent) {
    const mac = this.o.system === "macos";
    const items = this.items().filter(usable);
    const at = document.activeElement as HTMLElement | null;
    const i = at ? items.indexOf(at) : -1;
    const go = (to: HTMLElement | undefined) => {
      e.preventDefault();
      to?.focus({ preventScroll: true });
    };
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    switch (e.key) {
      case "ArrowDown":
        return go(i < 0 ? items[0] : i < items.length - 1 ? items[i + 1] : mac ? at! : items[0]);
      case "ArrowUp":
        return go(i < 0 ? items.at(-1) : i > 0 ? items[i - 1] : mac ? at! : items.at(-1));
      case "ArrowRight": {
        const sub = at && i >= 0 ? this.subs.get(at) : undefined;
        if (sub) {
          e.preventDefault();
          this.#openSub(sub, "keyboard");
        }
        return;
      }
      case "ArrowLeft":
        if (this.parent) {
          e.preventDefault();
          this.close();
          this.opener!.focus({ preventScroll: true });
        }
        return;
      case "Escape":
        e.preventDefault();
        if (this.parent) {
          this.close();
          this.opener!.focus({ preventScroll: true });
        } else this.closeAll(true);
        return;
      case " ":
        // a space within a name being typed is part of it
        if (mac && performance.now() - this.#typedAt <= TYPED_MS) {
          e.preventDefault();
          return this.#typeSelect(e.key, items);
        }
      // falls through: Space chooses as Return does
      case "Enter": {
        e.preventDefault();
        if (!at || i < 0 || e.repeat) return;
        const sub = this.subs.get(at);
        if (sub) this.#openSub(sub, "keyboard");
        else if (mac) this.#chosen(at);
        else this.#pressed = at;
        return;
      }
      case "Tab":
        // macOS closes the menu and Tab goes on from what opened it; WinUI keeps focus in it
        if (mac) this.closeAll(true);
        else e.preventDefault();
        return;
      case "Home":
      case "PageUp":
        return mac ? go(items[0]) : e.preventDefault();
      case "End":
      case "PageDown":
        return mac ? go(items.at(-1)) : e.preventDefault();
    }
    if (mac && e.key.length === 1) {
      e.preventDefault();
      this.#typeSelect(e.key, items);
    }
  }

  #keyUp(e: KeyboardEvent) {
    if (e.key !== "Enter" && e.key !== " ") return;
    const at = this.#pressed;
    this.#pressed = null;
    if (at && at === document.activeElement) this.#chosen(at);
  }

  // typing goes to the first item whose title or subtitle it begins
  #typeSelect(key: string, items: HTMLElement[]) {
    const now = performance.now();
    this.#typed = now - this.#typedAt > TYPED_MS ? key : this.#typed + key;
    this.#typedAt = now;
    const typed = this.#typed.toLocaleLowerCase();
    const said = (el: Element | null) => el?.textContent?.trim().toLocaleLowerCase() ?? "";
    const named = (item: HTMLElement) => [
      said(item.querySelector('[data-part="title"]') ?? item),
      said(item.querySelector('[data-part="line"]')),
    ];
    items
      .find((item) => named(item).some((n) => n.startsWith(typed)))
      ?.focus({ preventScroll: true });
  }
}

// Open claude.ai and Open chatgpt.com in Pitboard's menus: a submenu of the site's accounts, or
// with one account an item that opens its window (account_windows/accounts.rs).
export function drawSites(menu: Menu, s: State) {
  for (const [site, tool] of sites) {
    const [opener] = [...menu.subs.keys()].filter((item) => item.dataset.does === site);
    const one = menu.el.querySelector<HTMLElement>(`[data-does="${site}"][data-one]`)!;
    const submenu = menu.subs.get(opener)!;
    const want = listed(s, tool);
    if (want.length === 1 && submenu.open) submenu.close();
    shown(opener, want.length > 1);
    shown(one, want.length === 1);
    if (want.length === 1) {
      one.dataset.account = want[0].id;
      text(one.querySelector('[data-part="title"]'), openSiteAs(site, want[0].label));
      continue;
    }
    const template = submenu.el.querySelector<HTMLTemplateElement>('template[data-part="account"]');
    keyed(
      submenu.el,
      want.map((a) => a.id),
      0,
      template,
    ).forEach((row, i) => text(row.querySelector('[data-part="title"]'), want[i].label));
  }
}
