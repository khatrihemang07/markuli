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
One piece of Ink: a freehand stroke with its points, optional pressures, color, width and opacity. It has the same shape as an Excalidraw freedraw element.
_Avoid_: shape, object, path

**Stroke**:
The gesture of drawing one Element with the Pen, from pointer down to pointer up.

**Clear**:
Removes all Ink from the Overlay.
_Avoid_: reset, wipe

**Laser**:
A fading pointer trail. It is never part of Ink and can't be selected, undone or copied.
_Avoid_: pointer, spotlight

**Tool**:
What pointer input does in Draw Mode. One of Select, Pen, Eraser or Laser.

**Selection**:
The Elements currently chosen with the Select Tool, for moving, deleting or copying.

**Settings**:
User preferences that persist across launches: the hotkeys and launch at login. Ink is never part of Settings.

## Relationships

- An **Overlay** shows exactly one **Ink**. Leaving **Draw Mode** hides the **Ink** (the **Overlay** is destroyed, the **Ink** stays in memory); entering again on the same display shows the same **Ink**, with its undo history. Entering on another display **Clears** it.
- **Ink** is made of zero or more **Elements**. A **Selection** is a subset of the **Ink**.
- **Tools** act only in **Draw Mode**. **Ink** is shown only in **Draw Mode**.

## Example dialogue

> **Dev:** "If I toggle on the second monitor, does the Ink follow?"
> **Domain expert:** "No. The Overlay moves, and moving Clears the Ink. Arrows pointing at monitor 1 would mean nothing on monitor 2."

## Flagged ambiguities

- "Excalidraw" means the **ink look and data format** (perfect-freehand settings, laser decay, freedraw element JSON), not the Excalidraw app or its code. See ADR-0001.
