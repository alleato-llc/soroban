import { defineCollection } from "astro:content";
import { glob } from "astro/loaders";

/** The Anzan language spec is rendered from the repo's CANONICAL copy
 *  (docs/ANZAN.md) at build time — the site never carries a second copy
 *  that could drift from the engine. The glob loader reads the file with
 *  plain fs, so living outside the site's Vite root is fine. */
const docs = defineCollection({
  loader: glob({ pattern: "ANZAN.md", base: "../docs" }),
});

export const collections = { docs };
