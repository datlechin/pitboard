import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { test } from "node:test";
import { symbolOf, symbolsOf } from "./symbolic.ts";

const icons = new URL("../assets/icons/", import.meta.url);
const themes = readdirSync(icons, { withFileTypes: true })
  .filter((d) => d.isDirectory())
  .map((d) => d.name);
// the files GTK's figures draw; Plasma's Breeze icons sit by context and size under breeze/
const files = Object.fromEntries(
  themes.flatMap((theme) =>
    readdirSync(new URL(`${theme}/`, icons), { withFileTypes: true })
      .filter((f) => f.isFile())
      .map((f) => [
        `/assets/icons/${theme}/${f.name}`,
        readFileSync(new URL(`${theme}/${f.name}`, icons), "utf8"),
      ]),
  ),
);
const read = (path: string) => files[`/assets/icons/${path}`];

test("a symbolic icon is drawn in the foreground colour, as GTK recolours it", () => {
  const add = symbolOf(read("adwaita/list-add-symbolic.svg"), true);
  assert.equal(add.viewBox, "0 0 16 16");
  assert.match(add.body, /^<path d="m 7 1 v 6 [^"]+" fill="currentColor"><\/path>$/);
  // Breeze's says its colour in a style attribute and a class
  const breeze = symbolOf(read("breeze/list-add-symbolic.svg"), true);
  assert.match(
    breeze.body,
    /^<path d="M 7 3[^"]+" fill="currentColor" fill-opacity="1" stroke="none">/,
  );
  // GTK 4.22's path icons stroke the foreground, and show their initial state
  const error = symbolOf(read("gtk/dialog-error-symbolic.svg"), true);
  assert.equal(error.viewBox, "0 0 16 16");
  assert.equal(error.body.match(/<path/g)?.length, 1);
  assert.match(error.body, /fill="currentColor"/);
  const link = symbolOf(read("libadwaita/adw-external-link-symbolic.svg"), true);
  assert.match(link.body, /stroke="currentColor"/);
  assert.match(link.body, /fill="none"/);
});

test("an icon that is not symbolic keeps its colours, its `color` giving currentColor its own", () => {
  const app = symbolOf(
    readFileSync(
      new URL(
        "../../../apps/linux/data/icons/scalable/apps/com.usepitboard.Pitboard.svg",
        import.meta.url,
      ),
      "utf8",
    ),
    false,
  );
  assert.equal(app.viewBox, "0 0 128 128");
  assert.match(app.body, /fill="#4dd170"/);
  assert.doesNotMatch(app.body, /currentColor/);
  const inked = symbolOf(
    '<svg viewBox="0 0 16 16"><style>.Text { color: #232629; }</style><path class="Text" style="fill:currentColor" d="M0 0h16v16H0z"/></svg>',
    false,
  );
  assert.match(inked.body, /fill="#232629"/);
});

test("every icon becomes a symbol the page can carry", () => {
  const symbols = symbolsOf(files);
  assert.equal(symbols.length, Object.keys(files).filter((f) => f.endsWith(".svg")).length);
  for (const [id, { viewBox, body }] of symbols) {
    assert.match(id, /^(adwaita|breeze|gtk|libadwaita)-[a-z0-9-]+$/);
    assert.match(viewBox, /^[\d. -]+$/, id);
    assert.ok(body.length > 0, id);
    assert.doesNotMatch(body, /\s(style|class|id)=|url\(|:\w+=|<(style|metadata|defs)/, id);
  }
  assert.ok(symbols.some(([id]) => id === "adwaita-list-add"));
  assert.ok(symbols.some(([id]) => id === "libadwaita-adw-external-link"));
});

test("each theme's icons sit beside its licence", () => {
  const licences: Record<string, string> = {
    adwaita: "AdwaitaIconTheme-LICENSE.txt",
    breeze: "BreezeIcons-LICENSE.txt",
    gtk: "GTK-LICENSE.txt",
    libadwaita: "libadwaita-LICENSE.txt",
  };
  assert.deepEqual(themes.sort(), Object.keys(licences).sort());
  for (const file of Object.values(licences)) assert.ok(existsSync(new URL(file, icons)), file);
});
