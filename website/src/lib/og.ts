// Each page's link preview image, drawn once per build and named by a hash of its bytes, so a
// changed image gets a new address that every platform fetches again.

import { createHash } from "node:crypto";
import interBold from "../assets/fonts/Inter-4.1-700.ttf?inline";
import inter from "../assets/fonts/Inter-4.1-400.ttf?inline";
import monoBold from "../assets/fonts/JetBrainsMono-2.304-700.ttf?inline";
import mono from "../assets/fonts/JetBrainsMono-2.304-500.ttf?inline";
import lockup from "../assets/logo/lockup-color.svg?raw";
import template from "../assets/logo/template.svg?raw";
import css from "../styles/global.css?raw";
import { GITHUB, cards, describe, render, type Assets, type Card } from "./card.ts";

export type CardName = keyof typeof cards;

export interface OgImage {
  name: CardName;
  slug: string;
  path: string;
  png: Buffer;
  alt: string;
}

const bytes = (dataUrl: string) => Buffer.from(dataUrl.slice(dataUrl.indexOf(",") + 1), "base64");

const assets: Assets = {
  css,
  lockup,
  template,
  fonts: [
    { name: "Inter", data: bytes(inter), weight: 400 },
    { name: "Inter", data: bytes(interBold), weight: 700 },
    { name: "JetBrains Mono", data: bytes(mono), weight: 500 },
    { name: "JetBrains Mono", data: bytes(monoBold), weight: 700 },
  ],
};

const slug = (name: string, png: Buffer) =>
  `${name}-${createHash("sha256").update(png).digest("hex").slice(0, 8)}`;

let drawing: Promise<OgImage[]> | null = null;

export function ogImages(): Promise<OgImage[]> {
  drawing ??= Promise.all(
    (Object.entries(cards) as [CardName, Card][]).map(async ([name, card]) => {
      const png = await render(card, assets);
      const named = slug(name, png);
      return { name, slug: named, path: `/og/${named}.png`, png, alt: describe(card) };
    }),
  );
  return drawing;
}

// The repository's social preview, which GitHub takes only as an upload in its settings
export async function socialPreview(): Promise<{ slug: string; png: Buffer }> {
  const png = await render(cards.home, assets, GITHUB);
  return { slug: slug("github", png), png };
}

export async function ogImage(name: CardName): Promise<OgImage> {
  return (await ogImages()).find((image) => image.name === name)!;
}
