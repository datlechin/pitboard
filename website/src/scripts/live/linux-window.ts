// <pb-linux-window>: the Linux app's window (plan 3.3, 3.5), one drawing for GNOME and KDE Plasma:
// its header bar, a group an account with its header's button and its menu, the notices above
// them, Diagnostics and Activity pushed over the accounts, the main menu, and the dialogs. The
// window closes as the desktop has it: on stock GNOME, with no status item, closing it quits
// Pitboard until Reset; under KDE Plasma's tray Pitboard runs on, and the tray brings it back.
import { AccountsFigure, accountOf, part } from "./accounts.ts";
import { readLog } from "./activity.ts";
import { attr, cloned, keyed, placeAt, shown, text } from "./draw.ts";
import { bringForward, help } from "./forward.ts";
import { linuxSheets, type Shown } from "./linux-sheets.ts";
import { INSET, Menu, sectioned, type How } from "./menu.ts";
import {
  closeAppWindow,
  model,
  quit,
  refreshFromApp,
  surfaces,
  warmNote,
  warmStanding,
  type State,
} from "../../lib/model.ts";
import {
  READING,
  isInUse,
  limitRow,
  limitTitle,
  listed,
  signInExpired,
  switchSpoken,
  toolsOn,
  updated,
} from "../../lib/words.ts";
import { SWITCH, WARM_UP_OFF, signInAgain, signInAsking, toolIfShown } from "../../data/window.ts";
import { accountMenu, headerOffer, levelOf, signInOf } from "../../data/linux-window.ts";
import type { Account } from "../../data/world.ts";

const FRONT = "gnome";
// a popover's arrow, which sits between it and its button (_popovers.scss)
const ARROW = 8;
type Page = "accounts" | "diagnostics" | "activity";

class LinuxWindow extends AccountsFigure {
  protected readonly rowSelector = '[data-part="list"] > [data-account]';
  #main!: Menu;
  #menu!: Menu;
  // the account the account menu is open for, and its group's button
  #menuFor: string | null = null;
  #opener: HTMLElement | null = null;
  #page: Page = "accounts";
  #first: Page = "accounts";

  protected windowOf() {
    return this;
  }

  // the desk the window is drawn on, which its menus and dialogs keep to as to a screen
  get desk(): HTMLElement {
    return this.parentElement?.closest<HTMLElement>(".desk") ?? this;
  }

