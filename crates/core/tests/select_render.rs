//! Seam 1, pixels out: the Selection overlay is repainted incrementally and
//! must always equal a from-scratch render, with damage covering every change.

#![allow(clippy::cast_precision_loss, reason = "tiny test values")]

use markuli_core::{Annotator, Damage, DisplayId, Event, Format, Key, Point, Theme};
use tiny_skia::Pixmap;

const W: u32 = 300;
const H: u32 = 200;

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn render(a: &mut Annotator, pm: &mut Pixmap) -> Option<Damage> {
    let mut m = pm.as_mut();
    a.render(&mut m, Format::Rgba)
}

fn fresh(a: &mut Annotator) -> Pixmap {
    let mut pm = Pixmap::new(W, H).expect("pixmap");
    a.handle(Event::SurfaceReset);
    render(a, &mut pm);
    pm
}

fn far(a: &[u8], b: &[u8]) -> bool {
    a.iter().zip(b).any(|(p, q)| p.abs_diff(*q) > 64)
}

struct Scene {
    a: Annotator,
    shown: Pixmap,
    before: Pixmap,
}

impl Scene {
    fn new(scale: f32) -> Self {
        let mut a = Annotator::new();
        a.handle(Event::Theme(Theme::Light));
        a.handle(Event::ScaleFactor(scale));
        a.handle(Event::Resize {
            width: W,
            height: H,
        });
        a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
        let mut shown = Pixmap::new(W, H).expect("pixmap");
        render(&mut a, &mut shown);
        let before = fresh(&mut a);
        Self { a, shown, before }
    }

    /// Feeds `event`, renders incrementally and compares with a fresh render.
    fn step(&mut self, event: Event, what: &str) {
        self.a.handle(event);
        let damage = render(&mut self.a, &mut self.shown);
        let now = fresh(&mut self.a);
        for y in 0..H {
            for x in 0..W {
                let i = ((y * W + x) * 4) as usize;
                let (old, new, got) = (
                    &self.before.data()[i..i + 4],
                    &now.data()[i..i + 4],
                    &self.shown.data()[i..i + 4],
                );
                let inside = damage.is_some_and(|d| {
                    x >= d.x && x < d.x + d.width && y >= d.y && y < d.y + d.height
                });
                assert!(
                    inside || !far(old, new),
                    "{what}: pixel {x},{y} changed outside {damage:?}"
                );
                assert!(!far(got, new), "{what}: pixel {x},{y} differs from fresh");
            }
        }
        self.before = now;
    }

    fn key(&mut self, key: Key, command: bool, shift: bool, what: &str) {
        self.step(
            Event::Key {
                key,
                command,
                shift,
            },
            what,
        );
    }

    fn drag(&mut self, from: (f32, f32), to: (f32, f32), what: &str) {
        self.step(Event::PointerDown(p(from.0, from.1)), what);
        for i in 1..=4_u8 {
            let t = f32::from(i) / 4.0;
            let at = p(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            self.step(Event::PointerMove(at), what);
        }
        self.step(Event::PointerUp(p(to.0, to.1)), what);
    }

    fn stroke(&mut self, a: (f32, f32), b: (f32, f32), c: (f32, f32)) {
        self.key(Key::Char('p'), false, false, "pen");
        self.drag(a, b, "stroke leg 1");
        self.step(Event::PointerDown(p(c.0, c.1)), "stroke down");
        self.step(Event::PointerUp(p(c.0, c.1)), "stroke up");
    }
}

fn run(scale: f32) {
    let mut s = Scene::new(scale);
    s.stroke((30.0, 90.0), (110.0, 140.0), (60.0, 150.0));
    s.stroke((150.0, 100.0), (230.0, 150.0), (200.0, 90.0));
    s.key(Key::Char('v'), false, false, "select tool");
    s.step(Event::PointerDown(p(70.0, 115.0)), "click down");
    s.step(Event::PointerUp(p(70.0, 115.0)), "click up");
    s.step(Event::Modifiers { shift: true }, "shift");
    s.step(Event::PointerDown(p(190.0, 125.0)), "shift-click down");
    s.step(Event::PointerUp(p(190.0, 125.0)), "shift-click up");
    s.step(Event::Modifiers { shift: false }, "shift off");
    s.drag((70.0, 115.0), (90.0, 70.0), "move both");
    s.key(Key::Char('z'), true, false, "undo move");
    s.drag((5.0, 60.0), (270.0, 190.0), "box select");
    s.key(Key::Delete, false, false, "delete");
    s.key(Key::Char('z'), true, false, "undo delete");
    s.key(Key::Char('a'), true, false, "select all");
    s.key(Key::Escape, false, false, "deselect");
}

#[test]
fn selection_overlay_updates_incrementally_at_1x() {
    run(1.0);
}

#[test]
fn selection_overlay_updates_incrementally_at_2x() {
    run(2.0);
}
