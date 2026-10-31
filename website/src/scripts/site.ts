import { say } from "./live/say.ts";
import { platformOf, type System } from "../lib/platform.ts";

const d = document.documentElement;

function put(k: string, v: string) {
  try {
    localStorage.setItem(k, v);
  } catch {}
}

function all<T extends Element = HTMLElement>(sel: string, root: ParentNode = document): T[] {
  return Array.from(root.querySelectorAll<T>(sel));
}

function head() {
  const set = () => d.classList.toggle("scrolled", scrollY > 4);
  addEventListener("scroll", set, { passive: true });
  set();
}

function theming() {
  const names: Record<string, string> = { system: "System", light: "Light", dark: "Dark" };
  const btn = document.getElementById("theme-btn");
  const menu = document.getElementById("theme-menu");
  const items = menu ? all<HTMLButtonElement>("[role=menuitemradio]", menu) : [];
  const segs = all<HTMLButtonElement>("#theme-seg button");
  const metas = all<HTMLMetaElement>('meta[name="theme-color"]');
  const current = () => d.getAttribute("data-theme") || "system";
  function sync() {
    const v = current();
    btn?.setAttribute("aria-label", `Theme: ${names[v]}`);
    for (const i of items) i.setAttribute("aria-checked", String(i.dataset.v === v));
    for (const b of segs) b.setAttribute("aria-pressed", String(b.dataset.v === v));
    for (const m of metas) {
      const own = m.dataset.scheme === "dark" ? "#090b0b" : "#ffffff";
      m.content = v === "system" ? own : v === "dark" ? "#090b0b" : "#ffffff";
    }
  }
  function set(v: string) {
    if (v === "system") d.removeAttribute("data-theme");
    else d.setAttribute("data-theme", v);
    put("theme", v);
    sync();
  }
  for (const b of segs) b.addEventListener("click", () => set(b.dataset.v!));
  sync();
  if (!btn || !menu) return;
  const open = (i?: number) => {
    menu.hidden = false;
    btn.setAttribute("aria-expanded", "true");
    const n =
      i ??
      Math.max(
        0,
        items.findIndex((x) => x.dataset.v === current()),
      );
    items[n].focus();
  };
  const close = (refocus: boolean) => {
    menu.hidden = true;
    btn.setAttribute("aria-expanded", "false");
    if (refocus) btn.focus();
  };
  btn.addEventListener("click", () => (menu.hidden ? open() : close(false)));
  btn.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      open(0);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      open(items.length - 1);
    }
  });
  items.forEach((it, i) => {
    it.addEventListener("click", () => {
      set(it.dataset.v!);
      close(true);
    });
    it.addEventListener("keydown", (e) => {
      let n: number | null = null;
      if (e.key === "ArrowDown") n = (i + 1) % items.length;
      else if (e.key === "ArrowUp") n = (i - 1 + items.length) % items.length;
      else if (e.key === "Home") n = 0;
      else if (e.key === "End") n = items.length - 1;
      else if (e.key === "Escape") {
        e.preventDefault();
        close(true);
        return;
      } else if (e.key === "Tab") {
        close(false);
        return;
      }
      if (n != null) {
        e.preventDefault();
        items[n].focus();
      }
    });
  });
  document.addEventListener("click", (e) => {
    if (!menu.hidden && !(e.target as Element).closest("#theme")) close(false);
  });
}

function phoneMenu() {
  for (const m of all<HTMLDetailsElement>("#mnav")) {
    m.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && m.open) {
        e.preventDefault();
        m.open = false;
        m.querySelector("summary")?.focus();
      }
    });
  }
}

// Each choice the root keeps as data-<name>, and the key that stores it.
const keys = { surface: "tab", platform: "platform", desktop: "desktop" } as const;
type Choice = keyof typeof keys;
interface Chosen {
  name: Choice;
  value: string;
}

// The tabs and pressed buttons carrying a choice's attribute set it, and all of them follow it.
function choice(name: Choice): (value: string) => void {
  const tabs = all<HTMLButtonElement>(`[role=tab][data-${name}]`);
  const picks = all<HTMLButtonElement>(`button[data-${name}]:not([role=tab])`);
  const mark = (value: string) => {
    for (const t of tabs) {
      const on = t.dataset[name] === value;
      t.setAttribute("aria-selected", String(on));
      t.tabIndex = on ? 0 : -1;
    }
    for (const b of picks) b.setAttribute("aria-pressed", String(b.dataset[name] === value));
  };
  const show = (value: string) => {
    mark(value);
    d.dataset[name] = value;
    document.dispatchEvent(new CustomEvent<Chosen>("choice", { detail: { name, value } }));
  };
  const select = (value: string, focus: HTMLElement | null) => {
    show(value);
    focus?.focus();
    put(keys[name], value);
  };
  for (const b of picks) b.addEventListener("click", () => select(b.dataset[name]!, null));
  for (const tl of all("[role=tablist]")) {
    const own = all<HTMLButtonElement>(`[role=tab][data-${name}]`, tl);
    own.forEach((t, i) => {
      t.addEventListener("click", () => select(t.dataset[name]!, null));
      t.addEventListener("keydown", (e) => {
        let n: HTMLButtonElement | null = null;
        if (e.key === "ArrowRight") n = own[(i + 1) % own.length];
        else if (e.key === "ArrowLeft") n = own[(i - 1 + own.length) % own.length];
        else if (e.key === "Home") n = own[0];
        else if (e.key === "End") n = own[own.length - 1];
        if (n) {
          e.preventDefault();
          select(n.dataset[name]!, n);
        }
      });
    });
  }
  const now = d.dataset[name];
  if (now) mark(now);
  return show;
}

