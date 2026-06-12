# Soroban landing page

The marketing site for [Soroban・算盤](https://github.com/alleato-llc/soroban)
— two static pages: the landing page (hero, a faux app window, twelve
feature cards, open source + donations, footer) and `/anzan`, the language
spec rendered from the repo's canonical `docs/ANZAN.md` at build time.

**Stack**: Astro 5 + Preact + TypeScript (strict), static output. One Preact
island (the theme toggle); everything else is plain HTML/CSS. System font
stacks, no webfonts, no trackers.

## Commands

```sh
npm install
npm run dev        # local dev server with HMR
npm run build      # → dist/ (deploy anywhere static)
npm run preview    # serve dist/ locally
```

## Layout

```
src/
  pages/index.astro        the landing page; `links` const at the top holds
                           every external URL (download/source/donate)
  pages/anzan.astro        /anzan — renders the Anzan language spec
  content.config.ts        loads ../docs/ANZAN.md via the glob loader (plain
                           fs, so living outside the Vite root is fine) —
                           the site NEVER carries a copy of the spec
  layouts/Layout.astro     <head>, header/footer, and the PRE-PAINT theme
                           script (see Theming)
  components/
    ThemeToggle.tsx        island: light/dark override, persisted
    Card.astro             one feature card; `scopes={["log"|"grid"|"cli"]}`
                           renders the upper-right badge saying where the
                           feature lives (all three → "everywhere"; omit the
                           prop for app-level features like themes)
  styles/global.css        all styling; CSS custom properties up top
public/favicon.svg         one abacus rod (frame, beam, beads)
public/spec.html           the Living Specification (generated — see below)
public/report.html         the interactive test report (generated)
```

## The Living Specification (`/spec.html`, `/report.html`)

The **Verified** nav link points at `/spec.html` — the engine's Gherkin
suite rendered by PickleKit's `ReportSuite` as a *living specification*:
every behavior as verified Given/When/Then prose, with the full
interactive test report one click deeper at `/report.html` (they
cross-link). Both use the same Solarized/Dracula palette as the site, so
they read as one product.

These two files are **generated, not hand-written** — regenerate after
behavior changes with:

```sh
scripts/generate-living-spec.sh   # runs the Gherkin suite → site/public/
```

They're committed as a static snapshot so the site deploys with them; CI
will refresh them on release. (Build-time generation isn't wired yet —
the snapshot is the source of truth until it is.)

## Theming (the contract)

The palette custom properties in `global.css` are **lifted verbatim from
the app's own theme JSONs** (`App/Resources/Themes/*.json` — currently
Solarized Light / Dracula) so the site renders in the app's design
language. If you swap palettes, copy hex values from a theme JSON and note
which one in the comment.

Resolution order, implemented by the inline script in `Layout.astro` +
`ThemeToggle`:

1. a stored choice (`localStorage["soroban-theme"]`) wins;
2. otherwise the system (`prefers-color-scheme`), **followed live** until
   the user picks;
3. `data-theme` is set on `<html>` *before first paint* — no flash.

Code blocks on `/anzan` follow the same pair: shiki renders both
Solarized Light and Dracula (`astro.config.mjs`, `defaultColor: false`),
and `global.css` picks per `data-theme` via the `--shiki-*` variables.

## The hero is a theme-matched screenshot carousel

The hero auto-cycles real app screenshots from `public/screenshots/`, one set
per theme (dark shots in dark mode, light in light mode — `.shot-dark` /
`.shot-light` keyed on `data-theme`). Single-click pauses/plays, double-click
maximizes into a lightbox (Esc / click-away / double-click exits); the carousel
script lives inline in `index.astro`, styles in `global.css` (`.hero-shot`,
`.shots`, `.dots`, `.lightbox`). Each theme's set leads with the simple-calc
log shot. **When the app's look changes, retake the screenshots** (same window
size, both themes) so the hero doesn't drift.

It's mobile-safe: `html { overflow-x: clip }` absorbs the hero-shot breakout
(which only applies at `min-width: 940px`), and a `max-width: 600px` query drops
the header nav to its own row.

## Conventions

- Site-only commits use `[skip ci]` — every push to main otherwise cuts a
  release (see ../docs/RELEASING.md), and a copy tweak shouldn't spend a
  version number.
- The Download button points at
  `https://github.com/alleato-llc/soroban/releases/latest/download/Soroban.dmg`
  — the versionless asset on the latest GitHub Release (stable across versions).
- Donations: Buy Me a Coffee (`buymeacoffee.com/alleato`).

## Deploying

`npm run build` produces a fully static `dist/`. In practice it's deployed by
the **`deploy-site.yml`** workflow: regenerate the living spec/report, build,
and publish `dist/` to the static host (`soroban.alleato.dev`). Deploy
credentials are repo variables/secrets and the hosting infra is provisioned out
of band — the output is plain static, so any host would serve it.
