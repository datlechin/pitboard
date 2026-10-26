// The Forget alert, the Rename sheet and the sign-in sheet of each system, drawn once and opened
// over whichever window asks.
import { combobox, type Combobox } from "./combobox.ts";
import { closeDialog, isOpen, openDialog, type Cancelled } from "./dialog.ts";
import { attr, shown, text } from "./draw.ts";
import { say } from "./say.ts";
import {
  cancelSignIn,
  forget,
  model,
  nameProblem,
  rename,
  signInFromApp,
  typed,
  type State,
} from "../../lib/model.ts";
import {
  addAccount,
  couldNotRename,
  finishInBrowser,
  forgetMessage,
  forgetTitle,
  inAppWaiting,
  labelTaken,
  park,
  parkToAdd,
  parkToSignInAgain,
  parking,
  program,
  putBack,
  renameTitle,
  RENAMING,
  signIn,
  signInMessage,
  signInToAgain,
  signingInTo,
  typeAName,
  unusableName,
  waitingToQuit,
  RENAME,
  CANCEL,
  LATER,
  WAITING_FOR_BROWSER,
  type System,
} from "../../data/window.ts";
import { tools, type Account, type Tool } from "../../data/world.ts";

// the window a sheet opens over: its element is the desk, and Forget hands it back the list
export interface SheetOwner extends HTMLElement {
  window: HTMLElement;
  forgotten(): void;
}

const toolName = (tool: Tool) => tools.find(([t]) => t === tool)![1];
const part = <T extends Element = HTMLElement>(in_: ParentNode, name: string) =>
  in_.querySelector<T>(`[data-part="${name}"]`);

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
    this.#renaming = null;
    this.#signing = null;
  }

  forget(from: SheetOwner, a: Account) {
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

  rename(from: SheetOwner, a: Account) {
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
  signIn(from: SheetOwner, tool: Tool | null, again: Account | null) {
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
