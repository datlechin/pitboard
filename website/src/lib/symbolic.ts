// An icon theme's file, as its theme ships it, made a <symbol> as GTK draws it: a symbolic icon's
// every fill and stroke but `none` in the foreground colour, another's in its own, a Breeze
// ColorScheme class giving `currentColor` its colour. Classes and style become attributes, and
// what the page may not carry (style, ids, editors' metadata) goes.

interface Node {
  tag: string;
  attrs: [string, string][];
  children: Node[];
}

const decode = (s: string) =>
  s.replace(
    /&(quot|amp|lt|gt|apos);/g,
    (_, e: string) => ({ quot: '"', amp: "&", lt: "<", gt: ">", apos: "'" })[e]!,
  );

function parse(xml: string): { root: Node; style: string } {
  const top: Node = { tag: "#top", attrs: [], children: [] };
  const open = [top];
  let style = "";
  const tag =
    /<!--[\s\S]*?-->|<\?[\s\S]*?\?>|<!\[CDATA\[([\s\S]*?)\]\]>|<\/([^>\s]+)\s*>|<([^\s/>]+)((?:\s+[^\s=]+\s*=\s*(?:"[^"]*"|'[^']*'))*)\s*(\/?)>|([^<]+)/g;
  for (const m of xml.matchAll(tag)) {
    const [, cdata, close, name, rest, empty, text] = m;
    const parent = open[open.length - 1];
    if (close) open.pop();
    else if (name) {
      const attrs = [...(rest ?? "").matchAll(/([^\s=]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/g)].map(
        (a): [string, string] => [a[1], decode(a[2] ?? a[3])],
      );
      const node = { tag: name, attrs, children: [] };
      parent.children.push(node);
      if (!empty) open.push(node);
    } else if ((cdata ?? text) && parent.tag === "style") style += cdata ?? text;
  }
  const root = top.children.find((n) => n.tag === "svg");
  if (!root) throw new Error("no <svg> in the icon");
  return { root, style };
}

// `.Name { property: value; }`, the only rules the themes' files hold
function rules(css: string): Map<string, [string, string][]> {
  const out = new Map<string, [string, string][]>();
  for (const [, names, body] of css.matchAll(/([^{}]+)\{([^}]*)\}/g)) {
    for (const name of names.split(",").map((n) => n.trim().replace(/^\./, "")))
      out.set(name, [...(out.get(name) ?? []), ...declarations(body)]);
  }
  return out;
}

const declarations = (css: string): [string, string][] =>
  css
    .split(";")
    .map((d) => d.split(":").map((s) => s.trim()))
    .filter((d): d is [string, string] => d.length === 2 && !!d[0] && !!d[1]);

// what a <symbol> has no use for, and the attributes that draw
const DROPPED = new Set(["metadata", "title", "desc", "defs", "style"]);
const DRAWN = new Set(
  "d x y width height rx ry cx cy r x1 y1 x2 y2 points transform opacity fill fill-opacity fill-rule clip-rule stroke stroke-width stroke-linecap stroke-linejoin stroke-opacity".split(
    " ",
  ),
);

function draw(
  node: Node,
  classes: Map<string, [string, string][]>,
  symbolic: boolean,
  color: string | null,
): string | null {
  if (DROPPED.has(node.tag) || node.tag.includes(":")) return null;
  const own = new Map<string, string>();
  for (const [name, value] of node.attrs) if (!name.includes(":")) own.set(name, value);
  const said = new Map<string, string>();
  for (const name of (own.get("class") ?? "").split(/\s+/).filter(Boolean))
    for (const [p, v] of classes.get(name) ?? []) said.set(p, v);
  for (const [p, v] of declarations(own.get("style") ?? "")) said.set(p, v);
  for (const [p, v] of said) own.set(p, v);
  if (own.get("display") === "none") return null;
  const colour = own.get("color") ?? color;
  const attrs: string[] = [];
  for (const [name, value] of own) {
    if (!DRAWN.has(name)) continue;
    let v = value;
    if ((name === "fill" || name === "stroke") && v !== "none")
      v = symbolic ? "currentColor" : v === "currentColor" && colour ? colour : v;
    attrs.push(`${name}="${v.replace(/"/g, "&quot;")}"`);
  }
  const inner = node.children
    .map((c) => draw(c, classes, symbolic, colour))
    .filter((s): s is string => s !== null)
    .join("");
  // an editor's layer with nothing drawn in it
  if (node.tag === "g" && !inner) return null;
  return `<${node.tag}${attrs.length ? " " + attrs.join(" ") : ""}>${inner}</${node.tag}>`;
}

export interface Symbol {
  viewBox: string;
  body: string;
}

export function symbolOf(svg: string, symbolic: boolean): Symbol {
  const { root, style } = parse(svg);
  const classes = rules(style);
  const get = (name: string) => root.attrs.find(([n]) => n === name)?.[1];
  const size = (name: string) => parseFloat(get(name) ?? "16");
  const viewBox = get("viewBox") ?? `0 0 ${size("width")} ${size("height")}`;
  const body = root.children
    .map((c) => draw(c, classes, symbolic, null))
    .filter((s): s is string => s !== null)
    .join("");
  return { viewBox, body };
}

// The icons a figure draws, by theme, from assets/icons/<theme>/<name>.svg: `adwaita-list-add` for
// Adwaita's list-add-symbolic.
export function symbolsOf(files: Record<string, string>): [id: string, Symbol][] {
  return Object.entries(files)
    .map(([path, svg]): [string, Symbol] => {
      const m = /\/([\w-]+)\/([\w.-]+?)(-symbolic)?\.svg$/.exec(path);
      if (!m) throw new Error(`${path} is no theme's icon`);
      return [`${m[1]}-${m[2]}`, symbolOf(svg, !!m[3])];
    })
    .sort(([a], [b]) => a.localeCompare(b));
}
