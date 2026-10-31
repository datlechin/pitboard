// A custom SF Symbol of the macOS app's asset catalog drawn as a <symbol>: its Regular-M paths
// between their margins, over the 16 unit box template.svg draws the mark in, which
// pitboard.symbolset's own Regular-M lies over, and its secondary layer dimmed as hierarchical
// rendering dims it.

export interface Drawn {
  viewBox: string;
  paths: { d: string; secondary: boolean }[];
}

const margin = (svg: string, side: "left" | "right") => {
  const found = new RegExp(`id="${side}-margin-Regular-M"[^>]*\\sx1="([-\\d.]+)"`).exec(svg);
  if (!found) throw new Error(`the symbol has no ${side} margin for Regular-M`);
  return Number(found[1]);
};

export function regularM(svg: string): { width: number; paths: Drawn["paths"] } {
  const group = /<g id="Regular-M"[^>]*>([\s\S]*?)<\/g>/.exec(svg)?.[1];
  if (!group) throw new Error("the symbol has no Regular-M");
  const paths = [...group.matchAll(/<path\b[^>]*\sclass="([^"]*)"[^>]*\sd="([^"]+)"/g)].map(
    ([, kind, d]) => ({ d, secondary: kind.includes("secondary") }),
  );
  return { width: margin(svg, "right") - margin(svg, "left"), paths };
}

// the points of a path of absolute M, L and C commands, each `x y`
export const points = (d: string): [number, number][] => {
  const n = [...d.matchAll(/-?\d+(?:\.\d+)?/g)].map(Number);
  return Array.from({ length: n.length / 2 }, (_, i) => [n[2 * i], n[2 * i + 1]]);
};

// `symbol` drawn where `plain`, the mark's own symbol, lies over `template`'s first path: the
// same scale across its margins, and its top where the template's is
export function sfSymbol(symbol: string, plain: string, template: string): Drawn {
  const mark = regularM(plain);
  const board = /<path\b[^>]*\sd="([^"]+)"/.exec(template)?.[1];
  if (!board) throw new Error("template.svg has no path");
  const unit = mark.width / 16;
  const top = (d: string) => Math.min(...points(d).map(([, y]) => y));
  const y = top(mark.paths[0].d) - top(board) * unit;
  const drawn = regularM(symbol);
  return { viewBox: `0 ${y} ${drawn.width} ${drawn.width}`, paths: drawn.paths };
}
