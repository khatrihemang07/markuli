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
| RAM after leaving Draw Mode with Ink | near idle (the Overlay is destroyed) | leave Draw Mode after drawing, `footprint <pid>` | macOS: 14 MB (idle 11 MB, Draw Mode 29-30 MB on a 2560x1664 Retina display; a short-lived always-double-buffered build used 45-47 MB, see ADR-0003), the same floor as after Clear. Windows: not measured. |
| Idle CPU | 0% | Activity Monitor / Task Manager for 30 s with no Ink | macOS: **met**, 0.0% (also 0.0% in Draw Mode and with Ink visible). Windows: not measured. |
| Hotkey to first Stroke | under 16 ms | screen-record at 60 fps, count frames from key press to first ink | macOS: **missed** for the first Overlay after the process has been idle: 49-75 ms cold (NSWindow creation 58 ms, orderFront 11-14 ms; WindowServer and AppKit cost), 7-20 ms warm. Pre-creating the window would cost idle RAM and contradict ADR-0002, so it stays. Measured with stderr traces, not a screen recording. Windows: not measured. |

Note for macOS: AppKit and the tray put the idle baseline near 10-11 MB before any window exists. Opening Settings loads AppKit text and control caches that stay resident (measured about +8 MB, flat across repeated opens). Record the numbers; do not hide a regression.

## 2. Overlay and Draw Mode (stories 1-13)

