# Tell Core Animation the IOSurface changed (private `setContentsChanged`), with a double-buffered fallback

The macOS Overlay shows an `IOSurface` as the `contents` of a `CALayer`, and the core draws into that surface's memory. After each CPU write the presenter assigned the same surface to `contents` again. Core Animation compares the object, not the surface's contents or seed, so an unchanged `contents` is no change: the compositor kept the frame it had. On a real display the first frame appeared, later Strokes never did, and the toolbar stayed on screen after leaving Draw Mode.

Screenshots hid this. `screencapture` and `SCScreenshotManager` render the window again from the surface memory, so they showed the up-to-date pixels while the display showed the old frame. Every screenshot-based check passed. The compositor's own output is visible only as the frames an `SCStream` is delivered (it emits a frame, with dirty rectangles, only when the display is re-composited): with the bug, drawing Strokes delivered no frames at all, and the latest delivered frame was the stale one. The QA tool lives outside the repo; the rule is what matters: **a screenshot is not proof that the screen changed.** The release checklist says to look with real eyes.

## Decision

Call `-[CALayer setContentsChanged]` after assigning `contents`, inside the same transaction. It is the call WebKit and Chromium use for `IOSurface`-backed layers. It is private API (SPI), so:

- guard it with `respondsToSelector:` once, when the Overlay is created;
- when it is missing, alternate two surfaces so `contents` really changes on every frame. Before the core renders into the back surface, the previous frame's damage rectangle is copied into it from the shown surface, because the core renders incrementally and assumes the target holds the last frame. The second surface costs one more screen-sized buffer (about 8 MB at 1920x1080), and only on that path.

`MARKULI_PRESENT_FALLBACK` (only in builds with the `dev-hooks` feature) forces the fallback, so it can be tested on a machine that has the SPI.

## Considered options

- **Always double-buffer.** Public API only, but one more screen-sized buffer on every machine (8 MB at 1080p, 17 MB at 2560x1664) against the idle budget, and a per-frame damage copy.
- **Assign a new `CGImage` per frame.** The first presenter did this: Core Animation copies each image, and the footprint spiked to about 220 MB.
- **Set `contents` to nil and back.** Works, but flickers and costs a full re-composite per frame.
- **Metal layer.** Brings the Metal stack into a 1 MB binary for no gain.

## Consequences

- If a future macOS removes the SPI, the fallback takes over with no code change, at the cost above.
- The SPI is not App Store safe. Markuli is distributed unsigned and outside the store.
- The fallback and the SPI path are both checked by hand on a real display (checklist section 2).
