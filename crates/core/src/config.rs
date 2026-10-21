//! Settings: the two hotkeys, launch at login and the remembered Style. Never Ink.

use crate::palette::{Palette, COLORS, WIDTHS};
use crate::recent::{RecentColors, CAPACITY};
use crate::style::Style;
use crate::ToolbarPosition;

/// The `palette=` value of the current format: the red, amber, green, blue,
/// grape defaults (0.6.0). Version 2 (widths in pt, 0.4.0) is migrated slot by
/// slot; configs without a version hold old Excalidraw-unit widths and colors.
const PALETTE_VERSION: &str = "3";

/// The version 2 default colors. A slot still holding one was never edited,
/// so it takes the new default.
const V2_DEFAULT: [[u8; 3]; COLORS] = [
    [0xe0, 0x31, 0x31],
    [0x19, 0x71, 0xc2],
    [0x2f, 0x9e, 0x44],
    [0xfa, 0xb0, 0x05],
    [0x1e, 0x1e, 0x1e],
];

/// What the user can change in Settings. Hotkeys are kept as text because the
/// core knows no key codes; the platform layer parses and validates them.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub toggle: String,
    pub clear: String,
    pub launch_at_login: bool,
    /// The Pen's Style, remembered across launches.
    pub style: Style,
    /// Where the Toolbar sits.
    pub toolbar: ToolbarPosition,
    /// The color and width slots, remembered across launches.
    pub palette: Palette,
    /// The Recent Colors, newest first.
    pub recent: RecentColors,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            toggle: "alt+Backquote".into(),
            clear: "alt+Digit1".into(),
            launch_at_login: false,
            style: Style::default(),
            toolbar: ToolbarPosition::default(),
            palette: Palette::DEFAULT,
            recent: RecentColors::default(),
        }
    }
}

impl Config {
    /// Reads a `key=value` file. Unknown keys, malformed lines and empty
    /// values are skipped, so a damaged file never blocks startup.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut config = Self::default();
        let mut version = "";
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "toggle" if !value.is_empty() => config.toggle = value.into(),
                "clear" if !value.is_empty() => config.clear = value.into(),
                "palette" => version = value,
                "launch_at_login" => config.launch_at_login = value == "true",
                "color" => config.style.color = index(value, COLORS, config.style.color),
                "width" => config.style.width = index(value, WIDTHS, config.style.width),
                "colors" => parse_colors(value, &mut config.palette.colors),
                "widths" => parse_widths(value, &mut config.palette.widths),
                "recent" => config.recent = parse_recent(value),
                "toolbar" => {
                    config.toolbar = ToolbarPosition::from_name(value).unwrap_or(config.toolbar);
                }
                _ => {}
            }
        }
        match version {
            PALETTE_VERSION => {}
            // Same format, old defaults: only the slots the user never edited
            // change; their Style, widths and edits are kept.
            "2" => {
                let defaults = Palette::DEFAULT.colors.into_iter().zip(V2_DEFAULT);
                for (slot, (new, old)) in config.palette.colors.iter_mut().zip(defaults) {
                    if *slot == old {
                        *slot = new;
                    }
                }
            }
            // One-time reset: an older file's colors, widths and Style
            // indices mean something else now. Everything else is kept.
            _ => {
                config.style = Style::default();
                config.palette = Palette::DEFAULT;
            }
        }
        config
    }

    #[must_use]
    pub fn to_text(&self) -> String {
        format!(
            "toggle={}\nclear={}\nlaunch_at_login={}\ncolor={}\nwidth={}\ntoolbar={}\npalette={PALETTE_VERSION}\ncolors={}\nwidths={}\nrecent={}\n",
            self.toggle,
            self.clear,
            self.launch_at_login,
            self.style.color,
            self.style.width,
            self.toolbar.name(),
            join(self.palette.colors.into_iter().map(hex)),
            join(self.palette.widths.iter().map(f32::to_string)),
            join(self.recent.as_slice().iter().copied().map(hex)),
        )
    }
}

/// A `0..len` index, or `keep` when `value` is not one.
fn index(value: &str, len: usize, keep: usize) -> usize {
    value.parse().ok().filter(|&i| i < len).unwrap_or(keep)
}

fn join(items: impl Iterator<Item = String>) -> String {
    items.collect::<Vec<_>>().join(",")
}

/// Each `#rrggbb` of the list sets its slot; a slot whose entry is missing or
/// invalid keeps what it holds.
fn parse_colors(value: &str, slots: &mut [[u8; 3]]) {
    for (slot, item) in slots.iter_mut().zip(value.split(',')) {
        if let Some(rgb) = color(item.trim()) {
            *slot = rgb;
        }
    }
}

fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// The valid `#rrggbb` items of the list, up to the capacity.
fn parse_recent(value: &str) -> RecentColors {
    let colors: Vec<[u8; 3]> = value
        .split(',')
        .filter_map(|item| color(item.trim()))
        .take(CAPACITY)
        .collect();
    RecentColors::new(&colors)
}

fn color(text: &str) -> Option<[u8; 3]> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

/// Same for widths: snapped to 0.25 and clamped like an edit.
fn parse_widths(value: &str, slots: &mut [f32]) {
    for (slot, item) in slots.iter_mut().zip(value.split(',')) {
        if let Some(width) = item.trim().parse().ok().and_then(Palette::snap_width) {
            *slot = width;
        }
    }
}
