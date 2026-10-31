// The Linux app's dialogs (dialogs/), drawn once with the cut window's and opened over whichever
// window asks, on either desktop: Add Account and Sign In Again with the sign-in they wait on,
// Name This Account and Rename, Remove's question, and Open Link, Keyboard Shortcuts and About
// Pitboard from the main menu.
import { closeDialog, isOpen, openDialog, type Cancelled } from "./dialog.ts";
import { attr, shown, text } from "./draw.ts";
import { say } from "./say.ts";
import type { SheetOwner, Sheets } from "./sheets.ts";
import {
  cancelSignIn,
  forget,
  model,
  nameProblem,
  named,
  rename,
  signInFromApp,
  type State,
} from "../../lib/model.ts";
import { systemOf } from "../../lib/platform.ts";
import { toolsOn } from "../../lib/words.ts";
import {
  DONE,
  addAccount,
  couldNotRename,
  finishInBrowser,
  nameTaken,
  nameThisAccount,
  naming,
  removeMessage,
  removeTitle,
  renameTitle,
  RENAME,
  RENAMING,
  signInAsking,
  signInMessage,
  signInToAgain,
  signingInTo,
  unusableName,
} from "../../data/window.ts";
import type { Account, Tool } from "../../data/world.ts";

const FRONT = "gnome";
const tools = toolsOn("linux");
const toolName = (tool: Tool) => tools.find(([t]) => t === tool)![1];
const part = <T extends Element = HTMLElement>(in_: ParentNode, name: string) =>
  in_.querySelector<T>(`[data-part="${name}"]`);

export type Shown = "open-link" | "shortcuts" | "about";

class LinuxSheets implements Sheets {
  // the account the name dialog names, and whether it was just added rather than renamed
  #naming: { a: Account; added: boolean } | null = null;
  #signing: { tool: Tool; again: Account | null; started: boolean; from: SheetOwner } | null = null;
  #said = "";

  constructor(readonly get: (name: string) => HTMLDialogElement) {
    const name = this.#field();
    name.addEventListener("input", () => this.#checkName());
    for (const toggle of this.#toggles())
      toggle.addEventListener("click", () => this.#pick(toggle.value as Tool));
  }

  #field() {
    return this.get("name").querySelector<HTMLInputElement>('input[name="name"]')!;
  }

  #toggles() {
    return [
      ...this.get("sign-in").querySelectorAll<HTMLButtonElement>('[data-part="tool"] button'),
    ];
  }

  #over(
    from: SheetOwner,
    dialog: HTMLDialogElement,
    o: {
      answer: (value: string) => void;
      cancel: (by: Cancelled) => boolean;
      focus?: HTMLElement | null;
    },
  ) {
    this.#dismiss();
    openDialog(dialog, { system: systemOf(from), window: from.window, desk: from.desk, ...o });
  }

