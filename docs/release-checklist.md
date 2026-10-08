# Release checklist

The platform layer (windows, transparency, focus, hotkeys, tray, native Settings, clipboard, launch at login) is not covered by automated tests (coding standard 20). Run this by hand on **both** macOS and Windows before pushing a release tag. Story numbers refer to the spec (#1).

Use a clean user profile or a clean config: delete the `config` file in `~/Library/Application Support/Markuli/` (macOS) or `%APPDATA%\Markuli\` (Windows) first.

## 0. Build the artifact

- [ ] Push a tag `vX.Y.Z`. The **Release** workflow builds the universal macOS dmg and the Windows zip and attaches both to a GitHub Release.
- [ ] Locally, `scripts/package-macos.sh X.Y.Z` (macOS) and `scripts/package-windows.ps1 X.Y.Z` (Windows) produce the same files in `dist/`. Both scripts fail if the artifact is over 4 MB.
- [ ] macOS: `lipo -info` on `Markuli.app/Contents/MacOS/markuli` lists `x86_64 arm64` (story 57).
- [ ] Windows: the zip contains `Markuli.exe` and starts on a machine with no Rust or runtime installed (story 58).
- [ ] The build is unsigned. macOS: first launch is Control-click, Open (or `xattr -d com.apple.quarantine`). Windows: SmartScreen shows "More info", "Run anyway". No auto-updater exists.

## 1. Footprint (stories 51-54)

Measure the **release** build, not debug.

| Measure | Target | How | Result |
|---|---|---|---|
| Artifact size | at most 4 MB | size of the dmg / zip | macOS: **met**. Universal dmg 1,542,833 B (1.47 MiB), universal binary 2,252,960 B, arm64 binary 1,013,536 B (`scripts/package-macos.sh`, final build). Windows: not measured. |
| RAM idle, no Ink | at most 10 MB | macOS: `footprint <pid>` (phys_footprint, not RSS). Windows: Task Manager, Details tab, "Memory (private working set)" | macOS: **missed**, 11 MB (the bare AppKit baseline, see note below); after Clear 14 MB, stable over repeated cycles. Windows: not measured. |
| RAM while drawing | at most 40 MB | same, during a Stroke on the largest display | macOS: **met**. Draw Mode with toolbar 21 MB, 23 MB after two Strokes (1920x1080 at 1x, final build, synthesized events). Windows: not measured. |
| RAM after closing Settings | back to idle (no growth after repeated open/close) | open and close Settings 5 times, compare to the first close | macOS: **missed**. 19-20 MB after close vs 11 MB idle (AppKit text and control caches), flat over 6 open/close cycles, so no leak. Windows: not measured. |
| RAM after leaving Draw Mode with Ink | near idle (the Overlay is destroyed) | leave Draw Mode after drawing, `footprint <pid>` | macOS: 14 MB (idle 11 MB, Draw Mode 29-30 MB before double buffering, 45 MB after (second 17 MB IOSurface; over the 40 MB drawing budget on a 2560x1664 display, see ADR-0003), 2 Strokes), the same floor as after Clear. Windows: not measured. |
| Idle CPU | 0% | Activity Monitor / Task Manager for 30 s with no Ink | macOS: **met**, 0.0% (also 0.0% in Draw Mode and with Ink visible). Windows: not measured. |
| Hotkey to first Stroke | under 16 ms | screen-record at 60 fps, count frames from key press to first ink | macOS: **missed** for the first Overlay after the process has been idle: 49-75 ms cold (NSWindow creation 58 ms, orderFront 11-14 ms; WindowServer and AppKit cost), 7-20 ms warm. Pre-creating the window would cost idle RAM and contradict ADR-0002, so it stays. Measured with stderr traces, not a screen recording. Windows: not measured. |

Note for macOS: AppKit and the tray put the idle baseline near 10-11 MB before any window exists. Opening Settings loads AppKit text and control caches that stay resident (measured about +8 MB, flat across repeated opens). Record the numbers; do not hide a regression.

## 2. Overlay and Draw Mode (stories 1-13)

- [ ] Default hotkey `Alt+`` enters Draw Mode on the display under the cursor (1, 2). Try with the cursor on each display.
- [ ] The Overlay is ready immediately; the first Stroke is not lost (3).
- [ ] The same hotkey leaves Draw Mode: the toolbar, style panel and Ink all disappear at once, on the real display, not just in a screenshot (4, 5). Entering Draw Mode again on the same display shows the same Ink and undo history.
- [ ] Outside Draw Mode there is no Overlay: clicks, scrolls and keystrokes reach the app underneath (6).
- [ ] With a real mouse, every Stroke appears while it is drawn, not only the first one (ADR-0003). Screenshots read the surface memory and can look right while the screen is stale: check with your eyes.
- [ ] The Clear hotkey (`Alt+1`) and the toolbar Clear button remove all Ink and stay in Draw Mode; Cmd/Ctrl+Z brings the Ink back. Outside Draw Mode `Alt+1` clears the hidden Ink and no Overlay appears (7).
- [ ] Leaving Draw Mode gives the focus back to the app and window that had it: type at once, without a click (B1). Tool keys work right after entering, with no click: `V`/`1` Select, `P`/`2` Pen, `E`/`3` Eraser, `K`/`4` Laser (`7` Pen and `0` Eraser still work); after another app took focus, one click on the Overlay brings them back. The toolbar shows no number hints.
- [ ] Hovering the style panel and toolbar in Draw Mode never makes them blink (ADR-0003, double buffering).
- [ ] Toggling on a second display moves the Overlay there and starts with no Ink (8).
- [ ] The Overlay is absent from the Dock/taskbar, Alt-Tab / Cmd-Tab and Mission Control (9).
- [ ] macOS: toggling over a full-screen app does not create or switch Spaces (10).
- [ ] The Overlay is above normal windows (11).
- [ ] The Overlay covers the display and Ink lines up with what is under the cursor, including the menu bar / taskbar areas, on a Retina or scaled (125%, 150%) display (12).
- [ ] Esc in Draw Mode never clears Ink (13).
- [ ] Pen, style panel, Eraser, Laser, Select, toolbar, undo/redo and copy to Excalidraw: check against their own tickets (stories 14-43).

## 3. Settings (stories 44-50)

- [ ] The tray icon menu has "Settings..." and "Quit" (44). Quit discards Ink (56).
- [ ] "Settings..." opens a small native window; choosing it again while open just brings it to the front.
- [ ] Click the Toggle button, press a new combo (for example `Ctrl+Alt+Shift+J`): the button shows it, and the new hotkey works at once with no restart (45, 47). The old combo no longer does anything.
- [ ] Same for Clear.
- [ ] Esc while recording cancels and keeps the old combo. A bare letter is refused with a message; a function key alone is accepted.
- [ ] Conflict: pick a combo another app owns (or run a second Markuli that holds it). The window shows a clear message and the old binding keeps working (48). Windows: also try a system-reserved combo such as `Win+L`.
- [ ] Choosing the Clear combo for Toggle (or the reverse) is refused with a message.
- [ ] Quit and relaunch: the new hotkeys, and the launch-at-login checkbox, are remembered (49). The `config` file is plain `key=value` and holds nothing else; edit it to garbage and relaunch: the app still starts with defaults.
- [ ] Launch at login: tick the box, **log out and in (or reboot)**: Markuli is running (46). macOS: `~/Library/LaunchAgents/com.markuli.app.plist` exists. Windows: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` has a `Markuli` value. Untick: the file / value is gone and Markuli no longer starts.
- [ ] Close Settings: the window is gone and memory is back at the idle level of section 1 (50).

## 4. Privacy and permissions (story 55)

- [ ] macOS: no permission prompt appears at any point (no Accessibility, Input Monitoring, Screen Recording). System Settings, Privacy & Security lists no entry for Markuli.
- [ ] No network access: run with a firewall (Little Snitch, `lsof -i -a -p <pid>`, Windows Resource Monitor) and confirm no connections.
- [ ] No Ink on disk: after drawing and quitting, nothing but `config` exists in the config directory, and the file has no Ink.

## 5. Test hooks (not for users)

These exist only in a build made with `cargo build --release --features dev-hooks`. The shipped release binary does not contain them (check with `strings target/release/markuli | grep MARKULI_`, which must print nothing). Footprint numbers in section 1 come from the build without the feature.

- `MARKULI_CONFIG_DIR` points the config file at another folder.
- `MARKULI_LAUNCH_AGENT_DIR` (macOS) redirects the LaunchAgent plist.
- macOS: a dev-hooks build prints `present: two IOSurfaces...` on stderr when an Overlay is created (ADR-0003).
- `MARKULI_OPEN_SETTINGS=N` opens Settings at start and again after each close, N times. Use it to script the open/close footprint check, since a tray menu cannot be clicked from a script.
