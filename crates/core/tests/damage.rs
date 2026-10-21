//! Seam 1, pixel output: the damage region returned by `render` covers every
//! pixel that changed, so an incrementally updated buffer stays equal to a
//! from-scratch render.

#![allow(clippy::cast_precision_loss, reason = "tiny test values")]

use markuli_core::{Annotator, Damage, DisplayId, Event, Format, Point};
use tiny_skia::Pixmap;

const W: u32 = 240;
const H: u32 = 180;

struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / 16_777_216.0
    }
}

fn render(a: &mut Annotator, pm: &mut Pixmap, format: Format) -> Option<Damage> {
    let mut m = pm.as_mut();
    a.render(&mut m, format)
}

/// What a from-scratch render of the current Ink looks like.
fn fresh(a: &mut Annotator, format: Format, (w, h): (u32, u32)) -> Pixmap {
    let mut pm = Pixmap::new(w, h).expect("pixmap");
    a.handle(Event::SurfaceReset);
    render(a, &mut pm, format);
    pm
}

fn pixel(pm: &Pixmap, x: u32, y: u32) -> &[u8] {
    let i = ((y * pm.width() + x) * 4) as usize;
    &pm.data()[i..i + 4]
}

/// The rasterizer's edge coverage can flip by a sample (16/255) on edges whose
/// chopping differs between a band and the full surface, even far from the
/// change. A missed damage region is a whole stroke fringe, off by ~255.
fn far(a: &[u8], b: &[u8]) -> bool {
    a.iter().zip(b).any(|(p, q)| p.abs_diff(*q) > 64)
}

/// Renders incrementally and checks against from-scratch renders: every pixel
/// that changed since the previous frame lies inside the returned damage, and
/// the incrementally updated buffer matches a fresh one.
fn check(a: &mut Annotator, pm: &mut Pixmap, before: &mut Pixmap, format: Format, what: &str) {
    let damage = render(a, pm, format);
    let now = fresh(a, format, (pm.width(), pm.height()));
    for y in 0..pm.height() {
        for x in 0..pm.width() {
            let inside = damage
                .is_some_and(|d| x >= d.x && x < d.x + d.width && y >= d.y && y < d.y + d.height);
            assert!(
                inside || !far(pixel(before, x, y), pixel(&now, x, y)),
                "{what}: pixel {x},{y} changed outside {damage:?}"
            );
            assert!(
                !far(pixel(pm, x, y), pixel(&now, x, y)),
                "{what}: pixel {x},{y} differs from a fresh render: {:?} vs {:?}",
                pixel(pm, x, y),
                pixel(&now, x, y)
            );
        }
    }
    *before = now;
}

fn run(scale: f32, format: Format, pressure: bool) {
    let mut a = Annotator::new();
    let mut pm = Pixmap::new(W, H).expect("pixmap");
    a.handle(Event::ScaleFactor(scale));
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    render(&mut a, &mut pm, format);
    let mut before = fresh(&mut a, format, (W, H));
    let mut rng = Lcg(7);
    let (w, h) = (W as f32, H as f32);
    for stroke in 0..5 {
        let mut at = Point {
            x: rng.next() * w,
            y: rng.next() * h,
        };
        a.handle(Event::Pressure(pressure.then(|| 0.1 + rng.next() * 0.8)));
        a.handle(Event::PointerDown(at));
        check(
            &mut a,
            &mut pm,
            &mut before,
            format,
            &format!("stroke {stroke} down"),
        );
        for step in 0..5 + stroke * 12 {
            // Mixed fast and slow movement, including a sharp turn now and then.
            let speed = if step % 7 == 0 { 30.0 } else { 3.0 };
            at.x = (at.x + (rng.next() - 0.5) * speed * 2.0).clamp(-10.0, w + 10.0);
            at.y = (at.y + (rng.next() - 0.5) * speed * 2.0).clamp(-10.0, h + 10.0);
            a.handle(Event::Pressure(pressure.then(|| 0.1 + rng.next() * 0.8)));
            a.handle(Event::PointerMove(at));
            let what = format!("stroke {stroke} step {step}");
            check(&mut a, &mut pm, &mut before, format, &what);
        }
        a.handle(Event::PointerUp(at));
        let what = format!("stroke {stroke} commit");
        check(&mut a, &mut pm, &mut before, format, &what);
    }
}

#[test]
fn incremental_damage_covers_all_changed_pixels_with_simulated_pressure() {
    run(1.0, Format::Rgba, false);
}

#[test]
fn incremental_damage_covers_all_changed_pixels_with_stylus_pressure_on_bgra() {
    run(1.0, Format::Bgra, true);
}

#[test]
fn incremental_damage_covers_all_changed_pixels_on_a_retina_scale() {
    run(2.0, Format::Rgba, false);
}

