# Present through two IOSurfaces and never render into the one on screen

The macOS Overlay shows an `IOSurface` as the `contents` of a `CALayer`, and the core draws into that surface's memory. Two bugs came from drawing into the surface that is on screen.

1. **Stale frames.** After each CPU write the presenter assigned the same surface to `contents` again. Core Animation compares the object, not the surface's contents or seed, so an unchanged `contents` is no change: the compositor kept the frame it had. On a real display the first frame appeared, later Strokes never did, and the toolbar stayed on screen after leaving Draw Mode. The first fix called the private `-[CALayer setContentsChanged]`.
2. **Flicker.** The core paints a frame in several passes (backdrop, Ink, toolbar, style panel) straight into the displayed memory. The compositor reads that memory whenever it likes, so while hovering the style panel it now and then caught a frame with the panel not yet painted: the panel vanished for a few ms. Measured with an `SCStream` at up to 120 fps over 3x3 hover passes on a Retina display: 29 glitch frames in about 415 with one surface, 0 in 440 with two. `setContentsChanged` cannot fix this: it says the pixels changed, not that they are complete.

Screenshots hid both. `screencapture` and `SCScreenshotManager` render the window again from the surface memory, so they showed the up-to-date pixels. The compositor's own output is visible only as the frames an `SCStream` is delivered. **A screenshot is not proof that the screen changed.**

## Decision

Always use two surfaces. The core renders into the one that is not shown; `present` sets it as `contents` (a different object every frame, so Core Animation sees the change) and swaps. Before the core renders, the previous frame's damage rectangle is copied from the shown surface into the back one, because the core renders incrementally and assumes the target holds the last frame. The layer's `contents` action is disabled so a swap never cross-fades. The private SPI is gone.

## Cost

One more screen-sized buffer on every machine: 17 MB at 2560x1664, 8 MB at 1080p. Measured Draw Mode footprint on the Retina display went from 29 MB to 45 MB, over the 40 MB drawing budget there (the buffers are freed when Draw Mode ends: 13 MB afterwards). Accepted: a visible flicker is worse than the budget miss on one display size; revisit by rendering into a half-height back buffer or using a tile-based damage copy if it matters.

## Considered options

- **`setContentsChanged` only (the first fix).** Lean, private API, and leaves the flicker.
- **Assign a new `CGImage` per frame.** Core Animation copies each image; the footprint spiked to about 220 MB.
- **Set `contents` to nil and back.** Flickers and costs a full re-composite per frame.
- **Render the whole frame into a scratch buffer, then copy.** Same extra memory, plus a full copy per frame.
- **Metal layer.** Brings the Metal stack into a 1 MB binary for no gain.