  #dismiss() {
    for (const name of ["sign-in", "name", "remove", "open-link", "shortcuts", "about"]) {
      const d = this.get(name);
      if (isOpen(d)) closeDialog(d);
    }
  }

  close() {
    this.#dismiss();
    if (this.#naming?.added) named();
    this.#naming = null;
    this.#signing = null;
  }

  // the main menu's dialogs, which say what they say and close; Open Link's Paste Link reads
  // nothing, as the page reads no clipboard it was not given
  show(from: SheetOwner, what: Shown) {
    this.#over(from, this.get(what), { answer: () => {}, cancel: () => true });
  }

  // AdwAlertDialog with Remove the default response, and so focused (adw-alert-dialog.c
  // grab_focus), as the model makes it (present/accounts.rs `forget`, dialogs/question.rs)
  forget(from: SheetOwner, a: Account) {
    const d = this.get("remove");
    text(part(d, "title"), removeTitle(a, model.get(), FRONT));
    text(part(d, "message"), removeMessage(a, FRONT));
    this.#over(from, d, {
      answer: (value) => {
        if (value !== "forget") return;
        forget(a.tool, a.label);
        closeDialog(d);
        from.forgotten();
      },
      cancel: () => true,
      focus: d.querySelector<HTMLElement>('button[value="forget"]'),
    });
  }

  // Rename, or Name This Account for one just added under the name the core picked, which has no
  // Cancel and cannot be closed but by Done (dialogs/name.rs, sheet.ui)
  rename(from: SheetOwner, a: Account, added = false) {
    const d = this.get("name");
    this.#naming = { a, added };
    text(part(d, "title"), added ? nameThisAccount(FRONT) : renameTitle(a.label));
    text(part(d, "message"), added ? naming(a.email, toolName(a.tool)) : RENAMING);
    const confirm = part<HTMLButtonElement>(d, "confirm")!;
    text(confirm, added ? DONE : RENAME);
    shown(part(d, "cancel"), !added);
    const field = this.#field();
    field.value = a.label;
    shown(part(d, "failure"), false);
    this.#checkName();
    this.#over(from, d, {
      answer: (value) => {
        // Return in the name's row answers as the default button does
        if (value === "name" || value === "") this.#rename();
      },
      cancel: () => {
        if (this.#naming?.added) return false;
        this.#naming = null;
        return true;
      },
      focus: field,
    });
    field.select();
  }

  #rename() {
    const was = this.#naming;
    const d = this.get("name");
    const confirm = part<HTMLButtonElement>(d, "confirm")!;
    if (!was || confirm.disabled) return;
    const to = this.#field().value.trim();
    if (to === was.a.label) return this.close();
    const out = rename(was.a.tool, was.a.label, to);
    if (out === "label_taken") {
      const taken = model.get().accounts.find((x) => x.tool === was.a.tool && x.label === to);
      const failure = part(d, "failure")!;
      text(failure, `${couldNotRename(was.a.label)}\n${nameTaken(to, taken?.email ?? "")}`);
      shown(failure, true);
      say(`${couldNotRename(was.a.label)} ${nameTaken(to, taken?.email ?? "")}`);
      this.#field().focus();
      return;
    }
    if (out === "ok") this.close();
  }

  #checkName() {
    const was = this.#naming;
    if (!was) return;
    const d = this.get("name");
    const typed = this.#field().value.trim();
    const problem = nameProblem(typed);
    const said = problem && unusableName(problem);
    const line = part(d, "problem")!;
    shown(line, !!said);
    text(line, said ?? "");
    if (said && said !== this.#said) say(said);
    this.#said = said ?? "";
    shown(part(d, "failure"), false);
    part<HTMLButtonElement>(d, "confirm")!.disabled =
      !!problem || (!was.added && typed === was.a.label);
  }

  // Add Account with `tool` picked, or Sign In Again for `again` (dialogs/sign_in.rs): the
  // sign-in first, then a new account is named
  signIn(from: SheetOwner, tool: Tool | null, again: Account | null) {
    if (model.get().signing) return;
    const d = this.get("sign-in");
    const picked = again?.tool ?? tool ?? tools[0][0];
    this.#signing = { tool: picked, again, started: false, from };
    this.#pick(picked);
    this.#over(from, d, {
      answer: (value) => this.#signInAnswer(value),
      cancel: (by) => this.#signInCancel(by),
      focus: part(d, "confirm"),
    });
  }

  #pick(tool: Tool) {
    if (!this.#signing || this.#signing.started) return;
    this.#signing.tool = tool;
    for (const toggle of this.#toggles())
      attr(toggle, "aria-pressed", String(toggle.value === tool));
    this.#form();
  }

  // the dialog before it starts: the toggle for the tool where one is added, or the account
  // signed in to again, and the step's button
  #form() {
    const d = this.get("sign-in");
    const sign = this.#signing;
    if (!sign) return;
    const { tool, again } = sign;
    text(part(d, "title"), again ? signInToAgain(FRONT, again.label) : addAccount(FRONT));
    text(part(d, "message"), signInMessage(toolName(tool), again?.label ?? null));
    shown(part(d, "tool"), !again && tools.length > 1);
    shown(part(d, "account"), !!again);
    text(part(d, "label"), again?.label ?? "");
    const confirm = part<HTMLButtonElement>(d, "confirm")!;
    text(confirm, signInAsking(FRONT));
    shown(confirm, true);
    shown(part(d, "waiting"), false);
    shown(part(d, "link"), false);
  }

  #signInAnswer(value: string) {
    const sign = this.#signing;
    if (value !== "sign-in" || !sign || sign.started) return;
    sign.started = true;
    signInFromApp(sign.tool, sign.again?.label ?? null);
  }

  // Escape is Cancel while the sign-in runs, as before it (sign_in.rs)
  #signInCancel(_by: Cancelled) {
    if (this.#signing?.started) cancelSignIn();
    this.#signing = null;
    return true;
  }

  render(s: State) {
    const sign = this.#signing;
    const d = this.get("sign-in");
    if (!sign || !isOpen(d) || !sign.started) return;
    const run = s.signing;
    if (!run) {
      // a new account is added under the name the core picked, which Name This Account offers
      const added =
        s.naming &&
        s.accounts.find((a) => a.tool === s.naming!.tool && a.label === s.naming!.label);
      const from = sign.from;
      this.close();
      if (added && !sign.again) this.rename(from, added, true);
      return;
    }
    text(part(d, "title"), signingInTo(FRONT, toolName(run.tool)));
    text(part(d, "message"), finishInBrowser(run.label));
    shown(part(d, "tool"), false);
    shown(part(d, "account"), false);
    shown(part<HTMLButtonElement>(d, "confirm"), false);
    shown(part(d, "waiting"), true);
    shown(part(d, "link"), true);
  }
}

let made: LinuxSheets | null | undefined;
export function linuxSheets(): LinuxSheets | null {
  if (made === undefined) {
    const get = (name: string) =>
      document.getElementById(`linux-accounts-${name}`) as HTMLDialogElement | null;
    made = get("sign-in") ? new LinuxSheets((name) => get(name)!) : null;
  }
  return made;
}
