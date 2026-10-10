//! Seam 1: undo/redo over the operation log, Clear, Esc and Overlay lifecycle.

use markuli_core::{Annotator, DisplayId, Event, Key, Point};

const D1: DisplayId = DisplayId::new(1);
const D2: DisplayId = DisplayId::new(2);

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn stroke(a: &mut Annotator, at: f32) {
    a.handle(Event::PointerDown(p(at, at)));
    a.handle(Event::PointerMove(p(at + 5.0, at + 9.0)));
    a.handle(Event::PointerUp(p(at + 10.0, at + 10.0)));
}

fn key(a: &mut Annotator, key: Key, command: bool, shift: bool) {
    a.handle(Event::Key {
        key,
        command,
        shift,
        alt: false,
    });
}

fn undo(a: &mut Annotator) {
    key(a, Key::Char('z'), true, false);
}

fn redo(a: &mut Annotator) {
    key(a, Key::Char('z'), true, true);
}

fn drawing() -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::ToggleDrawMode(D1));
    a
}

#[test]
fn undo_removes_the_last_stroke_and_redo_brings_it_back() {
    let mut a = drawing();
    stroke(&mut a, 10.0);
    stroke(&mut a, 50.0);
    let both = a.ink().clone();
    undo(&mut a);
    assert_eq!(a.ink().len(), 1);
    let first = &a.ink().elements()[0];
    assert_eq!((first.x(), first.y()), (10.0, 10.0));
    undo(&mut a);
    assert!(a.ink().is_empty());
    redo(&mut a);
    redo(&mut a);
    assert_eq!(*a.ink(), both);
}

#[test]
fn undo_and_redo_with_nothing_to_do_are_ignored() {
    let mut a = drawing();
    undo(&mut a);
    redo(&mut a);
    assert!(a.ink().is_empty());
    stroke(&mut a, 10.0);
    redo(&mut a);
    assert_eq!(a.ink().len(), 1);
}

#[test]
fn ctrl_y_also_redoes() {
    let mut a = drawing();
    stroke(&mut a, 10.0);
    undo(&mut a);
    key(&mut a, Key::Char('y'), true, false);
    assert_eq!(a.ink().len(), 1);
}

#[test]
fn a_new_stroke_discards_the_redo_branch() {
    let mut a = drawing();
    stroke(&mut a, 10.0);
    undo(&mut a);
    stroke(&mut a, 70.0);
    redo(&mut a);
    assert_eq!(a.ink().len(), 1);
    let first = &a.ink().elements()[0];
    assert_eq!((first.x(), first.y()), (70.0, 70.0));
}

#[test]
fn undo_needs_the_command_key_and_draw_mode() {
    let mut a = drawing();
    stroke(&mut a, 10.0);
    key(&mut a, Key::Char('z'), false, false);
    assert_eq!(a.ink().len(), 1);
    a.handle(Event::ToggleDrawMode(D1));
    undo(&mut a);
    assert_eq!(a.ink().len(), 1);
}

#[test]
fn undo_asks_for_a_redraw() {
    let mut a = drawing();
    stroke(&mut a, 10.0);
    let mut buffer = vec![0_u8; 64 * 64 * 4];
    let mut pixmap = tiny_skia::PixmapMut::from_bytes(&mut buffer, 64, 64).unwrap();
    a.render(&mut pixmap, markuli_core::Format::Rgba);
    assert!(!a.view().needs_render);
    undo(&mut a);
    assert!(a.view().needs_render);
}

#[test]
fn clear_removes_ink_and_stays_in_draw_mode() {
    let mut a = drawing();
    stroke(&mut a, 10.0);
    let v = a.handle(Event::Clear);
    assert!(a.ink().is_empty());
    assert!(v.draw_mode);
    assert!(v.needs_render);
}

#[test]
fn clear_outside_draw_mode_clears_the_hidden_ink_and_does_not_enter_it() {
    let mut a = drawing();
    stroke(&mut a, 10.0);
    a.handle(Event::ToggleDrawMode(D1));
    let v = a.handle(Event::Clear);
    assert!(a.ink().is_empty());
    assert!(!v.draw_mode);
    // Entering again shows nothing, and the Clear can be undone.
    a.handle(Event::ToggleDrawMode(D1));
    assert!(a.ink().is_empty());
    undo(&mut a);
    assert_eq!(a.ink().len(), 1);
}

#[test]
fn clear_is_undoable_and_redoable() {
    let mut a = drawing();
    stroke(&mut a, 10.0);
    stroke(&mut a, 50.0);
    let before = a.ink().clone();
    a.handle(Event::Clear);
    undo(&mut a);
    assert_eq!(*a.ink(), before);
    redo(&mut a);
    assert!(a.ink().is_empty());
    undo(&mut a);
    undo(&mut a);
    undo(&mut a);
    assert!(a.ink().is_empty());
}

