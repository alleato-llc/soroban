# Bundled fonts

These monospace fonts are embedded into the `soroban-gui` binary (via
`include_bytes!`) and offered in Settings → Appearance → Font, so the app renders
identically on every platform.

| Font | Version | License | Upstream |
|------|---------|---------|----------|
| JetBrains Mono | Regular | SIL Open Font License 1.1 — [`JetBrainsMono-OFL.txt`](JetBrainsMono-OFL.txt) | https://github.com/JetBrains/JetBrainsMono |
| Source Code Pro | Regular | SIL Open Font License 1.1 — [`SourceCodePro-OFL.txt`](SourceCodePro-OFL.txt) | https://github.com/adobe-fonts/source-code-pro |

The OFL permits bundling and redistribution provided the license text ships with
the fonts (the `*-OFL.txt` files here) and the fonts aren't sold on their own.
The "System" font option uses the platform's default monospace and bundles
nothing.