  // the window's width in units, by which a unit's pixels are read as the page scales it
  #unit() {
    return this.getBoundingClientRect().width / 600;
  }

  protected connect() {
    this.#page = this.#first =
      (this.querySelector<HTMLElement>(":scope > div > [data-page]:not([hidden])")?.dataset
        .page as Page) ?? "accounts";
    const mainButton = () =>
      this.#pageEl("accounts").querySelector<HTMLElement>('[data-does="main-menu"]');
    this.#main = new Menu(this.querySelector<HTMLElement>(`#${this.id}-main-menu`)!, {
      toolkit: "gtk",
      desk: this.desk,
      choose: (item) => this.#chosen(item.dataset.does!),
      shown: (open, refocus) => {
        attr(mainButton(), "aria-expanded", String(open));
        if (!open && refocus) mainButton()?.focus({ preventScroll: true });
      },
    });
    this.#menu = new Menu(this.querySelector<HTMLElement>(`#${this.id}-account-menu`)!, {
      toolkit: "gtk",
      desk: this.desk,
      choose: (item) =>
        this.offer(item.dataset.offer!, accountOf(model.get(), this.#menuFor ?? undefined)),
      shown: (open, refocus) => {
        attr(this.#opener, "aria-expanded", String(open));
        if (!open && refocus) this.#opener?.focus({ preventScroll: true });
      },
    });
    // a popover hides at a press anywhere but itself and its button, which closes it as it opens
    // it (gtkpopover.c autohide)
    document.addEventListener(
      "pointerdown",
      (e) => {
        const at = e.target as Node;
        for (const [menu, button] of [
          [this.#main, mainButton()],
          [this.#menu, this.#opener],
        ] as const)
          if (menu.open && !menu.contains(at) && !button?.contains(at)) menu.closeAll(false);
      },
      { capture: true, signal: this.off.signal },
    );
    this.addEventListener("keydown", (e) => this.#key(e));
  }

  #pageEl(page: Page) {
    return this.querySelector<HTMLElement>(`:scope > div > [data-page="${page}"]`)!;
  }

  // the window's own keys (window/keys.rs), where focus is in it and not in one of its dialogs
  #key(e: KeyboardEvent) {
    const at = e.target as Element;
    if (e.defaultPrevented || at.closest('dialog, [role="dialog"], [role="menu"]')) return;
    const ctrl = e.ctrlKey && !e.altKey && !e.metaKey;
    const does = (act: () => void) => {
      e.preventDefault();
      act();
    };
    if (e.key === "F10" && !ctrl && !e.shiftKey) return does(() => this.#toggleMain("keyboard"));
    if (e.altKey && !e.ctrlKey && e.key === "ArrowLeft") return does(() => this.#back());
    if (e.key === "Escape" && this.#page !== "accounts" && !at.closest("input"))
      return does(() => this.#back());
    if (!ctrl) return;
    switch (e.key.toLowerCase()) {
      case "n":
        return does(() => this.present("add"));
      case "r":
        return does(() => this.#refresh());
      case ",":
        return does(() => this.#preferences("general"));
      case "?":
        return does(() => this.#dialog("shortcuts"));
      case "w":
        return does(() => this.#close());
      case "q":
        return does(() => quit());
    }
  }

  protected does(does: string, el: HTMLElement, row: HTMLElement | null): boolean {
    switch (does) {
      case "main-menu":
        this.#toggleMain(this.#how(el));
        return true;
      case "account-menu":
        if (row) this.#toggleAccount(el, row);
        return true;
      case "back":
        this.#back();
        return true;
      case "refresh":
        this.#refresh();
        return true;
      case "refresh-log":
        readLog(this.#pageEl("activity"), this.system, model.get());
        return true;
      case "warm-preferences":
        this.#preferences("warm");
        return true;
      case "close":
        this.#close();
        return true;
      case "minimize":
        // out of sight until the tray's Open Pitboard brings it back, as Activate does
        closeAppWindow();
        return true;
      case "maximize":
        // the figure keeps its window the size its desk draws
        return true;
    }
    return false;
  }

  // a press of a button the keyboard made, which opens a menu on its first item
  #how(el: HTMLElement): How {
    return el.matches(":focus-visible") ? "keyboard" : "pointer";
  }

  #refresh() {
    if (this.#page === "activity") readLog(this.#pageEl("activity"), this.system, model.get());
    else if (this.#page === "accounts") refreshFromApp(this.system);
  }

  // Close and Ctrl+W: where no status item keeps Pitboard running, closing its window quits it
  // (application.rs, Host::keeps_running)
  #close() {
    if (surfaces(this.system).statusItem) closeAppWindow();
    else quit();
  }

  #dialog(what: Shown) {
    linuxSheets()?.show(this, what);
  }

  // Preferences over this window where its figure draws them, or the figure that does
  #preferences(page: string) {
    const own = this.querySelector<HTMLElement & { present(page: string): void }>(
      "pb-linux-preferences",
    );
    if (own?.present) own.present(page);
    else bringForward("settings", this.system, page);
  }

  #chosen(does: string) {
    switch (does) {
      case "open-link":
      case "shortcuts":
      case "about":
        return this.#dialog(does);
      case "diagnostics":
      case "activity":
        return this.#push(does);
      case "preferences":
        return this.#preferences("general");
      case "help":
        return help();
    }
  }

  #toggleMain(how: How) {
    if (this.#main.open) return this.#main.closeAll(true);
    if (this.#page !== "accounts" || !model.get().running) return;
    this.#menu.closeAll(false);
    this.#main.show(how, how === "keyboard" ? "first" : "menu");
  }

  // an account's menu under its group's button, slid to keep on the desk, or above the button
  // where the desk has no room under it
  #toggleAccount(button: HTMLElement, row: HTMLElement) {
    const open = this.#menu.open && this.#opener === button;
    this.#menu.closeAll(false);
    if (open) return;
    const s = model.get();
    const a = accountOf(s, row.dataset.account);
    if (!a) return;
    this.#main.closeAll(false);
    this.#menuFor = a.id;
    this.#opener = button;
    this.#fillMenu(a, s);
    const el = this.#menu.el;
    const w = this.getBoundingClientRect();
    const b = button.getBoundingClientRect();
    const arrow = ARROW * this.#unit();
    delete el.dataset.side;
    el.style.setProperty("--x", `${b.left + b.width / 2 - w.left}px`);
    el.style.setProperty("--y", `${b.bottom + arrow - w.top}px`);
    const how = this.#how(button);
    this.#menu.show(how, how === "keyboard" ? "first" : "menu");
    const desk = this.desk.getBoundingClientRect();
    const m = el.getBoundingClientRect();
    if (m.bottom > desk.bottom - INSET && b.top - arrow - m.height >= desk.top + INSET) {
      el.dataset.side = "above";
      el.style.setProperty("--y", `${b.top - arrow - m.height - w.top}px`);
    }
  }

  // the model's actions for the account but the one its header's button offers, each dimmed
  // while held back, then Copy Email Address and Remove Account… (account_group.rs `offer`)
  #fillMenu(a: Account, s: State) {
    const sections = accountMenu(a, s, {
      switching: s.switching?.id ?? null,
      signing: !!s.signing,
      warmDue: a.tool === "claude" && warmStanding(s, a, "now").stands === "due",
    });
    const offers = sections.flat();
    for (const item of this.#menu.el.querySelectorAll<HTMLElement>("[data-offer]")) {
      const o = offers.find((x) => x.offer === item.dataset.offer);
      shown(item, !!o);
      if (!o) continue;
      attr(item, "aria-disabled", o.enabled ? null : "true");
      if (item.dataset.offer === "use") text(part(item, "title"), o.title);
    }
    sectioned(this.#menu.el, {
      does: sections[0].length > 0,
      copy: sections[1].length > 0,
      forget: sections[2].length > 0,
    });
  }

  // AdwNavigationView: a page pushed over the accounts, its back button focused, and popped
  #push(page: Page) {
    this.#show(page);
    if (page === "activity") readLog(this.#pageEl("activity"), this.system, model.get());
    this.#pageEl(page).querySelector<HTMLElement>('[data-does="back"]')?.focus({
      preventScroll: true,
    });
  }

  #back() {
    if (this.#page === "accounts") return;
    this.#show("accounts");
    this.#pageEl("accounts")
      .querySelector<HTMLElement>('[data-does="main-menu"]')
      ?.focus({ preventScroll: true });
  }

  #show(page: Page) {
    this.#main.closeAll(false);
    this.#menu.closeAll(false);
    this.#page = page;
    for (const el of this.querySelectorAll<HTMLElement>(":scope > div > [data-page]"))
      el.hidden = el.dataset.page !== page;
    this.render(model.get());
  }

  protected restart() {
    this.#main.closeAll(false);
    this.#menu.closeAll(false);
    this.sheets?.close();
    if (this.#page !== this.#first) this.#show(this.#first);
    for (const scroller of this.querySelectorAll<HTMLElement>('[data-part="scroller"]'))
      scroller.scrollTop = 0;
  }

  // after Remove Account its group has gone, and focus goes to the window's first control
  forgotten() {
    this.#pageEl("accounts").querySelector<HTMLElement>("button")?.focus({ preventScroll: true });
  }

  protected render(s: State) {
    const open = s.running && s.windowShown;
    attr(this, "data-state", open ? null : "closed");
    if (!open) {
      this.#main.closeAll(false);
      this.#menu.closeAll(false);
    }
    this.sheets?.render(s);
    const accounts = this.#pageEl("accounts");
    text(
      accounts.querySelector('[data-part="header-bar"] [data-part="subtitle"]'),
      s.reading !== null ? READING : updated(s.readAt),
    );
    attr(
      accounts.querySelector('[data-does="refresh"]'),
      "disabled",
      s.reading !== null ? "" : null,
    );
    if (this.#page !== "accounts") return;
    this.notices(s);
    shown(part(this, "notices"), !!this.querySelector("[data-notice]:not([hidden])"));
    this.#groups(s);
  }

  // a group an account, each tool's in the model's order, the hero's window one tool's alone
  #groups(s: State) {
    const list = part(this, "list")!;
    const drawn = new Set(
      [...list.querySelectorAll<HTMLElement>(":scope > [data-tool]")].map((g) => g.dataset.tool),
    );
    const want = toolsOn("linux")
      .filter(([tool]) => drawn.has(tool))
      .flatMap(([tool]) => listed(s, tool));
    const groups = keyed(
      list,
      want.map((a) => a.id),
      0,
      this.template("group"),
      (node, id) => {
        const own = (v: string) => v.replace(`${this.id}-added`, `${this.id}-${id}`);
        for (const el of [node, ...node.querySelectorAll("[id]")]) if (el.id) el.id = own(el.id);
        node.dataset.tool = want.find((a) => a.id === id)!.tool;
        node.setAttribute("aria-labelledby", `${this.id}-${id}-title`);
      },
    );
    want.forEach((a, i) => this.#group(s, groups[i], a));
  }

  #group(s: State, group: HTMLElement, a: Account) {
    const on = isInUse(a, s);
    const switching = s.switching?.id === a.id;
    const title = part(group, "group-title");
    shown(title, true);
    text(title, a.label);
    const said = [toolIfShown(s, FRONT, a.tool), a.plan].filter(Boolean).join(", ");
    shown(part(group, "description"), !!said);
    text(part(group, "description"), said);
    shown(part(group, "in-use"), on && !switching);
    shown(part(group, "switching"), switching);
    const header = headerOffer(a, s, s.switching?.id ?? null);
    shown(part(group, "action"), header !== null);
    const button = group.querySelector<HTMLButtonElement>('[data-does="use"]')!;
    const again = signInExpired(a, s);
    text(button, again ? signInAsking(FRONT) : SWITCH);
    attr(
      button,
      "aria-label",
      header === null ? null : again ? signInAgain(FRONT) : switchSpoken(a),
    );
    attr(button, "disabled", s.switching ? "" : null);
    const rows = part(group, "rows")!;
    attr(rows, "aria-label", a.label);
    this.#limits(s, rows, a);
    this.#property(group, "email", a.email || null);
    this.#property(group, "sign-in", signInOf(a, s));
    this.#property(group, "warm", a.tool === "claude" ? (warmNote(s, a) ?? WARM_UP_OFF) : null);
  }

  #property(group: HTMLElement, name: string, said: string | null) {
    const row = part(group, name)!;
    shown(row, said !== null);
    const subtitle = part(row, "subtitle");
    shown(subtitle, said !== null);
    text(subtitle, said ?? "");
  }

  // each limit's row ahead of the details', made again only where there are not as many
  #limits(s: State, rows: HTMLElement, a: Account) {
    let limits = [...rows.querySelectorAll<HTMLElement>(':scope > [data-part="limit"]')];
    if (limits.length !== a.limits.length) {
      for (const row of limits) row.remove();
      limits = a.limits.map((_, i) => {
        const row = cloned(this.template("limit"));
        placeAt(rows, row, i);
        return row;
      });
    }
    a.limits.forEach((limit, i) => {
      const row = limits[i];
      const l = limitRow(limit, s);
      text(part(row, "name"), limitTitle(limit));
      text(part(row, "spoken"), l.spoken);
      text(part(row, "resets"), l.resets);
      const fill = part(row, "fill")!;
      attr(fill, "width", String((Math.min(100, l.percent) * 120) / 100));
      attr(fill, "data-fill", levelOf[l.level]);
      text(part(row, "figure"), l.figure);
    });
  }

  // the window's Add Account dialog, asked for from elsewhere: the tray's Add Account…, on the
  // accounts
  present(sheet: string) {
    if (sheet !== "add") return;
    if (this.#page !== "accounts") this.#show("accounts");
    super.present(sheet);
  }

  // no account here has a window of its own: offering one is the macOS and Windows figures'
  protected offer(what: string, a: Account | undefined) {
    if (what !== "open") super.offer(what, a);
  }
}

customElements.define("pb-linux-window", LinuxWindow);
