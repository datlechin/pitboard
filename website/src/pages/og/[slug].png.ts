import type { APIRoute, GetStaticPaths } from "astro";
import { ogImages, socialPreview } from "../../lib/og.ts";

export const getStaticPaths = (async () =>
  [...(await ogImages()), await socialPreview()].map(({ slug, png }) => ({
    params: { slug },
    props: { png },
  }))) satisfies GetStaticPaths;

export const GET: APIRoute<{ png: Buffer }> = ({ props }) =>
  new Response(new Uint8Array(props.png), { headers: { "Content-Type": "image/png" } });
