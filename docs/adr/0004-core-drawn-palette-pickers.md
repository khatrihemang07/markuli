# Draw the Color Picker and Width Picker in the core

Until now a right-click on a Palette slot opened native UI: `NSColorPanel` or `ChooseColor` for a color, an `NSPopover` or a hand-built Win32 popup for a width. That gave no curated colors, behaved differently on each platform, needed about 600 lines of unsafe platform code per OS, and could not be covered by goldens (standard 20: the platform layer is checked by hand).

## Decision

The core draws both pickers, as a popover painted after the Toolbar in the same band composition (ADR-0003). They are icon-only: tabs, Reset, "..." and the low-contrast warning are small vector icons, and a width is read from a live preview stroke. The core therefore gains no text rendering, no fonts and no layout engine.

- Same pixels on macOS and Windows, with golden tests through `render` for every layout (standards 17 and 18).
- The native width editors and both Reset buttons are deleted; the platform layer keeps only the system color panel, behind the "..." icon, for exact values.
- The popover state is `(slot, tab, drag)`; geometry is recomputed on every use, so Toolbar Position, insets, scale and resize can never leave it stale.
- Custom keeps two cached pixel buffers (saturation/value square, hue strip), allocated at the first open and freed when Draw Mode ends. Nothing else is allocated while the pointer moves.

## Considered options

- **Keep the native editors and add presets to them.** Rejected: an `NSColorPanel` accessory view and a `ChooseColor` hook are fragile, and the Windows and macOS looks would keep diverging.
- **Draw text for hex values and pt readouts.** Rejected: it needs a font, glyph rasterization and localization in a core whose budget is 4 MB. The swatch ring, the preview stroke and the system panel cover the same needs.
- **A webview.** Rejected by ADR-0001.

## Consequences

- About 1,000 lines of new core code and about 900 lines of platform code removed. The binary must stay at or under 4 MB; measure it.
- A user who wants an exact hex value goes through "...", so that path stays platform code checked by hand.
- Amber is faint on a white page (2.1:1): the warning badge marks such a color instead of forbidding it.
