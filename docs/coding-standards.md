# Coding standards

Markuli's two goals are **lean and fast** and **easy to change**. Every rule below serves one of them. When a rule and a measured result disagree, the measurement wins: record why in a comment or an ADR.

## Architecture

1. **Core and platform are separate.**
   - The annotator core (Ink, Tools, Draw Mode state, rendering to a pixel buffer, Excalidraw JSON) is pure Rust. It never imports winit, the OS crates, the clipboard, the hotkey or tray crates, or the filesystem.
   - The platform layer only translates OS events into core events and core output into OS calls. If logic can live in the core, it belongs in the core.
2. **One way into the core.** The core exposes one entry point: events in; Ink, view state, render and copy text out. Don't add side doors for tests or the platform layer. If a test needs one, the interface is wrong. Read access to Ink and Selection is part of the output; test-only layout locators are allowed only behind a compile-time feature (`test-support`) that release builds do not enable.
3. **Deep modules, small interfaces.**
   - Each module owns one concept from `CONTEXT.md`: freehand, laser, ink, render, toolbar UI, clipboard, config.
   - Keep module internals private (`pub(crate)` at most), and export the fewest types possible.
4. **Platform code is isolated per OS.**
   - Put OS-specific code in one module per OS behind a shared trait or function set.
   - Use `#[cfg(target_os = ...)]` only at that boundary, never scattered through the code.
5. **New Tools are added, not threaded through.** A Tool is a type implementing the tool interface (pointer down/move/up, key, cursor). Adding a Tool must not require editing the other Tools.

## Performance rules (the "lean" contract)

6. **Event-driven only.** No polling loops, no `sleep`, no busy timers. The event loop waits. Timed frames are allowed only while something is animating (the Laser) and must stop when it finishes.
7. **No work when nothing changed.**
   - Redraw only the damaged region.
   - Compute a stroke outline once, when the Stroke is committed, and cache it.
8. **Allocate deliberately.**
   - No per-frame allocations on the hot path (pointer move, render). Reuse buffers.
   - Use `f32` for geometry.
   - Undo uses an operation log, not snapshots.
9. **Every dependency needs a reason.**
   - Adding a crate requires a one-line justification in the PR. Prefer a short hand-written module (under ~100 lines) over a crate that pulls in a dependency tree.
   - Turn off default features.
   - Never use serde, tokio, or a UI toolkit.
10. **Size budget.** The release artifact must stay at or under 4 MB, RAM idle at or under 10 MB, and RAM while drawing at or under 40 MB. A PR that regresses any of these must say so and why.

## Code style

11. **`rustfmt` and `clippy -D warnings`** (pedantic, opt-out per item with a reason) must pass.
12. **Names come from `CONTEXT.md`.**
    - Write `Ink`, `Element`, `Overlay`, `DrawMode`, `Clear`, `Laser`, never `scene`, `canvas`, `board` or `wipe`.
    - A new concept goes into the glossary before it goes into the code.
13. **No `unwrap`/`expect` in the core.**
    - The core is total: invalid input is ignored, never a panic.
    - In the platform layer, `expect` is allowed only for setup failures that are unrecoverable, and the message must say what failed.
14. **`unsafe` only in platform modules.** Each block gets a `// SAFETY:` comment.
15. **Comments explain *why*, not *what*.** Every ported algorithm links its source (Excalidraw or perfect-freehand file and version) and lists any deviation.
16. **Keep files and functions small.** A file over ~400 lines or a function over ~60 lines is a signal to split it. Splitting isn't mandatory, but the reviewer should ask about it.

## Testing

17. **Test through the two seams only:**
    - the annotator core interface (scripted events in; Ink, copy JSON and pixels out)
    - stroke-outline parity: freehand and laser output compared with golden fixtures generated from the JS libraries
18. **Test behavior, not internals.** A refactor that keeps behavior must not break tests.
19. **Every bug fix starts with a failing test** at one of those two seams.
20. **The platform layer is checked by hand,** using the release checklist in the spec. Keep it thin enough that this is reasonable.

## Commits and PRs

21. **Small, focused PRs** that reference an issue.
22. **The PR description lists:** user-visible change, footprint impact (size/RAM if it touches the hot path or dependencies) and the manual checks done.
23. **Hard-to-reverse decisions get an ADR** in `docs/adr/`.
