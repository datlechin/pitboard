// What a live figure writes into the markup Astro drew (plan 2.4, rule 2): an attribute, text the
// model owns, words.ts's HTML, the order of nodes, and clones of the templates it drew. Each writes
// only what changed, so a figure redrawn every second keeps focus and selection.

// text, kept between the white space Astro's source lays it out in, so the same words are the same
// markup
export function text(el: Element | null | undefined, words: string) {
  const was = el?.textContent ?? "";
  if (!el || was.trim() === words) return;
  const node = el.childNodes.length === 1 ? el.firstChild : null;
  if (node instanceof Text) {
    const [, lead, , trail] = /^(\s*)([\s\S]*?)(\s*)$/.exec(was)!;
    node.data = lead + words + trail;
  } else el.textContent = words;
}

export function attr(el: Element | null | undefined, name: string, value: string | null) {
  if (!el || el.getAttribute(name) === value) return;
  if (value === null) el.removeAttribute(name);
  else el.setAttribute(name, value);
}

export function shown(el: Element | null | undefined, on: boolean) {
  attr(el, "hidden", on ? null : "");
}

const written = new WeakMap<Element, string>();

// words.ts's HTML, which escapes every value of the state it puts in
export function html(el: Element | null | undefined, markup: string) {
  if (!el || written.get(el) === markup) return;
  if (el.innerHTML !== markup) el.innerHTML = markup;
  written.set(el, markup);
}

// `el` as the `index`th child of `parent`, moved only when it is elsewhere
export function placeAt(parent: Element, el: Element, index: number) {
  const at = parent.children[index] ?? null;
  if (at !== el) parent.insertBefore(el, at);
}

// a copy of a template's first element
export function cloned<T extends Element = HTMLElement>(template: HTMLTemplateElement | null): T {
  if (!template?.content.firstElementChild) throw new Error("a figure lacks a template it needs");
  return template.content.firstElementChild.cloneNode(true) as T;
}

// One node per id, in the order given, after the `offset` nodes Astro drew ahead of them: a node
// whose account has gone is hidden, and one for an account added is made from `template`.
export function keyed(
  parent: Element,
  ids: string[],
  offset: number,
  template: HTMLTemplateElement | null,
  made?: (node: HTMLElement, id: string) => void,
): HTMLElement[] {
  const have = new Map(
    [...parent.querySelectorAll<HTMLElement>(":scope > [data-account]")].map((n) => [
      n.dataset.account!,
      n,
    ]),
  );
  const nodes = ids.map((id, i) => {
    let node = have.get(id);
    if (!node) {
      node = cloned(template);
      node.dataset.account = id;
      made?.(node, id);
    }
    placeAt(parent, node, offset + i);
    shown(node, true);
    return node;
  });
  for (const [id, node] of have) if (!ids.includes(id)) shown(node, false);
  return nodes;
}
