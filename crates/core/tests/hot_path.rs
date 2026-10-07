//! Standards rule 8: no per-move allocations of our own on the hot path.
//!
//! Lives in its own test binary because it installs a counting global
//! allocator; it holds a single test so nothing else allocates concurrently.
//!
//! Known, accepted remainder: tiny-skia's rasterizer makes a few small
//! transient allocations per `fill_path` call (edge list and coverage runs,
//! a few KiB, freed immediately). They are bounded by the damaged region's
//! width, not by Ink or the screen, so the test caps bytes rather than
//! demanding zero calls.

use markuli_core::{Annotator, DisplayId, Event, Format, Key, Point};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use tiny_skia::Pixmap;

struct Counting;

static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

fn count(size: usize) {
    CALLS.fetch_add(1, Ordering::Relaxed);
    BYTES.fetch_add(size, Ordering::Relaxed);
}

// SAFETY: forwards every call unchanged to the system allocator.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn at(i: u16) -> Point {
    let t = f32::from(i);
    Point {
        x: 100.0 + t * 1.5,
        y: 300.0 + 60.0 * (t / 9.0).sin(),
    }
}

#[test]
fn pointer_moves_do_not_allocate_and_render_allocates_nothing_that_scales() {
    let mut a = Annotator::new();
    let mut pm = Pixmap::new(1200, 800).expect("pixmap");
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    a.render(&mut pm.as_mut(), Format::Rgba);
    a.handle(Event::PointerDown(at(0)));
    // Warm up: outline and scratch buffers reach their working size.
    for i in 1..150 {
        a.handle(Event::PointerMove(at(i)));
        a.render(&mut pm.as_mut(), Format::Rgba);
    }
    let moves = 100_u16;
    let (mut handle_calls, mut render_bytes) = (0, 0);
    for i in 150..150 + moves {
        let c0 = CALLS.load(Ordering::Relaxed);
        a.handle(Event::PointerMove(at(i)));
        let (c1, b1) = (CALLS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed));
        a.render(&mut pm.as_mut(), Format::Rgba);
        handle_calls += c1 - c0;
        render_bytes += BYTES.load(Ordering::Relaxed) - b1;
    }
    // Only the amortized growth of the point list (past 256 points) is allowed.
    assert!(
        handle_calls <= 4,
        "{handle_calls} allocations handling {moves} moves"
    );
    let per_render = render_bytes / usize::from(moves);
    assert!(
        per_render < 16 * 1024,
        "{per_render} bytes allocated per render"
    );
    a.handle(Event::PointerUp(at(250)));

    // The Select Tool: first a box drag that encloses the Stroke, then a move
    // of the selected Stroke. Pointer moves allocate nothing of our own.
    a.handle(Event::Key {
        key: Key::Char('v'),
        command: false,
        shift: false,
        alt: false,
    });
    let corner = |i: u16| Point {
        x: 600.0 + f32::from(i),
        y: 500.0 + f32::from(i),
    };
    a.handle(Event::PointerDown(Point { x: 20.0, y: 150.0 }));
    for i in 0..40 {
        a.handle(Event::PointerMove(corner(i)));
        a.render(&mut pm.as_mut(), Format::Rgba);
    }
    assert_eq!(a.selection().len(), 1, "the box encloses the Stroke");
    assert_eq!(
        a.selection().first().copied(),
        a.ink().elements().first().map(markuli_core::Element::id)
    );
    assert_alloc_free("box drag", &mut a, &mut pm, |i| corner(40 + i));
    a.handle(Event::PointerUp(corner(140)));

    // Press on the Stroke (it is selected, so the press grabs it) and drag.
    let on_stroke = at(10);
    let drag = |i: u16| Point {
        x: on_stroke.x + f32::from(i),
        y: on_stroke.y + f32::from(i) / 2.0,
    };
    a.handle(Event::PointerDown(on_stroke));
    for i in 0..40 {
        a.handle(Event::PointerMove(drag(i)));
        a.render(&mut pm.as_mut(), Format::Rgba);
    }
    assert_alloc_free("select move", &mut a, &mut pm, |i| drag(40 + i));
}

/// 100 pointer moves of the active Tool: `handle` must not allocate and
/// `render` must stay bounded.
fn assert_alloc_free(what: &str, a: &mut Annotator, pm: &mut Pixmap, point: impl Fn(u16) -> Point) {
    let moves = 100_u16;
    let (mut handle_calls, mut render_bytes) = (0, 0);
    for i in 0..moves {
        let c0 = CALLS.load(Ordering::Relaxed);
        a.handle(Event::PointerMove(point(i)));
        let (c1, b1) = (CALLS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed));
        a.render(&mut pm.as_mut(), Format::Rgba);
        handle_calls += c1 - c0;
        render_bytes += BYTES.load(Ordering::Relaxed) - b1;
    }
    assert_eq!(
        handle_calls, 0,
        "{what}: allocations handling {moves} moves"
    );
    let per_render = render_bytes / usize::from(moves);
    // The Selection overlay is never incremental (it redraws the whole
    // overlay area, see selection.rs), so tiny-skia's transient edge buffers
    // are larger than for a Pen segment (measured ~110 KiB); still bounded by
    // the area's width, not by Ink.
    assert!(
        per_render < 256 * 1024,
        "{what}: {per_render} bytes allocated per render"
    );
}