/// A picker scene on an Overlay big enough for a popover at `scale`.
fn picker_run(scale: f32, format: Format, steps: impl FnOnce(&mut Annotator, &mut Step<'_>)) {
    let (w, h) = (700_u32, 520_u32);
    let mut a = Annotator::new();
    let mut pm = Pixmap::new(w, h).expect("pixmap");
    a.handle(Event::Resize {
        width: w,
        height: h,
    });
    a.handle(Event::ScaleFactor(scale));
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    // A stroke under where the popover goes, so it has Ink to cover.
    a.handle(Event::PointerDown(Point { x: 200.0, y: 200.0 }));
    a.handle(Event::PointerMove(Point { x: 420.0, y: 300.0 }));
    a.handle(Event::PointerUp(Point { x: 440.0, y: 240.0 }));
    render(&mut a, &mut pm, format);
    let mut before = fresh(&mut a, format, (w, h));
    let mut step = Step {
        pm: &mut pm,
        before: &mut before,
        format,
    };
    steps(&mut a, &mut step);
}

struct Step<'a> {
    pm: &'a mut Pixmap,
    before: &'a mut Pixmap,
    format: Format,
}

impl Step<'_> {
    fn check(&mut self, a: &mut Annotator, what: &str) {
        check(a, self.pm, self.before, self.format, what);
    }
}

#[test]
fn incremental_damage_covers_the_color_picker_through_a_whole_session() {
    use markuli_core::{Button, PickerPart};
    for (scale, format) in [(1.0, Format::Rgba), (2.0, Format::Bgra)] {
        picker_run(scale, format, |a, step| {
            let at = |a: &Annotator, part| a.picker_point(part).expect("open");
            let slot = a.button_center(Button::Color(2)).expect("toolbar");
            a.handle(Event::SecondaryClick(slot));
            step.check(a, "open");
            let cell = at(a, PickerPart::Preset(10));
            a.handle(Event::PointerMove(cell));
            step.check(a, "hover a cell");
            let other = at(a, PickerPart::Preset(30));
            a.handle(Event::PointerMove(other));
            step.check(a, "hover another");
            a.handle(Event::PointerMove(Point { x: 5.0, y: 500.0 }));
            step.check(a, "pointer leaves");
            let tab = at(a, PickerPart::Tab(1));
            a.handle(Event::PointerDown(tab));
            a.handle(Event::PointerUp(tab));
            step.check(a, "custom tab");
            a.handle(Event::PointerDown(at(a, PickerPart::Square(0.3, 0.3))));
            step.check(a, "drag starts");
            for f in [0.4, 0.5, 0.9] {
                a.handle(Event::PointerMove(at(a, PickerPart::Square(f, f))));
                step.check(a, "square drag");
            }
            a.handle(Event::PointerUp(at(a, PickerPart::Square(0.9, 0.9))));
            step.check(a, "release closes");
            // Hue drag, then Escape.
            a.handle(Event::SecondaryClick(slot));
            step.check(a, "reopen on custom");
            a.handle(Event::PointerDown(at(a, PickerPart::Hue(0.2))));
            a.handle(Event::PointerMove(at(a, PickerPart::Hue(0.7))));
            step.check(a, "hue drag");
            a.handle(Event::Key {
                key: markuli_core::Key::Escape,
                command: false,
                shift: false,
                alt: false,
            });
            step.check(a, "escape closes");
            a.handle(Event::PointerUp(Point { x: 5.0, y: 500.0 }));
            // Presets pick, then the Width Picker, then the Toolbar moves
            // under an open popover.
            a.handle(Event::SecondaryClick(slot));
            let tab0 = at(a, PickerPart::Tab(0));
            a.handle(Event::PointerDown(tab0));
            a.handle(Event::PointerUp(tab0));
            step.check(a, "presets tab");
            a.handle(Event::PointerDown(at(a, PickerPart::Preset(12))));
            a.handle(Event::PointerUp(Point { x: 5.0, y: 500.0 }));
            step.check(a, "pick closes");
            let width = a.button_center(Button::Width(1)).expect("toolbar");
            a.handle(Event::SecondaryClick(width));
            step.check(a, "open width");
            a.handle(Event::PointerDown(at(a, PickerPart::Track(0.3))));
            a.handle(Event::PointerMove(at(a, PickerPart::Track(0.8))));
            step.check(a, "slide");
            a.handle(Event::PointerUp(at(a, PickerPart::Track(0.8))));
            a.handle(Event::SecondaryClick(width));
            for position in markuli_core::ToolbarPosition::ALL {
                a.handle(Event::ToolbarPosition(position));
                step.check(a, "toolbar moves under the popover");
            }
            a.handle(Event::Theme(markuli_core::Theme::Dark));
            step.check(a, "theme changes");
        });
    }
}