#[test]
fn clearing_empty_ink_logs_nothing() {
    let mut a = drawing();
    stroke(&mut a, 10.0);
    a.handle(Event::Clear);
    a.handle(Event::Clear);
    undo(&mut a);
    assert_eq!(a.ink().len(), 1);
}

#[test]
fn the_q_key_clears_like_the_clear_button() {
    let mut a = drawing();
    stroke(&mut a, 10.0);
    stroke(&mut a, 50.0);
    key(&mut a, Key::Char('q'), false, true);
    key(&mut a, Key::Char('q'), true, false);
    a.handle(Event::Key {
        key: Key::Char('q'),
        command: false,
        shift: false,
        alt: true,
    });
    assert_eq!(a.ink().len(), 2);

    a.handle(Event::PointerDown(p(80.0, 80.0)));
    a.handle(Event::PointerMove(p(90.0, 90.0)));
    key(&mut a, Key::Char('q'), false, false);
    a.handle(Event::PointerUp(p(95.0, 95.0)));
    assert_eq!(a.ink().len(), 3);

    let before = a.ink().clone();
    let view = a.handle(Event::Key {
        key: Key::Char('q'),
        command: false,
        shift: false,
        alt: false,
    });
    assert!(a.ink().is_empty());
    assert!(view.draw_mode);
    undo(&mut a);
    assert_eq!(*a.ink(), before);
}

#[test]
fn esc_mid_stroke_discards_only_that_stroke() {
    let mut a = drawing();
    stroke(&mut a, 10.0);
    a.handle(Event::PointerDown(p(80.0, 80.0)));
    a.handle(Event::PointerMove(p(90.0, 90.0)));
    key(&mut a, Key::Escape, false, false);
    a.handle(Event::PointerMove(p(95.0, 95.0)));
    a.handle(Event::PointerUp(p(95.0, 95.0)));
    assert_eq!(a.ink().len(), 1);
    let first = &a.ink().elements()[0];
    assert_eq!((first.x(), first.y()), (10.0, 10.0));
    assert!(a.view().draw_mode);
    // The cancelled stroke left nothing to undo.
    undo(&mut a);
    assert!(a.ink().is_empty());
}

#[test]
fn esc_never_clears() {
    let mut a = drawing();
    stroke(&mut a, 10.0);
    key(&mut a, Key::Escape, false, false);
    assert_eq!(a.ink().len(), 1);
    assert!(a.view().draw_mode);
}

#[test]
fn moving_to_another_display_starts_a_fresh_history() {
    let mut a = drawing();
    stroke(&mut a, 10.0);
    a.handle(Event::ToggleDrawMode(D1));
    let v = a.handle(Event::ToggleDrawMode(D2));
    assert_eq!(v.display, Some(D2));
    assert!(a.ink().is_empty());
    undo(&mut a);
    assert!(a.ink().is_empty());
}

/// xorshift: a deterministic stand-in for a random-testing crate.
struct Rng(u64);

impl Rng {
    fn next(&mut self, below: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % below
    }
}

#[test]
fn random_event_sequences_never_panic_and_undo_all_returns_to_empty_ink() {
    for seed in 1..=300_u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let mut a = Annotator::new();
        for _ in 0..200 {
            #[allow(clippy::cast_precision_loss)]
            let at = p(rng.next(500) as f32, rng.next(500) as f32);
            let flag = |r: &mut Rng| r.next(2) == 0;
            let event = match rng.next(10) {
                0 => Event::ToggleDrawMode(DisplayId::new(rng.next(3))),
                1 => Event::SurfaceReset,
                2 | 3 => Event::PointerDown(at),
                4 => Event::PointerMove(at),
                5 => Event::PointerUp(at),
                6 => Event::Clear,
                7 => Event::Key {
                    key: Key::Escape,
                    command: flag(&mut rng),
                    shift: flag(&mut rng),
                    alt: false,
                },
                _ => Event::Key {
                    #[allow(clippy::cast_possible_truncation)]
                    key: Key::Char(['z', 'y', 'x'][rng.next(3) as usize]),
                    command: flag(&mut rng),
                    shift: flag(&mut rng),
                    alt: false,
                },
            };
            a.handle(event);
        }
        // Back into Draw Mode on the Overlay's display, then undo everything.
        if !a.view().draw_mode {
            let display = a.view().display.unwrap_or(D1);
            a.handle(Event::ToggleDrawMode(display));
        }
        a.handle(Event::PointerUp(p(0.0, 0.0)));
        for _ in 0..1000 {
            undo(&mut a);
        }
        assert!(a.ink().is_empty(), "seed {seed}");
    }
}
