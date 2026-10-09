# Markuli

An on-screen annotator: draw over anything on your screen with Excalidraw-identical ink, then get out of the way.

## Language

**Overlay**:
The single transparent, always-on-top window, covering one display, that Ink is drawn on.
_Avoid_: canvas, board, motion-board

**Draw Mode**:
The state in which the Overlay exists, captures pointer and keyboard input, and shows the Ink. Outside Draw Mode there is no Overlay, so the screen shows nothing of Markuli.
_Avoid_: edit mode, active mode

**Ink**:
The set of elements drawn on the Overlay. It exists only in memory and is shown only in Draw Mode.
_Avoid_: drawing, scene, annotations

**Element**:
One piece of Ink: a freehand stroke with its points, optional pressures, color, width and opacity. It has the same shape as an Excalidraw freedraw element. Its opacity is always 100 when drawn.
_Avoid_: shape, object, path

**Stroke**:
The gesture of drawing one Element with the Pen, from pointer down to pointer up.

**Clear**:
Removes all Ink (undoable). It does not change **Draw Mode**: in Draw Mode the Overlay stays, outside it the hidden Ink is cleared silently and no Overlay appears.
_Avoid_: reset, wipe

**Laser**:
A fading pointer trail. It is never part of Ink and can't be selected, undone or copied.
_Avoid_: pointer, spotlight

**Tool**:
What pointer input does in Draw Mode. One of Select, Pen, Eraser or Laser.

**Selection**:
The Elements currently chosen with the Select Tool, for moving, deleting or copying.

**Toolbar**:
One row or column of islands shown in Draw Mode: the Tools, the five colors, the three widths, then Undo, Redo and Clear. It is always the same size. The color and width controls are dimmed when the active Tool styles nothing. It is part of the Overlay, not Ink.
_Avoid_: panel, style panel

**Toolbar Position**:
Which edge of the Overlay the Toolbar sits on: top, bottom, left or right. It stays clear of the menu bar, notch, Dock and taskbar. A Setting; the default is top.

**Palette**:
The 5 color slots and 3 width slots on the Toolbar. The user can edit them (a secondary click on a slot), and they are remembered across launches. Editing a slot also chooses it. Elements already drawn keep their own color and width. A width is the drawn line's thickness in pt (logical px) at mid pressure, from 0.5 to 30 in steps of 0.25. Defaults are red, blue, green, yellow and black, and widths 2, 4 and 8 pt. The config marks this format with `palette=2`; an older file's colors, widths and chosen slots are ignored once.
_Avoid_: swatches, presets

**Style**:
The color and width the next Stroke is drawn with, and that Select applies to the Selection. A color slot and a width slot of the Palette. It is remembered across launches, but it is not a Setting, and it is not reset when Draw Mode is entered.
_Avoid_: pen settings, brush

**Settings**:
User preferences that persist across launches: the hotkeys, launch at login and the Toolbar Position. Ink is never part of Settings.

## Relationships

- An **Overlay** shows exactly one **Ink**. Leaving **Draw Mode** hides the **Ink** (the **Overlay** is destroyed, the **Ink** stays in memory); entering again on the same display shows the same **Ink**, with its undo history. Entering on another display **Clears** it.
- **Ink** is made of zero or more **Elements**. A **Selection** is a subset of the **Ink**.
- **Tools** act only in **Draw Mode**. **Ink** is shown only in **Draw Mode**.

## Example dialogue

> **Dev:** "If I toggle on the second monitor, does the Ink follow?"
> **Domain expert:** "No. The Overlay moves, and moving Clears the Ink. Arrows pointing at monitor 1 would mean nothing on monitor 2."

## Flagged ambiguities

- "Excalidraw" means the **ink look and data format** (perfect-freehand settings, laser decay, freedraw element JSON), not the Excalidraw app or its code. See ADR-0001.
