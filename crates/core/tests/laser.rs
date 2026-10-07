//! Seam 1: the Laser Tool through the annotator core interface.

use markuli_core::{Annotator, DisplayId, Event, Format, Key, Point};
use tiny_skia::PixmapMut;

const W: u32 = 400;
const H: u32 = 200;

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

/// Draw Mode on, Laser selected with its key.
fn laser() -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::Resize {
        width: W,
        height: H,
    });
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    key(&mut a, 'k');
    a
}

fn key(a: &mut Annotator, c: char) {
    a.handle(Event::Key {
        key: Key::Char(c),
        command: false,
        shift: false,
        alt: false,
    });
}

/// A 20-point drag along y = 100, one point every 10 ms from `t0`. Returns the
/// time of the last event.
fn sweep(a: &mut Annotator, t0: u64) -> u64 {
    a.handle(Event::Clock(t0));
    a.handle(Event::PointerDown(p(50.0, 100.0)));
    for i in 1..20_u64 {
        a.handle(Event::Clock(t0 + i * 10));
        #[allow(clippy::cast_precision_loss, reason = "small test values")]
        a.handle(Event::PointerMove(p(50.0 + i as f32 * 10.0, 100.0)));
    }
    a.handle(Event::Clock(t0 + 200));
    a.handle(Event::PointerUp(p(250.0, 100.0)));
    t0 + 200
}

fn render(a: &mut Annotator) -> Vec<u8> {
    let mut buf = vec![0_u8; (W * H * 4) as usize];
    let mut target = PixmapMut::from_bytes(&mut buf, W, H).expect("pixmap");
    a.render(&mut target, Format::Rgba);
    buf
}

fn red_pixels(buf: &[u8]) -> usize {
    buf.chunks_exact(4)
        .filter(|px| px[0] > 200 && px[1] < 50 && px[2] < 50)
        .count()
}

#[test]
fn laser_input_never_changes_ink_or_the_undo_history() {
    let mut a = laser();
    key(&mut a, 'p');
    a.handle(Event::PointerDown(p(10.0, 10.0)));
    a.handle(Event::PointerUp(p(60.0, 40.0)));
    let before = a.ink().clone();
    key(&mut a, 'k');
    sweep(&mut a, 1000);
    assert_eq!(a.ink(), &before);
    // Undo reaches the Pen stroke: the Laser left nothing in the log.
    a.handle(Event::Key {
        key: Key::Char('z'),
        command: true,
        shift: false,
        alt: false,
    });
    assert!(a.ink().is_empty());
}

#[test]
fn next_frame_is_set_only_while_the_trail_is_alive() {
    let mut a = laser();
    assert_eq!(a.view().next_frame, None, "nothing animates before a trail");
    a.handle(Event::Clock(1000));
    let view = a.handle(Event::PointerDown(p(50.0, 100.0)));
    assert!(view.next_frame.is_some_and(|t| t > 1000 && t <= 1100));
    let end = sweep(&mut a, 1000);
    assert!(a.view().next_frame.is_some(), "fading after pointer up");
    // Mid-fade the trail is still visible and a frame is due soon.
    let view = a.handle(Event::Clock(end + 500));
    assert!(view.next_frame.is_some_and(|t| t > end + 500));
    // After the decay time (1 s) plus the last stamp the trail is gone.
    let view = a.handle(Event::Clock(end + 1100));
    assert_eq!(view.next_frame, None);
    assert_eq!(a.handle(Event::Clock(end + 5000)).next_frame, None);
}

#[test]
fn the_trail_is_drawn_while_alive_and_leaves_nothing_behind() {
    let mut a = laser();
    let end = sweep(&mut a, 1000);
    a.handle(Event::Clock(end + 16));
    assert!(red_pixels(&render(&mut a)) > 100);
    a.handle(Event::Clock(end + 2000));
    assert!(a.view().needs_render, "the last frame erases the trail");
    assert_eq!(red_pixels(&render(&mut a)), 0);
    assert!(!a.view().needs_render);
}

#[test]
fn leaving_draw_mode_drops_the_trail() {
    let mut a = laser();
    a.handle(Event::Clock(1000));
    a.handle(Event::PointerDown(p(50.0, 100.0)));
    a.handle(Event::Clock(1010));
    a.handle(Event::PointerMove(p(90.0, 100.0)));
    let view = a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    assert_eq!(view.next_frame, None);
    assert!(!view.overlay_needed);
}

#[test]
fn a_held_pointer_that_stops_moving_still_fades_out() {
    let mut a = laser();
    a.handle(Event::Clock(1000));
    a.handle(Event::PointerDown(p(50.0, 100.0)));
    a.handle(Event::Clock(1010));
    a.handle(Event::PointerMove(p(90.0, 100.0)));
    a.handle(Event::Clock(1020));
    a.handle(Event::PointerMove(p(130.0, 100.0)));
    assert!(a.handle(Event::Clock(1500)).next_frame.is_some());
    assert_eq!(a.handle(Event::Clock(3000)).next_frame, None);
}
