//! Seam 1: the toolbar and the style panel lay out below the display's top
//! inset (macOS menu bar and notch, Windows work area), while Ink can still be
//! drawn anywhere.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    reason = "test pixel coordinates are small and positive"
)]

use markuli_core::{Annotator, Button, Control, DisplayId, Event, Format, Point};
use tiny_skia::Pixmap;

const W: u32 = 1600;
const H: u32 = 900;

fn scene(scale: f32, inset: u32) -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::ScaleFactor(scale));
    a.handle(Event::Resize {
        width: W,
        height: H,
    });
    a.handle(Event::Insets { top: inset });
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    a
}

fn render(a: &mut Annotator) -> Pixmap {
    let mut pm = Pixmap::new(W, H).expect("pixmap");
    a.render(&mut pm.as_mut(), Format::Rgba);
    pm
}

/// The tallest alpha in the top `rows` rows.
fn top_alpha(pm: &Pixmap, rows: u32) -> u8 {
    pm.data()
        .as_chunks::<4>()
        .0
        .iter()
        .take((rows * W) as usize)
        .map(|p| p[3])
        .max()
        .unwrap_or(0)
}

#[test]
fn toolbar_and_panel_sit_below_the_inset() {
    for (scale, inset) in [(1.0, 37_u32), (2.0, 74)] {
        let mut a = scene(scale, inset);
        let bar = a.button_center(Button::Tool(1)).expect("toolbar").y;
        let swatch = a.panel_center(Control::Color(0)).expect("panel").y;
        assert!(bar > inset as f32 + 16.0 * scale, "toolbar at {bar}");
        assert!(swatch > bar, "panel at {swatch} under the toolbar at {bar}");
        // Nothing of the chrome (shadows included) is painted in the inset:
        // only the 1/255 hit-test backdrop.
        let pm = render(&mut a);
        assert!(top_alpha(&pm, inset) <= 1, "scale {scale}");
    }
}

#[test]
fn without_an_inset_the_toolbar_keeps_its_place() {
    let a = scene(1.0, 0);
    let bar = a.button_center(Button::Tool(1)).expect("toolbar").y;
    assert!((bar - 36.0).abs() < 0.5, "toolbar at {bar}");
}

#[test]
fn ink_can_still_be_drawn_in_the_inset() {
    let mut a = scene(1.0, 37);
    let at = Point { x: 400.0, y: 10.0 };
    a.handle(Event::PointerDown(at));
    a.handle(Event::PointerMove(Point { x: 500.0, y: 12.0 }));
    a.handle(Event::PointerUp(Point { x: 600.0, y: 10.0 }));
    assert_eq!(a.ink().len(), 1);
    let pm = render(&mut a);
    assert!(top_alpha(&pm, 37) > 200);
}
