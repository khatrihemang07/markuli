# Compose each damaged region off-screen, copy it once into one IOSurface

The macOS Overlay shows an `IOSurface` as the `contents` of a `CALayer`, and the core draws into that surface's memory. Two bugs came from that.

1. **Stale frames.** After each CPU write the presenter assigned the same surface to `contents` again. Core Animation compares the object, not the surface's contents or seed, so an unchanged `contents` is no change: the compositor kept the frame it had. Fix: call the private `-[CALayer setContentsChanged]` after assigning `contents`, guarded by `respondsToSelector:`. Where it is missing, two surfaces alternate (the only case with a second buffer; `MARKULI_PRESENT_FALLBACK` in a dev-hooks build forces it).
2. **Flicker on the style panel and toolbar.** Root cause, measured: the core painted a frame in passes straight into the displayed memory. For a hover repaint the first pass (`render_ink`) wrote the backdrop and the Ink over the whole damaged region, and only then the panel (or toolbar) was painted on top. The compositor reads the surface whenever it likes, so a sample between the two passes showed the region without the panel: the panel blinked for a few ms. Two things were ruled out with an instrumented build (Retina, 3361 pointer moves over the panel and toolbar): hover does *not* repaint on every move (the Look compares the hovered control, so moves inside one swatch or button give no damage: 711 renders and presents for 3361 moves, a core test pins it), and each render is a single present. What multiplies the exposure is that each of those 711 renders was two writes of the same pixels (clear, then panel/toolbar) into the displayed surface, 55 of 718 SCStream frames caught the intermediate state.

The first fix for 2 (commit 07fc49a) always double-buffered, rendering into the surface that is not on screen. It worked (0 glitch frames) but cost a second full-screen surface: 45-47 MB in Draw Mode on a 2560x1664 display, over the 40 MB drawing budget.

## Decision

Fix the cause, not the symptom: never expose a partial frame.

- The core composes every layer of a damaged region (backdrop, Ink, Laser, Selection overlay, toolbar, style panel) in one small scratch pixmap sized to the damage (at most 1 MB per band, grown once and reused, no per-move allocation), then copies the finished band into the target in one pass. Every target pixel is written once, with its final value; a pixel that does not change is written with the same bytes. A compositor sample can therefore never see "panel not painted yet". Chrome and overlay drawing shifts into the band through a small `Canvas` (a transform), so the drawing code is unchanged.
- The core reports no damage, and the platform no present, when nothing visible changed (hover target unchanged, no Stroke in progress).
- The presenter goes back to one surface plus `setContentsChanged`, with the two-surface path only as the fallback when the SPI is missing.

## Measured (Retina 2560x1664, SCStream at 120 fps, 21 sample pixels in the panel and toolbar padding, 54 hover passes over the panel and toolbar)

| Build | Glitch frames | Draw Mode footprint |
|---|---|---|
| One surface, painted in passes (before) | 55 of 718 | 29 MB |
| Two surfaces always (07fc49a) | 0 of 723 | 47 MB |
| One surface, composed off-screen, copied once (this ADR) | 0 of 717 | 30 MB |

## Considered options

- **Always two surfaces.** Correct, but +17 MB at 2560x1664 (see above).
- **Assign a new `CGImage` per frame.** Core Animation copies each image; the footprint spiked to about 220 MB.
- **Set `contents` to nil and back.** Flickers and costs a full re-composite per frame.
- **Metal layer.** Brings the Metal stack into a 1 MB binary for no gain.

## Consequences

- A full redraw (Clear, undo, entering Draw Mode) is composed and copied in bands of at most 1 MB, so it can show as a top-to-bottom update, never as a cleared frame without the toolbar. Each band contains all layers.
- Rasterisation depends on the band origin only at edge-coincident coordinates (the opacity digits golden changed by one anti-aliasing sample and was regenerated).
- Screenshots hid both bugs: `screencapture` and `SCScreenshotManager` render from the surface memory. Only the frames an `SCStream` delivers show what the compositor showed. **A screenshot is not proof that the screen changed.**
