// A Breeze icon as Plasma and a Qt app draw it (assets/icons/breeze/<context>/<size>,
// BreezeIcons-LICENSE.txt), inline with no stylesheet or style attribute, which CSP and the guard
// refuse. KIconLoader puts the theme's colours in an icon's own stylesheet, so ColorScheme-Text
// draws in currentColor and another class in its colour, where GTK, which lib/symbolic.ts follows,
// inks a symbolic icon whole. A full-colour icon keeps its gradients, under ids of its drawing's own.

export interface Drawn {
  viewBox: string;
  body: string;
}

// what a style attribute may say as an SVG presentation attribute
const PRESENTATION = new Set([
  "clip-rule",
  "color",
  "display",
  "fill",
  "fill-opacity",
  "fill-rule",
  "opacity",
  "stop-color",
  "stop-opacity",
  "stroke",
  "stroke-dasharray",
  "stroke-dashoffset",
  "stroke-linecap",
  "stroke-linejoin",
  "stroke-miterlimit",
  "stroke-opacity",
  "stroke-width",
  "visibility",
]);

const attrPattern = /\s([\w:-]+)="([^"]*)"/g;

function attrsOf(tag: string): [string, string][] {
  return [...tag.matchAll(attrPattern)].map((m) => [m[1], m[2]]);
}

// `key` names this drawing of the icon, so the ids it keeps are the page's own
export function breeze(svg: string, key: string): Drawn {
  const source = svg
    .replace(/<\?xml[\s\S]*?\?>/g, "")
    .replace(/<!DOCTYPE[^>]*>/gi, "")
    .replace(/<!--[\s\S]*?-->/g, "");
  const root = /<svg\b([^>]*)>([\s\S]*)<\/svg>/.exec(source);
  if (!root) throw new Error(`${key}: not an SVG`);
  const rootAttrs = new Map(attrsOf(` ${root[1]}`));
  const viewBox =
    rootAttrs.get("viewBox") ?? `0 0 ${rootAttrs.get("width")} ${rootAttrs.get("height")}`;
  if (/undefined/.test(viewBox)) throw new Error(`${key}: no viewBox or size`);

  // the colour each ColorScheme class has in the icon's own stylesheet
  const scheme = new Map<string, string>();
  for (const m of source.matchAll(/\.(ColorScheme-[\w-]+)\s*\{[^}]*?color:\s*([^;}\s]+)/g))
    scheme.set(m[1], m[2]);

  let body = root[2]
    .replace(/<style\b[\s\S]*?<\/style>/g, "")
    .replace(/<metadata\b[\s\S]*?<\/metadata>/g, "")
    .replace(/<sodipodi:namedview\b[^>]*?\/>/g, "")
    .replace(/<sodipodi:namedview\b[\s\S]*?<\/sodipodi:namedview>/g, "")
    .replace(/<defs\b[^>]*>\s*<\/defs>/g, "")
    .replace(/<defs\b[^>]*\/>/g, "");

  const referenced = new Set(
    [...body.matchAll(/url\(#([^)]+)\)|href="#([^"]+)"/g)].map((m) => m[1] ?? m[2]),
  );

  body = body.replace(/<([\w:-]+)\b([^>]*?)(\/?)>/g, (_whole, tag: string, rest: string, close) => {
    const attrs = new Map<string, string>();
    for (const [name, value] of attrsOf(` ${rest}`)) {
      if (name.includes(":") && name !== "xlink:href") continue;
      attrs.set(name === "xlink:href" ? "href" : name, value);
    }
    const style = attrs.get("style");
    attrs.delete("style");
    for (const declaration of (style ?? "").split(";")) {
      const [prop, ...value] = declaration.split(":");
      const name = prop?.trim();
      if (name && PRESENTATION.has(name)) attrs.set(name, value.join(":").trim());
    }
    const cls = attrs.get("class");
    attrs.delete("class");
    for (const c of (cls ?? "").split(/\s+/)) {
      if (c.startsWith("ColorScheme-") && c !== "ColorScheme-Text" && scheme.has(c))
        attrs.set("color", scheme.get(c)!);
    }
    const id = attrs.get("id");
    if (id !== undefined) {
      if (referenced.has(id)) attrs.set("id", `${key}-${id}`);
      else attrs.delete("id");
    }
    for (const [name, value] of attrs) {
      if (name === "href" && value.startsWith("#")) attrs.set(name, `#${key}-${value.slice(1)}`);
      else if (value.includes("url(#"))
        attrs.set(name, value.replace(/url\(#([^)]+)\)/g, `url(#${key}-$1)`));
    }
    const written = [...attrs].map(([n, v]) => ` ${n}="${v}"`).join("");
    return `<${tag}${written}${close}>`;
  });

  body = body.replace(/>\s+</g, "><").trim();
  if (/\sstyle=|<style/.test(body)) throw new Error(`${key}: a style is left`);
  return { viewBox, body };
}
