import type { APIRoute } from "astro";
import { ogImage } from "../lib/og.ts";

// Links made before each card was named by its hash keep a preview.
export const GET: APIRoute = async () =>
  new Response(new Uint8Array((await ogImage("home")).png), {
    headers: { "Content-Type": "image/png" },
  });
