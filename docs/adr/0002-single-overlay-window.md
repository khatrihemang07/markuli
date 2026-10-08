# One Overlay that moves between displays and Clears on move

Penio keeps one Overlay window per display, all alive at all times, and polls every 1.5 s to keep them in sync with the monitors. Markuli keeps a single Overlay. It's created lazily on the display under the cursor when Draw Mode is toggled on, moved (and Cleared) when toggled on a different display, and destroyed when Ink is empty and Draw Mode is off.

> **Amended 2026-10-08:** the Overlay is now destroyed whenever Draw Mode is off, even with Ink. Ink is shown only in Draw Mode (it stays in memory, so entering Draw Mode again on the same display shows it again with its undo history). This removes the click-through Overlay and its idle cost. Moving to another display still Clears. This keeps memory to one screen-sized pixel buffer at most and removes all polling. Display changes are picked up at the next toggle.

## Considered options

- **One Overlay per display.** Rejected because it multiplies memory and needs monitor tracking.
- **Ink travels with the Overlay.** Rejected because annotations would point at the wrong content.
- **Per-display Ink kept while away.** Rejected because it adds state for a rare case.

## Consequences

- You can't keep annotations on two displays at the same time.
