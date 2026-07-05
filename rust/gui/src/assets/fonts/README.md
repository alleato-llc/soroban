# Bundled fonts

These monospace fonts are embedded into the `soroban-gui` binary (via
`include_bytes!`) and offered in Settings → Appearance → Font, so the app renders
identically on every platform.

| Font | Version | License | Upstream |
|------|---------|---------|----------|
| JetBrains Mono | Regular | SIL Open Font License 1.1 — [`JetBrainsMono-OFL.txt`](JetBrainsMono-OFL.txt) | https://github.com/JetBrains/JetBrainsMono |
| Source Code Pro | Regular | SIL Open Font License 1.1 — [`SourceCodePro-OFL.txt`](SourceCodePro-OFL.txt) | https://github.com/adobe-fonts/source-code-pro |
| IBM Plex Mono | Regular | SIL Open Font License 1.1 — [`IBMPlexMono-OFL.txt`](IBMPlexMono-OFL.txt) | https://github.com/IBM/plex |
| Hack | Regular | MIT + Bitstream Vera — [`Hack-LICENSE.md`](Hack-LICENSE.md) | https://github.com/source-foundry/Hack |
| Fira Mono | Regular | SIL Open Font License 1.1 — [`FiraMono-OFL.txt`](FiraMono-OFL.txt) | https://github.com/mozilla/Fira |

The OFL permits bundling and redistribution provided the license text ships with
the fonts (the `*-OFL.txt` / `*-LICENSE.md` files here) and the fonts aren't sold
on their own; Hack's MIT-style license is equally permissive.

Beyond these bundled families, Settings → Appearance also offers a curated list
of well-known **system** monospace fonts per platform (Menlo/SF Mono/Monaco on
macOS, Consolas/Cascadia on Windows, DejaVu/Liberation on Linux). Those aren't
bundled — iced's font database loads the OS's installed fonts, so they resolve
by name when present and fall back to the default monospace when absent. The
"System" option uses the platform default and bundles nothing.
