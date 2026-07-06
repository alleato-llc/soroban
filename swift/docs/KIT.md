# BinaryEditorKit

The shared bit-editor UI component — the macOS-Calculator-style bit grid, the
bit-field format decode/encode, and the visual builder — factored into its own
SwiftPM package so both the calculator and the standalone Tama app use one
component instead of duplicating it.

> **Status:** skeleton — full content lands in Phase 2. The bit-field *format
> model* (numeric / flags / enum / reserved / unused fields, the `Bits` module)
> is language-shared and documented in [../../docs/PROGRAMMER.md](../../docs/PROGRAMMER.md);
> this page covers the Swift *UI* component specifically.

## See also

- [APP.md](APP.md) · [ARCHITECTURE.md](ARCHITECTURE.md).
- [../../docs/PROGRAMMER.md](../../docs/PROGRAMMER.md) — the shared bit-field format model.
