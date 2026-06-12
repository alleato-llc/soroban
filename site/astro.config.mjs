// @ts-check
import { defineConfig } from "astro/config";
import preact from "@astrojs/preact";

export default defineConfig({
  integrations: [preact()],
  // Emit flat files (anzan.html, not anzan/index.html) so extensionless URLs
  // resolve under the host's edge routing function, which maps `/anzan` →
  // `/anzan.html` (it appends `.html`, not `/index.html`). Without this the
  // host has no `anzan.html` object to serve.
  build: { format: "file" },
  markdown: {
    // The spec page's code blocks follow the site's two themes exactly —
    // shiki ships the same Solarized Light / Dracula palettes the app uses.
    // defaultColor:false emits only --shiki-light/--shiki-dark vars; the
    // theme-switching rules live in global.css under `.astro-code`.
    shikiConfig: {
      themes: { light: "solarized-light", dark: "dracula" },
      defaultColor: false,
    },
  },
});
