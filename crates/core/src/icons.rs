//! Icon data and the stroke renderer for it.
//!
//! The path data is Excalidraw's `components/icons.tsx` (MIT, see
//! `THIRD_PARTY_LICENSES`), copied verbatim; tabler-derived icons are noted
//! there too. All icons are stroke-only with round caps and joins.
//! A Tool brings its own icon as a `const Icon`, so adding one never edits
//! this file's other icons.

use crate::render::Canvas;
use crate::svg_path;
use tiny_skia::{LineCap, LineJoin, Paint, Stroke, Transform};

pub(crate) struct Icon {
    /// The square viewBox edge, in path units.
    pub view: f32,
    /// Stroke width in path units.
    pub stroke: f32,
    pub paths: &'static [&'static str],
}

/// `FreedrawIcon`: tabler pencil, modified.
pub(crate) const PEN: Icon = Icon {
    view: 20.0,
    stroke: 1.25,
    paths: &[
        "m7.643 15.69 7.774-7.773a2.357 2.357 0 1 0-3.334-3.334L4.31 12.357a3.333 3.333 0 0 0-.977 2.357v1.953h1.953c.884 0 1.732-.352 2.357-.977Z",
        "m11.25 5.417 3.333 3.333",
    ],
};

/// `SelectionIcon`, Excalidraw's own arrow. Its paths use a 24 grid inside a
/// 22 viewBox, exactly as Excalidraw ships it.
pub(crate) const SELECTION: Icon = Icon {
    view: 22.0,
    stroke: 1.25,
    paths: &[
        "M6 6l4.153 11.793a0.365 .365 0 0 0 .331 .207a0.366 .366 0 0 0 .332 -.207l2.184 -4.793l4.787 -1.994a0.355 .355 0 0 0 .213 -.323a0.355 .355 0 0 0 -.213 -.323l-11.787 -4.36z",
        "M13.5 13.5l4.5 4.5",
    ],
};

/// `EraserIcon`: tabler eraser.
pub(crate) const ERASER: Icon = Icon {
    view: 24.0,
    stroke: 1.5,
    paths: &[
        "M19 20h-10.5l-4.21 -4.3a1 1 0 0 1 0 -1.41l10 -10a1 1 0 0 1 1.41 0l5 5a1 1 0 0 1 0 1.41l-9.2 9.3",
        "M18 13.3l-6.3 -6.3",
    ],
};

/// `laserPointerToolIcon`. Excalidraw draws it inside `rotate(90 10 10)`;
/// `Icon` has no transform, so the paths are pre-rotated (x, y) -> (20 - y, x)
/// into absolute coordinates, arcs included (circular, so only the end moves).
pub(crate) const LASER: Icon = Icon {
    view: 20.0,
    stroke: 1.25,
    paths: &[
        "M6.31 9.644L14.083 17.418A2.357 2.357 0 0 0 17.417 14.084L9.643 6.311L8 8L6.31 9.643Z",
        "M16.583 13.25L13.25 16.583M10 10L12 12M5 5L8 8M2.106 2.156L3.106 3.156M0.971 5.453L2.378 5.309M8.113 2.377L6.995 3.243M2.727 8.354L3.485 7.16M5.348 0.953L5.218 2.361",
    ],
};

/// `handIcon`: tabler hand-stop.
pub(crate) const HAND: Icon = Icon {
    view: 24.0,
    stroke: 1.25,
    paths: &[
        "M8 13v-7.5a1.5 1.5 0 0 1 3 0v6.5",
        "M11 5.5v-2a1.5 1.5 0 1 1 3 0v8.5",
        "M14 5.5a1.5 1.5 0 0 1 3 0v6.5",
        "M17 7.5a1.5 1.5 0 0 1 3 0v8.5a6 6 0 0 1 -6 6h-2h.208a6 6 0 0 1 -5.012 -2.7a69.74 69.74 0 0 1 -.196 -.3c-.312 -.479 -1.407 -2.388 -3.286 -5.728a1.5 1.5 0 0 1 .536 -2.022a1.867 1.867 0 0 1 2.28 .28l1.47 1.47",
    ],
};

/// `UndoIcon`.
pub(crate) const UNDO: Icon = Icon {
    view: 20.0,
    stroke: 1.25,
    paths: &["M7.5 10.833 4.167 7.5 7.5 4.167M4.167 7.5h9.166a3.333 3.333 0 0 1 0 6.667H12.5"],
};

/// `RedoIcon`.
pub(crate) const REDO: Icon = Icon {
    view: 20.0,
    stroke: 1.25,
    paths: &["M12.5 10.833 15.833 7.5 12.5 4.167M15.833 7.5H6.667a3.333 3.333 0 1 0 0 6.667H7.5"],
};

/// `TrashIcon`, Excalidraw's own "Clear canvas" glyph.
pub(crate) const TRASH: Icon = Icon {
    view: 20.0,
    stroke: 1.25,
    paths: &["M3.333 5.833h13.334M8.333 9.167v5M11.667 9.167v5M4.167 5.833l.833 10c0 .92.746 1.667 1.667 1.667h6.666c.92 0 1.667-.746 1.667-1.667l.833-10M7.5 5.833v-2.5c0-.46.373-.833.833-.833h3.334c.46 0 .833.373.833.833v2.5"],
};

/// Strokes `icon` into a `size` x `size` pixel square at `(x, y)`.
pub(crate) fn draw(
    target: &mut Canvas<'_, '_>,
    icon: &Icon,
    (x, y, size): (f32, f32, f32),
    paint: &Paint<'_>,
) {
    let k = size / icon.view;
    stroke_paths(
        target,
        icon.paths,
        icon.stroke,
        Transform::from_row(k, 0.0, 0.0, k, x, y),
        paint,
    );
}

fn stroke_paths(
    target: &mut Canvas<'_, '_>,
    paths: &[&str],
    width: f32,
    at: Transform,
    paint: &Paint<'_>,
) {
    let stroke = Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    for d in paths {
        if let Some(path) = svg_path::parse(d) {
            target.stroke_path(&path, paint, &stroke, at);
        }
    }
}
