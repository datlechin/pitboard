// The Remove alert, the Rename sheet, which names an account just added too, and the sign-in sheet
// of each system, drawn once and opened over whichever window asks.
import { combobox, type Combobox } from "./combobox.ts";
import { closeDialog, isOpen, openDialog, type Cancelled } from "./dialog.ts";
import { attr, shown, text } from "./draw.ts";
import { say } from "./say.ts";
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
import {
  CONTINUE,
  DONE,
  NAME_PROMPT,
  addAccount,
  couldNotRename,
  finishInBrowser,
  inAppNote,
  inAppSteps,
  nameTaken,
  nameThisAccount,
  naming,
  program,
  removeMessage,
  removeTitle,
  renameTitle,
  RENAMING,
  signInAsking,
  signInMessage,
  signInToAgain,
  signingInTo,
  signsOutFirst,
  signingOut,
  switchBackTo,
  unusableName,
  waitingToQuit,
  RENAME,
  CANCEL,
  LATER,
  WAITING_FOR_BROWSER,
  type System,
} from "../../data/window.ts";
import { tools, type Account, type Tool } from "../../data/world.ts";

// the window a sheet opens over: its element is the desk, and Remove hands it back the list
export interface SheetOwner extends HTMLElement {
  window: HTMLElement;
  forgotten(): void;
}

const toolName = (tool: Tool) => tools.find(([t]) => t === tool)![1];
const part = <T extends Element = HTMLElement>(in_: ParentNode, name: string) =>
  in_.querySelector<T>(`[data-part="${name}"]`);

class Sheets {
  // the account the Rename sheet names, and whether it was just added rather than renamed
  #renaming: { a: Account; added: boolean } | null = null;
  #signing: { tool: Tool; again: Account | null; started: boolean; from: SheetOwner } | null = null;
  #tools: Combobox | null = null;
  #said = "";

