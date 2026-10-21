//! The Width Picker's slider: pixels to a width and back. The track is
//! linear from `MIN_WIDTH` to `MAX_WIDTH`, snapped to 0.25 by
//! [`Palette::snap_width`].

use super::layout::KNOB;
use crate::palette::{Palette, MAX_WIDTH, MIN_WIDTH};
use crate::toolbar::Area;

/// The knob center's travel along the track: its ends are one knob radius in
/// from the track's, so the knob never leaves it.
pub(super) fn travel(track: Area, scale: f32) -> (f32, f32) {
    (track.x + KNOB * scale, track.x + track.w - KNOB * scale)
}

/// The width at pixel `x`; the ends hold beyond the track.
pub(super) fn width_at(x: f32, track: Area, scale: f32) -> Option<f32> {
    let (from, to) = travel(track, scale);
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    Palette::snap_width(MIN_WIDTH + t * (MAX_WIDTH - MIN_WIDTH))
}

/// Where the knob's center is for `width`.
pub(super) fn knob_x(width: f32, track: Area, scale: f32) -> f32 {
    let (from, to) = travel(track, scale);
    let t = ((width - MIN_WIDTH) / (MAX_WIDTH - MIN_WIDTH)).clamp(0.0, 1.0);
    from + t * (to - from)
}
