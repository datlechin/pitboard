// The Accounts window on both systems (plan 3.3 to 3.5): its list selects and switches accounts,
// each row's shortcut or context menu and the list's keys offer what applies to it, and Forget,
// Rename, Add Account and Sign In Again ask in the app's own alert, sheets and dialogs. A sign-in
// is simulated: the page loads no sign-in page and ends it after 3 s (answer 16 a). Every view of
// the window on the page, the cut ones too, follows the one state; the dialogs are drawn once a
// system and open over the window they are asked from.
import { combobox, type Combobox } from "./combobox.ts";
import { attr, cloned, keyed, shown, text } from "./draw.ts";
import { closeDialog, isOpen, openDialog, type Cancelled } from "./dialog.ts";
import { LiveFigure } from "./figure.ts";
import { Grid } from "./grid.ts";
import { hold } from "./hold.ts";
import { INSET, Menu } from "./menu.ts";
import { say } from "./say.ts";
import {
  cancelSignIn,
  declineSecond,
  dismissNotice,
  forget,
  model,
  nameProblem,
  openWindow,
  refreshFromApp,
  rename,
  signInFromApp,
  switchFromApp,
  typed,
  warmNote,
  warmStanding,
  warmUp,
  type State,
} from "../../lib/model.ts";
import {
  READING,
  accountSpoken,
  isInUse,
  limitRow,
  listed,
  notes,
  spokenLimit,
  updated,
  useSpoken,
  whole,
} from "../../lib/words.ts";
import {
  addAccount,
  couldNotRename,
  finishInBrowser,
  keysOf,
  forgetMessage,
  forgetTitle,
  inAppWaiting,
  labelTaken,
  noticeOf,
  openSite,
  park,
  parkToAdd,
  parkToSignInAgain,
  parking,
  program,
  putBack,
  renameTitle,
  RENAMING,
  setupStepOf,
  signIn,
  signInAgain,
  signInMessage,
  signInToAgain,
  signingInTo,
  siteOf,
  typeAName,
  unusableName,
  useOffer,
  waitingToQuit,
  RENAME,
  CANCEL,
  LATER,
  WAITING_FOR_BROWSER,
  type System,
} from "../../data/window.ts";
import { tools, type Account, type Tool } from "../../data/world.ts";

const toolName = (tool: Tool) => tools.find(([t]) => t === tool)![1];
const part = <T extends Element = HTMLElement>(in_: ParentNode, name: string) =>
  in_.querySelector<T>(`[data-part="${name}"]`);
const accountOf = (s: State, id: string | undefined) => s.accounts.find((a) => a.id === id);
// a parked login past its life signs in again in place of a switch
const expired = (a: Account, s: State) =>
  !isInUse(a, s) && a.refreshExpiresAt !== undefined && a.refreshExpiresAt <= s.now;

export class AccountsWindow extends LiveFigure {
  system: System = "macos";
  window!: HTMLElement;
  #grid!: Grid;
  #menu!: Menu;
  #menuFor: string | null = null;
  // what raised the coming contextmenu: a right or Control press, or the keys that open a menu
  #raised: "pointer" | "keys" | null = null;
  // a Control-click's click, which selects nothing
  #swallow = false;
  #saying: Tool | null = null;

