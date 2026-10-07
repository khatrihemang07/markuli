//! Icon data and the stroke renderer for it.
//!
//! The path data is Excalidraw's `components/icons.tsx` (MIT, see
//! `THIRD_PARTY_LICENSES`), copied verbatim; tabler-derived icons are noted
//! there too. All icons are stroke-only with round caps and joins.
//! A Tool brings its own icon as a `const Icon`, so adding one never edits
//! this file's other icons.

use crate::svg_path;
use tiny_skia::{LineCap, LineJoin, Paint, PixmapMut, Stroke, Transform};

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
    target: &mut PixmapMut<'_>,
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

/// The key-binding hint digits Excalidraw prints on its buttons. A font
/// would break the size budget (standards rule 10), so digits are strokes in
/// a 5 x 9 box.
fn digit(c: char) -> Option<&'static str> {
    Some(match c {
        '0' => "M.5 .5h4v8h-4z",
        '1' => "M1 2l2.5-1.5v8",
        '2' => "M.5 1.5l1-1h2l1 1v2l-4 5h4",
        '3' => "M.5 .5h4l-2 3.5l2 1v2.5l-1 1h-2l-1-1",
        '4' => "M3.5 8.5v-8l-3 6h4",
        '5' => "M4.5 .5h-4v3.5h3l1 1v3l-1 1h-3l-1-1",
        '6' => "M4 .5h-2l-1.5 1.5v6l1 1h3l1-1v-3l-1-1h-3",
        '7' => "M.5 .5h4l-2.5 8",
        '8' => "M1.5 .5h2l1 1v2l-1 1l1 1v2.5l-1 1h-3l-1-1v-2.5l1-1l-1-1v-2z",
        '9' => "M.5 8.5h2l1.5-1.5v-6l-1-1h-3l-1 1v3l1 1h3",
        _ => return None,
    })
}

/// Draws the hint digit `c` with its bottom-right corner at `(right, bottom)`,
/// `height` pixels tall.
pub(crate) fn draw_digit(
    target: &mut PixmapMut<'_>,
    c: char,
    (right, bottom, height): (f32, f32, f32),
    paint: &Paint<'_>,
) {
    let Some(d) = digit(c) else { return };
    let k = height / 9.0;
    let at = Transform::from_row(k, 0.0, 0.0, k, right - 5.0 * k, bottom - 9.0 * k);
    stroke_paths(target, &[d], 1.1, at, paint);
}

fn stroke_paths(
    target: &mut PixmapMut<'_>,
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
            target.stroke_path(&path, paint, &stroke, at, None);
        }
    }
}