- [ ] Default hotkey `Alt+`` enters Draw Mode on the display under the cursor (1, 2). Try with the cursor on each display.
- [ ] The Overlay is ready immediately; the first Stroke is not lost (3).
- [ ] The same hotkey leaves Draw Mode: the Toolbar and Ink all disappear at once, on the real display, not just in a screenshot (4, 5). Entering Draw Mode again on the same display shows the same Ink and undo history.
- [ ] Outside Draw Mode there is no Overlay: clicks, scrolls and keystrokes reach the app underneath (6).
- [ ] With a real mouse, every Stroke appears while it is drawn, not only the first one (ADR-0003). Screenshots read the surface memory and can look right while the screen is stale: check with your eyes.
- [ ] The Clear hotkey (`Alt+1`) and the toolbar Clear button remove all Ink and stay in Draw Mode; Cmd/Ctrl+Z brings the Ink back. Outside Draw Mode `Alt+1` clears the hidden Ink and no Overlay appears (7).
- [ ] Leaving Draw Mode gives the focus back to the app and window that had it: type at once, without a click (B1). Tool keys work right after entering, with no click: `V` Select, `P` Pen, `E` Eraser, `K` Laser (`7` Pen and `0` Eraser still work; digits `1`-`4` no longer choose Tools); after another app took focus, one click on the Overlay brings them back. The toolbar shows no number hints.
- [ ] The cursor shows what the Tool does: Pen = a marker whose nib is in the current color and widens thin/medium/bold, with a dot exactly as wide as the stroke under its tip (a stroke starts exactly under the dot; both follow the chosen colour and width), Eraser = a pink and white eraser block touching the pointer with its corner, Laser = a red ring with a center dot, Select and the toolbar = the system arrow. Crisp on a Retina display. `P` and `E` (and `7`, `0`) toggle between Pen and Eraser (the key of the active Tool switches to the other); `V` and `K` only select; toolbar clicks never toggle.
- [ ] Style keys: `1`-`5` pick black, red, green, blue, yellow and `[`/`]` step thin/medium/bold (stopping at the ends); `6`, `8`, `9` do nothing. From Eraser, Laser or an empty Select they switch to the Pen; with a Selection they restyle it as one undo step and leave the Style alone. They are ignored mid-Stroke, with Cmd/Ctrl held, and (digits only) with Alt held; `[`/`]` still work with Alt/Option.
- [ ] One Toolbar row, in this order: Tools, 5 colours, 3 widths, undo/redo/Clear, about 580 px wide and centred; it never moves or resizes when the Tool or Selection changes. The current colour and width are highlighted (neither for a Selection with mixed values). With the Eraser, Laser or an empty Select the colours and widths look dimmed but a click still works, like its key (it switches to the Pen). A press on any Toolbar button never starts a Stroke; the cursor is the arrow over it. There is no separate style panel, at any window width.
- [ ] Toolbar Position (macOS Settings, "Toolbar" row of four radio buttons; real mouse, second Retina display): for each of Top, Bottom, Left and Right, with the Dock at the bottom, then on the left, then on the right, the Toolbar stays clear of the Dock and menu bar (nothing, shadow included, is drawn under them), is centred between them, and the Bottom shadow stays off the Dock. On a notched display the Top row sits below the notch's safe area. Top and Bottom are a row, Left and Right a column (Tools first, at the top). Choosing a radio button moves the Toolbar at once with no leftover Toolbar pixels at the old place and the Ink untouched, and exactly one radio is on. Quit and relaunch: the position is remembered (`toolbar=top|bottom|left|right` in `config`; `toolbar=sideways` falls back to Top). On a display too short for the 612 px column, Left and Right fall back to the Top row. The insets are read when Draw Mode is entered, so a Dock change shows at the next entry. Windows: the work-area insets apply, but Settings has no radio buttons yet (set the `config` key by hand).
- [ ] Hovering the Toolbar in Draw Mode never makes it blink (ADR-0003: each damaged region is composed off-screen and copied once).
- [ ] Toggling on a second display moves the Overlay there and starts with no Ink (8).
- [ ] The Overlay is absent from the Dock/taskbar, Alt-Tab / Cmd-Tab and Mission Control (9).
- [ ] macOS: toggling over a full-screen app does not create or switch Spaces (10).
- [ ] The Overlay is above normal windows (11).
- [ ] The Overlay covers the display and Ink lines up with what is under the cursor, including the menu bar / taskbar areas, on a Retina or scaled (125%, 150%) display (12).
- [ ] Esc in Draw Mode never clears Ink (13).
- [ ] Pen, Eraser, Laser, Select, toolbar, undo/redo and copy to Excalidraw: check against their own tickets (stories 14-43).

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
- [ ] Style is remembered: in Draw Mode pick a colour and a width (Pen), leave Draw Mode, quit and relaunch, enter Draw Mode: the Toolbar highlights them and the next Stroke uses them. The `config` file gains `color=<0..4>` and `width=<0..2>`; set `color=9` by hand and relaunch: red (the default) is used. The file is rewritten only when the style changes (check its modified time while only drawing).
- [ ] Close Settings: the window is gone and memory is back at the idle level of section 1 (50).

## 4. Privacy and permissions (story 55)

- [ ] macOS: no permission prompt appears at any point (no Accessibility, Input Monitoring, Screen Recording). System Settings, Privacy & Security lists no entry for Markuli.
- [ ] No network access: run with a firewall (Little Snitch, `lsof -i -a -p <pid>`, Windows Resource Monitor) and confirm no connections.
- [ ] No Ink on disk: after drawing and quitting, nothing but `config` exists in the config directory, and the file has no Ink.

## 5. Test hooks (not for users)

These exist only in a build made with `cargo build --release --features dev-hooks`. The shipped release binary does not contain them (check with `strings target/release/markuli | grep MARKULI_`, which must print nothing). Footprint numbers in section 1 come from the build without the feature.

- `MARKULI_CONFIG_DIR` points the config file at another folder.
- `MARKULI_LAUNCH_AGENT_DIR` (macOS) redirects the LaunchAgent plist.
- `MARKULI_PRESENT_FALLBACK=1` (macOS) forces the double-buffered presenter used when `CALayer.setContentsChanged` is missing (ADR-0003).
- `MARKULI_OPEN_SETTINGS=N` opens Settings at start and again after each close, N times. Use it to script the open/close footprint check, since a tray menu cannot be clicked from a script.
