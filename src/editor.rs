//! Pure conversions the Palette editors share. No OS calls, so they are unit
//! tested on every host; the Win32 widgets in `platform::windows_editors`
//! only move these values in and out of controls.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "byte extraction from a COLORREF; trackbar positions are 1..=40"
)]

use markuli_core::Palette;

/// A Win32 `COLORREF` (0x00BBGGRR) for `rgb`.
pub fn colorref(rgb: [u8; 3]) -> u32 {
    u32::from(rgb[0]) | u32::from(rgb[1]) << 8 | u32::from(rgb[2]) << 16
}

/// The color a `COLORREF` holds; the high byte is ignored.
pub fn rgb_from_colorref(colorref: u32) -> [u8; 3] {
    [
        colorref as u8,
        (colorref >> 8) as u8,
        (colorref >> 16) as u8,
    ]
}

/// Trackbar positions are half pixels: position 1 is width 0.5.
pub const MIN_POS: i32 = 1;
pub const MAX_POS: i32 = 40;

/// The trackbar position for a slot width (snapped and clamped first).
pub fn pos_from_width(width: f32) -> i32 {
    let width = Palette::snap_width(width).unwrap_or(Palette::DEFAULT.widths[1]);
    ((width * 2.0).round() as i32).clamp(MIN_POS, MAX_POS)
}

/// The slot width a trackbar position stands for.
pub fn width_from_pos(pos: i32) -> f32 {
    pos.clamp(MIN_POS, MAX_POS) as f32 / 2.0
}

/// A width typed in the edit box: snapped to 0.5 and clamped to the slot
/// range, `None` when it is not a number. A comma counts as the decimal point.
pub fn parse_width(text: &str) -> Option<f32> {
    Palette::snap_width(text.trim().replace(',', ".").parse().ok()?)
}

/// Text for the edit box: `2` and `2.5`, never `2.0`.
pub fn width_text(width: f32) -> String {
    if width.fract() == 0.0 {
        format!("{width:.0}")
    } else {
        format!("{width:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorref_is_blue_green_red_order() {
        assert_eq!(colorref([0x11, 0x22, 0x33]), 0x0033_2211);
    }

    #[test]
    fn colorref_round_trips_and_drops_the_high_byte() {
        assert_eq!(rgb_from_colorref(0x0033_2211), [0x11, 0x22, 0x33]);
        assert_eq!(rgb_from_colorref(0xFF33_2211), [0x11, 0x22, 0x33]);
        assert_eq!(rgb_from_colorref(colorref([255, 0, 7])), [255, 0, 7]);
    }

    #[test]
    fn trackbar_positions_are_half_pixels() {
        assert_eq!(pos_from_width(0.5), 1);
        assert_eq!(pos_from_width(2.0), 4);
        assert_eq!(pos_from_width(20.0), 40);
        assert_eq!(width_from_pos(1), 0.5);
        assert_eq!(width_from_pos(5), 2.5);
        assert_eq!(width_from_pos(40), 20.0);
    }

    #[test]
    fn out_of_range_positions_and_widths_clamp() {
        assert_eq!(width_from_pos(0), 0.5);
        assert_eq!(width_from_pos(99), 20.0);
        assert_eq!(pos_from_width(100.0), 40);
        assert_eq!(pos_from_width(f32::NAN), 4);
    }

    #[test]
    fn typed_widths_snap_and_clamp() {
        assert_eq!(parse_width(" 3 "), Some(3.0));
        assert_eq!(parse_width("2.3"), Some(2.5));
        assert_eq!(parse_width("2,5"), Some(2.5));
        assert_eq!(parse_width("0"), Some(0.5));
        assert_eq!(parse_width("57"), Some(20.0));
    }

    #[test]
    fn typed_junk_is_not_a_width() {
        assert_eq!(parse_width(""), None);
        assert_eq!(parse_width("abc"), None);
        assert_eq!(parse_width("inf"), None);
        assert_eq!(parse_width("NaN"), None);
    }

    #[test]
    fn width_text_drops_a_useless_fraction() {
        assert_eq!(width_text(2.0), "2");
        assert_eq!(width_text(2.5), "2.5");
        assert_eq!(width_text(0.5), "0.5");
    }
}
