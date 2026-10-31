// The systems the site draws, in the order it names them; head.js chooses one for each reader.
export type Platform = "macos" | "windows" | "linux";
export const platforms: Platform[] = ["macos", "windows", "linux"];
export const platformNames: Record<Platform, string> = {
  macos: "macOS",
  windows: "Windows",
  linux: "Linux",
};

// The Linux desk a page shows, GNOME unless the reader chose KDE Plasma.
export type Desktop = "gnome" | "kde";
export const desktops: Desktop[] = ["gnome", "kde"];
export const desktopNames: Record<Desktop, string> = { gnome: "GNOME", kde: "KDE Plasma" };

// What a drawing draws and a part of a page says: a platform, Linux by its desktop.
export type System = Exclude<Platform, "linux"> | Desktop;
export const systems: System[] = ["macos", "windows", "gnome", "kde"];
export const systemNames: Record<System, string> = {
  macos: "macOS",
  windows: "Windows",
  ...desktopNames,
};

// What words an app says, by the system that says them, as crates/pitboard-model/src/front.rs
// names its fronts: one libadwaita app, GNOME's, on every Linux desktop.
export type Front = Extract<System, "macos" | "windows" | "gnome">;
export const fronts: Front[] = ["macos", "windows", "gnome"];
export const frontOf = (system: System): Front => (system === "kde" ? "gnome" : system);

export const platformOf = (system: System): Platform =>
  system === "macos" || system === "windows" ? system : "linux";

// The system a live drawing draws: its own, or for a drawing both desktops share the one shown.
export function systemOf(el: Element): System {
  const drawn = el.closest<HTMLElement>("[data-drawing]")?.dataset.drawing?.split(" ")[1];
  if (drawn === "linux")
    return el.ownerDocument.documentElement.dataset.desktop === "kde" ? "kde" : "gnome";
  if (!systems.includes(drawn as System)) throw new Error(`no drawing names "${drawn}"`);
  return drawn as System;
}

// The systems a prop names, in the site's order: `linux` stands for both desktops.
export function systemsOf(on: readonly (Platform | Desktop)[]): System[] {
  return systems.filter((s) => on.includes(s) || on.includes(platformOf(s)));
}

// A `data-on` value: global.css hides a part of a page, a drawing or a caption from every reader
// whose system it does not list.
export const onOf = (on: readonly (Platform | Desktop)[]) => systemsOf(on).join(" ");

// What a page without JavaScript says above a part for some systems: "On macOS and Windows:".
export function labelOf(on: readonly (Platform | Desktop)[]): string {
  const shown = systemsOf(on);
  if (shown.length === 0) throw new Error("a part of a page shows to no system");
  const names = [
    ...shown.filter((s) => s === "macos" || s === "windows").map((s) => systemNames[s]),
    ...(desktops.every((d) => shown.includes(d))
      ? [platformNames.linux]
      : desktops.filter((d) => shown.includes(d)).map((d) => desktopNames[d])),
  ];
  const last = names.pop()!;
  return `On ${names.length ? `${names.join(", ")} and ${last}` : last}:`;
}

// The systems each of a figure's drawings shows to. A figure draws each system its section shows
// to and no other, macOS among them, whose drawing a page without JavaScript shows alone.
export function drawingsOf(
  drawn: readonly (Platform | Desktop)[],
  on: readonly System[] = systems,
): System[][] {
  const each = drawn.map((d) => systemsOf([d]));
  const all = each.flat();
  for (const s of systems) {
    const times = all.filter((x) => x === s).length;
    if (times > 1) throw new Error(`a figure draws ${systemNames[s]} twice`);
    if (times === 0 && on.includes(s))
      throw new Error(`a figure shown on ${systemNames[s]} has no drawing for it`);
    if (times === 1 && !on.includes(s))
      throw new Error(`a figure draws ${systemNames[s]}, which its section does not show to`);
  }
  if (!all.includes("macos")) throw new Error("a figure has no macOS drawing");
  return each;
}

// Each of a figure's words, with the systems whose drawing they describe: Linux's describe both
// desktops, and each desktop may have its own instead. Given what is drawn, each system drawn needs
// words and no other has any; given nothing, as for a terminal, macOS's describe every system with
// none of its own.
const keys = ["macos", "windows", "linux", "gnome", "kde"] as const;
export function wordsOf<T>(
  words: Partial<Record<Platform | Desktop, T>>,
  drawn?: readonly System[],
): [System[], T][] {
  if (words.macos === undefined) throw new Error("a figure has no words for macOS");
  if (words.linux !== undefined && desktops.some((d) => words[d] !== undefined))
    throw new Error("a figure has words for Linux and for one of its desktops");
  const each = keys.flatMap((k): [System[], T][] => {
    const w = words[k];
    return w === undefined ? [] : [[systemsOf([k]), w]];
  });
  const described = each.flatMap(([on]) => on);
  if (drawn === undefined) {
    const rest = systems.filter((s) => !described.includes(s));
    return each.map(([on, w]) => [on.includes("macos") ? [...on, ...rest] : on, w]);
  }
  for (const s of systems) {
    if (drawn.includes(s) && !described.includes(s))
      throw new Error(`a figure's ${systemNames[s]} drawing has no words of its own`);
    if (!drawn.includes(s) && described.includes(s))
      throw new Error(`a figure has words for a ${systemNames[s]} drawing it does not have`);
  }
  return each;
}