  connectedCallback() {
    const drawing = this.closest<HTMLElement>("[data-drawing]")?.dataset.drawing ?? "";
    this.system = drawing.endsWith("windows") ? "windows" : "macos";
    this.window = this.firstElementChild as HTMLElement;
    const grid = this.querySelector<HTMLElement>('[role="grid"]')!;
    this.#grid = new Grid({
      system: this.system,
      grid,
      rows: () => [...grid.querySelectorAll<HTMLElement>('[role="row"]')].filter((r) => !r.hidden),
      act: (row) => this.#act(row),
      forget: (row) => this.#ask("forget", row),
      rename: (row) => this.#ask("rename", row),
      name: (row) => accountOf(model.get(), row.dataset.account)?.label ?? "",
    });
    this.#menu = new Menu(this.querySelector<HTMLElement>(`#${this.id}-menu`)!, {
      system: this.system,
      desk: this,
      choose: (item) => this.#offer(item.dataset.offer!),
      shown: (open, refocus) => {
        const row = this.#rowFor(this.#menuFor);
        // macOS outlines the row a shortcut menu acts on while it is open
        if (this.system === "macos") attr(row, "data-state", open ? "menu" : null);
        if (!open && refocus) row?.focus({ preventScroll: true });
      },
    });
    this.#listen(grid);
    super.connectedCallback();
  }

  protected restart() {
    this.#menu.closeAll(false);
    sheets(this.system)?.close();
    this.#grid.select(null, false);
  }

  // the window's Add Account sheet, asked for from elsewhere: the menu bar's Add Account…
  present(sheet: string) {
    if (sheet === "add") sheets(this.system)?.signIn(this, null, null);
  }

  #rowFor(id: string | null) {
    return id ? this.querySelector<HTMLElement>(`[role="row"][data-account="${id}"]`) : null;
  }

  #listen(grid: HTMLElement) {
    this.addEventListener("click", (e) => {
      const does = (e.target as Element).closest<HTMLElement>("[data-does]");
      if (!does || !this.window.contains(does)) return;
      const row = does.closest<HTMLElement>('[role="row"]');
      switch (does.dataset.does) {
        case "use":
          if (row) this.#act(row);
          return;
        case "refresh":
          return refreshFromApp(this.system);
        case "add":
          return sheets(this.system)?.signIn(
            this,
            does.closest('[data-part="setup"]') ? this.#setupTool : null,
            null,
          );
        case "not-now":
          if (this.#setupTool) declineSecond(this.#setupTool);
          return;
        case "dismiss":
          return dismissNotice(does.closest<HTMLElement>("[data-notice]")!.dataset.notice as Tool);
      }
    });
    // a right click, a Control-click or a two-finger click opens a row's shortcut menu where it
    // is; on Windows Shift+F10 and the menu key open it above the row, and on macOS no key does
    this.addEventListener(
      "pointerdown",
      (e) => {
        this.#raised = e.button === 2 || (e.button === 0 && e.ctrlKey) ? "pointer" : null;
        this.#swallow = false;
      },
      true,
    );
    this.addEventListener("keydown", () => (this.#raised = null), true);
    // the press that opens one leaves focus where it was, for the menu to take
    grid.addEventListener("mousedown", (e) => {
      if (e.button === 2 || (e.button === 0 && e.ctrlKey)) e.preventDefault();
    });
    // light dismiss: a press anywhere but the menu closes it, while the window is shown
    document.addEventListener(
      "pointerdown",
      (e) => {
        if (this.#menu.open && this.offsetParent && !this.#menu.contains(e.target as Node))
          this.#menu.closeAll(false);
      },
      true,
    );
    grid.addEventListener("keydown", (e) => {
      if (e.key !== "ContextMenu" && !(e.key === "F10" && e.shiftKey)) return;
      e.preventDefault();
      this.#raised = "keys";
      const row = (e.target as Element).closest<HTMLElement>('[role="row"]');
      if (row && this.system === "windows") this.#openMenu(row, null, null);
    });
    grid.addEventListener(
      "click",
      (e) => {
        if (e.detail > 0 && this.#swallow) e.stopPropagation();
      },
      true,
    );
    grid.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      const by = this.#raised;
      this.#raised = null;
      this.#swallow = by === "pointer";
      const row = (e.target as Element).closest<HTMLElement>('[role="row"]');
      // the keys above answered it already, where a browser raises one after them
      if (!row || by === "keys") return;
      // a touch held still opened it already (hold.ts)
      if (this.#menu.open && this.#menuFor === row.dataset.account) return;
      // with no press before it, a screen reader asked, as VoiceOver's VO-Shift-M does
      if (by === "pointer") this.#openMenu(row, e.clientX, e.clientY);
      else this.#openMenu(row, null, null);
    });
    hold(grid, '[role="row"]', (row, x, y) => this.#openMenu(row, x, y));
    // ⌘C and Ctrl+C copy the account's email, with nothing on screen saying so
    grid.addEventListener("copy", (e) => {
      const row =
        this.system === "macos"
          ? this.#grid.selected
          : (document.activeElement as Element | null)?.closest<HTMLElement>('[role="row"]');
      const email = accountOf(model.get(), row?.dataset.account)?.email;
      if (!email) return;
      e.preventDefault();
      e.clipboardData?.setData("text/plain", email);
    });
  }

  #setupTool: Tool | null = null;

  // Use, Return, Enter or a double click: the row's button
  #act(row: HTMLElement) {
    const s = model.get();
    const a = accountOf(s, row.dataset.account);
    if (!a || isInUse(a, s) || s.switching) return;
    if (expired(a, s)) return sheets(this.system)?.signIn(this, a.tool, a);
    this.#saying = a.tool;
    switchFromApp(a.tool, a.label, this.system);
  }

  #ask(what: "forget" | "rename", row: HTMLElement) {
    const s = model.get();
    const a = accountOf(s, row.dataset.account);
    if (!a) return;
    if (what === "forget" && !isInUse(a, s)) sheets(this.system)?.forget(this, a);
    if (what === "rename") sheets(this.system)?.rename(this, a);
  }

  #openMenu(row: HTMLElement, x: number | null, y: number | null) {
    const s = model.get();
    const a = accountOf(s, row.dataset.account);
    if (!a) return;
    this.#menu.closeAll(false);
    this.#menuFor = a.id;
    this.#fillMenu(a, s);
    if (this.system === "macos") attr(row, "data-state", "menu");
    const el = this.#menu.el;
    this.#menu.show(
      x === null ? "keyboard" : "pointer",
      this.system === "windows" ? "first" : "menu",
    );
    // one opened from the keyboard sits above the row; each is kept on the desk, opening the
    // other way where it would leave it, as a menu keeps on its screen
    const desk = this.getBoundingClientRect();
    const r = row.getBoundingClientRect();
    const m = el.getBoundingClientRect();
    let left = x ?? r.left + r.width / 2 - m.width / 2;
    let top = y ?? r.top - m.height;
    if (left + m.width > desk.right - INSET) left -= m.width;
    if (top + m.height > desk.bottom - INSET) top -= m.height;
    left = Math.max(desk.left + INSET, left);
    top = Math.max(desk.top + INSET, top);
    const box = (el.offsetParent ?? this).getBoundingClientRect();
    el.style.setProperty("--x", `${left - box.left}px`);
    el.style.setProperty("--y", `${top - box.top}px`);
  }

  // only what applies to the account, as the shortcut menu leaves out the rest (present/accounts.rs
  // `offers`; AccountCommands.swift; AccountMenu.cs)
  #fillMenu(a: Account, s: State) {
    const on = isInUse(a, s);
    const sections: Record<string, boolean> = {};
    const offer = (name: string, applies: boolean, words?: string) => {
      const item = this.#menu.el.querySelector<HTMLElement>(`[data-offer="${name}"]`)!;
      shown(item, applies);
      if (words !== undefined) text(part(item, "title") ?? item, words);
      return applies;
    };
    const switches = !on && !expired(a, s) && !s.switching;
    sections.offers = [
      offer("use", switches, useOffer(a.label)),
      offer("sign-in-again", !s.signing),
      offer("rename", true),
      offer("warm", a.tool === "claude" && warmStanding(s, a, "now").stands === "due"),
    ].some(Boolean);
    const site = siteOf(a.tool);
    sections.open = offer("open", site !== null, site ? openSite(site) : "");
    sections.copy = offer("copy", a.email !== "");
    sections.forget = offer("forget", !on);
    // a separator only between two sections that are there
    const order = ["offers", "open", "copy", "forget"];
    for (const sep of this.#menu.el.querySelectorAll<HTMLElement>("[data-after]")) {
      const i = order.indexOf(sep.dataset.after!);
      shown(
        sep,
        order.slice(0, i + 1).some((k) => sections[k]) &&
          order.slice(i + 1).some((k) => sections[k]),
      );
    }
  }

  #offer(what: string) {
    const s = model.get();
    const a = accountOf(s, this.#menuFor ?? undefined);
    if (!a) return;
    switch (what) {
      case "use":
        this.#saying = a.tool;
        return switchFromApp(a.tool, a.label, this.system);
      case "sign-in-again":
        return sheets(this.system)?.signIn(this, a.tool, a);
      case "rename":
        return sheets(this.system)?.rename(this, a);
      case "warm":
        return void warmUp(a.label);
      case "open":
        return openWindow(a.tool, a.label);
      case "copy":
        return void navigator.clipboard?.writeText(a.email).catch(() => {});
      case "forget":
        return sheets(this.system)?.forget(this, a);
    }
  }

  // after Forget the row has gone and nothing is selected; macOS's list keeps focus, and Windows
  // gives it to the window's title bar
  forgotten() {
    this.#grid.lost();
    if (this.system !== "windows") return;
    const bar = this.querySelector<HTMLElement>('[data-part="title-bar"]');
    if (!bar) return;
    bar.tabIndex = -1;
    bar.focus({ preventScroll: true });
  }

  protected render(s: State) {
    shown(this.window, s.running);
    if (!s.running) this.#menu.closeAll(false);
    text(part(this, "updated"), s.reading !== null ? READING : updated(s.readAt));
    attr(this.querySelector('[data-does="refresh"]'), "disabled", s.reading !== null ? "" : null);
    this.#notices(s);
    this.#setup(s);
    const above = this.querySelectorAll(
      '[data-notice]:not([hidden]), [data-part="setup"]:not([hidden])',
    );
    attr(part(this, "first"), "data-state", above.length ? null : "top");
    for (const group of this.querySelectorAll<HTMLElement>('[role="rowgroup"][data-tool]'))
      this.#rows(s, group, group.dataset.tool as Tool);
    this.#grid.refresh();
    sheets(this.system)?.render(s);
  }

  #notices(s: State) {
    for (const box of this.querySelectorAll<HTMLElement>("[data-notice]")) {
      const tool = box.dataset.notice as Tool;
      const n = noticeOf(s, tool, this.system);
      shown(box, !!n);
      if (!n) continue;
      text(part(box, "title"), n.title);
      box.querySelectorAll('[data-part="line"]').forEach((line, i) => {
        shown(line, i < n.lines.length);
        text(line, n.lines[i] ?? "");
      });
      shown(part(box, "countdown"), !!n.countdown);
      text(part(box, "countdown"), n.countdown ?? "");
      for (const icon of box.querySelectorAll<HTMLElement>("[data-severity]"))
        shown(icon, icon.dataset.severity === n.severity);
      // a switch asked for here says the notice it leaves
      if (this.#saying === tool && !s.switching) {
        this.#saying = null;
        say(n.title);
      }
    }
  }

  #setup(s: State) {
    const box = part(this, "setup");
    if (!box) return;
    const step = setupStepOf(s, s.declinedSecond, this.system);
    this.#setupTool = step?.tool ?? null;
    shown(box, !!step);
    if (!step) return;
    text(part(box, "title"), step.title);
    text(part(box, "detail"), step.detail);
  }

  // a tool's accounts, the one in use first
  #rows(s: State, group: HTMLElement, tool: Tool) {
    const want = listed(s, tool);
    // the heading, and on Windows the rule under it, come first
    const offset = this.system === "windows" ? 2 : 1;
    const rows = keyed(
      group,
      want.map((a) => a.id),
      offset,
      this.#template("row"),
    );
    want.forEach((a, i) => this.#row(s, rows[i], a));
  }

  // the templates the window's top view draws for every view of its system
  #template(name: string) {
    return document.querySelector<HTMLTemplateElement>(
      `[data-drawing="app ${this.system}"] pb-accounts template[data-part="${name}"]`,
    );
  }

  #row(s: State, row: HTMLElement, a: Account) {
    const on = isInUse(a, s);
    const switching = s.switching?.id === a.id;
    attr(row, "aria-label", accountSpoken(a, s));
    attr(row, "aria-keyshortcuts", keysOf(a, s, this.system));
    shown(part(row, "in-use-symbol"), on);
    shown(part(row, "parked-symbol"), !on);
    text(part(row, "label"), a.label);
    shown(part(row, "plan"), !!a.plan);
    text(part(row, "plan"), a.plan ?? "");
    shown(part(row, "email"), !!a.email);
    text(part(row, "email"), a.email);
    shown(part(row, "in-use"), on && !switching);
    shown(part(row, "switching"), switching);
    const button = row.querySelector<HTMLButtonElement>('[data-does="use"]')!;
    const again = expired(a, s);
    shown(button, !on && !switching);
    text(button, again ? signInAgain(this.system) : "Use");
    attr(button, "aria-label", on || again ? null : useSpoken(a));
    attr(button, "disabled", s.switching && !switching ? "" : null);
    this.#limits(s, row, a);
    const said = notes(a, s, warmNote(s, a));
    const box = part(row, "notes")!;
    if ([...box.children].map((n) => n.textContent?.trim()).join("\n") !== said.join("\n")) {
      box.replaceChildren(
        ...said.map((n) => {
          const p = cloned(this.#template("note"));
          p.textContent = n;
          return p;
        }),
      );
    }
  }

  #limits(s: State, row: HTMLElement, a: Account) {
    const box = part(row, "limits")!;
    shown(box, a.limits.length > 0);
    let groups = [...box.querySelectorAll<HTMLElement>('[data-part="limit"]')];
    if (groups.length !== a.limits.length) {
      box.replaceChildren(...a.limits.map(() => cloned(this.#template("limit"))));
      groups = [...box.children] as HTMLElement[];
    }
    a.limits.forEach((limit, i) => {
      const g = groups[i];
      const l = limitRow(limit, s);
      const standing = l.pace?.standing ?? "even";
      const even = whole(l.pace?.expected ?? 0);
      text(part(g, "short"), l.short);
      text(part(g, "spoken"), spokenLimit(limit, s));
      const fill = part(g, "fill")!;
      attr(fill, "width", String(l.percent));
      attr(fill, "data-state", l.level);
      const mark = part(g, "mark")!;
      shown(mark, standing !== "even");
      if (mark.localName === "line") {
        attr(mark, "x1", String(even));
        attr(mark, "x2", String(even));
        attr(mark, "data-state", standing);
      } else {
        const [ground, line] = mark.querySelectorAll("rect");
        attr(ground, "x", String(even - 2));
        attr(line, "x", String(even - 1));
        attr(line, "data-state", standing);
      }
      text(part(g, "figure"), l.figure);
      text(part(g, "resets"), l.resets);
      text(part(g, "pace"), l.pace?.said ?? "");
      attr(part(g, "pace"), "data-state", standing);
    });
  }
}

// The Forget alert, the Rename sheet and the sign-in sheet of one system, drawn once with the
// window's top view and opened over whichever window asks.
class Sheets {
  #renaming: Account | null = null;
  #signing: { tool: Tool; again: Account | null; started: boolean } | null = null;
  #tools: Combobox | null = null;
  #said = "";

  constructor(
    readonly system: System,
    readonly forgetDialog: HTMLDialogElement,
    readonly renameDialog: HTMLDialogElement,
    readonly signInDialog: HTMLDialogElement,
  ) {
    const field = renameDialog.querySelector<HTMLInputElement>('input[name="name"]')!;
    field.addEventListener("input", () => this.#checkRename());
    const name = signInDialog.querySelector<HTMLInputElement>('input[name="name"]')!;
    name.addEventListener("input", () => this.#checkSignIn());
    for (const radio of signInDialog.querySelectorAll<HTMLInputElement>('input[name="tool"]'))
      radio.addEventListener("change", () => this.#pick(radio.value as Tool));
    const box = part(signInDialog, "tool-box");
    const list = signInDialog.querySelector<HTMLElement>('[role="listbox"]');
    if (box && list) this.#tools = combobox(box, list, "popup", (v) => this.#pick(v as Tool));
  }

  #over(
    from: AccountsWindow,
    dialog: HTMLDialogElement,
    o: {
      answer: (value: string) => void;
      cancel: (by: Cancelled) => boolean;
      focus?: HTMLElement | null;
    },
  ) {
    this.#dismiss();
    openDialog(dialog, { system: this.system, window: from.window, desk: from, ...o });
  }

  #dismiss() {
    for (const d of [this.forgetDialog, this.renameDialog, this.signInDialog])
      if (isOpen(d)) closeDialog(d);
  }

  close() {
    this.#dismiss();
    this.#renaming = null;
    this.#signing = null;
  }

  forget(from: AccountsWindow, a: Account) {
    const s = model.get();
    const d = this.forgetDialog;
    text(part(d, "title"), forgetTitle(a, s, this.system));
    text(part(d, "message"), forgetMessage(a, this.system));
    this.#over(from, d, {
      answer: (value) => {
        if (value !== "forget") return;
        forget(a.tool, a.label);
        closeDialog(d);
        from.forgotten();
      },
      cancel: () => true,
    });
  }

  rename(from: AccountsWindow, a: Account) {
    const d = this.renameDialog;
    this.#renaming = a;
    text(part(d, "title"), renameTitle(a.label));
    text(part(d, "message"), RENAMING);
    const field = d.querySelector<HTMLInputElement>('input[name="name"]')!;
    field.value = a.label;
    field.placeholder = a.label;
    shown(part(d, "failure"), false);
    this.#checkRename();
    this.#over(from, d, {
      answer: (value) => {
        if (value !== "rename" || !this.#renaming) return;
        const was = this.#renaming;
        const to = field.value.trim();
        const out = rename(was.tool, was.label, to);
        if (out === "label_taken") {
          const taken = model.get().accounts.find((x) => x.tool === was.tool && x.label === to);
          const failure = part(d, "failure")!;
          text(part(failure, "failure-title"), couldNotRename(was.label));
          text(
            part(failure, "failure-message"),
            labelTaken(typed({ tool: was.tool, label: to }), taken?.email ?? ""),
          );
          shown(failure, true);
          say(couldNotRename(was.label));
          field.focus();
          return;
        }
        if (out === "ok") closeDialog(d);
      },
      cancel: () => true,
      focus: field,
    });
    field.select();
  }

  #checkRename() {
    const a = this.#renaming;
    const d = this.renameDialog;
    if (!a) return;
    const field = d.querySelector<HTMLInputElement>('input[name="name"]')!;
    const typedName = field.value.trim();
    const problem = nameProblem(typedName);
    const said =
      problem === "empty" && this.system === "windows"
        ? typeAName(RENAME)
        : problem
          ? unusableName(problem, false)
          : null;
    this.#problem(d, said);
    shown(part(d, "failure"), false);
    const confirm = d.querySelector<HTMLButtonElement>('button[value="rename"]')!;
    confirm.disabled = !!problem || typedName === a.label;
  }

  #problem(d: HTMLDialogElement, said: string | null) {
    const line = part(d, "problem")!;
    shown(line, !!said);
    text(line, said ?? "");
    if (said && said !== this.#said) say(said);
    this.#said = said ?? "";
  }

  // Add Account with `tool` picked, or Sign In Again for `again`
  signIn(from: AccountsWindow, tool: Tool | null, again: Account | null) {
    const s = model.get();
    if (s.signing) return;
    const d = this.signInDialog;
    const offered = tools
      .filter(([t]) => this.system === "macos" || t !== "desktop")
      .map(([t]) => t);
    const picked = again?.tool ?? tool ?? offered[0];
    this.#signing = { tool: picked, again, started: false };
    shown(part(d, "tool"), !again);
    shown(part(d, "name"), !again);
    shown(part(d, "account"), !!again);
    text(part(d, "label"), again?.label ?? "");
    const name = d.querySelector<HTMLInputElement>('input[name="name"]')!;
    name.value = "";
    this.#pick(picked);
    this.#over(from, d, {
      answer: (value) => this.#signInAnswer(value),
      cancel: (by) => this.#signInCancel(by),
      focus: again ? d.querySelector<HTMLElement>('[data-part="confirm"]') : name,
    });
  }

  #pick(tool: Tool) {
    if (!this.#signing) return;
    this.#signing.tool = tool;
    this.#tools?.set(tool);
    for (const radio of this.signInDialog.querySelectorAll<HTMLInputElement>('input[name="tool"]'))
      radio.checked = radio.value === tool;
    this.#form(model.get());
    this.#checkSignIn();
  }

  // the form before it starts: its words and its button for the tool picked, which a render
  // redraws without taking away what the name was last told
  #form(s: State) {
    const d = this.signInDialog;
    const sign = this.#signing;
    if (!sign || sign.started) return;
    const { tool, again } = sign;
    const work = s.inUse.desktop;
    const [message, confirm] =
      tool === "desktop"
        ? again
          ? [parkToSignInAgain(program(tool), again.label, work), park(work)]
          : [parkToAdd(program(tool), work), park(work)]
        : [signInMessage(toolName(tool), again?.label ?? null), signIn(this.system)];
    text(
      part(d, "title"),
      again ? signInToAgain(this.system, again.label) : addAccount(this.system),
    );
    text(part(d, "message"), message);
    const button = part<HTMLButtonElement>(d, "confirm")!;
    text(button, confirm);
    shown(button, true);
    shown(part(d, "form"), true);
    shown(part(d, "page"), false);
    shown(part(d, "waiting"), false);
    text(d.querySelector('[data-does="cancel"]'), CANCEL);
    shown(d.querySelector('[data-does="put-back"]'), false);
  }

  #checkSignIn() {
    const sign = this.#signing;
    if (!sign || sign.started) return;
    const d = this.signInDialog;
    const button = part<HTMLButtonElement>(d, "confirm")!;
    if (sign.again) {
      this.#problem(d, null);
      button.disabled = false;
      return;
    }
    const name = d.querySelector<HTMLInputElement>('input[name="name"]')!.value.trim();
    const problem = nameProblem(name);
    this.#problem(
      d,
      problem === "empty" && this.system === "windows"
        ? typeAName(button.textContent!.trim())
        : problem
          ? unusableName(problem, true)
          : null,
    );
    button.disabled = !!problem;
  }

  #signInAnswer(value: string) {
    const sign = this.#signing;
    const d = this.signInDialog;
    if (!sign) return;
    if (value === "put-back") {
      cancelSignIn();
      return this.close();
    }
    if (value !== "sign-in" || sign.started) return;
    const name =
      sign.again?.label ?? d.querySelector<HTMLInputElement>('input[name="name"]')!.value.trim();
    const taken =
      !sign.again && model.get().accounts.find((a) => a.tool === sign.tool && a.label === name);
    if (taken)
      return this.#problem(d, labelTaken(typed({ tool: sign.tool, label: name }), taken.email));
    sign.started = true;
    signInFromApp(sign.tool, name, !!sign.again);
  }

  // sheets.rs `escape_cancels`: no key puts away a sign-in Claude Desktop runs, where its Cancel or
  // Later button does
  #signInCancel(by: Cancelled) {
    const sign = this.#signing;
    if (by === "key" && sign?.started && model.get().signing?.tool === "desktop") return false;
    if (sign?.started) cancelSignIn();
    return true;
  }

  // the sign-in under way, as the state has it
  render(s: State) {
    const sign = this.#signing;
    if (!sign || !isOpen(this.signInDialog)) return;
    if (!sign.started) return this.#form(s);
    const run = s.signing;
    if (!run) return this.close();
    const d = this.signInDialog;
    const tool = run.tool;
    shown(part(d, "form"), false);
    shown(part<HTMLButtonElement>(d, "confirm"), false);
    shown(part(d, "waiting"), true);
    this.#problem(d, null);
    const cancel = d.querySelector<HTMLElement>('[data-does="cancel"]');
    const back = d.querySelector<HTMLElement>('[data-does="put-back"]');
    const spinner =
      part(d, "waiting")?.querySelector("svg") ?? part(d, "page")?.querySelector("svg");
    if (tool === "desktop") {
      const work = s.inUse.desktop;
      const app = program(tool);
      const waiting = run.phase === "waiting";
      text(
        part(d, "title"),
        run.again ? signInToAgain(this.system, run.label) : addAccount(this.system),
      );
      text(
        part(d, "message"),
        waiting ? inAppWaiting(app, run.label, run.again) : parking(work)[0],
      );
      attr(spinner, "aria-label", waiting ? waitingToQuit(app) : parking(work)[1]);
      shown(part(d, "page"), false);
      text(cancel, waiting ? LATER : CANCEL);
      shown(back, waiting);
      text(back, putBack(this.system, work));
      return;
    }
    text(part(d, "title"), signingInTo(this.system, toolName(tool)));
    text(part(d, "message"), finishInBrowser(run.label, toolName(tool)));
    shown(part(d, "page"), true);
    attr(spinner, "aria-label", WAITING_FOR_BROWSER);
    text(cancel, CANCEL);
    shown(back, false);
  }
}

const made = new Map<System, Sheets | null>();
function sheets(system: System): Sheets | null {
  if (!made.has(system)) {
    const id = system === "windows" ? "win-accounts" : "mac-accounts";
    const get = (name: string) =>
      document.getElementById(`${id}-${name}`) as HTMLDialogElement | null;
    const [f, r, i] = [get("forget"), get("rename"), get("sign-in")];
    made.set(system, f && r && i ? new Sheets(system, f, r, i) : null);
  }
  return made.get(system)!;
}

customElements.define("pb-accounts", AccountsWindow);
