# Excalidraw-native Rust instead of embedding Excalidraw in a webview

Markuli is inspired by Penio, which embeds the full Excalidraw React app in a Tauri webview: one per monitor, ~34 MB of assets and ~120+ MB of RAM per webview. Our tool set is only Select, Pen, Eraser and Laser. For that, Excalidraw boils down to perfect-freehand with its exact options, the laser-pointer decay and the freedraw element JSON. So we port those algorithms to a single native Rust binary (winit + tiny-skia) with no webview anywhere. Targets: ~2–4 MB download, ~5–40 MB RAM, 0 idle CPU and instant Draw Mode.

## Considered options

- **Real Excalidraw in a webview**, frozen or destroyed when idle. Every Excalidraw feature comes for free, but it costs ~40–120 MB RAM and 30–300 ms to enter Draw Mode, and the webview's memory can't be controlled.
- **Hybrid**: a native Pen, with the Excalidraw webview loaded only for Select. Rejected because it means two rendering engines that must look identical.
- **Own canvas in a webview.** Rejected because it keeps the webview's cost without Excalidraw's features.

## Consequences

- New Excalidraw tools (shapes, text) don't arrive for free. Each one has to be ported.
- Ink parity depends on an exact port, guarded by golden tests generated from the JS libraries.
- Users who want the full app copy Ink as Excalidraw clipboard JSON and paste it into excalidraw.com.
