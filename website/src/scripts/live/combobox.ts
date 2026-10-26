// A select-only combobox (WAI-ARIA APG) with the keys and pointer rules of macOS's pop-up button
// or WinUI's ComboBox. Focus stays on the box.

export type Kind = "popup" | "combobox";

export interface Combobox {
  set(value: string): void;
  // shut, choosing nothing, as Reset leaves it
  close(): void;
}

// a run of keys typed within this long names one item
const TYPED_MS = 1000;
// NSMenu blinks a chosen item off and on once before the menu goes
const BLINK_MS = 70;
// the menu's fade as it goes, as the static menu's transition draws it
const FADE_MS = 150;

export function combobox(
  box: HTMLElement,
  list: HTMLElement,
  kind: Kind,
  choose: (value: string) => void,
): Combobox {
  const options = [...list.querySelectorAll<HTMLElement>('[role="option"]')];
  const values = options.map((o) => o.dataset.value!);
  const shown = [...box.querySelectorAll<HTMLElement>("[data-value]")];
  let value = "";
  let active = -1;
  let busy = false;
  // the press that is down: the item it started on, whether it opened the menu, and whether it
  // has moved off that item since
  let press: { from: number; opened: boolean; moved: boolean } | null = null;
  let typed = "";
  let typedAt = 0;
  // where the pointer last moved to
  let at = "";

  const isOpen = () => !list.hidden;
  const chosen = () => values.indexOf(value);

  function activate(i: number) {
    active = i;
    options.forEach((o, j) => o.setAttribute("aria-selected", String(j === i)));
    if (i >= 0 && isOpen()) box.setAttribute("aria-activedescendant", options[i].id);
    else box.removeAttribute("aria-activedescendant");
  }

  function open() {
    if (isOpen()) return;
    list.style.setProperty("--at", String(chosen()));
    list.hidden = false;
    box.setAttribute("aria-expanded", "true");
    activate(chosen());
    document.addEventListener("pointerdown", outside, true);
  }

  function close() {
    list.hidden = true;
    delete list.dataset.state;
    box.setAttribute("aria-expanded", "false");
    activate(chosen());
    press = null;
    busy = false;
    document.removeEventListener("pointerdown", outside, true);
  }

  const take = (v: string) => {
    if (v !== value) choose(v);
  };

  function commit(i: number) {
    if (i < 0) return close();
    if (kind === "combobox") {
      close();
      return take(values[i]);
    }
    busy = true;
    const item = options[i];
    item.dataset.state = "blink";
    setTimeout(() => {
      delete item.dataset.state;
      setTimeout(() => {
        list.dataset.state = "closing";
        setTimeout(() => {
          close();
          take(values[i]);
        }, FADE_MS);
      }, BLINK_MS);
    }, BLINK_MS);
  }

  function outside(e: PointerEvent) {
    const t = e.target as Node;
    if (!list.contains(t) && !box.contains(t)) close();
  }

  const under = (e: PointerEvent) => {
    const o = document
      .elementFromPoint(e.clientX, e.clientY)
      ?.closest<HTMLElement>('[role="option"]');
    return o ? options.indexOf(o) : -1;
  };

  // macOS: the press opens the menu, the highlight follows the pointer, and the release chooses
  // the item under it, unless it came straight back up on the item it opened over
  function track(e: PointerEvent) {
    // WebKit also sends a move when the page moves under a pointer that stays still
    const here = `${e.clientX} ${e.clientY}`;
    if (here === at) return;
    at = here;
    if (busy || !isOpen()) return;
    const i = under(e);
    if (press && i !== press.from) press.moved = true;
    activate(i);
  }

  function release(e: PointerEvent) {
    if (!press || busy) return;
    const i = under(e);
    const { from, opened, moved } = press;
    press = null;
    if (i >= 0 && (!opened || moved || i !== from)) commit(i);
    else if (opened && moved) close();
  }

  if (kind === "popup") {
    box.addEventListener("pointerdown", (e) => {
      if (e.button !== 0 || busy) return;
      e.preventDefault();
      box.focus({ preventScroll: true });
      if (isOpen()) return close();
      open();
      press = { from: chosen(), opened: true, moved: false };
    });
    list.addEventListener("pointerdown", (e) => {
      if (e.button !== 0 || busy) return;
      e.preventDefault();
      press = { from: under(e), opened: false, moved: false };
    });
    document.addEventListener("pointermove", track);
    document.addEventListener("pointerup", release);
    document.addEventListener("pointercancel", () => (press = null));
  } else {
    box.addEventListener("click", () => {
      box.focus({ preventScroll: true });
      if (isOpen()) close();
      else open();
    });
    // the list takes no focus from the box, as WinUI's popup takes none from its ComboBox
    list.addEventListener("pointerdown", (e) => e.preventDefault());
    list.addEventListener("click", (e) => {
      const o = (e.target as Element).closest<HTMLElement>('[role="option"]');
      if (o) commit(options.indexOf(o));
    });
  }

  // type-select: the next item whose title starts with what was typed
  function find(key: string): number {
    const now = performance.now();
    typed = now - typedAt < TYPED_MS ? typed + key.toLowerCase() : key.toLowerCase();
    typedAt = now;
    const from = isOpen() ? active : chosen();
    const titles = options.map((o) => o.textContent!.trim().toLowerCase());
    const same = typed.split("").every((c) => c === typed[0]);
    const word = same ? typed[0] : typed;
    for (let step = same ? 1 : 0; step <= options.length; step++) {
      const i = (Math.max(from, 0) + step) % options.length;
      if (titles[i].startsWith(word)) return i;
    }
    return -1;
  }

  const last = options.length - 1;
  const clamp = (i: number) => Math.min(last, Math.max(0, i));

  box.addEventListener("keydown", (e) => {
    if (busy) return e.preventDefault();
    const typing = e.key.length === 1 && e.key !== " " && !e.metaKey && !e.ctrlKey && !e.altKey;
    if (!isOpen()) {
      if (kind === "popup") {
        // full keyboard access: Space opens the pop-up, as the arrows do
        if (e.key === " " || e.key === "ArrowDown" || e.key === "ArrowUp") {
          e.preventDefault();
          open();
        }
        return;
      }
      const opens =
        e.key === "Enter" ||
        e.key === " " ||
        e.key === "F4" ||
        (e.altKey && (e.key === "ArrowDown" || e.key === "ArrowUp"));
      if (opens) {
        e.preventDefault();
        return open();
      }
      // WinUI changes the choice in place, at once
      const to =
        e.key === "ArrowDown"
          ? clamp(chosen() + 1)
          : e.key === "ArrowUp"
            ? clamp(chosen() - 1)
            : e.key === "Home"
              ? 0
              : e.key === "End"
                ? last
                : typing
                  ? find(e.key)
                  : -1;
      if (e.key.startsWith("Arrow") || e.key === "Home" || e.key === "End" || typing)
        e.preventDefault();
      if (to >= 0) take(values[to]);
      return;
    }
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        return activate(active < 0 ? 0 : clamp(active + 1));
      case "ArrowUp":
        e.preventDefault();
        return activate(active < 0 ? last : clamp(active - 1));
      case "Home":
      case "PageUp":
        if (kind === "popup") return;
        e.preventDefault();
        return activate(0);
      case "End":
      case "PageDown":
        if (kind === "popup") return;
        e.preventDefault();
        return activate(last);
      case "Enter":
      case " ":
        e.preventDefault();
        return commit(active);
      case "F4":
        if (kind === "popup") return;
        e.preventDefault();
        return commit(active);
      case "Escape":
        e.preventDefault();
        return close();
      case "Tab":
        return close();
    }
    if (e.altKey && (e.key === "ArrowDown" || e.key === "ArrowUp") && kind === "combobox") {
      e.preventDefault();
      return commit(active);
    }
    if (typing) {
      e.preventDefault();
      const i = find(e.key);
      if (i >= 0) activate(i);
    }
  });

  box.addEventListener("blur", () => {
    if (isOpen() && !busy) close();
  });

  return {
    close() {
      if (isOpen()) close();
    },
    set(v: string) {
      if (v === value) return;
      value = v;
      for (const s of shown) s.hidden = s.dataset.value !== v;
      if (kind === "popup")
        options.forEach((o) => o.setAttribute("aria-checked", String(o.dataset.value === v)));
      if (!isOpen()) activate(chosen());
    },
  };
}