// A link into a <details> opens it, and one into another system's part shows that system.
function openTarget(showPlatform: (value: string) => void, showDesktop: (value: string) => void) {
  const id = decodeURIComponent(location.hash.slice(1));
  const target = id ? document.getElementById(id) : null;
  const box = target?.closest("details");
  if (box) box.open = true;
  const on = target?.closest<HTMLElement>("[data-on]")?.dataset.on?.split(" ") as
    System[] | undefined;
  const platform = d.dataset.platform;
  const reader = platform === "linux" ? d.dataset.desktop : platform;
  if (!on || on.includes(reader as System)) return;
  const to = on.find((s) => platformOf(s) === platform) ?? on[0];
  if (platformOf(to) !== platform) showPlatform(platformOf(to));
  if (platformOf(to) === "linux" && to !== d.dataset.desktop) showDesktop(to);
}

function copying() {
  for (const b of all<HTMLButtonElement>("[data-copy]")) {
    const label = b.textContent;
    const name = b.getAttribute("aria-label");
    let timer = 0;
    let shown: Element | null = null;
    const restore = () => {
      b.textContent = label;
      if (name) b.setAttribute("aria-label", name);
      say("");
      if (shown) {
        shown.classList.add("sr-only");
        shown.parentElement?.removeAttribute("data-full");
        shown = null;
      }
    };
    const done = (ok: boolean) => {
      if (ok) {
        b.textContent = "Copied";
        // the visible label stays part of the accessible name (WCAG 2.5.3)
        if (name) b.setAttribute("aria-label", "Copied");
        say("Copied");
      } else {
        const code = b.parentElement?.querySelector("code");
        if (code) {
          // a digest shown in part shows whole while it is selected
          const hid = code.querySelector(".sr-only");
          if (hid) {
            hid.classList.remove("sr-only");
            code.setAttribute("data-full", "");
            shown = hid;
          }
          const r = document.createRange();
          r.selectNodeContents(code);
          const s = getSelection();
          s?.removeAllRanges();
          s?.addRange(r);
        } else {
          // a link to a part of this page: the address bar holds it instead
          const text = b.dataset.copy!;
          const at = text.indexOf("#");
          const part = at > -1 ? document.getElementById(text.slice(at + 1)) : null;
          if (part) history.replaceState(null, "", `#${part.id}`);
        }
      }
      clearTimeout(timer);
      timer = window.setTimeout(restore, 2000);
    };
    b.addEventListener("click", () => {
      if (navigator.clipboard?.writeText) {
        navigator.clipboard.writeText(b.dataset.copy!).then(
          () => done(true),
          () => done(false),
        );
      } else done(false);
    });
  }
}

function afterDownload() {
  for (const a of all("[data-download]")) {
    a.addEventListener("click", () => {
      for (const p of all(`[data-after="${a.dataset.download}"]`)) p.setAttribute("data-shown", "");
    });
  }
}

// Tabs marked data-tabs, each showing its own part of the page: a click or the arrows, Home and End
// choose one.
function tabs() {
  for (const list of all("[role=tablist][data-tabs]")) {
    const own = all<HTMLButtonElement>("[role=tab]", list);
    const select = (tab: HTMLButtonElement) => {
      for (const t of own) {
        const on = t === tab;
        t.setAttribute("aria-selected", String(on));
        t.tabIndex = on ? 0 : -1;
        document
          .getElementById(t.getAttribute("aria-controls")!)
          ?.toggleAttribute("data-chosen", on);
      }
    };
    own.forEach((t, i) => {
      t.addEventListener("click", () => select(t));
      t.addEventListener("keydown", (e) => {
        let n: HTMLButtonElement | null = null;
        if (e.key === "ArrowRight") n = own[(i + 1) % own.length];
        else if (e.key === "ArrowLeft") n = own[(i - 1 + own.length) % own.length];
        else if (e.key === "Home") n = own[0];
        else if (e.key === "End") n = own[own.length - 1];
        if (n) {
          e.preventDefault();
          select(n);
          n.focus();
        }
      });
    });
  }
}

// A browser that names its machine gives an Arm PC the file its download link keeps for one.
interface Hints {
  platform?: string;
  getHighEntropyValues?(hints: string[]): Promise<{ architecture?: string; bitness?: string }>;
}
function armDownloads() {
  const links = all<HTMLAnchorElement>("a[data-arm]");
  const hints = (navigator as Navigator & { userAgentData?: Hints }).userAgentData;
  if (!links.length || hints?.platform !== "Windows" || !hints.getHighEntropyValues) return;
  hints.getHighEntropyValues(["architecture", "bitness"]).then(
    ({ architecture }) => {
      if (architecture !== "arm") return;
      for (const a of links) a.href = a.dataset.arm!;
    },
    () => {},
  );
}

// The hero's demo is its live module's (live/demo.ts); a page without the hero has none to play.
function demo() {
  if (!document.querySelector("[data-demo-desk]")) d.removeAttribute("data-demo");
}

head();
theming();
phoneMenu();
choice("surface");
const showPlatform = choice("platform");
const showDesktop = choice("desktop");
openTarget(showPlatform, showDesktop);
addEventListener("hashchange", () => openTarget(showPlatform, showDesktop));
tabs();
copying();
afterDownload();
armDownloads();
demo();
