# Markuli

A tiny on-screen annotator for macOS with Excalidraw-identical ink. Press a hotkey, draw over anything, and press it again to put it away.

- About 1.5 MB to download, about 11 MB of RAM when idle, 0% CPU when idle
- Native Rust, no webview, no permission prompts

## Install

Download the dmg from [Releases](https://github.com/khatrihemang07/markuli/releases) and drag **Markuli** to Applications.

The build is unsigned. On first launch, right-click the app and choose **Open**.

## Use

| Key | Action |
|---|---|
| `Alt` + `` ` `` | Enter or leave Draw Mode |
| `Alt` + `1` | Clear the Ink |
| `P` / `E` | Toggle Pen and Eraser |
| `V` / `K` | Select / Laser (`7` and `0` also toggle Pen and Eraser) |
| `1`–`5` | Color: black, red, green, blue, yellow |
| `[` / `]` | Thinner / bolder (thin, medium, bold) |
| `Cmd` + `Z` / `Cmd` + `Shift` + `Z` | Undo / redo |
| `Cmd` + `C` | Copy the Ink as Excalidraw data |

Ink shows only in Draw Mode. When you enter Draw Mode again on the same display, your Ink comes back. You change hotkeys and launch at login from the tray icon.

## Build

```sh
cargo build --release                 # target/release/markuli
scripts/package-macos.sh 0.1.0        # universal dmg in dist/
cargo test --workspace
```

## Docs

- [`CONTEXT.md`](CONTEXT.md) is the glossary.
- [`docs/adr/`](docs/adr/) holds the design decisions.
- [`docs/coding-standards.md`](docs/coding-standards.md) covers the code rules and footprint budgets.
- [`docs/release-checklist.md`](docs/release-checklist.md) is the manual release check.

Windows builds exist but are untested.