  constructor(
    readonly system: System,
    readonly forgetDialog: HTMLDialogElement,
    readonly renameDialog: HTMLDialogElement,
    readonly signInDialog: HTMLDialogElement,
  ) {
    const field = renameDialog.querySelector<HTMLInputElement>('input[name="name"]')!;
    field.addEventListener("input", () => this.#checkName());
    for (const radio of signInDialog.querySelectorAll<HTMLInputElement>('input[name="tool"]'))
      radio.addEventListener("change", () => this.#pick(radio.value as Tool));
    const box = part(signInDialog, "tool-box");
    const list = signInDialog.querySelector<HTMLElement>('[role="listbox"]');
    if (box && list) this.#tools = combobox(box, list, "popup", (v) => this.#pick(v as Tool));
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
    openDialog(dialog, { system: this.system, window: from.window, desk: from, ...o });
  }

  #dismiss() {
    for (const d of [this.forgetDialog, this.renameDialog, this.signInDialog])
      if (isOpen(d)) closeDialog(d);
  }

  close() {
    this.#dismiss();
    if (this.#renaming?.added) named();
    this.#renaming = null;
    this.#signing = null;
  }

  forget(from: SheetOwner, a: Account) {
    const s = model.get();
    const d = this.forgetDialog;
    text(part(d, "title"), removeTitle(a, s, this.system));
    text(part(d, "message"), removeMessage(a, this.system));
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

  // Rename, or Name This Account for one just added under the name the core picked, which Done
  // and Cancel keep where it is not changed (present/sheets.rs `check_name`)
  rename(from: SheetOwner, a: Account, added = false) {
    const d = this.renameDialog;
    this.#renaming = { a, added };
    text(part(d, "title"), added ? nameThisAccount(this.system) : renameTitle(a.label));
    text(part(d, "message"), added ? naming(a.email, toolName(a.tool)) : RENAMING);
    text(part(d, "confirm"), added ? DONE : RENAME);
    const field = d.querySelector<HTMLInputElement>('input[name="name"]')!;
    field.value = a.label;
    field.placeholder = added ? NAME_PROMPT : a.label;
    shown(part(d, "failure"), false);
    this.#checkName();
    this.#over(from, d, {
      answer: (value) => {
        const was = this.#renaming;
        if (value !== "rename" || !was) return;
        const to = field.value.trim();
        if (to === was.a.label) return this.close();
        const out = rename(was.a.tool, was.a.label, to);
        if (out === "label_taken") {
          const taken = model.get().accounts.find((x) => x.tool === was.a.tool && x.label === to);
          const failure = part(d, "failure")!;
          text(part(failure, "failure-title"), couldNotRename(was.a.label));
          text(part(failure, "failure-message"), nameTaken(to, taken?.email ?? ""));
          shown(failure, true);
          say(couldNotRename(was.a.label));
          field.focus();
          return;
        }
        if (out === "ok") this.close();
      },
      cancel: () => {
        if (this.#renaming?.added) named();
        this.#renaming = null;
        return true;
      },
      focus: field,
    });
    field.select();
  }

  #checkName() {
    const was = this.#renaming;
    const d = this.renameDialog;
    if (!was) return;
    const field = d.querySelector<HTMLInputElement>('input[name="name"]')!;
    const typed = field.value.trim();
    const problem = nameProblem(typed);
    this.#problem(d, problem && unusableName(problem));
    shown(part(d, "failure"), false);
    const confirm = d.querySelector<HTMLButtonElement>('button[value="rename"]')!;
    confirm.disabled = !!problem || (!was.added && typed === was.a.label);
  }

  #problem(d: HTMLDialogElement, said: string | null) {
    const line = part(d, "problem")!;
    shown(line, !!said);
    text(line, said ?? "");
    if (said && said !== this.#said) say(said);
    this.#said = said ?? "";
  }

  // Add Account with `tool` picked, or Sign In Again for `again`: the sign-in comes first, and a
  // new account is named once it is added
  signIn(from: SheetOwner, tool: Tool | null, again: Account | null) {
    const s = model.get();
    if (s.signing) return;
    const d = this.signInDialog;
    const offered = tools
      .filter(([t]) => this.system === "macos" || t !== "desktop")
      .map(([t]) => t);
    const picked = again?.tool ?? tool ?? offered[0];
    this.#signing = { tool: picked, again, started: false, from };
    shown(part(d, "tool"), !again);
    shown(part(d, "account"), !!again);
    text(part(d, "label"), again?.label ?? "");
    this.#pick(picked);
    this.#over(from, d, {
      answer: (value) => this.#signInAnswer(value),
      cancel: (by) => this.#signInCancel(by),
      focus: d.querySelector<HTMLElement>('[data-part="confirm"]'),
    });
  }

  #pick(tool: Tool) {
    if (!this.#signing) return;
    this.#signing.tool = tool;
    this.#tools?.set(tool);
    for (const radio of this.signInDialog.querySelectorAll<HTMLInputElement>('input[name="tool"]'))
      radio.checked = radio.value === tool;
    this.#form(model.get());
  }

  // the form before it starts: its words and its button for the tool picked
  #form(s: State) {
    const d = this.signInDialog;
    const sign = this.#signing;
    if (!sign || sign.started) return;
    const { tool, again } = sign;
    const work = s.inUse.desktop;
    const [message, confirm] =
      tool === "desktop"
        ? [signsOutFirst(program(tool), work, again?.label ?? null), CONTINUE]
        : [signInMessage(toolName(tool), again?.label ?? null), signInAsking(this.system)];
    this.#steps(tool === "desktop" ? inAppSteps(program(tool), again?.label ?? null) : []);
    text(
      part(d, "title"),
      again ? signInToAgain(this.system, again.label) : addAccount(this.system),
    );
    text(part(d, "message"), message);
    const button = part<HTMLButtonElement>(d, "confirm")!;
    text(button, confirm);
    shown(button, true);
    button.disabled = false;
    shown(part(d, "form"), true);
    shown(part(d, "page"), false);
    shown(part(d, "waiting"), false);
    text(d.querySelector('[data-does="cancel"]'), CANCEL);
    shown(d.querySelector('[data-does="put-back"]'), false);
  }

  #signInAnswer(value: string) {
    const sign = this.#signing;
    if (!sign) return;
    if (value === "put-back") {
      cancelSignIn();
      return this.close();
    }
    if (value !== "sign-in" || sign.started) return;
    sign.started = true;
    signInFromApp(sign.tool, sign.again?.label ?? null);
  }

  // sheets.rs `escape_cancels`: no key puts away a sign-in Claude Desktop runs, where its Cancel or
  // Later button does
  #signInCancel(by: Cancelled) {
    const sign = this.#signing;
    if (by === "key" && sign?.started && model.get().signing?.tool === "desktop") return false;
    if (sign?.started) cancelSignIn();
    this.#signing = null;
    return true;
  }

  render(s: State) {
    const sign = this.#signing;
    if (!sign) return;
    if (!isOpen(this.signInDialog)) return;
    if (!sign.started) return this.#form(s);
    const run = s.signing;
    if (!run) {
      // a new account is added under the name the core picked, which the name sheet offers
      const added =
        s.naming &&
        s.accounts.find((a) => a.tool === s.naming!.tool && a.label === s.naming!.label);
      const from = sign.from;
      this.close();
      if (added && !sign.again) this.rename(from, added, true);
      return;
    }
    const d = this.signInDialog;
    const tool = run.tool;
    shown(part(d, "form"), false);
    shown(part<HTMLButtonElement>(d, "confirm"), false);
    this.#problem(d, null);
    const cancel = d.querySelector<HTMLElement>('[data-does="cancel"]');
    const back = d.querySelector<HTMLElement>('[data-does="put-back"]');
    // macOS's sheet says the wait in a row of its form, or under Claude's steps, under its title
    const line = part(d, "wait-line");
    if (tool === "desktop") {
      const work = s.inUse.desktop;
      const app = program(tool);
      const waiting = run.phase === "waiting";
      text(
        part(d, "title"),
        run.label !== null ? signInToAgain(this.system, run.label) : addAccount(this.system),
      );
      text(part(d, "message"), "");
      this.#steps(inAppSteps(app, run.again ? run.label : null));
      shown(part(d, "waiting"), true);
      text(part(d, "wait-note"), waiting ? inAppNote(app) : signingOut(work)[0]);
      attr(
        part(d, "waiting")?.querySelector("svg") ?? null,
        "aria-label",
        waiting ? waitingToQuit(app) : signingOut(work)[1],
      );
      shown(part(d, "page"), false);
      text(cancel, waiting ? LATER : CANCEL);
      shown(back, waiting);
      text(back, switchBackTo(this.system, work));
      return;
    }
    text(part(d, "title"), signingInTo(this.system, toolName(tool)));
    shown(part(d, "page"), true);
    if (line) {
      text(part(d, "message"), "");
      text(line, finishInBrowser(run.label));
      shown(part(d, "waiting"), false);
    } else {
      text(part(d, "message"), finishInBrowser(run.label));
      shown(part(d, "waiting"), true);
      attr(
        part(d, "waiting")?.querySelector("svg") ?? part(d, "page")?.querySelector("svg") ?? null,
        "aria-label",
        WAITING_FOR_BROWSER,
      );
    }
    text(cancel, CANCEL);
    shown(back, false);
  }

  // Claude's numbered steps, where the sheet draws them
  #steps(said: string[]) {
    const box = part(this.signInDialog, "steps");
    if (!box) return;
    shown(box, said.length > 0);
    box.querySelectorAll('[data-part="step"]').forEach((step, i) => {
      shown(step as HTMLElement, i < said.length);
      text(part(step, "said"), said[i] ?? "");
    });
  }
}

const made = new Map<System, Sheets | null>();
export function sheets(system: System): Sheets | null {
  if (!made.has(system)) {
    const id = system === "windows" ? "win-accounts" : "mac-accounts";
    const get = (name: string) =>
      document.getElementById(`${id}-${name}`) as HTMLDialogElement | null;
    const [f, r, i] = [get("forget"), get("rename"), get("sign-in")];
    made.set(system, f && r && i ? new Sheets(system, f, r, i) : null);
  }
  return made.get(system)!;
}
