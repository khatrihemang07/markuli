//! Seam 1: the frame the platform presents right after Draw Mode starts or the
//! theme changes must cover the toolbar, without any pointer event first.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test pixel coordinates are small and positive"
)]

use markuli_core::{Annotator, Button, Damage, DisplayId, Event, Format, Theme};
use tiny_skia::PixmapMut;

const W: u32 = 800;
const H: u32 = 300;

fn render(a: &mut Annotator) -> Option<Damage> {
    let mut buf = vec![0_u8; (W * H * 4) as usize];
    let mut target = PixmapMut::from_bytes(&mut buf, W, H).expect("size matches");
    a.render(&mut target, Format::Rgba)
}

fn covers_toolbar(a: &Annotator, d: Damage) -> bool {
    let c = a.button_center(Button::Tool(1)).expect("toolbar is visible");
    let (x, y) = (c.x as u32, c.y as u32);
    d.x <= x && x < d.x + d.width && d.y <= y && y < d.y + d.height
}

#[test]
fn first_frame_after_entering_draw_mode_covers_the_toolbar() {
    let mut a = Annotator::new();
    a.handle(Event::Resize {
        width: W,
        height: H,
    });
    a.handle(Event::Theme(Theme::Light));
    let view = a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    assert!(view.needs_render);
    let damage = render(&mut a).expect("a frame is due");
    assert!(covers_toolbar(&a, damage));
}

#[test]
fn theme_change_in_draw_mode_repaints_the_toolbar() {
    let mut a = Annotator::new();
    a.handle(Event::Resize {
        width: W,
        height: H,
    });
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    render(&mut a);
    let view = a.handle(Event::Theme(Theme::Dark));
    assert!(view.needs_render);
    let damage = render(&mut a).expect("a frame is due");
    assert!(covers_toolbar(&a, damage));
}
